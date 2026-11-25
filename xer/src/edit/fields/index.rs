//! Lookups built once per pass over the document, and the value of a field
//! for one activity.
use super::{finish_value, start_value, Field, Kind, Source, Value};
use crate::edit::activity::{Measure, Phase, DURATION_TYPES, PERCENT_TYPES};
use crate::edit::records::{self, float, num};
use crate::edit::wbs::{self, Tree};
use crate::ev::Figures;
use crate::{constraint_label, is_critical, status_label, type_label, ProjectData};
use openxer_engine::model::CstrType;
use std::cell::OnceCell;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Predecessor and successor activity indexes per activity.
type Neighbors = (Vec<Vec<usize>>, Vec<Vec<usize>>);

/// One activity code value.
#[derive(Clone, Debug, Default)]
pub struct CodeValue {
    pub short_name: String,
    pub name: String,
    pub sequence: i64,
    pub type_id: i64,
    /// The value above it in its code type's hierarchy; 0 at the top.
    pub parent: i64,
}

/// The project's WBS: its tree, full codes (project ID, then the short
/// codes from the top down), names, short codes and pre-order positions.
pub struct WbsIndex {
    pub tree: Tree,
    pub codes: BTreeMap<i64, String>,
    pub names: BTreeMap<i64, String>,
    pub short_codes: BTreeMap<i64, String>,
    pub order: BTreeMap<i64, usize>,
}

/// Lookups built once per pass over the document, each only when a field
/// that needs it is asked for.
#[derive(Default)]
pub struct Index {
    /// (longest path defines critical, total float threshold in hours)
    pub critical: (bool, f64),
    task_rows: OnceCell<HashMap<i64, usize>>,
    codes: OnceCell<HashMap<(i64, i64), i64>>,
    code_values: OnceCell<HashMap<i64, CodeValue>>,
    udfs: OnceCell<HashMap<(i64, i64), usize>>,
    figures: OnceCell<HashMap<i64, [Figures; 4]>>,
    /// Actual this period per activity: labor units, nonlabor units, labor
    /// cost, nonlabor cost, material cost.
    this_period: OnceCell<HashMap<i64, [f64; 5]>>,
    wbs: OnceCell<WbsIndex>,
    links: OnceCell<Neighbors>,
}

impl Index {
    pub fn new(critical: (bool, f64)) -> Self {
        Self {
            critical,
            ..Default::default()
        }
    }
    /// An index with the project's own critical definition.
    pub fn for_project(p: &ProjectData) -> Self {
        Self::new(crate::critical_definition(p))
    }
    fn task_row(&self, p: &ProjectData, task_id: i64) -> Option<usize> {
        self.task_rows
            .get_or_init(|| {
                let pid = p.model.project.proj_id;
                p.document.table("TASK").map_or_else(HashMap::new, |t| {
                    t.rows
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| num(t.value(r, "proj_id")) == pid)
                        .map(|(i, r)| (num(t.value(r, "task_id")), i))
                        .collect()
                })
            })
            .get(&task_id)
            .copied()
    }
    /// A TASK column of the activity, as stored.
    pub fn task_value(&self, p: &ProjectData, task_id: i64, column: &str) -> String {
        self.task_row(p, task_id)
            .map(|i| records::get(&p.document, "TASK", i, column))
            .unwrap_or_default()
    }
    /// The code value an activity has for a code type.
    pub fn code_of(&self, p: &ProjectData, task_id: i64, type_id: i64) -> Option<CodeValue> {
        let code = self.code_id(p, task_id, type_id)?;
        self.code_values(p).get(&code).cloned()
    }
    /// The `actv_code_id` an activity has for a code type.
    pub fn code_id(&self, p: &ProjectData, task_id: i64, type_id: i64) -> Option<i64> {
        let codes = self.codes.get_or_init(|| {
            let pid = p.model.project.proj_id;
            p.document.table("TASKACTV").map_or_else(HashMap::new, |t| {
                let has_project = t.field("proj_id").is_some();
                t.rows
                    .iter()
                    .filter(|r| !has_project || num(t.value(r, "proj_id")) == pid)
                    .map(|r| {
                        (
                            (
                                num(t.value(r, "task_id")),
                                num(t.value(r, "actv_code_type_id")),
                            ),
                            num(t.value(r, "actv_code_id")),
                        )
                    })
                    .collect()
            })
        });
        codes.get(&(task_id, type_id)).copied()
    }
    /// Every activity code value by `actv_code_id`.
    pub fn code_values(&self, p: &ProjectData) -> &HashMap<i64, CodeValue> {
        self.code_values.get_or_init(|| {
            p.document.table("ACTVCODE").map_or_else(HashMap::new, |t| {
                t.rows
                    .iter()
                    .map(|r| {
                        (
                            num(t.value(r, "actv_code_id")),
                            CodeValue {
                                short_name: t.value(r, "short_name").into(),
                                name: t.value(r, "actv_code_name").into(),
                                sequence: num(t.value(r, "seq_num")),
                                type_id: num(t.value(r, "actv_code_type_id")),
                                parent: num(t.value(r, "parent_actv_code_id")),
                            },
                        )
                    })
                    .collect()
            })
        })
    }
    /// The value of user defined field `type_id` of an activity, read as
    /// `kind`.
    pub fn udf(&self, p: &ProjectData, task_id: i64, type_id: i64, kind: Kind) -> Value {
        let rows = self.udfs.get_or_init(|| {
            p.document.table("UDFVALUE").map_or_else(HashMap::new, |t| {
                t.rows
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        (
                            (num(t.value(r, "udf_type_id")), num(t.value(r, "fk_id"))),
                            i,
                        )
                    })
                    .collect()
            })
        });
        let Some(&row) = rows.get(&(type_id, task_id)) else {
            return Value::Empty;
        };
        let get = |k: &str| records::get(&p.document, "UDFVALUE", row, k);
        match kind {
            Kind::Date => Value::date(
                crate::parse_relative(p.model.origin_serial, &get("udf_date"))
                    .ok()
                    .flatten(),
            ),
            Kind::Number | Kind::Cost => {
                get("udf_number")
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .map_or(Value::Empty, |v| {
                        if kind == Kind::Cost {
                            Value::Cost(v)
                        } else {
                            Value::Number(v)
                        }
                    })
            }
            _ => {
                let mut text = get("udf_text");
                if text.is_empty() {
                    let code = num(&get("udf_code_id"));
                    if code != 0 {
                        text = records::lookup(
                            &p.document,
                            "UDFCODE",
                            "udf_code_id",
                            code,
                            "short_name",
                        );
                    }
                }
                Value::text(text)
            }
        }
    }
    /// [units or cost] figures per activity, keyed by `Measure` position.
    pub fn figures(&self, p: &ProjectData, task_id: i64, measure: Measure) -> Option<Figures> {
        let all = self.figures.get_or_init(|| crate::ev::figure_table(p));
        let pick = |key: i64, slot: usize| all.get(&key).map(|f| f[slot]);
        match measure {
            Measure::LaborUnits => pick(task_id * 2, 0),
            Measure::NonlaborUnits => pick(task_id * 2, 1),
            Measure::MaterialUnits => pick(task_id * 2, 2),
            Measure::LaborCost => pick(task_id * 2 + 1, 0),
            Measure::NonlaborCost => pick(task_id * 2 + 1, 1),
            Measure::MaterialCost => pick(task_id * 2 + 1, 2),
            Measure::ExpenseCost => pick(task_id * 2 + 1, 3),
            Measure::TotalCost => all.get(&(task_id * 2 + 1)).map(crate::ev::total),
        }
    }
    /// Actual this period of the activity for an editable measure: the sum
    /// of its assignments of that type, or its own figure without one.
    pub fn this_period(&self, p: &ProjectData, task_id: i64, measure: Measure) -> f64 {
        let all = self.this_period.get_or_init(|| {
            let doc = &p.document;
            let pid = p.model.project.proj_id;
            let mut out: HashMap<i64, [f64; 5]> = HashMap::new();
            let mut has = [HashSet::new(), HashSet::new()];
            if let Some(t) = doc.table("TASKRSRC") {
                let types: HashMap<i64, String> =
                    doc.table("RSRC").map_or_else(HashMap::new, |r| {
                        r.rows
                            .iter()
                            .map(|row| {
                                (
                                    num(r.value(row, "rsrc_id")),
                                    r.value(row, "rsrc_type").to_string(),
                                )
                            })
                            .collect()
                    });
                for r in &t.rows {
                    if t.field("proj_id").is_some() && num(t.value(r, "proj_id")) != pid {
                        continue;
                    }
                    let task = num(t.value(r, "task_id"));
                    let mut kind = t.value(r, "rsrc_type").to_string();
                    if kind.is_empty() {
                        kind = types
                            .get(&num(t.value(r, "rsrc_id")))
                            .cloned()
                            .unwrap_or_default();
                    }
                    let (units, cost) = match kind.as_str() {
                        "RT_Equip" => (Some(1), 3),
                        "RT_Mat" => (None, 4),
                        _ => (Some(0), 2),
                    };
                    if let Some(u) = units {
                        has[u].insert(task);
                        out.entry(task).or_default()[u] += float(t.value(r, "act_this_per_qty"));
                    }
                    out.entry(task).or_default()[cost] += float(t.value(r, "act_this_per_cost"));
                }
            }
            if let Some(t) = doc.table("TASK") {
                for r in &t.rows {
                    if num(t.value(r, "proj_id")) != pid {
                        continue;
                    }
                    let task = num(t.value(r, "task_id"));
                    for (slot, column) in
                        [(0, "act_this_per_work_qty"), (1, "act_this_per_equip_qty")]
                    {
                        let v = float(t.value(r, column));
                        if v != 0. && !has[slot].contains(&task) {
                            out.entry(task).or_default()[slot] += v;
                        }
                    }
                }
            }
            out
        });
        let slot = match measure {
            Measure::LaborUnits => 0,
            Measure::NonlaborUnits => 1,
            Measure::LaborCost => 2,
            Measure::NonlaborCost => 3,
            Measure::MaterialCost => 4,
            _ => return 0.,
        };
        all.get(&task_id).map_or(0., |f| f[slot])
    }
    /// The project's WBS lookups.
    pub fn wbs(&self, p: &ProjectData) -> &WbsIndex {
        self.wbs.get_or_init(|| {
            let tree = Tree::build(p);
            let preorder = tree.preorder();
            let codes = wbs::full_codes_in(p, &tree, &preorder);
            let order = preorder
                .iter()
                .enumerate()
                .map(|(n, (id, _))| (*id, n))
                .collect();
            WbsIndex {
                codes,
                names: p.wbs.iter().map(|w| (w.id, w.name.clone())).collect(),
                short_codes: p.wbs.iter().map(|w| (w.id, w.code.clone())).collect(),
                order,
                tree,
            }
        })
    }
    /// Predecessor and successor activity indexes of every activity.
    pub fn links(&self, p: &ProjectData) -> &(Vec<Vec<usize>>, Vec<Vec<usize>>) {
        self.links.get_or_init(|| {
            let n = p.model.activities.len();
            let mut preds = vec![vec![]; n];
            let mut succs = vec![vec![]; n];
            for l in &p.model.links {
                if l.pred < n && l.succ < n {
                    preds[l.succ].push(l.pred);
                    succs[l.pred].push(l.succ);
                }
            }
            (preds, succs)
        })
    }
}

/// The amount of one phase.
fn phase_value(f: &Figures, phase: Phase) -> f64 {
    match phase {
        Phase::Budgeted => f.budget,
        Phase::Actual => f.actual,
        Phase::Remaining => f.remaining,
        Phase::AtCompletion => f.actual + f.remaining,
    }
}

/// The value of a field from `source` (holding `kind`) for activity `i`.
pub fn value(p: &ProjectData, ix: &Index, source: Source, kind: Kind, i: usize) -> Value {
    let Some(a) = p.model.activities.get(i) else {
        return Value::Empty;
    };
    let task_id = a.task_id;
    match source {
        Source::Code(type_id) => ix
            .code_of(p, task_id, type_id)
            .map_or(Value::Empty, |c| Value::text(c.short_name)),
        Source::Udf(type_id) => ix.udf(p, task_id, type_id, kind),
        Source::Field(field) => field_value(p, ix, i, field),
    }
}

fn field_value(p: &ProjectData, ix: &Index, i: usize, field: Field) -> Value {
    let a = &p.model.activities[i];
    let d = p.details.get(&a.task_id);
    let leveled = p.model.options.do_level;
    let original = d.map_or(0, |d| d.original_duration);
    let elapsed = || -> i64 {
        let finish = a.act_finish.or(p.model.project.data_date);
        match (a.act_start, finish, p.model.calendars.get(a.cal)) {
            (Some(s), Some(f), Some(cal)) if f > s => cal.between(s, f).max(0),
            _ => 0,
        }
    };
    let duration_percent = || {
        if original > 0 {
            ((original - a.rem) as f64 / original as f64 * 100.).clamp(0., 100.)
        } else if a.is_completed() {
            100.
        } else {
            0.
        }
    };
    let list = |items: &[usize]| {
        Value::text(
            items
                .iter()
                .filter_map(|&j| p.model.activities.get(j).map(|b| b.task_code.as_str()))
                .collect::<Vec<_>>()
                .join(", "),
        )
    };
    match field {
        Field::ActivityId => Value::text(a.task_code.clone()),
        Field::ActivityName => Value::text(p.name(i)),
        Field::ActivityType => Value::text(type_label(a.task_type)),
        Field::ActivityStatus => Value::text(status_label(a.status)),
        Field::Wbs => d.map_or(Value::Empty, |d| {
            Value::text(ix.wbs(p).codes.get(&d.wbs).cloned().unwrap_or_default())
        }),
        Field::WbsName => d.map_or(Value::Empty, |d| {
            Value::text(ix.wbs(p).names.get(&d.wbs).cloned().unwrap_or_default())
        }),
        Field::Calendar => Value::text(p.calendar_names.get(a.cal).cloned().unwrap_or_default()),
        Field::PrimaryResource => {
            let id = num(&ix.task_value(p, a.task_id, "rsrc_id"));
            if id == 0 {
                Value::Empty
            } else {
                Value::text(
                    p.resource_names
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| id.to_string()),
                )
            }
        }
        Field::DurationType => {
            let code = ix.task_value(p, a.task_id, "duration_type");
            Value::text(
                DURATION_TYPES
                    .iter()
                    .find(|(c, _)| *c == code)
                    .map_or("", |(_, l)| l),
            )
        }
        Field::PercentType => {
            let code = ix.task_value(p, a.task_id, "complete_pct_type");
            Value::text(
                PERCENT_TYPES
                    .iter()
                    .find(|(c, _)| *c == code)
                    .map_or(PERCENT_TYPES[1].1, |(_, l)| l),
            )
        }
        Field::AutoComputeActuals => {
            Value::Flag(ix.task_value(p, a.task_id, "auto_compute_act_flag") == "Y")
        }
        Field::Critical => Value::Flag(is_critical(a, ix.critical)),
        Field::LongestPath => Value::Flag(a.driving_path),
        Field::Start => start_value(a, leveled),
        Field::Finish => finish_value(a, leveled),
        Field::EarlyStart => Value::date(a.early_start),
        Field::EarlyFinish => Value::date(a.early_finish),
        Field::LateStart => Value::date(a.late_start),
        Field::LateFinish => Value::date(a.late_finish),
        Field::ActualStart => Value::date(a.act_start),
        Field::ActualFinish => Value::date(a.act_finish),
        Field::PlannedStart => Value::date(a.target_start),
        Field::PlannedFinish => Value::date(a.target_finish),
        Field::RemainingEarlyStart => Value::date(if a.is_completed() { None } else { a.restart }),
        Field::RemainingEarlyFinish => Value::date(if a.is_completed() { None } else { a.reend }),
        Field::RemainingLateStart => Value::date(a.rem_late_start),
        Field::RemainingLateFinish => Value::date(a.rem_late_finish),
        Field::ExpectedFinish => Value::date(a.expect_end),
        Field::PrimaryConstraint => {
            if a.cstr == CstrType::None {
                Value::Empty
            } else {
                Value::text(constraint_label(a.cstr))
            }
        }
        Field::PrimaryConstraintDate => Value::date(a.cstr_date),
        Field::SecondaryConstraint => {
            if a.cstr2 == CstrType::None {
                Value::Empty
            } else {
                Value::text(constraint_label(a.cstr2))
            }
        }
        Field::SecondaryConstraintDate => Value::date(a.cstr_date2),
        Field::SuspendDate => Value::date(
            crate::parse_relative(
                p.model.origin_serial,
                &ix.task_value(p, a.task_id, "suspend_date"),
            )
            .ok()
            .flatten(),
        ),
        Field::ResumeDate => Value::date(a.resume),
        Field::OriginalDuration => Value::Duration(original),
        Field::ActualDuration => Value::Duration(elapsed()),
        Field::RemainingDuration => Value::Duration(a.rem),
        Field::AtCompletionDuration => Value::Duration(elapsed() + a.rem),
        Field::DurationPercent => Value::Percent(duration_percent()),
        Field::TotalFloat => a.total_float.map_or(Value::Empty, Value::Duration),
        Field::FreeFloat => a.free_float.map_or(Value::Empty, Value::Duration),
        Field::FloatPath => Value::Number(a.float_path as f64),
        Field::FloatPathOrder => Value::Number(a.float_path_order as f64),
        Field::PhysicalPercent => Value::Percent(d.map_or(0., |d| d.percent)),
        Field::ActivityPercent => match ix.task_value(p, a.task_id, "complete_pct_type").as_str() {
            "CP_Phys" => Value::Percent(d.map_or(0., |d| d.percent)),
            "CP_Units" => {
                let f = ix
                    .figures(p, a.task_id, Measure::LaborUnits)
                    .unwrap_or_default();
                let total = f.actual + f.remaining;
                Value::Percent(if total > 0. {
                    f.actual / total * 100.
                } else if a.is_completed() {
                    100.
                } else {
                    0.
                })
            }
            _ => Value::Percent(duration_percent()),
        },
        Field::Predecessors => list(&ix.links(p).0[i]),
        Field::Successors => list(&ix.links(p).1[i]),
        Field::ThisPeriod(measure) => {
            let v = ix.this_period(p, a.task_id, measure);
            if measure.is_cost() {
                Value::Cost(v)
            } else {
                Value::Units(v)
            }
        }
        Field::Figure(measure, phase) => {
            let v = ix
                .figures(p, a.task_id, measure)
                .map_or(0., |f| phase_value(&f, phase));
            if measure.is_cost() {
                Value::Cost(v)
            } else {
                Value::Units(v)
            }
        }
    }
}
