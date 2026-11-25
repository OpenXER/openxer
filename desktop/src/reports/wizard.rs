//! Report Wizard definitions: a subject area, columns, a filter, one
//! grouping level, sort keys, a title block and an output. Activity reports
//! run through the activity table's own machinery (column registry, filters,
//! group and sort), so a report shows exactly what a layout with the same
//! settings shows. Resources, resource assignments and relationships use
//! small column sets of their own. Definitions are saved in the workspace
//! preferences and can be rerun.
use super::doc::{self, Cell, Column, Output, Report, Row, Section, Table, Tone};
use super::rpt::store::ReportScope;
use crate::format;
use crate::grid::columns::{self, Index, Value};
use crate::grid::filters::{FilterSelection, Standard, UserFilter};
use crate::grid::group::{self, Scope};
use crate::grid::layout::{ColumnSetting, GroupLevel, GroupSort, Layout, SortKey};
use openxer_engine::model::LinkType;
use openxer_xer::ProjectData;
use serde_json::{json, Value as Json};
use std::collections::BTreeSet;

pub const DEFINITIONS_KEY: &str = "report_definitions_v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
    Activities,
    Resources,
    Assignments,
    Relationships,
}

impl Subject {
    pub const ALL: [Subject; 4] = [
        Subject::Activities,
        Subject::Resources,
        Subject::Assignments,
        Subject::Relationships,
    ];
    pub fn caption(self) -> &'static str {
        match self {
            Subject::Activities => "Activities",
            Subject::Resources => "Resources",
            Subject::Assignments => "Resource Assignments",
            Subject::Relationships => "Relationships",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Subject::Activities => "activities",
            Subject::Resources => "resources",
            Subject::Assignments => "assignments",
            Subject::Relationships => "relationships",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.key() == key)
    }
    /// Columns a new report of this subject starts with.
    pub fn default_columns(self) -> Vec<String> {
        let ids: &[&str] = match self {
            Subject::Activities => &[
                "activity_id",
                "activity_name",
                "original_duration",
                "remaining_duration",
                "start",
                "finish",
                "total_float",
            ],
            Subject::Resources => &[
                "rsrc_id",
                "rsrc_name",
                "rsrc_type",
                "unit",
                "max_units",
                "calendar",
            ],
            Subject::Assignments => &[
                "activity_id",
                "activity_name",
                "rsrc_id",
                "budget",
                "actual",
                "remaining",
                "start",
                "finish",
            ],
            Subject::Relationships => &[
                "pred_id",
                "pred_name",
                "succ_id",
                "succ_name",
                "type",
                "lag",
                "driving",
            ],
        };
        ids.iter().map(|s| s.to_string()).collect()
    }
}

/// A column of the non-activity subjects.
#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub key: &'static str,
    pub title: &'static str,
    pub width: f32,
    pub numeric: bool,
}

impl Field {
    /// A date field: sorted as a number but shown left like the table's
    /// date columns.
    pub fn is_date(&self) -> bool {
        matches!(self.key, "start" | "finish" | "pred_finish" | "succ_start")
    }
}

const fn f(key: &'static str, title: &'static str, width: f32, numeric: bool) -> Field {
    Field {
        key,
        title,
        width,
        numeric,
    }
}

const RESOURCE_FIELDS: [Field; 10] = [
    f("rsrc_id", "Resource ID", 14., false),
    f("rsrc_name", "Resource Name", 28., false),
    f("rsrc_type", "Resource Type", 12., false),
    f("unit", "Unit of Measure", 10., false),
    f("default_units", "Default Units/Time", 10., true),
    f("max_units", "Max Units/Time", 10., true),
    f("price", "Price/Unit", 10., true),
    f("calendar", "Calendar", 20., false),
    f("assignments", "Assignments", 8., true),
    f("active", "Active", 6., false),
];

const ASSIGNMENT_FIELDS: [Field; 12] = [
    f("activity_id", "Activity ID", 12., false),
    f("activity_name", "Activity Name", 30., false),
    f("rsrc_id", "Resource ID", 12., false),
    f("rsrc_name", "Resource Name", 22., false),
    f("budget", "Budgeted Units", 10., true),
    f("actual", "Actual Units", 10., true),
    f("remaining", "Remaining Units", 10., true),
    f("at_completion", "At Completion Units", 10., true),
    f("budget_cost", "Budgeted Cost", 10., true),
    f("start", "Start", 14., true),
    f("finish", "Finish", 14., true),
    f("wbs", "WBS", 14., false),
];

const RELATIONSHIP_FIELDS: [Field; 10] = [
    f("pred_id", "Predecessor", 12., false),
    f("pred_name", "Predecessor Name", 28., false),
    f("succ_id", "Successor", 12., false),
    f("succ_name", "Successor Name", 28., false),
    f("type", "Relationship Type", 8., false),
    f("lag", "Lag", 8., true),
    f("driving", "Driving", 7., false),
    f("critical", "Critical", 7., false),
    f("pred_finish", "Predecessor Finish", 14., true),
    f("succ_start", "Successor Start", 14., true),
];

pub fn fields(subject: Subject) -> &'static [Field] {
    match subject {
        Subject::Activities => &[],
        Subject::Resources => &RESOURCE_FIELDS,
        Subject::Assignments => &ASSIGNMENT_FIELDS,
        Subject::Relationships => &RELATIONSHIP_FIELDS,
    }
}

/// (id, title) of every column a subject offers.
pub fn available(subject: Subject, p: &ProjectData) -> Vec<(String, String)> {
    match subject {
        Subject::Activities => columns::catalog(p)
            .into_iter()
            .map(|c| (c.id, c.title))
            .collect(),
        s => fields(s)
            .iter()
            .map(|f| (f.key.to_string(), f.title.to_string()))
            .collect(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Definition {
    pub name: String,
    pub subject: Subject,
    pub title: String,
    pub header: String,
    pub footer: String,
    pub columns: Vec<String>,
    /// Activity filter keys (`std:critical`, `user:Name`); all must match
    /// when `match_all`, else any.
    pub filters: Vec<String>,
    pub match_all: bool,
    /// Rows whose text contains this (any column), ignoring case.
    pub contains: String,
    /// Column id to group by (`wbs` groups by the WBS hierarchy).
    pub group_by: Option<String>,
    pub sort: Vec<SortKey>,
    pub show_totals: bool,
    /// `None` previews on screen.
    pub output: Option<Output>,
    pub path: String,
    /// The report group it belongs to (`report_groups_v1` ids).
    pub group: Option<u32>,
    /// Who may run it: any project, or the project it was made for.
    pub scope: ReportScope,
    /// When it last ran (`YYYY-MM-DD HH:MM:SS`, local time).
    pub last_run: Option<String>,
}

impl Definition {
    pub fn new(subject: Subject) -> Self {
        Self {
            name: String::new(),
            subject,
            title: format!("{} Report", subject.caption()),
            header: String::new(),
            footer: String::new(),
            columns: subject.default_columns(),
            filters: vec![],
            match_all: true,
            contains: String::new(),
            group_by: (subject == Subject::Activities).then(|| "wbs".to_string()),
            sort: vec![SortKey {
                column: subject.default_columns()[0].clone(),
                descending: false,
            }],
            show_totals: false,
            output: None,
            path: String::new(),
            group: None,
            scope: ReportScope::Global,
            last_run: None,
        }
    }

    /// A definition that reproduces the activity table's current layout.
    pub fn from_layout(layout: &Layout) -> Self {
        let mut d = Self::new(Subject::Activities);
        d.title = layout.name.clone();
        d.columns = layout.columns.iter().map(|c| c.id.clone()).collect();
        if layout.filter.active() {
            d.filters = layout.filter.selected.clone();
            d.match_all = layout.filter.match_all;
        }
        d.group_by = layout.group_sort.levels.first().map(|l| l.column.clone());
        d.sort = layout.group_sort.sort.clone();
        d.show_totals = layout.group_sort.show_grand_totals;
        d
    }

    pub fn to_json(&self) -> Json {
        json!({
            "name": self.name,
            "subject": self.subject.key(),
            "title": self.title,
            "header": self.header,
            "footer": self.footer,
            "columns": self.columns,
            "filters": self.filters,
            "match_all": self.match_all,
            "contains": self.contains,
            "group_by": self.group_by,
            "sort": self.sort.iter().map(|s| json!({"column": s.column, "descending": s.descending})).collect::<Vec<_>>(),
            "show_totals": self.show_totals,
            "output": self.output.map(|o| o.key()),
            "path": self.path,
            "group": self.group,
            "scope": self.scope.to_db().0,
            "scope_project": self.scope.to_db().1,
            "last_run": self.last_run,
        })
    }

    pub fn from_json(v: &Json) -> Result<Self, String> {
        let text = |k: &str| v[k].as_str().unwrap_or("").to_string();
        let subject = Subject::from_key(v["subject"].as_str().unwrap_or(""))
            .ok_or("a report needs a subject area")?;
        let strings = |k: &str| -> Vec<String> {
            v[k].as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default()
        };
        let columns = strings("columns");
        if columns.is_empty() {
            return Err("a report needs at least one column".into());
        }
        Ok(Self {
            name: text("name"),
            subject,
            title: text("title"),
            header: text("header"),
            footer: text("footer"),
            columns,
            filters: strings("filters"),
            match_all: v["match_all"].as_bool().unwrap_or(true),
            contains: text("contains"),
            group_by: v["group_by"].as_str().map(String::from),
            sort: v["sort"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|s| {
                            Some(SortKey {
                                column: s["column"].as_str()?.to_string(),
                                descending: s["descending"].as_bool().unwrap_or(false),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
            show_totals: v["show_totals"].as_bool().unwrap_or(false),
            output: v["output"].as_str().and_then(Output::from_key),
            path: text("path"),
            group: v["group"].as_u64().and_then(|g| u32::try_from(g).ok()),
            scope: ReportScope::from_db(
                v["scope"].as_str().unwrap_or("global"),
                v["scope_project"].as_str().map(String::from),
            ),
            last_run: v["last_run"].as_str().map(String::from),
        })
    }
}

pub fn definitions_to_json(list: &[Definition]) -> String {
    Json::Array(list.iter().map(Definition::to_json).collect()).to_string()
}

/// Saved definitions; an entry that does not read is skipped.
pub fn definitions_from_json(text: &str) -> Vec<Definition> {
    serde_json::from_str::<Json>(text)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|v| Definition::from_json(v).ok())
        .collect()
}

/// The filter choices for activity reports: (key, caption).
pub fn filter_choices(users: &[UserFilter]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Standard::ALL
        .iter()
        .map(|s| (s.key(), s.caption().to_string()))
        .collect();
    out.extend(users.iter().map(|u| (u.key(), u.name.clone())));
    out
}

/// A typed cell. Durations are in days of activity `i`'s own calendar (the
/// project calendar for a band total), the same days the text shows.
fn value_cell(p: &ProjectData, i: Option<usize>, v: &Value) -> Cell {
    let text = columns::display(p, i, v);
    match v {
        Value::Date { minute, .. } => Cell::date(p.model.origin_serial, Some(*minute), text),
        Value::Duration(m) => {
            let day = i.map_or_else(
                || format::project_periods(p),
                |i| format::activity_periods(p, i),
            );
            Cell::number(*m as f64 / day.day.max(1) as f64, text)
        }
        Value::Percent(x) | Value::Units(x) | Value::Cost(x) | Value::Number(x) => {
            Cell::number(*x, text)
        }
        _ => Cell::Text(text),
    }
}

/// What a report needs from the app besides the definition.
pub struct Context<'a> {
    pub ix: &'a Index,
    pub users: &'a [UserFilter],
}

fn activity_table(def: &Definition, p: &ProjectData, cx: &Context) -> (Table, usize) {
    let catalog = columns::catalog(p);
    let cols: Vec<&columns::Column> = def
        .columns
        .iter()
        .filter_map(|id| columns::find(&catalog, id))
        .collect();
    let layout = Layout {
        name: def.title.clone(),
        columns: cols
            .iter()
            .map(|c| ColumnSetting::new(&c.id, c.width))
            .collect(),
        filter: FilterSelection {
            all_activities: def.filters.is_empty(),
            selected: def.filters.clone(),
            match_all: def.match_all,
            highlight: false,
        },
        group_sort: GroupSort {
            levels: def
                .group_by
                .iter()
                .map(|c| GroupLevel {
                    column: c.clone(),
                    to_level: None,
                    indent: true,
                    interval: String::new(),
                })
                .collect(),
            show_grand_totals: def.show_totals,
            sort: def.sort.clone(),
            ..GroupSort::default()
        },
        gantt: None,
        bottom: None,
        panes: None,
        table: Default::default(),
    };
    let collapsed = BTreeSet::new();
    let built = group::build(
        p,
        cx.ix,
        &catalog,
        &layout,
        &Scope {
            query: &def.contains,
            wbs_filter: None,
            collapsed: &collapsed,
            users: cx.users,
        },
    );
    let mut count = 0;
    let rows = built
        .rows
        .iter()
        .map(|row| match *row {
            crate::grid::Row::Group(b) => {
                let band = &built.bands[b];
                let mut cells: Vec<Cell> = cols
                    .iter()
                    .map(|c| {
                        group::band_total(p, cx.ix, c, &band.members)
                            .map_or(Cell::Text(String::new()), |v| value_cell(p, None, &v))
                    })
                    .collect();
                if let Some(first) = cells.first_mut() {
                    *first = Cell::Text(band.label.clone());
                }
                Row::new(cells, Tone::Band(band.level.min(3) as u8))
            }
            crate::grid::Row::Activity(i) => {
                count += 1;
                Row::new(
                    cols.iter()
                        .map(|c| value_cell(p, Some(i), &columns::value(p, cx.ix, c, i)))
                        .collect(),
                    Tone::Normal,
                )
            }
        })
        .collect();
    let table = Table {
        name: def.subject.caption().into(),
        caption: String::new(),
        columns: cols
            .iter()
            .map(|c| Column {
                title: c.title.clone(),
                width: (c.width / 7.).max(6.),
                right: c.align == columns::Align::Right,
            })
            .collect(),
        rows,
    };
    (table, count)
}

/// One row of a non-activity subject: a cell per field key.
type Line = Vec<(&'static str, Cell)>;

/// The rows of a non-activity subject.
fn subject_rows(subject: Subject, p: &ProjectData) -> Vec<Line> {
    let origin = p.model.origin_serial;
    let date = |m: Option<i64>| Cell::date(origin, m, format::date(origin, m));
    let num = |v: f64| Cell::number(v, crate::records::format_number((v * 100.).round() / 100.));
    let leveled = p.model.options.do_level;
    match subject {
        Subject::Activities => vec![],
        Subject::Resources => crate::views::resources::resources(p)
            .into_iter()
            .map(|r| {
                vec![
                    ("rsrc_id", Cell::Text(r.code.clone())),
                    ("rsrc_name", Cell::Text(r.name.clone())),
                    ("rsrc_type", Cell::Text(r.kind.label().into())),
                    ("unit", Cell::Text(r.unit.clone())),
                    ("default_units", num(r.default_units)),
                    (
                        "max_units",
                        r.max_units.map_or(Cell::Text(String::new()), num),
                    ),
                    (
                        "price",
                        r.price.map_or(Cell::Text(String::new()), |v| {
                            Cell::number(v, format::cost(v))
                        }),
                    ),
                    ("calendar", Cell::Text(r.calendar.clone())),
                    (
                        "assignments",
                        Cell::number(r.assignments as f64, r.assignments.to_string()),
                    ),
                    (
                        "active",
                        Cell::Text(if r.active { "Yes" } else { "No" }.into()),
                    ),
                ]
            })
            .collect(),
        Subject::Assignments => {
            let ix = Index::new((false, 0.));
            let catalog = columns::catalog(p);
            let wbs = columns::find(&catalog, "wbs").cloned();
            crate::views::assignments::assignments(p)
                .into_iter()
                .map(|a| {
                    let act = &p.model.activities[a.task];
                    let name = p.resource_names.get(&a.rsrc).cloned().unwrap_or_default();
                    let code = crate::records::lookup(
                        &p.document,
                        "RSRC",
                        "rsrc_id",
                        a.rsrc,
                        "rsrc_short_name",
                    );
                    vec![
                        ("activity_id", Cell::Text(act.task_code.clone())),
                        ("activity_name", Cell::Text(p.name(a.task).into())),
                        ("rsrc_id", Cell::Text(code)),
                        ("rsrc_name", Cell::Text(name)),
                        ("budget", num(a.budget)),
                        ("actual", num(a.actual)),
                        ("remaining", num(a.remaining)),
                        ("at_completion", num(a.actual + a.remaining)),
                        (
                            "budget_cost",
                            Cell::number(
                                a.budget * a.cost_per_qty,
                                format::cost(a.budget * a.cost_per_qty),
                            ),
                        ),
                        ("start", date(crate::util::displayed_start(act, leveled))),
                        ("finish", date(crate::util::displayed_finish(act, leveled))),
                        (
                            "wbs",
                            Cell::Text(wbs.as_ref().map_or(String::new(), |c| {
                                columns::display(
                                    p,
                                    Some(a.task),
                                    &columns::value(p, &ix, c, a.task),
                                )
                            })),
                        ),
                    ]
                })
                .collect()
        }
        Subject::Relationships => {
            let definition = openxer_xer::critical_definition(p);
            let critical =
                |x: &openxer_engine::model::Activity| columns::is_critical(x, definition);
            p.model
                .links
                .iter()
                .filter(|l| l.pred < p.model.activities.len() && l.succ < p.model.activities.len())
                .map(|l| {
                    let (a, b) = (&p.model.activities[l.pred], &p.model.activities[l.succ]);
                    let short = match l.link_type {
                        LinkType::FS => "FS",
                        LinkType::SS => "SS",
                        LinkType::FF => "FF",
                        LinkType::SF => "SF",
                        LinkType::None => "",
                    };
                    let lag = format::duration(l.lag, format::lag_periods(p, l.pred, l.succ));
                    vec![
                        ("pred_id", Cell::Text(a.task_code.clone())),
                        ("pred_name", Cell::Text(p.name(l.pred).into())),
                        ("succ_id", Cell::Text(b.task_code.clone())),
                        ("succ_name", Cell::Text(p.name(l.succ).into())),
                        ("type", Cell::Text(short.into())),
                        ("lag", Cell::number(l.lag as f64 / 60., lag)),
                        (
                            "driving",
                            Cell::Text(if l.driving_path { "Yes" } else { "No" }.into()),
                        ),
                        (
                            "critical",
                            Cell::Text(
                                if critical(a) && critical(b) {
                                    "Yes"
                                } else {
                                    "No"
                                }
                                .into(),
                            ),
                        ),
                        (
                            "pred_finish",
                            date(crate::util::displayed_finish(a, leveled)),
                        ),
                        ("succ_start", date(crate::util::displayed_start(b, leveled))),
                    ]
                })
                .collect()
        }
    }
}

fn sort_value(c: &Cell) -> (u8, f64, String) {
    match c {
        Cell::Number { value, .. } => (1, *value, String::new()),
        Cell::Date { serial, .. } => (1, *serial, String::new()),
        Cell::Text(s) if s.is_empty() => (0, 0., String::new()),
        Cell::Text(s) => (2, 0., s.to_lowercase()),
    }
}

fn simple_table(def: &Definition, p: &ProjectData) -> (Table, usize) {
    let all = fields(def.subject);
    let chosen: Vec<&Field> = def
        .columns
        .iter()
        .filter_map(|k| all.iter().find(|f| f.key == k))
        .collect();
    let needle = def.contains.trim().to_lowercase();
    let mut rows: Vec<Line> = subject_rows(def.subject, p)
        .into_iter()
        .filter(|r| {
            needle.is_empty()
                || r.iter()
                    .any(|(_, c)| c.text().to_lowercase().contains(&needle))
        })
        .collect();
    let get = |r: &Line, key: &str| -> Cell {
        r.iter()
            .find(|(k, _)| *k == key)
            .map_or(Cell::Text(String::new()), |(_, c)| c.clone())
    };
    rows.sort_by(|a, b| {
        for key in &def.sort {
            let (x, y) = (
                sort_value(&get(a, &key.column)),
                sort_value(&get(b, &key.column)),
            );
            let o =
                x.0.cmp(&y.0)
                    .then(x.1.total_cmp(&y.1))
                    .then_with(|| x.2.cmp(&y.2));
            let o = if key.descending { o.reverse() } else { o };
            if o != std::cmp::Ordering::Equal {
                return o;
            }
        }
        std::cmp::Ordering::Equal
    });
    let count = rows.len();
    let line = |r: &Line| -> Row {
        Row::new(chosen.iter().map(|f| get(r, f.key)).collect(), Tone::Normal)
    };
    let totals = |list: &[&Line], label: String, level: u8| -> Row {
        let mut cells: Vec<Cell> = chosen
            .iter()
            .map(|f| {
                let sum: f64 = list
                    .iter()
                    .filter_map(|r| match get(r, f.key) {
                        Cell::Number { value, .. } if f.numeric && !matches!(f.key, "lag") => {
                            Some(value)
                        }
                        _ => None,
                    })
                    .sum();
                if f.numeric
                    && list
                        .iter()
                        .any(|r| matches!(get(r, f.key), Cell::Number { .. }))
                    && f.key != "lag"
                {
                    Cell::number(
                        sum,
                        crate::records::format_number((sum * 100.).round() / 100.),
                    )
                } else {
                    Cell::Text(String::new())
                }
            })
            .collect();
        if let Some(first) = cells.first_mut() {
            *first = Cell::Text(label);
        }
        Row::new(cells, Tone::Band(level))
    };
    let mut out = vec![];
    let all_rows: Vec<&Line> = rows.iter().collect();
    if def.show_totals {
        out.push(totals(&all_rows, "Total".into(), 0));
    }
    let level = if def.show_totals { 1 } else { 0 };
    match def
        .group_by
        .as_deref()
        .filter(|g| all.iter().any(|f| f.key == *g))
    {
        Some(key) => {
            let mut groups: Vec<(String, Vec<&Line>)> = vec![];
            for r in &rows {
                let label = get(r, key).text().to_string();
                match groups.iter_mut().find(|g| g.0 == label) {
                    Some(g) => g.1.push(r),
                    None => groups.push((label, vec![r])),
                }
            }
            groups.sort_by_key(|a| a.0.to_lowercase());
            let title = all.iter().find(|f| f.key == key).map_or("", |f| f.title);
            for (label, list) in groups {
                let label = if label.is_empty() {
                    format!("No {title}")
                } else {
                    label
                };
                out.push(totals(&list, label, level));
                out.extend(list.iter().map(|r| line(r)));
            }
        }
        None => out.extend(rows.iter().map(line)),
    }
    let table = Table {
        name: def.subject.caption().into(),
        caption: String::new(),
        columns: chosen
            .iter()
            .map(|f| Column {
                title: f.title.into(),
                width: f.width,
                right: f.numeric && !f.is_date(),
            })
            .collect(),
        rows: out,
    };
    (table, count)
}

/// Run a definition on a project.
pub fn build(def: &Definition, p: &ProjectData, cx: &Context) -> Report {
    let (table, count) = match def.subject {
        Subject::Activities => activity_table(def, p, cx),
        _ => simple_table(def, p),
    };
    let mut paragraphs = vec![format!(
        "{count} {}",
        match (def.subject, count) {
            (Subject::Activities, 1) => "activity",
            (Subject::Activities, _) => "activities",
            (Subject::Resources, 1) => "resource",
            (Subject::Resources, _) => "resources",
            (Subject::Assignments, 1) => "resource assignment",
            (Subject::Assignments, _) => "resource assignments",
            (Subject::Relationships, 1) => "relationship",
            (Subject::Relationships, _) => "relationships",
        }
    )];
    if def.subject == Subject::Activities && !def.filters.is_empty() {
        let names: Vec<String> = def
            .filters
            .iter()
            .filter_map(|k| crate::grid::filters::filter_caption(k, cx.users))
            .collect();
        paragraphs.push(format!(
            "Filter: {}",
            names.join(if def.match_all { " and " } else { " or " })
        ));
    }
    if !def.contains.trim().is_empty() {
        paragraphs.push(format!("Rows containing \"{}\"", def.contains.trim()));
    }
    let mut header = vec![];
    if !def.header.trim().is_empty() {
        header.push(def.header.trim().to_string());
    }
    let origin = p.model.origin_serial;
    header.push(format!(
        "Project: {} - {}",
        p.model.project.name,
        crate::views::project_title(p)
    ));
    header.push(format!(
        "Data date: {}",
        format::date(origin, p.model.project.data_date)
    ));
    header.push(format!("Run {} by {}", doc::run_stamp(), doc::user_name()));
    let title = if def.title.trim().is_empty() {
        format!("{} Report", def.subject.caption())
    } else {
        def.title.trim().to_string()
    };
    Report {
        title,
        header,
        sections: vec![Section {
            heading: def.subject.caption().into(),
            paragraphs,
            tables: vec![table],
        }],
        footer: def.footer.trim().to_string(),
    }
}
