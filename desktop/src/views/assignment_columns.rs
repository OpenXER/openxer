//! The one column catalog of resource assignment fields, shared by the
//! Resource Assignments window's table and the Activity Details Resources
//! tab: each field's id, title, width, group, where its value comes from,
//! its text, whether a cell takes a value for an activity in its status and
//! how a typed or picked value becomes an assignment edit.
//!
//! The Resources tab lists the fields marked `tab`, in catalog order, with
//! their data column numbers (`TAB_COLUMNS`, what `details::grid_columns`
//! shows); the window offers every field. Fields that come from the
//! activity (Activity ID, Activity Name, Activity Status, WBS, Project ID)
//! read through `grid::columns` (which delegates built-in activity fields
//! to the shared `openxer_xer::edit::fields` catalog), so they show as on
//! the Activities page.
//!
//! Positions in `FIELDS` are what a saved window layout stores, so a new
//! field goes at the end. User defined fields of assignments follow the
//! window's own fields as extra columns (`grid::Columns::extra`, via
//! `views::udf_columns`).
use crate::details::assignments::{
    dictionary, id_name, number_text, parse_money, parse_number, parse_units, parse_units_per_time,
    resource_type_label, Rows,
};
use crate::format::{self, Periods};
use crate::views::grid::EditKind;
use openxer_xer::assignments::{AssignmentEdit, AssignmentView, RATE_SOURCES, RATE_TYPES};
use openxer_xer::Document;

/// Where a field's value comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// A column of the activity table (`grid::columns`), by its id.
    Activity(&'static str),
    /// A value of the assignment, by its data column (the Resources tab's
    /// cell numbers; 39 and up are the window's own).
    Data(usize),
}

/// One field of the catalog.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Field {
    pub id: &'static str,
    pub title: &'static str,
    pub width: f32,
    /// The Columns dialog's group.
    pub group: &'static str,
    pub source: Source,
    /// Listed by the Resources tab of Activity Details.
    pub tab: bool,
    /// A number: right aligned.
    pub right: bool,
}

pub const GENERAL: &str = "General";
pub const UNITS: &str = "Units";
pub const COSTS: &str = "Costs";
pub const DATES: &str = "Dates";
pub const DURATIONS: &str = "Durations";

const fn activity(id: &'static str, title: &'static str, width: f32) -> Field {
    Field {
        id,
        title,
        width,
        group: GENERAL,
        source: Source::Activity(id),
        tab: false,
        right: false,
    }
}

const fn data(
    id: &'static str,
    title: &'static str,
    width: f32,
    group: &'static str,
    column: usize,
    right: bool,
) -> Field {
    Field {
        id,
        title,
        width,
        group,
        source: Source::Data(column),
        tab: true,
        right,
    }
}

/// A field of the window only.
const fn window(field: Field) -> Field {
    Field {
        tab: false,
        ..field
    }
}

/// Every field, in the order the window stores them. The Resources tab's
/// fields keep the tab's column order (the reference's defaults first).
#[rustfmt::skip]
pub const FIELDS: [Field; 47] = [
    activity("activity_id", "Activity ID", 100.),
    activity("activity_name", "Activity Name", 150.),
    data("resource_id_name", "Resource ID Name", 150., GENERAL, 0, false),
    data("price", "Price / Unit", 80., COSTS, 1, true),
    data("rate_type", "Rate Type", 100., COSTS, 2, false),
    data("rate_source", "Rate Source", 80., COSTS, 3, false),
    data("primary_resource", "Primary Resource", 60., GENERAL, 4, false),
    data("budgeted_units", "Budgeted Units", 80., UNITS, 5, true),
    data("actual_units", "Actual Units", 80., UNITS, 6, true),
    data("remaining_units", "Remaining Units", 100., UNITS, 7, true),
    data("at_completion_units", "At Completion Units", 115., UNITS, 31, true),
    data("remaining_units_per_time", "Remaining Units / Time", 120., UNITS, 8, true),
    data("role", "Role", 100., GENERAL, 9, false),
    data("cost_account", "Cost Account", 100., GENERAL, 10, false),
    data("units_per_time", "Units / Time", 70., UNITS, 11, true),
    data("original_duration", "Original Duration", 100., DURATIONS, 12, true),
    data("remaining_duration", "Remaining Duration", 110., DURATIONS, 13, true),
    data("drive_activity_dates", "Drive Activity Dates", 115., GENERAL, 14, false),
    data("curve", "Curve", 110., GENERAL, 15, false),
    data("lag", "Lag", 70., DURATIONS, 16, true),
    data("planned_lag", "Planned Lag", 80., DURATIONS, 17, true),
    data("overtime_factor", "Overtime Factor", 95., UNITS, 18, true),
    data("actual_regular_units", "Actual Regular Units", 115., UNITS, 19, true),
    data("actual_overtime_units", "Actual Overtime Units", 120., UNITS, 20, true),
    data("actual_regular_cost", "Actual Regular Cost", 110., COSTS, 21, true),
    data("actual_overtime_cost", "Actual Overtime Cost", 115., COSTS, 22, true),
    data("budgeted_cost", "Budgeted Cost", 90., COSTS, 27, true),
    data("actual_cost", "Actual Cost", 90., COSTS, 28, true),
    data("remaining_cost", "Remaining Cost", 95., COSTS, 29, true),
    data("at_completion_cost", "At Completion Cost", 110., COSTS, 30, true),
    data("cost_units_linked", "Cost Units Linked", 100., GENERAL, 36, false),
    data("units_this_period", "Units This Period", 100., UNITS, 37, true),
    data("cost_this_period", "Cost This Period", 100., COSTS, 38, true),
    data("start", "Start", 135., DATES, 32, false),
    data("finish", "Finish", 135., DATES, 33, false),
    data("planned_start", "Planned Start", 135., DATES, 34, false),
    data("planned_finish", "Planned Finish", 135., DATES, 35, false),
    data("actual_start", "Actual Start", 135., DATES, 23, false),
    data("actual_finish", "Actual Finish", 135., DATES, 24, false),
    data("remaining_start", "Remaining Start", 135., DATES, 25, false),
    data("remaining_finish", "Remaining Finish", 135., DATES, 26, false),
    activity("activity_status", "Activity Status", 85.),
    window(data("resource_type", "Resource Type", 100., GENERAL, 39, false)),
    activity("wbs", "WBS", 120.),
    activity("project_id", "Project ID", 90.),
    window(data("remaining_late_start", "Remaining Late Start", 135., DATES, 40, false)),
    window(data("remaining_late_finish", "Remaining Late Finish", 135., DATES, 41, false)),
];

/// The number of fields the Resources tab lists.
const TAB_COUNT: usize = 39;

const fn tab_columns() -> [(&'static str, f32, usize); TAB_COUNT] {
    let mut out = [("", 0., 0); TAB_COUNT];
    let (mut i, mut k) = (0, 0);
    while i < FIELDS.len() {
        if let (true, Source::Data(column)) = (FIELDS[i].tab, FIELDS[i].source) {
            out[k] = (FIELDS[i].title, FIELDS[i].width, column);
            k += 1;
        }
        i += 1;
    }
    assert!(k == TAB_COUNT);
    out
}

/// The Resources tab's columns: (title, width, data column), in order.
pub const TAB_COLUMNS: [(&str, f32, usize); TAB_COUNT] = tab_columns();

const fn ids() -> [&'static str; FIELDS.len()] {
    let mut out = [""; FIELDS.len()];
    let mut i = 0;
    while i < FIELDS.len() {
        out[i] = FIELDS[i].id;
        i += 1;
    }
    out
}

const fn titles() -> [&'static str; FIELDS.len()] {
    let mut out = [""; FIELDS.len()];
    let mut i = 0;
    while i < FIELDS.len() {
        out[i] = FIELDS[i].title;
        i += 1;
    }
    out
}

/// Field ids and titles in catalog order (the window's field numbers).
pub const IDS: [&str; FIELDS.len()] = ids();
pub const TITLES: [&str; FIELDS.len()] = titles();

/// The field numbers of the right aligned (number) fields.
pub fn right_fields() -> Vec<usize> {
    (0..FIELDS.len()).filter(|&f| FIELDS[f].right).collect()
}

/// The field number of `id`.
pub fn position(id: &str) -> Option<usize> {
    IDS.iter().position(|x| *x == id)
}

/// The window's columns out of the box, by id: the reference's set under
/// the reference defaults (`crate::defaults`), else the earlier two.
pub fn default_ids() -> &'static [&'static str] {
    if crate::defaults::reference() {
        crate::defaults::ASSIGNMENT_COLUMNS
    } else {
        &["activity_id", "activity_name"]
    }
}

/// The window's default columns as field numbers.
pub fn default_order() -> Vec<usize> {
    default_ids().iter().filter_map(|id| position(id)).collect()
}

/// The dictionaries an assignment's cells name: resources, roles, cost
/// accounts (id, [code, name]), resource curves and resource types.
pub struct Lookups {
    pub resources: Rows,
    pub roles: Rows,
    pub accounts: Rows,
    pub curves: Vec<String>,
    /// (resource id, type label).
    pub kinds: Vec<(i64, &'static str)>,
}

impl Lookups {
    pub fn of(doc: &Document) -> Self {
        let kinds = doc.table("RSRC").map_or(vec![], |t| {
            t.rows
                .iter()
                .map(|r| {
                    (
                        crate::records::num(t.value(r, "rsrc_id")),
                        resource_type_label(t.value(r, "rsrc_type")),
                    )
                })
                .collect()
        });
        Self {
            resources: dictionary(doc, "RSRC", "rsrc_id", &["rsrc_short_name", "rsrc_name"]),
            roles: dictionary(doc, "ROLES", "role_id", &["role_short_name", "role_name"]),
            accounts: dictionary(doc, "ACCOUNT", "acct_id", &["acct_short_name", "acct_name"]),
            curves: crate::enterprise::ops::curve_choices(doc)
                .into_iter()
                .map(|c| c.0)
                .collect(),
            kinds,
        }
    }
}

/// What a cell needs to know of the assignment's activity.
#[derive(Clone, Copy, Debug)]
pub struct Activity {
    pub started: bool,
    pub completed: bool,
    pub periods: Periods,
    pub origin: i64,
}

/// An assignment and what its cells show.
pub struct Cells<'a> {
    pub view: &'a AssignmentView,
    pub activity: Activity,
    /// The curve's name ("Manual" once period values were typed).
    pub curve: String,
}

fn yes_no(on: bool) -> String {
    if on { "Yes" } else { "No" }.to_string()
}

fn rate_type_label(code: &str) -> &str {
    RATE_TYPES
        .iter()
        .find(|(c, _, _)| *c == code)
        .map_or(code, |(_, l, _)| l)
}

fn rate_source_label(code: &str) -> &'static str {
    RATE_SOURCES
        .iter()
        .find(|(c, _)| *c == code)
        .map_or("", |(_, l)| l)
}

/// The date of data column `c` (23 to 26, 32 to 35, 40, 41).
fn date_of(v: &AssignmentView, c: usize) -> Option<i64> {
    let d = &v.dates;
    match c {
        23 => d.act_start,
        24 => d.act_finish,
        25 => d.rem_start,
        32 => d.act_start.or(d.rem_start),
        33 => d.act_finish.or(d.rem_finish),
        34 => d.target_start,
        35 => d.target_finish,
        40 => d.rem_late_start,
        41 => d.rem_late_finish,
        _ => d.rem_finish,
    }
}

/// Whether data column `c`'s date shows the actual mark.
fn date_is_actual(v: &AssignmentView, c: usize) -> bool {
    match c {
        23 | 24 => true,
        32 => v.dates.act_start.is_some(),
        33 => v.dates.act_finish.is_some(),
        _ => false,
    }
}

/// The text of data column `c` of an assignment, as the Resources tab
/// shows it.
pub fn text(look: &Lookups, cells: &Cells, c: usize) -> String {
    let v = cells.view;
    let a = cells.activity;
    match c {
        0 => id_name(&look.resources, v.rsrc_id),
        1 => format::cost(v.price),
        2 => rate_type_label(&v.rate_type).to_string(),
        3 => rate_source_label(&v.rate_source).to_string(),
        4 => yes_no(v.primary),
        5 => format::units(v.budgeted.units),
        6 => format::units(v.actual_units),
        7 => format::units(v.remaining.units),
        8 => format::units_per_time(v.remaining.per_hour),
        9 => id_name(&look.roles, v.role_id),
        10 => id_name(&look.accounts, v.acct_id),
        11 => format::units_per_time(v.budgeted.per_hour),
        12 => format::duration(v.original_duration, a.periods),
        13 => format::duration(v.remaining_duration, a.periods),
        14 => yes_no(v.drives_dates),
        15 => cells.curve.clone(),
        16 => format::duration(v.lag, a.periods),
        17 => format::duration(v.planned_lag, a.periods),
        18 => number_text(v.ot_factor),
        19 => format::units(v.actual_regular_units),
        20 => format::units(v.actual_overtime_units),
        21 => format::cost(v.actual_regular_cost),
        22 => format::cost(v.actual_overtime_cost),
        23..=26 | 32..=35 | 40 | 41 => {
            format::date_actual(a.origin, date_of(v, c), date_is_actual(v, c))
        }
        27 => format::cost(v.budgeted_cost),
        28 => format::cost(v.actual_cost),
        29 => format::cost(v.remaining_cost),
        30 => format::cost(v.actual_cost + v.remaining_cost),
        31 => format::units(v.actual_units + v.remaining.units),
        36 => yes_no(v.cost_linked),
        37 => format::units(v.this_period_units),
        38 => format::cost(v.this_period_cost),
        39 => v
            .rsrc_id
            .and_then(|id| look.kinds.iter().find(|k| k.0 == id))
            .map_or(String::new(), |k| k.1.to_string()),
        _ => String::new(),
    }
}

fn names(rows: &Rows) -> Vec<String> {
    rows.iter()
        .map(|(_, c)| format!("{}  {}", c[0], c[1]))
        .collect()
}

fn yes_no_choice() -> EditKind {
    EditKind::Choice(vec!["Yes".into(), "No".into()])
}

/// Whether data column `c` of an assignment takes a value for an activity
/// in its status, and how: the Resources tab's rules (a cell the tab opens
/// no editor on takes none here either; the edit layer refuses what the
/// status does not allow).
pub fn edit_kind(look: &Lookups, cells: &Cells, c: usize) -> Option<EditKind> {
    let v = cells.view;
    let Activity {
        started, completed, ..
    } = cells.activity;
    let text = Some(EditKind::Text);
    match c {
        0 if !look.resources.is_empty() => Some(EditKind::Choice(names(&look.resources))),
        // A price from the resource is not typed: choose Override first.
        1 if v.rate_source != "ST_Rsrc" => text,
        2 => Some(EditKind::Choice(
            RATE_TYPES.iter().map(|t| t.1.to_string()).collect(),
        )),
        3 => Some(EditKind::Choice(
            RATE_SOURCES.iter().map(|t| t.1.to_string()).collect(),
        )),
        4 if v.rsrc_id.is_some() => Some(yes_no_choice()),
        5..=8 | 11 | 12 | 17 | 18 | 27..=31 => text,
        9 if !look.roles.is_empty() => {
            let mut items = names(&look.roles);
            if v.rsrc_id.is_some() {
                items.insert(0, String::new());
            }
            Some(EditKind::Choice(items))
        }
        10 if !look.accounts.is_empty() => {
            let mut items = names(&look.accounts);
            items.insert(0, String::new());
            Some(EditKind::Choice(items))
        }
        13 if !completed => text,
        14 | 36 => Some(yes_no_choice()),
        15 if !look.curves.is_empty() => {
            let mut items = look.curves.clone();
            items.insert(0, String::new());
            Some(EditKind::Choice(items))
        }
        16 if !started => text,
        19..=22 | 37 | 38 if started => text,
        // Actual Start and Finish, Start and Finish take a date whatever
        // the status; a remaining start only before the start, a remaining
        // finish until the finish.
        23 | 24 | 32 | 33 => text,
        25 if !started && !completed => text,
        26 if !completed => text,
        _ => None,
    }
}

/// A change a cell asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Edit(AssignmentEdit),
    /// Another resource on the assignment (the staffing and price choices
    /// of User Preferences apply).
    Resource(i64),
    Primary(bool),
    /// The curve by name; `None` clears it.
    Curve(Option<String>),
    /// Cost Units Linked ticked again: asked first.
    Relink,
}

fn pick(rows: &Rows, text: &str) -> Option<i64> {
    let names = names(rows);
    names
        .iter()
        .position(|n| n == text.trim() || n.trim() == text.trim())
        .map(|k| rows[k].0)
}

/// The change `text` typed or picked into data column `c` asks for:
/// `Ok(None)` when it leaves the value as it is.
pub fn parse(
    look: &Lookups,
    cells: &Cells,
    c: usize,
    text: &str,
) -> Result<Option<Change>, String> {
    let v = cells.view;
    let a = cells.activity;
    if text.trim() == self::text(look, cells, c).trim() {
        return Ok(None);
    }
    let bad = || format!("{} is not a value this column takes.", text.trim());
    let units = || parse_units(text).ok_or_else(bad);
    let per_time = || parse_units_per_time(text).ok_or_else(bad);
    let money = || parse_money(text).ok_or_else(bad);
    let duration = || format::parse_duration(text, a.periods);
    let yes = || match text.trim() {
        "Yes" => Ok(true),
        "No" => Ok(false),
        _ => Err(bad()),
    };
    let edit = |e: AssignmentEdit| Ok(Some(Change::Edit(e)));
    match c {
        0 => {
            let id = pick(&look.resources, text).ok_or_else(bad)?;
            Ok(Some(Change::Resource(id)))
        }
        1 => edit(AssignmentEdit::PricePerUnit(money()?)),
        2 => {
            let found = RATE_TYPES.iter().find(|t| t.1 == text.trim());
            edit(AssignmentEdit::RateType(found.ok_or_else(bad)?.0.into()))
        }
        3 => {
            let found = RATE_SOURCES.iter().find(|t| t.1 == text.trim());
            edit(AssignmentEdit::RateSource(found.ok_or_else(bad)?.0.into()))
        }
        4 => Ok(Some(Change::Primary(yes()?))),
        5 => edit(AssignmentEdit::BudgetedUnits(units()?)),
        6 => edit(AssignmentEdit::ActualRegularUnits(
            (units()? - v.actual_overtime_units).max(0.),
        )),
        7 => edit(AssignmentEdit::RemainingUnits(units()?)),
        8 => edit(AssignmentEdit::RemainingUnitsPerTime(per_time()?)),
        9 if text.trim().is_empty() => edit(AssignmentEdit::Role(None)),
        9 => {
            let id = pick(&look.roles, text).ok_or_else(bad)?;
            edit(AssignmentEdit::Role(Some(id)))
        }
        10 if text.trim().is_empty() => edit(AssignmentEdit::CostAccount(None)),
        10 => {
            let id = pick(&look.accounts, text).ok_or_else(bad)?;
            edit(AssignmentEdit::CostAccount(Some(id)))
        }
        11 => edit(AssignmentEdit::BudgetedUnitsPerTime(per_time()?)),
        12 => edit(AssignmentEdit::OriginalDuration(duration()?)),
        13 => edit(AssignmentEdit::RemainingDuration(duration()?)),
        14 => edit(AssignmentEdit::DriveActivityDates(yes()?)),
        15 if text.trim().is_empty() => Ok(Some(Change::Curve(None))),
        15 => {
            let found = look.curves.iter().find(|n| n.as_str() == text.trim());
            Ok(Some(Change::Curve(Some(found.ok_or_else(bad)?.clone()))))
        }
        16 => edit(AssignmentEdit::Lag(duration()?)),
        17 => edit(AssignmentEdit::PlannedLag(duration()?)),
        18 => edit(AssignmentEdit::OvertimeFactor(
            parse_number(text).filter(|f| *f >= 0.).ok_or_else(bad)?,
        )),
        19 => edit(AssignmentEdit::ActualRegularUnits(units()?)),
        20 => edit(AssignmentEdit::ActualOvertimeUnits(units()?)),
        21 => edit(AssignmentEdit::ActualRegularCost(money()?)),
        22 => edit(AssignmentEdit::ActualOvertimeCost(money()?)),
        23..=26 | 32 | 33 => {
            let typed = format::strip_date_marker(text);
            let t = format::parse_date(a.origin, typed, date_of(v, c))?
                .ok_or_else(|| "Enter a date".to_string())?;
            edit(match c {
                23 => AssignmentEdit::ActualStart(t),
                24 => AssignmentEdit::ActualFinish(t),
                32 if a.started => AssignmentEdit::ActualStart(t),
                33 if a.completed => AssignmentEdit::ActualFinish(t),
                25 | 32 => AssignmentEdit::RemainingStart(t),
                _ => AssignmentEdit::RemainingFinish(t),
            })
        }
        27 => edit(AssignmentEdit::BudgetedCost(money()?)),
        28 => edit(AssignmentEdit::ActualCost(money()?)),
        29 => edit(AssignmentEdit::RemainingCost(money()?)),
        30 => edit(AssignmentEdit::AtCompletionCost(money()?)),
        31 => edit(AssignmentEdit::AtCompletionUnits(units()?)),
        36 if yes()? => Ok(Some(Change::Relink)),
        36 => edit(AssignmentEdit::CostUnitsLinked(false)),
        37 => edit(AssignmentEdit::ThisPeriodUnits(units()?)),
        38 => edit(AssignmentEdit::ThisPeriodCost(money()?)),
        _ => Err("That column cannot be typed in here.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_resources_tab_lists_its_columns_from_the_catalog() {
        // The tab's columns, as it listed them before the catalog.
        let titles: Vec<&str> = TAB_COLUMNS.iter().map(|c| c.0).collect();
        let want = [
            "Resource ID Name",
            "Price / Unit",
            "Rate Type",
            "Rate Source",
            "Primary Resource",
            "Budgeted Units",
            "Actual Units",
            "Remaining Units",
            "At Completion Units",
            "Remaining Units / Time",
            "Role",
        ];
        assert_eq!(titles[..11], want);
        assert_eq!(TAB_COLUMNS.last().map(|c| c.0), Some("Remaining Finish"));
        // Every data column 0 to 38 once.
        let mut data: Vec<usize> = TAB_COLUMNS.iter().map(|c| c.2).collect();
        data.sort_unstable();
        assert_eq!(data, (0..39).collect::<Vec<_>>());
        // Saved window layouts count the first two fields as before.
        assert_eq!(&IDS[..2], &["activity_id", "activity_name"]);
        let mut ids = IDS.to_vec();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), FIELDS.len());
        // Activity-level fields stay in step with the shared activity field
        // catalog (`openxer_xer::edit::fields`), except desktop-only Project ID.
        let shared = openxer_xer::edit::fields::builtin();
        for f in FIELDS {
            if let Source::Activity(id) = f.source {
                if id == "project_id" {
                    continue;
                }
                let def = shared
                    .iter()
                    .find(|d| d.id == id)
                    .unwrap_or_else(|| panic!("{id} is in xer::edit::fields::builtin"));
                assert_eq!(def.title, f.title, "title of {id}");
            }
        }
    }
}
