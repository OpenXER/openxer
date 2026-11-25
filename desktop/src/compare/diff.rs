//! The comparison itself: two projects matched by Activity ID, WBS code and
//! predecessor/successor pair. Pure: no UI, no store. Every value is read
//! through the activity column registry, so the texts are what the activity
//! table shows in the user's formats, and dates are compared on the
//! absolute clock (the two projects may count minutes from different days).
use crate::format;
use crate::grid::columns::{self, Column, Index, Kind, Source, Value};
use crate::reports::doc::Cell;
use openxer_engine::calendar::DAY;
use openxer_engine::model::LinkType;
use openxer_engine::options::{FloatType, LagCalendar};
use openxer_xer::ProjectData;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// What a comparison looks at; each can be switched off in the dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Area {
    Project,
    Wbs,
    Activities,
    General,
    Durations,
    Dates,
    Actuals,
    Constraints,
    Percent,
    Float,
    Codes,
    Relationships,
}

impl Area {
    pub const ALL: [Area; 12] = [
        Area::Project,
        Area::Wbs,
        Area::Activities,
        Area::General,
        Area::Durations,
        Area::Dates,
        Area::Actuals,
        Area::Constraints,
        Area::Percent,
        Area::Float,
        Area::Codes,
        Area::Relationships,
    ];
    /// The areas listed under "Activity Information".
    pub const ACTIVITY_FIELDS: [Area; 8] = [
        Area::General,
        Area::Durations,
        Area::Dates,
        Area::Actuals,
        Area::Constraints,
        Area::Percent,
        Area::Float,
        Area::Codes,
    ];
    pub fn caption(self) -> &'static str {
        match self {
            Area::Project => "Project Information",
            Area::Wbs => "Added/deleted WBS",
            Area::Activities => "Added/deleted Activities",
            Area::General => "General Activity information",
            Area::Durations => "Activity Durations",
            Area::Dates => "Activity Dates",
            Area::Actuals => "Activity Status and Actual Dates",
            Area::Constraints => "Activity Constraints",
            Area::Percent => "Activity Percent Complete",
            Area::Float => "Activity Float",
            Area::Codes => "Activity Code Assignments",
            Area::Relationships => "Added/deleted Relationships",
        }
    }
    /// Short name for filters and the field column.
    pub fn short(self) -> &'static str {
        match self {
            Area::Project => "Project",
            Area::Wbs => "WBS",
            Area::Activities => "Activities",
            Area::General => "General",
            Area::Durations => "Durations",
            Area::Dates => "Dates",
            Area::Actuals => "Status and Actuals",
            Area::Constraints => "Constraints",
            Area::Percent => "Percent Complete",
            Area::Float => "Float",
            Area::Codes => "Activity Codes",
            Area::Relationships => "Relationships",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Area::Project => "project",
            Area::Wbs => "wbs",
            Area::Activities => "activities",
            Area::General => "general",
            Area::Durations => "durations",
            Area::Dates => "dates",
            Area::Actuals => "actuals",
            Area::Constraints => "constraints",
            Area::Percent => "percent",
            Area::Float => "float",
            Area::Codes => "codes",
            Area::Relationships => "relationships",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.key() == key)
    }
}

/// Activity fields compared, by area, as column registry ids.
const FIELDS: [(Area, &str); 18] = [
    (Area::General, "activity_name"),
    (Area::General, "activity_type"),
    (Area::General, "calendar"),
    (Area::General, "wbs"),
    (Area::Durations, "original_duration"),
    (Area::Durations, "remaining_duration"),
    (Area::Dates, "start"),
    (Area::Dates, "finish"),
    (Area::Actuals, "activity_status"),
    (Area::Actuals, "actual_start"),
    (Area::Actuals, "actual_finish"),
    (Area::Constraints, "primary_constraint"),
    (Area::Constraints, "primary_constraint_date"),
    (Area::Constraints, "secondary_constraint"),
    (Area::Constraints, "secondary_constraint_date"),
    (Area::Percent, "activity_percent_complete"),
    (Area::Float, "total_float"),
    (Area::Float, "free_float"),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub areas: BTreeSet<Area>,
    /// Report changes under one band per activity (else one table per area).
    pub group_by_activity: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            areas: Area::ALL.into_iter().collect(),
            group_by_activity: true,
        }
    }
}

impl Options {
    pub fn has(&self, area: Area) -> bool {
        self.areas.contains(&area)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind2 {
    Added,
    Deleted,
    Changed,
}

impl Kind2 {
    pub fn caption(self) -> &'static str {
        match self {
            Kind2::Added => "Added",
            Kind2::Deleted => "Deleted",
            Kind2::Changed => "Changed",
        }
    }
}

/// One field that differs.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldChange {
    pub area: Area,
    pub field: String,
    pub old: Cell,
    pub new: Cell,
    /// New minus old: calendar days for dates, work days for durations and
    /// float, points for percents.
    pub variance: Option<f64>,
    pub variance_text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActivityDiff {
    pub kind: Kind2,
    pub code: String,
    pub name: String,
    pub wbs: String,
    /// Model index in the original and the revised project.
    pub old: Option<usize>,
    pub new: Option<usize>,
    /// For added and deleted activities: original duration, start, finish.
    pub facts: Vec<Cell>,
    pub changes: Vec<FieldChange>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LinkSide {
    pub link_type: LinkType,
    pub lag: i64,
    pub lag_text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LinkDiff {
    pub kind: Kind2,
    pub pred: String,
    pub pred_name: String,
    pub succ: String,
    pub succ_name: String,
    pub old: Option<LinkSide>,
    pub new: Option<LinkSide>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WbsDiff {
    pub kind: Kind2,
    pub code: String,
    pub name: String,
    pub old_name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Comparison {
    pub old_label: String,
    pub new_label: String,
    pub options: Options,
    pub project: Vec<FieldChange>,
    pub wbs: Vec<WbsDiff>,
    pub activities: Vec<ActivityDiff>,
    pub links: Vec<LinkDiff>,
    pub old_activities: usize,
    pub new_activities: usize,
    pub old_links: usize,
    pub new_links: usize,
    /// Data date and project finish of each side as cells, for the summary.
    pub data_date: (Cell, Cell),
    pub finish: (Cell, Cell),
    /// Project finish movement in calendar days (revised minus original).
    pub finish_variance: Option<f64>,
}

impl Comparison {
    pub fn count(&self, kind: Kind2) -> usize {
        self.activities.iter().filter(|a| a.kind == kind).count()
    }
    pub fn link_count(&self, kind: Kind2) -> usize {
        self.links.iter().filter(|l| l.kind == kind).count()
    }
    /// Field changes of `area` across all changed activities.
    pub fn changes_in(&self, area: Area) -> usize {
        self.activities
            .iter()
            .flat_map(|a| &a.changes)
            .filter(|c| c.area == area)
            .count()
    }
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.project.is_empty()
            && self.wbs.is_empty()
            && self.activities.is_empty()
            && self.links.is_empty()
    }
}

/// A side of the comparison with its lookups.
struct Side<'p> {
    p: &'p ProjectData,
    ix: Index,
    catalog: Vec<Column>,
    by_code: HashMap<String, usize>,
}

impl<'p> Side<'p> {
    fn new(p: &'p ProjectData) -> Self {
        let mut by_code = HashMap::new();
        for (i, a) in p.model.activities.iter().enumerate() {
            by_code.entry(a.task_code.clone()).or_insert(i);
        }
        Self {
            p,
            ix: Index::new(openxer_xer::critical_definition(p)),
            catalog: columns::catalog(p),
            by_code,
        }
    }
    /// Work hours in a day on activity `i`'s own calendar, the same
    /// calendar its duration text is written in.
    fn hours_per_day(&self, i: usize) -> f64 {
        format::activity_periods(self.p, i).day as f64 / 60.
    }
    fn origin(&self) -> i64 {
        self.p.model.origin_serial
    }
    fn column(&self, id: &str) -> Option<&Column> {
        columns::find(&self.catalog, id)
    }
    /// A full WBS code without the project's own code in front, so a
    /// baseline (a renamed copy) matches its parent.
    fn relative(&self, code: &str) -> String {
        let project = &self.p.model.project.name;
        if code == project {
            return String::new();
        }
        code.strip_prefix(&format!("{project}."))
            .unwrap_or(code)
            .to_string()
    }
    fn raw(&self, column: &Column, i: usize) -> Value {
        match columns::value(self.p, &self.ix, column, i) {
            Value::Text(code) if column.id == "wbs" => Value::Text(self.relative(&code)),
            v => v,
        }
    }
    /// The value with dates moved to the absolute clock.
    fn value(&self, column: &Column, i: usize) -> Value {
        match self.raw(column, i) {
            Value::Date { minute, mark } => Value::Date {
                minute: minute + self.origin() * DAY,
                mark,
            },
            v => v,
        }
    }
    fn cell(&self, column: &Column, i: usize) -> Cell {
        let v = self.raw(column, i);
        let text = columns::display(self.p, Some(i), &v);
        match v {
            Value::Date { minute, .. } => Cell::date(self.origin(), Some(minute), text),
            Value::Duration(m) => Cell::number(m as f64 / 60. / self.hours_per_day(i), text),
            Value::Percent(x) | Value::Number(x) | Value::Units(x) | Value::Cost(x) => {
                Cell::number(x, text)
            }
            _ => Cell::Text(text),
        }
    }
    fn wbs_code(&self, i: usize) -> String {
        self.column("wbs")
            .map_or(String::new(), |c| self.cell(c, i).text().to_string())
    }
}

fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Percent(x), Value::Percent(y))
        | (Value::Number(x), Value::Number(y))
        | (Value::Units(x), Value::Units(y))
        | (Value::Cost(x), Value::Cost(y)) => (x - y).abs() < 1e-6,
        (Value::Text(x), Value::Text(y)) => x.trim() == y.trim(),
        // A constraint change is reported on its own fields, not again as
        // a Start or Finish change through the constraint marker.
        (Value::Date { minute: x, mark: m }, Value::Date { minute: y, mark: n }) => {
            use crate::format::DateMark;
            x == y && (*m == DateMark::Actual) == (*n == DateMark::Actual)
        }
        _ => a == b,
    }
}

fn signed(v: f64, unit: &str) -> String {
    let rounded = (v * 10.).round() / 10.;
    let text = if rounded.fract() == 0. {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded:.1}")
    };
    if rounded > 0. {
        format!("+{text}{unit}")
    } else {
        format!("{text}{unit}")
    }
}

/// New minus old for numeric kinds.
fn variance(kind: Kind, old: &Value, new: &Value, hours_per_day: f64) -> Option<(f64, String)> {
    match (kind, old, new) {
        (Kind::Date, Value::Date { minute: a, .. }, Value::Date { minute: b, .. }) => {
            let days = (b - a) as f64 / DAY as f64;
            Some((days, signed(days, "d")))
        }
        (Kind::Duration, Value::Duration(a), Value::Duration(b)) => {
            let days = (b - a) as f64 / 60. / hours_per_day;
            Some((days, signed(days, "d")))
        }
        (Kind::Percent, Value::Percent(a), Value::Percent(b)) => Some((b - a, signed(b - a, "%"))),
        _ => None,
    }
}

/// Activity fields to compare: the fixed set plus one per activity code
/// type found in either project (matched by code type name).
fn compared_fields(old: &Side, new: &Side, options: &Options) -> Vec<(Area, String)> {
    let mut out: Vec<(Area, String)> = FIELDS
        .iter()
        .filter(|(area, _)| options.has(*area))
        .map(|(area, id)| (*area, id.to_string()))
        .collect();
    if options.has(Area::Codes) {
        let mut seen = BTreeSet::new();
        for side in [old, new] {
            for c in &side.catalog {
                if matches!(c.source, Source::Code(_)) && seen.insert(c.id.clone()) {
                    out.push((Area::Codes, c.id.clone()));
                }
            }
        }
    }
    out
}

fn facts(side: &Side, i: usize) -> Vec<Cell> {
    ["original_duration", "start", "finish"]
        .iter()
        .map(|id| {
            side.column(id)
                .map_or(Cell::Text(String::new()), |c| side.cell(c, i))
        })
        .collect()
}

fn link_type_name(t: LinkType) -> &'static str {
    match t {
        LinkType::FS => "Finish to Start",
        LinkType::SS => "Start to Start",
        LinkType::FF => "Finish to Finish",
        LinkType::SF => "Start to Finish",
        LinkType::None => "",
    }
}

pub fn link_text(side: &LinkSide) -> String {
    let short = match side.link_type {
        LinkType::FS => "FS",
        LinkType::SS => "SS",
        LinkType::FF => "FF",
        LinkType::SF => "SF",
        LinkType::None => "",
    };
    if side.lag == 0 {
        short.to_string()
    } else {
        format!("{short} {}", side.lag_text)
    }
}

pub fn link_type_caption(t: LinkType) -> &'static str {
    link_type_name(t)
}

fn name_of(old: &Side, new: &Side, code: &str) -> String {
    new.by_code
        .get(code)
        .map(|&i| new.p.name(i).to_string())
        .or_else(|| old.by_code.get(code).map(|&i| old.p.name(i).to_string()))
        .unwrap_or_default()
}

/// The logic changes (`openxer_xer::compare::tie_changes`, the same the
/// agents' compare reads), with each side's lag in its own words.
fn compare_links(old: &Side, new: &Side) -> Vec<LinkDiff> {
    use openxer_xer::compare::{self as core, Change};
    let side = |s: &Side, t: Option<core::TieSide>| {
        t.map(|t| {
            let l = &s.p.model.links[t.link];
            LinkSide {
                link_type: t.link_type,
                lag: t.lag,
                lag_text: format::duration(t.lag, format::lag_periods(s.p, l.pred, l.succ)),
            }
        })
    };
    core::tie_changes(old.p, new.p)
        .into_iter()
        .map(|c| LinkDiff {
            kind: match c.change {
                Change::Added => Kind2::Added,
                Change::Deleted => Kind2::Deleted,
                Change::Changed => Kind2::Changed,
            },
            pred_name: name_of(old, new, &c.pred),
            succ_name: name_of(old, new, &c.succ),
            old: side(old, c.old),
            new: side(new, c.new),
            pred: c.pred,
            succ: c.succ,
        })
        .collect()
}

fn project_finish(p: &ProjectData) -> Option<i64> {
    let leveled = p.model.options.do_level;
    p.model
        .activities
        .iter()
        .filter_map(|a| crate::util::displayed_finish(a, leveled))
        .max()
}

fn yes_no(v: bool) -> &'static str {
    if v {
        "Yes"
    } else {
        "No"
    }
}

/// Project-level facts compared: (field, text, absolute minute for dates).
fn project_facts(p: &ProjectData) -> Vec<(&'static str, String, Option<i64>)> {
    let m = &p.model;
    let origin = m.origin_serial;
    let o = &m.options;
    let date = |v: Option<i64>| (format::date(origin, v), v.map(|v| v + origin * DAY));
    let mut out = vec![
        ("Project ID", m.project.name.clone(), None),
        ("Project Name", crate::views::project_title(p), None),
    ];
    for (field, v) in [
        ("Data Date", m.project.data_date),
        ("Project Planned Start", m.project.plan_start),
        ("Must Finish By", m.project.plan_end),
        ("Scheduled Finish", m.project.scd_end),
        ("Project Finish (latest activity finish)", project_finish(p)),
    ] {
        let (text, abs) = date(v);
        out.push((field, text, abs));
    }
    out.push((
        "Default Calendar",
        p.calendar_names
            .get(m.project_calendar)
            .cloned()
            .unwrap_or_default(),
        None,
    ));
    let options: [(&'static str, String); 6] = [
        (
            "When scheduling progressed activities use",
            if o.retained_logic {
                "Retained Logic"
            } else if o.progress_override {
                "Progress Override"
            } else {
                "Actual Dates"
            }
            .into(),
        ),
        (
            "Make open-ended activities critical",
            yes_no(o.open_end_uses_early).into(),
        ),
        (
            "Use Expected Finish Dates",
            yes_no(o.use_expected_finish).into(),
        ),
        (
            "Calculate start-to-start lag from",
            if o.lag_from_early_start {
                "Early Start"
            } else {
                "Actual Start"
            }
            .into(),
        ),
        (
            "Compute Total Float as",
            match o.float_type {
                FloatType::Start => "Start Float",
                FloatType::Finish => "Finish Float",
                FloatType::Min => "Smallest of Start Float and Finish Float",
            }
            .into(),
        ),
        (
            "Calendar for scheduling Relationship Lag",
            match o.lag_calendar {
                LagCalendar::PredCal => "Predecessor Activity Calendar",
                LagCalendar::SuccCal => "Successor Activity Calendar",
                LagCalendar::Hour24 => "24 Hour Calendar",
                LagCalendar::ProjCal => "Project Default Calendar",
            }
            .into(),
        ),
    ];
    out.extend(options.into_iter().map(|(f, v)| (f, v, None)));
    out.push(("Level resources", yes_no(o.do_level).into(), None));
    out
}

fn date_cell(abs: Option<i64>, text: &str) -> Cell {
    Cell::date(0, abs, text)
}

fn compare_project(old: &ProjectData, new: &ProjectData) -> Vec<FieldChange> {
    let a = project_facts(old);
    let b = project_facts(new);
    a.into_iter()
        .zip(b)
        .filter(|(x, y)| x.1 != y.1 || x.2 != y.2)
        .map(|(x, y)| {
            let (variance, variance_text) = match (x.2, y.2) {
                (Some(o), Some(n)) => {
                    let days = (n - o) as f64 / DAY as f64;
                    (Some(days), signed(days, "d"))
                }
                _ => (None, String::new()),
            };
            FieldChange {
                area: Area::Project,
                field: x.0.to_string(),
                old: date_cell(x.2, &x.1),
                new: date_cell(y.2, &y.1),
                variance,
                variance_text,
            }
        })
        .collect()
}

fn compare_wbs(old: &Side, new: &Side) -> Vec<WbsDiff> {
    let map = |s: &Side| -> BTreeMap<String, String> {
        let w = s.ix.wbs(s.p);
        s.p.wbs
            .iter()
            .map(|n| {
                let full = w
                    .codes
                    .get(&n.id)
                    .cloned()
                    .unwrap_or_else(|| n.code.clone());
                (s.relative(&full), n.name.clone())
            })
            .filter(|(code, _)| !code.is_empty())
            .collect()
    };
    let a = map(old);
    let b = map(new);
    let mut out = vec![];
    for (code, name) in &b {
        match a.get(code) {
            None => out.push(WbsDiff {
                kind: Kind2::Added,
                code: code.clone(),
                name: name.clone(),
                old_name: String::new(),
            }),
            Some(o) if o != name => out.push(WbsDiff {
                kind: Kind2::Changed,
                code: code.clone(),
                name: name.clone(),
                old_name: o.clone(),
            }),
            _ => {}
        }
    }
    for (code, name) in &a {
        if !b.contains_key(code) {
            out.push(WbsDiff {
                kind: Kind2::Deleted,
                code: code.clone(),
                name: String::new(),
                old_name: name.clone(),
            });
        }
    }
    out
}

/// Compare `old` (the original project or baseline) with `new` (the revised
/// project).
pub fn compare(
    old: &ProjectData,
    old_label: &str,
    new: &ProjectData,
    new_label: &str,
    options: &Options,
) -> Comparison {
    let a = Side::new(old);
    let b = Side::new(new);
    let fields = compared_fields(&a, &b, options);
    let mut activities = vec![];
    // Which activities match, were added or deleted: the shared core
    // (`openxer_xer::compare`), the same the agents' compare reads.
    let matches = openxer_xer::compare::match_activities(old, new);
    let matched: HashMap<usize, usize> = matches.pairs.iter().map(|&(i, j)| (j, i)).collect();
    for (j, act) in new.model.activities.iter().enumerate() {
        if !matched.contains_key(&j) && !matches.added.contains(&j) {
            continue;
        }
        match matched.get(&j) {
            None => {
                if options.has(Area::Activities) {
                    activities.push(ActivityDiff {
                        kind: Kind2::Added,
                        code: act.task_code.clone(),
                        name: new.name(j).to_string(),
                        wbs: b.wbs_code(j),
                        old: None,
                        new: Some(j),
                        facts: facts(&b, j),
                        changes: vec![],
                    });
                }
            }
            Some(&i) => {
                let mut changes = vec![];
                for (area, id) in &fields {
                    let (Some(ca), Some(cb)) = (a.column(id), b.column(id)) else {
                        // A code type only one side has: compare against blank.
                        let (side, column, k) = match (a.column(id), b.column(id)) {
                            (Some(c), None) => (&a, c, i),
                            (None, Some(c)) => (&b, c, j),
                            _ => continue,
                        };
                        let v = side.value(column, k);
                        if matches!(v, Value::Empty) {
                            continue;
                        }
                        let filled = side.cell(column, k);
                        let blank = Cell::Text(String::new());
                        let (o, n) = if std::ptr::eq(side, &a) {
                            (filled, blank)
                        } else {
                            (blank, filled)
                        };
                        changes.push(FieldChange {
                            area: *area,
                            field: column.title.clone(),
                            old: o,
                            new: n,
                            variance: None,
                            variance_text: String::new(),
                        });
                        continue;
                    };
                    let va = a.value(ca, i);
                    let vb = b.value(cb, j);
                    if same(&va, &vb) {
                        continue;
                    }
                    let (variance, variance_text) = variance(cb.kind, &va, &vb, b.hours_per_day(j))
                        .map_or((None, String::new()), |(v, t)| (Some(v), t));
                    let (old_cell, new_cell) = (a.cell(ca, i), b.cell(cb, j));
                    // A difference too small to show (a fraction of a
                    // minute, a cent) is not a change the reader can see.
                    if old_cell.text() == new_cell.text()
                        && variance.is_none_or(|v| (v * 10.).round() == 0.)
                    {
                        continue;
                    }
                    changes.push(FieldChange {
                        area: *area,
                        field: cb.title.clone(),
                        old: old_cell,
                        new: new_cell,
                        variance,
                        variance_text,
                    });
                }
                if !changes.is_empty() {
                    activities.push(ActivityDiff {
                        kind: Kind2::Changed,
                        code: act.task_code.clone(),
                        name: new.name(j).to_string(),
                        wbs: b.wbs_code(j),
                        old: Some(i),
                        new: Some(j),
                        facts: vec![],
                        changes,
                    });
                }
            }
        }
    }
    if options.has(Area::Activities) {
        for &i in &matches.deleted {
            let act = &old.model.activities[i];
            {
                activities.push(ActivityDiff {
                    kind: Kind2::Deleted,
                    code: act.task_code.clone(),
                    name: old.name(i).to_string(),
                    wbs: a.wbs_code(i),
                    old: Some(i),
                    new: None,
                    facts: facts(&a, i),
                    changes: vec![],
                });
            }
        }
    }
    activities.sort_by(|x, y| x.code.cmp(&y.code).then(x.kind.cmp(&y.kind)));
    let finish = (project_finish(old), project_finish(new));
    let dd = |p: &ProjectData| p.model.project.data_date;
    Comparison {
        old_label: old_label.into(),
        new_label: new_label.into(),
        options: options.clone(),
        project: if options.has(Area::Project) {
            compare_project(old, new)
        } else {
            vec![]
        },
        wbs: if options.has(Area::Wbs) {
            compare_wbs(&a, &b)
        } else {
            vec![]
        },
        activities,
        links: if options.has(Area::Relationships) {
            compare_links(&a, &b)
        } else {
            vec![]
        },
        old_activities: old.model.activities.len(),
        new_activities: new.model.activities.len(),
        old_links: old.model.links.len(),
        new_links: new.model.links.len(),
        data_date: (
            Cell::date(
                old.model.origin_serial,
                dd(old),
                format::date(old.model.origin_serial, dd(old)),
            ),
            Cell::date(
                new.model.origin_serial,
                dd(new),
                format::date(new.model.origin_serial, dd(new)),
            ),
        ),
        finish: (
            Cell::date(
                old.model.origin_serial,
                finish.0,
                format::date(old.model.origin_serial, finish.0),
            ),
            Cell::date(
                new.model.origin_serial,
                finish.1,
                format::date(new.model.origin_serial, finish.1),
            ),
        ),
        finish_variance: match finish {
            (Some(x), Some(y)) => Some(
                ((y + new.model.origin_serial * DAY) - (x + old.model.origin_serial * DAY)) as f64
                    / DAY as f64,
            ),
            _ => None,
        },
    }
}

pub fn signed_days(v: f64) -> String {
    signed(v, "d")
}
