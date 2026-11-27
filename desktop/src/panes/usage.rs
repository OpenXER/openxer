//! Activity and resource usage bottom views: time-phased spreadsheets and
//! histograms on the top Gantt's timescale. Units per period come from the
//! assignment spreads in `views::assignments` (budget, actual, remaining and
//! remaining late, shaped by resource curves through `openxer_xer::spread`),
//! so every view agrees with the Resources view histogram.
use super::histogram::{self, Curve, Segment};
use super::profile_options::{self as popt, Series};
use super::stacked::{self, StackBy};
use super::time_fields::{self, Figures, TimeField};
use super::usage_fields as fields;
use super::usage_list;
use super::usage_menu::UsageMenu;
use super::*;
use crate::grid::columns as cols;
use crate::hscroll::BAR_HEIGHT;
use crate::theme;
use crate::timescale::{Cell, TimeScale, Unit};
use crate::views::assignments::{self as asg, Assignment};
use crate::views::resources as rs;
use std::collections::HashMap;
use std::rc::Rc;

/// Line number column of the spreadsheets.
pub const NUMBER_W: f32 = 30.;
/// Value column after the table ("Remaining Labor Units"), measured 87 px.
pub const VALUE_W: f32 = 87.;
pub const HEADER_H: f32 = 35.;
pub const ROW_H: f32 = 18.;
/// Profile: header button, Y axis and the two-tier timescale under the chart.
pub const DISPLAY_H: f32 = 18.;
pub const AXIS_W: f32 = 44.;
pub const TIER_H: f32 = 17.;
pub const LINE: Color32 = Color32::from_rgb(0xC0, 0xC0, 0xC0);
pub const NONWORK: Color32 = theme::color(theme::NONWORK);
pub const SIGHT: Color32 = theme::color(theme::DAY_LINE);
/// Profile bar colors: fill and edge.
pub const BUDGET: (Color32, Color32) = (theme::color(0xA9B9CC), theme::color(0x5C7893));
pub const ACTUAL: (Color32, Color32) = (theme::color(theme::TEAL), theme::color(theme::TEAL_DARK));
pub const REMAINING: (Color32, Color32) =
    (theme::color(theme::BLUE), theme::color(theme::BLUE_DARK));
/// Remaining units over the late dates.
pub const REMAINING_LATE: (Color32, Color32) = (
    theme::color(theme::LOE_REMAINING),
    theme::color(theme::BLUE_DARK),
);
/// The activity profile's baseline budgets: project and primary baseline.
pub const BASELINE_SERIES: (Color32, Color32) =
    (theme::color(theme::BASELINE), theme::color(theme::SLATE));
pub const PRIMARY_SERIES: (Color32, Color32) =
    (theme::color(theme::AMBER), theme::color(theme::AMBER_DARK));
pub const OVER: (Color32, Color32) = (
    theme::color(theme::CRITICAL),
    theme::color(theme::CRITICAL_DARK),
);
/// Overtime units on the Resource Usage Profile.
pub const OVERTIME: (Color32, Color32) =
    (theme::color(theme::AMBER), theme::color(theme::AMBER_DARK));
/// Unused capacity under the limit.
pub const AVAILABLE: (Color32, Color32) = (theme::color(0xE3ECF5), theme::color(0x6F8CAB));
/// Resource band row in the resource spreadsheet.
pub const RESOURCE_BAND: Color32 = theme::color(theme::NAVY);
pub const RESOURCE_TEXT: Color32 = Color32::WHITE;
/// The period picked on a profile or spreadsheet.
pub const PICKED: Color32 = Color32::from_rgb(0xCC, 0xE8, 0xFF);
/// The spreadsheet's selected cells (`usage_cells`).
pub const SELECTED_CELLS: Color32 = Color32::from_rgb(0xB5, 0xD5, 0xF5);
/// A resource's or role's heading inside a summed selection.
pub const SUB_BAND: Color32 = theme::color(theme::LIGHT_BLUE);
/// Role demand: remaining units no resource is named for.
pub const UNSTAFFED: (Color32, Color32) = (
    theme::color(theme::BASELINE_LIGHT),
    theme::color(theme::SLATE),
);

/// One row of the Activity Usage Spreadsheet for drawing and for Copy to
/// Excel: the field cells, then for each time-phased field (one line each)
/// the total and the value of each period, with what falls before and
/// after the periods.
pub(crate) struct SheetLine {
    pub band: bool,
    pub selected: bool,
    pub fill: Color32,
    pub ink: Color32,
    /// The field cells; a band has its label in the first.
    pub texts: Vec<String>,
    /// One per time-phased field, in the chosen order.
    pub lines: Vec<Figures>,
}

/// One row of the Resource Usage Spreadsheet: the resource row, then its
/// assignments. `before` and `after` hold what falls outside the periods.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResRow {
    pub texts: Vec<String>,
    pub values: Vec<f64>,
    pub before: f64,
    pub after: f64,
    pub total: f64,
    pub band: bool,
    /// A resource or role heading inside a selection of several.
    pub sub: bool,
    /// What the units count: a material's unit of measure, or hours.
    pub unit: Option<String>,
    /// Which time-phased field the line shows (its place in the chosen
    /// list; a resource or assignment has one line per field, the field
    /// texts on the first), its label and whether it runs cumulative.
    pub field: usize,
    pub label: String,
    pub cumulative: bool,
    /// The TASKRSRC row of an assignment's line of this project (the one
    /// whose cells take typed values, `usage_cells`).
    pub assignment: Option<usize>,
}

impl ResRow {
    /// A plain row from values spread over `edged` periods.
    fn edged(texts: Vec<String>, values: &[f64], total: f64, unit: Option<&str>) -> Self {
        let (before, values, after) = unedge(values);
        Self {
            texts,
            values,
            before,
            after,
            total,
            band: false,
            sub: false,
            unit: unit.map(str::to_string),
            field: 0,
            label: String::new(),
            cumulative: false,
            assignment: None,
        }
    }

    /// Line `k` of a row: field `f`'s values spread over `edged` periods.
    pub(crate) fn field_line(
        texts: Vec<String>,
        (k, f): (usize, TimeField),
        values: &[f64],
        total: f64,
        unit: Option<&str>,
    ) -> Self {
        Self {
            field: k,
            label: f.label(),
            cumulative: f.cumulative,
            ..Self::edged(texts, values, total, unit)
        }
    }
}

/// `periods` with an edge period on each side: one from the start of time
/// up to the first period and one from the end of the last to the end of
/// time. Spread over these, the edges hold what falls before and after the
/// periods on show, so a running total starts from the true start of the
/// data and the periods plus the edges add up to every spread quantity,
/// whatever is scrolled into view. No periods, no edges.
pub fn edged(periods: &[(i64, i64)]) -> Vec<(i64, i64)> {
    let (Some(first), Some(last)) = (periods.first(), periods.last()) else {
        return vec![];
    };
    let mut out = Vec::with_capacity(periods.len() + 2);
    out.push((-asg::EDGE, first.0));
    out.extend_from_slice(periods);
    out.push((last.1, asg::EDGE));
    out
}

/// Values over `edged` periods split into what falls before the periods,
/// the periods' own values and what falls after them.
pub fn unedge(values: &[f64]) -> (f64, Vec<f64>, f64) {
    match values {
        [before, inner @ .., after] => (*before, inner.to_vec(), *after),
        _ => (0., vec![], 0.),
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActivityUsage {
    pub budget: Vec<f64>,
    pub actual: Vec<f64>,
    pub remaining: Vec<f64>,
    pub remaining_late: Vec<f64>,
}

/// The usage views' Time Interval: the timescale's lower tier, or a fixed
/// period length.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Interval {
    #[default]
    Timescale,
    Day,
    Week,
    Month,
    Quarter,
    Year,
}

impl Interval {
    pub const ALL: [Interval; 6] = [
        Interval::Timescale,
        Interval::Day,
        Interval::Week,
        Interval::Month,
        Interval::Quarter,
        Interval::Year,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Interval::Timescale => "timescale",
            Interval::Day => "day",
            Interval::Week => "week",
            Interval::Month => "month",
            Interval::Quarter => "quarter",
            Interval::Year => "year",
        }
    }
    pub fn caption(self) -> &'static str {
        match self {
            Interval::Timescale => "Timescale",
            Interval::Day => "Day",
            Interval::Week => "Week",
            Interval::Month => "Month",
            Interval::Quarter => "Quarter",
            Interval::Year => "Year",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|i| i.key() == key)
    }
    fn unit(self) -> Option<timescale::Unit> {
        match self {
            Interval::Timescale => None,
            Interval::Day => Some(timescale::Unit::Day),
            Interval::Week => Some(timescale::Unit::Week),
            Interval::Month => Some(timescale::Unit::Month),
            Interval::Quarter => Some(timescale::Unit::Quarter),
            Interval::Year => Some(timescale::Unit::Year),
        }
    }
}

/// What the profiles draw (Profile Options, `profile_options`). Overtime,
/// availability, limits and overallocation apply to the Resource Usage
/// Profile; the legend and the Graph tab to both profiles; each profile has
/// its own series.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileOptions {
    /// Overtime hours are shown apart from the regular actual units.
    pub overtime: bool,
    /// The unused capacity under the limit is shaded.
    pub availability: bool,
    /// The limit line.
    pub limits: bool,
    /// Units above the limit are shown in the overallocation color.
    pub overallocation: bool,
    /// The legend box.
    pub legend: bool,
    /// Each series' By Date and Cumulative switches and color, on the
    /// activity profile and on the resource profile.
    pub activity: popt::SeriesSet,
    pub resource: popt::SeriesSet,
    /// The activity profile's resource types (`profile_options::TYPES`).
    pub types: [bool; 4],
    pub graph: popt::Graph,
}

impl Default for ProfileOptions {
    fn default() -> Self {
        Self {
            overtime: false,
            availability: false,
            limits: true,
            overallocation: true,
            legend: true,
            activity: popt::default_series(false),
            resource: popt::default_series(true),
            types: [true, false, false, false],
            graph: popt::Graph::default(),
        }
    }
}

impl ProfileOptions {
    pub const FIELDS: [(&'static str, &'static str); 5] = [
        ("overtime", "Overtime"),
        ("availability", "Availability"),
        ("limits", "Limits"),
        ("overallocation", "Overallocation"),
        ("legend", "Legend"),
    ];
    pub fn flag(&mut self, key: &str) -> Option<&mut bool> {
        Some(match key {
            "overtime" => &mut self.overtime,
            "availability" => &mut self.availability,
            "limits" => &mut self.limits,
            "overallocation" => &mut self.overallocation,
            "legend" => &mut self.legend,
            _ => return None,
        })
    }
    pub fn json(&self) -> serde_json::Value {
        let types: serde_json::Map<String, serde_json::Value> = popt::TYPES
            .iter()
            .zip(self.types)
            .map(|((key, _), on)| (key.to_string(), on.into()))
            .collect();
        serde_json::json!({
            "overtime": self.overtime,
            "availability": self.availability,
            "limits": self.limits,
            "overallocation": self.overallocation,
            "legend": self.legend,
            "series": {
                "activity": popt::series_json(&self.activity),
                "resource": popt::series_json(&self.resource),
            },
            "types": types,
            "graph": self.graph.json(),
        })
    }
    /// Reads a stored form. One stored before the series could be switched
    /// draws what it drew then (`profile_options::earlier_series`).
    pub fn restore(&mut self, v: &serde_json::Value) {
        for (key, _) in Self::FIELDS {
            if let (Some(b), Some(flag)) = (v[key].as_bool(), self.flag(key)) {
                *flag = b;
            }
        }
        if v["series"].is_object() {
            popt::restore_series(&mut self.activity, &v["series"]["activity"]);
            popt::restore_series(&mut self.resource, &v["series"]["resource"]);
        } else {
            self.activity = popt::earlier_series(false);
            self.resource = popt::earlier_series(true);
        }
        for (k, (key, _)) in popt::TYPES.iter().enumerate() {
            if let Some(b) = v["types"][key].as_bool() {
                self.types[k] = b;
            }
        }
        self.graph.restore(&v["graph"]);
    }
}

/// How a spreadsheet list is ordered: a field id and the direction.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SheetSort {
    pub field: Option<String>,
    pub descending: bool,
}

#[derive(Clone, Debug)]
pub struct UsageState {
    /// Show Usage for: selected activities instead of all activities.
    pub selected_only: bool,
    pub resource: Option<i64>,
    /// Display: costs instead of units.
    pub costs: bool,
    /// Display: running totals instead of the values of each period.
    pub cumulative: bool,
    pub interval: Interval,
    /// Display Activities for selected Time Period: the activity table shows
    /// only the activities working in `period` (picked on the profile).
    pub time_period: bool,
    /// The period picked on the activity profile (model minutes).
    pub period: Option<(i64, i64)>,
    /// Display Activities for selected Resource: the activity table shows
    /// only the activities the selected resource works on.
    pub resource_filter: bool,
    /// Show All Projects: the totals also count the assignments of the
    /// other open projects.
    pub show_all_projects: bool,
    pub profile: ProfileOptions,
    /// Show Actual and Remaining (the profile's right-click menu and Profile
    /// Options): the actual and remaining units of a period stand on each
    /// other in one bar (on) or beside each other (off); the activity
    /// profile is off and the resource profile on by default. With Stacked
    /// Histogram on, each series' segment is split into its actual part (a
    /// darker shade, at the bottom) and its remaining part.
    pub stacked_activity: bool,
    pub stacked_resource: bool,
    /// Stacked Histogram (the profile's right-click menu): the bars split by
    /// resource or by role, with a legend, on the activity and the resource
    /// profile each.
    pub stack_activity: bool,
    pub stack_resource: bool,
    pub stack_by: StackBy,
    /// Line numbers in the leftmost column of the spreadsheets.
    pub line_numbers: bool,
    /// Spreadsheet Fields: the activity columns of the Activity Usage
    /// Spreadsheet, the assignment columns of the Resource Usage
    /// Spreadsheet, and the resource list columns of the spreadsheet and
    /// of the Resource Usage Profile.
    pub activity_fields: Vec<String>,
    pub assignment_fields: Vec<String>,
    pub resource_fields: Vec<String>,
    pub profile_fields: Vec<String>,
    pub assignment_sort: SheetSort,
    /// Spreadsheet Fields > Customize...: the time-phased fields of the
    /// Activity Usage Spreadsheet and of the Resource Usage Spreadsheet,
    /// saved ids (`time_fields::TimeField::id`), one line each.
    pub activity_time_fields: Vec<String>,
    pub resource_time_fields: Vec<String>,
    /// The resource lists of the Resource Usage Profile and Spreadsheet:
    /// each pane's filter, grouping, sort and view.
    pub profile_list: usage_list::ListState,
    pub sheet_list: usage_list::ListState,
    /// The selected rows of the resource lists (Ctrl and Shift add rows);
    /// `resource` is the one clicked last, or `None` for a group or role.
    pub picked: Vec<usage_list::ListKey>,
    pub anchor: Option<usage_list::ListKey>,
    /// Scroll offsets of the two lists (profile, spreadsheet), not saved.
    pub list_scroll: [f32; 2],
    /// How far the two lists' columns are scrolled sideways, not saved.
    pub list_scroll_x: [f32; 2],
    /// Column widths of the two lists as dragged, by field id (a role view
    /// column as `role:<id>`); a field not here has its default width.
    pub list_widths: [BTreeMap<String, f32>; 2],
    /// Width of the left pane (table or list), when dragged.
    pub table_width: Option<f32>,
    /// Width of the resource list in the Resource Usage Spreadsheet, when
    /// dragged.
    pub list_width: Option<f32>,
}

impl Default for UsageState {
    fn default() -> Self {
        Self {
            selected_only: false,
            resource: None,
            costs: false,
            cumulative: false,
            interval: Interval::Timescale,
            time_period: false,
            period: None,
            resource_filter: false,
            show_all_projects: false,
            profile: ProfileOptions::default(),
            stacked_activity: false,
            stacked_resource: true,
            stack_activity: false,
            stack_resource: false,
            stack_by: StackBy::Resource,
            line_numbers: false,
            activity_fields: fields::default_ids(fields::Which::Activity),
            assignment_fields: fields::default_ids(fields::Which::Assignment),
            resource_fields: fields::default_ids(fields::Which::Resource),
            profile_fields: fields::default_ids(fields::Which::ProfileResource),
            assignment_sort: SheetSort::default(),
            activity_time_fields: time_fields::default_ids(time_fields::Sheet::Activity),
            resource_time_fields: time_fields::default_ids(time_fields::Sheet::Resource),
            profile_list: Default::default(),
            sheet_list: Default::default(),
            picked: vec![],
            anchor: None,
            list_scroll: [0.; 2],
            list_scroll_x: [0.; 2],
            list_widths: Default::default(),
            table_width: None,
            list_width: None,
        }
    }
}

fn sort_json(s: &SheetSort) -> serde_json::Value {
    serde_json::json!({"field": s.field, "descending": s.descending})
}

fn restore_sort(v: &serde_json::Value, s: &mut SheetSort, valid: impl Fn(&str) -> bool) {
    if v.is_object() {
        s.field = v["field"].as_str().filter(|f| valid(f)).map(str::to_string);
        s.descending = v["descending"].as_bool().unwrap_or(false);
    }
}

/// A saved width, when it is a sensible one.
fn restore_split(v: &serde_json::Value) -> Option<f32> {
    v.as_f64()
        .map(|w| w as f32)
        .filter(|w| w.is_finite() && (MIN_PANE..=4000.).contains(w))
}

/// Narrowest left pane of a usage view.
pub const MIN_PANE: f32 = 90.;

impl UsageState {
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "selected_only": self.selected_only,
            "costs": self.costs,
            "cumulative": self.cumulative,
            "interval": self.interval.key(),
            "time_period": self.time_period,
            "resource_filter": self.resource_filter,
            "show_all_projects": self.show_all_projects,
            "profile": self.profile.json(),
            "stacked_activity": self.stacked_activity,
            "stacked_resource": self.stacked_resource,
            "stack_activity": self.stack_activity,
            "stack_resource": self.stack_resource,
            "stack_by": self.stack_by.key(),
            "line_numbers": self.line_numbers,
            "fields": {
                "activity": self.activity_fields,
                "assignment": self.assignment_fields,
                "resource": self.resource_fields,
                "profile": self.profile_fields,
            },
            "time_fields": {
                "activity": self.activity_time_fields,
                "resource": self.resource_time_fields,
            },
            "sort": {
                "assignment": sort_json(&self.assignment_sort),
            },
            "list": {
                "profile": self.profile_list.json(),
                "sheet": self.sheet_list.json(),
            },
            "list_widths": {
                "profile": self.list_widths[0],
                "sheet": self.list_widths[1],
            },
            "table_width": self.table_width,
            "list_width": self.list_width,
        })
    }
    pub fn restore(&mut self, v: &serde_json::Value) {
        for (key, field) in [
            ("selected_only", &mut self.selected_only),
            ("costs", &mut self.costs),
            ("cumulative", &mut self.cumulative),
            ("time_period", &mut self.time_period),
            ("resource_filter", &mut self.resource_filter),
            ("show_all_projects", &mut self.show_all_projects),
            ("stacked_activity", &mut self.stacked_activity),
            ("stacked_resource", &mut self.stacked_resource),
            ("stack_activity", &mut self.stack_activity),
            ("stack_resource", &mut self.stack_resource),
            ("line_numbers", &mut self.line_numbers),
        ] {
            if let Some(b) = v[key].as_bool() {
                *field = b;
            }
        }
        if let Some(by) = v["stack_by"].as_str().and_then(StackBy::from_key) {
            self.stack_by = by;
        }
        if let Some(i) = v["interval"].as_str().and_then(Interval::from_key) {
            self.interval = i;
        }
        self.profile.restore(&v["profile"]);
        for (key, which, list) in [
            (
                "activity",
                fields::Which::Activity,
                &mut self.activity_fields,
            ),
            (
                "assignment",
                fields::Which::Assignment,
                &mut self.assignment_fields,
            ),
            (
                "resource",
                fields::Which::Resource,
                &mut self.resource_fields,
            ),
            (
                "profile",
                fields::Which::ProfileResource,
                &mut self.profile_fields,
            ),
        ] {
            if let Some(ids) = fields::restore_ids(&v["fields"][key], which) {
                *list = ids;
            }
        }
        for (key, sheet, list) in [
            (
                "activity",
                time_fields::Sheet::Activity,
                &mut self.activity_time_fields,
            ),
            (
                "resource",
                time_fields::Sheet::Resource,
                &mut self.resource_time_fields,
            ),
        ] {
            if let Some(ids) = time_fields::restore_ids(&v["time_fields"][key], sheet) {
                *list = ids;
            }
        }
        // Layouts saved before each list kept its own choices had one
        // resource sort for both.
        if v["list"].is_object() {
            self.profile_list.restore(&v["list"]["profile"]);
            self.sheet_list.restore(&v["list"]["sheet"]);
        } else {
            for list in [&mut self.profile_list, &mut self.sheet_list] {
                restore_sort(&v["sort"]["resource"], &mut list.sort, |f| {
                    fields::known_resource_field(f)
                });
            }
        }
        restore_sort(&v["sort"]["assignment"], &mut self.assignment_sort, |f| {
            f.len() < 80
        });
        for (ix, key) in ["profile", "sheet"].into_iter().enumerate() {
            if let Some(map) = v["list_widths"][key].as_object() {
                self.list_widths[ix] = map
                    .iter()
                    .filter_map(|(id, w)| {
                        let w = w.as_f64()? as f32;
                        let known = id.strip_prefix("role:").unwrap_or(id);
                        (fields::known_resource_field(known)
                            && (crate::views::grid::MIN_WIDTH..=2000.).contains(&w))
                        .then(|| (id.clone(), w))
                    })
                    .collect();
            }
        }
        if v.get("table_width").is_some() {
            self.table_width = restore_split(&v["table_width"]);
        }
        if v.get("list_width").is_some() {
            self.list_width = restore_split(&v["list_width"]);
        }
    }
    /// The selected rows of the resource lists. A `resource` set from
    /// outside the list (a test, a printout) stands alone.
    pub fn selection(&self) -> Vec<usage_list::ListKey> {
        match self.resource {
            Some(r) if !self.picked.contains(&usage_list::ListKey::Resource(r)) => {
                vec![usage_list::ListKey::Resource(r)]
            }
            _ => self.picked.clone(),
        }
    }
    /// A value as the views show it: units, or costs in the currency format.
    pub fn text(&self, v: f64) -> String {
        value_text(v, None, self.costs)
    }
    /// `text` for resources that count `unit`: a material's unit of
    /// measure, or hours when `None`.
    pub fn text_in(&self, v: f64, unit: Option<&str>) -> String {
        value_text(v, unit, self.costs)
    }
    /// Per-period values as displayed: running totals when cumulative,
    /// starting from `before`, what falls before the first period.
    pub fn shown(&self, before: f64, values: &[f64]) -> Vec<f64> {
        if !self.cumulative {
            return values.to_vec();
        }
        running(before, values)
    }
    /// A time-phased field's per-period values as displayed: running
    /// totals when the field is a cumulative one or Display > Cumulative is
    /// on.
    pub fn shown_field(&self, f: &Figures) -> Vec<f64> {
        if self.cumulative || f.cumulative {
            running(f.before, &f.values)
        } else {
            f.values.clone()
        }
    }
    /// The value column's title of a spreadsheet showing `fields`: the
    /// field and the measure for one field (`Remaining Labor Units`), the
    /// measure alone over several, each line naming its field.
    pub fn value_title(&self, fields: &[TimeField]) -> String {
        match fields {
            [one] => format!("{} {}", one.label(), self.measure()),
            _ => self.measure().to_string(),
        }
    }
    /// What the values measure, for column titles and the legend.
    pub fn measure(&self) -> &'static str {
        if self.costs {
            "Cost"
        } else {
            "Units"
        }
    }
}

/// Overtime units (or costs) per period of `list`, one resource's
/// assignments of `p`: the part of its actual work booked as overtime,
/// spread over the actual dates.
pub fn resource_overtime<'a>(
    p: &ProjectData,
    list: impl IntoIterator<Item = &'a Assignment>,
    periods: &[(i64, i64)],
    costs: bool,
) -> Vec<f64> {
    let mut out = vec![0.; periods.len()];
    let Some(t) = p.document.table("TASKRSRC") else {
        return out;
    };
    let field = if costs { "act_ot_cost" } else { "act_ot_qty" };
    for a in list {
        if p.model.activities[a.task].act_start.is_none() {
            continue;
        }
        let quantity = t
            .value(&t.rows[a.row], field)
            .trim()
            .parse::<f64>()
            .unwrap_or(0.);
        asg::actual_of_into(p, a, quantity, periods, &mut out);
    }
    out
}

/// Resource ids whose type is labor.
pub fn labor_resources(p: &ProjectData) -> BTreeSet<i64> {
    openxer_xer::spread::labor_resources(&p.document)
}

/// Labor assignments of the open project, grouped by activity.
#[cfg(test)]
pub fn labor_assignments(p: &ProjectData) -> BTreeMap<usize, Vec<Assignment>> {
    labor_assignments_in(p, &asg::assignments(p))
}

/// `labor_assignments` from `list`, the project's assignments already built
/// (the clones share their spread plans with `list`).
pub fn labor_assignments_in(
    p: &ProjectData,
    list: &[Assignment],
) -> BTreeMap<usize, Vec<Assignment>> {
    let labor = labor_resources(p);
    let mut out: BTreeMap<usize, Vec<Assignment>> = BTreeMap::new();
    for a in list {
        if labor.contains(&a.rsrc) {
            out.entry(a.task).or_default().push(a.clone());
        }
    }
    out
}

/// The resource types the activity profile counts: labor, nonlabor and
/// material (the first three of `profile_options::TYPES`).
pub type Kinds = [bool; 3];

/// The kinds of `p`'s resources by id: 0 labor, 1 nonlabor (`RT_Equip`), 2
/// material (`RT_Mat`), as `labor_resources` tells labor apart.
fn resource_kinds(p: &ProjectData) -> HashMap<i64, usize> {
    p.document.table("RSRC").map_or_else(HashMap::new, |t| {
        t.rows
            .iter()
            .map(|r| {
                let kind = match t.value(r, "rsrc_type") {
                    "RT_Equip" => 1,
                    "RT_Mat" => 2,
                    _ => 0,
                };
                (crate::records::num(t.value(r, "rsrc_id")), kind)
            })
            .collect()
    })
}

/// `labor_assignments_in` for the resource kinds in `kinds`.
pub fn kind_assignments_in(
    p: &ProjectData,
    list: &[Assignment],
    kinds: Kinds,
) -> BTreeMap<usize, Vec<Assignment>> {
    let of = resource_kinds(p);
    let mut out: BTreeMap<usize, Vec<Assignment>> = BTreeMap::new();
    for a in list {
        // An assignment whose resource is not in RSRC counts as labor, as
        // `labor_resources` would not exclude it either.
        if kinds[of.get(&a.rsrc).copied().unwrap_or(0)] {
            out.entry(a.task).or_default().push(a.clone());
        }
    }
    out
}

/// The expenses (PROJCOST rows) of `p`'s activities in `only` (all when
/// `None`) added into `u` as costs: budgeted over the planned dates, actual,
/// remaining over the remaining early dates and over the late dates, each
/// spread evenly over the activity's dates by the shared spread.
pub fn expenses_into(
    p: &ProjectData,
    periods: &[(i64, i64)],
    only: Option<&BTreeSet<usize>>,
    u: &mut ActivityUsage,
) {
    use openxer_xer::spread;
    let Some(t) = p.document.table("PROJCOST") else {
        return;
    };
    let index: HashMap<i64, usize> = p
        .model
        .activities
        .iter()
        .enumerate()
        .map(|(i, a)| (a.task_id, i))
        .collect();
    let pid = p.model.project.proj_id;
    let has_project = t.field("proj_id").is_some();
    let m = &p.model;
    for r in &t.rows {
        if has_project && crate::records::num(t.value(r, "proj_id")) != pid {
            continue;
        }
        let Some(&task) = index.get(&crate::records::num(t.value(r, "task_id"))) else {
            continue;
        };
        if only.is_some_and(|o| !o.contains(&task)) {
            continue;
        }
        let activity = &m.activities[task];
        let item = spread::Item {
            activity,
            original: p
                .details
                .get(&activity.task_id)
                .map_or(0, |d| d.original_duration),
            shaping: None,
        };
        let plan = spread::Plan::of(m, &item);
        let v = |k: &str| crate::records::float(t.value(r, k));
        let (budget, actual, remaining) = (v("target_cost"), v("act_cost"), v("remain_cost"));
        spread::budget(m, &item, &plan, budget, periods, &mut u.budget);
        spread::actual(m, &item, &plan, actual, periods, &mut u.actual);
        spread::remaining(m, &item, &plan, remaining, periods, &mut u.remaining);
        spread::remaining_late(m, &item, &plan, remaining, periods, &mut u.remaining_late);
    }
}

/// A parked project's assignments, kept while the open set is unchanged:
/// the list, its positions by resource, resource ids by Resource ID and
/// its labor assignments by activity.
pub struct ParkedAssignments {
    pub list: Vec<Assignment>,
    pub by_resource: HashMap<i64, Vec<usize>>,
    pub by_code: HashMap<String, i64>,
    pub labor: BTreeMap<usize, Vec<Assignment>>,
}

impl ParkedAssignments {
    pub fn of(p: &ProjectData) -> Self {
        let list = asg::assignments(p);
        let mut by_code = HashMap::new();
        for r in rs::resources(p) {
            by_code.entry(r.code).or_insert(r.id);
        }
        Self {
            by_resource: asg::by_resource(&list),
            labor: labor_assignments_in(p, &list),
            by_code,
            list,
        }
    }

    /// Resource `rsrc`'s assignments.
    pub fn of_resource(&self, rsrc: i64) -> impl Iterator<Item = &Assignment> {
        self.by_resource
            .get(&rsrc)
            .into_iter()
            .flatten()
            .map(|&k| &self.list[k])
    }
}

/// Labor units per period of the activities in `only` (all when `None`):
/// budgeted over the planned dates, actual up to the data date, remaining
/// over the remaining early dates and remaining over the late dates.
#[cfg(test)]
pub fn activity_usage(
    p: &ProjectData,
    by_task: &BTreeMap<usize, Vec<Assignment>>,
    periods: &[(i64, i64)],
    only: Option<&BTreeSet<usize>>,
) -> ActivityUsage {
    activity_usage_of(p, by_task, periods, only, false)
}

/// `activity_usage` in units, or in costs when `costs` is set.
pub fn activity_usage_of(
    p: &ProjectData,
    by_task: &BTreeMap<usize, Vec<Assignment>>,
    periods: &[(i64, i64)],
    only: Option<&BTreeSet<usize>>,
    costs: bool,
) -> ActivityUsage {
    let n = periods.len();
    let mut u = ActivityUsage {
        budget: vec![0.; n],
        actual: vec![0.; n],
        remaining: vec![0.; n],
        remaining_late: vec![0.; n],
    };
    for (&task, list) in by_task {
        if only.is_some_and(|o| !o.contains(&task)) {
            continue;
        }
        for x in list {
            let costed;
            let x = if costs {
                costed = x.as_costs();
                &costed
            } else {
                x
            };
            asg::budget_into(p, x, periods, &mut u.budget);
            asg::actual_into(p, x, periods, &mut u.actual);
            asg::remaining_into(p, x, periods, &mut u.remaining);
            asg::remaining_late_into(p, x, periods, &mut u.remaining_late);
        }
    }
    u
}

/// A profile's timescale: axis, upper and lower band cells, and periods.
type Frame<'a> = (&'a TimeScale, &'a [Cell], &'a [Cell], &'a [(i64, i64)]);

/// Cumulative over a Stacked Histogram: the running total of the stacks,
/// from `before`, what all of them hold before the first period.
fn curves_of(series: &[stacked::Series], cumulative: bool, before: f64) -> Vec<Curve> {
    if !cumulative || series.is_empty() {
        return vec![];
    }
    let n = series[0].values.len();
    let total: Vec<f64> = (0..n)
        .map(|k| series.iter().map(|s| s.values[k]).sum())
        .collect();
    vec![Curve {
        totals: running(before, &total),
        color: INK,
        dashed: false,
    }]
}

/// Stacked series spread over `edged` periods: the series over the
/// periods, and what all of them hold before the first period.
fn unedge_series(series: Vec<stacked::Series>) -> (Vec<stacked::Series>, f64) {
    let mut before = 0.;
    let series = series
        .into_iter()
        .map(|s| {
            let (b, values, _) = unedge(&s.values);
            before += b;
            stacked::Series {
                values,
                actual: unedge(&s.actual).1,
                ..s
            }
        })
        .collect();
    (series, before)
}

/// A spreadsheet value as the usage views show it: blank for nothing,
/// units (a material's in its unit of measure, `unit`), or costs in the
/// currency format.
pub(crate) fn value_text(v: f64, unit: Option<&str>, costs: bool) -> String {
    match unit {
        Some(u) if !costs => usage_list::material_text(v, u),
        _ if v.abs() < 0.005 => String::new(),
        _ if costs => format::cost(v),
        _ => format::units(v),
    }
}

/// Running totals of per-period values, from `before`.
pub(crate) fn running(before: f64, values: &[f64]) -> Vec<f64> {
    let mut sum = before;
    values
        .iter()
        .map(|v| {
            sum += v;
            sum
        })
        .collect()
}

/// What a period cell shows: `text` when it fits in `width`, else `###` (or
/// `#`), so a value too wide for its cell never reads as an empty one.
pub(crate) fn cell_text(text: &str, width: f32, measure: impl Fn(&str) -> f32) -> Option<String> {
    if text.is_empty() {
        return None;
    }
    [text, "###", "#"]
        .into_iter()
        .find(|t| measure(t) <= width)
        .map(str::to_string)
}

/// The period cells of one spreadsheet row from `cells_left`: the cell
/// lines and each value right-aligned in its cell (see `cell_text`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_period_cells(
    p: &egui::Painter,
    row: Rect,
    axis: &TimeScale,
    periods: &[(i64, i64)],
    cells_left: f32,
    values: &[String],
    font: &FontId,
    ink: Color32,
) {
    let cp = p.with_clip_rect(Rect::from_min_max(
        Pos2::new(cells_left, row.top()),
        row.max,
    ));
    let measure = |t: &str| cp.layout_no_wrap(t.to_string(), font.clone(), ink).size().x;
    for (n, &(a, b)) in periods.iter().enumerate() {
        let x0 = axis.x(a as f64).round();
        let x1 = axis.x(b as f64).round();
        chrome::vline(&cp, x1 - 1., row.top(), row.bottom(), LINE);
        let text = values.get(n).map_or("", String::as_str);
        if let Some(text) = cell_text(text, x1 - x0 - 2., measure) {
            let g = cp.layout_no_wrap(text, font.clone(), ink);
            cp.galley(
                Pos2::new(x1 - 3. - g.size().x, row.center().y - g.size().y / 2.),
                g,
                ink,
            );
        }
    }
}

/// The value column and period cells of a usage spreadsheet row (`label`
/// names its time-phased field when there are several): the total right
/// aligned in the value column, then the period values (`paint_period_cells`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_values(
    p: &egui::Painter,
    row: Rect,
    axis: &TimeScale,
    periods: &[(i64, i64)],
    (value_left, cells_left): (f32, f32),
    label: Option<&str>,
    total: String,
    values: &[String],
    font: &FontId,
    ink: Color32,
) {
    // The column is sized to the widest total (`usage_value_width`).
    let value = p.with_clip_rect(Rect::from_min_max(
        Pos2::new(value_left, row.top()),
        Pos2::new(cells_left, row.bottom()),
    ));
    if let Some(label) = label {
        value.text(
            Pos2::new(value_left + 3., row.center().y),
            Align2::LEFT_CENTER,
            label,
            font.clone(),
            ink,
        );
    }
    value.text(
        Pos2::new(cells_left - 4., row.center().y),
        Align2::RIGHT_CENTER,
        total,
        font.clone(),
        ink,
    );
    paint_period_cells(p, row, axis, periods, cells_left, values, font, ink);
}

/// One timescale band under a profile chart (the S-Curve report's too).
pub(crate) fn paint_tier(
    p: &egui::Painter,
    rect: Rect,
    axis: &TimeScale,
    unit: Unit,
    cells: &[Cell],
) {
    crate::timeline::paint_band(p, rect, axis, unit, cells, &chrome::table_font(), INK);
}

impl App {
    /// The usage views' time axis: the Gantt's zoom and first date, starting
    /// at `left`.
    fn usage_axis(&self, left: f32) -> TimeScale {
        TimeScale::new(
            self.timeline_origin as f64,
            self.pixels_per_day as f64 * 7.,
            left,
        )
    }

    #[cfg(test)]
    pub(crate) fn usage_axis_for_tests(&self, left: f32) -> TimeScale {
        self.usage_axis(left)
    }

    fn usage_cells(&self, axis: &TimeScale, right: f32) -> (Vec<Cell>, Vec<Cell>) {
        let origin = self.project.model.origin_serial;
        let last = axis.x_date(right);
        let options = self.label_options();
        (
            timescale::band_cells(
                self.view.upper,
                true,
                origin,
                axis.first_minute,
                last,
                &options,
            ),
            timescale::band_cells(
                self.view.lower,
                false,
                origin,
                axis.first_minute,
                last,
                &options,
            ),
        )
    }

    /// Activities counted by Show Usage for.
    fn usage_scope(&self) -> Option<BTreeSet<usize>> {
        self.panes.usage.selected_only.then(|| {
            (0..self.project.model.activities.len())
                .filter(|&i| self.is_selected(i))
                .collect()
        })
    }

    /// The usage views' periods: the timescale's lower tier, or the Time
    /// Interval's periods across the same dates.
    fn usage_periods(&self, axis: &TimeScale, right: f32, lower: &[Cell]) -> Vec<(i64, i64)> {
        let cells = match self.panes.usage.interval.unit() {
            None => lower.to_vec(),
            Some(unit) => timescale::cells(
                unit,
                None,
                self.project.model.origin_serial,
                axis.first_minute,
                axis.x_date(right),
            ),
        };
        cells
            .iter()
            .map(|c| (c.start_minute, c.end_minute))
            .collect()
    }

    /// The usage periods (the Time Interval's, or the timescale's lower
    /// tier) from `first` to `last`, model minutes: Copy to Excel writes
    /// the whole data this way, not only the periods on show.
    fn usage_periods_over(&self, first: f64, last: f64) -> Vec<(i64, i64)> {
        let origin = self.project.model.origin_serial;
        let cells = match self.panes.usage.interval.unit() {
            None => timescale::band_cells(
                self.view.lower,
                false,
                origin,
                first,
                last,
                &self.label_options(),
            ),
            Some(unit) => timescale::cells(unit, None, origin, first, last),
        };
        cells
            .iter()
            .map(|c| (c.start_minute, c.end_minute))
            .collect()
    }

    /// The Display menu of the usage views: units or costs, cumulative and
    /// the time interval. These are view settings saved with the layout.
    pub(crate) fn usage_display_menu(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(170.);
        let u = self.panes.usage.clone();
        if self.item(
            ui,
            "usage_display_units",
            Item::new("Units").checked(!u.costs),
        ) {
            self.panes.usage.costs = false;
        }
        if self.item(
            ui,
            "usage_display_costs",
            Item::new("Costs").checked(u.costs),
        ) {
            self.panes.usage.costs = true;
        }
        chrome::menu_separator(ui);
        // On a profile, Cumulative switches the running-total curves of all
        // its series (Profile Options switches them one by one); on a
        // spreadsheet, the running totals of its periods.
        let profile = match self.panes.bottom {
            BottomView::ActivityUsageProfile => Some(false),
            BottomView::ResourceUsageProfile => Some(true),
            _ => None,
        };
        let checked = profile.map_or(u.cumulative, |r| u.profile.any_cumulative(r));
        if self.item(
            ui,
            "usage_display_cumulative",
            Item::new("Cumulative").checked(checked),
        ) {
            match profile {
                Some(resource) => {
                    // On: a curve for each series drawn as bars.
                    for s in self.panes.usage.profile.series_mut(resource) {
                        s.cumulative = !checked && s.by_date;
                    }
                }
                None => self.panes.usage.cumulative = !u.cumulative,
            }
        }
        chrome::menu_separator(ui);
        chrome::submenu(ui, Item::new("Time Interval"), |ui| {
            for interval in Interval::ALL {
                let item = Item::new(interval.caption()).checked(u.interval == interval);
                if self.item(ui, &format!("usage_interval_{}", interval.key()), item) {
                    self.panes.usage.interval = interval;
                }
            }
        });
    }

    /// Display Activities for selected Time Period and Resource: with the
    /// box checked and a period picked on the activity profile, keep the
    /// activities whose dates overlap the period; with Resource checked on
    /// a resource view, keep the activities the selected resource works on.
    /// The bands that still hold an activity stay.
    pub(crate) fn time_period_rows(&self, rows: Vec<grid::Row>) -> Vec<grid::Row> {
        let (period, members) = self.usage_table_filter();
        if period.is_none() && members.is_none() {
            return rows;
        }
        let assigned: Option<BTreeSet<usize>> = members.map(|(resources, roles)| {
            let list = self.memo(self.project_serial(), "assignments", (), || {
                asg::assignments(&self.project)
            });
            list.iter()
                .filter(|a| {
                    resources.contains(&a.rsrc) || roles.contains(&crate::records::num(&a.role))
                })
                .map(|a| a.task)
                .collect()
        });
        let shown = self.shown();
        let m = &shown.model;
        let working = |i: usize| {
            let Some(a) = m.activities.get(i) else {
                return false;
            };
            if assigned.as_ref().is_some_and(|set| !set.contains(&i)) {
                return false;
            }
            let Some((from, to)) = period else {
                return true;
            };
            let start = a.act_start.or(displayed_start(a, m.options.do_level));
            let finish = a.act_finish.or(displayed_finish(a, m.options.do_level));
            match (start, finish) {
                (Some(s), Some(f)) => s < to && f.max(s + 1) > from,
                _ => false,
            }
        };
        let bands = self.grid.bands.borrow();
        let level = |b: usize| bands.get(b).map_or(0, |b| b.level);
        let keep_activity: Vec<bool> = rows
            .iter()
            .map(|r| matches!(r, grid::Row::Activity(i) if working(*i)))
            .collect();
        let mut out = vec![];
        for (k, row) in rows.iter().enumerate() {
            match *row {
                grid::Row::Activity(_) => {
                    if keep_activity[k] {
                        out.push(*row);
                    }
                }
                grid::Row::Group(b) => {
                    // A band stays when a kept activity follows before the
                    // next band at its level or above.
                    let mine = level(b);
                    let holds = rows[k + 1..]
                        .iter()
                        .zip(&keep_activity[k + 1..])
                        .take_while(|(r, _)| !matches!(r, grid::Row::Group(o) if level(*o) <= mine))
                        .any(|(_, keep)| *keep);
                    if holds {
                        out.push(*row);
                    }
                }
            }
        }
        out
    }

    /// The strip along the bottom of a usage view that holds its
    /// timescale's scroll bar (none in a printout), and the rest of the view.
    fn usage_strip(&self, rect: Rect) -> (Rect, Option<Rect>) {
        if self.panes.printing || rect.height() < 4. * BAR_HEIGHT {
            return (rect, None);
        }
        let top = (rect.bottom() - BAR_HEIGHT).round();
        (
            Rect::from_min_max(rect.min, Pos2::new(rect.right(), top)),
            Some(Rect::from_min_max(Pos2::new(rect.left(), top), rect.max)),
        )
    }

    /// A usage view's timescale scrolls and zooms like the Gantt's, whose
    /// timescale it shares: a drag on `pan` pans and on `zoom` zooms, the
    /// wheel pans over the timescale, the chart and the bar, Ctrl+wheel over
    /// `body` zooms around the pointer, and the bar in `strip` (under the
    /// time axis, from `left`) scrolls. A right-click opens the view's menu.
    #[allow(clippy::too_many_arguments)]
    fn usage_timeline(
        &mut self,
        ui: &mut egui::Ui,
        name: &str,
        menu: UsageMenu,
        pan: Rect,
        zoom: Rect,
        body: Rect,
        strip: Option<Rect>,
        left: f32,
    ) {
        let right = pan.right().max(zoom.right());
        let chart = crate::timeline::Chart {
            pan,
            zoom,
            body,
            bar: strip.map(|s| {
                Rect::from_min_max(
                    Pos2::new(left, s.top()),
                    Pos2::new(right.max(left), s.bottom()),
                )
            }),
            left,
            width: (right - left).max(0.),
        };
        let id = ui.id().with((name, "timeline"));
        let outcome = self.main_control(ui, id, &chart);
        // `name` is the whole timescale (both bands), as before.
        let both = pan.union(zoom);
        self.action_rects.insert(
            name.to_string(),
            [both.left(), both.top(), both.right(), both.bottom()],
        );
        self.action(&format!("{name}_pan"), &outcome.pan);
        self.action(&format!("{name}_zoom"), &outcome.zoom);
        self.action_rects.insert(
            format!("{name}_body"),
            [body.left(), body.top(), body.right(), body.bottom()],
        );
        self.usage_menu(&outcome.pan, menu);
        self.usage_menu(&outcome.zoom, menu);
        if let Some(strip) = strip {
            ui.painter().rect_filled(strip, 0., chrome::pal::FACE);
            self.main_bar(ui, id, &format!("{name}_hscroll"), &chart);
        }
    }

    /// Column headers over a list: face fill, black dividers and bottom line.
    pub(super) fn list_header(p: &egui::Painter, rect: Rect, columns: &[(&str, f32)]) {
        p.rect_filled(rect, 0., CHROME);
        let mut x = rect.left();
        for (title, width) in columns {
            let cell = Rect::from_min_max(
                Pos2::new(x, rect.top()),
                Pos2::new((x + width).min(rect.right()), rect.bottom()),
            );
            let g = p.layout(
                (*title).to_string(),
                chrome::table_font(),
                INK,
                (cell.width() - 6.).max(10.),
            );
            p.with_clip_rect(cell)
                .galley(cell.min + Vec2::new(3., 3.), g, INK);
            chrome::vline(p, cell.right() - 1., rect.top(), rect.bottom(), INK);
            x += width;
        }
        chrome::hline(p, rect.left(), rect.right(), rect.bottom() - 1., INK);
    }

    /// The left pane's width: the dragged one, or the table's own.
    pub(crate) fn usage_table_width(&self, default: f32, total: f32) -> f32 {
        self.panes.usage.table_width.map_or(default, |w| {
            w.clamp(usage::MIN_PANE, (total - 150.).max(usage::MIN_PANE))
        })
    }

    /// A 6 px splitter face that can be dragged: `set` receives the new
    /// width of the pane to its left, measured from `left`.
    fn usage_splitter(
        &mut self,
        ui: &mut egui::Ui,
        face: Rect,
        left: f32,
        name: &str,
        limits: (f32, f32),
        set: fn(&mut UsageState, f32),
    ) {
        crate::activities::paint_splitter(ui.painter(), face, true);
        let drag = ui
            .interact(face, ui.id().with(name), Sense::drag())
            .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        self.action(name, &drag);
        if let Some(pos) = drag.interact_pointer_pos().filter(|_| drag.dragged()) {
            let width = (pos.x - left - face.width() / 2.).round();
            set(
                &mut self.panes.usage,
                width.clamp(limits.0, limits.1.max(limits.0)),
            );
        }
    }

    /// The field cells of activity `i`, as the activity table shows them.
    fn activity_cells(&self, fields: &[fields::Field], ix: &cols::Index, i: usize) -> Vec<String> {
        let shown = self.shown();
        fields
            .iter()
            .map(|f| {
                f.column.as_ref().map_or(String::new(), |c| {
                    cols::display(shown, Some(i), &cols::shown_value(shown, ix, c, i))
                })
            })
            .collect()
    }

    /// The Activity Usage Spreadsheet's time-phased fields.
    pub(crate) fn activity_time_fields(&self) -> Vec<TimeField> {
        time_fields::fields_of(
            time_fields::Sheet::Activity,
            &self.panes.usage.activity_time_fields,
        )
    }

    /// The Resource Usage Spreadsheet's time-phased fields.
    pub(crate) fn resource_time_fields(&self) -> Vec<TimeField> {
        time_fields::fields_of(
            time_fields::Sheet::Resource,
            &self.panes.usage.resource_time_fields,
        )
    }

    /// One row of the Activity Usage Spreadsheet for drawing and for Copy
    /// to Excel: the field cells, then each time-phased field's total and
    /// the value of each period.
    pub(crate) fn activity_sheet_line(
        &self,
        kind: grid::Row,
        fields: &[fields::Field],
        ix: &cols::Index,
        periods: &[(i64, i64)],
        serial: Option<u64>,
        costs: bool,
    ) -> SheetLine {
        let (members, fill, ink, texts, selected) = match kind {
            grid::Row::Group(b) => {
                let level = self.grid.bands.borrow().get(b).map_or(0, |b| b.level);
                let style = self.grid.layout.group_sort.band_style(level);
                let c = |x: [u8; 3]| Color32::from_rgb(x[0], x[1], x[2]);
                (
                    self.band_members(b),
                    c(style.background),
                    c(style.text),
                    vec![self.band_label(b)],
                    false,
                )
            }
            grid::Row::Activity(i) => {
                let selected = self.is_selected(i);
                (
                    vec![i],
                    if selected { SELECT } else { Color32::WHITE },
                    if selected { Color32::WHITE } else { INK },
                    self.activity_cells(fields, ix, i),
                    selected,
                )
            }
        };
        let scope: BTreeSet<usize> = members.into_iter().collect();
        let chosen = self.activity_time_fields();
        let key = (
            periods.to_vec(),
            scope.iter().copied().collect::<Vec<_>>(),
            costs,
            chosen.iter().map(|f| f.id()).collect::<Vec<_>>(),
        );
        let figures = self.memo(serial, "activity_usage_row", key, || {
            let by_task = self.all_by_task(serial);
            let kinds = self.memo(serial, "resource_kinds", (), || {
                time_fields::kinds(&self.project)
            });
            let edges = edged(periods);
            chosen
                .iter()
                .map(|f| {
                    let (values, total) = time_fields::activity_into(
                        &self.project,
                        &by_task,
                        &kinds,
                        &scope,
                        *f,
                        costs,
                        &edges,
                    );
                    Figures::edged(*f, &values, total)
                })
                .collect::<Vec<_>>()
        });
        SheetLine {
            band: matches!(kind, grid::Row::Group(_)),
            selected,
            fill,
            ink,
            texts,
            lines: (*figures).clone(),
        }
    }

    /// Every assignment of the active project by activity, cloned from
    /// `assignment_list` so they share its spread plans.
    pub(crate) fn all_by_task(&self, serial: Option<u64>) -> Rc<BTreeMap<usize, Vec<Assignment>>> {
        self.memo(serial, "all_assignments_by_task", (), || {
            time_fields::by_task(&self.assignment_list(serial))
        })
    }

    /// The remaining units of the other open projects, per period, for Show
    /// All Projects: their own labor assignments over the same dates, with
    /// what falls before and after the periods (as `unedge` splits them),
    /// and their stored total. Kept while the periods and the open set are
    /// unchanged.
    #[allow(clippy::type_complexity)]
    pub(crate) fn other_projects_usage(
        &self,
        periods: &[(i64, i64)],
        costs: bool,
    ) -> Rc<((f64, Vec<f64>, f64), f64)> {
        let key = (periods.to_vec(), costs, self.open_set.version);
        self.memo(self.project_serial(), "other_projects_usage", key, || {
            let edges = edged(periods);
            let mut values = vec![0.; edges.len()];
            let mut total = 0.;
            let origin = self.project.model.origin_serial;
            for (k, other) in self.open_set.parked.iter().map(|p| &p.project).enumerate() {
                let shift = (origin - other.model.origin_serial) * DAY;
                let moved: Vec<(i64, i64)> =
                    edges.iter().map(|(a, b)| (a + shift, b + shift)).collect();
                let parked = self.parked_assignments(k);
                let u = activity_usage_of(other, &parked.labor, &moved, None, costs);
                for (v, x) in values.iter_mut().zip(&u.remaining) {
                    *v += x;
                }
                total += parked
                    .labor
                    .values()
                    .flatten()
                    .map(|a| if costs { a.remaining_cost } else { a.remaining })
                    .sum::<f64>();
            }
            (unedge(&values), total)
        })
    }

    /// The columns of a spreadsheet's left pane: the line number column
    /// when asked for, then the fields.
    pub(super) fn sheet_columns<'f>(
        fields: &'f [fields::Field],
        widths: &[f32],
        numbers: bool,
    ) -> Vec<(&'f str, f32)> {
        let mut out = vec![];
        if numbers {
            out.push(("", NUMBER_W));
        }
        out.extend(
            fields
                .iter()
                .zip(widths)
                .map(|(f, w)| (f.title.as_str(), *w)),
        );
        out
    }

    /// Draws the cells of one spreadsheet row's left pane, line number
    /// first: each field in its column, aligned as the activity table does.
    #[allow(clippy::too_many_arguments)]
    fn paint_sheet_cells(
        p: &egui::Painter,
        row: Rect,
        left: f32,
        number: Option<usize>,
        fields: &[fields::Field],
        widths: &[f32],
        texts: &[String],
        font: &FontId,
        ink: Color32,
    ) {
        let mut x = left;
        if let Some(n) = number {
            let cell = Rect::from_min_max(
                Pos2::new(x, row.top()),
                Pos2::new(x + NUMBER_W, row.bottom()),
            );
            p.with_clip_rect(cell).text(
                Pos2::new(cell.right() - 3., cell.center().y),
                Align2::RIGHT_CENTER,
                n.to_string(),
                font.clone(),
                ink,
            );
            chrome::vline(p, cell.right() - 1., row.top(), row.bottom(), LINE);
            x += NUMBER_W;
        }
        for ((field, width), text) in fields.iter().zip(widths).zip(texts) {
            let cell = Rect::from_min_max(
                Pos2::new(x, row.top()),
                Pos2::new((x + width).min(row.right()), row.bottom()),
            );
            let (anchor, align) = match field.column.as_ref().map(|c| c.align) {
                Some(cols::Align::Right) => (cell.right() - 3., Align2::RIGHT_CENTER),
                Some(cols::Align::Center) => (cell.center().x, Align2::CENTER_CENTER),
                _ => (cell.left() + 3., Align2::LEFT_CENTER),
            };
            p.with_clip_rect(cell).text(
                Pos2::new(anchor, cell.center().y),
                align,
                text,
                font.clone(),
                ink,
            );
            chrome::vline(p, cell.right() - 1., row.top(), row.bottom(), LINE);
            x += width;
        }
    }

    /// Activity Usage Spreadsheet: the activity rows at the left, the
    /// remaining labor units per timescale period at the right.
    pub(crate) fn activity_usage_sheet(&mut self, ui: &mut egui::Ui, rect: Rect, table_w: f32) {
        let table_w = self.usage_table_width(table_w, rect.width());
        let (rect, strip) = self.usage_strip(rect);
        let p = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        p.rect_filled(rect, 0., Color32::WHITE);
        let split = rect.left() + table_w;
        let value_left = split + 6.;
        let value_w = self.usage_value_width(ui.ctx(), true);
        let cells_left = value_left + value_w;
        let axis = self.usage_axis(cells_left);
        let (upper, lower) = self.usage_cells(&axis, rect.right());
        let head = Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.top() + HEADER_H));
        let numbers = self.panes.usage.line_numbers;
        let sheet_fields = fields::fields_of(
            &self.project,
            fields::Which::Activity,
            &self.panes.usage.activity_fields,
        );
        let widths =
            fields::widths_in(&sheet_fields, table_w - if numbers { NUMBER_W } else { 0. });
        Self::list_header(
            &p,
            Rect::from_min_max(head.min, Pos2::new(split, head.bottom())),
            &Self::sheet_columns(&sheet_fields, &widths, numbers),
        );
        let value_head = Rect::from_min_max(
            Pos2::new(value_left, head.top()),
            Pos2::new(cells_left, head.bottom()),
        );
        let value_title = self.panes.usage.value_title(&self.activity_time_fields());
        Self::list_header(&p, value_head, &[(value_title.as_str(), value_w)]);
        self.usage_display_button(ui, value_head, "usage_sheet_display");
        let tiers = Rect::from_min_max(Pos2::new(cells_left, head.top()), head.max);
        let tp = p.with_clip_rect(tiers);
        paint_tier(
            &tp,
            Rect::from_min_size(tiers.min, Vec2::new(tiers.width(), TIER_H)),
            &axis,
            self.view.upper,
            &upper,
        );
        paint_tier(
            &tp,
            Rect::from_min_max(Pos2::new(tiers.left(), tiers.top() + TIER_H), tiers.max),
            &axis,
            self.view.lower,
            &lower,
        );
        chrome::hline(&p, tiers.left(), tiers.right(), head.bottom() - 1., INK);
        self.usage_timeline(
            ui,
            "usage_sheet_timescale",
            UsageMenu::ActivitySheet,
            Rect::from_min_size(tiers.min, Vec2::new(tiers.width(), TIER_H)),
            Rect::from_min_max(Pos2::new(tiers.left(), tiers.top() + TIER_H), tiers.max),
            Rect::from_min_max(Pos2::new(cells_left, head.bottom()), rect.max),
            strip,
            cells_left,
        );
        // Splitter between the table and the spreadsheet.
        self.usage_splitter(
            ui,
            Rect::from_min_max(
                Pos2::new(split, rect.top()),
                Pos2::new(value_left, rect.bottom()),
            ),
            rect.left(),
            "usage_sheet_splitter",
            (usage::MIN_PANE, rect.width() - 150.),
            |u, w| u.table_width = Some(w),
        );
        let periods = self.usage_periods(&axis, rect.right(), &lower);
        self.panes.sheet_periods = periods.clone();
        let serial = self.project_serial();
        let ix = self.shared_index();
        let display = self.panes.usage.clone();
        let rows = self.rows();
        // Several time-phased fields: a row is tall enough for one line each.
        let lines_per_row = self.activity_time_fields().len().max(1);
        let mut metrics = self.row_metrics(&rows);
        if lines_per_row > 1 {
            let least = lines_per_row as f32 * ROW_H;
            let mut top = 0.;
            let offsets = std::rc::Rc::make_mut(&mut metrics.offsets);
            for (k, h) in metrics.heights.iter_mut().enumerate() {
                *h = h.max(least);
                offsets[k] = top;
                top += *h;
            }
            if let Some(last) = offsets.last_mut() {
                *last = top;
            }
        }
        let body = Rect::from_min_max(Pos2::new(rect.left(), head.bottom()), rect.max);
        // Right-click below the last row.
        let background = ui.interact(body, ui.id().with("usage_sheet_body"), Sense::click());
        self.usage_menu(&background, UsageMenu::ActivitySheet);
        let others = display
            .show_all_projects
            .then(|| self.other_projects_usage(&periods, display.costs))
            .filter(|_| !self.open_set.parked.is_empty());
        let extra = if others.is_some() { ROW_H } else { 0. };
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(body)
                .layout(Layout::top_down(Align::LEFT)),
        );
        child.set_clip_rect(body.intersect(ui.clip_rect()));
        let mut click = None;
        let mut right = None;
        let ctrl = ui.input(|i| i.modifiers.ctrl);
        egui::ScrollArea::vertical()
            .id_salt("activity_usage_sheet")
            .auto_shrink([false, false])
            // Ctrl+wheel zooms the timescale instead.
            // No drag-to-scroll: a press (either button) would grab the
            // sheet and scroll it by the pointer's travel that frame, so a
            // right-click made while the pointer jumps lands on another row
            // or none, and the menu never opens.
            .scroll_source(egui::scroll_area::ScrollSource {
                mouse_wheel: !ctrl,
                drag: false,
                ..Default::default()
            })
            .show_viewport(&mut child, |ui, viewport| {
                ui.set_height(*metrics.offsets.last().unwrap_or(&0.) + extra);
                let top = ui.max_rect().top();
                let p = ui.painter().clone();
                let first = metrics.row_at(viewport.min.y);
                let end = (metrics.row_at(viewport.max.y) + 1).min(rows.len());
                let font = chrome::table_font();
                for (k, &kind) in rows.iter().enumerate().take(end).skip(first) {
                    let y = top + metrics.offsets[k];
                    let h = metrics.heights[k];
                    let row =
                        Rect::from_min_size(Pos2::new(body.left(), y), Vec2::new(body.width(), h));
                    let response =
                        ui.interact(row, ui.id().with(("usage_sheet_row", k)), Sense::click());
                    self.action(&format!("usage_sheet_row_{k}"), &response);
                    if let grid::Row::Activity(i) = kind {
                        if response.clicked() {
                            click = Some(i);
                        }
                        if response.secondary_clicked() {
                            right = Some(i);
                        }
                    }
                    self.usage_menu(&response, UsageMenu::ActivitySheet);
                    let line = self.activity_sheet_line(
                        kind,
                        &sheet_fields,
                        &ix,
                        &periods,
                        serial,
                        display.costs,
                    );
                    let (fill, ink, band, selected) =
                        (line.fill, line.ink, line.band, line.selected);
                    let table = Rect::from_min_max(row.min, Pos2::new(split, row.bottom()));
                    p.rect_filled(table, 0., fill);
                    let sheet = Rect::from_min_max(Pos2::new(value_left, row.top()), row.max);
                    p.rect_filled(
                        sheet,
                        0.,
                        if band || selected {
                            fill
                        } else {
                            Color32::WHITE
                        },
                    );
                    let text_font = if band {
                        chrome::bold_font()
                    } else {
                        font.clone()
                    };
                    let number = numbers.then_some(k + 1);
                    if band {
                        let mut x = table.left() + 4.;
                        if let Some(n) = number {
                            p.with_clip_rect(table).text(
                                Pos2::new(table.left() + NUMBER_W - 3., table.center().y),
                                Align2::RIGHT_CENTER,
                                n.to_string(),
                                text_font.clone(),
                                ink,
                            );
                            x += NUMBER_W;
                        }
                        p.with_clip_rect(table).text(
                            Pos2::new(x, table.center().y),
                            Align2::LEFT_CENTER,
                            line.texts.first().map_or("", String::as_str),
                            text_font.clone(),
                            ink,
                        );
                    } else {
                        Self::paint_sheet_cells(
                            &p,
                            table,
                            table.left(),
                            number,
                            &sheet_fields,
                            &widths,
                            &line.texts,
                            &font,
                            ink,
                        );
                    }
                    chrome::hline(&p, table.left(), table.right(), row.bottom() - 1., LINE);
                    // One line per time-phased field, sharing the row's
                    // height.
                    let count = line.lines.len().max(1);
                    let pitch = row.height() / count as f32;
                    for (n, figures) in line.lines.iter().enumerate() {
                        let sub = Rect::from_min_size(
                            Pos2::new(row.left(), row.top() + pitch * n as f32),
                            Vec2::new(row.width(), pitch),
                        );
                        let shown = display.shown_field(figures);
                        self.paint_sheet_values(
                            &p,
                            sub,
                            &axis,
                            &periods,
                            (value_left, cells_left),
                            (count > 1).then_some(figures.label.as_str()),
                            display.text(figures.total),
                            &shown.iter().map(|v| display.text(*v)).collect::<Vec<_>>(),
                            (&text_font, &font),
                            ink,
                        );
                    }
                }
                // Show All Projects: the other open projects' assignments
                // in a row of their own.
                if let Some(((before, values, _), total)) = others.as_deref() {
                    let y = top + *metrics.offsets.last().unwrap_or(&0.);
                    let row = Rect::from_min_size(
                        Pos2::new(body.left(), y),
                        Vec2::new(body.width(), ROW_H),
                    );
                    if row.intersects(body) {
                        let style = self.grid.layout.group_sort.band_style(0);
                        let (fill, ink) = (
                            Color32::from_rgb(
                                style.background[0],
                                style.background[1],
                                style.background[2],
                            ),
                            Color32::from_rgb(style.text[0], style.text[1], style.text[2]),
                        );
                        p.rect_filled(row, 0., fill);
                        p.with_clip_rect(Rect::from_min_max(
                            row.min,
                            Pos2::new(split, row.bottom()),
                        ))
                        .text(
                            Pos2::new(row.left() + 4., row.center().y),
                            Align2::LEFT_CENTER,
                            "Other open projects",
                            chrome::bold_font(),
                            ink,
                        );
                        let shown = display.shown(*before, values);
                        // Its units are the remaining labor units.
                        self.paint_sheet_values(
                            &p,
                            row,
                            &axis,
                            &periods,
                            (value_left, cells_left),
                            (lines_per_row > 1).then_some("Remaining Labor"),
                            display.text(*total),
                            &shown.iter().map(|v| display.text(*v)).collect::<Vec<_>>(),
                            (&chrome::bold_font(), &font),
                            ink,
                        );
                    }
                }
            });
        if let Some(i) = right.filter(|i| !self.is_selected(*i)) {
            self.click_row(i, Default::default());
        }
        if let Some(i) = click {
            self.click_row(i, ui.input(|x| x.modifiers));
        }
    }

    /// The value column and the period cells of one spreadsheet row (one
    /// line of it with several time-phased fields: `label` names the field
    /// at the left of the value column).
    #[allow(clippy::too_many_arguments)]
    fn paint_sheet_values(
        &self,
        p: &egui::Painter,
        row: Rect,
        axis: &TimeScale,
        periods: &[(i64, i64)],
        (value_left, cells_left): (f32, f32),
        label: Option<&str>,
        total: String,
        values: &[String],
        (total_font, font): (&FontId, &FontId),
        ink: Color32,
    ) {
        let value = Rect::from_min_max(
            Pos2::new(value_left, row.top()),
            Pos2::new(cells_left, row.bottom()),
        );
        if let Some(label) = label {
            p.with_clip_rect(value).text(
                Pos2::new(value.left() + 3., value.center().y),
                Align2::LEFT_CENTER,
                label,
                font.clone(),
                ink,
            );
        }
        p.with_clip_rect(value).text(
            Pos2::new(value.right() - 4., value.center().y),
            Align2::RIGHT_CENTER,
            total,
            total_font.clone(),
            ink,
        );
        paint_period_cells(p, row, axis, periods, cells_left, values, font, ink);
        chrome::hline(p, value_left, row.right(), row.bottom() - 1., LINE);
    }

    /// The value column header of a spreadsheet opens the Display menu.
    fn usage_display_button(&mut self, ui: &mut egui::Ui, rect: Rect, name: &str) {
        let response = ui.interact(rect, ui.id().with(name), Sense::click());
        self.action(name, &response);
        egui::Popup::menu(&response)
            .style(chrome::popup_menu_style)
            .show(|ui| self.usage_display_menu(ui));
    }

    /// The activity profile's left controls: Show Usage for and Display
    /// Activities for selected Time Period.
    fn usage_scope_controls(&mut self, controls: &mut egui::Ui) {
        if self.panes.printing {
            return;
        }
        controls.horizontal(|ui| {
            let r = crate::dialog_kit::flow_radio(
                ui,
                &mut self.panes.usage.selected_only,
                false,
                "All activities",
                true,
            );
            self.action("usage_all_activities", &r);
            let r = crate::dialog_kit::flow_radio(
                ui,
                &mut self.panes.usage.selected_only,
                true,
                "Selected activities",
                true,
            );
            self.action("usage_selected_activities", &r);
        });
        controls.separator();
        controls
            .label(RichText::new("Display Activities for selected...").font(chrome::table_font()));
        let r = crate::dialog_kit::flow_checkbox(
            controls,
            &mut self.panes.usage.time_period,
            "Time Period",
            true,
        );
        self.action("usage_time_period", &r);
    }

    /// Activity Usage Profile: Show Usage for at the left, the labor
    /// histogram at the right.
    pub(crate) fn activity_usage_profile(&mut self, ui: &mut egui::Ui, rect: Rect, table_w: f32) {
        let table_w = self.usage_table_width(table_w, rect.width());
        let (rect, strip) = self.usage_strip(rect);
        self.panes.usage_strip = strip;
        let split = rect.left() + table_w;
        let left = Rect::from_min_max(rect.min, Pos2::new(split, rect.bottom()));
        let face = if self.panes.printing {
            Color32::WHITE
        } else {
            CHROME
        };
        ui.painter().rect_filled(left, 0., face);
        // Right-click on the left pane's empty parts opens the profile menu.
        let background = ui.interact(left, ui.id().with("usage_profile_left"), Sense::click());
        self.usage_menu(&background, UsageMenu::ActivityProfile);
        let groups = Rect::from_min_max(
            Pos2::new(left.left() + 6., left.bottom() - 112.),
            Pos2::new(left.right() - 6., left.bottom() - 6.),
        );
        let mut controls = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(groups)
                .layout(Layout::top_down(Align::LEFT)),
        );
        if !self.panes.printing {
            controls.label(RichText::new("Show Usage for").font(chrome::table_font()));
        }
        self.usage_scope_controls(&mut controls);
        let chart = Rect::from_min_max(Pos2::new(split + 6., rect.top()), rect.max);
        self.usage_splitter(
            ui,
            Rect::from_min_max(
                Pos2::new(split, rect.top()),
                Pos2::new(split + 6., rect.bottom()),
            ),
            rect.left(),
            "usage_profile_splitter",
            (usage::MIN_PANE, rect.width() - 150.),
            |u, w| u.table_width = Some(w),
        );
        let axis = self.usage_axis(chart.left() + AXIS_W);
        let (upper, lower) = self.usage_cells(&axis, chart.right());
        let periods = self.usage_periods(&axis, chart.right(), &lower);
        let serial = self.project_serial();
        let scope = self.usage_scope();
        let display = self.panes.usage.clone();
        let u = self.activity_profile_usage(&periods);
        let set = display.profile.activity;
        let baselines = self.profile_baselines();
        // A series counts when it is switched on and has data to show: the
        // baselines only when a stored baseline fills the slot.
        let usable = |s: Series| match s {
            Series::ProjectBaseline => baselines[0],
            Series::PrimaryBaseline => baselines[1],
            _ => true,
        };
        let by_date = |s: Series| set[s.index()].by_date && usable(s);
        let cumulative = |s: Series| set[s.index()].cumulative && usable(s);
        let colors = |s: Series| popt::colors(set[s.index()].color);
        // Each series over the periods, and what it holds before them.
        let project_bl = baselines[0].then(|| self.activity_baseline_usage(0, &periods));
        let primary_bl = baselines[1].then(|| self.activity_baseline_usage(1, &periods));
        let empty = vec![0.; periods.len() + 2];
        // In `Series::ALL` order.
        let data = [
            unedge(&u.budget),
            unedge(&u.actual),
            unedge(&u.remaining),
            unedge(&u.remaining_late),
            unedge(project_bl.as_deref().unwrap_or(&empty)),
            unedge(primary_bl.as_deref().unwrap_or(&empty)),
        ];
        let values = |s: Series| &data[s.index()].1;
        // Each series switched to Cumulative adds its running total, from
        // what it holds before the first period.
        let curves: Vec<Curve> = Series::ALL
            .into_iter()
            .filter(|s| cumulative(*s))
            .map(|s| Curve {
                totals: running(data[s.index()].0, values(s)),
                color: colors(s).1,
                dashed: s.dashed(),
            })
            .collect();
        if display.stack_activity {
            let by = display.stack_by;
            let key = (
                periods.clone(),
                scope
                    .as_ref()
                    .map(|s| s.iter().copied().collect::<Vec<_>>()),
                display.costs,
                by.key(),
            );
            let series = self.memo(serial, "activity_stack", key, || {
                let by_task = self.labor_by_task(serial);
                let list: Vec<&Assignment> = by_task
                    .iter()
                    .filter(|(t, _)| scope.as_ref().is_none_or(|s| s.contains(t)))
                    .flat_map(|(_, l)| l.iter())
                    .collect();
                unedge_series(stacked::series(
                    &self.project,
                    &list,
                    &edged(&periods),
                    display.costs,
                    by,
                ))
            });
            let (series, before) = (&series.0, series.1);
            self.paint_stacked(
                ui,
                chart,
                (&axis, &upper, &lower, &periods),
                series,
                &curves_of(series, display.profile.any_cumulative(false), before),
                by,
                display.stacked_activity,
                None,
            );
            return;
        }
        let stacked = display.stacked_activity;
        let bars: Vec<Vec<Vec<Segment>>> = (0..periods.len())
            .map(|n| {
                let single = |s: Series| {
                    vec![Segment {
                        from: 0.,
                        to: values(s)[n],
                        colors: colors(s),
                    }]
                };
                let mut slots = vec![];
                if by_date(Series::Budgeted) {
                    slots.push(single(Series::Budgeted));
                }
                let (actual, early) = (by_date(Series::Actual), by_date(Series::RemainingEarly));
                if stacked && actual && early {
                    // Actual and remaining units stand on each other.
                    let a = values(Series::Actual)[n];
                    slots.push(vec![
                        Segment {
                            from: 0.,
                            to: a,
                            colors: colors(Series::Actual),
                        },
                        Segment {
                            from: a,
                            to: a + values(Series::RemainingEarly)[n],
                            colors: colors(Series::RemainingEarly),
                        },
                    ]);
                } else {
                    for (on, s) in [(actual, Series::Actual), (early, Series::RemainingEarly)] {
                        if on {
                            slots.push(single(s));
                        }
                    }
                }
                for s in [
                    Series::RemainingLate,
                    Series::ProjectBaseline,
                    Series::PrimaryBaseline,
                ] {
                    if by_date(s) {
                        slots.push(single(s));
                    }
                }
                slots
            })
            .collect();
        // The legend names the data: one resource type, or Total.
        let (kinds, expenses) = self.profile_kinds();
        let counted: Vec<&str> = popt::TYPES
            .iter()
            .zip(kinds.iter().chain([&expenses]))
            .filter(|(_, on)| **on)
            .map(|((_, caption), _)| *caption)
            .collect();
        let what = match counted.as_slice() {
            [one] => one.to_string(),
            _ => "Total".to_string(),
        };
        let labels: Vec<(String, (Color32, Color32))> = Series::ALL
            .into_iter()
            .filter(|s| by_date(*s) || cumulative(*s))
            .map(|s| {
                let name = match s {
                    Series::RemainingEarly => "Remaining",
                    other => other.caption(),
                };
                (format!("{name} {what}"), colors(s))
            })
            .collect();
        let legend: Vec<histogram::LegendEntry> = labels
            .iter()
            .map(|(label, colors)| (label.as_str(), *colors, false))
            .collect();
        self.paint_histogram(
            ui,
            chart,
            &axis,
            &upper,
            &lower,
            &periods,
            &bars,
            None,
            &curves,
            display.measure(),
            &legend,
            None,
        );
    }

    /// The Activity Usage Profile's labor per period over `edged(periods)`
    /// (split them with `unedge`): the activities of Show Usage for, plus
    /// the other open projects' labor with Show All Projects when every
    /// activity counts (a selection holds only this project's).
    pub(crate) fn activity_profile_usage(&self, periods: &[(i64, i64)]) -> Rc<ActivityUsage> {
        let serial = self.project_serial();
        let scope = self.usage_scope();
        let costs = self.panes.usage.costs;
        let all_projects = self.panes.usage.show_all_projects && scope.is_none();
        let (kinds, expenses) = self.profile_kinds();
        let key = (
            periods.to_vec(),
            scope
                .as_ref()
                .map(|s| s.iter().copied().collect::<Vec<_>>()),
            costs,
            all_projects,
            self.open_set.version,
            (kinds, expenses),
        );
        self.memo(serial, "activity_usage", key, || {
            let edges = edged(periods);
            let by_task = self.kind_by_task(serial, kinds);
            let mut u = activity_usage_of(&self.project, &by_task, &edges, scope.as_ref(), costs);
            if expenses {
                expenses_into(&self.project, &edges, scope.as_ref(), &mut u);
            }
            if all_projects {
                self.add_other_projects_usage(&mut u, &edges, costs, kinds, expenses);
            }
            u
        })
    }

    /// The resource kinds the activity profile counts, and whether it counts
    /// expenses (a cost: with Costs only).
    pub(crate) fn profile_kinds(&self) -> (Kinds, bool) {
        let t = self.panes.usage.profile.types;
        ([t[0], t[1], t[2]], t[3] && self.panes.usage.costs)
    }

    /// The active project's assignments of `kinds` by activity (the labor
    /// ones are `labor_by_task`).
    pub(crate) fn kind_by_task(
        &self,
        serial: Option<u64>,
        kinds: Kinds,
    ) -> Rc<BTreeMap<usize, Vec<Assignment>>> {
        if kinds == [true, false, false] {
            return self.labor_by_task(serial);
        }
        self.memo(serial, "kind_assignments", kinds, || {
            kind_assignments_in(&self.project, &self.assignment_list(serial), kinds)
        })
    }

    /// The budget of the baseline in `slot` (0: project, 1: primary) per
    /// period over `edged(periods)`, spread over the baseline's own planned
    /// dates by the shared spread, for the activities of Show Usage for
    /// matched by Activity ID, of the activity profile's kinds and expenses.
    /// Zero when no stored baseline is assigned to the slot.
    pub(crate) fn activity_baseline_usage(
        &self,
        slot: usize,
        periods: &[(i64, i64)],
    ) -> Rc<Vec<f64>> {
        use crate::baselines::{data::Source, Slot};
        let compare = self.baseline_compare();
        let slot = [Slot::Project, Slot::Primary][slot.min(1)];
        let edges = edged(periods);
        let Source::Baseline(loaded) = compare.source(slot) else {
            return Rc::new(vec![0.; edges.len()]);
        };
        let Some(bp) = loaded.project.clone() else {
            return Rc::new(vec![0.; edges.len()]);
        };
        let serial = self.project_serial();
        let scope = self.usage_scope();
        let costs = self.panes.usage.costs;
        let (kinds, expenses) = self.profile_kinds();
        let key = (
            slot.index(),
            periods.to_vec(),
            scope
                .as_ref()
                .map(|s| s.iter().copied().collect::<Vec<_>>()),
            costs,
            (kinds, expenses),
            std::sync::Arc::as_ptr(loaded) as usize,
        );
        self.memo(serial, "activity_baseline_usage", key, || {
            let shift = (self.project.model.origin_serial - bp.model.origin_serial) * DAY;
            let moved: Vec<(i64, i64)> =
                edges.iter().map(|(a, b)| (a + shift, b + shift)).collect();
            // The activities in scope, as the baseline knows them.
            let only: Option<BTreeSet<usize>> = scope.as_ref().map(|s| {
                let codes: BTreeSet<&str> = s
                    .iter()
                    .filter_map(|&i| self.project.model.activities.get(i))
                    .map(|a| a.task_code.as_str())
                    .collect();
                bp.model
                    .activities
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| codes.contains(a.task_code.as_str()))
                    .map(|(i, _)| i)
                    .collect()
            });
            let by_task = kind_assignments_in(&bp, &asg::assignments(&bp), kinds);
            let mut u = activity_usage_of(&bp, &by_task, &moved, only.as_ref(), costs);
            if expenses {
                expenses_into(&bp, &moved, only.as_ref(), &mut u);
            }
            u.budget
        })
    }

    /// Resource `rsrc`'s units (costs) and limit per period over
    /// `edged(periods)` (split them with `unedge`; the edges have no
    /// limit), with the other open projects' units of the resource of the
    /// same ID for Show All Projects.
    pub(crate) fn resource_profile_usage(
        &self,
        rsrc: i64,
        periods: &[(i64, i64)],
    ) -> Rc<rs::Usage> {
        let serial = self.project_serial();
        let costs = self.panes.usage.costs;
        let all_projects = self.panes.usage.show_all_projects;
        let key = (
            rsrc,
            periods.to_vec(),
            costs,
            all_projects,
            self.open_set.version,
        );
        self.memo(serial, "resource_usage", key, || {
            let edges = edged(periods);
            let mut u = rs::usage_in(
                &self.project,
                rsrc,
                self.resource_assignments(serial, rsrc).iter(),
                &edges,
                costs,
            );
            if all_projects {
                let code = rs::resources(&self.project)
                    .into_iter()
                    .find(|r| r.id == rsrc)
                    .map(|r| r.code)
                    .unwrap_or_default();
                let (actual, remaining) = self.other_resource_usage(&code, &edges, costs);
                for k in 0..edges.len() {
                    u.actual[k] += actual[k];
                    u.remaining[k] += remaining[k];
                }
            }
            u
        })
    }

    /// Resource `rsrc`'s budgeted units (costs) over the planned dates and
    /// remaining units over the late dates per period over
    /// `edged(periods)`, with the other open projects' of the resource of
    /// the same ID for Show All Projects.
    pub(crate) fn resource_plan_usage(
        &self,
        rsrc: i64,
        periods: &[(i64, i64)],
    ) -> Rc<(Vec<f64>, Vec<f64>)> {
        let serial = self.project_serial();
        let costs = self.panes.usage.costs;
        let all_projects = self.panes.usage.show_all_projects;
        let key = (
            rsrc,
            periods.to_vec(),
            costs,
            all_projects,
            self.open_set.version,
        );
        self.memo(serial, "resource_plan_usage", key, || {
            let edges = edged(periods);
            let mut budget = vec![0.; edges.len()];
            let mut late = vec![0.; edges.len()];
            let mut add = |p: &ProjectData, list: Vec<&Assignment>, at: &[(i64, i64)]| {
                for a in list {
                    let costed;
                    let a = if costs {
                        costed = a.as_costs();
                        &costed
                    } else {
                        a
                    };
                    asg::budget_into(p, a, at, &mut budget);
                    asg::remaining_late_into(p, a, at, &mut late);
                }
            };
            let mine = self.resource_assignments(serial, rsrc);
            add(&self.project, mine.iter().collect(), &edges);
            if all_projects {
                let code = rs::resources(&self.project)
                    .into_iter()
                    .find(|r| r.id == rsrc)
                    .map(|r| r.code)
                    .unwrap_or_default();
                let origin = self.project.model.origin_serial;
                for (k, other) in self.open_set.parked.iter().map(|p| &p.project).enumerate() {
                    let parked = self.parked_assignments(k);
                    let Some(&id) = parked.by_code.get(&code) else {
                        continue;
                    };
                    let shift = (origin - other.model.origin_serial) * DAY;
                    let moved: Vec<(i64, i64)> =
                        edges.iter().map(|(a, b)| (a + shift, b + shift)).collect();
                    add(other, parked.of_resource(id).collect(), &moved);
                }
            }
            (budget, late)
        })
    }

    /// Adds the other open projects' usage to `u` (Show All Projects): their
    /// own assignments of `kinds` (and expenses) over the same dates.
    fn add_other_projects_usage(
        &self,
        u: &mut ActivityUsage,
        periods: &[(i64, i64)],
        costs: bool,
        kinds: Kinds,
        expenses: bool,
    ) {
        let origin = self.project.model.origin_serial;
        for (k, other) in self.open_set.parked.iter().map(|p| &p.project).enumerate() {
            let shift = (origin - other.model.origin_serial) * DAY;
            let moved: Vec<(i64, i64)> = periods
                .iter()
                .map(|(a, b)| (a + shift, b + shift))
                .collect();
            let parked = self.parked_assignments(k);
            let typed;
            let by_task = if kinds == [true, false, false] {
                &parked.labor
            } else {
                typed = kind_assignments_in(other, &parked.list, kinds);
                &typed
            };
            let mut more = activity_usage_of(other, by_task, &moved, None, costs);
            if expenses {
                expenses_into(other, &moved, None, &mut more);
            }
            for (mine, theirs) in [
                (&mut u.budget, &more.budget),
                (&mut u.actual, &more.actual),
                (&mut u.remaining, &more.remaining),
                (&mut u.remaining_late, &more.remaining_late),
            ] {
                for (a, b) in mine.iter_mut().zip(theirs) {
                    *a += b;
                }
            }
        }
    }

    /// Resource Usage Profile: the resource list at the left, the selected
    /// resource's histogram (actual, remaining early, overallocation, limit).
    pub(crate) fn resource_usage_profile(&mut self, ui: &mut egui::Ui, rect: Rect, table_w: f32) {
        let table_w = self.usage_table_width(table_w, rect.width());
        let (rect, strip) = self.usage_strip(rect);
        self.panes.usage_strip = strip;
        let split = rect.left() + table_w;
        self.usage_resource_list(
            ui,
            Rect::from_min_max(rect.min, Pos2::new(split, rect.bottom())),
            true,
        );
        self.usage_splitter(
            ui,
            Rect::from_min_max(
                Pos2::new(split, rect.top()),
                Pos2::new(split + 6., rect.bottom()),
            ),
            rect.left(),
            "usage_profile_splitter",
            (usage::MIN_PANE, rect.width() - 150.),
            |u, w| u.table_width = Some(w),
        );
        let chart = Rect::from_min_max(Pos2::new(split + 6., rect.top()), rect.max);
        let axis = self.usage_axis(chart.left() + AXIS_W);
        let (upper, lower) = self.usage_cells(&axis, chart.right());
        let periods = self.usage_periods(&axis, chart.right(), &lower);
        let (members, roles) = self.usage_members(true);
        let display = self.panes.usage.clone();
        let mixed = !display.costs
            && roles.is_empty()
            && usage_list::mixed_units(&self.usage_resources(), &members);
        // Materials count in their unit of measure, not in hours.
        let unit = usage_list::material_unit(&self.usage_resources(), &members);
        if (members.is_empty() && roles.is_empty()) || mixed {
            self.paint_histogram(
                ui,
                chart,
                &axis,
                &upper,
                &lower,
                &periods,
                &[],
                None,
                &[],
                "",
                &[],
                None,
            );
            if mixed {
                let g = ui.painter().layout(
                    usage_list::MIXED_NOTE.to_string(),
                    chrome::table_font(),
                    INK,
                    (chart.width() - 80.).max(120.),
                );
                ui.painter().galley(
                    Pos2::new(chart.left() + AXIS_W + 12., chart.top() + DISPLAY_H + 30.),
                    g,
                    INK,
                );
            }
            return;
        }
        if !roles.is_empty() {
            self.role_demand_profile(ui, chart, (&axis, &upper, &lower, &periods), &roles);
            return;
        }
        let rsrc = members.first().copied().unwrap_or_default();
        let options = display.profile.clone();
        let serial = self.project_serial();
        if display.stack_resource {
            let by = display.stack_by;
            let key = (rsrc, periods.clone(), display.costs, by.key());
            let series = self.memo(serial, "resource_stack", key, || {
                let list = stacked::resource_profile_assignments(&self.project, Some(rsrc));
                let refs: Vec<&Assignment> = list.iter().collect();
                unedge_series(stacked::series(
                    &self.project,
                    &refs,
                    &edged(&periods),
                    display.costs,
                    by,
                ))
            });
            let (series, before) = (&series.0, series.1);
            self.paint_stacked(
                ui,
                chart,
                (&axis, &upper, &lower, &periods),
                series,
                &curves_of(series, display.profile.any_cumulative(true), before),
                by,
                display.stacked_resource,
                unit.as_deref(),
            );
            return;
        }
        // The selected resources' units add up; so do their limits. Each
        // resource counts its own assignments only (no roll-up of children).
        let n = periods.len();
        let mut u = rs::Usage {
            actual: vec![0.; n],
            remaining: vec![0.; n],
            limit: vec![0.; n],
        };
        let mut budget = vec![0.; n];
        let mut late = vec![0.; n];
        // What the selection holds before the first period, per series,
        // where each running total starts.
        let mut before = [0.; 4];
        for &rsrc in &members {
            let one = self.resource_profile_usage(rsrc, &periods);
            let plan = self.resource_plan_usage(rsrc, &periods);
            let (a0, actual, _) = unedge(&one.actual);
            let (r0, remaining, _) = unedge(&one.remaining);
            let (_, limit, _) = unedge(&one.limit);
            let (b0, planned, _) = unedge(&plan.0);
            let (l0, lates, _) = unedge(&plan.1);
            for (k, v) in [b0, a0, r0, l0].into_iter().enumerate() {
                before[k] += v;
            }
            for k in 0..n {
                u.actual[k] += actual[k];
                u.remaining[k] += remaining[k];
                u.limit[k] += limit[k];
                budget[k] += planned[k];
                late[k] += lates[k];
            }
        }
        if display.costs {
            // Costs have no limit: everything is within it.
            u.limit = vec![f64::INFINITY; periods.len()];
        }
        let (actual, remaining) = (u.actual.clone(), u.remaining.clone());
        let limit = u.limit.clone();
        let mut overtime = vec![0.; periods.len()];
        if options.overtime {
            for &rsrc in &members {
                let one = self.memo(
                    serial,
                    "resource_overtime",
                    (rsrc, periods.clone(), display.costs),
                    || {
                        resource_overtime(
                            &self.project,
                            self.resource_assignments(serial, rsrc).iter(),
                            &periods,
                            display.costs,
                        )
                    },
                );
                for (o, v) in overtime.iter_mut().zip(one.iter()) {
                    *o += v;
                }
            }
        }
        let set = options.resource;
        let on = |s: Series| set[s.index()].by_date;
        let colors = |s: Series| popt::colors(set[s.index()].color);
        let series_values = |s: Series| match s {
            Series::Budgeted => &budget,
            Series::Actual => &actual,
            Series::RemainingEarly => &remaining,
            _ => &late,
        };
        // Each series switched to Cumulative adds its running total.
        let curves: Vec<Curve> = Series::RESOURCE
            .into_iter()
            .filter(|s| set[s.index()].cumulative)
            .map(|s| Curve {
                totals: running(before[s.index()], series_values(s)),
                color: colors(s).1,
                dashed: s.dashed(),
            })
            .collect();
        let stacked = display.stacked_resource;
        let bars: Vec<Vec<Vec<Segment>>> = (0..periods.len())
            .map(|n| {
                let mut slots = vec![];
                if on(Series::Budgeted) {
                    slots.push(vec![Segment {
                        from: 0.,
                        to: budget[n],
                        colors: colors(Series::Budgeted),
                    }]);
                }
                let regular = (actual[n] - overtime[n]).max(0.);
                let mut actual_part = vec![
                    Segment {
                        from: 0.,
                        to: regular,
                        colors: colors(Series::Actual),
                    },
                    Segment {
                        from: regular,
                        to: actual[n],
                        colors: OVERTIME,
                    },
                ];
                // Stacked: the remaining units stand on the actual ones in
                // the same bar; otherwise they have a bar of their own.
                let together = stacked && on(Series::Actual);
                let (base, top) = if together {
                    (actual[n], actual[n] + remaining[n])
                } else {
                    (0., remaining[n])
                };
                // Units above the limit take the overallocation color unless
                // that is switched off.
                let within = if options.overallocation {
                    top.min(limit[n].max(base))
                } else {
                    top
                };
                let mut remaining_part = vec![Segment {
                    from: base,
                    to: within,
                    colors: colors(Series::RemainingEarly),
                }];
                if top > within {
                    remaining_part.push(Segment {
                        from: within,
                        to: top,
                        colors: OVER,
                    });
                }
                if options.availability && limit[n].is_finite() && limit[n] > top {
                    remaining_part.push(Segment {
                        from: top,
                        to: limit[n],
                        colors: AVAILABLE,
                    });
                }
                match (on(Series::Actual), on(Series::RemainingEarly)) {
                    (true, true) if stacked => {
                        actual_part.extend(remaining_part);
                        slots.push(actual_part);
                    }
                    (a, r) => {
                        if a {
                            slots.push(actual_part);
                        }
                        if r {
                            slots.push(remaining_part);
                        }
                    }
                }
                if on(Series::RemainingLate) {
                    slots.push(vec![Segment {
                        from: 0.,
                        to: late[n],
                        colors: colors(Series::RemainingLate),
                    }]);
                }
                slots
            })
            .collect();
        let measure = if display.costs { "Cost" } else { "Units" };
        let named = |s: Series| set[s.index()].by_date || set[s.index()].cumulative;
        let labels: Vec<(String, (Color32, Color32))> = Series::RESOURCE
            .into_iter()
            .filter(|s| named(*s))
            .map(|s| (format!("{} {measure}", s.caption()), colors(s)))
            .collect();
        let mut legend: Vec<histogram::LegendEntry> = labels
            .iter()
            .map(|(label, colors)| (label.as_str(), *colors, false))
            .collect();
        if options.overtime {
            let at = legend
                .iter()
                .position(|e| e.0.starts_with("Actual"))
                .map_or(0, |k| k + 1);
            legend.insert(at, ("Overtime", OVERTIME, false));
        }
        if !display.costs {
            if options.overallocation {
                let at = legend
                    .iter()
                    .position(|e| e.0.starts_with("Remaining Early"))
                    .map_or(legend.len(), |k| k + 1);
                legend.insert(at, ("Overallocated Early Units", OVER, false));
            }
            if options.limits {
                legend.push(("Limit", (INK, INK), true));
            }
            if options.availability {
                legend.push(("Availability", AVAILABLE, false));
            }
        }
        self.paint_histogram(
            ui,
            chart,
            &axis,
            &upper,
            &lower,
            &periods,
            &bars,
            (!display.costs && options.limits).then_some(limit.as_slice()),
            &curves,
            "",
            &legend,
            unit.as_deref(),
        );
    }

    /// Select View > By Role: the demand of the selected roles, actual
    /// units, then the remaining units of assignments that name a resource
    /// (staffed) and of those that name only the role (unstaffed). Roles
    /// have no limit line; Show All Projects does not add other projects'
    /// role demand.
    fn role_demand_profile(
        &mut self,
        ui: &mut egui::Ui,
        chart: Rect,
        (axis, upper, lower, periods): Frame,
        roles: &BTreeSet<i64>,
    ) {
        let display = self.panes.usage.clone();
        let serial = self.project_serial();
        let key = (
            roles.iter().copied().collect::<Vec<_>>(),
            periods.to_vec(),
            display.costs,
        );
        let edged_demand = self.memo(serial, "role_demand", key, || {
            usage_list::role_demand(&self.project, roles, &edged(periods), display.costs)
        });
        // The periods on show, and what the roles ask for before them.
        let (a0, actual, _) = unedge(&edged_demand.actual);
        let (s0, staffed, _) = unedge(&edged_demand.staffed);
        let (u0, unstaffed, _) = unedge(&edged_demand.unstaffed);
        let d = usage_list::RoleDemand {
            actual,
            staffed,
            unstaffed,
        };
        let curves: Vec<Curve> = if display.profile.any_cumulative(true) {
            let total: Vec<f64> = (0..periods.len())
                .map(|k| d.actual[k] + d.staffed[k] + d.unstaffed[k])
                .collect();
            vec![Curve {
                totals: running(a0 + s0 + u0, &total),
                color: INK,
                dashed: false,
            }]
        } else {
            vec![]
        };
        let stacked = display.stacked_resource;
        let bars: Vec<Vec<Vec<Segment>>> = (0..periods.len())
            .map(|n| {
                let seg = |from: f64, to: f64, colors| Segment { from, to, colors };
                let (a, s, u) = (d.actual[n], d.staffed[n], d.unstaffed[n]);
                if stacked {
                    vec![vec![
                        seg(0., a, ACTUAL),
                        seg(a, a + s, REMAINING),
                        seg(a + s, a + s + u, UNSTAFFED),
                    ]]
                } else {
                    vec![
                        vec![seg(0., a, ACTUAL)],
                        vec![seg(0., s, REMAINING), seg(s, s + u, UNSTAFFED)],
                    ]
                }
            })
            .collect();
        let legend: Vec<(&str, (Color32, Color32), bool)> = if display.costs {
            vec![
                ("Actual Cost", ACTUAL, false),
                ("Staffed Remaining Cost", REMAINING, false),
                ("Unstaffed Remaining Cost", UNSTAFFED, false),
            ]
        } else {
            vec![
                ("Actual Units", ACTUAL, false),
                ("Staffed Remaining Units", REMAINING, false),
                ("Unstaffed Remaining Units", UNSTAFFED, false),
            ]
        };
        self.paint_histogram(
            ui, chart, axis, upper, lower, periods, &bars, None, &curves, "", &legend, None,
        );
    }

    /// A Stacked Histogram: one bar per period, the series stacked from the
    /// largest at the bottom, with the series legend. With `split` (Show
    /// Actual and Remaining) each series' segment shows its actual units at
    /// the bottom in a darker shade.
    #[allow(clippy::too_many_arguments)]
    fn paint_stacked(
        &mut self,
        ui: &mut egui::Ui,
        chart: Rect,
        (axis, upper, lower, periods): Frame,
        series: &[stacked::Series],
        curves: &[Curve],
        by: StackBy,
        split: bool,
        unit: Option<&str>,
    ) {
        let bars: Vec<Vec<Vec<Segment>>> = (0..periods.len())
            .map(|n| {
                let stack = stacked::segments(series, n, split)
                    .into_iter()
                    .map(|(from, to, colors)| Segment { from, to, colors })
                    .collect();
                vec![stack]
            })
            .collect();
        let legend: Vec<(&str, (Color32, Color32), bool)> = if series.is_empty() {
            vec![(
                "No assignments in these periods",
                (Color32::WHITE, LINE),
                false,
            )]
        } else {
            series
                .iter()
                .enumerate()
                .map(|(k, s)| (s.label.as_str(), stacked::colors(k, s.other), false))
                .chain(split.then(|| ("Darker: actual", stacked::actual_colors(0, true), false)))
                .collect()
        };
        self.paint_histogram(
            ui,
            chart,
            axis,
            upper,
            lower,
            periods,
            &bars,
            None,
            curves,
            by.title(),
            &legend,
            unit,
        );
    }

    /// The histogram frame shared by both profiles: a `Display:` header
    /// button, Y axis labels with dashed gridlines, nonwork shading, the
    /// bars (one slot per series in each period, each slot a stack), the
    /// limit as a stepped line, the data date, the two-tier timescale under
    /// the chart and the legend box at the top left.
    #[allow(clippy::too_many_arguments)]
    fn paint_histogram(
        &mut self,
        ui: &mut egui::Ui,
        chart: Rect,
        axis: &TimeScale,
        upper: &[Cell],
        lower: &[Cell],
        periods: &[(i64, i64)],
        bars: &[Vec<Vec<Segment>>],
        limit: Option<&[f64]>,
        curves: &[Curve],
        legend_title: &str,
        legend: &[(&str, (Color32, Color32), bool)],
        unit: Option<&str>,
    ) {
        let display = Rect::from_min_size(chart.min, Vec2::new(chart.width(), DISPLAY_H));
        let button = crate::views::sheet::header_button(
            ui,
            "usage_profile_display",
            display,
            "Display: Open Projects Only",
        );
        self.action("usage_profile_display", &button);
        egui::Popup::menu(&button)
            .style(chrome::popup_menu_style)
            .show(|ui| self.usage_display_menu(ui));
        let p = ui.painter().with_clip_rect(chart.intersect(ui.clip_rect()));
        let tiers = Rect::from_min_max(
            Pos2::new(chart.left(), chart.bottom() - 2. * TIER_H),
            chart.max,
        );
        let body = Rect::from_min_max(
            Pos2::new(chart.left() + AXIS_W, display.bottom()),
            Pos2::new(chart.right(), tiers.top()),
        );
        p.rect_filled(
            Rect::from_min_max(Pos2::new(chart.left(), display.bottom()), tiers.min),
            0.,
            Color32::WHITE,
        );
        let graph = self.panes.usage.profile.graph.clone();
        p.rect_filled(body, 0., graph.background.color());
        let bp = p.with_clip_rect(body);
        let m = &self.project.model;
        let calendar = &m.calendars[m.project_calendar].cal;
        for c in lower {
            if calendar.duration(c.start_minute, c.end_minute) == 0 {
                let x0 = axis.x(c.start_minute as f64).round();
                let x1 = axis.x(c.end_minute as f64).round();
                bp.rect_filled(
                    Rect::from_min_max(Pos2::new(x0, body.top()), Pos2::new(x1, body.bottom())),
                    0.,
                    NONWORK,
                );
            }
        }
        // Vertical lines at the timescale's minor and major steps.
        for (on, cells, color) in [(graph.minor, lower, LINE), (graph.major, upper, SIGHT)] {
            if on {
                for c in cells {
                    let x = axis.x(c.start_minute as f64).round();
                    chrome::vline(&bp, x, body.top(), body.bottom(), color);
                }
            }
        }
        // Calculate Average: each period's total on the left scale (the bars
        // and the limit) divided; the running totals keep their sums.
        let divisors = graph.divisors(periods);
        let averaged = divisors
            .as_ref()
            .map(|d| histogram::divided(bars, limit, d));
        let (bars, limit) = match &averaged {
            Some((b, l)) => (b.as_slice(), l.as_deref()),
            None => (bars, limit),
        };
        let costs = self.panes.usage.costs;
        // The scale counts hours, a material's unit of measure, or money.
        let quantity = |v: f64| match unit {
            _ if costs => format::cost(v),
            Some(u) => usage_list::material_text(v, u),
            None => format::units(v),
        };
        // The picked period (Display Activities for selected Time Period),
        // as it was before this frame's click.
        let activity_profile = self.panes.bottom != BottomView::ResourceUsageProfile;
        let picked = self.panes.usage.period;
        let pick_response = ui.interact(body, ui.id().with("usage_profile_pick"), Sense::click());
        self.usage_menu(
            &pick_response,
            if activity_profile {
                UsageMenu::ActivityProfile
            } else {
                UsageMenu::ResourceProfile
            },
        );
        let pick = pick_response
            .interact_pointer_pos()
            .filter(|_| ui.input(|i| i.pointer.primary_released()));
        if let Some(pos) = pick {
            let at = axis.x_date(pos.x) as i64;
            self.panes.usage.period = periods.iter().copied().find(|(a, b)| at >= *a && at < *b);
        }
        histogram::paint_plot(
            &p,
            body,
            &histogram::Plot {
                axis,
                periods,
                bars,
                limit,
                curves,
                markers: &[],
                picked,
                data_date: self.project.model.project.data_date,
                quantity: &quantity,
                style: histogram::Style {
                    grid: match graph.hlines {
                        popt::HLines::None => None,
                        popt::HLines::Dotted => Some((graph.line_color.color(), true)),
                        popt::HLines::Solid => Some((graph.line_color.color(), false)),
                    },
                    bars_3d: graph.bars_3d,
                    values: graph.values,
                    unit: if divisors.is_some() {
                        graph.unit.trim().to_string()
                    } else {
                        String::new()
                    },
                },
            },
        );
        // Timescale under the chart: day letters above the week labels.
        let tp = p.with_clip_rect(Rect::from_min_max(
            Pos2::new(body.left(), tiers.top()),
            tiers.max,
        ));
        paint_tier(
            &tp,
            Rect::from_min_size(tiers.min, Vec2::new(tiers.width(), TIER_H)),
            axis,
            self.view.lower,
            lower,
        );
        paint_tier(
            &tp,
            Rect::from_min_max(Pos2::new(tiers.left(), tiers.top() + TIER_H), tiers.max),
            axis,
            self.view.upper,
            upper,
        );
        chrome::hline(&tp, tiers.left(), tiers.right(), tiers.top(), INK);
        // The upper tier names the cells (the lowest band, which zooms) and
        // the lower one their periods (which pans).
        let strip = self.panes.usage_strip.take();
        self.usage_timeline(
            ui,
            "usage_profile_timescale",
            if activity_profile {
                UsageMenu::ActivityProfile
            } else {
                UsageMenu::ResourceProfile
            },
            Rect::from_min_max(Pos2::new(body.left(), tiers.top() + TIER_H), tiers.max),
            Rect::from_min_max(
                Pos2::new(body.left(), tiers.top()),
                Pos2::new(tiers.right(), tiers.top() + TIER_H),
            ),
            body,
            strip,
            axis.left,
        );
        if legend.is_empty() || !self.panes.usage.profile.legend {
            return;
        }
        let mut legend = legend.to_vec();
        if !curves.is_empty() {
            legend.push(("Cumulative (right scale)", (INK, INK), true));
        }
        // Legend: white box with a black border at the chart's top left.
        histogram::paint_legend(
            &p,
            Pos2::new(chart.left() + 57., body.top() + 25.),
            legend_title,
            &legend,
        );
    }

    /// The other open projects' units for resource `code` per period:
    /// actual and remaining (Show All Projects).
    pub(crate) fn other_resource_usage(
        &self,
        code: &str,
        periods: &[(i64, i64)],
        costs: bool,
    ) -> (Vec<f64>, Vec<f64>) {
        let mut actual = vec![0.; periods.len()];
        let mut remaining = vec![0.; periods.len()];
        let origin = self.project.model.origin_serial;
        for (k, other) in self.open_set.parked.iter().map(|p| &p.project).enumerate() {
            let parked = self.parked_assignments(k);
            let Some(&id) = parked.by_code.get(code) else {
                continue;
            };
            let shift = (origin - other.model.origin_serial) * DAY;
            let moved: Vec<(i64, i64)> = periods
                .iter()
                .map(|(a, b)| (a + shift, b + shift))
                .collect();
            let u = rs::usage_in(other, id, parked.of_resource(id), &moved, costs);
            for k in 0..periods.len() {
                actual[k] += u.actual[k];
                remaining[k] += u.remaining[k];
            }
        }
        (actual, remaining)
    }

    /// The active project's assignments, built once per undo snapshot and
    /// shared by every usage view (their spread plans with them).
    pub(crate) fn assignment_list(&self, serial: Option<u64>) -> Rc<Vec<Assignment>> {
        self.memo(serial, "assignments", (), || {
            asg::assignments(&self.project)
        })
    }

    /// The active project's labor assignments by activity, cloned from
    /// `assignment_list` so they share its spread plans.
    pub(crate) fn labor_by_task(
        &self,
        serial: Option<u64>,
    ) -> Rc<BTreeMap<usize, Vec<Assignment>>> {
        self.memo(serial, "labor_assignments", (), || {
            labor_assignments_in(&self.project, &self.assignment_list(serial))
        })
    }

    /// Resource `rsrc`'s assignments in the active project, from the list
    /// indexed by resource once per snapshot.
    pub(crate) fn resource_assignments(&self, serial: Option<u64>, rsrc: i64) -> Vec<Assignment> {
        let list = self.assignment_list(serial);
        let index = self.memo(serial, "assignments_by_resource", (), || {
            asg::by_resource(&list)
        });
        index
            .get(&rsrc)
            .into_iter()
            .flatten()
            .map(|&k| list[k].clone())
            .collect()
    }

    /// Parked project `k`'s assignments and their indexes, kept while the
    /// open set is unchanged (a parked project changes only through it).
    pub(crate) fn parked_assignments(&self, k: usize) -> Rc<ParkedAssignments> {
        self.memo(
            self.project_serial(),
            "parked_assignments",
            (self.open_set.version, k),
            || ParkedAssignments::of(&self.open_set.parked[k].project),
        )
    }

    /// The Resource Usage Spreadsheet's rows for resource `rsrc`: the
    /// resource row and one row per assignment, with the field texts and
    /// the remaining units of each period.
    fn resource_sheet_rows(
        &self,
        rsrc: i64,
        afields: &[fields::Field],
        periods: &[(i64, i64)],
    ) -> Rc<Vec<ResRow>> {
        let display = &self.panes.usage;
        let serial = self.project_serial();
        let chosen = self.resource_time_fields();
        let key = (
            rsrc,
            periods.to_vec(),
            display.costs,
            afields.iter().map(|f| f.id.clone()).collect::<Vec<_>>(),
            display.assignment_sort.clone().field,
            display.assignment_sort.descending,
            display.show_all_projects,
            self.open_set.version,
            chosen.iter().map(|f| f.id()).collect::<Vec<_>>(),
        );
        let ix = self.shared_index();
        self.memo(serial, "resource_usage_sheet", key, || {
            let costs = display.costs;
            let list = rs::resources(&self.project);
            let code = list
                .iter()
                .find(|r| r.id == rsrc)
                .map_or(String::new(), |r| r.code.clone());
            let unit = usage_list::material_unit(&list, &BTreeSet::from([rsrc]));
            let unit = unit.as_deref();
            let mine = self.resource_assignments(serial, rsrc);
            // Spread over the periods and their edges, split apart below.
            let edges = edged(periods);
            let blank = vec![String::new(); afields.len()];
            // The resource row of each field: the remaining units as the
            // resource profile spreads them; the other fields add up the
            // assignment lines.
            let remaining = chosen
                .iter()
                .any(|f| f.series == time_fields::Series::Remaining)
                .then(|| rs::usage_in(&self.project, rsrc, mine.iter(), &edges, costs).remaining);
            let own = |f: &TimeField| f.series == time_fields::Series::Remaining;
            let mut totals: Vec<Vec<f64>> = chosen
                .iter()
                .map(|f| match (&remaining, own(f)) {
                    (Some(r), true) => r.clone(),
                    _ => vec![0.; edges.len()],
                })
                .collect();
            let mut sums = vec![0.; chosen.len()];
            let mut rows: Vec<ResRow> = vec![];
            let mut assignments = mine.iter().collect::<Vec<_>>();
            if let Some(field) = &display.assignment_sort.field {
                if let Some(column) = afields
                    .iter()
                    .find(|f| &f.id == field)
                    .and_then(|f| f.column.as_ref())
                {
                    let shown = self.shown();
                    assignments.sort_by(|a, b| {
                        cols::compare(
                            &cols::value(shown, &ix, column, a.task),
                            &cols::value(shown, &ix, column, b.task),
                        )
                    });
                    if display.assignment_sort.descending {
                        assignments.reverse();
                    }
                }
            }
            for a in assignments {
                let texts = self.activity_cells(afields, &ix, a.task);
                let a = if costs { a.as_costs() } else { a.clone() };
                for (k, f) in chosen.iter().enumerate() {
                    let mut values = vec![0.; edges.len()];
                    let total =
                        time_fields::assignment_into(&self.project, &a, *f, &edges, &mut values);
                    if !(remaining.is_some() && own(f)) {
                        for (t, v) in totals[k].iter_mut().zip(&values) {
                            *t += v;
                        }
                    }
                    sums[k] += total;
                    let texts = if k == 0 { texts.clone() } else { blank.clone() };
                    let mut line = ResRow::field_line(texts, (k, *f), &values, total, unit);
                    line.assignment = Some(a.row);
                    rows.push(line);
                }
            }
            if display.show_all_projects && !self.open_set.parked.is_empty() {
                let (_, other) = self.other_resource_usage(&code, &edges, costs);
                // The stored remaining units, like this project's rows: the
                // whole life of the assignments, not the periods on show.
                // Only the remaining fields count the other projects.
                let other_total = self.other_resource_total(&code, costs);
                if other_total.abs() > 1e-9 || other.iter().any(|v| v.abs() > 1e-9) {
                    for (k, f) in chosen.iter().enumerate() {
                        let (values, total) = if own(f) {
                            for (t, o) in totals[k].iter_mut().zip(&other) {
                                *t += o;
                            }
                            sums[k] += other_total;
                            (other.clone(), other_total)
                        } else {
                            (vec![0.; edges.len()], 0.)
                        };
                        let mut texts = blank.clone();
                        if k == 0 {
                            texts[0] = "Other open projects".into();
                        }
                        rows.push(ResRow::field_line(texts, (k, *f), &values, total, unit));
                    }
                }
            }
            let mut bands = vec![];
            for (k, f) in chosen.iter().enumerate() {
                let mut head = blank.clone();
                if k == 0 {
                    head[0] = code.clone();
                }
                let mut band = ResRow::field_line(head, (k, *f), &totals[k], sums[k], unit);
                band.band = true;
                bands.push(band);
            }
            bands.extend(rows);
            bands
        })
    }

    /// The value column's width on a usage spreadsheet (the activity one
    /// when `activity`): `VALUE_W`, or wide enough that the widest total
    /// shows whole in the band font. On the activity spreadsheet that is
    /// at most every labor assignment's remaining units together (with the
    /// other open projects' for Show All Projects); on the resource one the
    /// totals of the selection's rows.
    pub(crate) fn usage_value_width(&self, ctx: &egui::Context, activity: bool) -> f32 {
        let display = &self.panes.usage;
        let costs = display.costs;
        let font = chrome::bold_font();
        let measure =
            |t: String| ctx.fonts_mut(|f| f.layout_no_wrap(t, font.clone(), INK).size().x);
        let chosen = if activity {
            self.activity_time_fields()
        } else {
            self.resource_time_fields()
        };
        // Several fields: each line names its field at the left.
        let labels = if chosen.len() > 1 {
            chosen
                .iter()
                .map(|f| measure(f.label()) + 12.)
                .fold(0., f32::max)
        } else {
            0.
        };
        let texts: Vec<String> = if activity && chosen.len() > 1 {
            let serial = self.project_serial();
            let key = (costs, chosen.iter().map(|f| f.id()).collect::<Vec<_>>());
            let bounds = self.memo(serial, "activity_sheet_widest_fields", key, || {
                let by_task = self.all_by_task(serial);
                let kinds = time_fields::kinds(&self.project);
                let all: BTreeSet<usize> = by_task.keys().copied().collect();
                chosen
                    .iter()
                    .map(|f| {
                        time_fields::activity_into(
                            &self.project,
                            &by_task,
                            &kinds,
                            &all,
                            *f,
                            costs,
                            &[],
                        )
                        .1
                        .abs()
                    })
                    .collect::<Vec<f64>>()
            });
            bounds.iter().map(|b| display.text(*b)).collect()
        } else if activity {
            let serial = self.project_serial();
            let others = display.show_all_projects && !self.open_set.parked.is_empty();
            let key = (costs, others, self.open_set.version);
            let bound = self.memo(serial, "activity_sheet_widest", key, || {
                let own: f64 = self
                    .labor_by_task(serial)
                    .values()
                    .flatten()
                    .map(|a| if costs { a.remaining_cost } else { a.remaining }.abs())
                    .sum();
                let other = if others {
                    self.other_projects_usage(&[], costs).1.abs()
                } else {
                    0.
                };
                own + other
            });
            vec![display.text(*bound)]
        } else {
            let afields = fields::fields_of(
                &self.project,
                fields::Which::Assignment,
                &display.assignment_fields,
            );
            // The totals do not depend on the periods.
            self.selection_sheet_rows(&afields, &[])
                .map_or(vec![], |rows| {
                    rows.iter()
                        .map(|r| display.text_in(r.total, r.unit.as_deref()))
                        .collect()
                })
        };
        texts
            .into_iter()
            .map(|t| labels + measure(t) + 10.)
            .fold(VALUE_W, f32::max)
            .ceil()
    }

    /// The other open projects' stored remaining units (costs) of resource
    /// `code` (Show All Projects).
    pub(crate) fn other_resource_total(&self, code: &str, costs: bool) -> f64 {
        (0..self.open_set.parked.len())
            .map(|k| {
                let parked = self.parked_assignments(k);
                parked.by_code.get(code).map_or(0., |&id| {
                    parked
                        .of_resource(id)
                        .map(|a| if costs { a.remaining_cost } else { a.remaining })
                        .sum::<f64>()
                })
            })
            .sum()
    }

    /// The Resource Usage Spreadsheet's rows for the list's selection: one
    /// resource's rows; for several resources (or a group) a total row and
    /// each resource's rows under a heading of its own; for roles, the
    /// assignments that ask for each role.
    pub(crate) fn selection_sheet_rows(
        &self,
        afields: &[fields::Field],
        periods: &[(i64, i64)],
    ) -> Option<Rc<Vec<ResRow>>> {
        let (resources, roles) = self.usage_members(false);
        let n = periods.len();
        let parts: Vec<Rc<Vec<ResRow>>> = if roles.is_empty() {
            if resources.len() == 1 {
                return resources
                    .first()
                    .map(|r| self.resource_sheet_rows(*r, afields, periods));
            }
            resources
                .iter()
                .map(|r| self.resource_sheet_rows(*r, afields, periods))
                .collect()
        } else {
            let parts: Vec<Rc<Vec<ResRow>>> = roles
                .iter()
                .map(|r| self.role_sheet_rows(*r, afields, periods))
                .collect();
            if parts.len() == 1 {
                return parts.into_iter().next();
            }
            parts
        };
        if parts.is_empty() {
            return None;
        }
        let costs = self.panes.usage.costs;
        let mixed = roles.is_empty()
            && !costs
            && usage_list::mixed_units(&self.usage_resources(), &resources);
        let chosen = self.resource_time_fields();
        let mut totals = vec![vec![0.; n]; chosen.len()];
        let mut sums = vec![(0., 0., 0.); chosen.len()];
        let mut out: Vec<ResRow> = vec![];
        for part in &parts {
            for row in part.iter() {
                let mut row = row.clone();
                // Each part starts with its heading lines, one per field.
                if row.band {
                    if let (Some(total), Some(sum)) =
                        (totals.get_mut(row.field), sums.get_mut(row.field))
                    {
                        for (t, v) in total.iter_mut().zip(&row.values) {
                            *t += v;
                        }
                        sum.0 += row.total;
                        sum.1 += row.before;
                        sum.2 += row.after;
                    }
                    row.band = false;
                    row.sub = true;
                }
                out.push(row);
            }
        }
        let selection = self.panes.usage.selection();
        let lists = self.usage_list_rows(false);
        let mut label = match selection.as_slice() {
            [usage_list::ListKey::Group(_)] => lists
                .row(&selection[0])
                .map_or(String::new(), |r| r.label.clone()),
            _ if roles.is_empty() => format!("{} resources", resources.len()),
            _ => format!("{} roles", roles.len()),
        };
        if mixed {
            label.push_str(": units of different kinds are not added");
            totals = vec![vec![0.; n]; chosen.len()];
            sums = vec![(0., 0., 0.); chosen.len()];
        }
        let unit = usage_list::material_unit(&self.usage_resources(), &resources)
            .filter(|_| roles.is_empty());
        let mut heads = vec![];
        for (k, f) in chosen.iter().enumerate() {
            let mut head = vec![String::new(); afields.len().max(1)];
            if k == 0 {
                head[0] = label.clone();
            }
            heads.push(ResRow {
                texts: head,
                values: totals[k].clone(),
                before: sums[k].1,
                after: sums[k].2,
                total: sums[k].0,
                band: true,
                sub: false,
                unit: unit.clone(),
                field: k,
                label: f.label(),
                cumulative: f.cumulative,
                assignment: None,
            });
        }
        heads.extend(out);
        Some(Rc::new(heads))
    }

    /// The rows of role `role`: its heading with the remaining units of
    /// every assignment that asks for it, then those assignments.
    fn role_sheet_rows(
        &self,
        role: i64,
        afields: &[fields::Field],
        periods: &[(i64, i64)],
    ) -> Rc<Vec<ResRow>> {
        let display = &self.panes.usage;
        let serial = self.project_serial();
        let chosen = self.resource_time_fields();
        let key = (
            role,
            periods.to_vec(),
            display.costs,
            afields.iter().map(|f| f.id.clone()).collect::<Vec<_>>(),
            chosen.iter().map(|f| f.id()).collect::<Vec<_>>(),
        );
        let ix = self.shared_index();
        self.memo(serial, "role_usage_sheet", key, || {
            let costs = display.costs;
            let code = usage_list::roles(&self.project)
                .into_iter()
                .find(|r| r.id == role)
                .map_or(String::new(), |r| r.code);
            // Spread over the periods and their edges, split apart below.
            let edges = edged(periods);
            let blank = vec![String::new(); afields.len().max(1)];
            let mut totals = vec![vec![0.; edges.len()]; chosen.len()];
            let mut sums = vec![0.; chosen.len()];
            let mut rows: Vec<ResRow> = vec![];
            for a in self
                .assignment_list(serial)
                .iter()
                .filter(|a| crate::records::num(&a.role) == role)
            {
                let texts = self.activity_cells(afields, &ix, a.task);
                let a = if costs { a.as_costs() } else { a.clone() };
                for (k, f) in chosen.iter().enumerate() {
                    let mut values = vec![0.; edges.len()];
                    let total =
                        time_fields::assignment_into(&self.project, &a, *f, &edges, &mut values);
                    for (t, v) in totals[k].iter_mut().zip(&values) {
                        *t += v;
                    }
                    sums[k] += total;
                    let texts = if k == 0 { texts.clone() } else { blank.clone() };
                    let mut line = ResRow::field_line(texts, (k, *f), &values, total, None);
                    line.assignment = Some(a.row);
                    rows.push(line);
                }
            }
            let mut bands = vec![];
            for (k, f) in chosen.iter().enumerate() {
                let mut head = blank.clone();
                if k == 0 {
                    head[0] = code.clone();
                }
                let mut band = ResRow::field_line(head, (k, *f), &totals[k], sums[k], None);
                band.band = true;
                bands.push(band);
            }
            bands.extend(rows);
            bands
        })
    }

    /// Rows the Resource Usage Spreadsheet has for the selection, for the
    /// printout's page breaks.
    pub(crate) fn resource_sheet_line_count(&self) -> usize {
        let afields = fields::fields_of(
            &self.project,
            fields::Which::Assignment,
            &self.panes.usage.assignment_fields,
        );
        let periods = self.panes.sheet_periods.clone();
        self.selection_sheet_rows(&afields, &periods)
            .map_or(0, |rows| rows.len())
    }

    /// Resource Usage Spreadsheet: resources, the selected resource's
    /// assignments, and remaining units per period for both.
    pub(crate) fn resource_usage_sheet(&mut self, ui: &mut egui::Ui, rect: Rect, table_w: f32) {
        self.claim_sheet(super::usage_cells::SheetHost::Usage);
        let table_w = self.usage_table_width(table_w, rect.width());
        let (rect, strip) = self.usage_strip(rect);
        let split = rect.left() + table_w;
        let list_w = self
            .panes
            .usage
            .list_width
            .map_or((table_w * 0.6).round(), |w| {
                w.clamp(
                    usage::MIN_PANE,
                    (table_w - usage::MIN_PANE).max(usage::MIN_PANE),
                )
            });
        self.usage_resource_list(
            ui,
            Rect::from_min_max(
                rect.min,
                Pos2::new(rect.left() + list_w - 3., rect.bottom()),
            ),
            false,
        );
        let p = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        self.usage_splitter(
            ui,
            Rect::from_min_max(
                Pos2::new(rect.left() + list_w - 3., rect.top()),
                Pos2::new(rect.left() + list_w + 3., rect.bottom()),
            ),
            rect.left(),
            "usage_list_splitter",
            (usage::MIN_PANE, table_w - usage::MIN_PANE),
            |u, w| u.list_width = Some(w + 3.),
        );
        let middle = Rect::from_min_max(
            Pos2::new(rect.left() + list_w + 3., rect.top()),
            Pos2::new(split, rect.bottom()),
        );
        p.rect_filled(middle, 0., Color32::WHITE);
        let value_left = split + 6.;
        let value_w = self.usage_value_width(ui.ctx(), false);
        let cells_left = value_left + value_w;
        self.usage_splitter(
            ui,
            Rect::from_min_max(
                Pos2::new(split, rect.top()),
                Pos2::new(value_left, rect.bottom()),
            ),
            rect.left(),
            "usage_sheet_splitter",
            (usage::MIN_PANE, rect.width() - 150.),
            |u, w| u.table_width = Some(w),
        );
        let axis = self.usage_axis(cells_left);
        let (upper, lower) = self.usage_cells(&axis, rect.right());
        let periods = self.usage_periods(&axis, rect.right(), &lower);
        self.panes.sheet_periods = periods.clone();
        let display = self.panes.usage.clone();
        let mw = middle.width();
        let afields = fields::fields_of(
            &self.project,
            fields::Which::Assignment,
            &display.assignment_fields,
        );
        let numbers = display.line_numbers;
        let widths = fields::weighted(&afields, mw - if numbers { NUMBER_W } else { 0. });
        Self::list_header(
            &p,
            Rect::from_min_size(middle.min, Vec2::new(mw, HEADER_H)),
            &Self::sheet_columns(&afields, &widths, numbers),
        );
        let value_head = Rect::from_min_size(
            Pos2::new(value_left, rect.top()),
            Vec2::new(value_w, HEADER_H),
        );
        let value_title = display.value_title(&self.resource_time_fields());
        Self::list_header(&p, value_head, &[(value_title.as_str(), value_w)]);
        self.usage_display_button(ui, value_head, "usage_sheet_display");
        let tiers = Rect::from_min_max(
            Pos2::new(cells_left, rect.top()),
            Pos2::new(rect.right(), rect.top() + HEADER_H),
        );
        let tp = p.with_clip_rect(tiers);
        paint_tier(
            &tp,
            Rect::from_min_size(tiers.min, Vec2::new(tiers.width(), TIER_H)),
            &axis,
            self.view.upper,
            &upper,
        );
        paint_tier(
            &tp,
            Rect::from_min_max(Pos2::new(tiers.left(), tiers.top() + TIER_H), tiers.max),
            &axis,
            self.view.lower,
            &lower,
        );
        chrome::hline(&p, tiers.left(), tiers.right(), tiers.bottom() - 1., INK);
        self.usage_timeline(
            ui,
            "usage_sheet_timescale",
            UsageMenu::ResourceSheet,
            Rect::from_min_size(tiers.min, Vec2::new(tiers.width(), TIER_H)),
            Rect::from_min_max(Pos2::new(tiers.left(), tiers.top() + TIER_H), tiers.max),
            Rect::from_min_max(Pos2::new(cells_left, tiers.bottom()), rect.max),
            strip,
            cells_left,
        );
        let body = Rect::from_min_max(Pos2::new(middle.left(), rect.top() + HEADER_H), rect.max);
        let background = ui.interact(body, ui.id().with("usage_sheet_body"), Sense::click());
        self.usage_menu(&background, UsageMenu::ResourceSheet);
        let mut clicked_at = background
            .interact_pointer_pos()
            .filter(|_| background.clicked());
        // The rows of the selection, as stored (the display options other
        // than costs apply as they are drawn).
        let Some(computed) = self.selection_sheet_rows(&afields, &periods) else {
            return;
        };
        let font = chrome::table_font();
        let mut y = rect.top() + HEADER_H;
        let picked = display.period;
        let several_fields = self.resource_time_fields().len() > 1;
        // The selected cells (`usage_cells`): a click outside the
        // spreadsheet takes the keyboard from them.
        let outside = ui.input(|i| {
            let away = i.pointer.interact_pos().is_some_and(|p| !rect.contains(p));
            i.pointer.primary_clicked() && away
        });
        if outside {
            self.panes.sheet_cells.focused = false;
        }
        let cell_range = self.panes.sheet_cells.range(&periods);
        let cursor = self.panes.sheet_cells.cursor;
        let mut cell_click: Option<(usize, (i64, i64), bool, bool)> = None;
        let mut editor_cell: Option<Rect> = None;
        let mut cell_rects: Vec<(String, [f32; 4])> = vec![];
        for (k, line) in computed.iter().enumerate() {
            if y > rect.bottom() {
                break;
            }
            let band = line.band || line.sub;
            let values = if line.cumulative {
                running(line.before, &line.values)
            } else {
                display.shown(line.before, &line.values)
            };
            let fill = if line.band {
                RESOURCE_BAND
            } else if line.sub {
                SUB_BAND
            } else {
                Color32::WHITE
            };
            let ink = if line.band { RESOURCE_TEXT } else { INK };
            let row = Rect::from_min_max(
                Pos2::new(middle.left(), y),
                Pos2::new(rect.right(), y + ROW_H),
            );
            let response = ui.interact(
                row,
                ui.id().with(("usage_assignment_row", k)),
                Sense::click(),
            );
            self.action(&format!("usage_assignment_row_{k}"), &response);
            if response.clicked() {
                clicked_at = response.interact_pointer_pos();
            }
            // A click on a period cell selects it (Shift: up to it); a
            // double-click opens its editor; a right-click outside the
            // selection selects the cell under it before the menu opens.
            let pressed = response.clicked() || response.secondary_clicked();
            if let Some(pos) = response
                .interact_pointer_pos()
                .filter(|p| pressed && p.x >= cells_left)
            {
                let at = axis.x_date(pos.x) as i64;
                if let Some(&period) = periods.iter().find(|(a, b)| at >= *a && at < *b) {
                    let n = periods.iter().position(|q| *q == period).unwrap_or(0);
                    let inside = cell_range
                        .as_ref()
                        .is_some_and(|(lines, ks)| lines.contains(&k) && ks.contains(&n));
                    if response.clicked() || !inside {
                        let shift = response.clicked() && ui.input(|i| i.modifiers.shift);
                        cell_click = Some((k, period, shift, response.double_clicked()));
                    }
                }
            }
            self.usage_menu(&response, UsageMenu::ResourceSheet);
            p.rect_filled(
                Rect::from_min_max(row.min, Pos2::new(split, row.bottom())),
                0.,
                fill,
            );
            p.rect_filled(
                Rect::from_min_max(Pos2::new(value_left, y), row.max),
                0.,
                fill,
            );
            // The picked period (Display Activities for selected Time
            // Period) shows as a light column.
            if let Some((a, b)) = picked.filter(|_| !line.band) {
                let x0 = axis.x(a as f64).round().max(cells_left);
                let x1 = axis.x(b as f64).round();
                if x1 > x0 {
                    p.rect_filled(
                        Rect::from_min_max(Pos2::new(x0, y), Pos2::new(x1, row.bottom())),
                        0.,
                        PICKED,
                    );
                }
            }
            // The selected cells, the selected cell framed; an assignment's
            // cells are named for the scenarios.
            let cell_of = |(a, b): (i64, i64)| {
                let x0 = axis.x(a as f64).round().max(cells_left);
                let x1 = axis.x(b as f64).round();
                let r = Rect::from_min_max(Pos2::new(x0, y), Pos2::new(x1, row.bottom()));
                (x1 > x0).then_some(r)
            };
            if let Some((_, ks)) = cell_range.as_ref().filter(|(lines, _)| lines.contains(&k)) {
                for n in ks.clone() {
                    if let Some(cell) = periods.get(n).and_then(|q| cell_of(*q)) {
                        p.rect_filled(cell, 0., SELECTED_CELLS);
                    }
                }
            }
            if line.assignment.is_some() {
                for (n, q) in periods.iter().enumerate() {
                    if let Some(c) = cell_of(*q) {
                        let key = format!("usage_sheet_cell_{k}_{n}");
                        cell_rects.push((key, [c.left(), c.top(), c.right(), c.bottom()]));
                    }
                }
            }
            if let Some((_, start)) = cursor.filter(|(l, _)| *l == k) {
                let at = periods.iter().find(|q| q.0 == start);
                editor_cell = at.and_then(|q| cell_of(*q)).or(editor_cell);
            }
            let text_font = if band {
                chrome::bold_font()
            } else {
                font.clone()
            };
            let table = Rect::from_min_max(row.min, Pos2::new(split, row.bottom()));
            if band {
                // The resource row carries its ID alone, with no cell lines.
                let mut x = table.left() + 3.;
                if numbers {
                    x += NUMBER_W;
                }
                p.with_clip_rect(table).text(
                    Pos2::new(x, table.center().y),
                    Align2::LEFT_CENTER,
                    line.texts.first().map_or("", String::as_str),
                    text_font.clone(),
                    ink,
                );
            } else {
                Self::paint_sheet_cells(
                    &p,
                    table,
                    table.left(),
                    numbers.then_some(k + 1),
                    &afields,
                    &widths,
                    &line.texts,
                    &text_font,
                    ink,
                );
            }
            let unit = line.unit.as_deref();
            let texts: Vec<String> = values.iter().map(|v| display.text_in(*v, unit)).collect();
            paint_values(
                &p,
                row,
                &axis,
                &periods,
                (value_left, cells_left),
                (several_fields).then_some(line.label.as_str()),
                display.text_in(line.total, unit),
                &texts,
                &font,
                ink,
            );
            chrome::hline(&p, middle.left(), rect.right(), y + ROW_H - 1., LINE);
            y += ROW_H;
        }
        // A click on the period cells picks that period.
        if let Some(pos) = clicked_at.filter(|pos| pos.x >= cells_left) {
            let at = axis.x_date(pos.x) as i64;
            self.panes.usage.period = periods.iter().copied().find(|(a, b)| at >= *a && at < *b);
        }
        self.action_rects.extend(cell_rects);
        let time = ui.input(|i| i.time);
        if let Some((k, period, shift, double)) = cell_click {
            self.sheet_cell_click(k, period, shift, double, time);
        }
        self.sheet_cell_editor(ui, editor_cell, &font, time);
    }
}

impl App {
    /// The spreadsheet on show: the Activity Usage Spreadsheet (on top or
    /// at the bottom) or the Resource Usage Spreadsheet.
    fn usage_sheet_on_show(&self) -> Option<UsageMenu> {
        match self.panes.top {
            TopView::ActivityUsageSheet => return Some(UsageMenu::ActivitySheet),
            TopView::ResourceUsageSheet => return Some(UsageMenu::ResourceSheet),
            _ => {}
        }
        match self.panes.bottom {
            BottomView::ActivityUsageSheet => Some(UsageMenu::ActivitySheet),
            BottomView::ResourceUsageSheet => Some(UsageMenu::ResourceSheet),
            _ => None,
        }
    }

    /// Copy to Excel (spreadsheet menu): asks where to save, then writes.
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn copy_usage_to_excel(&mut self) {
        let name = format!(
            "{} usage.xlsx",
            crate::interchange::safe_file_name(&self.project.model.project.name)
        );
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Excel workbook", &["xlsx"])
            .set_file_name(name)
            .save_file()
        {
            self.copy_usage_to_excel_to(&path);
        }
    }

    /// Writes the spreadsheet on show to `path`: the line numbers when on,
    /// the fields, the value column and the value of each period, for every
    /// row. The periods run over the whole data, not only those on show, so
    /// they add up to the value column; anything still outside them (data
    /// past the schedule's dates) gets a Before or After column.
    pub(crate) fn copy_usage_to_excel_to(&mut self, path: &Path) {
        match self.usage_sheet_workbook() {
            Ok((bytes, rows)) => match openxer_xer::write_file_atomic(path, &bytes) {
                Ok(()) => self.status = format!("Copied {rows} rows to {}", path.display()),
                Err(e) => self.error(format!("{} could not be saved: {e}", path.display())),
            },
            Err(e) => self.error(e),
        }
    }

    /// The periods Copy to Excel writes: the usage periods over the data's
    /// dates, from the earliest to a day past the latest.
    pub(crate) fn usage_export_periods(&self) -> Vec<(i64, i64)> {
        let (start, finish) = self.gantt_data_extent();
        self.usage_periods_over(start as f64, (finish + DAY) as f64)
    }

    /// The workbook bytes and the number of rows written.
    fn usage_sheet_workbook(&mut self) -> Result<(Vec<u8>, usize), String> {
        use openxer_interchange::report_xlsx::{report_bytes, ReportCell, ReportRow, ReportSheet};
        /// One row before it is written.
        struct Line {
            texts: Vec<String>,
            /// The time-phased field the line shows, and whether it runs
            /// cumulative.
            label: String,
            cumulative: bool,
            total: f64,
            /// Before, the periods' values and after.
            values: Vec<f64>,
            fill: Option<u32>,
            ink: Option<u32>,
            bold: bool,
        }
        let kind = self
            .usage_sheet_on_show()
            .ok_or("Show a usage spreadsheet first (View > Show on Bottom).")?;
        let periods = self.usage_export_periods();
        if periods.is_empty() {
            return Err("The project has no dates to spread its units over.".into());
        }
        let display = self.panes.usage.clone();
        let origin = self.project.model.origin_serial;
        let which = match kind {
            UsageMenu::ActivitySheet => fields::Which::Activity,
            _ => fields::Which::Assignment,
        };
        let ids = match kind {
            UsageMenu::ActivitySheet => &display.activity_fields,
            _ => &display.assignment_fields,
        };
        let sheet_fields = fields::fields_of(&self.project, which, ids);
        let rgb = |c: Color32| u32::from(c.r()) << 16 | u32::from(c.g()) << 8 | u32::from(c.b());
        let edges = |before: f64, values: &[f64], after: f64| {
            let mut v = Vec::with_capacity(values.len() + 2);
            v.push(before);
            v.extend_from_slice(values);
            v.push(after);
            v
        };
        let mut lines: Vec<Line> = vec![];
        let chosen = match kind {
            UsageMenu::ActivitySheet => self.activity_time_fields(),
            _ => self.resource_time_fields(),
        };
        let several = chosen.len() > 1;
        match kind {
            UsageMenu::ActivitySheet => {
                let serial = self.project_serial();
                let ix = self.shared_index();
                for row in self.rows() {
                    let line = self.activity_sheet_line(
                        row,
                        &sheet_fields,
                        &ix,
                        &periods,
                        serial,
                        display.costs,
                    );
                    let (fill, ink) = if line.band {
                        (Some(rgb(line.fill)), Some(rgb(line.ink)))
                    } else {
                        (None, None)
                    };
                    // One line per field, the field cells on the first.
                    for (n, f) in line.lines.iter().enumerate() {
                        lines.push(Line {
                            values: edges(f.before, &f.values, f.after),
                            texts: if n == 0 { line.texts.clone() } else { vec![] },
                            label: f.label.clone(),
                            cumulative: f.cumulative,
                            total: f.total,
                            fill,
                            ink,
                            bold: line.band,
                        });
                    }
                }
                if display.show_all_projects && !self.open_set.parked.is_empty() {
                    let others = self.other_projects_usage(&periods, display.costs);
                    let ((before, values, after), total) = &*others;
                    let style = self.grid.layout.group_sort.band_style(0);
                    let c = |x: [u8; 3]| rgb(Color32::from_rgb(x[0], x[1], x[2]));
                    let mut texts = vec![String::new(); sheet_fields.len().max(1)];
                    texts[0] = "Other open projects".into();
                    lines.push(Line {
                        texts,
                        label: "Remaining Labor".into(),
                        cumulative: false,
                        total: *total,
                        values: edges(*before, values, *after),
                        fill: Some(c(style.background)),
                        ink: Some(c(style.text)),
                        bold: true,
                    });
                }
            }
            _ => {
                let computed = self
                    .selection_sheet_rows(&sheet_fields, &periods)
                    .ok_or("Select a resource in the Resource Usage Spreadsheet first.")?;
                for line in computed.iter() {
                    let (fill, ink) = if line.band {
                        (Some(rgb(RESOURCE_BAND)), Some(rgb(RESOURCE_TEXT)))
                    } else if line.sub {
                        (Some(rgb(SUB_BAND)), Some(rgb(INK)))
                    } else {
                        (None, None)
                    };
                    lines.push(Line {
                        texts: line.texts.clone(),
                        label: line.label.clone(),
                        cumulative: line.cumulative,
                        total: line.total,
                        values: edges(line.before, &line.values, line.after),
                        fill,
                        ink,
                        bold: line.band || line.sub,
                    });
                }
            }
        }
        // Columns for what falls outside the periods, only when some does.
        let n = periods.len();
        let held = |k: usize| lines.iter().any(|l| l.values[k].abs() >= 0.005);
        let (first, last) = (held(0), held(n + 1));
        let keep = |k: usize| (k > 0 || first) && (k <= n || last);
        let mut header: Vec<String> = vec![];
        if display.line_numbers {
            header.push("No.".into());
        }
        header.extend(sheet_fields.iter().map(|f| f.title.clone()));
        // Several fields: a column naming each line's field, under the
        // measure, then the totals.
        if several {
            header.push(display.measure().into());
            header.push("Total".into());
        } else {
            header.push(display.value_title(&chosen));
        }
        if first {
            header.push(format!(
                "Before {}",
                format::date(origin, Some(periods[0].0))
            ));
        }
        header.extend(periods.iter().map(|(a, _)| format::date(origin, Some(*a))));
        if last {
            header.push(format!(
                "After {}",
                format::date(origin, Some(periods[n - 1].1))
            ));
        }
        let mut widths: Vec<f64> = vec![];
        if display.line_numbers {
            widths.push(6.);
        }
        widths.extend(
            sheet_fields
                .iter()
                .map(|f| f64::from(f.width / 7.).max(10.)),
        );
        if several {
            widths.push(24.);
        }
        widths.push(16.);
        widths.extend(
            (0..n + 2)
                .filter(|k| keep(*k))
                .map(|k| if k == 0 || k > n { 18. } else { 9. }),
        );
        let number = |v: f64| {
            if v.abs() < 0.005 {
                ReportCell::Empty
            } else {
                ReportCell::Number(v)
            }
        };
        let mut rows: Vec<ReportRow> = vec![];
        for (k, line) in lines.into_iter().enumerate() {
            let mut cells: Vec<ReportCell> = vec![];
            if display.line_numbers {
                cells.push(ReportCell::Number((k + 1) as f64));
            }
            for f in 0..sheet_fields.len() {
                cells.push(match line.texts.get(f) {
                    Some(t) if !t.is_empty() => ReportCell::Text(t.clone()),
                    _ => ReportCell::Empty,
                });
            }
            if several {
                cells.push(ReportCell::Text(line.label.clone()));
            }
            cells.push(number(line.total));
            // Cumulative runs from the start of the data, the Before column
            // included.
            let shown = if line.cumulative {
                running(0., &line.values)
            } else {
                display.shown(0., &line.values)
            };
            cells.extend(
                shown
                    .into_iter()
                    .enumerate()
                    .filter(|(k, _)| keep(*k))
                    .map(|(_, v)| number(v)),
            );
            rows.push(ReportRow {
                cells,
                fill: line.fill,
                ink: line.ink,
                bold: line.bold,
            });
        }
        let count = rows.len();
        let sheet = ReportSheet {
            name: match kind {
                UsageMenu::ActivitySheet => "Activity Usage".into(),
                _ => "Resource Usage".into(),
            },
            title: vec![
                self.project.model.project.name.clone(),
                match kind {
                    UsageMenu::ActivitySheet => "Activity Usage Spreadsheet".into(),
                    _ => "Resource Usage Spreadsheet".into(),
                },
            ],
            header,
            widths,
            rows,
        };
        let bytes = report_bytes(&[sheet], "dd-mmm-yy")?;
        Ok((bytes, count))
    }
}
