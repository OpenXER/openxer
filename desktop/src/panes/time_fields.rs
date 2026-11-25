//! The time-phased fields of the usage spreadsheets (Spreadsheet Fields >
//! Customize...): which values a spreadsheet shows for each activity or
//! assignment, one line per chosen field. A field is a series (planned,
//! actual, remaining, remaining late or at completion), for the Activity
//! Usage Spreadsheet also a resource kind (labor, nonlabor, material or all
//! of them), and whether it runs as a cumulative total. Display > Units or
//! Costs decides what every field measures, as it does for the profiles.
use super::*;
use crate::views::assignments::{self as asg, Assignment};
use std::collections::HashMap;

/// Which spreadsheet a field list belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sheet {
    Activity,
    Resource,
}

/// What a field spreads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Series {
    Planned,
    Actual,
    Remaining,
    RemainingLate,
    AtCompletion,
}

impl Series {
    pub const ALL: [Series; 5] = [
        Series::Planned,
        Series::Actual,
        Series::Remaining,
        Series::RemainingLate,
        Series::AtCompletion,
    ];
    fn key(self) -> &'static str {
        match self {
            Series::Planned => "planned",
            Series::Actual => "actual",
            Series::Remaining => "remaining",
            Series::RemainingLate => "remaining_late",
            Series::AtCompletion => "at_completion",
        }
    }
    fn caption(self) -> &'static str {
        match self {
            Series::Planned => "Planned",
            Series::Actual => "Actual",
            Series::Remaining => "Remaining",
            Series::RemainingLate => "Remaining Late",
            Series::AtCompletion => "At Completion",
        }
    }
}

/// Which assignments a field of the Activity Usage Spreadsheet counts, by
/// the resource's type. The Resource Usage Spreadsheet's fields count the
/// resource's own assignments (`Own`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Labor,
    Nonlabor,
    Material,
    /// Every kind: in costs all three, in units labor and nonlabor (material
    /// units are counted in their own unit of measure, so they do not add).
    Total,
    Own,
}

impl Scope {
    const ACTIVITY: [Scope; 4] = [Scope::Labor, Scope::Nonlabor, Scope::Material, Scope::Total];
    fn key(self) -> &'static str {
        match self {
            Scope::Labor => "labor",
            Scope::Nonlabor => "nonlabor",
            Scope::Material => "material",
            Scope::Total => "total",
            Scope::Own => "",
        }
    }
    fn caption(self) -> &'static str {
        match self {
            Scope::Labor => "Labor",
            Scope::Nonlabor => "Nonlabor",
            Scope::Material => "Material",
            Scope::Total => "Total",
            Scope::Own => "",
        }
    }
    /// Whether an assignment of a resource of `kind` counts, in units or in
    /// costs.
    fn counts(self, kind: Kind, costs: bool) -> bool {
        match self {
            Scope::Labor => kind == Kind::Labor,
            Scope::Nonlabor => kind == Kind::Nonlabor,
            Scope::Material => costs && kind == Kind::Material,
            Scope::Total => costs || kind != Kind::Material,
            Scope::Own => true,
        }
    }
}

/// A resource's type, as the usage views count it: anything that is not
/// equipment or material is labor (`openxer_xer::spread::labor_resources`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Labor,
    Nonlabor,
    Material,
}

/// The type of each resource of the project, by resource id.
pub fn kinds(p: &ProjectData) -> HashMap<i64, Kind> {
    p.document.table("RSRC").map_or_else(HashMap::new, |t| {
        t.rows
            .iter()
            .map(|r| {
                let kind = match t.value(r, "rsrc_type") {
                    "RT_Equip" => Kind::Nonlabor,
                    "RT_Mat" => Kind::Material,
                    _ => Kind::Labor,
                };
                (crate::records::num(t.value(r, "rsrc_id")), kind)
            })
            .collect()
    })
}

/// One time-phased field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeField {
    pub series: Series,
    pub scope: Scope,
    /// The Cumulative group: running totals from the start of the data.
    pub cumulative: bool,
}

impl TimeField {
    /// Its saved id: `[cum_]<series>[_<scope>]`.
    pub fn id(self) -> String {
        let mut id = String::new();
        if self.cumulative {
            id.push_str("cum_");
        }
        id.push_str(self.series.key());
        if self.scope != Scope::Own {
            id.push('_');
            id.push_str(self.scope.key());
        }
        id
    }

    /// The field with saved id `id` on `sheet`'s list.
    pub fn parse(sheet: Sheet, id: &str) -> Option<TimeField> {
        catalog(sheet).into_iter().find(|f| f.id() == id)
    }

    /// What a spreadsheet line shows beside the field's total.
    pub fn label(self) -> String {
        let base = match self.scope {
            Scope::Own => self.series.caption().to_string(),
            scope => format!("{} {}", self.series.caption(), scope.caption()),
        };
        if self.cumulative {
            format!("Cumulative {base}")
        } else {
            base
        }
    }

    /// The group of the chooser's Available Options it is listed in.
    pub fn group(self) -> &'static str {
        if self.cumulative {
            GROUPS[0]
        } else {
            GROUPS[1]
        }
    }

    /// Its field name in the Fields dialog and the Spreadsheet Fields menu,
    /// in units or in costs: `Remaining Labor Units` on the Activity Usage
    /// Spreadsheet, `Remaining Early Units` on the Resource Usage
    /// Spreadsheet, the cumulative ones with `Cum` in front, as the
    /// reference names them.
    pub fn name(self, sheet: Sheet, costs: bool) -> String {
        let series = match (self.series, sheet) {
            (Series::Planned, _) => "Budgeted",
            (Series::Actual, _) => "Actual",
            (Series::Remaining, Sheet::Resource) => "Remaining Early",
            (Series::Remaining, Sheet::Activity) => "Remaining",
            (Series::RemainingLate, _) => "Remaining Late",
            (Series::AtCompletion, _) => "At Completion",
        };
        let mut words = vec![];
        if self.cumulative {
            words.push("Cum");
        }
        words.push(series);
        if self.scope != Scope::Own {
            words.push(self.scope.caption());
        }
        words.push(if costs { "Cost" } else { "Units" });
        words.join(" ")
    }

    /// How the Fields dialog's Selected Options lists it: the name, or on
    /// the Resource Usage Spreadsheet the column title and the name in
    /// brackets where they differ (`Remaining Units  (Remaining Early
    /// Units)`).
    pub fn selected_name(self, sheet: Sheet, costs: bool) -> String {
        let name = self.name(sheet, costs);
        if sheet == Sheet::Resource && self.series == Series::Remaining {
            let title = name.replace("Remaining Early", "Remaining");
            return format!("{title}  ({name})");
        }
        name
    }
}

/// The groups of the chooser's Available Options, in the reference's
/// order.
pub const GROUPS: [&str; 2] = ["Cumulative", "Time Interval"];

/// Every field `sheet` offers, in the order the chooser lists them.
pub fn catalog(sheet: Sheet) -> Vec<TimeField> {
    let scopes: &[Scope] = match sheet {
        Sheet::Activity => &Scope::ACTIVITY,
        Sheet::Resource => &[Scope::Own],
    };
    let mut out = vec![];
    for cumulative in [false, true] {
        for series in Series::ALL {
            for &scope in scopes {
                out.push(TimeField {
                    series,
                    scope,
                    cumulative,
                });
            }
        }
    }
    out
}

/// The list a spreadsheet starts with: remaining labor units on the
/// Activity Usage Spreadsheet, remaining units on the Resource Usage
/// Spreadsheet.
pub fn default_ids(sheet: Sheet) -> Vec<String> {
    vec![match sheet {
        Sheet::Activity => "remaining_labor".to_string(),
        Sheet::Resource => "remaining".to_string(),
    }]
}

/// Most fields one spreadsheet shows.
pub const MAX_FIELDS: usize = 12;

/// The fields of saved ids `ids` that `sheet` knows, in order and without
/// repeats; the default list when none is left.
pub fn fields_of(sheet: Sheet, ids: &[String]) -> Vec<TimeField> {
    let mut out: Vec<TimeField> = vec![];
    for f in ids.iter().filter_map(|id| TimeField::parse(sheet, id)) {
        if !out.contains(&f) && out.len() < MAX_FIELDS {
            out.push(f);
        }
    }
    if out.is_empty() {
        default_ids(sheet)
            .iter()
            .filter_map(|id| TimeField::parse(sheet, id))
            .collect()
    } else {
        out
    }
}

/// Saved ids read back from a layout: the known ones, or `None`.
pub fn restore_ids(v: &serde_json::Value, sheet: Sheet) -> Option<Vec<String>> {
    let ids: Vec<String> = v
        .as_array()?
        .iter()
        .filter_map(|x| x.as_str())
        .map(str::to_string)
        .collect();
    let known = ids.iter().any(|id| TimeField::parse(sheet, id).is_some());
    let kept: Vec<String> = fields_of(sheet, &ids).iter().map(|f| f.id()).collect();
    known.then_some(kept)
}

/// One field's figures on a spreadsheet line: its total, what falls before
/// the periods, each period's value and what falls after them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Figures {
    pub label: String,
    pub cumulative: bool,
    pub total: f64,
    pub before: f64,
    pub values: Vec<f64>,
    pub after: f64,
}

impl Figures {
    /// Figures from values spread over `edged` periods (see
    /// `usage::edged`).
    pub fn edged(field: TimeField, values: &[f64], total: f64) -> Self {
        let (before, values, after) = super::usage::unedge(values);
        Self {
            label: field.label(),
            cumulative: field.cumulative,
            total,
            before,
            values,
            after,
        }
    }
}

/// One assignment's `field` spread over `periods` (added into `out`) and
/// its stored total, in units or in costs.
pub fn assignment_into(
    p: &ProjectData,
    a: &Assignment,
    field: TimeField,
    periods: &[(i64, i64)],
    out: &mut [f64],
) -> f64 {
    match field.series {
        Series::Planned => {
            asg::budget_into(p, a, periods, out);
            a.budget
        }
        Series::Actual => {
            asg::actual_into(p, a, periods, out);
            a.actual
        }
        Series::Remaining => {
            asg::remaining_into(p, a, periods, out);
            a.remaining
        }
        Series::RemainingLate => {
            asg::remaining_late_into(p, a, periods, out);
            a.remaining
        }
        Series::AtCompletion => {
            asg::actual_into(p, a, periods, out);
            asg::remaining_into(p, a, periods, out);
            a.actual + a.remaining
        }
    }
}

/// The Activity Usage Spreadsheet's `field` for the activities in `scope`:
/// the spread of every assignment the field counts over `edged` periods,
/// and their stored total.
pub fn activity_into(
    p: &ProjectData,
    by_task: &BTreeMap<usize, Vec<Assignment>>,
    kinds: &HashMap<i64, Kind>,
    scope: &BTreeSet<usize>,
    field: TimeField,
    costs: bool,
    edged: &[(i64, i64)],
) -> (Vec<f64>, f64) {
    let mut values = vec![0.; edged.len()];
    let mut total = 0.;
    for task in scope {
        for a in by_task.get(task).into_iter().flatten() {
            let Some(kind) = kinds.get(&a.rsrc) else {
                continue;
            };
            if !field.scope.counts(*kind, costs) {
                continue;
            }
            let costed;
            let a = if costs {
                costed = a.as_costs();
                &costed
            } else {
                a
            };
            total += assignment_into(p, a, field, edged, &mut values);
        }
    }
    (values, total)
}

/// Every assignment of the project by activity (all resource types),
/// cloned from `list` so they share its spread plans.
pub fn by_task(list: &[Assignment]) -> BTreeMap<usize, Vec<Assignment>> {
    let mut out: BTreeMap<usize, Vec<Assignment>> = BTreeMap::new();
    for a in list {
        out.entry(a.task).or_default().push(a.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_read_back_and_unknown_ones_drop() {
        for sheet in [Sheet::Activity, Sheet::Resource] {
            for f in catalog(sheet) {
                assert_eq!(TimeField::parse(sheet, &f.id()), Some(f));
            }
        }
        assert_eq!(catalog(Sheet::Activity).len(), 40);
        assert_eq!(catalog(Sheet::Resource).len(), 10);
        // A resource sheet field is not an activity sheet field.
        assert_eq!(TimeField::parse(Sheet::Activity, "remaining"), None);
        let ids = ["nonsense", "cum_planned_total", "cum_planned_total"].map(String::from);
        let kept = fields_of(Sheet::Activity, &ids);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].label(), "Cumulative Planned Total");
        assert_eq!(
            fields_of(Sheet::Resource, &[]),
            vec![TimeField {
                series: Series::Remaining,
                scope: Scope::Own,
                cumulative: false,
            }]
        );
        assert_eq!(
            restore_ids(&serde_json::json!(["x", "actual"]), Sheet::Resource),
            Some(vec!["actual".to_string()])
        );
        assert_eq!(
            restore_ids(&serde_json::json!(["x"]), Sheet::Resource),
            None
        );
    }

    #[test]
    fn fields_carry_the_reference_names() {
        let activity = |id: &str| TimeField::parse(Sheet::Activity, id).unwrap();
        let resource = |id: &str| TimeField::parse(Sheet::Resource, id).unwrap();
        let labor = activity("remaining_labor").name(Sheet::Activity, false);
        assert_eq!(labor, "Remaining Labor Units");
        let early = resource("remaining").selected_name(Sheet::Resource, false);
        assert_eq!(early, "Remaining Units  (Remaining Early Units)");
        let budget = resource("planned").name(Sheet::Resource, false);
        assert_eq!(budget, "Budgeted Units");
        let cost = resource("cum_actual").name(Sheet::Resource, true);
        assert_eq!(cost, "Cum Actual Cost");
        let nonlabor = activity("planned_nonlabor").selected_name(Sheet::Activity, true);
        assert_eq!(nonlabor, "Budgeted Nonlabor Cost");
        assert_eq!(GROUPS, ["Cumulative", "Time Interval"]);
    }

    #[test]
    fn scopes_count_by_resource_type() {
        use Kind::*;
        assert!(Scope::Labor.counts(Labor, false));
        assert!(!Scope::Labor.counts(Nonlabor, true));
        assert!(Scope::Nonlabor.counts(Nonlabor, false));
        // Material counts in costs only; Total adds material in costs only.
        assert!(!Scope::Material.counts(Material, false));
        assert!(Scope::Material.counts(Material, true));
        assert!(!Scope::Total.counts(Material, false));
        assert!(Scope::Total.counts(Material, true));
        assert!(Scope::Total.counts(Nonlabor, false));
    }
}
