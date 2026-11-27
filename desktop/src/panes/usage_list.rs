//! The resource list at the left of the Resource Usage Profile and the
//! Resource Usage Spreadsheet: which resources (or roles) it lists (Filter
//! By), how it arranges them (Group and Sort By, a header click), its
//! columns, the selection (Ctrl and Shift add to it) and Display Activities
//! for selected Resource and Time Period. Each pane keeps its own choices,
//! saved with the layout; the selection is shared by both panes.
//!
//! A parent resource's values are its own: the hierarchy never rolls its
//! children's units up. A parent listed only because a child matches the
//! filter is drawn greyed and can still be selected, showing its own values.
use super::usage::SheetSort;
use super::usage_fields as fields;
use super::usage_menu::UsageMenu;
use super::*;
use crate::records::{self, num};
use crate::views::assignments as asg;
use crate::views::grid as vgrid;
use crate::views::record_view;
use crate::views::resources::{self as rs, Resource, ResourceType};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// A row of the list: a resource, a role or a group heading (its path).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ListKey {
    Resource(i64),
    Role(i64),
    Group(String),
}

impl ListKey {
    pub fn resource(&self) -> Option<i64> {
        match self {
            ListKey::Resource(id) => Some(*id),
            _ => None,
        }
    }
}

/// Filter By: which resources (or roles) the list shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ListFilter {
    All,
    /// Resources assigned to an activity, or named on a risk or issue, of
    /// the open projects (roles: those assignments ask for).
    #[default]
    Project,
    Custom,
}

impl ListFilter {
    pub const ALL: [ListFilter; 3] = [ListFilter::All, ListFilter::Project, ListFilter::Custom];
    pub fn key(self) -> &'static str {
        match self {
            ListFilter::All => "all",
            ListFilter::Project => "project",
            ListFilter::Custom => "custom",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.key() == key)
    }
    pub fn caption(self, view: ListView) -> &'static str {
        match (self, view) {
            (ListFilter::All, ListView::Resource) => "All Resources",
            (ListFilter::Project, ListView::Resource) => "Current Project's Resources",
            (ListFilter::Custom, ListView::Resource) => "Custom Resource Filter",
            (ListFilter::All, ListView::Role) => "All Roles",
            (ListFilter::Project, ListView::Role) => "Current Project's Roles",
            (ListFilter::Custom, ListView::Role) => "Custom Role Filter",
        }
    }
}

/// Group and Sort By. A new layout lists the resources flat (List); a
/// saved layout keeps the choice it was saved with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ListGroup {
    Hierarchy,
    #[default]
    List,
    Role,
    Type,
    /// By resource codes, one level per code type.
    Custom,
}

impl ListGroup {
    pub const ALL: [ListGroup; 5] = [
        ListGroup::Hierarchy,
        ListGroup::List,
        ListGroup::Role,
        ListGroup::Type,
        ListGroup::Custom,
    ];
    pub fn key(self) -> &'static str {
        match self {
            ListGroup::Hierarchy => "hierarchy",
            ListGroup::List => "list",
            ListGroup::Role => "role",
            ListGroup::Type => "type",
            ListGroup::Custom => "custom",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|g| g.key() == key)
    }
    pub fn caption(self) -> &'static str {
        match self {
            ListGroup::Hierarchy => "Hierarchy",
            ListGroup::List => "List",
            ListGroup::Role => "Primary Role",
            ListGroup::Type => "Resource Type",
            ListGroup::Custom => "Resource Codes",
        }
    }
}

/// Select View: resources, or roles and their demand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ListView {
    #[default]
    Resource,
    Role,
}

impl ListView {
    pub fn key(self) -> &'static str {
        match self {
            ListView::Resource => "resource",
            ListView::Role => "role",
        }
    }
}

/// Filter By > Customize...: a text match on ID or name, resource types, a
/// resource code value, and optionally only the open projects' resources.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CustomFilter {
    pub text: String,
    pub labor: bool,
    pub nonlabor: bool,
    pub material: bool,
    /// Resource code type name and value (empty: any).
    pub code_type: String,
    pub code_value: String,
    pub project_only: bool,
}

impl Default for CustomFilter {
    fn default() -> Self {
        Self {
            text: String::new(),
            labor: true,
            nonlabor: true,
            material: true,
            code_type: String::new(),
            code_value: String::new(),
            project_only: false,
        }
    }
}

impl CustomFilter {
    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "text": self.text,
            "labor": self.labor,
            "nonlabor": self.nonlabor,
            "material": self.material,
            "code_type": self.code_type,
            "code_value": self.code_value,
            "project_only": self.project_only,
        })
    }
    fn restore(&mut self, v: &serde_json::Value) {
        for (key, field) in [
            ("text", &mut self.text),
            ("code_type", &mut self.code_type),
            ("code_value", &mut self.code_value),
        ] {
            if let Some(s) = v[key].as_str() {
                *field = s.chars().take(200).collect();
            }
        }
        for (key, field) in [
            ("labor", &mut self.labor),
            ("nonlabor", &mut self.nonlabor),
            ("material", &mut self.material),
            ("project_only", &mut self.project_only),
        ] {
            if let Some(b) = v[key].as_bool() {
                *field = b;
            }
        }
    }
}

/// One level of Group and Sort By > Customize...: a resource code type and
/// the order of its values.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GroupLevel {
    pub code_type: String,
    pub descending: bool,
}

/// The list choices of one pane.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ListState {
    pub filter: ListFilter,
    pub custom: CustomFilter,
    pub group: ListGroup,
    pub levels: Vec<GroupLevel>,
    pub view: ListView,
    /// The column header sort, applied within each level.
    pub sort: SheetSort,
    /// Collapsed rows (not saved: ids belong to one project).
    pub collapsed: BTreeSet<ListKey>,
}

impl ListState {
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "filter": self.filter.key(),
            "custom": self.custom.json(),
            "group": self.group.key(),
            "levels": self.levels.iter().map(|l| serde_json::json!({
                "code_type": l.code_type,
                "descending": l.descending,
            })).collect::<Vec<_>>(),
            "view": self.view.key(),
            "sort": {"field": self.sort.field, "descending": self.sort.descending},
        })
    }
    pub fn restore(&mut self, v: &serde_json::Value) {
        if !v.is_object() {
            return;
        }
        if let Some(f) = v["filter"].as_str().and_then(ListFilter::from_key) {
            self.filter = f;
        }
        self.custom.restore(&v["custom"]);
        if let Some(g) = v["group"].as_str().and_then(ListGroup::from_key) {
            self.group = g;
        }
        if let Some(list) = v["levels"].as_array() {
            self.levels = list
                .iter()
                .filter_map(|l| {
                    Some(GroupLevel {
                        code_type: l["code_type"].as_str()?.chars().take(100).collect(),
                        descending: l["descending"].as_bool().unwrap_or(false),
                    })
                })
                .filter(|l| !l.code_type.is_empty())
                .take(MAX_LEVELS)
                .collect();
        }
        match v["view"].as_str() {
            Some("role") => self.view = ListView::Role,
            Some("resource") => self.view = ListView::Resource,
            _ => {}
        }
        let sort = &v["sort"];
        if sort.is_object() {
            self.sort.field = sort["field"]
                .as_str()
                .filter(|f| fields::known_resource_field(f))
                .map(str::to_string);
            self.sort.descending = sort["descending"].as_bool().unwrap_or(false);
        }
    }
}

/// Code levels Group and Sort By > Customize... offers.
pub const MAX_LEVELS: usize = 3;

/// The list a Filter By or Group and Sort By dialog edits: a bottom pane's
/// list or the Resources window's table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListOwner {
    Profile,
    Sheet,
    Resources,
}

impl ListOwner {
    pub fn pane(profile: bool) -> Self {
        if profile {
            ListOwner::Profile
        } else {
            ListOwner::Sheet
        }
    }
}

/// A role (ROLES row).
#[derive(Clone, Debug, PartialEq)]
pub struct Role {
    pub id: i64,
    pub parent: Option<i64>,
    pub code: String,
    pub name: String,
    pub seq: i64,
}

pub fn roles(p: &ProjectData) -> Vec<Role> {
    let Some(t) = p.document.table("ROLES") else {
        return vec![];
    };
    let mut out: Vec<Role> = t
        .rows
        .iter()
        .map(|r| {
            let id = num(t.value(r, "role_id"));
            Role {
                id,
                parent: t
                    .value(r, "parent_role_id")
                    .trim()
                    .parse()
                    .ok()
                    .filter(|x| *x != id && *x != 0),
                code: t.value(r, "role_short_name").to_string(),
                name: t.value(r, "role_name").to_string(),
                seq: num(t.value(r, "seq_num")),
            }
        })
        .collect();
    out.sort_by(|a, b| (a.seq, &a.code).cmp(&(b.seq, &b.code)));
    out
}

/// Resource code types: (id, name), in file order.
pub fn code_types(p: &ProjectData) -> Vec<(i64, String)> {
    p.document.table("RCATTYPE").map_or(vec![], |t| {
        t.rows
            .iter()
            .map(|r| {
                (
                    num(t.value(r, "rsrc_catg_type_id")),
                    t.value(r, "rsrc_catg_type").to_string(),
                )
            })
            .collect()
    })
}

/// The values of code type `name`, by value, in file order.
pub fn code_values_of(p: &ProjectData, name: &str) -> Vec<String> {
    let Some(kind) = code_types(p).into_iter().find(|t| t.1 == name).map(|t| t.0) else {
        return vec![];
    };
    p.document.table("RCATVAL").map_or(vec![], |t| {
        t.rows
            .iter()
            .filter(|r| num(t.value(r, "rsrc_catg_type_id")) == kind)
            .map(|r| t.value(r, "rsrc_catg_short_name").to_string())
            .collect()
    })
}

/// Resource code values per resource: rsrc id -> (code type name -> value).
pub fn resource_codes(p: &ProjectData) -> BTreeMap<i64, BTreeMap<String, String>> {
    let types: BTreeMap<i64, String> = code_types(p).into_iter().collect();
    let values: BTreeMap<i64, (i64, String)> =
        p.document.table("RCATVAL").map_or(BTreeMap::new(), |t| {
            t.rows
                .iter()
                .map(|r| {
                    (
                        num(t.value(r, "rsrc_catg_id")),
                        (
                            num(t.value(r, "rsrc_catg_type_id")),
                            t.value(r, "rsrc_catg_short_name").to_string(),
                        ),
                    )
                })
                .collect()
        });
    let mut out: BTreeMap<i64, BTreeMap<String, String>> = BTreeMap::new();
    if let Some(t) = p.document.table("RSRCRCAT") {
        for r in &t.rows {
            let value = values.get(&num(t.value(r, "rsrc_catg_id")));
            let kind = value
                .map(|v| v.0)
                .unwrap_or_else(|| num(t.value(r, "rsrc_catg_type_id")));
            if let (Some(name), Some((_, short))) = (types.get(&kind), value) {
                out.entry(num(t.value(r, "rsrc_id")))
                    .or_default()
                    .insert(name.clone(), short.clone());
            }
        }
    }
    out
}

/// Resource ids of `p` assigned to its activities or named on its risks
/// and issues.
fn own_project_resources(p: &ProjectData) -> BTreeSet<i64> {
    let doc = &p.document;
    let pid = p.model.project.proj_id;
    let mut out = BTreeSet::new();
    for (table, keys) in [
        ("TASKRSRC", &["rsrc_id"][..]),
        (
            records::risk_table(doc),
            &["rsrc_id", "identified_by_id"][..],
        ),
        ("PROJISSU", &["rsrc_id"][..]),
    ] {
        for row in record_view::project_rows(doc, table, pid) {
            for key in keys {
                let id = num(&records::get(doc, table, row, key));
                if id != 0 {
                    out.insert(id);
                }
            }
        }
    }
    out
}

/// Current Project's Resources: the resources of `p` used by `p` or (by
/// Resource ID) by one of the other open projects `others`.
pub fn project_resources(p: &ProjectData, others: &[&ProjectData]) -> BTreeSet<i64> {
    let mut out = own_project_resources(p);
    if others.is_empty() {
        return out;
    }
    let mine: BTreeMap<String, i64> = rs::resources(p)
        .into_iter()
        .map(|r| (r.code, r.id))
        .collect();
    for other in others {
        let codes: BTreeMap<i64, String> = rs::resources(other)
            .into_iter()
            .map(|r| (r.id, r.code))
            .collect();
        for id in own_project_resources(other) {
            if let Some(&mine) = codes.get(&id).and_then(|c| mine.get(c)) {
                out.insert(mine);
            }
        }
    }
    out
}

/// Role ids of `p` its assignments ask for.
fn own_project_roles(p: &ProjectData) -> BTreeSet<i64> {
    asg::assignments(p)
        .iter()
        .map(|a| num(&a.role))
        .filter(|r| *r != 0)
        .collect()
}

/// Current Project's Roles: the roles the open projects' assignments ask
/// for (other projects' roles matched by Role ID).
pub fn project_roles(p: &ProjectData, others: &[&ProjectData]) -> BTreeSet<i64> {
    let mut out = own_project_roles(p);
    let mine: BTreeMap<String, i64> = roles(p).into_iter().map(|r| (r.code, r.id)).collect();
    for other in others {
        let codes: BTreeMap<i64, String> =
            roles(other).into_iter().map(|r| (r.id, r.code)).collect();
        for id in own_project_roles(other) {
            if let Some(&mine) = codes.get(&id).and_then(|c| mine.get(c)) {
                out.insert(mine);
            }
        }
    }
    out
}

/// One row of the list as drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct ListRow {
    pub key: ListKey,
    pub depth: usize,
    /// A group heading's caption.
    pub label: String,
    /// Shown only as the parent of a match: greyed.
    pub context: bool,
    pub children: bool,
    pub collapsed: bool,
    /// A group heading's resources (every level below it).
    pub members: Vec<i64>,
}

impl ListRow {
    fn leaf(key: ListKey, depth: usize) -> Self {
        Self {
            key,
            depth,
            label: String::new(),
            context: false,
            children: false,
            collapsed: false,
            members: vec![],
        }
    }
}

/// The rows of a list and every row that has rows under it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lists {
    pub rows: Vec<ListRow>,
    pub parents: Vec<ListKey>,
}

impl Lists {
    pub fn row(&self, key: &ListKey) -> Option<&ListRow> {
        self.rows.iter().find(|r| &r.key == key)
    }
    /// The resources and roles selected rows stand for: a group stands for
    /// its members.
    pub fn members(&self, keys: &[ListKey]) -> (BTreeSet<i64>, BTreeSet<i64>) {
        let mut resources = BTreeSet::new();
        let mut roles = BTreeSet::new();
        for k in keys {
            match k {
                ListKey::Resource(id) => {
                    resources.insert(*id);
                }
                ListKey::Role(id) => {
                    roles.insert(*id);
                }
                ListKey::Group(_) => {
                    if let Some(r) = self.row(k) {
                        resources.extend(r.members.iter().copied());
                    }
                }
            }
        }
        (resources, roles)
    }
}

/// The rows of `lists` as the shared table (`views::grid`) draws them, with
/// each row's key. Group headings are bands spanning the table and the
/// rows under them carry their strips; the tree is in the first column.
/// `cells` gives a resource's or a role's icon and cells (`columns` of
/// them); `None` leaves the row out.
pub fn grid_rows(
    lists: &Lists,
    columns: usize,
    mut cells: impl FnMut(&ListKey) -> Option<(Option<chrome::Icon>, Vec<String>)>,
) -> (Vec<ListKey>, Vec<vgrid::Row>) {
    let grouped = lists
        .rows
        .iter()
        .any(|r| matches!(r.key, ListKey::Group(_)));
    let mut keys = vec![];
    let mut rows = vec![];
    for row in &lists.rows {
        let built = match &row.key {
            ListKey::Group(_) => {
                let mut texts = vec![String::new(); columns.max(1)];
                texts[0] = row.label.clone();
                vgrid::Row {
                    depth: row.depth,
                    expander: Some(!row.collapsed),
                    cells: texts,
                    band: true,
                    span: true,
                    strips: row.depth,
                    ..Default::default()
                }
            }
            key => {
                let Some((icon, texts)) = cells(key) else {
                    continue;
                };
                vgrid::Row {
                    depth: row.depth,
                    expander: row.children.then_some(!row.collapsed),
                    icon,
                    cells: texts,
                    strips: if grouped { row.depth } else { 0 },
                    ..Default::default()
                }
            }
        };
        keys.push(row.key.clone());
        rows.push(built);
    }
    (keys, rows)
}

/// A value to sort by: numbers before text, text without case.
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub enum SortValue {
    Empty,
    Number(f64),
    Text(String),
}

fn compare(a: &SortValue, b: &SortValue) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    use SortValue::*;
    match (a, b) {
        (Number(x), Number(y)) => x.total_cmp(y),
        (Text(x), Text(y)) => x.cmp(y),
        (Empty, Empty) => Equal,
        // Empty values go last either way round.
        (Empty, _) => Greater,
        (_, Empty) => Less,
        (Number(_), Text(_)) => Less,
        (Text(_), Number(_)) => Greater,
    }
}

/// Orders `ids` by the header sort (`values` holds each id's value),
/// keeping the given order for ties and when there is no sort.
fn sort_ids(ids: &mut [i64], values: &BTreeMap<i64, SortValue>, descending: bool) {
    if values.is_empty() {
        return;
    }
    ids.sort_by(|a, b| {
        let (x, y) = (
            values.get(a).unwrap_or(&SortValue::Empty),
            values.get(b).unwrap_or(&SortValue::Empty),
        );
        let o = compare(x, y);
        // Empty stays last when descending too.
        match (x, y) {
            (SortValue::Empty, _) | (_, SortValue::Empty) => o,
            _ if descending => o.reverse(),
            _ => o,
        }
    });
}

/// A tree of `items` (id, parent) in order: the rows with their depth.
/// `context` marks ids shown only as parents.
fn tree_rows(
    items: &[(i64, Option<i64>)],
    key: fn(i64) -> ListKey,
    context: &BTreeSet<i64>,
    collapsed: &BTreeSet<ListKey>,
    parents: &mut Vec<ListKey>,
) -> Vec<ListRow> {
    let ids: HashSet<i64> = items.iter().map(|i| i.0).collect();
    let mut children: HashMap<Option<i64>, Vec<i64>> = HashMap::new();
    for (id, parent) in items {
        children
            .entry(parent.filter(|p| ids.contains(p)))
            .or_default()
            .push(*id);
    }
    let mut out = vec![];
    let mut seen: HashSet<i64> = HashSet::new();
    let mut walk =
        |root: Option<i64>, depth: usize, out: &mut Vec<ListRow>, seen: &mut HashSet<i64>| {
            let mut stack: Vec<(Option<i64>, usize, usize)> = vec![(root, depth, 0)];
            while let Some((parent, depth, next)) = stack.pop() {
                let Some(&id) = children.get(&parent).and_then(|k| k.get(next)) else {
                    continue;
                };
                stack.push((parent, depth, next + 1));
                if !seen.insert(id) {
                    continue;
                }
                let has = children.get(&Some(id)).is_some_and(|k| !k.is_empty());
                let k = key(id);
                let folded = has && collapsed.contains(&k);
                if has {
                    parents.push(k.clone());
                }
                out.push(ListRow {
                    context: context.contains(&id),
                    children: has,
                    collapsed: folded,
                    ..ListRow::leaf(k, depth)
                });
                if folded {
                    // Hidden under a closed parent: not a loop to list.
                    let mut under = vec![id];
                    while let Some(x) = under.pop() {
                        for c in children.get(&Some(x)).into_iter().flatten() {
                            if seen.insert(*c) {
                                under.push(*c);
                            }
                        }
                    }
                } else {
                    stack.push((Some(id), depth + 1, 0));
                }
            }
        };
    walk(None, 0, &mut out, &mut seen);
    // Parent loops: their members at the top level.
    for (id, _) in items {
        if !seen.contains(id) {
            out.push(ListRow::leaf(key(*id), 0));
            seen.insert(*id);
            walk(Some(*id), 1, &mut out, &mut seen);
        }
    }
    out
}

/// Ids in `keep` plus their ancestors (`parent_of`); the ancestors that are
/// not in `keep` are returned apart, as context.
fn with_ancestors(
    keep: &BTreeSet<i64>,
    parent_of: &HashMap<i64, Option<i64>>,
) -> (BTreeSet<i64>, BTreeSet<i64>) {
    let mut all = keep.clone();
    let mut context = BTreeSet::new();
    for id in keep {
        let mut at = parent_of.get(id).copied().flatten();
        while let Some(p) = at {
            if !parent_of.contains_key(&p) || !all.insert(p) {
                break;
            }
            context.insert(p);
            at = parent_of.get(&p).copied().flatten();
        }
    }
    (all, context)
}

/// Everything the list needs about the open projects.
pub struct ListData<'a> {
    pub project: &'a ProjectData,
    pub resources: &'a [Resource],
    pub roles: &'a [Role],
    pub codes: &'a BTreeMap<i64, BTreeMap<String, String>>,
    pub project_resources: &'a BTreeSet<i64>,
    pub project_roles: &'a BTreeSet<i64>,
}

fn contains(hay: &str, needle: &str) -> bool {
    hay.to_lowercase().contains(needle)
}

/// The resources the filter lets through.
fn matching_resources(d: &ListData, s: &ListState) -> BTreeSet<i64> {
    let f = &s.custom;
    let text = f.text.trim().to_lowercase();
    d.resources
        .iter()
        .filter(|r| match s.filter {
            ListFilter::All => true,
            ListFilter::Project => d.project_resources.contains(&r.id),
            ListFilter::Custom => {
                (!f.project_only || d.project_resources.contains(&r.id))
                    && (text.is_empty() || contains(&r.code, &text) || contains(&r.name, &text))
                    && match r.kind {
                        ResourceType::Labor => f.labor,
                        ResourceType::Nonlabor => f.nonlabor,
                        ResourceType::Material => f.material,
                    }
                    && (f.code_type.is_empty()
                        || d.codes
                            .get(&r.id)
                            .and_then(|c| c.get(&f.code_type))
                            .is_some_and(|v| f.code_value.is_empty() || *v == f.code_value))
            }
        })
        .map(|r| r.id)
        .collect()
}

/// The header sort values of the resources.
fn resource_sort_values(d: &ListData, s: &ListState) -> BTreeMap<i64, SortValue> {
    let Some(field) = &s.sort.field else {
        return BTreeMap::new();
    };
    d.resources
        .iter()
        .map(|r| (r.id, fields::resource_sort_value(d.project, r, field)))
        .collect()
}

/// The list rows for state `s`.
pub fn build(d: &ListData, s: &ListState) -> Lists {
    match s.view {
        ListView::Role => build_roles(d, s),
        ListView::Resource => build_resources(d, s),
    }
}

fn build_resources(d: &ListData, s: &ListState) -> Lists {
    let matched = matching_resources(d, s);
    let values = resource_sort_values(d, s);
    let mut parents = vec![];
    let in_order = |set: &BTreeSet<i64>| -> Vec<i64> {
        let mut ids: Vec<i64> = d
            .resources
            .iter()
            .map(|r| r.id)
            .filter(|id| set.contains(id))
            .collect();
        // A flat list or a group has no sibling order of its own: by
        // Resource ID unless a column sorts it.
        if s.group != ListGroup::Hierarchy {
            let code: HashMap<i64, String> = d
                .resources
                .iter()
                .map(|r| (r.id, r.code.to_lowercase()))
                .collect();
            ids.sort_by(|a, b| code.get(a).cmp(&code.get(b)));
        }
        sort_ids(&mut ids, &values, s.sort.descending);
        ids
    };
    let rows = match s.group {
        ListGroup::Hierarchy => {
            let parent_of: HashMap<i64, Option<i64>> =
                d.resources.iter().map(|r| (r.id, r.parent)).collect();
            let (all, context) = with_ancestors(&matched, &parent_of);
            let items: Vec<(i64, Option<i64>)> = in_order(&all)
                .into_iter()
                .map(|id| (id, parent_of.get(&id).copied().flatten()))
                .collect();
            tree_rows(
                &items,
                ListKey::Resource,
                &context,
                &s.collapsed,
                &mut parents,
            )
        }
        ListGroup::List => in_order(&matched)
            .into_iter()
            .map(|id| ListRow::leaf(ListKey::Resource(id), 0))
            .collect(),
        ListGroup::Role | ListGroup::Type | ListGroup::Custom => {
            let by_id: HashMap<i64, &Resource> = d.resources.iter().map(|r| (r.id, r)).collect();
            let role_names: BTreeMap<String, (String, String)> = d
                .roles
                .iter()
                .map(|r| (r.id.to_string(), (r.code.clone(), r.name.clone())))
                .collect();
            // Each resource's group path: (sort value, caption) per level;
            // "None" groups carry no sort value and go last.
            let levels: Vec<(Option<String>, bool)> = match s.group {
                ListGroup::Custom => s
                    .levels
                    .iter()
                    .map(|l| (Some(l.code_type.clone()), l.descending))
                    .collect(),
                _ => vec![(None, false)],
            };
            let path = |r: &Resource| -> Vec<(Option<String>, String)> {
                match s.group {
                    ListGroup::Role => {
                        let role = r.role.trim();
                        vec![match role_names.get(role) {
                            Some((code, name)) => (
                                Some(code.to_lowercase()),
                                if name.is_empty() {
                                    code.clone()
                                } else {
                                    format!("{code} - {name}")
                                },
                            ),
                            None => (None, "(No Primary Role)".into()),
                        }]
                    }
                    ListGroup::Type => {
                        let rank = match r.kind {
                            ResourceType::Labor => "1",
                            ResourceType::Nonlabor => "2",
                            ResourceType::Material => "3",
                        };
                        vec![(Some(rank.into()), r.kind.label().into())]
                    }
                    _ => levels
                        .iter()
                        .map(|(kind, _)| {
                            let kind = kind.clone().unwrap_or_default();
                            match d.codes.get(&r.id).and_then(|c| c.get(&kind)) {
                                Some(v) => (Some(v.to_lowercase()), format!("{kind}: {v}")),
                                None => (None, format!("(No {kind})")),
                            }
                        })
                        .collect(),
                }
            };
            let ids = in_order(&matched);
            let items: Vec<Grouped> = ids
                .iter()
                .filter_map(|id| by_id.get(id).map(|r| (path(r), *id)))
                .collect();
            let mut out = vec![];
            group_rows(
                &items,
                0,
                &levels,
                s.group.key(),
                &s.collapsed,
                &mut parents,
                &mut out,
            );
            out
        }
    };
    Lists { rows, parents }
}

/// A resource and its group path: (sort value, caption) per level.
type Grouped = (Vec<(Option<String>, String)>, i64);

/// Group headings for `items` (path, id) at `level`, then their rows.
#[allow(clippy::too_many_arguments)]
fn group_rows(
    items: &[Grouped],
    level: usize,
    levels: &[(Option<String>, bool)],
    prefix: &str,
    collapsed: &BTreeSet<ListKey>,
    parents: &mut Vec<ListKey>,
    out: &mut Vec<ListRow>,
) {
    if level >= levels.len() || items.iter().all(|(path, _)| path.len() <= level) {
        out.extend(
            items
                .iter()
                .map(|(_, id)| ListRow::leaf(ListKey::Resource(*id), level)),
        );
        return;
    }
    // Groups in value order (descending when asked), "None" last.
    let mut groups: Vec<(Option<String>, String)> = vec![];
    for (path, _) in items {
        if !groups.contains(&path[level]) {
            groups.push(path[level].clone());
        }
    }
    let descending = levels[level].1;
    groups.sort_by(|a, b| match (&a.0, &b.0) {
        (None, None) => a.1.cmp(&b.1),
        (None, _) => std::cmp::Ordering::Greater,
        (_, None) => std::cmp::Ordering::Less,
        (Some(x), Some(y)) if descending => y.cmp(x),
        (Some(x), Some(y)) => x.cmp(y),
    });
    for group in groups {
        let inside: Vec<Grouped> = items
            .iter()
            .filter(|(path, _)| path[level] == group)
            .cloned()
            .collect();
        let path_key = format!("{prefix}:{}:{}", level, group.1);
        let key = ListKey::Group(path_key.clone());
        let folded = collapsed.contains(&key);
        parents.push(key.clone());
        out.push(ListRow {
            label: group.1.clone(),
            children: true,
            collapsed: folded,
            members: inside.iter().map(|(_, id)| *id).collect(),
            ..ListRow::leaf(key, level)
        });
        if !folded {
            group_rows(
                &inside,
                level + 1,
                levels,
                &path_key,
                collapsed,
                parents,
                out,
            );
        }
    }
}

fn build_roles(d: &ListData, s: &ListState) -> Lists {
    let f = &s.custom;
    let text = f.text.trim().to_lowercase();
    let matched: BTreeSet<i64> = d
        .roles
        .iter()
        .filter(|r| match s.filter {
            ListFilter::All => true,
            ListFilter::Project => d.project_roles.contains(&r.id),
            ListFilter::Custom => {
                (!f.project_only || d.project_roles.contains(&r.id))
                    && (text.is_empty() || contains(&r.code, &text) || contains(&r.name, &text))
            }
        })
        .map(|r| r.id)
        .collect();
    let values: BTreeMap<i64, SortValue> = match s.sort.field.as_deref() {
        Some("resource_name") => d
            .roles
            .iter()
            .map(|r| (r.id, SortValue::Text(r.name.to_lowercase())))
            .collect(),
        Some(_) => d
            .roles
            .iter()
            .map(|r| (r.id, SortValue::Text(r.code.to_lowercase())))
            .collect(),
        None => BTreeMap::new(),
    };
    let parent_of: HashMap<i64, Option<i64>> = d.roles.iter().map(|r| (r.id, r.parent)).collect();
    let flat = s.group == ListGroup::List;
    let (all, context) = if flat {
        (matched.clone(), BTreeSet::new())
    } else {
        with_ancestors(&matched, &parent_of)
    };
    let mut ids: Vec<i64> = d
        .roles
        .iter()
        .map(|r| r.id)
        .filter(|id| all.contains(id))
        .collect();
    sort_ids(&mut ids, &values, s.sort.descending);
    let mut parents = vec![];
    let rows = if flat {
        ids.into_iter()
            .map(|id| ListRow::leaf(ListKey::Role(id), 0))
            .collect()
    } else {
        let items: Vec<(i64, Option<i64>)> = ids
            .into_iter()
            .map(|id| (id, parent_of.get(&id).copied().flatten()))
            .collect();
        tree_rows(&items, ListKey::Role, &context, &s.collapsed, &mut parents)
    };
    Lists { rows, parents }
}

/// Role demand per period: actual units, and remaining units of the
/// assignments that name a resource (staffed) or only the role (unstaffed).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoleDemand {
    pub actual: Vec<f64>,
    pub staffed: Vec<f64>,
    pub unstaffed: Vec<f64>,
}

pub fn role_demand(
    p: &ProjectData,
    roles: &BTreeSet<i64>,
    periods: &[(i64, i64)],
    costs: bool,
) -> RoleDemand {
    let n = periods.len();
    let mut out = RoleDemand {
        actual: vec![0.; n],
        staffed: vec![0.; n],
        unstaffed: vec![0.; n],
    };
    let known: HashSet<i64> = rs::resources(p).into_iter().map(|r| r.id).collect();
    for a in asg::assignments(p) {
        if !roles.contains(&num(&a.role)) {
            continue;
        }
        let a = if costs { a.as_costs() } else { a };
        asg::actual_into(p, &a, periods, &mut out.actual);
        let target = if a.rsrc != 0 && known.contains(&a.rsrc) {
            &mut out.staffed
        } else {
            &mut out.unstaffed
        };
        asg::remaining_into(p, &a, periods, target);
    }
    out
}

/// What a resource's units measure: time (labor and nonlabor hours), or a
/// material's own unit of measure.
fn measure(r: &Resource) -> Option<&str> {
    match r.kind {
        ResourceType::Material => Some(r.unit.trim()),
        _ => None,
    }
}

/// Kinds of units that cannot be added: material with labor or nonlabor,
/// or materials in different units of measure.
pub fn mixed_units(resources: &[Resource], ids: &BTreeSet<i64>) -> bool {
    let kinds: BTreeSet<Option<&str>> = resources
        .iter()
        .filter(|r| ids.contains(&r.id))
        .map(measure)
        .collect();
    kinds.len() > 1
}

/// The unit of measure of `ids` when every one is a material in that unit
/// (it may be blank): their quantities are counts of it, not hours. `None`
/// when any is labor or nonlabor, or the units differ.
pub fn material_unit(resources: &[Resource], ids: &BTreeSet<i64>) -> Option<String> {
    let mut kinds = resources
        .iter()
        .filter(|r| ids.contains(&r.id))
        .map(measure);
    let first = kinds.next()??;
    kinds.all(|k| k == Some(first)).then(|| first.to_string())
}

/// A material quantity in its unit of measure ("80 CY"), as the views
/// show it; blank for nothing.
pub fn material_text(v: f64, unit: &str) -> String {
    if v.abs() < 0.005 {
        return String::new();
    }
    let n = crate::records::format_number((v * 100.).round() / 100.);
    if unit.is_empty() {
        n
    } else {
        format!("{n} {unit}")
    }
}

/// The note shown instead of summed units of different kinds.
pub const MIXED_NOTE: &str = "Material units do not add to labor or nonlabor units, or to \
    another material's unit of measure. Select resources of one kind, or show costs.";

/// The Display Activities for selected controls under the list.
pub const CONTROLS_H: f32 = 40.;

impl App {
    /// The list choices of the profile's or the spreadsheet's list.
    pub(crate) fn list_state(&self, profile: bool) -> &ListState {
        if profile {
            &self.panes.usage.profile_list
        } else {
            &self.panes.usage.sheet_list
        }
    }

    pub(crate) fn list_state_mut(&mut self, profile: bool) -> &mut ListState {
        if profile {
            &mut self.panes.usage.profile_list
        } else {
            &mut self.panes.usage.sheet_list
        }
    }

    /// The list choices a Customize dialog edits.
    pub(crate) fn owner_list_mut(&mut self, owner: ListOwner) -> &mut ListState {
        match owner {
            ListOwner::Profile => &mut self.panes.usage.profile_list,
            ListOwner::Sheet => &mut self.panes.usage.sheet_list,
            ListOwner::Resources => &mut self.views.resources.list,
        }
    }

    pub(crate) fn others(&self) -> Vec<&ProjectData> {
        self.open_set.parked.iter().map(|p| &p.project).collect()
    }

    /// The resources of the open project, memoized.
    pub(crate) fn usage_resources(&self) -> Rc<Vec<Resource>> {
        self.memo(self.project_serial(), "usage_list_resources", (), || {
            rs::resources(&self.project)
        })
    }

    /// The rows of a pane's list.
    pub(crate) fn usage_list_rows(&self, profile: bool) -> Rc<Lists> {
        let state = self.list_state(profile).clone();
        let serial = self.project_serial();
        let key = (state.clone(), self.open_set.version);
        self.memo(serial, "usage_list_rows", key, || {
            let resources = self.usage_resources();
            let others = self.others();
            let pr = self.memo(serial, "usage_list_project", self.open_set.version, || {
                (
                    project_resources(&self.project, &others),
                    project_roles(&self.project, &others),
                )
            });
            let role_list = roles(&self.project);
            let codes = resource_codes(&self.project);
            build(
                &ListData {
                    project: &self.project,
                    resources: &resources,
                    roles: &role_list,
                    codes: &codes,
                    project_resources: &pr.0,
                    project_roles: &pr.1,
                },
                &state,
            )
        })
    }

    /// The resources and roles the selection of a pane's list stands for.
    pub(crate) fn usage_members(&self, profile: bool) -> (BTreeSet<i64>, BTreeSet<i64>) {
        let lists = self.usage_list_rows(profile);
        lists.members(&self.panes.usage.selection())
    }

    /// Selects `key` alone, or (Ctrl) toggles it, or (Shift) selects the
    /// rows from the last one clicked.
    fn pick_list_row(&mut self, rows: &[ListRow], k: usize, modifiers: egui::Modifiers) {
        let key = rows[k].key.clone();
        let u = &mut self.panes.usage;
        let mut picked = u.selection();
        if modifiers.shift {
            let from = u
                .anchor
                .as_ref()
                .and_then(|a| rows.iter().position(|r| &r.key == a))
                .unwrap_or(k);
            let (a, b) = (from.min(k), from.max(k));
            picked = rows[a..=b].iter().map(|r| r.key.clone()).collect();
        } else if modifiers.command || modifiers.ctrl {
            if let Some(i) = picked.iter().position(|p| *p == key) {
                if picked.len() > 1 {
                    picked.remove(i);
                }
            } else {
                picked.push(key.clone());
            }
            u.anchor = Some(key.clone());
        } else {
            picked = vec![key.clone()];
            u.anchor = Some(key.clone());
        }
        // `resource` is the clicked resource while it is selected, else the
        // last selected one; it is always one of `picked`.
        u.resource = key
            .resource()
            .filter(|_| picked.contains(&key))
            .or_else(|| picked.iter().rev().find_map(|p| p.resource()));
        u.picked = picked;
    }

    /// The resource list of the resource usage views. `profile` is the
    /// Resource Usage Profile's list (its own fields and choices).
    pub(crate) fn usage_resource_list(&mut self, ui: &mut egui::Ui, rect: Rect, profile: bool) {
        use super::usage::{DISPLAY_H, HEADER_H};
        let kind = if profile {
            UsageMenu::ResourceProfile
        } else {
            UsageMenu::ResourceSheet
        };
        let pane = if profile { "profile" } else { "sheet" };
        let printing = self.panes.printing;
        let p = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        p.rect_filled(rect, 0., Color32::WHITE);
        let state = self.list_state(profile).clone();
        // The Display Options bar: Filter By, Group and Sort By, Columns,
        // Select View.
        let bar = Rect::from_min_size(rect.min, Vec2::new(rect.width(), DISPLAY_H));
        let caption = format!("Display: {}", state.filter.caption(state.view));
        let bar_name = format!("usage_list_bar_{pane}");
        if printing {
            p.rect_filled(bar, 0., Color32::WHITE);
            p.text(
                Pos2::new(bar.left() + 4., bar.center().y),
                Align2::LEFT_CENTER,
                &caption,
                chrome::table_font(),
                INK,
            );
        } else {
            let button = crate::views::sheet::header_button(ui, &bar_name, bar, &caption);
            self.action(&bar_name, &button);
            egui::Popup::menu(&button)
                .style(chrome::popup_menu_style)
                .show(|ui| {
                    let mut m = crate::context_menu::Menu::on(ui, &bar_name);
                    self.usage_list_items(&mut m, profile);
                });
        }
        let controls_h = if printing { 0. } else { CONTROLS_H };
        let list = Rect::from_min_max(
            Pos2::new(rect.left(), bar.bottom()),
            Pos2::new(rect.right(), (rect.bottom() - controls_h).max(bar.bottom())),
        );
        let background = ui.interact(
            list,
            ui.id().with(("usage_resource_list", profile)),
            Sense::click(),
        );
        self.usage_list_menu(&background, profile, kind);
        let lists = self.usage_list_rows(profile);
        let rows = &lists.rows;
        // Nothing selected that this list shows: the first row.
        let selection = self.panes.usage.selection();
        if !rows.iter().any(|r| selection.contains(&r.key)) {
            if let Some(first) = rows.first() {
                self.panes.usage.picked = vec![first.key.clone()];
                self.panes.usage.resource = first.key.resource();
                self.panes.usage.anchor = Some(first.key.clone());
            } else {
                self.panes.usage.picked.clear();
                self.panes.usage.resource = None;
            }
        }
        let selection = self.panes.usage.selection();
        let shown_fields = self.list_fields(profile);
        let ix = usize::from(!profile);
        let role_view = state.view == ListView::Role;
        // Widths dragged in the table, by field (a role column apart).
        let width_key = |f: &fields::Field| {
            if role_view {
                format!("role:{}", f.id)
            } else {
                f.id.clone()
            }
        };
        let widths: Vec<f32> = shown_fields
            .iter()
            .map(|f| {
                self.panes.usage.list_widths[ix]
                    .get(&width_key(f))
                    .copied()
                    .unwrap_or(f.width)
            })
            .collect();
        let mut columns = vgrid::Columns::new(&widths);
        columns.scroll_x = self.panes.usage.list_scroll_x[ix];
        let titles: Vec<&str> = shown_fields.iter().map(|f| f.title.as_str()).collect();
        let right: Vec<usize> = shown_fields
            .iter()
            .enumerate()
            .filter(|(_, f)| !role_view && fields::NUMBER_FIELDS.contains(&f.id.as_str()))
            .map(|(i, _)| i)
            .collect();
        // The rows as the shared table draws them, kept while the list,
        // its columns and the formats are unchanged.
        let key = (
            state.clone(),
            self.open_set.version,
            shown_fields
                .iter()
                .map(|f| f.id.clone())
                .collect::<Vec<_>>(),
            format!("{:?}", crate::format::current()),
        );
        let rows = self.memo(self.project_serial(), "usage_list_grid_rows", key, || {
            let resources = self.usage_resources();
            let role_list = roles(&self.project);
            let (_, mut rows) = grid_rows(&lists, shown_fields.len(), |key| {
                Some(match key {
                    ListKey::Resource(id) => match resources.iter().find(|r| r.id == *id) {
                        Some(r) => (
                            Some(r.kind.icon()),
                            shown_fields
                                .iter()
                                .map(|f| fields::resource_text(&self.project, r, &f.id))
                                .collect(),
                        ),
                        None => (None, vec![]),
                    },
                    ListKey::Role(id) => (
                        None,
                        role_list
                            .iter()
                            .find(|r| r.id == *id)
                            .map_or(vec![], |r| vec![r.code.clone(), r.name.clone()]),
                    ),
                    ListKey::Group(_) => (None, vec![]),
                })
            });
            // Every list row has its table row: parents listed only for a
            // match are greyed.
            for (row, listed) in rows.iter_mut().zip(&lists.rows) {
                row.dim = listed.context;
            }
            rows
        });
        let painted = vgrid::Selection {
            rows: rows
                .iter()
                .zip(&lists.rows)
                .enumerate()
                .filter(|(_, (_, r))| selection.contains(&r.key))
                .map(|(k, _)| k)
                .collect(),
            current: None,
            reveal: false,
        };
        let table_id = format!("usage_list_{pane}");
        let table = vgrid::Table {
            id: &table_id,
            titles: &titles,
            right: &right,
            tree_field: 0,
            header_height: HEADER_H,
            row_lines: true,
            band_pitch: vgrid::BandPitch::Fixed(vgrid::ROW_HEIGHT),
            band_caption: &[],
            menu: None,
            open_menu: false,
            chart_menu: None,
        };
        let sorted = state
            .sort
            .field
            .as_ref()
            .and_then(|f| shown_fields.iter().position(|x| &x.id == f))
            .map(|c| (c, state.sort.descending));
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(list)
                .layout(Layout::top_down(Align::LEFT)),
        );
        child.set_clip_rect(list.intersect(ui.clip_rect()));
        child.spacing_mut().item_spacing = Vec2::ZERO;
        let mut scroll = self.panes.usage.list_scroll[ix];
        let response = vgrid::show_with(
            &mut child,
            &table,
            &mut columns,
            &rows,
            &painted,
            None,
            None,
            &mut self.action_rects,
            vgrid::Extras {
                line_numbers: self.panes.usage.line_numbers,
                sorted,
                scroll: Some(&mut scroll),
                ..Default::default()
            },
        );
        let u = &mut self.panes.usage;
        u.list_scroll[ix] = scroll;
        u.list_scroll_x[ix] = columns.scroll_x;
        // A dragged edge keeps its width; a dragged title moves its field.
        for (c, field) in shown_fields.iter().enumerate() {
            if columns.widths[c] != widths[c] {
                u.list_widths[ix].insert(width_key(field), columns.widths[c]);
            }
        }
        if !role_view && columns.order.iter().enumerate().any(|(i, c)| i != *c) {
            let ids = columns
                .order
                .iter()
                .map(|c| shown_fields[*c].id.clone())
                .collect();
            if profile {
                u.profile_fields = ids;
            } else {
                u.resource_fields = ids;
            }
        }
        if rows.is_empty() {
            let note = match state.view {
                ListView::Resource => "No resources match the filter.",
                ListView::Role => "No roles match the filter.",
            };
            vgrid::empty_note(&child, &response, &table_id, note, &mut self.action_rects);
        }
        if printing {
            return;
        }
        // The names the tests and the QA sweep know the list's parts by.
        let alias = |app: &mut Self, from: String, to: String, grow: f32| {
            if let Some(r) = app.action_rects.get(&from).copied() {
                let r = [r[0] - grow, r[1] - grow, r[2] + grow, r[3] + grow];
                app.action_rects.insert(to, r);
            }
        };
        for (k, _) in &response.rows {
            alias(
                self,
                format!("{table_id}_row_{k}"),
                format!("usage_resource_{k}"),
                0.,
            );
            alias(
                self,
                format!("{table_id}_expand_{k}"),
                format!("usage_list_toggle_{pane}_{k}"),
                2.,
            );
        }
        for (c, _) in &response.titles {
            alias(
                self,
                format!("{table_id}_col_{c}"),
                format!("usage_list_head_{pane}_{c}"),
                0.,
            );
        }
        // A title click sorts by the column (again: the other way).
        if let Some(field) = response.header_clicked.and_then(|c| shown_fields.get(c)) {
            let sort = &mut self.list_state_mut(profile).sort;
            if sort.field.as_deref() == Some(field.id.as_str()) {
                sort.descending = !sort.descending;
            } else {
                sort.field = Some(field.id.clone());
                sort.descending = false;
            }
        }
        // The expand box toggles; elsewhere a click selects, and a
        // right-click outside the selection selects the row alone.
        let list_rows = &lists.rows;
        if let Some(k) = response.toggled.filter(|k| *k < list_rows.len()) {
            let key = list_rows[k].key.clone();
            let collapsed = &mut self.list_state_mut(profile).collapsed;
            if !collapsed.remove(&key) {
                collapsed.insert(key);
            }
        }
        if let Some(k) = response.clicked.filter(|k| *k < list_rows.len()) {
            if !response.secondary {
                self.pick_list_row(list_rows, k, response.modifiers);
            } else if !selection.contains(&list_rows[k].key) {
                self.pick_list_row(list_rows, k, Default::default());
            }
        }
        for (_, r) in response.titles.iter().chain(&response.rows) {
            self.usage_list_menu(r, profile, kind);
        }
        self.usage_list_controls(ui, rect, controls_h, pane);
    }

    /// Display Activities for selected Resource and Time Period, under the
    /// list. Off while Show All Projects is on (the activity table holds
    /// only this project).
    fn usage_list_controls(&mut self, ui: &mut egui::Ui, rect: Rect, h: f32, pane: &str) {
        let area = Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - h), rect.max);
        ui.painter().rect_filled(area, 0., CHROME);
        chrome::hline(
            ui.painter(),
            area.left(),
            area.right(),
            area.top(),
            chrome::pal::SHADOW,
        );
        let mut controls = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(area.shrink2(Vec2::new(6., 3.)))
                .layout(Layout::top_down(Align::LEFT)),
        );
        controls.spacing_mut().item_spacing.y = 1.;
        controls.label(RichText::new("Display Activities for selected").font(chrome::table_font()));
        let blocked = self.panes.usage.show_all_projects;
        let tip = "Turn off Show All Projects to filter the activities: the table lists this project only.";
        controls.horizontal(|ui| {
            let mut resource = self.panes.usage.resource_filter;
            let r = crate::dialog_kit::flow_checkbox(ui, &mut resource, "Resource", !blocked)
                .on_disabled_hover_text(tip);
            self.action(&format!("usage_list_{pane}_resource"), &r);
            self.panes.usage.resource_filter = resource;
            let mut period = self.panes.usage.time_period;
            let r = crate::dialog_kit::flow_checkbox(ui, &mut period, "Time Period", !blocked)
                .on_disabled_hover_text(tip)
                .on_hover_text("Click a period on the profile or a spreadsheet column to pick it.");
            self.action(&format!("usage_list_{pane}_time_period"), &r);
            self.panes.usage.time_period = period;
        });
    }

    /// The fields the list shows: the pane's resource fields, or the role
    /// view's ID and name.
    pub(crate) fn list_fields(&self, profile: bool) -> Vec<fields::Field> {
        let u = &self.panes.usage;
        if self.list_state(profile).view == ListView::Role {
            return vec![
                fields::Field {
                    id: "resource_id".into(),
                    title: "Role ID".into(),
                    width: 120.,
                    column: None,
                },
                fields::Field {
                    id: "resource_name".into(),
                    title: "Role Name".into(),
                    width: 150.,
                    column: None,
                },
            ];
        }
        let (which, ids) = if profile {
            (fields::Which::ProfileResource, &u.profile_fields)
        } else {
            (fields::Which::Resource, &u.resource_fields)
        };
        fields::fields_of(&self.project, which, ids)
    }

    /// The list's right-click menu: its own items, then the pane's.
    fn usage_list_menu(&mut self, response: &egui::Response, profile: bool, kind: UsageMenu) {
        // The pane's own prefix: the list adds its items to the pane's menu.
        crate::context_menu::ContextMenu::new(kind.prefix())
            .min_width(210.)
            .show(response, self, |app, m| {
                app.usage_list_items(m, profile);
                m.separator();
                app.usage_menu_items(m, kind);
            });
    }

    /// Filter By, Group and Sort By, Columns, Select View, Expand All and
    /// Collapse All.
    pub(crate) fn usage_list_items(&mut self, m: &mut crate::context_menu::Menu, profile: bool) {
        use chrome::{Icon, Item};
        let s = self.list_state(profile).clone();
        let roles_view = s.view == ListView::Role;
        m.submenu(
            "filter_by",
            Item::new("Filter By").icon(Icon::Filter),
            |m| {
                for f in [ListFilter::All, ListFilter::Project] {
                    let item = Item::new(f.caption(s.view)).checked(s.filter == f);
                    if m.item(&format!("filter_{}", f.key()), item) {
                        self.list_state_mut(profile).filter = f;
                    }
                }
                m.separator();
                let item = Item::new("Customize...").checked(s.filter == ListFilter::Custom);
                if m.item("filter_custom", item) {
                    self.panes.usage_list_filter =
                        Some((ListOwner::pane(profile), s.custom.clone()));
                }
            },
        );
        m.submenu(
            "group_by",
            Item::new("Group and Sort By").icon(Icon::GroupSort),
            |m| {
                for g in [
                    ListGroup::Hierarchy,
                    ListGroup::List,
                    ListGroup::Role,
                    ListGroup::Type,
                ] {
                    let enabled =
                        !roles_view || matches!(g, ListGroup::Hierarchy | ListGroup::List);
                    let item = Item::new(g.caption())
                        .checked(s.group == g)
                        .enabled(enabled);
                    if m.item(&format!("group_{}", g.key()), item) {
                        self.list_state_mut(profile).group = g;
                    }
                }
                m.separator();
                let item = Item::new("Customize...")
                    .checked(s.group == ListGroup::Custom)
                    .enabled(!roles_view);
                if m.item("group_custom", item) {
                    let mut levels = s.levels.clone();
                    if levels.is_empty() {
                        levels.push(GroupLevel {
                            code_type: String::new(),
                            descending: false,
                        });
                    }
                    self.panes.usage_list_group = Some((ListOwner::pane(profile), levels));
                }
                m.separator();
                let item = Item::new("Clear Sort").enabled(s.sort.field.is_some());
                if m.item("sort_clear", item) {
                    self.list_state_mut(profile).sort = SheetSort::default();
                }
            },
        );
        m.submenu("columns", Item::new("Columns").icon(Icon::Columns), |m| {
            let item = Item::new("Customize...").enabled(!roles_view);
            if m.item("columns_customize", item) {
                self.open_spreadsheet_fields(if profile {
                    fields::FieldsView::ResourceProfile
                } else {
                    fields::FieldsView::ResourceSheet
                });
            }
        });
        m.submenu("select_view", Item::new("Select View"), |m| {
            for (key, label, view) in [
                ("view_resource", "By Resource", ListView::Resource),
                ("view_role", "By Role", ListView::Role),
            ] {
                if m.item(key, Item::new(label).checked(s.view == view)) {
                    let state = self.list_state_mut(profile);
                    if state.view != view {
                        state.view = view;
                        state.collapsed.clear();
                        state.sort = SheetSort::default();
                        if view == ListView::Role
                            && !matches!(state.group, ListGroup::Hierarchy | ListGroup::List)
                        {
                            state.group = ListGroup::default();
                        }
                    }
                }
            }
        });
        m.separator();
        let lists = self.usage_list_rows(profile);
        let any = !lists.parents.is_empty() || !s.collapsed.is_empty();
        if m.item("expand_all", Item::new("Expand All").enabled(any)) {
            self.list_state_mut(profile).collapsed.clear();
        }
        if m.item("collapse_all", Item::new("Collapse All").enabled(any)) {
            // Rows under collapsed rows are not built: open all first.
            self.list_state_mut(profile).collapsed.clear();
            let all = self.usage_list_rows(profile);
            self.list_state_mut(profile).collapsed = all.parents.iter().cloned().collect();
        }
    }

    /// Filter By > Customize... and Group and Sort By > Customize...
    pub(crate) fn usage_list_dialogs(&mut self, ctx: &egui::Context) {
        self.usage_list_filter_dialog(ctx);
        self.usage_list_group_dialog(ctx);
    }

    fn usage_list_filter_dialog(&mut self, ctx: &egui::Context) {
        use crate::dialog_kit as kit;
        let Some((owner, mut d)) = self.panes.usage_list_filter.take() else {
            return;
        };
        let roles_view = self.owner_list_mut(owner).view == ListView::Role;
        let types: Vec<String> = code_types(&self.project).into_iter().map(|t| t.1).collect();
        let values: Vec<String> = if d.code_type.is_empty() {
            vec![]
        } else {
            code_values_of(&self.project, &d.code_type)
        };
        let (title, what) = if roles_view {
            ("Role Filter", "roles")
        } else {
            ("Resource Filter", "resources")
        };
        let (clicked, closed, actions) = kit::window(
            ctx,
            "usage_list_filter",
            title,
            Vec2::new(392., 236.),
            |c| {
                c.label(
                    10.,
                    6.,
                    16.,
                    &format!("Show {what} whose ID or name contains:"),
                    true,
                );
                c.edit(
                    "usage_list_filter_text",
                    10.,
                    24.,
                    372.,
                    21.,
                    &mut d.text,
                    true,
                    Align::LEFT,
                );
                c.label(10., 54., 16., "Resource Type", !roles_view);
                for (k, (key, caption)) in [
                    ("usage_list_filter_labor", "Labor"),
                    ("usage_list_filter_nonlabor", "Nonlabor"),
                    ("usage_list_filter_material", "Material"),
                ]
                .into_iter()
                .enumerate()
                {
                    let flag = match k {
                        0 => &mut d.labor,
                        1 => &mut d.nonlabor,
                        _ => &mut d.material,
                    };
                    c.checkbox(
                        key,
                        18. + 110. * k as f32,
                        72.,
                        100.,
                        18.,
                        caption,
                        flag,
                        !roles_view,
                    );
                }
                c.label(10., 100., 16., "Resource Code", !roles_view);
                let mut names: Vec<(&str, bool)> = vec![("(Any code)", true)];
                names.extend(types.iter().map(|t| (t.as_str(), true)));
                let mut chosen = types
                    .iter()
                    .position(|t| *t == d.code_type)
                    .map_or(0, |i| i + 1);
                if c.combo(
                    "usage_list_filter_code_type",
                    10.,
                    118.,
                    180.,
                    21.,
                    &names,
                    &mut chosen,
                    !roles_view && !types.is_empty(),
                ) {
                    d.code_type = if chosen == 0 {
                        String::new()
                    } else {
                        types[chosen - 1].clone()
                    };
                    d.code_value.clear();
                }
                let mut list: Vec<(&str, bool)> = vec![("(Any value)", true)];
                list.extend(values.iter().map(|v| (v.as_str(), true)));
                let mut value = values
                    .iter()
                    .position(|v| *v == d.code_value)
                    .map_or(0, |i| i + 1);
                if c.combo(
                    "usage_list_filter_code_value",
                    202.,
                    118.,
                    180.,
                    21.,
                    &list,
                    &mut value,
                    !roles_view && !d.code_type.is_empty(),
                ) {
                    d.code_value = if value == 0 {
                        String::new()
                    } else {
                        values[value - 1].clone()
                    };
                }
                c.checkbox(
                    "usage_list_filter_project",
                    10.,
                    152.,
                    372.,
                    18.,
                    &format!("Only the current project's {what}"),
                    &mut d.project_only,
                    true,
                );
                kit::button_row(
                    c,
                    382.,
                    202.,
                    &[
                        kit::Entry::ok("usage_list_filter_ok"),
                        kit::Entry::cancel("usage_list_filter_cancel"),
                    ],
                )
                .clicked
            },
        );
        self.record_dialog_actions(actions);
        let clicked = clicked.as_deref();
        match clicked {
            Some("usage_list_filter_ok") => {
                let state = self.owner_list_mut(owner);
                state.custom = d;
                state.filter = ListFilter::Custom;
            }
            Some(_) => {}
            None if !closed => self.panes.usage_list_filter = Some((owner, d)),
            None => {}
        }
    }

    fn usage_list_group_dialog(&mut self, ctx: &egui::Context) {
        use crate::dialog_kit as kit;
        let Some((owner, mut levels)) = self.panes.usage_list_group.take() else {
            return;
        };
        let types: Vec<String> = code_types(&self.project).into_iter().map(|t| t.1).collect();
        let (clicked, closed, actions) = kit::window(
            ctx,
            "usage_list_group",
            "Group Resources by Code",
            Vec2::new(392., 196.),
            |c| {
                if types.is_empty() {
                    c.label(10., 6., 16., "This project has no resource codes.", true);
                } else {
                    c.label(
                        10.,
                        6.,
                        16.,
                        "Group by these resource codes, in order:",
                        true,
                    );
                }
                c.label(10., 28., 16., "Group By", true);
                c.label(250., 28., 16., "Order", true);
                let mut names: Vec<(&str, bool)> = vec![("(None)", true)];
                names.extend(types.iter().map(|t| (t.as_str(), true)));
                for k in 0..MAX_LEVELS {
                    let y = 48. + 30. * k as f32;
                    let enabled = !types.is_empty() && k <= levels.len();
                    let mut chosen = levels
                        .get(k)
                        .and_then(|l| types.iter().position(|t| *t == l.code_type))
                        .map_or(0, |i| i + 1);
                    if c.combo(
                        &format!("usage_list_group_code_{k}"),
                        10.,
                        y,
                        230.,
                        21.,
                        &names,
                        &mut chosen,
                        enabled,
                    ) {
                        if chosen == 0 {
                            levels.truncate(k);
                        } else {
                            let level = GroupLevel {
                                code_type: types[chosen - 1].clone(),
                                descending: levels.get(k).is_some_and(|l| l.descending),
                            };
                            if k < levels.len() {
                                levels[k] = level;
                            } else {
                                levels.push(level);
                            }
                        }
                    }
                    let mut order = usize::from(levels.get(k).is_some_and(|l| l.descending));
                    let has = levels.get(k).is_some_and(|l| !l.code_type.is_empty());
                    if c.combo(
                        &format!("usage_list_group_order_{k}"),
                        250.,
                        y,
                        132.,
                        21.,
                        &[("Ascending", true), ("Descending", true)],
                        &mut order,
                        has,
                    ) {
                        if let Some(l) = levels.get_mut(k) {
                            l.descending = order == 1;
                        }
                    }
                }
                kit::button_row(
                    c,
                    382.,
                    162.,
                    &[
                        kit::Entry::ok("usage_list_group_ok"),
                        kit::Entry::cancel("usage_list_group_cancel"),
                    ],
                )
                .clicked
            },
        );
        self.record_dialog_actions(actions);
        let clicked = clicked.as_deref();
        match clicked {
            Some("usage_list_group_ok") => {
                levels.retain(|l| !l.code_type.is_empty());
                let state = self.owner_list_mut(owner);
                state.group = if levels.is_empty() {
                    ListGroup::List
                } else {
                    ListGroup::Custom
                };
                state.levels = levels;
                state.collapsed.clear();
                let group = state.group;
                if owner == ListOwner::Resources {
                    // The Resources table keeps its choice apart from its
                    // per-filter default.
                    self.views.resources.group = Some(group);
                }
            }
            Some(_) => {}
            None if !closed => self.panes.usage_list_group = Some((owner, levels)),
            None => {}
        }
    }

    /// What the usage panes filter the activity table by, for the layout
    /// bar's `Filter:` caption; `None` when they do not.
    pub(crate) fn usage_filter_caption(&self) -> Option<String> {
        let u = &self.panes.usage;
        let (period, members) = self.usage_table_filter();
        let mut parts = vec![];
        if let Some((resources, roles)) = members {
            let profile = self.panes.bottom == BottomView::ResourceUsageProfile;
            let selection = u.selection();
            let lists = self.usage_list_rows(profile);
            let name = match selection.as_slice() {
                [ListKey::Group(_)] => lists.row(&selection[0]).map(|r| r.label.clone()),
                [ListKey::Resource(id)] => self
                    .usage_resources()
                    .iter()
                    .find(|r| r.id == *id)
                    .map(|r| r.code.clone()),
                [ListKey::Role(id)] => {
                    roles_of(&self.project, *id).map(|r| format!("role {}", r.code))
                }
                _ => None,
            };
            let count = resources.len() + roles.len();
            parts.push(match name {
                Some(n) => format!("activities of {n}"),
                None => format!("activities of {count} selected"),
            });
        }
        if let Some((a, b)) = period {
            let origin = self.project.model.origin_serial;
            parts.push(format!(
                "working {} to {}",
                format::date(origin, Some(a)),
                format::date(origin, Some((b - 1).max(a)))
            ));
        }
        (!parts.is_empty()).then(|| format!("Usage: {}", parts.join(", ")))
    }

    /// Turns off Display Activities for selected Resource and Time Period.
    pub(crate) fn clear_usage_table_filter(&mut self) {
        self.panes.usage.resource_filter = false;
        self.panes.usage.time_period = false;
    }

    /// The usage panes' filter on the activity table: the picked period,
    /// and the resources and roles whose activities stay.
    #[allow(clippy::type_complexity)]
    pub(crate) fn usage_table_filter(
        &self,
    ) -> (Option<(i64, i64)>, Option<(BTreeSet<i64>, BTreeSet<i64>)>) {
        let u = &self.panes.usage;
        let bottom = self.panes.bottom;
        let activity_view = matches!(
            bottom,
            BottomView::ActivityUsageProfile | BottomView::ActivityUsageSheet
        );
        let resource_view = matches!(
            bottom,
            BottomView::ResourceUsageSheet | BottomView::ResourceUsageProfile
        );
        let open = self.show_details && (activity_view || (resource_view && !u.show_all_projects));
        let period = u.period.filter(|_| u.time_period && open);
        let members = (u.resource_filter && open && resource_view)
            .then(|| self.usage_members(bottom == BottomView::ResourceUsageProfile));
        (period, members)
    }
}

fn roles_of(p: &ProjectData, id: i64) -> Option<Role> {
    roles(p).into_iter().find(|r| r.id == id)
}
