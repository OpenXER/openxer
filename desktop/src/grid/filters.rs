//! Activity filters: the standard set, user-defined criteria lists and the
//! selection a layout applies (All/Any, replace or highlight).
use super::columns::{self, Column, Index, Kind, Value};
use super::*;
use crate::layout_opts::checks::CheckFilter;

/// The standard filters listed under "Default" in the Filters dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standard {
    Completed,
    Critical,
    HasFinishConstraint,
    HasStartConstraint,
    InProgress,
    InProgressNotStarted,
    LevelOfEffort,
    LongestPath,
    Milestone,
    NegativeFloat,
    NonCritical,
    Normal,
    NotStarted,
    NonWhatIf,
}

impl Standard {
    pub const ALL: [Standard; 14] = [
        Standard::Completed,
        Standard::Critical,
        Standard::HasFinishConstraint,
        Standard::HasStartConstraint,
        Standard::InProgress,
        Standard::InProgressNotStarted,
        Standard::LevelOfEffort,
        Standard::LongestPath,
        Standard::Milestone,
        Standard::NegativeFloat,
        Standard::NonCritical,
        Standard::Normal,
        Standard::NotStarted,
        Standard::NonWhatIf,
    ];
    /// The Default group of the Filters dialog and the Filter By menu, in
    /// the reference order. In Progress/Not Started is kept for layouts that
    /// selected it but is not offered.
    pub const LISTED: [Standard; 13] = [
        Standard::Completed,
        Standard::Critical,
        Standard::HasFinishConstraint,
        Standard::HasStartConstraint,
        Standard::InProgress,
        Standard::LevelOfEffort,
        Standard::LongestPath,
        Standard::Milestone,
        Standard::NegativeFloat,
        Standard::NonWhatIf,
        Standard::NonCritical,
        Standard::Normal,
        Standard::NotStarted,
    ];
    pub fn caption(self) -> &'static str {
        match self {
            Standard::Completed => "Completed",
            Standard::Critical => "Critical",
            Standard::HasFinishConstraint => "Has Finish Constraint",
            Standard::HasStartConstraint => "Has Start Constraint",
            Standard::InProgress => "In Progress",
            Standard::InProgressNotStarted => "In Progress/Not Started",
            Standard::LevelOfEffort => "Level of Effort",
            Standard::LongestPath => "Longest Path",
            Standard::Milestone => "Milestone",
            Standard::NegativeFloat => "Negative Float",
            Standard::NonCritical => "Non-critical",
            Standard::Normal => "Normal",
            Standard::NotStarted => "Not Started",
            Standard::NonWhatIf => "Non What-If",
        }
    }
    /// Key stored in layouts.
    pub fn key(self) -> String {
        format!(
            "std:{}",
            self.caption().to_lowercase().replace(['/', '-', ' '], "_")
        )
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.key() == key)
    }
    pub fn matches(self, a: &Activity, critical: (bool, f64)) -> bool {
        let start_side = |c: CstrType| {
            matches!(
                c,
                CstrType::StartOn
                    | CstrType::StartOnBefore
                    | CstrType::StartOnAfter
                    | CstrType::MandatoryStart
            )
        };
        let finish_side = |c: CstrType| {
            matches!(
                c,
                CstrType::FinishOn
                    | CstrType::FinishOnBefore
                    | CstrType::FinishOnAfter
                    | CstrType::MandatoryFinish
            )
        };
        match self {
            Standard::Completed => a.status == StatusType::Completed,
            Standard::Critical => columns::is_critical(a, critical),
            Standard::HasFinishConstraint => finish_side(a.cstr) || finish_side(a.cstr2),
            Standard::HasStartConstraint => start_side(a.cstr) || start_side(a.cstr2),
            Standard::InProgress => a.status == StatusType::InProgress,
            Standard::InProgressNotStarted => a.status != StatusType::Completed,
            Standard::LevelOfEffort => a.task_type == TaskType::Loe,
            Standard::LongestPath => !a.is_completed() && a.driving_path,
            Standard::Milestone => matches!(a.task_type, TaskType::Mile | TaskType::FinMile),
            Standard::NegativeFloat => !a.is_completed() && a.total_float.is_some_and(|f| f < 0),
            Standard::NonCritical => !columns::is_critical(a, critical),
            Standard::Normal => matches!(a.task_type, TaskType::Task | TaskType::Resource),
            Standard::NotStarted => a.status == StatusType::NotStarted,
            // Without the project this reads as an ordinary project.
            Standard::NonWhatIf => true,
        }
    }

    /// `matches` with the project at hand: Non What-If reads the project's
    /// status (its WBS node's status `WS_Whatif` is a what-if project).
    pub fn matches_in(self, p: &ProjectData, a: &Activity, critical: (bool, f64)) -> bool {
        match self {
            Standard::NonWhatIf => {
                let pid = p.model.project.proj_id;
                let what_if = p.document.table("PROJWBS").is_some_and(|t| {
                    t.rows.iter().any(|r| {
                        t.value(r, "proj_node_flag") == "Y"
                            && crate::records::num(t.value(r, "proj_id")) == pid
                            && t.value(r, "status_code") == "WS_Whatif"
                    })
                });
                !what_if
            }
            _ => self.matches(a, critical),
        }
    }
}

/// The "Is" choices of a criterion row: the shared catalog's, which also
/// has "is under" for the fields that form a hierarchy.
pub use openxer_xer::edit::fields::Op;

/// Parameter of a row that opens a nested group whose rows must all hold.
pub const GROUP_ALL: &str = "(all)";
/// Parameter of a row that opens a nested group where any row may hold.
pub const GROUP_ANY: &str = "(any)";
/// Deepest nesting a filter may use.
pub const MAX_LEVEL: usize = 20;

/// One "Where" row of a user filter, or the head of a nested group
/// (`GROUP_ALL` / `GROUP_ANY` as the parameter). `level` nests rows: a group
/// at level n holds the rows after it at level n + 1.
#[derive(Clone, Debug, PartialEq)]
pub struct Criterion {
    pub column: String,
    pub op: Op,
    pub value: String,
    pub high: String,
    pub level: usize,
}

impl Default for Criterion {
    fn default() -> Self {
        Self {
            column: String::new(),
            op: Op::Equals,
            value: String::new(),
            high: String::new(),
            level: 0,
        }
    }
}

impl Criterion {
    /// `Some(all)` for a group head.
    pub fn group(&self) -> Option<bool> {
        match self.column.as_str() {
            GROUP_ALL => Some(true),
            GROUP_ANY => Some(false),
            _ => None,
        }
    }
}

/// A filter's rows as a tree: the root group holds level 0.
#[derive(Debug, PartialEq)]
pub enum Node<'a> {
    Row(&'a Criterion),
    /// (all rows must hold, members)
    Group(bool, Vec<Node<'a>>),
}

/// Nest a filter's rows. A row deeper than the group above it allows joins
/// that group; rows without a parameter are left out.
pub fn tree(f: &UserFilter) -> Node<'_> {
    // (level of the members, all, members)
    let mut stack: Vec<(usize, bool, Vec<Node>)> = vec![(0, f.match_all, vec![])];
    let close = |stack: &mut Vec<(usize, bool, Vec<Node<'_>>)>| {
        if stack.len() > 1 {
            let (_, all, members) = stack.pop().expect("a group to close");
            stack
                .last_mut()
                .expect("the root")
                .2
                .push(Node::Group(all, members));
        }
    };
    for row in &f.rows {
        while stack.len() > 1 && row.level < stack.last().map_or(0, |s| s.0) {
            close(&mut stack);
        }
        let inner = stack.last().map_or(0, |s| s.0);
        match row.group() {
            Some(all) => stack.push((inner + 1, all, vec![])),
            None if row.column.is_empty() => {}
            None => stack.last_mut().expect("the root").2.push(Node::Row(row)),
        }
    }
    while stack.len() > 1 {
        close(&mut stack);
    }
    let (_, all, members) = stack.pop().expect("the root");
    Node::Group(all, members)
}

/// Is `text` a value the parameter `kind` accepts (relative dates for
/// dates)?
pub fn check_value(p: &ProjectData, kind: Kind, text: &str) -> Result<(), String> {
    if kind == Kind::Date && super::relative::parse(text).is_some() {
        return Ok(());
    }
    columns::parse_input(p, None, kind, text).map(|_| ())
}

/// A named user-defined filter: all or any of its rows must match.
#[derive(Clone, Debug, PartialEq)]
pub struct UserFilter {
    pub name: String,
    pub match_all: bool,
    pub rows: Vec<Criterion>,
}

impl UserFilter {
    pub fn key(&self) -> String {
        format!("user:{}", self.name)
    }
}

/// Which filters the current layout applies.
#[derive(Clone, Debug, PartialEq)]
pub struct FilterSelection {
    /// The "All Activities" check: no filter applies.
    pub all_activities: bool,
    /// Selected filter keys (`std:critical`, `user:My filter`).
    pub selected: Vec<String>,
    /// All selected filters (true) or any selected filter (false).
    pub match_all: bool,
    /// Highlight matches instead of hiding the rest.
    pub highlight: bool,
}

impl Default for FilterSelection {
    fn default() -> Self {
        Self {
            all_activities: true,
            selected: vec![],
            match_all: true,
            highlight: false,
        }
    }
}

impl FilterSelection {
    pub fn active(&self) -> bool {
        !self.all_activities && !self.selected.is_empty()
    }
    /// Text for the layout bar's Filter button.
    pub fn summary(&self, users: &[UserFilter]) -> String {
        if !self.active() {
            return "All Activities".into();
        }
        let names: Vec<String> = self
            .selected
            .iter()
            .filter_map(|k| filter_caption(k, users))
            .collect();
        if names.is_empty() {
            return "All Activities".into();
        }
        names.join(if self.match_all { " and " } else { " or " })
    }
}

pub fn filter_caption(key: &str, users: &[UserFilter]) -> Option<String> {
    Standard::from_key(key)
        .map(|s| s.caption().to_string())
        .or_else(|| CheckFilter::from_key(key).map(|c| c.caption().to_string()))
        .or_else(|| {
            users
                .iter()
                .find(|u| u.key() == key)
                .map(|u| u.name.clone())
        })
}

/// Does one criterion hold for activity `i`? Rows without a parameter hold.
pub fn criterion_matches(
    p: &ProjectData,
    ix: &Index,
    catalog: &[Column],
    c: &Criterion,
    i: usize,
) -> bool {
    Prepared::new(p, catalog, c).matches(p, ix, i)
}

/// A criterion made ready to test many activities: its column looked up,
/// its parameter lowercased and relative dates resolved once.
pub struct Prepared<'a> {
    c: &'a Criterion,
    column: Option<&'a Column>,
    wanted: String,
    /// Relative date parameters (value, high) resolved, for date columns.
    value_date: Option<Option<Value>>,
    high_date: Option<Option<Value>>,
}

impl<'a> Prepared<'a> {
    pub fn new(p: &ProjectData, catalog: &'a [Column], c: &'a Criterion) -> Self {
        let column = columns::find(catalog, &c.column);
        let mut now = None;
        let mut relative = |s: &str| -> Option<Option<Value>> {
            if column?.kind != Kind::Date {
                return None;
            }
            let r = super::relative::parse(s)?;
            let now =
                *now.get_or_insert_with(|| super::relative::now_minute(p.model.origin_serial));
            Some(
                super::relative::resolve_at(p, &r, now).map(|minute| Value::Date {
                    minute,
                    mark: crate::format::DateMark::Plain,
                }),
            )
        };
        let value_date = relative(&c.value);
        let high_date = relative(&c.high);
        Self {
            c,
            column,
            wanted: c.value.trim().to_lowercase(),
            value_date,
            high_date,
        }
    }

    /// Does the criterion hold for activity `i`? The comparison is the
    /// shared catalog's (`openxer_xer::edit::fields::holds`), on the values and
    /// text this table shows.
    pub fn matches(&self, p: &ProjectData, ix: &Index, i: usize) -> bool {
        let Some(column) = self.column else {
            return true;
        };
        let value = columns::value(p, ix, column, i);
        let typed = |s: &str, date: &Option<Option<Value>>| match date {
            Some(resolved) => resolved.clone(),
            None => columns::parse_input(p, Some(i), column.kind, s).ok(),
        };
        let test = openxer_xer::edit::fields::Test {
            op: self.c.op,
            kind: column.kind,
            wanted: &self.wanted,
            high: &self.c.high,
        };
        openxer_xer::edit::fields::holds(
            &test,
            &value,
            &|| columns::display(p, Some(i), &value).to_lowercase(),
            &|high| {
                if high {
                    typed(&self.c.high, &self.high_date)
                } else {
                    typed(&self.c.value, &self.value_date)
                }
            },
            &|| {
                column.source.shared().is_some_and(|source| {
                    openxer_xer::edit::fields::is_under(p, &ix.core, source, i, &self.c.value)
                })
            },
        )
    }
}

/// A filter tree with its rows prepared (see `Prepared`).
enum PreparedNode<'a> {
    Row(Prepared<'a>),
    Group(bool, Vec<PreparedNode<'a>>),
}

impl<'a> PreparedNode<'a> {
    fn new(p: &ProjectData, catalog: &'a [Column], node: &Node<'a>) -> Self {
        match node {
            Node::Row(c) => PreparedNode::Row(Prepared::new(p, catalog, c)),
            Node::Group(all, members) => PreparedNode::Group(
                *all,
                members
                    .iter()
                    .map(|m| PreparedNode::new(p, catalog, m))
                    .collect(),
            ),
        }
    }

    fn matches(&self, p: &ProjectData, ix: &Index, i: usize) -> bool {
        match self {
            PreparedNode::Row(c) => c.matches(p, ix, i),
            PreparedNode::Group(_, members) if members.is_empty() => true,
            PreparedNode::Group(true, members) => members.iter().all(|m| m.matches(p, ix, i)),
            PreparedNode::Group(false, members) => members.iter().any(|m| m.matches(p, ix, i)),
        }
    }
}

/// One selected filter, ready to test activities.
enum PreparedKey<'a> {
    Standard(Standard),
    Check(CheckFilter),
    User(PreparedNode<'a>),
}

/// A layout's filter selection made ready to test every activity of a row
/// build: user filters nested and their rows prepared once, not once per
/// activity.
pub struct PreparedSelection<'a> {
    active: bool,
    match_all: bool,
    /// The selected filters that exist (unknown keys are ignored).
    keys: Vec<PreparedKey<'a>>,
}

impl<'a> PreparedSelection<'a> {
    pub fn new(
        p: &ProjectData,
        catalog: &'a [Column],
        selection: &FilterSelection,
        users: &'a [UserFilter],
    ) -> Self {
        let active = selection.active();
        let keys = if active {
            selection
                .selected
                .iter()
                .filter_map(|key| {
                    if let Some(s) = Standard::from_key(key) {
                        Some(PreparedKey::Standard(s))
                    } else if let Some(c) = CheckFilter::from_key(key) {
                        Some(PreparedKey::Check(c))
                    } else {
                        users
                            .iter()
                            .find(|u| u.key() == *key)
                            .map(|u| PreparedKey::User(PreparedNode::new(p, catalog, &tree(u))))
                    }
                })
                .collect()
        } else {
            vec![]
        };
        Self {
            active,
            match_all: selection.match_all,
            keys,
        }
    }

    /// Does activity `i` pass the selection?
    pub fn matches(&self, p: &ProjectData, ix: &Index, i: usize) -> bool {
        if !self.active || self.keys.is_empty() {
            return true;
        }
        let a = &p.model.activities[i];
        let pass = |k: &PreparedKey| match k {
            PreparedKey::Standard(s) => s.matches_in(p, a, ix.critical),
            PreparedKey::Check(c) => ix.check_set(p, *c).contains(&i),
            PreparedKey::User(node) => node.matches(p, ix, i),
        };
        if self.match_all {
            self.keys.iter().all(pass)
        } else {
            self.keys.iter().any(pass)
        }
    }
}

/// Does activity `i` pass the selection? Unknown filter keys are ignored.
pub fn selection_matches(
    p: &ProjectData,
    ix: &Index,
    catalog: &[Column],
    selection: &FilterSelection,
    users: &[UserFilter],
    i: usize,
) -> bool {
    PreparedSelection::new(p, catalog, selection, users).matches(p, ix, i)
}
