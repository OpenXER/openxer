//! Resources view: the resource dictionary as a tree table from RSRC and
//! RSRCRATE, the Resource Details tabs, and the per-period resource usage
//! (`usage_in`) the usage views draw from. Show on Bottom shows
//! or hides the details under the table; the Resource Usage Profile is
//! offered only in the Activities window's bottom pane.
//!
//! Every tab edits the file's rows (`resource_edit`): General, Details
//! (type, unit of measure, currency, overtime, calendar, default units/time,
//! the two flags), Units & Prices (shift, effective-dated max units and
//! prices), Codes, Roles and Notes. Calendars, max units and prices are
//! leveling inputs: scheduling rebuilds them from these rows before every F9
//! or Level, and Recalculate Assignment Costs reads the prices.
use super::grid::{self, Columns, Row};
use super::resource_edit::{self as edit, ResourceClip};
use super::sheet;
use super::*;
use crate::context_menu::{Entry, Spec};
use crate::panes::usage::SheetSort;
use crate::panes::usage_fields::{self as fields, Which};
use crate::panes::usage_list::{self as ul, ListFilter, ListGroup, ListKey, ListOwner, ListState};
use crate::schedule_dialogs::Canvas;
use openxer_xer::parse_absolute;
use std::collections::{BTreeSet, HashMap};

/// The table's columns in a new layout: fields of the shared resource field
/// catalog (`usage_fields`) and their widths. Layouts saved before the
/// columns could be chosen list these six by position.
pub const DEFAULT_COLUMNS: [(&str, f32); 6] = [
    ("resource_id", 150.),
    ("resource_name", 200.),
    ("resource_type", 100.),
    ("unit", 100.),
    ("primary_role", 150.),
    ("max_units", 95.),
];
pub const TABS: [&str; 7] = [
    "General",
    "Codes",
    "Details",
    "Units & Prices",
    "Roles",
    "Notes",
    "User Defined Fields",
];
/// The resource details open on Units & Prices.
pub const DEFAULT_TAB: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceType {
    Labor,
    Nonlabor,
    Material,
}

impl ResourceType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Labor => "Labor",
            Self::Nonlabor => "Nonlabor",
            Self::Material => "Material",
        }
    }
    /// The icon of the type in the resource tree.
    pub fn icon(self) -> chrome::Icon {
        match self {
            Self::Labor => chrome::Icon::Labor,
            Self::Nonlabor => chrome::Icon::Nonlabor,
            Self::Material => chrome::Icon::Material,
        }
    }
    fn from_code(code: &str) -> Self {
        match code {
            "RT_Equip" => Self::Nonlabor,
            "RT_Mat" => Self::Material,
            _ => Self::Labor,
        }
    }
}

/// One RSRC row with its effective rate at the project data date.
#[derive(Clone, Debug, PartialEq)]
pub struct Resource {
    pub id: i64,
    pub parent: Option<i64>,
    pub code: String,
    pub name: String,
    pub kind: ResourceType,
    pub unit: String,
    pub default_units: f64,
    pub max_units: Option<f64>,
    pub price: Option<f64>,
    pub calendar: String,
    pub active: bool,
    pub sequence: i64,
    pub assignments: usize,
    /// Primary role id (`RSRC.role_id`).
    pub role: String,
    /// The other fields the resource table can show.
    pub info: ResourceInfo,
}

/// RSRC fields shown only as table columns, as text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResourceInfo {
    pub currency: String,
    pub overtime: bool,
    pub overtime_factor: String,
    pub auto_compute: bool,
    pub cost_from_units: bool,
    pub employee_id: String,
    pub title: String,
    pub email: String,
    pub office_phone: String,
    pub location: String,
    /// The parent's Resource ID.
    pub parent_code: String,
}

fn num(s: &str) -> i64 {
    s.trim().parse().unwrap_or(0)
}

fn float(s: &str) -> f64 {
    s.trim().parse().unwrap_or(0.)
}

// Rate rows and the rate in effect at a date live in `openxer_xer::limits`,
// with the limit per period the profile draws and agents read.
pub use openxer_xer::limits::rate_at;
#[cfg(test)]
pub use openxer_xer::limits::rates;

pub fn resources(p: &ProjectData) -> Vec<Resource> {
    let Some(t) = p.document.table("RSRC") else {
        return vec![];
    };
    let units: BTreeMap<String, String> =
        p.document.table("UMEASURE").map_or(BTreeMap::new(), |u| {
            u.rows
                .iter()
                .map(|r| {
                    (
                        u.value(r, "unit_id").to_string(),
                        u.value(r, "unit_abbrev").to_string(),
                    )
                })
                .collect()
        });
    let pid = p.model.project.proj_id;
    let mut counts: BTreeMap<i64, usize> = BTreeMap::new();
    if let Some(a) = p.document.table("TASKRSRC") {
        for r in a.rows.iter().filter(|r| num(a.value(r, "proj_id")) == pid) {
            *counts.entry(num(a.value(r, "rsrc_id"))).or_default() += 1;
        }
    }
    let data_date = p
        .model
        .project
        .data_date
        .map_or(0, |d| d + p.model.origin_serial * DAY);
    let names = |table: &str, key: &str, field: &str| -> BTreeMap<String, String> {
        p.document.table(table).map_or(BTreeMap::new(), |u| {
            u.rows
                .iter()
                .map(|r| (u.value(r, key).to_string(), u.value(r, field).to_string()))
                .collect()
        })
    };
    // Every resource's price and limit rows, read once.
    let mut rate_index: HashMap<i64, Vec<(Option<i64>, f64, f64)>> = HashMap::new();
    if let Some(rt) = p.document.table("RSRCRATE") {
        for r in &rt.rows {
            rate_index
                .entry(num(rt.value(r, "rsrc_id")))
                .or_default()
                .push((
                    parse_absolute(rt.value(r, "start_date")),
                    float(rt.value(r, "max_qty_per_hr")),
                    float(rt.value(r, "cost_per_qty")),
                ));
        }
    }
    for list in rate_index.values_mut() {
        list.sort_by_key(|r| r.0);
    }
    let calendars: HashMap<&str, String> = p
        .model
        .calendars
        .iter()
        .enumerate()
        .rev()
        .map(|(i, c)| {
            (
                c.id.as_str(),
                p.calendar_names.get(i).cloned().unwrap_or_default(),
            )
        })
        .collect();
    let currencies = names("CURRTYPE", "curr_id", "curr_type");
    let locations = names("LOCATION", "location_id", "location_name");
    let codes = names("RSRC", "rsrc_id", "rsrc_short_name");
    let text = |r: &Vec<String>, field: &str| t.value(r, field).to_string();
    let mut out: Vec<Resource> = t
        .rows
        .iter()
        .map(|r| {
            let id = num(t.value(r, "rsrc_id"));
            let rate = rate_at(
                rate_index.get(&id).map_or(&[][..], Vec::as_slice),
                data_date,
            );
            let lookup = |map: &BTreeMap<String, String>, field: &str| {
                map.get(t.value(r, field)).cloned().unwrap_or_default()
            };
            let info = ResourceInfo {
                currency: lookup(&currencies, "curr_id"),
                overtime: t.value(r, "ot_flag") == "Y",
                overtime_factor: text(r, "ot_factor"),
                auto_compute: t.value(r, "auto_compute_act_flag") == "Y",
                cost_from_units: t.value(r, "def_cost_qty_link_flag") == "Y",
                employee_id: text(r, "employee_code"),
                title: text(r, "rsrc_title_name"),
                email: text(r, "email_addr"),
                office_phone: text(r, "office_phone"),
                location: lookup(&locations, "location_id"),
                parent_code: lookup(&codes, "parent_rsrc_id"),
            };
            Resource {
                id,
                parent: t
                    .value(r, "parent_rsrc_id")
                    .trim()
                    .parse()
                    .ok()
                    .filter(|x| *x != id),
                code: t.value(r, "rsrc_short_name").to_string(),
                name: t.value(r, "rsrc_name").to_string(),
                kind: ResourceType::from_code(t.value(r, "rsrc_type")),
                unit: units
                    .get(t.value(r, "unit_id"))
                    .cloned()
                    .unwrap_or_default(),
                default_units: float(t.value(r, "def_qty_per_hr")),
                max_units: rate.map(|r| r.0),
                price: rate.map(|r| r.1),
                calendar: calendars
                    .get(t.value(r, "clndr_id"))
                    .cloned()
                    .unwrap_or_default(),
                active: t.value(r, "active_flag") != "N",
                sequence: num(t.value(r, "rsrc_seq_num")),
                assignments: counts.get(&id).copied().unwrap_or(0),
                role: t.value(r, "role_id").to_string(),
                info,
            }
        })
        .collect();
    out.sort_by(|a, b| (a.sequence, &a.code).cmp(&(b.sequence, &b.code)));
    out
}

/// Units per period for the profile: actual and remaining (stacked) and the
/// limit line. Periods are model-relative minute ranges.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Usage {
    pub actual: Vec<f64>,
    pub remaining: Vec<f64>,
    pub limit: Vec<f64>,
}

#[cfg(test)]
pub fn usage(p: &ProjectData, rsrc: i64, periods: &[(i64, i64)], only: Option<usize>) -> Usage {
    usage_of(p, rsrc, periods, only, false)
}

/// `usage` in units, or in costs when `costs` is set (actual and remaining
/// cost spread the same way as the units; costs have no limit line).
#[cfg(test)]
pub fn usage_of(
    p: &ProjectData,
    rsrc: i64,
    periods: &[(i64, i64)],
    only: Option<usize>,
    costs: bool,
) -> Usage {
    let list = super::assignments::assignments(p);
    usage_in(
        p,
        rsrc,
        list.iter()
            .filter(|a| a.rsrc == rsrc && only.is_none_or(|o| o == a.task)),
        periods,
        costs,
    )
}

/// `usage_of` over `list`, the resource's assignments of `p` (taken from a
/// list built once for the project, `assignments::by_resource`, so drawing
/// many resources does not rebuild every assignment for each).
pub fn usage_in<'a>(
    p: &ProjectData,
    rsrc: i64,
    list: impl IntoIterator<Item = &'a super::assignments::Assignment>,
    periods: &[(i64, i64)],
    costs: bool,
) -> Usage {
    use super::assignments as asg;
    let m = &p.model;
    let n = periods.len();
    let mut u = Usage {
        actual: vec![0.; n],
        remaining: vec![0.; n],
        limit: vec![0.; n],
    };
    for a in list {
        let costed;
        let a = if costs {
            costed = a.as_costs();
            &costed
        } else {
            a
        };
        let activity = &m.activities[a.task];
        if activity.act_start.is_some() {
            asg::actual_into(p, a, periods, &mut u.actual);
        }
        if !activity.is_completed() {
            asg::remaining_into(p, a, periods, &mut u.remaining);
        }
    }
    if costs {
        return u;
    }
    let Some(limits) = openxer_xer::limits::per_period(p, rsrc, periods) else {
        // No price and limit row: no limit, as leveling reads it (a row
        // with max units 0 is a real limit of zero there, and here).
        u.limit = vec![f64::INFINITY; n];
        return u;
    };
    for (i, period) in periods.iter().enumerate() {
        // The usage views' edge periods have no limit to draw.
        if !asg::is_edge(period) {
            u.limit[i] = limits[i];
        }
    }
    u
}

/// A list window opened from a picker or an Assign button of the details.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Unit,
    Currency,
    Calendar,
    Role,
    Code,
}

/// What the Resources right-click menu reads of the view, as plain data.
#[derive(Clone, Copy, Debug, Default)]
pub struct ResourcesMenu {
    /// A resource is selected (Cut, Copy and Delete apply).
    pub selected: bool,
    /// Something was cut or copied.
    pub clipboard: bool,
    /// Resource Details is shown on the bottom.
    pub details: bool,
    /// Filter By and Group and Sort By as in effect.
    pub filter: ListFilter,
    pub group: ListGroup,
    /// A column header sorts the table.
    pub sorted: bool,
    /// Some rows can be expanded or collapsed.
    pub expandable: bool,
    /// Several resources are selected: Fill Down applies.
    pub fill: bool,
}

/// The Group and Sort By choices with a menu key of their own.
const GROUPS: [(&str, ListGroup); 4] = [
    ("group_hierarchy", ListGroup::Hierarchy),
    ("group_list", ListGroup::List),
    ("group_role", ListGroup::Role),
    ("group_type", ListGroup::Type),
];

/// The Filter By choices (the Display bar and the right-click menu).
fn filter_entries(m: &ResourcesMenu) -> Vec<Entry> {
    vec![
        Entry::Item(
            Spec::new(
                "display_all",
                ListFilter::All.caption(ul::ListView::Resource),
            )
            .checked(m.filter == ListFilter::All),
        ),
        Entry::Item(
            Spec::new(
                "display_project",
                ListFilter::Project.caption(ul::ListView::Resource),
            )
            .checked(m.filter == ListFilter::Project),
        ),
        Entry::Separator,
        Entry::Item(
            Spec::new("filter_custom", "Customize...").checked(m.filter == ListFilter::Custom),
        ),
    ]
}

/// The Group and Sort By choices, Customize... first as in the reference
/// (the Display bar and the right-click menu).
fn group_entries(m: &ResourcesMenu) -> Vec<Entry> {
    let mut group = vec![Entry::Item(
        Spec::new("group_custom", "Customize...").checked(m.group == ListGroup::Custom),
    )];
    group.extend(
        GROUPS
            .iter()
            .map(|&(key, g)| Entry::Item(Spec::new(key, g.caption()).checked(m.group == g))),
    );
    group.extend([
        Entry::Separator,
        Entry::Item(Spec::new("sort_clear", "Clear Sort").enabled(m.sorted)),
    ]);
    group
}

/// Filter By and Group and Sort By as two submenus.
fn filter_group_entries(m: &ResourcesMenu) -> [Entry; 2] {
    use chrome::Icon;
    [
        Entry::Submenu(
            Spec::new("filter_by", "Filter By").icon(Icon::Filter),
            filter_entries(m),
        ),
        Entry::Submenu(
            Spec::new("group_by", "Group and Sort By").icon(Icon::GroupSort),
            group_entries(m),
        ),
    ]
}

/// Expand All and Collapse All of the resource table.
fn expand_entries(m: &ResourcesMenu, shortcuts: bool) -> [Entry; 2] {
    use chrome::Icon;
    let key = |text: &'static str| if shortcuts { text } else { "" };
    [
        Entry::Item(
            Spec::new("expand_all", "Expand All")
                .key(key("Ctrl+Num +"))
                .icon(Icon::ExpandAll)
                .enabled(m.expandable),
        ),
        Entry::Item(
            Spec::new("collapse_all", "Collapse All")
                .key(key("Ctrl+Num -"))
                .icon(Icon::CollapseAll)
                .enabled(m.expandable),
        ),
    ]
}

/// Filter By, Group and Sort By, Columns..., Expand All and Collapse All:
/// the Display bar's menu. Labels and choices are the bottom resource
/// lists'.
pub fn display_entries(m: &ResourcesMenu) -> Vec<Entry> {
    let mut out = filter_group_entries(m).to_vec();
    out.push(Entry::Item(
        Spec::new("columns", "Columns...").icon(chrome::Icon::Columns),
    ));
    out.push(Entry::Separator);
    out.extend(expand_entries(m, false));
    out
}

/// The Resources table menu (a row, the empty area, a column title), in
/// the reference's order. Fill Down is listed greyed, as in the reference.
/// Delete or Merge... is OpenXER's own, after Delete.
pub fn resources_menu(m: &ResourcesMenu) -> Vec<Entry> {
    use chrome::Icon;
    let item = |key, label, shortcut, icon: Icon, enabled| {
        Entry::Item(
            Spec::new(key, label)
                .key(shortcut)
                .icon(icon)
                .enabled(enabled),
        )
    };
    let mut out = vec![
        Entry::Item(
            Spec::new("details", "Details")
                .icon(Icon::Details)
                .checked(m.details),
        ),
        Entry::Separator,
        item("cut", "Cut", "Ctrl+X", Icon::Cut, m.selected),
        item("copy", "Copy", "Ctrl+C", Icon::Copy, m.selected),
        item("paste", "Paste", "Ctrl+V", Icon::Paste, m.clipboard),
        item("fill_down", "Fill Down", "Ctrl+E", Icon::FillDown, m.fill),
        Entry::Separator,
        item("add", "Add", "Ins", Icon::Add, true),
        item("delete", "Delete", "Del", Icon::Delete, m.selected),
        item(
            "del_merge",
            "Delete or Merge...",
            "",
            Icon::Delete,
            m.selected,
        ),
        Entry::Separator,
        Entry::Submenu(
            Spec::new("columns_menu", "Columns").icon(Icon::Columns),
            vec![Entry::Item(
                Spec::new("columns", "Customize...").icon(Icon::Columns),
            )],
        ),
        Entry::Separator,
    ];
    out.extend(filter_group_entries(m));
    out.push(Entry::Separator);
    out.extend(expand_entries(m, true));
    out
}

/// Show on Bottom of the Resources window: no bottom pane or Resource
/// Details. Each entry is `(key, caption, details shown)`.
pub const BOTTOM_CHOICES: [(&str, &str, bool); 2] = [
    ("bottom_none", "No Bottom Layout", false),
    ("bottom_details", "Resource Details", true),
];

fn bottom_icon(details: bool) -> Option<chrome::Icon> {
    details.then_some(chrome::Icon::Details)
}

pub struct ResourcesState {
    /// Resources copied, in table order: Paste adds a copy of each.
    pub clipboard: Vec<ResourceClip>,
    /// Resources marked by Cut: Paste moves them beside the selected one.
    pub cut: Vec<i64>,
    /// Selected rows of the Units & Prices, Roles and Codes grids.
    pub rate_row: Option<usize>,
    pub role_row: Option<usize>,
    pub code_row: Option<usize>,
    /// Open list window and the resource it edits.
    pub pick: Option<(Pick, i64)>,
    /// Notes being edited: resource and text.
    pub notes_draft: Option<(i64, String)>,
    /// The table's columns: resource field ids in display order, and
    /// `columns` holding their widths in the same order.
    pub fields: Vec<String>,
    pub columns: Columns,
    /// Widths of fields chosen before, so a column added back keeps its
    /// width.
    pub widths: BTreeMap<String, f32>,
    /// The current resource: the details follow it.
    pub selected: Option<i64>,
    /// Resources selected along with the current one (Ctrl, Shift, Ctrl+A).
    pub rows: grid::RowSelection<i64>,
    /// Filter By between All Resources (false) and Current Project's
    /// Resources (true); `list.filter` set to Custom overrides it.
    pub assigned_only: bool,
    /// The list choices shared with the bottom resource lists: the custom
    /// filter (and whether it is on), the code levels, the header sort and
    /// the collapsed rows. Filter and group are read through `filter()` and
    /// `group()`.
    pub list: ListState,
    /// Group and Sort By as chosen; `None` follows the filter (Current
    /// Project's Resources: List, else Hierarchy).
    pub group: Option<ListGroup>,
    pub tab: usize,
    /// Resource Details is shown under the table.
    pub show_bottom: bool,
    pub details_height: f32,
    /// Width of the Details tab's left groups as last laid out.
    pub details_left: f32,
    /// Columns of the Units & Prices rate grid chosen in Customize Resource
    /// Rate Columns (titles in order); empty shows the default set.
    pub rate_columns: Vec<String>,
    /// The open Del. / Merge dialog.
    pub merge: Option<super::resource_merge::MergeDialog>,
}

/// Every column the rate grid can show: title, width, right aligned and the
/// RSRCRATE column it shows (`None`: the effective date).
pub const RATE_COLUMNS: [(&str, f32, bool, Option<&str>); 7] = [
    ("Effective Date", 150., false, None),
    ("Max Units / Time", 150., true, Some("max_qty_per_hr")),
    ("Price / Unit", 105., true, Some("cost_per_qty")),
    ("Price / Unit 2", 105., true, Some("cost_per_qty2")),
    ("Price / Unit 3", 105., true, Some("cost_per_qty3")),
    ("Price / Unit 4", 105., true, Some("cost_per_qty4")),
    ("Price / Unit 5", 105., true, Some("cost_per_qty5")),
];
/// The rate grid's own columns.
pub const RATE_DEFAULT: [&str; 3] = ["Effective Date", "Max Units / Time", "Price / Unit"];

impl ResourcesState {
    /// The selected resources in table order; just the current one when
    /// nothing else is selected.
    pub fn picked(&self) -> Vec<i64> {
        self.rows.picked(self.selected.as_ref())
    }

    /// The rate grid's columns, in order: the chosen ones, else the default.
    pub fn rate_columns(&self) -> Vec<(&'static str, f32, bool, Option<&'static str>)> {
        let titles: Vec<&str> = if self.rate_columns.is_empty() {
            RATE_DEFAULT.to_vec()
        } else {
            self.rate_columns.iter().map(String::as_str).collect()
        };
        titles
            .iter()
            .filter_map(|t| RATE_COLUMNS.iter().find(|c| c.0 == *t).copied())
            .collect()
    }
}

impl Default for ResourcesState {
    fn default() -> Self {
        Self {
            clipboard: vec![],
            cut: vec![],
            rate_row: None,
            role_row: None,
            code_row: None,
            pick: None,
            notes_draft: None,
            fields: DEFAULT_COLUMNS.iter().map(|c| c.0.to_string()).collect(),
            columns: Columns::new(&DEFAULT_COLUMNS.map(|c| c.1)),
            widths: BTreeMap::new(),
            selected: None,
            rows: Default::default(),
            assigned_only: true,
            list: ListState::default(),
            group: None,
            tab: DEFAULT_TAB,
            show_bottom: true,
            details_height: 530.,
            details_left: 600.,
            rate_columns: vec![],
            merge: None,
        }
    }
}

impl ResourcesState {
    pub fn reset(&mut self) {
        self.selected = None;
        self.rows = Default::default();
        self.cut.clear();
        self.list.collapsed.clear();
        self.pick = None;
        self.notes_draft = None;
        self.rate_row = None;
        self.role_row = None;
        self.code_row = None;
        self.merge = None;
    }
    /// Filter By as in effect.
    pub fn filter(&self) -> ListFilter {
        if self.list.filter == ListFilter::Custom {
            ListFilter::Custom
        } else if self.assigned_only {
            ListFilter::Project
        } else {
            ListFilter::All
        }
    }

    /// Group and Sort By as in effect: the choice made, else the default of
    /// the filter (the current project's resources as a flat list by ID,
    /// all resources and a custom filter as the resource hierarchy).
    pub fn group(&self) -> ListGroup {
        self.group.unwrap_or(match self.filter() {
            ListFilter::Project => ListGroup::List,
            _ => ListGroup::Hierarchy,
        })
    }

    /// Filter By: All Resources, Current Project's Resources or (Custom)
    /// the custom filter as last set.
    pub fn set_filter(&mut self, filter: ListFilter) {
        self.list.filter = filter;
        match filter {
            ListFilter::All => self.assigned_only = false,
            ListFilter::Project => self.assigned_only = true,
            ListFilter::Custom => {}
        }
    }

    /// The list choices `usage_list::build` arranges the table by.
    pub fn list_state(&self) -> ListState {
        ListState {
            filter: self.filter(),
            group: self.group(),
            view: ul::ListView::Resource,
            ..self.list.clone()
        }
    }

    /// The width a field's column takes: as last set, else its default.
    fn field_width(&self, id: &str) -> f32 {
        self.widths
            .get(id)
            .copied()
            .or_else(|| DEFAULT_COLUMNS.iter().find(|c| c.0 == id).map(|c| c.1))
            .or_else(|| fields::resource_field(id).map(|f| f.2))
            .unwrap_or(100.)
    }

    /// Keep the widths of the shown columns for later.
    fn remember_widths(&mut self) {
        for (id, w) in self.fields.iter().zip(&self.columns.widths) {
            self.widths.insert(id.clone(), *w);
        }
    }

    /// Show these fields, in order (Columns...). Empty keeps the current set.
    pub fn set_fields(&mut self, ids: Vec<String>) {
        if ids.is_empty() {
            return;
        }
        self.remember_widths();
        self.fields = ids;
        let widths: Vec<f32> = self.fields.iter().map(|f| self.field_width(f)).collect();
        self.columns = Columns::new(&widths);
        if let Some(field) = &self.list.sort.field {
            if !self.fields.contains(field) {
                self.list.sort = SheetSort::default();
            }
        }
    }

    /// After the table is drawn: a column dragged to another place moves
    /// its field there, and resized widths are kept.
    pub fn sync_columns(&mut self) {
        self.remember_widths();
        let identity = self.columns.order.iter().enumerate().all(|(i, c)| i == *c);
        if !identity && self.columns.order.len() == self.fields.len() {
            let ids = self
                .columns
                .order
                .iter()
                .filter_map(|c| self.fields.get(*c).cloned())
                .collect();
            self.set_fields(ids);
        }
    }

    pub fn json(&self) -> serde_json::Value {
        let mut widths = self.widths.clone();
        for (id, w) in self.fields.iter().zip(&self.columns.widths) {
            widths.insert(id.clone(), *w);
        }
        serde_json::json!({"fields": self.fields, "field_widths": widths,
            "group": self.group.map(|g| g.key()), "list": self.list.json(),
            "tab": self.tab,
            "bottom": "details",
            "show_bottom": self.show_bottom, "details_height": self.details_height,
            "assigned_only": self.assigned_only,
            "rate_columns": self.rate_columns})
    }
    pub fn restore(&mut self, value: &serde_json::Value) {
        self.restore_columns(value);
        if !value["group"].is_null() {
            self.group = value["group"].as_str().and_then(ListGroup::from_key);
        }
        self.list.restore(&value["list"]);
        self.list.view = ul::ListView::Resource;
        if let Some(tab) = value["tab"].as_u64().filter(|t| (*t as usize) < TABS.len()) {
            self.tab = tab as usize;
        }
        for (key, field) in [
            ("show_bottom", &mut self.show_bottom),
            ("assigned_only", &mut self.assigned_only),
        ] {
            if let Some(saved) = value[key].as_bool() {
                *field = saved;
            }
        }
        // Layouts saved while this view had a profile on the bottom carry
        // `"bottom": "profile"` and `stacked`, `stack`, `stack_by` and
        // `profile_legend_width`. The bottom pane now only shows the
        // details, so `bottom` and those keys are ignored.
        restore_height(&value["details_height"], &mut self.details_height);
        // Known titles, each once; anything else keeps the current set.
        if let Some(list) = value["rate_columns"].as_array() {
            let mut titles: Vec<String> = vec![];
            for t in list.iter().filter_map(|t| t.as_str()) {
                if RATE_COLUMNS.iter().any(|c| c.0 == t) && !titles.iter().any(|x| x == t) {
                    titles.push(t.to_string());
                }
            }
            if titles.len() == list.len() {
                self.rate_columns = titles;
            }
        }
    }
}

impl ResourcesState {
    /// The saved columns: field ids and widths, or a layout saved when the
    /// table had its six fixed columns (`columns`: their order and widths).
    fn restore_columns(&mut self, value: &serde_json::Value) {
        if let Some(map) = value["field_widths"].as_object() {
            for (id, w) in map {
                if let Some(w) = w.as_f64().map(|w| w as f32).filter(|w| {
                    fields::known_resource_field(id) && (grid::MIN_WIDTH..=2000.).contains(w)
                }) {
                    self.widths.insert(id.clone(), w);
                }
            }
        }
        let ids = match fields::restore_ids(&value["fields"], Which::Resource) {
            Some(ids) => ids,
            None if value["columns"].is_object() => {
                let mut old = Columns::new(&DEFAULT_COLUMNS.map(|c| c.1));
                old.restore(&value["columns"]);
                for (c, w) in DEFAULT_COLUMNS.iter().zip(&old.widths) {
                    self.widths.insert(c.0.to_string(), *w);
                }
                old.order
                    .iter()
                    .map(|c| DEFAULT_COLUMNS[*c].0.to_string())
                    .collect()
            }
            None => self.fields.clone(),
        };
        // `set_fields` keeps the shown widths first: start from the saved ones.
        self.columns = Columns::new(&[]);
        self.fields.clear();
        self.set_fields(ids);
    }
}

/// The text of field `id` in the resource table: the shared resource
/// fields, with the primary role by name (`roles`: names by role id).
pub fn table_text(
    p: &ProjectData,
    r: &Resource,
    id: &str,
    roles: &BTreeMap<String, String>,
) -> String {
    match id {
        "primary_role" => roles.get(&r.role).cloned().unwrap_or_default(),
        _ => fields::resource_text(p, r, id),
    }
}

#[cfg(test)]
thread_local! {
    /// How many times `table_rows` built the table (tests only).
    pub static ROW_BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The table rows of `lists`: each row's key (a group heading is not a
/// resource: `ListKey::resource` is `None` and it cannot be selected) and
/// the row.
/// Group headings are bands spanning the table; the rows under them carry
/// their strips. `fields` are the shown columns, the tree in the first.
pub fn table_rows(
    p: &ProjectData,
    list: &[Resource],
    lists: &ul::Lists,
    fields: &[String],
    roles: &BTreeMap<String, String>,
) -> (Vec<ListKey>, Vec<Row>) {
    #[cfg(test)]
    ROW_BUILDS.with(|n| n.set(n.get() + 1));
    let by_id: HashMap<i64, &Resource> = list.iter().map(|r| (r.id, r)).collect();
    ul::grid_rows(lists, fields.len(), |key| {
        let r = by_id.get(&key.resource()?)?;
        Some((
            Some(r.kind.icon()),
            fields.iter().map(|f| table_text(p, r, f, roles)).collect(),
        ))
    })
}

/// The resource table's title of field `id` (`catalog`: the fields this
/// project offers).
fn field_title(catalog: &[(String, String)], id: &str) -> String {
    match catalog.iter().find(|(f, _)| f == id) {
        Some((_, title)) => title.clone(),
        // A code or user field another project defines.
        None if id.starts_with("rcode_") => "Resource Code".into(),
        None => "User Field".into(),
    }
}

fn units_text(value: f64) -> String {
    format::units_per_time(value)
}

/// Units/Time in the resource table: units per duration without the unit
/// label (`8/d`), or a percentage, as the reference prints it.
pub fn table_units_text(per_hour: f64) -> String {
    let text = format::units_per_time(per_hour);
    match text.split_once('/') {
        Some((units, per)) => {
            let number: String = units
                .chars()
                .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '-'))
                .collect();
            format!("{}/{per}", if number.is_empty() { units } else { &number })
        }
        None => text,
    }
}

/// A price typed as `$85.00`, `85` or `85/h`, in the user's currency
/// format (decimal symbol, digit grouping). Not negative.
pub fn parse_price(text: &str) -> Option<f64> {
    let text = text.trim();
    let text = text.split_once('/').map_or(text, |(price, _)| price);
    format::parse_cost(text).filter(|v| v.is_finite() && *v >= 0.)
}

fn rsrc_value(p: &ProjectData, rsrc: i64, field: &str) -> String {
    p.document.table("RSRC").map_or(String::new(), |t| {
        t.rows
            .iter()
            .find(|r| num(t.value(r, "rsrc_id")) == rsrc)
            .map_or(String::new(), |r| t.value(r, field).to_string())
    })
}

/// Role names keyed by role id.
pub fn role_names(p: &ProjectData) -> BTreeMap<String, String> {
    p.document.table("ROLES").map_or(BTreeMap::new(), |t| {
        t.rows
            .iter()
            .map(|r| {
                (
                    t.value(r, "role_id").to_string(),
                    t.value(r, "role_name").to_string(),
                )
            })
            .collect()
    })
}

/// Pick a step of 1, 2 or 5 times a power of ten giving about four gridlines.
pub fn nice_step(max: f64) -> f64 {
    if max <= 0. || !max.is_finite() {
        return 1.;
    }
    let raw = max / 4.;
    let power = 10f64.powf(raw.log10().floor());
    [1., 2., 5., 10.]
        .into_iter()
        .map(|m| m * power)
        .find(|s| *s >= raw)
        .unwrap_or(10. * power)
}

impl App {
    fn set_rsrc_value(&mut self, rsrc: i64, field: &str, value: String) {
        if edit::set_value(&mut self.project.document, rsrc, field, value.clone()) {
            match field {
                // Leveling inputs: the resource calendar.
                "clndr_id" => self.resources_changed(),
                "rsrc_name" => {
                    self.project.resource_names.insert(rsrc, value);
                    self.touch_records();
                }
                _ => self.touch_records(),
            }
        }
    }

    /// Set the resource's ID (`rsrc_short_name`, nonempty and unique) or
    /// name (`rsrc_name`) from a table cell command. Returns whether it
    /// changed.
    pub(crate) fn set_resource_text(
        &mut self,
        rsrc: i64,
        column: &str,
        text: &str,
    ) -> Result<bool, String> {
        if edit::value(&self.project.document, rsrc, column) == text {
            return Ok(false);
        }
        if column == "rsrc_short_name" {
            let taken = resources(&self.project)
                .iter()
                .any(|o| o.id != rsrc && o.code.eq_ignore_ascii_case(text));
            if text.is_empty() || taken {
                return Err("Resource ID must be nonempty and unique.".into());
            }
        }
        self.set_rsrc_value(rsrc, column, text.to_string());
        Ok(true)
    }

    /// After an edit of resource calendars or rates: names, leveling inputs,
    /// and a new records revision (one undo step).
    fn resources_changed(&mut self) {
        self.project.refresh_resource_inputs();
        self.touch_records();
    }

    /// The selected resource's id, when the view shows it.
    fn selected_resource(&self) -> Option<i64> {
        self.views
            .resources
            .selected
            .filter(|id| edit::resource_row(&self.project.document, *id).is_some())
    }

    /// The selected resources in table order that still exist.
    fn picked_resources(&self) -> Vec<i64> {
        let doc = &self.project.document;
        self.views
            .resources
            .picked()
            .into_iter()
            .filter(|id| edit::resource_row(doc, *id).is_some())
            .collect()
    }

    /// Delete resources, children before their parents, through the
    /// per-resource delete. Stops at the first resource that cannot go.
    fn delete_resources(&mut self, ids: &[i64]) -> Result<(), String> {
        let mut left: Vec<i64> = ids.to_vec();
        loop {
            let mut refused = None;
            let before = left.len();
            // Later rows first: a child shows below its parent.
            for id in left.clone().into_iter().rev() {
                match edit::delete_resource(&mut self.project.document, id) {
                    Ok(()) => left.retain(|x| *x != id),
                    Err(e) => refused = Some(e),
                }
            }
            match refused {
                None => return Ok(()),
                Some(e) if left.len() == before => return Err(e),
                Some(_) => {}
            }
        }
    }

    /// Edit commands of the Resources view: Add, Delete, Cut, Copy, Paste.
    /// Delete, Cut and Copy act on every selected resource. Add and Paste
    /// put the resources beside the current one (under the same parent);
    /// Paste after Cut moves the cut resources there. Each change is one
    /// undo step.
    pub(crate) fn resource_command(&mut self, command: &str, time: f64) {
        // Add opens the New Resource wizard while it is switched on.
        if command == "Add" && self.activity_wizard.assistance.resource_wizard {
            self.open_resource_wizard();
            return;
        }
        self.resource_command_with(command, time, false);
    }

    /// The parent Add gives a new resource: the selected resource's own
    /// parent, so the new one goes beside it.
    pub(crate) fn new_resource_parent(&self) -> Option<i64> {
        self.selected_resource()
            .map(|id| records::num(&edit::value(&self.project.document, id, "parent_rsrc_id")))
            .filter(|p| *p != 0)
    }

    /// Yes in the confirmation of a delete of several resources.
    pub(crate) fn resource_delete_confirmed(&mut self, time: f64) {
        self.resource_command_with("Delete", time, true);
    }

    /// Add a resource under `parent` with the defaults of a new resource:
    /// the project calendar, the base currency and a rate row effective from
    /// the data date. Returns its id; the caller records the undo step.
    pub(crate) fn new_resource(&mut self, parent: Option<i64>) -> i64 {
        let doc = &self.project.document;
        let pid = self.project.model.project.proj_id;
        let calendar = records::lookup(doc, "PROJECT", "proj_id", pid, "clndr_id");
        // The base currency (the one the header names, else the
        // one with exchange rate 1), not merely the first row.
        let currency =
            crate::enterprise::ops::base_currency(doc).map_or(String::new(), |id| id.to_string());
        let origin = self.project.model.origin_serial;
        let day = self
            .project
            .model
            .project
            .data_date
            .unwrap_or(0)
            .div_euclid(DAY)
            * DAY;
        let effective = openxer_xer::date_string(origin, Some(day));
        edit::add_resource(
            &mut self.project.document,
            parent,
            &calendar,
            &currency,
            &effective,
        )
    }

    /// Assign Resources, New: add a top-level resource with the typed ID and
    /// name (the ID when no name was typed) as one undo step. Returns its id.
    pub(crate) fn add_named_resource(
        &mut self,
        code: &str,
        name: &str,
        time: f64,
    ) -> Result<i64, String> {
        let (code, name) = (code.trim(), name.trim());
        if code.is_empty() {
            return Err("Enter a Resource ID.".into());
        }
        if edit::unique_code(&self.project.document, code) != code {
            return Err(format!("Resource ID {code} is already in use."));
        }
        let before = self.project.clone();
        let revision = self.records_revision;
        let id = self.new_resource(None);
        let doc = &mut self.project.document;
        edit::set_value(doc, id, "rsrc_short_name", code.to_string());
        let name = if name.is_empty() { code } else { name };
        edit::set_value(doc, id, "rsrc_name", name.to_string());
        self.project.refresh_resource_inputs();
        self.edit_step(before, revision, time);
        Ok(id)
    }

    fn resource_command_with(&mut self, command: &str, time: f64, confirmed: bool) {
        if command == "Paste" {
            // Cut resources deleted since drop out; the rest still paste.
            let doc = &self.project.document;
            self.views
                .resources
                .cut
                .retain(|id| edit::resource_row(doc, *id).is_some());
        }
        let picked = self.picked_resources();
        if command == "Delete" && !confirmed && record_view::needs_confirm(picked.len()) {
            let doc = &self.project.document;
            let names: Vec<String> = picked
                .iter()
                .map(|id| {
                    format!(
                        "{} - {}",
                        edit::value(doc, *id, "rsrc_short_name"),
                        edit::value(doc, *id, "rsrc_name")
                    )
                })
                .collect();
            self.views.records.confirm_delete = Some(record_view::ViewDelete {
                view: MainView::Resources,
                text: record_view::delete_question(picked.len(), "resource", "resources", &names),
            });
            return;
        }
        let parent = self.new_resource_parent();
        let code = |app: &Self, id: i64| edit::value(&app.project.document, id, "rsrc_short_name");
        let before = self.project.clone();
        let revision = self.records_revision;
        let mut moved = false;
        let result: Result<Vec<i64>, String> = match command {
            "Add" => Ok(vec![self.new_resource(parent)]),
            "Delete" if picked.is_empty() => Err("Select a resource to delete.".into()),
            "Delete" => self.delete_resources(&picked).map(|()| vec![]),
            "Cut" => {
                self.status = match picked.as_slice() {
                    [] => "Select a resource to cut.".into(),
                    [id] => format!("Cut resource {}", code(self, *id)),
                    many => format!("Cut {} resources", many.len()),
                };
                if !picked.is_empty() {
                    self.views.resources.cut = picked;
                    self.views.resources.clipboard.clear();
                }
                return;
            }
            "Copy" => {
                let clips: Vec<ResourceClip> = picked
                    .iter()
                    .filter_map(|id| edit::copy_resource(&self.project.document, *id))
                    .collect();
                self.status = match picked.as_slice() {
                    [] => "Select a resource to copy.".into(),
                    [id] => format!("Copied resource {}", code(self, *id)),
                    many => format!("Copied {} resources", many.len()),
                };
                if !clips.is_empty() {
                    self.views.resources.cut.clear();
                    self.views.resources.clipboard = clips;
                }
                return;
            }
            "Paste" if !self.views.resources.cut.is_empty() => {
                // Cut resources already under the target parent stay put.
                let doc = &self.project.document;
                let there = |id: i64| {
                    let current = records::num(&edit::value(doc, id, "parent_rsrc_id"));
                    edit::resource_row(doc, id).is_some()
                        && (current != 0 && Some(current) == parent
                            || current == 0 && parent.is_none())
                };
                // Only the top-most cut resources move: their children
                // come along under them.
                let ids: Vec<i64> = edit::top_most(doc, &self.views.resources.cut)
                    .into_iter()
                    .filter(|&id| !there(id))
                    .collect();
                if ids.is_empty() {
                    self.views.resources.cut.clear();
                    self.status = "The resource is already there.".into();
                    return;
                }
                moved = true;
                ids.iter()
                    .try_for_each(|&id| edit::move_resource(&mut self.project.document, id, parent))
                    .map(|()| ids)
            }
            "Paste" if !self.views.resources.clipboard.is_empty() => {
                let clips = self.views.resources.clipboard.clone();
                Ok(edit::paste_resources(
                    &mut self.project.document,
                    &clips,
                    parent,
                ))
            }
            "Paste" => Err("Nothing to paste.".into()),
            _ => return,
        };
        match result {
            Ok(added) => {
                if moved {
                    self.views.resources.cut.clear();
                }
                self.project.refresh_resource_inputs();
                self.edit_step(before, revision, time);
                let verb = if moved { "moved" } else { "added" };
                self.status = match (command, added.as_slice()) {
                    ("Delete", _) if picked.len() > 1 => {
                        format!("{} resources deleted", picked.len())
                    }
                    ("Delete", _) => "Resource deleted".into(),
                    (_, [id]) => format!("Resource {} {verb}", code(self, *id)),
                    (_, []) => String::new(),
                    (_, many) => format!("{} resources {verb}", many.len()),
                };
                let state = &mut self.views.resources;
                if added.is_empty() {
                    state.selected = None;
                } else {
                    // A new resource is not assigned yet: show all resources.
                    if !moved {
                        state.set_filter(ListFilter::All);
                    }
                    if let Some(p) = parent {
                        state.list.collapsed.remove(&ListKey::Resource(p));
                    }
                    state.selected = state.rows.select(&added);
                }
            }
            Err(e) => {
                self.project = before;
                self.status = e.clone();
                if command == "Delete" {
                    self.error(e);
                }
            }
        }
    }

    /// Commands the right dock enables in the Resources view: (add, delete,
    /// copy, paste).
    pub(crate) fn resource_commands_enabled(&self) -> (bool, bool, bool, bool) {
        let selected = self.selected_resource().is_some();
        (true, selected, selected, self.resource_paste_ready())
    }

    /// Something was copied, or a cut resource still exists.
    fn resource_paste_ready(&self) -> bool {
        let state = &self.views.resources;
        !state.clipboard.is_empty()
            || state
                .cut
                .iter()
                .any(|id| edit::resource_row(&self.project.document, *id).is_some())
    }

    pub(super) fn resources_view(&mut self, host: &mut egui::Ui) {
        // The resource list, the arranged rows and their cells are kept
        // while the project, the list choices, the columns and the formats
        // are unchanged: a large dictionary is not rebuilt every frame.
        let serial = self.project_serial();
        let list = self.usage_resources();
        if self.views.resources.selected.is_none() {
            self.views.resources.selected = list
                .iter()
                .find(|r| r.assignments > 0)
                .or(list.first())
                .map(|r| r.id);
        }
        let selected = self
            .views
            .resources
            .selected
            .and_then(|id| list.iter().find(|r| r.id == id))
            .cloned();
        if self.views.resources.show_bottom {
            let tab = self.views.resources.tab;
            let (height, clicked) = self.details_pane(
                host,
                "resources",
                self.views.resources.details_height,
                &TABS,
                tab,
                |app, c, size| app.resource_details(c, size, tab, selected.as_ref()),
            );
            self.views.resources.details_height = height;
            if let Some(tab) = clicked {
                self.views.resources.tab = tab;
            }
        }
        let roles = self.memo(serial, "resources_roles", (), || role_names(&self.project));
        let arrangement = (
            self.views.resources.list_state(),
            self.open_set.parked.len(),
        );
        let lists = self.memo(serial, "resources_lists", arrangement.clone(), || {
            self.resource_table_lists(&list)
        });
        let catalog = self.memo(serial, "resources_catalog", (), || {
            fields::available(&self.project, Which::Resource)
        });
        let state = &self.views.resources;
        let field_ids = state.fields.clone();
        let titles: Vec<String> = field_ids.iter().map(|f| field_title(&catalog, f)).collect();
        // Resource user fields (`rudf_<id>`): numbers right aligned,
        // indicators with their marks, every one edited in place.
        let udf_of = |f: &str| match fields::dynamic_field(f) {
            Some(("rudf_", id)) => {
                openxer_xer::edit::udf::udf_type(&self.project.document, id).map(|x| x.1)
            }
            _ => None,
        };
        let udf_types: Vec<_> = field_ids.iter().map(|f| udf_of(f)).collect();
        use super::udf_columns::{is_indicator, is_number};
        let right: Vec<usize> = field_ids
            .iter()
            .enumerate()
            .filter(|(i, f)| {
                let udf = udf_types[*i].as_ref();
                fields::NUMBER_FIELDS.contains(&f.as_str()) || udf.is_some_and(is_number)
            })
            .map(|(i, _)| i)
            .collect();
        let indicators: Vec<usize> = (0..field_ids.len())
            .filter(|i| udf_types[*i].as_ref().is_some_and(is_indicator))
            .collect();
        let doc = &self.project.document;
        let editors: Vec<Option<grid::EditKind>> = udf_types
            .iter()
            .map(|t| super::udf_columns::editor(doc, t.as_ref()?))
            .collect();
        let menu_state = ResourcesMenu {
            fill: self.view_fill_ready(),
            selected: true,
            clipboard: self.resource_paste_ready(),
            details: state.show_bottom,
            filter: state.filter(),
            group: state.group(),
            sorted: state.list.sort.field.is_some(),
            expandable: !lists.parents.is_empty() || !state.list.collapsed.is_empty(),
        };
        let mut chosen: Option<&'static str> = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(sheet::MARGIN))
            .show_inside(host, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                let (inner, strip) = sheet::table_frame(ui, ui.max_rect());
                let bar = Rect::from_min_size(strip, Vec2::new(inner.width(), 17.));
                let text = format!(
                    "Display: {}",
                    menu_state.filter.caption(ul::ListView::Resource)
                );
                let header = sheet::header_button(ui, "resources", bar, &text);
                self.action("resources_display", &header);
                egui::Popup::menu(&header)
                    .style(chrome::popup_menu_style)
                    .show(|ui| {
                        let mut m = crate::context_menu::Menu::on(ui, "resources_display");
                        if let Some(key) = m.entries(&display_entries(&menu_state)) {
                            chosen = Some(key);
                        }
                    });
                let body = Rect::from_min_max(Pos2::new(inner.left(), bar.bottom()), inner.max);
                let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(body));
                let ui = &mut ui;
                let key = (
                    arrangement,
                    field_ids.clone(),
                    format!("{:?}", format::current()),
                );
                let mut cell_edit = self.take_cell_edit("resources");
                let built = self.memo(serial, "resources_rows", key, || {
                    table_rows(&self.project, &list, &lists, &field_ids, &roles)
                });
                let (keys, rows) = (&built.0, &built.1);
                // Group headings are not resources: they cannot be selected.
                let shown: Vec<Option<i64>> = keys.iter().map(ListKey::resource).collect();
                // The rows for the table's cell commands (Fill Down, Find).
                let cell_rows: std::rc::Rc<dyn super::table_cells::RowCells> = built.clone();
                self.note_view_cells(
                    "resources",
                    field_ids.clone(),
                    titles.clone(),
                    shown
                        .iter()
                        .map(|k| k.map(super::table_cells::CellKey::Resource))
                        .collect(),
                    cell_rows,
                );
                let state = &mut self.views.resources;
                let selection = state.rows.shown(&mut state.selected, &shown);
                let menu = |row: usize| {
                    resources_menu(&ResourcesMenu {
                        selected: shown.get(row).is_some_and(Option::is_some),
                        ..menu_state
                    })
                };
                let open_menu = self.ctx_menu.take();
                let title_refs: Vec<&str> = titles.iter().map(String::as_str).collect();
                let table = grid::Table {
                    id: "resources",
                    titles: &title_refs,
                    right: &right,
                    tree_field: 0,
                    header_height: grid::SHORT_HEADER,
                    row_lines: false,
                    band_pitch: grid::BandPitch::Styled,
                    band_caption: &[],
                    menu: Some(&menu),
                    open_menu,
                    chart_menu: None,
                };
                // The sorted column's title carries the arrow.
                let sort = &self.views.resources.list.sort;
                let sorted = sort
                    .field
                    .as_ref()
                    .and_then(|f| field_ids.iter().position(|x| x == f))
                    .map(|i| (i, sort.descending));
                let kind = |row: usize, field: usize| {
                    shown.get(row).copied().flatten()?;
                    editors.get(field).cloned().flatten()
                };
                let response = grid::show_with(
                    ui,
                    &table,
                    &mut self.views.resources.columns,
                    rows,
                    &if self.print.painting {
                        grid::Selection::default()
                    } else {
                        selection
                    },
                    None,
                    None,
                    &mut self.action_rects,
                    grid::Extras {
                        sorted,
                        indicators: &indicators,
                        editing: Some(grid::Editing {
                            kind: &kind,
                            open: &mut cell_edit,
                        }),
                        ..Default::default()
                    },
                );
                self.keep_cell_edit("resources", cell_edit);
                if let Some((row, field, text)) = response.committed.clone() {
                    let ty = udf_types.get(field).cloned().flatten().map(|t| t.type_id);
                    if let (Some(type_id), Some(Some(rsrc))) = (ty, shown.get(row)) {
                        let time = ui.ctx().input(|i| i.time);
                        self.commit_udf_cell(type_id, *rsrc, &text, time);
                    }
                }
                self.views.resources.sync_columns();
                if let Some(id) = response.header_clicked.and_then(|f| field_ids.get(f)) {
                    let sort = &mut self.views.resources.list.sort;
                    if sort.field.as_ref() == Some(id) {
                        sort.descending = !sort.descending;
                    } else {
                        sort.field = Some(id.clone());
                        sort.descending = false;
                    }
                }
                if let Some(key) = response.toggled.and_then(|i| keys.get(i)) {
                    let collapsed = &mut self.views.resources.list.collapsed;
                    if !collapsed.remove(key) {
                        collapsed.insert(key.clone());
                    }
                }
                if let Some(id) = response
                    .clicked
                    .and_then(|i| shown.get(i).copied().flatten())
                {
                    let state = &mut self.views.resources;
                    let current = state.selected;
                    state.selected = Some(state.rows.click(
                        current.as_ref(),
                        id,
                        response.modifiers,
                        response.secondary,
                    ));
                }
                if response
                    .double_clicked
                    .is_some_and(|i| shown.get(i).is_some_and(Option::is_some))
                {
                    self.views.resources.show_bottom = true;
                }
                if let Some((_, key)) = response.menu {
                    let time = ui.ctx().input(|i| i.time);
                    match key {
                        "add" => self.resource_command("Add", time),
                        "cut" => self.resource_command("Cut", time),
                        "copy" => self.resource_command("Copy", time),
                        "paste" => self.resource_command("Paste", time),
                        "delete" => self.resource_command("Delete", time),
                        "fill_down" => self.view_fill_down(time),
                        "del_merge" => self.open_resource_merge(),
                        "details" => {
                            let state = &mut self.views.resources;
                            state.show_bottom = !state.show_bottom;
                        }
                        key => chosen = Some(key),
                    }
                }
                if rows.is_empty() {
                    let text = match menu_state.filter {
                        ListFilter::Project => "No resources are assigned in this project.",
                        ListFilter::All => "This XER carries no resources.",
                        ListFilter::Custom => "No resources match the filter.",
                    };
                    grid::empty_note(ui, &response, "resources", text, &mut self.action_rects);
                }
            });
        if let Some(key) = chosen {
            self.resources_display_command(key);
        }
    }

    /// The rows of the resource table as Filter By and Group and Sort By
    /// arrange them, built by the bottom resource lists' `build`.
    pub(crate) fn resource_table_lists(&self, list: &[Resource]) -> ul::Lists {
        let state = self.views.resources.list_state();
        let others = self.others();
        let project = ul::project_resources(&self.project, &others);
        let role_list = ul::roles(&self.project);
        let codes = ul::resource_codes(&self.project);
        ul::build(
            &ul::ListData {
                project: &self.project,
                resources: list,
                roles: &role_list,
                codes: &codes,
                project_resources: &project,
                project_roles: &BTreeSet::new(),
            },
            &state,
        )
    }

    /// A Display item of the Resources table (the Display bar or the
    /// right-click menu): Filter By, Group and Sort By, Clear Sort,
    /// Columns..., Expand All and Collapse All. The Customize... items open
    /// the bottom resource lists' dialogs on the table's choices.
    pub(crate) fn resources_display_command(&mut self, key: &str) {
        match key {
            "display_all" => self.views.resources.set_filter(ListFilter::All),
            "display_project" => self.views.resources.set_filter(ListFilter::Project),
            "filter_custom" => {
                let custom = self.views.resources.list.custom.clone();
                self.panes.usage_list_filter = Some((ListOwner::Resources, custom));
            }
            "group_custom" => {
                let mut levels = self.views.resources.list.levels.clone();
                if levels.is_empty() {
                    levels.push(ul::GroupLevel {
                        code_type: String::new(),
                        descending: false,
                    });
                }
                self.panes.usage_list_group = Some((ListOwner::Resources, levels));
            }
            "sort_clear" => self.views.resources.list.sort = SheetSort::default(),
            "columns" => self.open_resource_columns(),
            "expand_all" => self.views.resources.list.collapsed.clear(),
            "collapse_all" => {
                // Rows under collapsed rows are not built: open all first.
                self.views.resources.list.collapsed.clear();
                let lists = self.resource_table_lists(&resources(&self.project));
                self.views.resources.list.collapsed = lists.parents.into_iter().collect();
            }
            key => {
                if let Some(&(_, group)) = GROUPS.iter().find(|g| g.0 == key) {
                    let state = &mut self.views.resources;
                    state.group = Some(group);
                    state.list.collapsed.clear();
                }
            }
        }
    }

    /// Columns... of the Resources table: the Customize Columns dialog over
    /// the shared resource field catalog (codes and user fields included).
    pub(crate) fn open_resource_columns(&mut self) {
        let catalog = fields::available(&self.project, Which::Resource);
        // The resource user fields go under "User Defined".
        let is_udf = |id: &str| id.starts_with("rudf_");
        let titles: Vec<&str> = catalog
            .iter()
            .filter(|c| !is_udf(&c.0))
            .map(|c| c.1.as_str())
            .collect();
        let udfs: Vec<(String, String)> =
            catalog.iter().filter(|c| is_udf(&c.0)).cloned().collect();
        let default: Vec<String> = DEFAULT_COLUMNS
            .iter()
            .map(|c| field_title(&catalog, c.0))
            .collect();
        let default: Vec<&str> = default.iter().map(String::as_str).collect();
        let shown: Vec<String> = self
            .views
            .resources
            .fields
            .iter()
            .filter(|f| catalog.iter().any(|c| &c.0 == *f))
            .map(|f| field_title(&catalog, f))
            .collect();
        self.open_view_columns_with(
            crate::details::grid_columns::ColumnTarget::Resources,
            "Resource",
            &titles,
            &udfs,
            &default,
            shown,
        );
    }

    /// OK in Columns...: the chosen titles become the table's fields.
    pub(crate) fn apply_resource_columns(&mut self, titles: &[String]) {
        let catalog = fields::available(&self.project, Which::Resource);
        let ids = titles
            .iter()
            .filter_map(|t| catalog.iter().find(|c| &c.1 == t).map(|c| c.0.clone()))
            .collect();
        self.views.resources.set_fields(ids);
    }

    /// View > Show on Bottom in the Resources window: No Bottom Layout or
    /// Resource Details, the current one checked.
    pub(crate) fn resources_bottom_items(&mut self, ui: &mut egui::Ui) {
        let current = self.views.resources.show_bottom;
        for (key, label, details) in BOTTOM_CHOICES {
            let mut item = chrome::Item::new(label).checked(current == details);
            if let Some(icon) = bottom_icon(details) {
                item = item.icon(icon);
            }
            if self.item(ui, &format!("menu_resources_{key}"), item) {
                self.views.resources.show_bottom = details;
            }
        }
    }

    fn resource_details(
        &mut self,
        c: &mut Canvas,
        size: Vec2,
        tab: usize,
        selected: Option<&Resource>,
    ) {
        let (w, h) = (size.x, size.y);
        let Some(r) = selected else {
            c.label(12., 12., 13., "Select a resource.", true);
            return;
        };
        match TABS[tab.min(TABS.len() - 1)] {
            "General" => self.resource_general(c, w, r),
            "Codes" => {
                let rows = self.resource_code_rows(r);
                let cols = [
                    ("Resource Code", 100., false),
                    ("Resource Code Value", 100., false),
                    ("Code Description", 200., false),
                ];
                let selected = self.views.resources.code_row.filter(|i| *i < rows.len());
                if let Some(i) = sheet::table(
                    c,
                    "resource_codes",
                    6.,
                    6.,
                    w - 12.,
                    h - 38.,
                    &cols,
                    &rows,
                    selected,
                ) {
                    self.views.resources.code_row = Some(i);
                }
                // Assign stays enabled with no code values: its list opens
                // empty (the rule of `details::form::add_button`).
                match sheet::buttons(
                    c,
                    "resource_codes_buttons",
                    5.,
                    h - 26.,
                    &[("Assign", true), ("Remove", selected.is_some())],
                ) {
                    Some(0) => self.views.resources.pick = Some((Pick::Code, r.id)),
                    Some(1) => {
                        if let Some(kind) =
                            selected.and_then(|i| self.resource_code_kinds(r).get(i).copied())
                        {
                            edit::remove_code(&mut self.project.document, r.id, kind);
                            self.views.resources.code_row = None;
                            self.resources_changed();
                        }
                    }
                    _ => {}
                }
            }
            "Details" => self.resource_more(c, w, h, r),
            "Units & Prices" => self.resource_rates(c, w, h, r),
            "Roles" => {
                let rows = self.resource_role_rows(r);
                let cols = [
                    ("Role ID", 150., false),
                    ("Role Name", 250., false),
                    ("Proficiency", 150., false),
                    ("Primary Role", 100., false),
                ];
                let selected = self.views.resources.role_row.filter(|i| *i < rows.len());
                // Assign stays enabled with no roles: the Roles list opens
                // empty (the rule of `details::form::add_button`).
                let any = true;
                let mut chosen = None;
                if let Some(i) = sheet::table_menu(
                    c,
                    "resource_roles",
                    6.,
                    6.,
                    w - 12.,
                    h - 38.,
                    &cols,
                    &rows,
                    selected,
                    Some(&mut |row, response, at| {
                        let on_row = row.is_some() || selected.is_some();
                        let entries = [
                            Entry::Item(Spec::new("assign", "Assign").enabled(any)),
                            Entry::Item(Spec::new("remove", "Remove").enabled(on_row)),
                        ];
                        if let Some(key) =
                            sheet::entries_menu("resource_roles_menu", response, at, &entries)
                        {
                            chosen = Some(key);
                        }
                    }),
                ) {
                    self.views.resources.role_row = Some(i);
                }
                let ids = self.resource_role_ids(r);
                let button = sheet::buttons(
                    c,
                    "resource_roles_buttons",
                    5.,
                    h - 26.,
                    &[
                        ("Assign", any),
                        ("Remove", selected.is_some()),
                        ("Primary", selected.is_some()),
                    ],
                );
                let clicked = button.or(match chosen {
                    Some("assign") => Some(0),
                    Some("remove") => Some(1),
                    _ => None,
                });
                match clicked {
                    Some(0) => self.views.resources.pick = Some((Pick::Role, r.id)),
                    Some(1) => {
                        if let Some(role) = selected.and_then(|i| ids.get(i).copied()) {
                            edit::remove_role(&mut self.project.document, r.id, role);
                            self.views.resources.role_row = None;
                            self.resources_changed();
                        }
                    }
                    Some(2) => {
                        if let Some(role) = selected.and_then(|i| ids.get(i).copied()) {
                            self.set_rsrc_value(r.id, "role_id", role.to_string());
                        }
                    }
                    _ => {}
                }
            }
            "Notes" => self.resource_notes(c, w, h, r),
            _ => {
                let rows = self.resource_udf_rows(r);
                let cols = [
                    ("User Defined Field", 200., false),
                    ("Data Type", 200., false),
                    ("Value", 200., false),
                ];
                sheet::table(
                    c,
                    "resource_udf",
                    1.,
                    1.,
                    w - 2.,
                    h - 34.,
                    &cols,
                    &rows,
                    None,
                );
                c.button(
                    "resource_udf_customize",
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
                    "resource_udf_hide",
                    300.,
                    h - 27.,
                    300.,
                    21.,
                    "Hide empty rows",
                    &mut hide,
                    false,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn resource_text(
        &mut self,
        c: &mut Canvas,
        x: f32,
        y: f32,
        w: f32,
        label: &str,
        rsrc: i64,
        field_name: &str,
    ) {
        let value = rsrc_value(&self.project, rsrc, field_name);
        if let Some(text) = sheet::text_field(
            c,
            &format!("resource_field_{field_name}"),
            x,
            y,
            w,
            label,
            &value,
            true,
        ) {
            if field_name == "rsrc_name" {
                self.project.resource_names.insert(rsrc, text.clone());
            }
            self.set_rsrc_value(rsrc, field_name, text);
        }
    }

    fn resource_general(&mut self, c: &mut Canvas, w: f32, r: &Resource) {
        sheet::section(
            c,
            5.,
            5.,
            w - 10.,
            485f32.min(c.ui.max_rect().height() - 5.),
            "",
        );
        let code = rsrc_value(&self.project, r.id, "rsrc_short_name");
        if let Some(code) = sheet::text_field(
            c,
            "resource_field_rsrc_short_name",
            34.,
            33.,
            288.,
            "Resource ID",
            &code,
            true,
        ) {
            let taken = resources(&self.project)
                .iter()
                .any(|o| o.id != r.id && o.code.eq_ignore_ascii_case(code.trim()));
            if code.trim().is_empty() || taken {
                self.status = "Resource ID must be nonempty and unique.".to_string();
            } else {
                self.set_rsrc_value(r.id, "rsrc_short_name", code);
            }
        }
        let wide = (w - 621.).max(200.);
        self.resource_text(
            c,
            345.,
            33.,
            wide + 245.,
            "Resource Name",
            r.id,
            "rsrc_name",
        );
        self.resource_text(c, 34., 83., 528., "Employee ID", r.id, "employee_code");
        self.resource_text(c, 584., 83., wide, "Title", r.id, "rsrc_title_name");
        self.resource_text(c, 34., 133., 528., "E-Mail Address", r.id, "email_addr");
        self.resource_text(c, 584., 133., 317., "Office Phone", r.id, "office_phone");
        let mut active = r.active;
        if c.checkbox(
            "resource_active",
            936.,
            134.,
            200.,
            17.,
            "Active",
            &mut active,
            true,
        )
        .clicked()
        {
            self.set_rsrc_value(r.id, "active_flag", if active { "Y" } else { "N" }.into());
        }
    }

    fn resource_more(&mut self, c: &mut Canvas, w: f32, h: f32, r: &Resource) {
        let flag = |app: &Self, field: &str| edit::value(&app.project.document, r.id, field) == "Y";
        let (mut ot, mut auto, mut link) = (
            flag(self, "ot_flag"),
            flag(self, "auto_compute_act_flag"),
            flag(self, "def_cost_qty_link_flag"),
        );
        let left_w = (w - 978.).max(600.);
        self.views.resources.details_left = left_w;
        let right = left_w + 19.;
        sheet::section(c, 5., 5., left_w, h - 5., "");
        sheet::section(c, 6., 6., left_w - 2., 100., "Resource Type");
        sheet::section(c, 6., 106., left_w - 2., h - 111., "Currency and Overtime");
        sheet::section(c, right, 5., (w - right - 5.).max(200.), h - 5., "Profile");
        for (key, x, kind) in [
            ("resource_type_labor", 14., ResourceType::Labor),
            ("resource_type_nonlabor", 293., ResourceType::Nonlabor),
            ("resource_type_material", 572., ResourceType::Material),
        ] {
            if c.radio(key, x, 42., 279., 12., kind.label(), r.kind == kind, true) && r.kind != kind
            {
                let code = edit::TYPES
                    .iter()
                    .find(|(_, l)| *l == kind.label())
                    .map_or("RT_Labor", |(c, _)| c);
                self.set_rsrc_value(r.id, "rsrc_type", code.to_string());
            }
        }
        let units = self
            .project
            .document
            .table("UMEASURE")
            .is_some_and(|t| !t.rows.is_empty());
        sheet::above(c, 576., 80., "Unit of Measure", true);
        if sheet::picker_button(
            c,
            "resource_unit",
            576.,
            80.,
            268.,
            Some(chrome::Icon::Resources),
            &r.unit,
            units,
        ) {
            self.views.resources.pick = Some((Pick::Unit, r.id));
        }
        sheet::above(c, 43., 148., "Currency", true);
        let currency = self.project.document.table("CURRTYPE").and_then(|t| {
            t.rows
                .iter()
                .find(|x| t.value(x, "curr_id") == rsrc_value(&self.project, r.id, "curr_id"))
                .map(|x| t.value(x, "curr_type").to_string())
        });
        let currencies = self
            .project
            .document
            .table("CURRTYPE")
            .is_some_and(|t| !t.rows.is_empty());
        if sheet::picker_button(
            c,
            "resource_currency",
            43.,
            148.,
            // 652 wide at the reference size; narrower windows keep it clear of the Profile group.
            (self.views.resources.details_left - 60.).min(652.),
            Some(chrome::Icon::Expenses),
            currency.as_deref().unwrap_or(""),
            currencies,
        ) {
            self.views.resources.pick = Some((Pick::Currency, r.id));
        }
        if c.checkbox(
            "resource_overtime",
            43.,
            174.,
            400.,
            17.,
            "Overtime Allowed",
            &mut ot,
            true,
        )
        .clicked()
        {
            self.set_rsrc_value(r.id, "ot_flag", if ot { "Y" } else { "N" }.into());
        }
        c.label(43., 213., 21., "Overtime Factor", ot);
        let factor = rsrc_value(&self.project, r.id, "ot_factor");
        if let Some(text) = sheet::commit_field(
            c,
            "resource_overtime_factor",
            131.,
            213.,
            262.,
            &factor,
            ot,
            Align::Max,
        ) {
            match text.trim().parse::<f64>() {
                Ok(v) if v.is_finite() && v >= 0. => {
                    self.set_rsrc_value(r.id, "ot_factor", records::format_number(v))
                }
                _ if text.trim().is_empty() => {
                    self.set_rsrc_value(r.id, "ot_factor", String::new())
                }
                _ => self.status = "Enter an overtime factor of zero or more.".into(),
            }
        }
        sheet::above(c, right + 17., 51., "Calendar", true);
        if sheet::picker_button(
            c,
            "resource_calendar",
            right + 17.,
            51.,
            246.,
            Some(chrome::Icon::Options),
            &r.calendar,
            !self.project.model.calendars.is_empty(),
        ) {
            self.views.resources.pick = Some((Pick::Calendar, r.id));
        }
        c.button(
            "resource_personal_calendar",
            right + 267.,
            51.,
            150.,
            21.,
            "Create Personal Calendar",
            false,
            false,
        );
        sheet::above(c, right + 17., 99., "Default Units / Time", true);
        if let Some(text) = sheet::commit_field(
            c,
            "resource_default_units",
            right + 17.,
            99.,
            246.,
            &units_text(r.default_units),
            true,
            Align::Max,
        ) {
            match crate::details::assignments::parse_units_per_time(&text) {
                Some(v) => self.set_rsrc_value(
                    r.id,
                    "def_qty_per_hr",
                    openxer_xer::assignments::quantity_text(v),
                ),
                None => self.status = format!("Enter units per time such as {}.", units_text(1.)),
            }
        }
        for (key, y, w, caption, value, field) in [
            (
                "resource_auto_actuals",
                134.,
                256.,
                "Auto Compute Actuals",
                &mut auto,
                "auto_compute_act_flag",
            ),
            (
                "resource_cost_link",
                158.,
                258.,
                "Calculate costs from units",
                &mut link,
                "def_cost_qty_link_flag",
            ),
        ] {
            if c.checkbox(key, right + 17., y, w, 18., caption, value, true)
                .clicked()
            {
                let v = if *value { "Y" } else { "N" };
                self.set_rsrc_value(r.id, field, v.into());
            }
        }
    }

    fn resource_rates(&mut self, c: &mut Canvas, w: f32, h: f32, r: &Resource) {
        c.label(10., 7., 21., "Shift Calendar:", true);
        // Shifts come from Enterprise > Resource Shifts.
        let doc = &self.project.document;
        let shifts = crate::enterprise::ops::list_ids(doc, &crate::enterprise::ops::SHIFTS);
        let names: Vec<(String, String)> = std::iter::once((String::new(), String::new()))
            .chain(shifts.iter().map(|id| {
                (
                    id.to_string(),
                    crate::enterprise::ops::list_value(
                        doc,
                        &crate::enterprise::ops::SHIFTS,
                        *id,
                        "shift_name",
                    ),
                )
            }))
            .collect();
        let row = edit::resource_row(doc, r.id);
        let current = row
            .map(|i| records::get(doc, "RSRC", i, "shift_id"))
            .unwrap_or_default();
        let periods = crate::enterprise::ops::shift_periods(doc, records::num(&current)).len();
        let options: Vec<(&str, &str)> = names
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        if let Some(code) = sheet::combo(
            c,
            "resource_shift_calendar",
            115.,
            7.,
            129.,
            &options,
            &current,
            row.is_some() && !shifts.is_empty(),
        ) {
            self.set_rsrc_value(r.id, "shift_id", code);
        }
        c.label(270., 7., 21., "Shift:", true);
        let mut shift = periods.max(1).to_string();
        // Wide enough for the number beside the arrows.
        c.spin(
            "resource_shift",
            315.,
            6.,
            48.,
            23.,
            &mut shift,
            1.,
            1.0..=1.,
            false,
        );
        let rate_rows = edit::rate_rows(&self.project.document, r.id);
        let get = |app: &Self, row: usize, k: &str| {
            records::get(&app.project.document, "RSRCRATE", row, k)
        };
        let shown = self.views.resources.rate_columns();
        // A price is per hour of labor or nonlabor, per unit of measure of
        // a material.
        let per = match r.unit.trim() {
            _ if r.kind != ResourceType::Material => "h",
            "" => "unit",
            u => u,
        };
        let rows: Vec<Vec<String>> = rate_rows
            .iter()
            .map(|&i| {
                shown
                    .iter()
                    .map(|(_, _, _, column)| match column {
                        None => format::date(
                            0,
                            openxer_xer::parse_absolute(&get(self, i, "start_date")),
                        ),
                        Some("max_qty_per_hr") => {
                            units_text(records::float(&get(self, i, "max_qty_per_hr")))
                        }
                        Some(price) => {
                            format!(
                                "{}/{per}",
                                format::cost(records::float(&get(self, i, price)))
                            )
                        }
                    })
                    .collect()
            })
            .collect();
        let cols: Vec<(&str, f32, bool)> = shown.iter().map(|c| (c.0, c.1, c.2)).collect();
        let selected = self
            .views
            .resources
            .rate_row
            .filter(|i| *i < rows.len())
            .or((!rows.is_empty()).then_some(0));
        let mut chosen = None;
        if let Some(i) = sheet::table_menu(
            c,
            "resource_rates",
            6.,
            36.,
            w - 12.,
            h - 68.,
            &cols,
            &rows,
            selected,
            Some(&mut |row, response, at| {
                // A right-click on a row selects it first, so Delete reads
                // true there; on the empty area only Add applies.
                let deletable = rows.len() > 1 && (row.is_some() || selected.is_some());
                let entries = [
                    Entry::Item(Spec::new("add", "Add")),
                    Entry::Item(Spec::new("delete", "Delete").enabled(deletable)),
                    Entry::Separator,
                    Entry::Item(Spec::new("customize", "Customize Resource Rate Columns...")),
                ];
                if let Some(key) =
                    sheet::entries_menu("resource_rates_menu", response, at, &entries)
                {
                    chosen = Some(key);
                }
            }),
        ) {
            self.views.resources.rate_row = Some(i);
        }
        // The selected row edits in place.
        let top = selected.and_then(|i| sheet::row_top(c, "resource_rates", 36., h - 68., i));
        if let (Some(i), Some(top)) = (selected, top) {
            let row = rate_rows[i];
            let mut x = 8.;
            for (column, (_, width, right)) in cols.iter().enumerate() {
                let key = format!("resource_rate_cell_{column}");
                let align = if *right { Align::Max } else { Align::Min };
                if let Some(text) = sheet::commit_cell(
                    c,
                    &key,
                    (x, top, *width, 18.),
                    &rows[i][column],
                    true,
                    align,
                ) {
                    self.edit_rate_cell(row, shown[column].3, &text);
                }
                x += width;
            }
        }
        let button = sheet::buttons(
            c,
            "resource_rates_buttons",
            5.,
            h - 26.,
            &[
                ("Add", true),
                ("Delete", rows.len() > 1 && selected.is_some()),
            ],
        );
        if chosen == Some("customize") {
            let shown_titles = shown.iter().map(|c| c.0.to_string()).collect();
            let all: Vec<&str> = RATE_COLUMNS.iter().map(|c| c.0).collect();
            self.open_view_columns(
                crate::details::grid_columns::ColumnTarget::ResourceRates,
                "Resource Rate",
                &all,
                &RATE_DEFAULT,
                shown_titles,
            );
        }
        let clicked = button.or(match chosen {
            Some("add") => Some(0),
            Some("delete") => Some(1),
            _ => None,
        });
        match clicked {
            Some(0) => {
                let origin = self.project.model.origin_serial;
                let day = self
                    .project
                    .model
                    .project
                    .data_date
                    .unwrap_or(0)
                    .div_euclid(DAY)
                    * DAY;
                let fallback = openxer_xer::date_string(origin, Some(day));
                edit::add_rate(&mut self.project.document, r.id, &fallback);
                let count = edit::rate_rows(&self.project.document, r.id).len();
                self.views.resources.rate_row = Some(count - 1);
                self.resources_changed();
            }
            Some(1) => {
                if let Some(i) = selected {
                    match edit::delete_rate(&mut self.project.document, rate_rows[i]) {
                        Ok(()) => {
                            self.views.resources.rate_row = Some(i.saturating_sub(1));
                            self.resources_changed();
                        }
                        Err(e) => self.status = e,
                    }
                }
            }
            _ => {}
        }
    }

    /// A committed edit of a Units & Prices cell: effective date, max units
    /// per time or price per unit.
    /// An edited rate cell: `column` is the RSRCRATE column it shows
    /// (`None`: the effective date).
    fn edit_rate_cell(&mut self, row: usize, column: Option<&str>, text: &str) {
        let doc = &mut self.project.document;
        let result = match column {
            None => {
                let keep =
                    openxer_xer::parse_absolute(&records::get(doc, "RSRCRATE", row, "start_date"));
                match format::parse_date(0, text, keep) {
                    Ok(Some(date)) => {
                        // Effective dates start at midnight.
                        edit::set_rate_date(doc, row, date.div_euclid(DAY) * DAY)
                    }
                    _ => Err("Enter an effective date.".into()),
                }
            }
            Some("max_qty_per_hr") => match crate::details::assignments::parse_units_per_time(text)
            {
                Some(v) => edit::set_rate_number(doc, row, "max_qty_per_hr", v),
                None => Err(format!(
                    "Enter max units per time such as {}.",
                    units_text(1.)
                )),
            },
            Some(price) => match parse_price(text) {
                Some(v) => edit::set_rate_number(doc, row, price, v),
                None => Err("Enter a price of zero or more.".into()),
            },
        };
        match result {
            Ok(()) => self.resources_changed(),
            Err(e) => self.status = e,
        }
    }

    fn resource_notes(&mut self, c: &mut Canvas, w: f32, h: f32, r: &Resource) {
        c.fill(6., 6., w - 12., 21., CHROME);
        c.bold(12., 6., 21., "Resource Notes");
        c.lowered(6., 27., w - 12., h - 27.);
        c.fill(7., 28., w - 14., h - 29. - 33., Color32::WHITE);
        let notes = super::wbs::memo_text(&rsrc_value(&self.project, r.id, "rsrc_notes"));
        let body = c.r(12., 32., w - 24., h - 76.);
        let editing = self
            .views
            .resources
            .notes_draft
            .as_ref()
            .is_some_and(|(id, _)| *id == r.id);
        if editing {
            let mut text = self
                .views
                .resources
                .notes_draft
                .take()
                .map(|(_, t)| t)
                .unwrap_or_default();
            let response = c.ui.put(
                body,
                egui::TextEdit::multiline(&mut text)
                    .id(c.id("resource_notes_edit"))
                    .frame(false)
                    .font(FontId::new(12., egui::FontFamily::Name("Arial".into())))
                    .text_color(INK)
                    .desired_width(body.width()),
            );
            crate::dialog_keys::note_keeps_enter(&response);
            c.act("resource_notes_edit", body);
            if response.lost_focus() {
                self.commit_notes(r.id, &text);
            } else {
                if !response.has_focus() {
                    response.request_focus();
                }
                self.views.resources.notes_draft = Some((r.id, text));
            }
        } else {
            let galley = c.ui.painter().layout(
                notes.clone(),
                FontId::new(12., egui::FontFamily::Name("Arial".into())),
                INK,
                body.width(),
            );
            c.ui.painter()
                .with_clip_rect(body.intersect(c.ui.clip_rect()))
                .galley(body.min, galley, INK);
        }
        match sheet::notes_buttons(c, "resource_notes", 10., h - 34., true, &notes) {
            Some(0) => {
                if let Some((id, text)) = self.views.resources.notes_draft.take() {
                    self.commit_notes(id, &text);
                } else {
                    self.views.resources.notes_draft = Some((r.id, notes));
                }
            }
            Some(2) => c.ui.ctx().copy_text(notes),
            _ => {}
        }
    }

    fn commit_notes(&mut self, rsrc: i64, text: &str) {
        let old = super::wbs::memo_text(&rsrc_value(&self.project, rsrc, "rsrc_notes"));
        if old != text {
            let value = if text.trim().is_empty() {
                String::new()
            } else {
                records::text_to_memo(text)
            };
            self.set_rsrc_value(rsrc, "rsrc_notes", value);
        }
    }

    /// Code type ids of the Codes grid rows, in the grid's order.
    fn resource_code_kinds(&self, r: &Resource) -> Vec<i64> {
        self.project.document.table("RSRCRCAT").map_or(vec![], |t| {
            t.rows
                .iter()
                .filter(|x| num(t.value(x, "rsrc_id")) == r.id)
                .map(|x| num(t.value(x, "rsrc_catg_type_id")))
                .collect()
        })
    }

    /// Role ids of the Roles grid rows, in the grid's order.
    fn resource_role_ids(&self, r: &Resource) -> Vec<i64> {
        self.project.document.table("RSRCROLE").map_or(vec![], |t| {
            t.rows
                .iter()
                .filter(|x| num(t.value(x, "rsrc_id")) == r.id)
                .map(|x| num(t.value(x, "role_id")))
                .collect()
        })
    }

    /// Open the dictionary dialog behind a picker in select mode: Admin
    /// Categories on Units of Measure, Currencies, Calendars or Roles. Its
    /// Select button sets the choice on resource `rsrc`; Close and Escape
    /// leave the resource as it was.
    fn open_resource_select(&mut self, pick: Pick, rsrc: i64) {
        let current = |column: &str| edit::value(&self.project.document, rsrc, column);
        match pick {
            Pick::Unit => {
                let unit = records::num(&current("unit_id"));
                self.open_dictionary(crate::dictionaries::Dictionary::AdminCategories);
                if let Some(a) = self.dictionaries.admin.as_mut() {
                    a.tab = crate::dictionaries::Category::Units;
                    a.selected = (unit != 0).then_some(unit);
                    a.select = Some((pick, rsrc));
                }
            }
            Pick::Currency => {
                let currency = records::num(&current("curr_id"));
                self.open_enterprise(crate::enterprise::Dialog::Currencies);
                let st = &mut self.enterprise.ui;
                st.selected = (currency != 0).then_some(currency);
                st.select = Some((pick, rsrc));
            }
            Pick::Role => {
                self.open_enterprise(crate::enterprise::Dialog::Roles);
                self.enterprise.ui.select = Some((pick, rsrc));
            }
            Pick::Calendar => {
                let calendar = current("clndr_id");
                self.open_calendars_to_select(pick, rsrc, &calendar);
            }
            Pick::Code => {}
        }
    }

    /// Set the choice a select-mode dictionary dialog made.
    pub(crate) fn apply_resource_pick(&mut self, pick: Pick, rsrc: i64, key: String) {
        // A choice made for the New Resource wizard's resource-to-be.
        if rsrc == crate::resource_wizard::WIZARD_RSRC {
            self.resource_wizard_pick(pick, key);
            return;
        }
        match pick {
            Pick::Unit => self.set_rsrc_value(rsrc, "unit_id", key),
            Pick::Currency => self.set_rsrc_value(rsrc, "curr_id", key),
            Pick::Calendar => self.set_rsrc_value(rsrc, "clndr_id", key),
            Pick::Role => {
                match edit::assign_role(&mut self.project.document, rsrc, records::num(&key)) {
                    Ok(()) => self.resources_changed(),
                    Err(e) => self.status = e,
                }
            }
            Pick::Code => {
                match edit::assign_code(&mut self.project.document, rsrc, records::num(&key)) {
                    Ok(()) => self.resources_changed(),
                    Err(e) => self.status = e,
                }
            }
        }
    }

    /// The window behind a picker or an Assign button. Unit of measure,
    /// currency, calendar and role open their dictionary dialogs in select
    /// mode; resource codes keep a plain list.
    pub(crate) fn resource_pick_window(&mut self, ctx: &egui::Context) {
        let Some((pick, rsrc)) = self.views.resources.pick else {
            return;
        };
        if pick != Pick::Code {
            self.views.resources.pick = None;
            self.open_resource_select(pick, rsrc);
            return;
        }
        let doc = &self.project.document;
        let options: Vec<(String, String)> = edit::code_values(doc)
            .into_iter()
            .map(|(id, _, kind, value, name)| (id.to_string(), format!("{kind}: {value}  {name}")))
            .collect();
        let mut open = true;
        let mut chosen: Option<String> = None;
        let mut close = false;
        crate::chrome::plain_window("Assign Resource Codes")
            .id(egui::Id::new("resource_pick"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_size([320., 300.])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(260.)
                    .show(ui, |ui| {
                        if options.is_empty() {
                            ui.label("Nothing to choose.");
                        }
                        for (i, (key, label)) in options.iter().enumerate() {
                            let response =
                                crate::dialog_kit::flow_selectable(ui, false, label, 290.);
                            self.action(&format!("resource_pick_item_{i}"), &response);
                            if response.clicked() {
                                chosen = Some(key.clone());
                            }
                        }
                    });
                ui.add_space(6.);
                let cancel = ui
                    .with_layout(Layout::right_to_left(Align::Min), |ui| {
                        crate::chrome::dialog_button(ui, &crate::chrome::DialogButton::cancel())
                    })
                    .inner;
                self.action("resource_pick_cancel", &cancel);
                if cancel.clicked() {
                    close = true;
                }
            });
        if let Some(key) = chosen {
            close = true;
            self.apply_resource_pick(pick, rsrc, key);
        }
        if !open || close || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.views.resources.pick = None;
        }
    }

    fn resource_code_rows(&self, r: &Resource) -> Vec<Vec<String>> {
        let d = &self.project.document;
        let types: BTreeMap<String, String> = d.table("RCATTYPE").map_or(BTreeMap::new(), |t| {
            t.rows
                .iter()
                .map(|x| {
                    (
                        t.value(x, "rsrc_catg_type_id").to_string(),
                        t.value(x, "rsrc_catg_type").to_string(),
                    )
                })
                .collect()
        });
        let values: BTreeMap<String, (String, String)> =
            d.table("RCATVAL").map_or(BTreeMap::new(), |t| {
                t.rows
                    .iter()
                    .map(|x| {
                        (
                            t.value(x, "rsrc_catg_id").to_string(),
                            (
                                t.value(x, "rsrc_catg_short_name").to_string(),
                                t.value(x, "rsrc_catg_name").to_string(),
                            ),
                        )
                    })
                    .collect()
            });
        d.table("RSRCRCAT").map_or(vec![], |t| {
            t.rows
                .iter()
                .filter(|x| num(t.value(x, "rsrc_id")) == r.id)
                .map(|x| {
                    let (short, name) = values
                        .get(t.value(x, "rsrc_catg_id"))
                        .cloned()
                        .unwrap_or_default();
                    vec![
                        types
                            .get(t.value(x, "rsrc_catg_type_id"))
                            .cloned()
                            .unwrap_or_default(),
                        short,
                        name,
                    ]
                })
                .collect()
        })
    }

    fn resource_role_rows(&self, r: &Resource) -> Vec<Vec<String>> {
        let d = &self.project.document;
        let roles: BTreeMap<String, (String, String)> =
            d.table("ROLES").map_or(BTreeMap::new(), |t| {
                t.rows
                    .iter()
                    .map(|x| {
                        (
                            t.value(x, "role_id").to_string(),
                            (
                                t.value(x, "role_short_name").to_string(),
                                t.value(x, "role_name").to_string(),
                            ),
                        )
                    })
                    .collect()
            });
        d.table("RSRCROLE").map_or(vec![], |t| {
            t.rows
                .iter()
                .filter(|x| num(t.value(x, "rsrc_id")) == r.id)
                .map(|x| {
                    let (id, name) = roles
                        .get(t.value(x, "role_id"))
                        .cloned()
                        .unwrap_or_else(|| (t.value(x, "role_id").to_string(), String::new()));
                    let primary = if t.value(x, "role_id") == r.role {
                        "Yes"
                    } else {
                        ""
                    };
                    vec![
                        id,
                        name,
                        t.value(x, "skill_level").to_string(),
                        primary.into(),
                    ]
                })
                .collect()
        })
    }

    fn resource_udf_rows(&self, r: &Resource) -> Vec<Vec<String>> {
        let d = &self.project.document;
        let types: BTreeMap<String, (String, String)> =
            d.table("UDFTYPE").map_or(BTreeMap::new(), |t| {
                t.rows
                    .iter()
                    .filter(|x| t.value(x, "table_name") == "RSRC")
                    .map(|x| {
                        (
                            t.value(x, "udf_type_id").to_string(),
                            (
                                t.value(x, "udf_type_label").to_string(),
                                t.value(x, "logical_data_type").to_string(),
                            ),
                        )
                    })
                    .collect()
            });
        let id = r.id.to_string();
        d.table("UDFVALUE").map_or(vec![], |t| {
            t.rows
                .iter()
                .filter(|x| t.value(x, "fk_id") == id)
                .filter_map(|x| {
                    let (label, kind) = types.get(t.value(x, "udf_type_id"))?.clone();
                    let value = [
                        t.value(x, "udf_text"),
                        t.value(x, "udf_number"),
                        t.value(x, "udf_date"),
                    ]
                    .into_iter()
                    .find(|v| !v.is_empty())
                    .unwrap_or("")
                    .to_string();
                    Some(vec![
                        label,
                        kind.trim_start_matches("FT_").to_string(),
                        value,
                    ])
                })
                .collect()
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Starter project with one labor resource (max 2/h) assigned 24 hours
    /// on the three-day survey activity (task 1010) and a child resource.
    pub(crate) fn with_resource() -> ProjectData {
        let mut doc = openxer_xer::starter().document;
        doc.table_mut("RSRC").add(&[
            ("rsrc_id", "7".into()),
            ("clndr_id", "1".into()),
            ("rsrc_short_name", "PIPE".into()),
            ("rsrc_name", "Pipefitter".into()),
            ("rsrc_type", "RT_Labor".into()),
            ("def_qty_per_hr", "1".into()),
            ("active_flag", "Y".into()),
            ("rsrc_seq_num", "2".into()),
        ]);
        doc.table_mut("RSRC").add(&[
            ("rsrc_id", "8".into()),
            ("parent_rsrc_id", "7".into()),
            ("clndr_id", "1".into()),
            ("rsrc_short_name", "PIPE.CRANE".into()),
            ("rsrc_name", "Crane".into()),
            ("rsrc_type", "RT_Equip".into()),
            ("rsrc_seq_num", "1".into()),
        ]);
        doc.table_mut("RSRC").add(&[
            ("rsrc_id", "9".into()),
            ("clndr_id", "1".into()),
            ("rsrc_short_name", "IDLE".into()),
            ("rsrc_name", "Unused".into()),
            ("rsrc_type", "RT_Mat".into()),
            ("rsrc_seq_num", "3".into()),
        ]);
        for (date, max, price) in [
            ("2026-01-01 00:00", "1", "40"),
            ("2026-10-01 00:00", "2", "55"),
        ] {
            doc.table_mut("RSRCRATE").add(&[
                ("rsrc_rate_id", max.to_string()),
                ("rsrc_id", "7".into()),
                ("max_qty_per_hr", max.into()),
                ("cost_per_qty", price.into()),
                ("start_date", date.into()),
            ]);
        }
        doc.table_mut("TASKRSRC").add(&[
            ("taskrsrc_id", "1".into()),
            ("task_id", "1010".into()),
            ("proj_id", "1".into()),
            ("rsrc_id", "7".into()),
            ("remain_qty", "24".into()),
            ("remain_qty_per_hr", "1".into()),
            ("target_qty", "24".into()),
            ("rollup_dates_flag", "N".into()),
        ]);
        let mut p = ProjectData::from_document(doc, 1, "rsrc".into()).unwrap();
        scheduler::schedule_with_resources(&mut p.model, &p.resources).unwrap();
        p
    }

    fn days(first: i64, count: i64) -> Vec<(i64, i64)> {
        (0..count)
            .map(|d| (first + d * DAY, first + (d + 1) * DAY))
            .collect()
    }

    #[test]
    fn dictionary_rows_use_the_rate_in_effect_and_tree_order() {
        let p = with_resource();
        let list = resources(&p);
        assert_eq!(list.len(), 3);
        let pipe = list.iter().find(|r| r.code == "PIPE").unwrap();
        assert_eq!(pipe.kind, ResourceType::Labor);
        assert_eq!(
            pipe.max_units,
            Some(2.),
            "the October rate is in effect on 2 Nov"
        );
        assert_eq!(pipe.price, Some(55.));
        assert_eq!(pipe.calendar, "Standard 5-day / 8-hour");
        assert_eq!(pipe.assignments, 1);
        assert_eq!(
            list.iter().find(|r| r.id == 8).unwrap().kind.label(),
            "Nonlabor"
        );
        assert_eq!(
            list.iter().find(|r| r.id == 9).unwrap().kind.label(),
            "Material"
        );
        assert_eq!(hierarchy(&list, false), [(7, 0), (8, 1), (9, 0)]);
        assert_eq!(hierarchy(&list, true), [(7, 0)]);
        assert_eq!(
            rate_at(&rates(&p, 7), 0),
            Some((1., 40.)),
            "before any rate, the first applies"
        );
    }

    /// The table's Hierarchy rows of list (id, depth): all resources, or
    /// (ssigned_only) the assigned ones and their parents.
    fn hierarchy(list: &[Resource], assigned_only: bool) -> Vec<(i64, usize)> {
        let p = with_resource();
        let assigned: BTreeSet<i64> = list
            .iter()
            .filter(|r| r.assignments > 0)
            .map(|r| r.id)
            .collect();
        let state = ListState {
            filter: if assigned_only {
                ListFilter::Project
            } else {
                ListFilter::All
            },
            group: ListGroup::Hierarchy,
            ..Default::default()
        };
        let data = ul::ListData {
            project: &p,
            resources: list,
            roles: &[],
            codes: &BTreeMap::new(),
            project_resources: &assigned,
            project_roles: &BTreeSet::new(),
        };
        ul::build(&data, &state)
            .rows
            .iter()
            .filter_map(|r| r.key.resource().map(|id| (id, r.depth)))
            .collect()
    }

    fn node(id: i64, parent: Option<i64>, assignments: usize) -> Resource {
        Resource {
            id,
            parent,
            code: format!("R{id}"),
            name: format!("Resource {id}"),
            kind: ResourceType::Labor,
            unit: String::new(),
            default_units: 1.,
            max_units: None,
            price: None,
            calendar: String::new(),
            active: true,
            sequence: id,
            assignments,
            role: String::new(),
            info: Default::default(),
        }
    }

    #[test]
    fn hierarchy_handles_20000_level_chains_loops_and_wide_tables() {
        use std::time::{Duration, Instant};
        let n = 20_000;
        let mut chain = vec![node(1, None, 0)];
        chain.extend((2..=n).map(|id| node(id, Some(id - 1), usize::from(id == n))));
        let started = Instant::now();
        let all = hierarchy(&chain, false);
        assert_eq!(all.len(), n as usize);
        assert_eq!(all[n as usize - 1], (n, n as usize - 1));
        // Assigned only: the one assigned leaf keeps all its 19,999 parents.
        assert_eq!(hierarchy(&chain, true).len(), n as usize);
        // A ring where every resource is the parent of the next.
        let ring: Vec<Resource> = (1..=n)
            .map(|id| node(id, Some(if id == 1 { n } else { id - 1 }), 1))
            .collect();
        let rows = hierarchy(&ring, true);
        assert_eq!(rows.len(), n as usize);
        assert_eq!(rows[0], (1, 0), "a loop is shown from its first member");
        // 20,000 children under one parent.
        let mut wide = vec![node(1, None, 0)];
        wide.extend((2..=n).map(|id| node(id, Some(1), 1)));
        assert_eq!(hierarchy(&wide, true).len(), n as usize);
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn histogram_buckets_remaining_units_over_work_time() {
        let p = with_resource();
        let a = p
            .model
            .activities
            .iter()
            .find(|a| a.task_id == 1010)
            .unwrap();
        let start = a.early_start.unwrap();
        let day0 = start.div_euclid(DAY) * DAY;
        let periods = days(day0 - DAY, 7);
        let u = usage(&p, 7, &periods, None);
        assert_eq!(u.remaining, [0., 8., 8., 8., 0., 0., 0.]);
        assert!(u.actual.iter().all(|v| *v == 0.));
        // Limit: 2 units/h over an 8-hour workday; none on the weekend.
        assert_eq!(u.limit[1..4], [16., 16., 16.]);
        assert_eq!(u.limit.iter().filter(|v| **v == 0.).count(), 2);
        let week = usage(&p, 7, &[(day0 - DAY, day0 + 6 * DAY)], None);
        assert_eq!(week.remaining, [24.]);
        // Without a price and limit row there is no limit (as leveling
        // reads it), so nothing shows overallocated.
        let mut bare = p.clone();
        bare.document.table_mut("RSRCRATE").rows.clear();
        let u = usage(&bare, 7, &periods, None);
        assert!(u.limit.iter().all(|v| v.is_infinite()), "{:?}", u.limit);
        let other = p
            .model
            .activities
            .iter()
            .position(|a| a.task_id == 1020)
            .unwrap();
        assert_eq!(usage(&p, 7, &periods, Some(other)).remaining, [0.; 7]);
        assert_eq!(usage(&p, 9, &periods, None).remaining, [0.; 7]);
    }

    #[test]
    fn histogram_splits_actual_and_remaining_for_progressed_work() {
        let mut p = with_resource();
        let i = p
            .model
            .activities
            .iter()
            .position(|a| a.task_id == 1010)
            .unwrap();
        let dd = p.model.project.data_date.unwrap();
        let day0 = dd.div_euclid(DAY) * DAY;
        // Started a day before the data date with 8 actual hours; 16 remain.
        let a = &mut p.model.activities[i];
        a.status = StatusType::InProgress;
        a.act_start = Some(dd - DAY);
        a.early_start = Some(dd);
        a.early_finish = Some(dd + DAY + 9 * 60);
        let table = p.document.table_mut("TASKRSRC");
        table.set(0, "act_reg_qty", "8".into());
        table.set(0, "remain_qty", "16".into());
        let periods = days(day0 - DAY, 3);
        let u = usage(&p, 7, &periods, None);
        assert_eq!(u.actual, [8., 0., 0.]);
        assert_eq!(u.remaining, [0., 8., 8.]);
    }

    #[test]
    fn spread_handles_partial_periods_and_zero_duration() {
        let p = with_resource();
        let cal = &p.model.calendars[0].cal;
        let dd = p.model.project.data_date.unwrap(); // Monday 08:00
        let day0 = dd.div_euclid(DAY) * DAY;
        let spread = openxer_xer::spread::even;
        let mut out = [0.; 2];
        // Monday 08:00 to Tuesday 12:00 is 12 work hours: 8 then 4.
        spread(
            12.,
            dd,
            day0 + DAY + 12 * 60,
            cal,
            &[(day0, day0 + DAY), (day0 + DAY, day0 + 2 * DAY)],
            &mut out,
        );
        assert_eq!(out, [8., 4.]);
        let mut out = [0.; 2];
        spread(
            5.,
            dd,
            dd,
            cal,
            &[(day0, day0 + DAY), (day0 + DAY, day0 + 2 * DAY)],
            &mut out,
        );
        assert_eq!(out, [5., 0.]);
        assert_eq!(nice_step(37.), 10.);
        assert_eq!(nice_step(8.), 2.);
        assert_eq!(nice_step(0.), 1.);
    }
}
