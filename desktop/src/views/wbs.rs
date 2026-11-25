//! WBS view: the project's work breakdown structure as a tree table with
//! rolled-up activity dates and float, the command bar (Add, Cut, Copy,
//! Paste, Del. / Merge, Shift up/down/left/right) and the WBS Details pane.
//!
//! `ProjectData::wbs` stays the source of truth for structure (id, parent,
//! short code, name, sequence). Every edit here also rewrites the matching
//! PROJWBS rows so the change survives save, reopen and XER export. Sequence
//! numbers are renumbered in tree pre-order after each structural edit, so
//! both sibling order and the flat order used by the Activities grouping
//! follow the tree.
use super::grid::{self, Columns, Row};
use super::projects::{check, icon_item, STATUSES};
use super::*;
use super::{chart, sheet};
use crate::context_menu::{Entry, Spec};
use crate::schedule_dialogs::Canvas;
use openxer_xer::edit::notebooks::Owner;
pub use openxer_xer::edit::wbs::{
    activity_counts, add, code_of, code_taken, commit, copy_subtree, delete_merge,
    delete_with_activities, full_codes, full_codes_in, guard, is_project_node, num, project_node,
    related, rename_code, separator, shift_down, shift_left, shift_right, shift_up,
    subtree_activities, sync_projwbs, CopyParts, Tree,
};
use openxer_xer::Wbs;
use std::collections::BTreeSet;

use super::wbs_columns::{self as wcols, Own, Source};
#[cfg(test)]
use openxer_xer::edit::wbs::MAX_FULL_CODE;
pub const TABS: [&str; 8] = [
    "General",
    "Notebook",
    "Budget Log",
    "Spending Plan",
    "Budget Summary",
    "WBS Milestones",
    "Documents",
    "Earned Value",
];
/// The WBS details open on Notebook.
pub const DEFAULT_TAB: usize = 1;
/// Table width to the splitter, as measured.
const TABLE_WIDTH: f32 = 559.;
/// Which Shift commands apply to a WBS element, from where it sits:
/// up and down need a sibling on that side, right needs a sibling above to
/// become a child of, left needs a parent that is not the project node.
fn shift_allowed(p: &ProjectData, id: i64) -> [bool; 4] {
    if guard(p, id).is_err() {
        return [false; 4];
    }
    let tree = Tree::build(p);
    let parent = tree.parent.get(&id).copied().flatten();
    let siblings = tree.kids(parent);
    let at = siblings.iter().position(|&x| x == id).unwrap_or(0);
    [
        at > 0,
        at + 1 < siblings.len(),
        parent.is_some_and(|parent| !is_project_node(p, parent)),
        at > 0,
    ]
}

/// What the WBS menu needs of the view, read when the menu opens.
#[derive(Clone, Copy)]
struct MenuState<'a> {
    clipboard: bool,
    details: bool,
    collapsed: &'a BTreeSet<i64>,
    root_collapsed: bool,
    group: WbsGroup,
}

/// How the WBS table lists its elements (Group and Sort By).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WbsGroup {
    /// The WBS tree (the reference's Default).
    #[default]
    Tree,
    /// Under bands of their responsible manager.
    Manager,
    /// One list, without the tree.
    List,
}

impl WbsGroup {
    /// Group and Sort By's choices in menu order: (group, menu key, caption).
    pub const MENU: [(WbsGroup, &'static str, &'static str); 3] = [
        (WbsGroup::Tree, "group_default", "Default"),
        (WbsGroup::Manager, "group_manager", "Responsible Manager"),
        (WbsGroup::List, "group_list", "List"),
    ];

    /// The name a layout stores.
    pub fn key(self) -> &'static str {
        match self {
            WbsGroup::Tree => "default",
            WbsGroup::Manager => "manager",
            WbsGroup::List => "list",
        }
    }

    pub fn from_key(key: &str) -> Option<WbsGroup> {
        Self::MENU.iter().find(|m| m.0.key() == key).map(|m| m.0)
    }
}

/// A row of the WBS table as listed: its key, the row and its bar.
type Listed = (i64, Row, Option<chart::Bar>);

/// The band id of responsible manager band `k` in the WBS table (bands
/// take negative ids, so they never meet a WBS element id).
fn manager_band(k: usize) -> i64 {
    -(k as i64) - 1
}

/// One WBS menu item with its shortcut text and icon.
fn wbs_item(
    key: &'static str,
    label: &'static str,
    shortcut: &'static str,
    icon: chrome::Icon,
    enabled: bool,
) -> Entry {
    Entry::Item(
        Spec::new(key, label)
            .key(shortcut)
            .icon(icon)
            .enabled(enabled),
    )
}

/// The WBS Details check item that heads both WBS menus.
fn wbs_details_item(shown: bool) -> Entry {
    Entry::Item(
        Spec::new("details", "WBS Details")
            .icon(chrome::Icon::Details)
            .checked(shown),
    )
}

/// The WBS table menu for the element `id` (`None`: the project row or the
/// empty area), in the reference's order: details, clipboard, Add and
/// Delete, then the layout items. Fill Down is listed greyed, as the
/// reference greys it in this window. The Shift commands are OpenXER's
/// own (also on the Move toolbar), in a group after Delete.
fn wbs_menu(p: &ProjectData, state: MenuState, id: Option<i64>) -> Vec<Entry> {
    use chrome::Icon;
    let element = id.is_some_and(|id| guard(p, id).is_ok());
    let [up, down, left, right] = id.map_or([false; 4], |id| shift_allowed(p, id));
    let tree = Tree::build(p);
    let any_collapsed = state.root_collapsed || !state.collapsed.is_empty();
    let any_open = !state.root_collapsed
        && p.wbs
            .iter()
            .any(|w| !tree.kids(Some(w.id)).is_empty() && !state.collapsed.contains(&w.id));
    vec![
        wbs_details_item(state.details),
        Entry::Separator,
        wbs_item("cut", "Cut", "Ctrl+X", Icon::Cut, element),
        wbs_item("copy", "Copy", "Ctrl+C", Icon::Copy, element),
        wbs_item("paste", "Paste", "Ctrl+V", Icon::Paste, state.clipboard),
        wbs_item("fill_down", "Fill Down", "Ctrl+E", Icon::FillDown, false),
        Entry::Separator,
        wbs_item("add", "Add", "Ins", Icon::Add, true),
        wbs_item("delete", "Delete", "Del", Icon::Delete, element),
        Entry::Separator,
        wbs_item("shift_up", "Shift up", "", Icon::ShiftUp, up),
        wbs_item("shift_down", "Shift down", "", Icon::ShiftDown, down),
        wbs_item("shift_left", "Shift left", "", Icon::ShiftLeft, left),
        wbs_item("shift_right", "Shift right", "", Icon::ShiftRight, right),
        Entry::Separator,
        Entry::Submenu(
            Spec::new("columns", "Columns").icon(Icon::Columns),
            vec![Entry::Item(
                Spec::new("columns_customize", "Customize...").icon(Icon::Columns),
            )],
        ),
        icon_item("table_font", "Table Font and Row...", Icon::TableFont),
        Entry::Separator,
        Entry::Submenu(
            Spec::new("group_by", "Group and Sort By").icon(Icon::GroupSort),
            WbsGroup::MENU
                .iter()
                .map(|&(g, key, caption)| check(key, caption, state.group == g))
                .collect(),
        ),
        Entry::Separator,
        wbs_item(
            "expand_all",
            "Expand All",
            "Ctrl+Num +",
            Icon::ExpandAll,
            any_collapsed,
        ),
        wbs_item(
            "collapse_all",
            "Collapse All",
            "Ctrl+Num -",
            Icon::CollapseAll,
            any_open,
        ),
        icon_item("collapse_to", "Collapse To...", Icon::CollapseTo),
    ]
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub root: i64,
    pub cut: bool,
}

/// What the Del. / Merge confirmation will do.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingDelete {
    pub id: i64,
    pub activities: usize,
    pub children: usize,
    /// Delete All was chosen: the confirmation lists what goes.
    pub delete_all: bool,
}

pub struct WbsState {
    pub columns: Columns,
    /// `None` selects the project row at the top of the tree.
    /// The current row (`None`: the project row).
    pub selected: Option<i64>,
    /// The row selection, one row at a time: the commands act on the
    /// current row only.
    pub rows: super::grid::RowSelection<Option<i64>>,
    pub collapsed: BTreeSet<i64>,
    pub root_collapsed: bool,
    pub tab: usize,
    pub show_details: bool,
    pub details_height: f32,
    pub clipboard: Option<Clip>,
    /// A paste of copied elements waiting for its options (`wbs_copy`).
    pub paste: Option<super::wbs_copy::WbsPaste>,
    /// The Copy WBS Options choices last made.
    pub copy_options: super::wbs_copy::WbsCopyOptions,
    pub pending: Option<PendingDelete>,
    pub code_edit: Option<(i64, String)>,
    pub table_width: f32,
    /// Selected notebook topic row on the Notebook tab.
    pub topic: usize,
    /// Selected row on the WPs & Docs tab.
    pub document: Option<usize>,
    pub(crate) scroll_to_selected: bool,
    /// The chart's first minute once scrolled (None: the week before the
    /// first bar), its zoom in pixels a week, where it was drawn and its
    /// default first minute.
    pub timeline_first: Option<f64>,
    pub zoom: f64,
    pub timeline_place: crate::timeline::Place,
    pub timeline_default: f64,
    /// Group and Sort By.
    pub group: WbsGroup,
    /// Table Font and Row: the table's font, cell colour and row height.
    pub table: crate::grid::layout::TableDisplay,
    pub row_height: f32,
}

impl Default for WbsState {
    fn default() -> Self {
        Self {
            columns: default_columns(),
            selected: None,
            rows: super::grid::RowSelection::single(),
            collapsed: BTreeSet::new(),
            root_collapsed: false,
            tab: DEFAULT_TAB,
            show_details: true,
            details_height: 560.,
            clipboard: None,
            paste: None,
            copy_options: Default::default(),
            pending: None,
            code_edit: None,
            table_width: TABLE_WIDTH,
            topic: 0,
            document: None,
            scroll_to_selected: false,
            timeline_first: None,
            zoom: chart::WEEK_PITCH,
            timeline_place: Default::default(),
            timeline_default: 0.,
            group: WbsGroup::Tree,
            table: Default::default(),
            row_height: grid::ROW_HEIGHT,
        }
    }
}

impl WbsState {
    pub fn reset(&mut self) {
        self.selected = None;
        self.rows = super::grid::RowSelection::single();
        self.collapsed.clear();
        self.root_collapsed = false;
        self.clipboard = None;
        self.paste = None;
        self.pending = None;
        self.code_edit = None;
        self.document = None;
        self.timeline_first = None;
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"columns": self.columns.json(), "tab": self.tab,
            "show_details": self.show_details, "details_height": self.details_height,
            "table_width": self.table_width, "zoom": self.zoom, "group": self.group.key(),
            "table": crate::grid::layout::table_to_json(&self.table),
            "row_height": self.row_height})
    }
    pub fn restore(&mut self, value: &serde_json::Value) {
        self.columns.restore(&value["columns"]);
        if let Some(tab) = value["tab"].as_u64().filter(|t| (*t as usize) < TABS.len()) {
            self.tab = tab as usize;
        }
        if let Some(show) = value["show_details"].as_bool() {
            self.show_details = show;
        }
        restore_height(&value["details_height"], &mut self.details_height);
        restore_width(&value["table_width"], &mut self.table_width);
        chart::restore_zoom(&value["zoom"], &mut self.zoom);
        if let Some(g) = value["group"].as_str().and_then(WbsGroup::from_key) {
            self.group = g;
        }
        if let Ok(table) = crate::grid::layout::table_from_json(&value["table"]) {
            self.table = table;
        }
        if let Some(h) = value["row_height"]
            .as_f64()
            .filter(|h| (17.0..=48.).contains(h))
        {
            self.row_height = h as f32;
        }
    }
}

/// Check that a WBS element can be merged into its parent (the shared
/// edit layer's `plan_delete`), as the confirmation's starting point.
pub fn plan_delete(p: &ProjectData, id: i64) -> Result<PendingDelete, String> {
    let plan = openxer_xer::edit::wbs::plan_delete(p, id)?;
    Ok(PendingDelete {
        id: plan.id,
        activities: plan.activities,
        children: plan.children,
        delete_all: false,
    })
}

/// Paste the clipboard below `target` (as its last child; the project row
/// pastes at the top level). Cut moves the subtree with its activities;
/// copy duplicates the WBS elements alone (the Paste command adds what Copy
/// WBS Options chooses, `wbs_copy`).
pub fn paste(p: &mut ProjectData, clip: &Clip, target: Option<i64>) -> Result<i64, String> {
    if !clip.cut {
        let parent = target.or(project_node(p));
        return copy_subtree(p, clip.root, parent, CopyParts::NONE).map(|map| map[&clip.root]);
    }
    let mut tree = Tree::build(p);
    if !tree.parent.contains_key(&clip.root) {
        return Err("The copied WBS element no longer exists.".into());
    }
    if is_project_node(p, clip.root) {
        return Err("The project node cannot be cut or copied.".into());
    }
    let parent = target.or(project_node(p));
    let subtree = tree.descendants(clip.root);
    if parent.is_some_and(|t| subtree.contains(&t)) {
        return Err("A WBS element cannot be pasted inside itself.".into());
    }
    if code_taken(p, &tree, parent, code_of(p, clip.root), clip.root) {
        return Err(format!(
            "WBS code {} is already used at the destination level.",
            code_of(p, clip.root)
        ));
    }
    tree.detach(clip.root);
    let end = tree.kids(parent).len();
    tree.attach(clip.root, parent, end);
    commit(p, &tree);
    Ok(clip.root)
}

/// Full code as shown in the WBS Code column: project ID, then the short
/// codes from the top level down, joined by the project's separator.
#[cfg(test)]
pub fn full_code(p: &ProjectData, tree: &Tree, id: i64) -> String {
    full_codes(p, tree).remove(&id).unwrap_or_default()
}

/// Rolled-up values for one tree node: activity count, earliest start, latest
/// finish and smallest total float of open activities, over the subtree.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rollup {
    pub activities: usize,
    pub start: Option<i64>,
    pub finish: Option<i64>,
    pub total_float: Option<i64>,
    /// Activities with an actual start, and completed activities.
    pub started: usize,
    pub completed: usize,
}

impl Rollup {
    /// The rolled-up start is an actual date once any activity has started.
    pub fn actual_start(&self) -> bool {
        self.started > 0
    }
    /// The rolled-up finish is an actual date once every activity is complete.
    pub fn actual_finish(&self) -> bool {
        self.activities > 0 && self.completed == self.activities
    }
    fn absorb(&mut self, other: &Rollup) {
        self.activities += other.activities;
        self.started += other.started;
        self.completed += other.completed;
        self.start = min_opt(self.start, other.start);
        self.finish = self.finish.max(other.finish);
        self.total_float = min_opt(self.total_float, other.total_float);
    }
}

fn min_opt(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, None) => a,
        (None, b) => b,
    }
}

/// Rollups for every WBS id plus the project total under key `None`.
pub fn rollups(p: &ProjectData, tree: &Tree) -> BTreeMap<Option<i64>, Rollup> {
    let leveled = p.model.options.do_level;
    let mut own: BTreeMap<i64, Rollup> = BTreeMap::new();
    for a in &p.model.activities {
        let Some(d) = p.details.get(&a.task_id) else {
            continue;
        };
        let r = own.entry(d.wbs).or_default();
        r.activities += 1;
        r.started += usize::from(a.act_start.is_some());
        r.completed += usize::from(a.is_completed());
        r.start = min_opt(r.start, displayed_start(a, leveled));
        r.finish = r.finish.max(displayed_finish(a, leveled));
        if !a.is_completed() {
            r.total_float = min_opt(r.total_float, a.total_float);
        }
    }
    let mut out: BTreeMap<Option<i64>, Rollup> = BTreeMap::new();
    let order = tree.preorder();
    for (id, _) in order.iter().rev() {
        let mut r = own.get(id).cloned().unwrap_or_default();
        for child in tree.kids(Some(*id)) {
            if let Some(c) = out.get(&Some(*child)) {
                r.absorb(&c.clone());
            }
        }
        out.insert(Some(*id), r);
    }
    let mut total = Rollup::default();
    for r in own.values() {
        total.absorb(r);
    }
    out.insert(None, total);
    out
}

/// OBS names keyed by obs_id, in the table's order.
pub fn obs_names(p: &ProjectData) -> Vec<(String, String)> {
    p.document.table("OBS").map_or(vec![], |t| {
        t.rows
            .iter()
            .map(|r| {
                (
                    t.value(r, "obs_id").to_string(),
                    t.value(r, "obs_name").to_string(),
                )
            })
            .collect()
    })
}

pub fn projwbs_value(p: &ProjectData, id: i64, field: &str) -> String {
    p.document.table("PROJWBS").map_or(String::new(), |t| {
        t.rows
            .iter()
            .find(|r| num(t.value(r, "wbs_id")) == id)
            .map_or(String::new(), |r| t.value(r, field).to_string())
    })
}

/// Plain text from an XER memo (HTML with 0x7f line breaks).
pub fn memo_text(html: &str) -> String {
    let normalized = html
        .replace("\u{7f}\u{7f}", "\n")
        .replace('\u{7f}', "\n")
        .replace("<br>", "\n")
        .replace("<BR>", "\n")
        .replace("</p>", "\n")
        .replace("</P>", "\n");
    let mut out = String::new();
    let mut in_tag = false;
    for c in normalized.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .trim()
        .to_string()
}

/// The editor of field `field` of WBS element `element` in the WBS table,
/// for the fields the reference edits in place: code, name, status,
/// responsible manager (a pick from the OBS) and the anticipated dates.
/// The project node's code is the project's and is not typed here.
pub fn cell_editor(
    p: &ProjectData,
    element: i64,
    field: usize,
    obs: &[(String, String)],
) -> Option<grid::EditKind> {
    let Source::Own(own) = wcols::catalog().get(field)?.source else {
        return None;
    };
    match own {
        Own::Code if is_project_node(p, element) => None,
        Own::Code | Own::Name | Own::AnticipatedStart | Own::AnticipatedFinish => {
            Some(grid::EditKind::Text)
        }
        Own::Status => Some(grid::EditKind::Choice(
            STATUSES.iter().map(|(_, v)| v.to_string()).collect(),
        )),
        Own::Manager if obs.is_empty() => None,
        Own::Manager => Some(grid::EditKind::Choice(
            obs.iter().map(|(_, name)| name.clone()).collect(),
        )),
        _ => None,
    }
}

/// The WBS table's columns out of the box.
fn default_columns() -> Columns {
    let mut c = Columns::new(&wcols::widths());
    c.order = crate::defaults::positions(&wcols::titles(), crate::defaults::WBS_COLUMNS);
    c
}

/// The activities (rows of `p.model.activities`) of every WBS element
/// and of the elements below it.
fn members(p: &ProjectData, tree: &Tree) -> BTreeMap<i64, Vec<usize>> {
    let mut own: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    for (i, a) in p.model.activities.iter().enumerate() {
        if let Some(d) = p.details.get(&a.task_id) {
            own.entry(d.wbs).or_default().push(i);
        }
    }
    let mut out: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    for (id, _) in tree.preorder().iter().rev() {
        let mut list = own.remove(id).unwrap_or_default();
        for child in tree.kids(Some(*id)) {
            if let Some(below) = out.get(child) {
                list.extend_from_slice(below);
            }
        }
        list.sort_unstable();
        out.insert(*id, list);
    }
    out
}

/// The value of a field of WBS element `id` itself.
pub fn own_value(p: &ProjectData, id: i64, field: Own) -> crate::grid::columns::Value {
    use crate::grid::columns::Value;
    let origin = p.model.origin_serial;
    let raw = |column: &str| projwbs_value(p, id, column);
    let amount = |column: &str| super::budget::stored_amount(&raw(column));
    let date = |column: &str| {
        openxer_xer::parse_relative(origin, &raw(column))
            .ok()
            .flatten()
            .map_or(Value::Empty, |minute| Value::Date {
                minute,
                mark: format::DateMark::Plain,
            })
    };
    let text = |s: String| {
        if s.is_empty() {
            Value::Empty
        } else {
            Value::Text(s)
        }
    };
    let pid = p.model.project.proj_id;
    let budget = || super::budget::node_budget(&p.document, pid, id, amount("orig_cost"));
    match field {
        Own::Code => text(code_of(p, id).to_string()),
        Own::Name => text(
            p.wbs
                .iter()
                .find(|w| w.id == id)
                .map_or(String::new(), |w| w.name.clone()),
        ),
        Own::Status => {
            let code = raw("status_code");
            text(
                STATUSES
                    .iter()
                    .find(|(k, _)| *k == code)
                    .map_or(code.clone(), |(_, v)| v.to_string()),
            )
        }
        Own::Manager => {
            let obs = raw("obs_id");
            text(
                obs_names(p)
                    .into_iter()
                    .find(|(k, _)| *k == obs)
                    .map_or(String::new(), |(_, v)| v),
            )
        }
        Own::AnticipatedStart => date("anticip_start_date"),
        Own::AnticipatedFinish => date("anticip_end_date"),
        Own::EstWeight => {
            let v = raw("est_wt");
            v.trim().parse::<f64>().map_or(Value::Empty, Value::Number)
        }
        Own::OriginalBudget => Value::Cost(amount("orig_cost")),
        Own::CurrentBudget => Value::Cost(budget().1),
        Own::ProposedBudget => Value::Cost(budget().2),
        Own::SpendingPlan => Value::Cost(super::budget::plan_totals(&p.document, pid, id).0),
        Own::BenefitPlan => Value::Cost(super::budget::plan_totals(&p.document, pid, id).1),
        Own::IndependentUnits => Value::Units(amount("indep_remain_work_qty")),
        Own::IndependentCost => Value::Cost(amount("indep_remain_total_cost")),
    }
}

/// A rolled-up value as the table shows it.
fn cell_text(p: &ProjectData, v: &crate::grid::columns::Value) -> String {
    crate::grid::columns::display(p, None, v)
}

/// The table text of a field of WBS element `id` itself; `obs` names the
/// responsible managers by `obs_id`.
fn own_text(p: &ProjectData, id: i64, field: Own, obs: &BTreeMap<String, String>) -> String {
    match field {
        // The manager by the lookup built once per table.
        Own::Manager => obs
            .get(&projwbs_value(p, id, "obs_id"))
            .cloned()
            .unwrap_or_default(),
        _ => crate::grid::columns::display(p, None, &own_value(p, id, field)),
    }
}

impl App {
    /// Table rows with their keys (`None` is a synthetic project row) and
    /// the Gantt bar of each row.
    /// Only the fields in `shown` get their text; the others stay empty.
    fn wbs_rows(&self, shown: &[usize]) -> (Vec<Option<i64>>, Vec<Row>, Vec<Option<chart::Bar>>) {
        let p = &self.project;
        let tree = Tree::build(p);
        let rollups = rollups(p, &tree);
        let origin = p.model.origin_serial;
        let project = project_node(p);
        let codes = full_codes(p, &tree);
        let nodes: BTreeMap<i64, &Wbs> = p.wbs.iter().map(|w| (w.id, w)).collect();
        let members = members(p, &tree);
        let all: Vec<usize> = (0..p.model.activities.len()).collect();
        let ix = self.grid_index();
        let obs: BTreeMap<String, String> = obs_names(p).into_iter().collect();
        let cells = |code: String, name: String, id: Option<i64>| {
            let list = id.and_then(|id| members.get(&id)).unwrap_or(&all);
            let mut out = vec![String::new(); wcols::catalog().len()];
            for &f in shown {
                let Some(c) = wcols::catalog().get(f) else {
                    continue;
                };
                out[f] = match (c.source, id) {
                    (Source::Own(Own::Code), _) => code.clone(),
                    (Source::Own(Own::Name), _) => name.clone(),
                    (Source::Own(field), Some(id)) => own_text(p, id, field, &obs),
                    (Source::Own(_), None) => String::new(),
                    (Source::Rolled(_), _) => {
                        let total = wcols::rolled_value(p, &ix, c, list);
                        total.map_or(String::new(), |v| cell_text(p, &v))
                    }
                };
            }
            out
        };
        let bar = |r: &Rollup, kind: chart::BarKind| {
            r.start.zip(r.finish).map(|(start, finish)| {
                let suffix = |actual: bool| if actual { " A" } else { "" };
                chart::Bar {
                    start,
                    finish,
                    kind,
                    start_label: (kind == chart::BarKind::Wbs).then(|| {
                        format!(
                            "{}{}",
                            format::date(origin, Some(start)),
                            suffix(r.actual_start())
                        )
                    }),
                    finish_label: format!(
                        "{}{}",
                        format::date(origin, Some(finish)),
                        suffix(r.actual_finish())
                    ),
                    ..Default::default()
                }
            })
        };
        let mut keys = vec![];
        let mut rows = vec![];
        let mut bars = vec![];
        let order = tree.preorder();
        let state = &self.views.wbs;
        let mut hidden_below: Option<usize> = None;
        if state.group != WbsGroup::Tree {
            // Every element once, without the tree: one list, or under a
            // band per responsible manager.
            let mut flat = vec![];
            for &(id, _) in &order {
                let is_project = Some(id) == project;
                let r = rollups.get(&Some(id)).cloned().unwrap_or_default();
                let (code, icon, kind) = if is_project {
                    let name = p.model.project.name.clone();
                    (name, chrome::Icon::ProjectOpen, chart::BarKind::ProjectNode)
                } else {
                    let code = codes.get(&id).cloned().unwrap_or_default();
                    (code, chrome::Icon::Wbs, chart::BarKind::Wbs)
                };
                let row = Row {
                    icon: Some(icon),
                    cells: cells(code, nodes[&id].name.clone(), Some(id)),
                    ..Default::default()
                };
                flat.push((id, row, bar(&r, kind)));
            }
            if state.group == WbsGroup::List {
                for (id, row, b) in flat {
                    keys.push(Some(id));
                    rows.push(row);
                    bars.push(b);
                }
                return (keys, rows, bars);
            }
            let manager = |id: i64| obs.get(&projwbs_value(p, id, "obs_id")).cloned();
            let mut bands: BTreeMap<(bool, String), Vec<Listed>> = BTreeMap::new();
            for entry in flat {
                let name = manager(entry.0).unwrap_or_default();
                let at = (name.is_empty(), name);
                bands.entry(at).or_default().push(entry);
            }
            for (k, ((blank, name), members)) in bands.into_iter().enumerate() {
                let band = manager_band(k);
                let collapsed = state.collapsed.contains(&band);
                let mut caption = vec![String::new(); wcols::catalog().len()];
                caption[0] = if blank { "(none)".to_string() } else { name };
                keys.push(Some(band));
                rows.push(Row {
                    expander: Some(!collapsed),
                    cells: caption,
                    band: true,
                    ..Default::default()
                });
                bars.push(None);
                if collapsed {
                    continue;
                }
                for (id, row, b) in members {
                    keys.push(Some(id));
                    rows.push(Row {
                        depth: 1,
                        strips: 1,
                        ..row
                    });
                    bars.push(b);
                }
            }
            return (keys, rows, bars);
        }
        // Without a project node in the XER, a project row heads the tree.
        let (offset, include_root) = if project.is_none() {
            (1, true)
        } else {
            (0, false)
        };
        if include_root {
            keys.push(None);
            rows.push(Row {
                depth: 0,
                expander: (!tree.kids(None).is_empty()).then_some(!state.root_collapsed),
                icon: Some(chrome::Icon::ProjectOpen),
                cells: cells(p.model.project.name.clone(), project_name(p), None),
                ..Default::default()
            });
            bars.push(bar(&rollups[&None], chart::BarKind::ProjectNode));
            if state.root_collapsed {
                return (keys, rows, bars);
            }
        }
        for (id, depth) in order {
            if hidden_below.is_some_and(|d| depth > d) {
                continue;
            }
            hidden_below = None;
            let collapsed = state.collapsed.contains(&id);
            if collapsed {
                hidden_below = Some(depth);
            }
            let has_kids = !tree.kids(Some(id)).is_empty();
            let is_project = Some(id) == project;
            let w = nodes[&id];
            let r = rollups.get(&Some(id)).cloned().unwrap_or_default();
            keys.push(Some(id));
            rows.push(Row {
                depth: depth + offset,
                expander: has_kids.then_some(!collapsed),
                icon: Some(if is_project {
                    chrome::Icon::ProjectOpen
                } else {
                    chrome::Icon::Wbs
                }),
                cells: cells(
                    if is_project {
                        p.model.project.name.clone()
                    } else {
                        codes.get(&id).cloned().unwrap_or_default()
                    },
                    w.name.clone(),
                    Some(id),
                ),
                ..Default::default()
            });
            bars.push(bar(
                &r,
                if is_project {
                    chart::BarKind::ProjectNode
                } else {
                    chart::BarKind::Wbs
                },
            ));
        }
        (keys, rows, bars)
    }

    fn wbs_status(&mut self, result: Result<(), String>, done: &str) {
        match result {
            Ok(()) => {
                self.touch_records();
                self.status = done.into();
            }
            Err(e) => self.status = e,
        }
    }

    /// Runs the WBS menu item `key` on the selected element.
    pub(crate) fn wbs_menu_command(&mut self, key: &str) {
        match key {
            "add" => self.wbs_command("Add"),
            "cut" => self.wbs_command("Cut"),
            "copy" => self.wbs_command("Copy"),
            "paste" => self.wbs_command("Paste"),
            "delete" => self.wbs_command("Del. / Merge"),
            "shift_up" => self.wbs_command("Shift up"),
            "shift_down" => self.wbs_command("Shift down"),
            "shift_left" => self.wbs_command("Shift left"),
            "shift_right" => self.wbs_command("Shift right"),
            "details" => self.views.wbs.show_details = !self.views.wbs.show_details,
            "columns_customize" => self.open_wbs_columns(),
            "expand_all" => {
                self.views.wbs.collapsed.clear();
                self.views.wbs.root_collapsed = false;
            }
            "collapse_all" => {
                self.views.wbs.collapsed = self.project.wbs.iter().map(|w| w.id).collect();
                // The manager bands too (one per OBS element and "(none)").
                let bands = obs_names(&self.project).len() + 1;
                let all = (0..bands).map(manager_band);
                self.views.wbs.collapsed.extend(all);
            }
            "table_font" => self.open_table_font_for(crate::layout_opts::TableTarget::Wbs),
            "collapse_to" => self.open_collapse_to_for(crate::layout_opts::CollapseTarget::Wbs),
            other => {
                if let Some(&(g, _, _)) = WbsGroup::MENU.iter().find(|m| m.1 == other) {
                    self.views.wbs.group = g;
                    self.views.wbs.collapsed.clear();
                    self.views.wbs.root_collapsed = false;
                }
            }
        }
    }

    /// Collapse To's levels in the WBS window: one per tree level that has
    /// elements with children (the bands, grouped by manager).
    pub(crate) fn wbs_outline_levels(&self) -> Vec<String> {
        match self.views.wbs.group {
            WbsGroup::List => vec![],
            WbsGroup::Manager => vec!["Responsible Manager".into()],
            WbsGroup::Tree => {
                let p = &self.project;
                let tree = Tree::build(p);
                let offset = usize::from(project_node(p).is_none());
                let deepest = tree
                    .preorder()
                    .iter()
                    .filter(|(id, _)| !tree.kids(Some(*id)).is_empty())
                    .map(|(_, depth)| depth + offset + 1)
                    .max()
                    .unwrap_or(offset);
                (1..=deepest).map(|n| format!("WBS Level {n}")).collect()
            }
        }
    }

    /// Show tree levels 1 to `level` of the WBS table, the elements of
    /// `level` and below collapsed.
    pub(crate) fn wbs_collapse_to(&mut self, level: usize) {
        let level = level.max(1);
        if self.views.wbs.group == WbsGroup::Manager {
            self.wbs_menu_command("collapse_all");
            return;
        }
        let p = &self.project;
        let tree = Tree::build(p);
        let offset = usize::from(project_node(p).is_none());
        let s = &mut self.views.wbs;
        s.root_collapsed = offset == 1 && level == 1;
        s.collapsed = tree
            .preorder()
            .iter()
            .filter(|(_, depth)| depth + offset + 1 >= level)
            .filter(|(id, _)| !tree.kids(Some(*id)).is_empty())
            .map(|(id, _)| *id)
            .collect();
    }

    /// Columns > Customize... of the WBS table (the Layout bar's Columns...
    /// and the right-click menu): the shared Customize Columns dialog.
    fn open_wbs_columns(&mut self) {
        let doc = &self.project.document;
        let udfs = super::udf_columns::Shown::of(doc, &self.views.wbs.columns);
        let own = wcols::titles();
        let titles: Vec<String> = own
            .iter()
            .map(|t| t.to_string())
            .chain(udfs.titles)
            .collect();
        let shown = self
            .views
            .wbs
            .columns
            .order
            .iter()
            .filter_map(|&c| titles.get(c).cloned())
            .collect();
        let choices = super::udf_columns::choices(doc, "wbs");
        self.open_view_columns_with(
            crate::details::grid_columns::ColumnTarget::Wbs,
            "WBS",
            &own,
            &choices,
            crate::defaults::WBS_COLUMNS,
            shown,
        );
        if let Some(d) = self.detail.column_dialog.as_mut() {
            d.groups = wcols::groups().into_iter().map(str::to_string).collect();
        }
    }

    /// The WBS view has an element selected that Delete or Merge can remove.
    pub(crate) fn wbs_can_delete(&self) -> bool {
        self.views.main == super::MainView::Wbs
            && self
                .views
                .wbs
                .selected
                .is_some_and(|id| guard(&self.project, id).is_ok())
    }

    /// Edit > Delete and the Delete key in the WBS view: Delete or Merge on
    /// the selected element, the same as its command bar button.
    pub(crate) fn wbs_delete_selected(&mut self) {
        if self.wbs_can_delete() {
            self.wbs_command("Del. / Merge");
        }
    }

    pub(super) fn wbs_command(&mut self, command: &str) {
        let selected = self.views.wbs.selected;
        match command {
            "Add" => match add(&mut self.project, selected) {
                Ok(id) => {
                    self.views.wbs.selected = Some(id);
                    self.views.wbs.scroll_to_selected = true;
                    if let Some(parent) = self
                        .project
                        .wbs
                        .iter()
                        .find(|w| w.id == id)
                        .and_then(|w| w.parent)
                    {
                        self.views.wbs.collapsed.remove(&parent);
                    }
                    self.views.wbs.root_collapsed = false;
                    self.wbs_status(Ok(()), "WBS element added");
                }
                Err(e) => self.status = e,
            },
            "Cut" | "Copy" => match selected {
                Some(id) if !is_project_node(&self.project, id) => {
                    self.views.wbs.clipboard = Some(Clip {
                        root: id,
                        cut: command == "Cut",
                    });
                    self.status = format!("{command}: {}", code_of(&self.project, id));
                }
                _ => self.status = "Select a WBS element below the project.".into(),
            },
            "Paste" => {
                let Some(clip) = self.views.wbs.clipboard.clone() else {
                    self.status = "Nothing to paste.".into();
                    return;
                };
                // A copy pastes through Copy WBS Options (`wbs_copy`).
                if !clip.cut {
                    self.request_wbs_paste(clip, selected);
                    return;
                }
                match paste(&mut self.project, &clip, selected) {
                    Ok(id) => {
                        if clip.cut {
                            self.views.wbs.clipboard = None;
                        }
                        self.views.wbs.selected = Some(id);
                        if let Some(target) = selected {
                            self.views.wbs.collapsed.remove(&target);
                        }
                        self.wbs_status(Ok(()), "WBS pasted");
                    }
                    Err(e) => self.status = e,
                }
            }
            "Del. / Merge" => match selected.map(|id| (id, plan_delete(&self.project, id))) {
                Some((_, Ok(plan))) => self.views.wbs.pending = Some(plan),
                // Nothing to merge into: deleting takes the activities along.
                Some((id, Err(_))) if guard(&self.project, id).is_ok() => {
                    let tree = Tree::build(&self.project);
                    self.views.wbs.pending = Some(PendingDelete {
                        id,
                        activities: activity_counts(&self.project)
                            .get(&id)
                            .copied()
                            .unwrap_or(0),
                        children: tree.kids(Some(id)).len(),
                        delete_all: true,
                    });
                }
                Some((_, Err(e))) => self.status = e,
                None => self.status = "Select a WBS element first.".into(),
            },
            "Shift up" | "Shift down" | "Shift left" | "Shift right" => {
                let Some(id) = selected else {
                    self.status = "Select a WBS element first.".into();
                    return;
                };
                let result = match command {
                    "Shift up" => shift_up(&mut self.project, id),
                    "Shift down" => shift_down(&mut self.project, id),
                    "Shift left" => shift_left(&mut self.project, id),
                    _ => shift_right(&mut self.project, id),
                };
                if result.is_ok() && command == "Shift right" {
                    if let Some(parent) = self
                        .project
                        .wbs
                        .iter()
                        .find(|w| w.id == id)
                        .and_then(|w| w.parent)
                    {
                        self.views.wbs.collapsed.remove(&parent);
                    }
                }
                self.wbs_status(result, command);
            }
            _ => {}
        }
    }

    /// Delete All in the WBS delete confirmation: the element, those below
    /// it and their activities, as one undo step.
    fn confirm_delete_all(&mut self, time: f64) {
        let Some(plan) = self.views.wbs.pending.take() else {
            return;
        };
        let parent = self
            .project
            .wbs
            .iter()
            .find(|w| w.id == plan.id)
            .and_then(|w| w.parent);
        let before = self.project.clone();
        let before_revision = self.records_revision;
        match delete_with_activities(&mut self.project, plan.id) {
            Ok(n) => {
                self.edit_step(before, before_revision, time);
                let count = self.project.model.activities.len();
                self.selected = self.selected.filter(|&i| i < count);
                self.views.wbs.selected =
                    parent.filter(|id| self.project.wbs.iter().any(|w| w.id == *id));
                self.status = format!("WBS element deleted with {n} activities");
            }
            Err(e) => {
                self.project = before;
                self.status = e;
            }
        }
    }

    fn confirm_delete(&mut self) {
        let Some(plan) = self.views.wbs.pending.take() else {
            return;
        };
        let parent = self
            .project
            .wbs
            .iter()
            .find(|w| w.id == plan.id)
            .and_then(|w| w.parent);
        let result = delete_merge(&mut self.project, plan.id);
        if result.is_ok() {
            self.views.wbs.selected =
                parent.filter(|id| self.project.wbs.iter().any(|w| w.id == *id));
        }
        self.wbs_status(
            result,
            if plan.activities > 0 || plan.children > 0 {
                "WBS element merged into its parent"
            } else {
                "WBS element deleted"
            },
        );
    }

    pub(super) fn wbs_view(&mut self, host: &mut egui::Ui) {
        let ctx = &host.ctx().clone();
        // The right dock's Edit and Move bars run these same commands; the
        // view's own bar holds only those of a bar the user has hidden.
        let (edit_bar, move_bar) = (self.bar_visible("Edit"), self.bar_visible("Move"));
        let shown = |name: &str| {
            if name.starts_with("Shift") {
                !move_bar
            } else {
                !edit_bar
            }
        };
        if !(edit_bar && move_bar) {
            self.wbs_command_bar(host, &shown);
        }
        self.wbs_view_body(host, ctx);
    }

    /// The WBS view's own command bar: the commands `shown` keeps.
    fn wbs_command_bar(&mut self, host: &mut egui::Ui, shown: &dyn Fn(&str) -> bool) {
        egui::SidePanel::right("wbs_command_bar")
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
                let has_selection = self
                    .views
                    .wbs
                    .selected
                    .is_some_and(|id| !is_project_node(&self.project, id));
                for (name, icon, enabled) in [
                    ("Add", chrome::Icon::Add, true),
                    ("Del. / Merge", chrome::Icon::Delete, has_selection),
                    ("Cut", chrome::Icon::Cut, has_selection),
                    ("Copy", chrome::Icon::Copy, has_selection),
                    (
                        "Paste",
                        chrome::Icon::Paste,
                        self.views.wbs.clipboard.is_some(),
                    ),
                    ("Shift up", chrome::Icon::ShiftUp, has_selection),
                    ("Shift down", chrome::Icon::ShiftDown, has_selection),
                    ("Shift left", chrome::Icon::ShiftLeft, has_selection),
                    ("Shift right", chrome::Icon::ShiftRight, has_selection),
                ]
                .into_iter()
                .filter(|(name, _, _)| shown(name))
                {
                    // The command keeps its name (and action key); the
                    // button reads in plain words.
                    let label = if name == "Del. / Merge" {
                        "Delete or Merge..."
                    } else {
                        name
                    };
                    let response = sheet::side_button(ui, icon, label, enabled);
                    self.action(&format!("wbs_cmd_{name}"), &response);
                    if response.clicked() {
                        self.wbs_command(name);
                    }
                }
            });
    }

    /// The WBS view beside its command bar: details and the table.
    fn wbs_view_body(&mut self, host: &mut egui::Ui, ctx: &egui::Context) {
        if self.views.wbs.show_details {
            let tab = self.views.wbs.tab;
            let (height, clicked) = self.details_pane(
                host,
                "wbs",
                self.views.wbs.details_height,
                &TABS,
                tab,
                |app, c, size| app.wbs_details(c, size, tab),
            );
            self.views.wbs.details_height = height;
            if let Some(tab) = clicked {
                self.views.wbs.tab = tab;
            }
        }
        let label_options = self.label_options();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(sheet::MARGIN))
            .show_inside(host, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                let (inner, strip) = sheet::table_frame(ui, ui.max_rect());
                let bar = Rect::from_min_size(strip, Vec2::new(inner.width(), 17.));
                let header = sheet::header_button(ui, "wbs", bar, "Layout: WBS");
                self.action("wbs_layout", &header);
                let mut columns = false;
                egui::Popup::menu(&header).show(|ui| {
                    let r = ui.button("Columns...");
                    self.action("wbs_layout_columns", &r);
                    if r.clicked() {
                        columns = true;
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Expand All").clicked() {
                        self.views.wbs.collapsed.clear();
                        self.views.wbs.root_collapsed = false;
                        ui.close();
                    }
                    if ui.button("Collapse All").clicked() {
                        self.views.wbs.collapsed = self.project.wbs.iter().map(|w| w.id).collect();
                        ui.close();
                    }
                    ui.separator();
                    ui.checkbox(&mut self.views.wbs.show_details, "WBS Details");
                });
                if columns {
                    self.open_wbs_columns();
                }
                let body = Rect::from_min_max(Pos2::new(inner.left(), bar.bottom()), inner.max);
                let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(body));
                let ui = &mut ui;
                // Rows, rollups and bars are kept while the project and the
                // collapsed elements are unchanged.
                let collapsed: Vec<i64> = self.views.wbs.collapsed.iter().copied().collect();
                // The User Defined columns after the table's own.
                let udfs =
                    super::udf_columns::Shown::of(&self.project.document, &self.views.wbs.columns);
                let extra = self.views.wbs.columns.extra.clone();
                let shown_fields = self.views.wbs.columns.order.clone();
                let built = self.memo(
                    self.project_serial(),
                    "wbs_rows",
                    (
                        collapsed,
                        self.views.wbs.root_collapsed,
                        extra.clone(),
                        shown_fields.clone(),
                        self.views.wbs.group,
                        format!("{:?}", format::current()),
                    ),
                    || {
                        let mut built = self.wbs_rows(&shown_fields);
                        let origin = self.project.model.origin_serial;
                        udfs.fill(&self.project.document, origin, &mut built.1, &built.0);
                        built
                    },
                );
                let (keys, rows, bars) = (&built.0, &built.1, &built.2);
                let titles: Vec<String> = wcols::titles()
                    .into_iter()
                    .map(str::to_string)
                    .chain(udfs.titles.iter().cloned())
                    .collect();
                // The rows for the table's cell commands (Find, Replace).
                let cell_rows: std::rc::Rc<dyn super::table_cells::RowCells> = built.clone();
                self.note_view_cells(
                    "wbs",
                    wcols::ids()
                        .into_iter()
                        .map(str::to_string)
                        .chain(extra.iter().cloned())
                        .collect(),
                    titles.clone(),
                    keys.iter()
                        .map(|k| Some(super::table_cells::CellKey::Wbs(*k)))
                        .collect(),
                    cell_rows,
                );
                let mut cell_edit = self.take_cell_edit("wbs");
                let first = bars.iter().flatten().map(|b| b.start).min();
                let mut gantt = chart::SummaryGantt::new(
                    self.project.model.origin_serial,
                    first,
                    label_options,
                    0.,
                );
                gantt.bars = bars.clone();
                // The chart scrolls and zooms like the Gantt, except in a
                // printout, which shows the whole chart at its own scale.
                let painting = self.print.painting;
                let default = if painting {
                    gantt.room_for_start_labels(ui.painter());
                    0.
                } else {
                    gantt.follow(self.views.wbs.timeline_first, self.views.wbs.zoom)
                };
                let selected_row = keys.iter().position(|k| *k == self.views.wbs.selected);
                let shown: Vec<Option<Option<i64>>> = keys.iter().copied().map(Some).collect();
                let s = &mut self.views.wbs;
                let mut current = Some(s.selected);
                let selection = s.rows.shown(&mut current, &shown);
                if let Some(next) = current.filter(|next| *next != s.selected) {
                    s.selected = next;
                    s.code_edit = None;
                }
                let scroll = self
                    .views
                    .wbs
                    .scroll_to_selected
                    .then_some(selected_row)
                    .flatten();
                self.views.wbs.scroll_to_selected = false;
                let state = MenuState {
                    clipboard: self.views.wbs.clipboard.is_some(),
                    details: self.views.wbs.show_details,
                    collapsed: &self.views.wbs.collapsed,
                    root_collapsed: self.views.wbs.root_collapsed,
                    group: self.views.wbs.group,
                };
                let project = &self.project;
                // Past the last row (the empty area) the menu reads as the
                // project row's: no element to cut, copy or delete.
                let menu = |row: usize| wbs_menu(project, state, keys.get(row).copied().flatten());
                let details_shown = state.details;
                let chart_menu = move || vec![wbs_details_item(details_shown)];
                let open_menu = self.ctx_menu.take();
                let title_refs: Vec<&str> = titles.iter().map(String::as_str).collect();
                let right: Vec<usize> = wcols::right().into_iter().chain(udfs.right()).collect();
                let indicators = udfs.indicators();
                let doc = &self.project.document;
                // The project row holds no WBS element of its own here.
                let obs = obs_names(project);
                let kind = |row: usize, field: usize| {
                    // A manager band holds no element (negative id).
                    let element = keys.get(row).copied().flatten().filter(|id| *id > 0)?;
                    match udfs.field(field) {
                        Some(ty) => super::udf_columns::editor(doc, ty),
                        None => cell_editor(project, element, field, &obs),
                    }
                };
                let table = grid::Table {
                    id: "wbs",
                    titles: &title_refs,
                    right: &right,
                    tree_field: 0,
                    header_height: grid::HEADER_HEIGHT,
                    row_lines: false,
                    band_pitch: grid::BandPitch::Styled,
                    band_caption: &[],
                    menu: Some(&menu),
                    open_menu,
                    chart_menu: Some(&chart_menu),
                };
                let mut width = self.views.wbs.table_width;
                let view = &self.views.wbs;
                let look = grid::Look::of(&view.table, view.row_height);
                let response = grid::show_with(
                    ui,
                    &table,
                    &mut self.views.wbs.columns,
                    rows,
                    &if self.print.painting {
                        grid::Selection::default()
                    } else {
                        selection
                    },
                    scroll,
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
                self.keep_cell_edit("wbs", cell_edit);
                if let Some((row, field, text)) = response.committed.clone() {
                    let ty = udfs.field(field).map(|t| t.type_id);
                    let time = ui.ctx().input(|i| i.time);
                    match (ty, keys.get(row)) {
                        (Some(type_id), Some(Some(element))) => {
                            self.commit_udf_cell(type_id, *element, &text, time);
                        }
                        (None, Some(Some(element))) => {
                            if let Some(c) = wcols::catalog().get(field) {
                                self.commit_wbs_cell(*element, c.id, &text, time);
                            }
                        }
                        _ => {}
                    }
                }
                self.views.wbs.table_width = width;
                if !painting {
                    let s = &mut self.views.wbs;
                    (s.timeline_first, s.zoom, s.timeline_place) =
                        gantt.kept(s.timeline_first, default);
                    s.timeline_default = default;
                }
                for (row, r) in &gantt.drawn {
                    self.action_rects.insert(
                        format!("wbs_bar_{row}"),
                        [r.left(), r.top(), r.right(), r.bottom()],
                    );
                }
                if let Some(i) = response.toggled {
                    match keys[i] {
                        None => self.views.wbs.root_collapsed = !self.views.wbs.root_collapsed,
                        Some(id) => {
                            if !self.views.wbs.collapsed.insert(id) {
                                self.views.wbs.collapsed.remove(&id);
                            }
                        }
                    }
                }
                if let Some(i) = response.clicked {
                    let s = &mut self.views.wbs;
                    let current = s.selected;
                    let key = keys[i];
                    s.selected =
                        s.rows
                            .click(Some(&current), key, response.modifiers, response.secondary);
                    s.code_edit = None;
                }
                if let Some((i, key)) = response.menu {
                    if let Some(&element) = keys.get(i) {
                        self.views.wbs.selected = element;
                    }
                    self.wbs_menu_command(key);
                }
                if let Some(key) = response.chart_menu {
                    self.wbs_menu_command(key);
                }
            });
        self.wbs_delete_dialog(ctx);
        self.copy_wbs_options_dialog(ctx);
    }

    fn wbs_delete_dialog(&mut self, ctx: &egui::Context) {
        let Some(plan) = self.views.wbs.pending.clone() else {
            return;
        };
        let code = code_of(&self.project, plan.id).to_string();
        let parent = self
            .project
            .wbs
            .iter()
            .find(|w| w.id == plan.id)
            .and_then(|w| w.parent)
            .and_then(|id| self.project.wbs.iter().find(|w| w.id == id))
            .map(|w| w.code.clone());
        let listed = if plan.delete_all {
            subtree_activities(&self.project, plan.id)
        } else {
            vec![]
        };
        let subtree = Tree::build(&self.project).descendants(plan.id);
        let below = subtree.len() - 1;
        // The rows that go or move with the element: the subtree for Delete
        // All, the element alone for a merge.
        let nodes: BTreeSet<i64> = if plan.delete_all {
            subtree.into_iter().collect()
        } else {
            BTreeSet::from([plan.id])
        };
        let related = related(&self.project, &nodes);
        let target = parent.as_deref().unwrap_or("the parent").to_string();
        let also = if plan.delete_all || parent.is_none() {
            let mut lines = vec![];
            let removed = related.removed();
            if !removed.is_empty() {
                lines.push(format!("Also deleted: {}.", removed.join(", ")));
            }
            let referring = related.referring();
            if !referring.is_empty() {
                lines.push(match &parent {
                    Some(code) => format!("Moved to {code}: {}.", referring.join(", ")),
                    None => format!("No longer tied to a WBS element: {}.", referring.join(", ")),
                });
            }
            lines
        } else {
            let (gone, mut moved): (Vec<String>, Vec<String>) = related
                .removed()
                .into_iter()
                .partition(|l| l.contains("notebook") || l.contains("user defined"));
            moved.extend(related.referring());
            let mut lines = vec![];
            if !moved.is_empty() {
                lines.push(format!("Also moved to {target}: {}.", moved.join(", ")));
            }
            if !gone.is_empty() {
                lines.push(format!("Also deleted: {}.", gone.join(", ")));
            }
            lines
        };
        let mut merge = false;
        let mut delete_all = false;
        let mut confirm_all = false;
        let mut cancel = false;
        let has_contents = plan.activities > 0 || plan.children > 0;
        crate::chrome::plain_window(if plan.delete_all {
            "Delete WBS Element and Activities"
        } else if has_contents {
            "Merge or Delete WBS Element"
        } else {
            "Delete WBS Element"
        })
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            if plan.delete_all {
                ui.label(format!(
                    "Delete WBS element {code}, the {below} element(s) below it and these {} activities?",
                    listed.len()
                ));
                egui::ScrollArea::vertical()
                    .max_height(160.)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (_, name) in &listed {
                            ui.label(RichText::new(name).color(Color32::from_gray(80)));
                        }
                    });
                ui.label("Their relationships, assignments, steps and other details go with them.");
            } else if has_contents {
                ui.label(format!(
                    "WBS element {code} has {} activities and {} child elements.",
                    plan.activities, plan.children
                ));
                ui.label(format!(
                    "Merge moves them to {} and removes {code}. Delete All removes them with it.",
                    parent.as_deref().unwrap_or("the parent")
                ));
            } else {
                ui.label(format!("Delete WBS element {code}?"));
            }
            for line in &also {
                ui.label(line);
            }
            ui.add_space(6.);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = crate::dialog_kit::ROW_GAP;
                // The first question (Merge, Delete All) leads to the second
                // when Delete All is picked; each takes its default from the
                // shared table.
                let question = if has_contents && !plan.delete_all {
                    crate::kit_window::Question::WbsMergeOrDelete
                } else {
                    crate::kit_window::Question::WbsDelete
                };
                let button = |ui: &mut egui::Ui, caption: &str| {
                    let response = crate::chrome::dialog_button(
                        ui,
                        &crate::chrome::DialogButton::new(caption, None),
                    );
                    if question.is_default(caption) {
                        crate::dialog_keys::mark_default(ui, &response);
                    }
                    response
                };
                if plan.delete_all {
                    let response = button(ui, "Delete");
                    self.action("wbs_confirm_delete_all_yes", &response);
                    confirm_all = response.clicked();
                } else {
                    let label = if has_contents {
                        "Merge Element(s)"
                    } else {
                        "Delete"
                    };
                    let response = button(ui, label);
                    self.action("wbs_confirm_merge", &response);
                    merge = response.clicked();
                    if has_contents {
                        let response = button(ui, "Delete All");
                        self.action("wbs_confirm_delete_all", &response);
                        delete_all = response.clicked();
                    }
                }
                let response = button(ui, "Cancel");
                self.action("wbs_confirm_cancel", &response);
                cancel = response.clicked();
            });
        });
        if merge {
            self.confirm_delete();
        } else if delete_all {
            if let Some(p) = self.views.wbs.pending.as_mut() {
                p.delete_all = true;
            }
        } else if confirm_all {
            self.confirm_delete_all(ctx.input(|i| i.time));
        } else if cancel || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.views.wbs.pending = None;
        }
    }

    /// Set the WBS table field `id` (a `wbs_columns` id) of element
    /// `element` from the text typed or picked in its cell, through the
    /// shared edit layer. Returns whether it changed; the caller makes the
    /// undo step. Only the fields `cell_editor` opens are accepted.
    pub(crate) fn set_wbs_cell(
        &mut self,
        element: i64,
        id: &str,
        text: &str,
    ) -> Result<bool, String> {
        let p = &self.project;
        let obs = obs_names(p);
        let field = wcols::field_of(id)
            .filter(|&f| cell_editor(p, element, f, &obs).is_some())
            .ok_or("That column cannot be typed in here.")?;
        let Source::Own(own) = wcols::catalog()[field].source else {
            return Err("That column cannot be typed in here.".into());
        };
        let column = own.column().ok_or("That column cannot be typed in here.")?;
        let text = text.trim();
        let value = match own {
            // The cell shows the whole code path; what follows the
            // parent's path is the element's own code.
            Own::Code => {
                let tree = Tree::build(p);
                let parent = tree.parent.get(&element).copied().flatten();
                let prefix = parent
                    .and_then(|id| full_codes(p, &tree).remove(&id))
                    .map(|path| format!("{path}{}", separator(p)));
                let code = prefix
                    .as_deref()
                    .and_then(|path| text.strip_prefix(path))
                    .unwrap_or(text);
                if code.is_empty() {
                    return Err("A WBS element needs a code.".into());
                }
                code.to_string()
            }
            Own::Name if text.is_empty() => return Err("A WBS element needs a name.".into()),
            Own::Status => STATUSES
                .iter()
                .find(|(k, v)| v.eq_ignore_ascii_case(text) || k.eq_ignore_ascii_case(text))
                .map(|(k, _)| k.to_string())
                .ok_or_else(|| format!("{text:?} is not a WBS status."))?,
            Own::Manager if text.is_empty() => String::new(),
            Own::Manager => obs
                .iter()
                .find(|(_, name)| name.as_str() == text)
                .map(|(k, _)| k.clone())
                .ok_or_else(|| format!("{text:?} is not in the OBS."))?,
            Own::AnticipatedStart | Own::AnticipatedFinish if text.is_empty() => String::new(),
            Own::AnticipatedStart | Own::AnticipatedFinish => {
                match crate::grid::columns::parse_input(
                    p,
                    None,
                    crate::grid::columns::Kind::Date,
                    text,
                )? {
                    crate::grid::columns::Value::Date { minute, .. } => {
                        openxer_xer::date_string(p.model.origin_serial, Some(minute))
                    }
                    _ => String::new(),
                }
            }
            _ => text.to_string(),
        };
        let changed =
            openxer_xer::edit::wbs::set_column(&mut self.project, element, column, &value)?;
        if changed {
            self.touch_records();
        }
        Ok(changed)
    }

    /// A WBS table cell typed or picked: one undo step when the value
    /// changed; a value the field refuses is reported and changes nothing.
    fn commit_wbs_cell(&mut self, element: i64, id: &str, text: &str, time: f64) {
        let before = self.project.clone();
        let before_revision = self.records_revision;
        match self.set_wbs_cell(element, id, text) {
            Ok(true) => self.edit_step(before, before_revision, time),
            Ok(false) => {}
            Err(e) => {
                self.project = before;
                self.error(e);
            }
        }
    }

    /// Write PROJWBS column `field` of element `id` through the shared edit
    /// layer (`xer::edit::wbs::set_column`).
    fn set_wbs_field(&mut self, id: i64, field: &str, value: String) {
        match openxer_xer::edit::wbs::set_column(&mut self.project, id, field, &value) {
            Ok(true) => self.touch_records(),
            Ok(false) => {}
            Err(e) => self.status = e,
        }
    }

    fn wbs_details(&mut self, c: &mut Canvas, size: Vec2, tab: usize) {
        let (w, h) = (size.x, size.y);
        // The selected element, or the project node for the project row.
        let id = self
            .views
            .wbs
            .selected
            .filter(|id| self.project.wbs.iter().any(|w| w.id == *id))
            .or_else(|| project_node(&self.project));
        let node: BTreeMap<String, String> = id.map_or_else(BTreeMap::new, |id| {
            self.project
                .document
                .table("PROJWBS")
                .map_or(BTreeMap::new(), |t| {
                    t.rows
                        .iter()
                        .find(|r| num(t.value(r, "wbs_id")) == id)
                        .map(|r| {
                            t.fields
                                .iter()
                                .enumerate()
                                .map(|(i, f)| {
                                    (
                                        f.to_ascii_lowercase(),
                                        r.get(i).cloned().unwrap_or_default(),
                                    )
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                })
        });
        match TABS[tab.min(TABS.len() - 1)] {
            "General" => self.wbs_general(c, w, h, id),
            "Notebook" => {
                // The project row's notebook is the project's own.
                let owner = id.map(|id| match project_node(&self.project) {
                    Some(node) if node == id => Owner::Project(id),
                    _ => Owner::Wbs(id),
                });
                let content = c.r(2., 2., w - 4., h - 4.);
                let topic = Some(self.views.wbs.topic);
                let out = self.notebook_pane(
                    c.ui,
                    content,
                    "wbs_notebook",
                    "wbs_notebook",
                    owner,
                    topic,
                    owner.is_some(),
                );
                if let Some(r) = out.select {
                    self.views.wbs.topic = r;
                }
                if out.cleared {
                    self.views.wbs.topic = 0;
                }
                if let (true, Some(owner)) = (out.add, owner) {
                    self.open_topic_picker(owner);
                }
            }
            "Budget Log" | "Spending Plan" | "Budget Summary" => {
                let scope = id.map(|wbs| budget::Scope {
                    proj: self.project.model.project.proj_id,
                    wbs,
                });
                match TABS[tab.min(TABS.len() - 1)] {
                    "Budget Log" => self.budget_log_tab(c, "wbs", w, h, scope, &node),
                    "Spending Plan" => self.spending_plan_tab(c, "wbs", w, h, scope, &node),
                    _ => self.budget_summary_tab(c, w, h, scope, &node),
                }
            }
            "WBS Milestones" => self.wbs_milestones(c, w, h, id),
            "Documents" => self.wbs_documents(c, w, h, id),
            _ => self.wbs_earned_value(c, w, h, id),
        }
    }

    /// The Earned Value tab: the element's (or the project node's)
    /// techniques for performance percent complete and for the estimate to
    /// complete, edited through the shared earned value setters. Each change
    /// is one undo step; the earned value columns read the same fields.
    fn wbs_earned_value(&mut self, c: &mut Canvas, w: f32, h: f32, id: Option<i64>) {
        use openxer_xer::edit::earned_value as ev;
        // Without an element (a project with no project node and the
        // project row selected) the defaults show, greyed out.
        let current = id.and_then(|id| ev::get(&self.project, id).ok().map(|t| (id, t)));
        let editable = current.is_some();
        let (id, t) = current.unwrap_or((
            0,
            ev::Techniques {
                performance: ev::PERFORMANCE[0].0.into(),
                custom_percent: 0.,
                to_complete: ev::TO_COMPLETE[0].0.into(),
                factor: 1.,
            },
        ));
        sheet::section(
            c,
            5.,
            5.,
            340.,
            h - 5.,
            "Technique for computing performance percent complete",
        );
        sheet::section(
            c,
            346.,
            5.,
            (w - 351.).max(200.),
            h - 5.,
            "Technique for computing Estimate to Complete (ETC)",
        );
        let mut error = None;
        let mut changed = false;
        let mut result = |r: Result<bool, String>| match r {
            Ok(c) => changed |= c,
            Err(e) => error = Some(e),
        };
        for (key, y, code) in [
            ("wbs_ev_activity", 47., "EC_Cmp_pct"),
            ("wbs_ev_milestones", 115., "EC_WBS_pct"),
            ("wbs_ev_0_100", 137., "EC_0_100"),
            ("wbs_ev_50_50", 159., "EC_50_50"),
            ("wbs_ev_custom", 181., "EC_User_pct"),
        ] {
            let caption = ev::PERFORMANCE
                .iter()
                .find(|(c, _, _)| *c == code)
                .map_or(code, |(_, _, caption)| caption);
            if c.radio(
                key,
                15.,
                y,
                252.,
                22.,
                caption,
                t.performance == code,
                editable,
            ) {
                result(ev::set_performance(&mut self.project, id, code));
            }
        }
        // Resource curves and future period buckets are not modeled: earned
        // value spreads over the activity dates.
        let mut curves = false;
        c.checkbox(
            "wbs_ev_curves",
            34.,
            79.,
            273.,
            17.,
            "Use resource curves / future period buckets",
            &mut curves,
            false,
        );
        let r = c.r(34., 79., 273., 17.);
        sheet::not_in_xer(
            c,
            "wbs_ev_curves_tip",
            r,
            "Resource curves and future period buckets are not modeled; activity percent complete is used as is.",
        );
        let custom = editable && t.performance == "EC_User_pct";
        let shown = records::format_number(t.custom_percent);
        if let Some(text) = sheet::commit_field(
            c,
            "wbs_ev_custom_pct",
            199.,
            183.,
            61.,
            &shown,
            custom,
            Align::Max,
        ) {
            result(ev::set_custom_percent(&mut self.project, id, &text));
        }
        c.label(263., 183., 21., "%", custom);
        if c.radio(
            "wbs_etc_remaining",
            358.,
            40.,
            479.,
            15.,
            ev::TO_COMPLETE[0].2,
            t.to_complete == "EE_Rem_hr",
            editable,
        ) {
            result(ev::set_to_complete(&mut self.project, id, "EE_Rem_hr"));
        }
        c.label(450., 60., 13., "or", true);
        c.label(
            358.,
            78.,
            13.,
            "ETC = PF * (Budget at Completion - Earned Value), where:",
            true,
        );
        for (key, y, code) in [
            ("wbs_etc_pf_1", 115., "EE_PF_1"),
            ("wbs_etc_pf_cpi", 137., "EE_PF_cpi"),
            ("wbs_etc_pf_cpi_spi", 159., "EE_PF_cpi_spi"),
            ("wbs_etc_pf_value", 181., "EE_PF_usr"),
        ] {
            let caption = ev::TO_COMPLETE
                .iter()
                .find(|(c, _, _)| *c == code)
                .map_or(code, |(_, _, caption)| caption);
            let width = if code == "EE_PF_usr" { 50. } else { 499. };
            if c.radio(
                key,
                357.,
                y,
                width,
                22.,
                caption,
                t.to_complete == code,
                editable,
            ) {
                result(ev::set_to_complete(&mut self.project, id, code));
            }
        }
        let factor = editable && t.to_complete == "EE_PF_usr";
        if let Some(text) = sheet::commit_field(
            c,
            "wbs_etc_pf",
            412.,
            185.,
            68.,
            &records::format_number(t.factor),
            factor,
            Align::Max,
        ) {
            result(ev::set_factor(&mut self.project, id, &text));
        }
        if changed {
            self.touch_records();
        }
        if let Some(e) = error {
            self.status = e;
        }
    }

    /// Documents: the work products and documents assigned to the WBS
    /// element itself (TASKDOC rows with no activity), sorted by title, with
    /// a Work Product check box; Assign, Remove and Details....
    fn wbs_documents(&mut self, c: &mut Canvas, w: f32, h: f32, id: Option<i64>) {
        let pid = self.project.model.project.proj_id;
        let rows = id.map_or(vec![], |id| {
            documents::wbs_assignments(&self.project.document, pid, id)
        });
        let selected = self.views.wbs.document.filter(|&r| r < rows.len());
        let (x, y, tw, th) = (12., 12., w - 24., h - 44.);
        let rest = (tw - 4. - 650.).max(100.);
        let cols = [
            ("Title", 550., false),
            ("Status Code", 100., false),
            ("Work Product", rest, false),
        ];
        let cells: Vec<Vec<String>> = rows
            .iter()
            .map(|(_, _, title, status)| vec![title.clone(), status.clone(), String::new()])
            .collect();
        if let Some(r) = sheet::table(c, "wbs_documents", x, y, tw, th, &cols, &cells, selected) {
            self.views.wbs.document = Some(r);
        }
        // Work Product check boxes, centered in their column.
        let mut flag = None;
        for (i, (a, _, _, _)) in rows.iter().enumerate() {
            let Some(top) = sheet::row_top(c, "wbs_documents", y, th, i) else {
                continue;
            };
            let mut wp = a.work_product;
            let left = x + 2. + 650. + (rest / 2. - 7.).floor();
            if c.checkbox(
                &format!("wbs_documents_wp_{i}"),
                left,
                top,
                13.,
                18.,
                "",
                &mut wp,
                true,
            )
            .clicked()
            {
                flag = Some((a.row, wp));
            }
        }
        let available = id.is_some_and(|id| {
            !documents::unassigned_for_wbs(&self.project.document, pid, id).is_empty()
        });
        match sheet::buttons(
            c,
            "wbs_documents_buttons",
            11.,
            h - 26.,
            &[("Assign", available), ("Remove", selected.is_some())],
        ) {
            Some(0) => {
                if let Some(id) = id {
                    self.open_picker(record_view::PickTarget::WbsDocument(id));
                }
            }
            Some(1) => {
                if let Some(r) = selected {
                    openxer_xer::edit::documents::remove_link(&mut self.project, rows[r].0.row);
                    self.views.wbs.document = None;
                    self.touch_records();
                }
            }
            _ => {}
        }
        if c.button(
            "wbs_documents_details",
            189.,
            h - 26.,
            87.,
            21.,
            "Details...",
            false,
            selected.is_some(),
        ) {
            self.detail.document_details = selected.map(|r| rows[r].1);
        }
        if let Some((row, wp)) = flag {
            openxer_xer::edit::documents::set_work_product(&mut self.project, row, wp);
            self.touch_records();
        }
    }

    fn wbs_general(&mut self, c: &mut Canvas, w: f32, h: f32, id: Option<i64>) {
        let left_w = (w - 495.).max(500.);
        let right = left_w + 20.;
        sheet::section(c, 5., 5., left_w, h - 5., "General");
        sheet::section(
            c,
            right,
            5.,
            (w - right - 5.).max(100.),
            h - 5.,
            "Anticipated Dates",
        );
        let name_w = (left_w - 408.).max(120.);
        let is_project = id.is_none_or(|id| is_project_node(&self.project, id));
        let mut error = None;
        let mut changed = false;
        // WBS Code: renamed on Enter or focus loss, checked for duplicates.
        let current = match id {
            Some(id) if !is_project => code_of(&self.project, id).to_string(),
            _ => self.project.model.project.name.clone(),
        };
        sheet::above(c, 46., 53., "WBS Code", true);
        if let Some(text) = sheet::commit_field(
            c,
            "wbs_code_edit",
            46.,
            53.,
            312.,
            &current,
            !is_project,
            Align::Min,
        ) {
            if let Some(id) = id.filter(|_| !is_project) {
                match rename_code(&mut self.project, id, &text) {
                    Ok(()) => changed = true,
                    Err(e) => error = Some(e),
                }
            }
        }
        let name = match id {
            Some(id) => self
                .project
                .wbs
                .iter()
                .find(|w| w.id == id)
                .map_or(String::new(), |w| w.name.clone()),
            None => project_name(&self.project),
        };
        if let Some(text) = sheet::text_field(
            c,
            "wbs_name_edit",
            382.,
            53.,
            name_w,
            "WBS Name",
            &name,
            id.is_some(),
        ) {
            if let Some(w) = id.and_then(|id| self.project.wbs.iter_mut().find(|w| w.id == id)) {
                w.name = text;
                sync_projwbs(&mut self.project);
                changed = true;
            }
        }
        sheet::above(c, 46., 99., "Status", true);
        let status = id.map_or(String::new(), |id| {
            projwbs_value(&self.project, id, "status_code")
        });
        if let Some(code) = sheet::combo(
            c,
            "wbs_status",
            46.,
            99.,
            312.,
            &STATUSES,
            &status,
            id.is_some(),
        ) {
            if let Some(id) = id {
                self.set_wbs_field(id, "status_code", code);
            }
        }
        sheet::above(c, 382., 99., "Responsible Manager", true);
        let obs = obs_names(&self.project);
        let options: Vec<(&str, &str)> =
            obs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let obs_id = id.map_or(String::new(), |id| {
            projwbs_value(&self.project, id, "obs_id")
        });
        if let Some(code) = sheet::combo(
            c,
            "wbs_obs",
            382.,
            99.,
            name_w,
            &options,
            &obs_id,
            id.is_some() && !options.is_empty(),
        ) {
            if let Some(id) = id {
                self.set_wbs_field(id, "obs_id", code);
            }
        }
        let origin = self.project.model.origin_serial;
        let fallback = self.project.model.project.data_date.unwrap_or(0);
        for (y, caption, field) in [
            (53., "Anticipated Start", "anticip_start_date"),
            (101., "Anticipated Finish", "anticip_end_date"),
        ] {
            sheet::above(c, right + 29., y, caption, true);
            let value = id.and_then(|id| {
                openxer_xer::parse_relative(origin, &projwbs_value(&self.project, id, field))
                    .ok()
                    .flatten()
            });
            match sheet::date_field(
                c,
                &format!("wbs_{field}"),
                right + 29.,
                y,
                origin,
                value,
                fallback,
                id.is_some(),
            ) {
                Ok(Some(v)) => {
                    if let Some(id) = id {
                        self.set_wbs_field(id, field, openxer_xer::date_string(origin, v));
                    }
                }
                Ok(None) => {}
                Err(e) => error = Some(e),
            }
        }
        if changed {
            self.touch_records();
        }
        if let Some(e) = error {
            self.status = e;
        }
    }

    /// WBS Milestones: the weighted milestones of a WBS element, edited in
    /// place; the percent complete follows the completed weight.
    fn wbs_milestones(&mut self, c: &mut Canvas, w: f32, h: f32, id: Option<i64>) {
        let pid = self.project.model.project.proj_id;
        let list = id.map_or(vec![], |id| {
            budget::milestones(&self.project.document, pid, id)
        });
        let pct = budget::milestone_percent(&list);
        c.label(17., 11., 21., "% Complete", true);
        sheet::value_box(
            c,
            "wbs_milestones_pct",
            156.,
            11.,
            170.,
            &format!("{pct:.0}%"),
            true,
        );
        let rows: Vec<Vec<String>> = list
            .iter()
            .map(|m| {
                vec![
                    m.name.clone(),
                    records::format_number(m.weight),
                    if m.done { "Yes" } else { "No" }.into(),
                ]
            })
            .collect();
        let cols = [
            ("WBS Milestone", 200., false),
            ("Weight", 100., true),
            ("Completed", 100., false),
        ];
        let selected = self
            .views
            .budget
            .step_row
            .filter(|r| *r < list.len())
            .or((!list.is_empty()).then_some(0))
            .filter(|_| id.is_some());
        if let Some(i) = sheet::table(
            c,
            "wbs_milestones",
            12.,
            42.,
            w - 24.,
            h - 74.,
            &cols,
            &rows,
            selected,
        ) {
            self.views.budget.step_row = Some(i);
        }
        if rows.is_empty() {
            c.label(24., 70., 18., "No WBS milestones are recorded.", false);
        }
        let mut edit = None;
        let top = selected.and_then(|i| sheet::row_top(c, "wbs_milestones", 42., h - 74., i));
        if let (Some(i), Some(top)) = (selected, top) {
            let mut x = 14.;
            for (column, (field, (_, width, right))) in [
                budget::StepField::Name,
                budget::StepField::Weight,
                budget::StepField::Done,
            ]
            .iter()
            .zip(cols)
            .enumerate()
            {
                let align = if right { Align::Max } else { Align::Min };
                if let Some(text) = sheet::commit_cell(
                    c,
                    &format!("wbs_milestone_cell_{column}"),
                    (x, top, width, 18.),
                    &rows[i][column],
                    true,
                    align,
                ) {
                    edit = Some((list[i].row, *field, text));
                }
                x += width;
            }
        }
        let clicked = sheet::buttons(
            c,
            "wbs_milestones_buttons",
            11.,
            h - 26.,
            &[("Add", id.is_some()), ("Delete", selected.is_some())],
        );
        let up = c.button(
            "wbs_milestones_up",
            189.,
            h - 26.,
            52.,
            21.,
            "Up",
            false,
            selected.is_some_and(|i| i > 0),
        );
        let down = c.button(
            "wbs_milestones_down",
            246.,
            h - 26.,
            48.,
            21.,
            "Down",
            false,
            selected.is_some_and(|i| i + 1 < list.len()),
        );
        let mut changed = false;
        if let Some((row, field, text)) = edit {
            match budget::set_milestone(&mut self.project.document, row, field, &text) {
                Ok(()) => changed = true,
                Err(e) => self.status = e,
            }
        }
        if let (Some(id), Some(i)) = (id, selected) {
            if up || down {
                if let Some(n) = budget::move_milestone(&mut self.project.document, pid, id, i, up)
                {
                    self.views.budget.step_row = Some(n);
                    changed = true;
                }
            }
            if clicked == Some(1) {
                budget::delete_milestone(&mut self.project.document, list[i].row);
                self.views.budget.step_row = Some(i.saturating_sub(1));
                changed = true;
            }
        }
        if let (Some(0), Some(id)) = (clicked, id) {
            let row = budget::add_milestone(&mut self.project.document, pid, id);
            let now = budget::milestones(&self.project.document, pid, id);
            self.views.budget.step_row = now.iter().position(|m| m.row == row);
            changed = true;
        }
        if changed {
            self.touch_records();
        }
    }
}

/// Project name: the project node's WBS name when present, else the Project ID.
pub fn project_name(p: &ProjectData) -> String {
    project_node(p)
        .and_then(|id| p.wbs.iter().find(|w| w.id == id))
        .map(|w| w.name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| p.model.project.name.clone())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn codes(p: &ProjectData, parent: Option<i64>) -> Vec<String> {
        let tree = Tree::build(p);
        tree.kids(parent)
            .iter()
            .map(|&id| code_of(p, id).to_string())
            .collect()
    }

    /// The starter project with a project node above its four WBS elements,
    /// the way the reference scheduler exports one.
    pub(crate) fn with_project_node() -> ProjectData {
        let mut doc = openxer_xer::starter().document;
        let table = doc.table_mut("PROJWBS");
        for i in 0..table.rows.len() {
            table.set(i, "parent_wbs_id", "5".into());
            table.set(i, "proj_node_flag", "N".into());
        }
        table.add(&[
            ("wbs_id", "5".into()),
            ("proj_id", "1".into()),
            ("wbs_short_name", "RIVERBEND".into()),
            ("wbs_name", "Riverbend Plant".into()),
            ("proj_node_flag", "Y".into()),
            ("parent_wbs_id", "900".into()),
            ("seq_num", "0".into()),
        ]);
        ProjectData::from_document(doc, 1, "node".into()).unwrap()
    }

    fn assert_consistent(p: &ProjectData) {
        let tree = Tree::build(p);
        let order = tree.preorder();
        assert_eq!(order.len(), p.wbs.len(), "every element is in the tree");
        let seqs: Vec<i64> = order
            .iter()
            .map(|(id, _)| p.wbs.iter().find(|w| w.id == *id).unwrap().sequence)
            .collect();
        assert!(
            seqs.windows(2).all(|w| w[0] < w[1]),
            "pre-order sequences {seqs:?}"
        );
        assert_eq!(
            p.wbs.iter().map(|w| w.id).collect::<Vec<_>>(),
            order.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            "the flat list follows the tree"
        );
        for (parent, kids) in &tree.children {
            let mut seen = BTreeSet::new();
            for &k in kids {
                assert!(
                    seen.insert(code_of(p, k).to_lowercase()),
                    "duplicate code under {parent:?}"
                );
            }
        }
        let table = p.document.table("PROJWBS").unwrap();
        for w in &p.wbs {
            let row = table
                .rows
                .iter()
                .find(|r| num(table.value(r, "wbs_id")) == w.id)
                .expect("PROJWBS row");
            assert_eq!(table.value(row, "wbs_short_name"), w.code);
            assert_eq!(table.value(row, "wbs_name"), w.name);
            assert_eq!(num(table.value(row, "seq_num")), w.sequence);
            assert_eq!(
                table.value(row, "parent_wbs_id"),
                w.parent.map_or(String::new(), |x| x.to_string())
            );
        }
        let rows = table
            .rows
            .iter()
            .filter(|r| num(table.value(r, "proj_id")) == p.model.project.proj_id)
            .count();
        assert_eq!(rows, p.wbs.len());
    }

    fn reopen(p: &ProjectData) -> ProjectData {
        let text = p.updated_document().text();
        ProjectData::from_document(Document::parse(&text).unwrap(), 1, "rt".into()).unwrap()
    }

    #[test]
    fn shift_up_down_reorder_siblings_and_renumber() {
        let mut p = openxer_xer::starter();
        assert_eq!(codes(&p, None), ["RB.1", "RB.2", "RB.3", "RB.4"]);
        shift_down(&mut p, 10).unwrap();
        assert_eq!(codes(&p, None), ["RB.2", "RB.1", "RB.3", "RB.4"]);
        shift_up(&mut p, 40).unwrap();
        assert_eq!(codes(&p, None), ["RB.2", "RB.1", "RB.4", "RB.3"]);
        assert!(shift_up(&mut p, 20).unwrap_err().contains("already first"));
        assert!(shift_down(&mut p, 30).unwrap_err().contains("already last"));
        assert_consistent(&p);
        let q = reopen(&p);
        assert_eq!(codes(&q, None), ["RB.2", "RB.1", "RB.4", "RB.3"]);
    }

    #[test]
    fn shift_right_then_left_reparents_and_keeps_activities() {
        let mut p = openxer_xer::starter();
        let before = p.details.clone();
        assert!(shift_right(&mut p, 10).unwrap_err().contains("above"));
        shift_right(&mut p, 30).unwrap();
        assert_eq!(codes(&p, None), ["RB.1", "RB.2", "RB.4"]);
        assert_eq!(codes(&p, Some(20)), ["RB.3"]);
        shift_right(&mut p, 40).unwrap();
        assert_eq!(codes(&p, Some(20)), ["RB.3", "RB.4"]);
        shift_right(&mut p, 40).unwrap();
        assert_eq!(codes(&p, Some(30)), ["RB.4"]);
        let tree = Tree::build(&p);
        assert_eq!(full_code(&p, &tree, 40), "RIVERBEND.RB.2.RB.3.RB.4");
        assert_consistent(&p);
        shift_left(&mut p, 40).unwrap();
        assert_eq!(codes(&p, Some(20)), ["RB.3", "RB.4"]);
        shift_left(&mut p, 30).unwrap();
        assert_eq!(codes(&p, None), ["RB.1", "RB.2", "RB.3"]);
        assert!(shift_left(&mut p, 10).unwrap_err().contains("top level"));
        assert_eq!(p.details, before, "activities keep their WBS ids");
        assert_consistent(&p);
        let q = reopen(&p);
        assert_eq!(codes(&q, Some(20)), ["RB.4"]);
        assert_eq!(
            q.wbs.iter().map(|w| w.id).collect::<Vec<_>>(),
            [10, 20, 40, 30]
        );
    }

    #[test]
    fn shifts_refuse_code_collisions_and_the_project_node() {
        let mut p = with_project_node();
        assert_eq!(project_node(&p), Some(5));
        assert!(shift_left(&mut p, 10)
            .unwrap_err()
            .contains("outside the project"));
        assert!(shift_up(&mut p, 5).unwrap_err().contains("project node"));
        rename_code(&mut p, 30, "X").unwrap();
        rename_code(&mut p, 40, "X").unwrap_err();
        shift_right(&mut p, 30).unwrap();
        rename_code(&mut p, 40, "x").unwrap();
        assert!(shift_right(&mut p, 40)
            .unwrap_err()
            .contains("already used"));
        assert_consistent(&p);
    }

    #[test]
    fn add_places_new_elements_with_unique_codes() {
        let mut p = with_project_node();
        let a = add(&mut p, Some(20)).unwrap();
        assert_eq!(codes(&p, Some(5)), ["RB.1", "RB.2", "RB.5", "RB.3", "RB.4"]);
        let b = add(&mut p, Some(5)).unwrap();
        assert_eq!(code_of(&p, b), "RB.6");
        assert!(a > 40 && b > a);
        assert_eq!(p.wbs.iter().find(|w| w.id == a).unwrap().name, "(New WBS)");
        assert_consistent(&p);
        let q = reopen(&p);
        assert_eq!(codes(&q, Some(5)), codes(&p, Some(5)));
    }

    #[test]
    fn delete_merge_moves_activities_and_children_to_the_parent() {
        let mut p = with_project_node();
        shift_right(&mut p, 30).unwrap();
        shift_right(&mut p, 40).unwrap();
        shift_right(&mut p, 40).unwrap();
        // RB.2 > RB.3 > RB.4 under the project node.
        let in_30 = p.details.values().filter(|d| d.wbs == 30).count();
        let in_20 = p.details.values().filter(|d| d.wbs == 20).count();
        let plan = plan_delete(&p, 30).unwrap();
        assert_eq!((plan.activities, plan.children), (in_30, 1));
        delete_merge(&mut p, 30).unwrap();
        assert_eq!(codes(&p, Some(20)), ["RB.4"]);
        assert_eq!(
            p.details.values().filter(|d| d.wbs == 20).count(),
            in_20 + in_30
        );
        assert!(p.details.values().all(|d| d.wbs != 30));
        assert_consistent(&p);
        // An empty leaf is simply removed.
        let empty = add(&mut p, Some(10)).unwrap();
        delete_merge(&mut p, empty).unwrap();
        assert!(p.wbs.iter().all(|w| w.id != empty));
        assert!(plan_delete(&p, 5).is_err());
        let q = reopen(&p);
        assert!(q.wbs.iter().all(|w| w.id != 30));
        assert!(q.details.values().all(|d| d.wbs != 30));
        assert_eq!(
            q.details.values().filter(|d| d.wbs == 20).count(),
            in_20 + in_30
        );
    }

    #[test]
    fn delete_with_activities_removes_the_subtree_and_its_activity_rows() {
        let mut p = openxer_xer::starter();
        shift_right(&mut p, 30).unwrap();
        let tasks: Vec<i64> = subtree_activities(&p, 20).iter().map(|t| t.0).collect();
        assert!(tasks.len() > 1, "RB.2 and RB.3 hold activities");
        let links_before = p.model.links.len();
        let n = delete_with_activities(&mut p, 20).unwrap();
        assert_eq!(n, tasks.len());
        assert!(p.wbs.iter().all(|w| w.id != 20 && w.id != 30));
        assert!(p
            .model
            .activities
            .iter()
            .all(|a| !tasks.contains(&a.task_id)));
        assert!(
            p.model.links.len() < links_before,
            "their relationships go too"
        );
        assert_consistent(&p);
        let q = reopen(&p);
        assert!(q.wbs.iter().all(|w| w.id != 20 && w.id != 30));
        assert_eq!(q.model.activities.len(), p.model.activities.len());
        let text = p.updated_document().text();
        let doc = Document::parse(&text).unwrap();
        let task = doc.table("TASK").unwrap();
        assert!(task
            .rows
            .iter()
            .all(|r| !tasks.contains(&num(task.value(r, "task_id")))));
        assert!(delete_with_activities(&mut with_project_node(), 5).is_err());
    }

    #[test]
    fn delete_all_confirms_with_the_activity_list_and_undoes_in_one_step() {
        let mut d = crate::qa_sweep::Driver::new("wbs-delete-all", openxer_xer::starter());
        d.click("directory_wbs");
        let before = d.app.project.clone();
        let undo = d.app.history.counts().0;
        // A top-level element has no parent to merge into: Delete goes
        // straight to the Delete All confirmation, which lists the activities.
        d.app.views.wbs.selected = Some(10);
        d.click("dock_delete");
        let pending = d.app.views.wbs.pending.clone().unwrap();
        assert!(pending.delete_all);
        let listed = subtree_activities(&d.app.project, 10);
        assert!(!listed.is_empty());
        assert!(
            d.hover_text(&listed[0].1),
            "the confirmation lists {}",
            listed[0].1
        );
        d.key(Key::Escape);
        assert!(d.app.views.wbs.pending.is_none());
        assert!(
            d.app.project.model == before.model,
            "Escape leaves the project"
        );
        // A child element: Merge or Delete All, then the listing.
        d.app.project = before.clone();
        shift_right(&mut d.app.project, 30).unwrap();
        d.app.touch_records();
        d.frames(2);
        let undo = d.app.history.counts().0.max(undo);
        let shifted = d.app.project.clone();
        d.app.views.wbs.selected = Some(30);
        d.click("dock_delete");
        assert!(!d.app.views.wbs.pending.as_ref().unwrap().delete_all);
        d.click("wbs_confirm_delete_all");
        assert!(d.app.views.wbs.pending.as_ref().unwrap().delete_all);
        let gone = subtree_activities(&d.app.project, 30).len();
        d.click("wbs_confirm_delete_all_yes");
        assert!(d.app.views.wbs.pending.is_none());
        assert_eq!(
            d.app.project.model.activities.len(),
            shifted.model.activities.len() - gone
        );
        assert!(d.app.project.wbs.iter().all(|w| w.id != 30));
        assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");
        let ctx = d.ctx.clone();
        d.app.history_move(&ctx, false);
        assert!(d.app.project.model == shifted.model);
        assert_eq!(d.app.project.wbs, shifted.wbs);
        assert_eq!(
            d.app.project.updated_document().text(),
            shifted.updated_document().text()
        );
    }

    #[test]
    fn delete_without_parent_is_refused_when_it_would_lose_activities() {
        let mut p = openxer_xer::starter();
        let err = plan_delete(&p, 10).unwrap_err();
        assert!(err.contains("no parent"), "{err}");
        let empty = add(&mut p, Some(40)).unwrap();
        delete_merge(&mut p, empty).unwrap();
        assert_consistent(&p);
    }

    #[test]
    fn cut_paste_moves_and_copy_paste_duplicates_structure() {
        let mut p = with_project_node();
        shift_right(&mut p, 40).unwrap(); // RB.4 under RB.3
        let moved = paste(
            &mut p,
            &Clip {
                root: 30,
                cut: true,
            },
            Some(10),
        )
        .unwrap();
        assert_eq!(moved, 30);
        assert_eq!(codes(&p, Some(10)), ["RB.3"]);
        assert_eq!(codes(&p, Some(30)), ["RB.4"]);
        assert!(paste(
            &mut p,
            &Clip {
                root: 10,
                cut: true
            },
            Some(40)
        )
        .unwrap_err()
        .contains("inside itself"));
        let activities = p.details.clone();
        let copy = paste(
            &mut p,
            &Clip {
                root: 30,
                cut: false,
            },
            Some(10),
        )
        .unwrap();
        assert_eq!(
            code_of(&p, copy),
            "RB.4",
            "trailing digits increment to a free code"
        );
        assert_eq!(codes(&p, Some(copy)), ["RB.4"]);
        assert_eq!(p.details, activities, "copy does not duplicate activities");
        assert_consistent(&p);
        let q = reopen(&p);
        assert_eq!(codes(&q, Some(copy)), ["RB.4"]);
    }

    #[test]
    fn rollups_cover_descendants_and_full_codes_use_the_separator() {
        let mut p = openxer_xer::starter();
        scheduler::schedule_with_resources(&mut p.model, &p.resources).unwrap();
        shift_right(&mut p, 20).unwrap();
        let tree = Tree::build(&p);
        let r = rollups(&p, &tree);
        let own = |w: i64| p.details.values().filter(|d| d.wbs == w).count();
        assert_eq!(r[&Some(10)].activities, own(10) + own(20));
        assert_eq!(r[&None].activities, p.model.activities.len());
        let finish_20 = p
            .model
            .activities
            .iter()
            .filter(|a| p.details[&a.task_id].wbs == 20)
            .filter_map(|a| displayed_finish(a, false))
            .max();
        assert!(r[&Some(10)].finish >= finish_20);
        assert_eq!(
            r[&Some(40)].total_float,
            Some(0),
            "commissioning is critical"
        );
        let table = p.document.table_mut("PROJECT");
        table.set(0, "name_sep_char", "/".into());
        assert_eq!(full_code(&p, &tree, 20), "RIVERBEND/RB.1/RB.2");
    }

    #[test]
    fn deep_wbs_full_codes_stay_linear_and_keep_the_tail() {
        // A 3,000 level chain: unbounded codes would total about 30 MB.
        let mut p = openxer_xer::starter();
        let depth = 3000;
        let mut parent = Some(40);
        for i in 0..depth {
            let id = 1000 + i;
            p.wbs.push(Wbs {
                id,
                parent,
                code: format!("L{i}"),
                name: format!("Level {i}"),
                sequence: id,
            });
            parent = Some(id);
        }
        let tree = Tree::build(&p);
        let started = std::time::Instant::now();
        let codes = full_codes(&p, &tree);
        assert!(started.elapsed().as_secs_f64() < 2.);
        assert_eq!(codes.len(), p.wbs.len());
        let total: usize = codes.values().map(String::len).sum();
        let longest = codes.values().map(String::len).max().unwrap();
        assert!(longest <= MAX_FULL_CODE + 16, "{longest}");
        assert!(total <= codes.len() * (MAX_FULL_CODE + 16), "{total}");
        // Shallow codes are complete; deep ones keep their own short code.
        assert_eq!(codes[&40], "RIVERBEND.RB.4");
        assert_eq!(codes[&1000], "RIVERBEND.RB.4.L0");
        let last = &codes[&(1000 + depth - 1)];
        assert!(last.ends_with(&format!(".L{}", depth - 1)), "{last}");
        // The activity grid's WBS column uses the same codes.
        let ix = crate::grid::columns::Index::new((false, 0.));
        assert_eq!(ix.wbs(&p).codes, codes);
    }

    #[test]
    fn memo_text_strips_markup() {
        assert_eq!(
            memo_text("<HTML><BODY><p>Line&nbsp;one</p>\u{7f}\u{7f}two &amp; three</BODY></HTML>"),
            "Line one\n\ntwo & three"
        );
    }
}
