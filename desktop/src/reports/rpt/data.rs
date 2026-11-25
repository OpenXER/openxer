//! Data binding: the sources a report loops over (project, WBS elements,
//! activities, resource assignments, resources, relationships, codes, user
//! defined fields and notebooks) and the fields it reads from them. Source
//! and field names follow the XER table and column names; a few computed
//! names (`status_code`, `wbs_concat_name`, `start_date`) are added. A field
//! this vocabulary does not know is an empty cell and a warning, never an
//! error.
use crate::grid::columns::{self, Column, Index, Source, Value};
use crate::views::assignments::{self, Assignment};
use crate::views::resources::{self, Resource};
use crate::views::wbs;
use chrono::{NaiveDate, NaiveDateTime};
use openxer_engine::calendar::DAY;
use openxer_engine::model::{LinkType, StatusType};
use openxer_xer::ProjectData;
use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeSet, HashMap};

/// A record of a data source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rec {
    Project,
    Wbs(usize),
    Task(usize),
    Assign(usize),
    Rsrc(usize),
    Link(usize),
    Code(usize),
    Udf(usize),
    Memo(usize),
}

impl Rec {
    pub fn table(self) -> &'static str {
        match self {
            Rec::Project => "PROJECT",
            Rec::Wbs(_) => "PROJWBS",
            Rec::Task(_) => "TASK",
            Rec::Assign(_) => "TASKRSRC",
            Rec::Rsrc(_) => "RSRC",
            Rec::Link(_) => "TASKPRED",
            Rec::Code(_) => "TASKACTV",
            Rec::Udf(_) => "UDFVALUE",
            Rec::Memo(_) => "TASKMEMO",
        }
    }
}

/// The data sources a report can loop over.
pub const SOURCES: [(&str, &str); 9] = [
    ("PROJECT", "Project"),
    ("PROJWBS", "WBS"),
    ("TASK", "Activities"),
    ("TASKRSRC", "Resource Assignments"),
    ("RSRC", "Resources"),
    ("TASKPRED", "Relationships"),
    ("TASKACTV", "Activity Codes"),
    ("UDFVALUE", "User Defined Fields"),
    ("TASKMEMO", "Notebook Topics"),
];

pub fn source_caption(table: &str) -> &'static str {
    SOURCES
        .iter()
        .find(|(t, _)| t.eq_ignore_ascii_case(table))
        .map_or("", |(_, c)| c)
}

/// Longest tree walk followed from the top of a hierarchy.
pub const MAX_TREE_DEPTH: usize = 64;

pub struct WbsNode {
    pub id: i64,
    pub parent: Option<usize>,
    pub level: u32,
    pub code: String,
    pub name: String,
    pub full: String,
    pub seq: i64,
}

struct Extra {
    task: usize,
    a: String,
    b: String,
    c: String,
}

pub struct Data<'a> {
    pub p: &'a ProjectData,
    pub ix: &'a Index,
    catalog: Vec<Column>,
    pub wbs: Vec<WbsNode>,
    wbs_kids: Vec<Vec<usize>>,
    task_wbs: Vec<Option<usize>>,
    wbs_tasks: Vec<Vec<usize>>,
    pub assigns: Vec<Assignment>,
    assign_by_task: Vec<Vec<usize>>,
    assign_by_rsrc: HashMap<i64, Vec<usize>>,
    pub resources: Vec<Resource>,
    rsrc_pos: HashMap<i64, usize>,
    rsrc_kids: Vec<Vec<usize>>,
    links_in: Vec<Vec<usize>>,
    links_out: Vec<Vec<usize>>,
    codes: OnceCell<Vec<Extra>>,
    udfs: OnceCell<Vec<Extra>>,
    memos: OnceCell<Vec<Extra>>,
    rollups: OnceCell<std::collections::BTreeMap<Option<i64>, wbs::Rollup>>,
    /// The run time, for relative dates and the date placeholders.
    pub now: NaiveDateTime,
    warnings: RefCell<BTreeSet<String>>,
}

fn num(s: &str) -> f64 {
    s.trim().parse().unwrap_or(0.)
}

fn epoch() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(1899, 12, 30)
        .expect("a valid date")
        .and_hms_opt(0, 0, 0)
        .expect("a valid time")
}

impl<'a> Data<'a> {
    pub fn new(p: &'a ProjectData, ix: &'a Index, now: NaiveDateTime) -> Self {
        let m = &p.model;
        let n = m.activities.len();
        // WBS elements in display order.
        let w = ix.wbs(p);
        let mut nodes: Vec<WbsNode> = vec![];
        let mut pos = HashMap::new();
        let mut stack: Vec<usize> = vec![];
        for (id, level) in w.tree.preorder() {
            let Some(row) = p.wbs.iter().find(|x| x.id == id) else {
                continue;
            };
            while stack
                .last()
                .is_some_and(|&s| nodes[s].level >= level as u32)
            {
                stack.pop();
            }
            pos.insert(id, nodes.len());
            nodes.push(WbsNode {
                id,
                parent: stack.last().copied(),
                level: level as u32,
                code: row.code.clone(),
                name: row.name.clone(),
                full: w.codes.get(&id).cloned().unwrap_or_default(),
                seq: row.sequence,
            });
            stack.push(nodes.len() - 1);
        }
        let mut wbs_kids = vec![vec![]; nodes.len()];
        for (i, nd) in nodes.iter().enumerate() {
            if let Some(par) = nd.parent {
                wbs_kids[par].push(i);
            }
        }
        let task_wbs: Vec<Option<usize>> = m
            .activities
            .iter()
            .map(|a| {
                p.details
                    .get(&a.task_id)
                    .and_then(|d| pos.get(&d.wbs).copied())
            })
            .collect();
        let mut wbs_tasks = vec![vec![]; nodes.len()];
        for (t, w) in task_wbs.iter().enumerate() {
            if let Some(w) = w {
                wbs_tasks[*w].push(t);
            }
        }
        let assigns = assignments::assignments(p);
        let mut assign_by_task = vec![vec![]; n];
        let mut assign_by_rsrc: HashMap<i64, Vec<usize>> = HashMap::new();
        for (i, a) in assigns.iter().enumerate() {
            if a.task < n {
                assign_by_task[a.task].push(i);
            }
            assign_by_rsrc.entry(a.rsrc).or_default().push(i);
        }
        let resources = resources::resources(p);
        let rsrc_pos: HashMap<i64, usize> = resources
            .iter()
            .enumerate()
            .map(|(i, r)| (r.id, i))
            .collect();
        let mut rsrc_kids = vec![vec![]; resources.len()];
        for (i, r) in resources.iter().enumerate() {
            if let Some(par) = r.parent.and_then(|id| rsrc_pos.get(&id)) {
                if *par != i {
                    rsrc_kids[*par].push(i);
                }
            }
        }
        let mut links_in = vec![vec![]; n];
        let mut links_out = vec![vec![]; n];
        for (i, l) in m.links.iter().enumerate() {
            if l.pred < n && l.succ < n {
                links_in[l.succ].push(i);
                links_out[l.pred].push(i);
            }
        }
        Data {
            p,
            ix,
            catalog: columns::catalog(p),
            wbs: nodes,
            wbs_kids,
            task_wbs,
            wbs_tasks,
            assigns,
            assign_by_task,
            assign_by_rsrc,
            resources,
            rsrc_pos,
            rsrc_kids,
            links_in,
            links_out,
            codes: OnceCell::new(),
            udfs: OnceCell::new(),
            memos: OnceCell::new(),
            rollups: OnceCell::new(),
            now,
            warnings: RefCell::new(BTreeSet::new()),
        }
    }

    pub fn warn(&self, text: String) {
        self.warnings.borrow_mut().insert(text);
    }

    pub fn take_warnings(&self) -> Vec<String> {
        std::mem::take(&mut *self.warnings.borrow_mut())
            .into_iter()
            .collect()
    }

    pub fn origin(&self) -> i64 {
        self.p.model.origin_serial
    }

    pub fn data_date(&self) -> Option<i64> {
        self.p.model.project.data_date
    }

    /// Model minutes of a date and time.
    pub fn minute_of(&self, when: NaiveDateTime) -> i64 {
        (when - epoch()).num_minutes() - self.origin() * DAY
    }

    pub fn when(&self, minute: i64) -> Option<NaiveDateTime> {
        epoch().checked_add_signed(chrono::Duration::minutes(self.origin() * DAY + minute))
    }

    // ------------------------------------------------------------ records

    pub fn records(&self, table: &str) -> Option<Vec<Rec>> {
        Some(match table.to_ascii_uppercase().as_str() {
            "PROJECT" => vec![Rec::Project],
            "PROJWBS" => (0..self.wbs.len()).map(Rec::Wbs).collect(),
            "TASK" => (0..self.p.model.activities.len()).map(Rec::Task).collect(),
            "TASKRSRC" => (0..self.assigns.len()).map(Rec::Assign).collect(),
            "RSRC" => (0..self.resources.len()).map(Rec::Rsrc).collect(),
            "TASKPRED" => (0..self.p.model.links.len()).map(Rec::Link).collect(),
            "TASKACTV" => (0..self.codes().len()).map(Rec::Code).collect(),
            "UDFVALUE" => (0..self.udfs().len()).map(Rec::Udf).collect(),
            "TASKMEMO" => (0..self.memos().len()).map(Rec::Memo).collect(),
            _ => return None,
        })
    }

    fn extras(&self, source: fn(&Source) -> bool) -> Vec<Extra> {
        let mut out = vec![];
        for c in self.catalog.iter().filter(|c| source(&c.source)) {
            for i in 0..self.p.model.activities.len() {
                let v = columns::value(self.p, self.ix, c, i);
                if matches!(v, Value::Empty) {
                    continue;
                }
                let long = match (c.source, crate::format::code_display()) {
                    (Source::Code(type_id), _) => self
                        .ix
                        .code_of(self.p, self.p.model.activities[i].task_id, type_id)
                        .map(|cv| cv.name)
                        .unwrap_or_default(),
                    _ => String::new(),
                };
                out.push(Extra {
                    task: i,
                    a: c.title.clone(),
                    b: columns::display(self.p, Some(i), &v),
                    c: long,
                });
            }
        }
        out
    }

    fn codes(&self) -> &Vec<Extra> {
        self.codes
            .get_or_init(|| self.extras(|s| matches!(s, Source::Code(_))))
    }

    fn udfs(&self) -> &Vec<Extra> {
        self.udfs
            .get_or_init(|| self.extras(|s| matches!(s, Source::Udf(_))))
    }

    fn memos(&self) -> &Vec<Extra> {
        self.memos
            .get_or_init(|| self.extras(|s| matches!(s, Source::Notebook(_))))
    }

    fn rollups(&self) -> &std::collections::BTreeMap<Option<i64>, wbs::Rollup> {
        self.rollups
            .get_or_init(|| wbs::rollups(self.p, &self.ix.wbs(self.p).tree))
    }

    /// Records of table `path` (a table or relationship name) related to
    /// `parent`; `None` when the two have no such relation.
    pub fn children(&self, parent: Rec, path: &str) -> Option<Vec<Rec>> {
        let last = path
            .rsplit(['.', '-'])
            .next()
            .unwrap_or(path)
            .trim_matches(':')
            .to_ascii_uppercase();
        let last = last.as_str();
        let tasks_of = |ts: &[usize]| ts.iter().copied().map(Rec::Task).collect::<Vec<_>>();
        Some(match (parent, last) {
            (Rec::Project, t) => return self.records(t),
            (Rec::Wbs(w), "TASK") => tasks_of(&self.wbs_tasks[w]),
            (Rec::Wbs(w), "PROJWBS") => self.wbs_kids[w].iter().copied().map(Rec::Wbs).collect(),
            (Rec::Wbs(w), "TASKRSRC") => self.wbs_tasks[w]
                .iter()
                .flat_map(|t| self.assign_by_task[*t].iter().copied().map(Rec::Assign))
                .collect(),
            (Rec::Task(t), "TASKRSRC") => self.assign_by_task[t]
                .iter()
                .copied()
                .map(Rec::Assign)
                .collect(),
            (Rec::Task(t), "TASKPREDPREDS" | "TASKPRED") => {
                self.links_in[t].iter().copied().map(Rec::Link).collect()
            }
            (Rec::Task(t), "TASKPREDSUCCS") => {
                self.links_out[t].iter().copied().map(Rec::Link).collect()
            }
            (Rec::Task(t), "PROJWBS") => self.task_wbs[t].map(Rec::Wbs).into_iter().collect(),
            (Rec::Task(t), "RSRC") => self.assign_by_task[t]
                .iter()
                .filter_map(|a| self.rsrc_pos.get(&self.assigns[*a].rsrc))
                .map(|r| Rec::Rsrc(*r))
                .collect(),
            (Rec::Task(t), "TASKACTV") => self.extra_of(self.codes(), t, Rec::Code),
            (Rec::Task(t), "UDFVALUE") => self.extra_of(self.udfs(), t, Rec::Udf),
            (Rec::Task(t), "TASKMEMO") => self.extra_of(self.memos(), t, Rec::Memo),
            (Rec::Task(t), "TASK") => vec![Rec::Task(t)],
            (Rec::Rsrc(r), "TASKRSRC") => self
                .assign_by_rsrc
                .get(&self.resources[r].id)
                .map_or(vec![], |v| v.iter().copied().map(Rec::Assign).collect()),
            (Rec::Rsrc(r), "TASK") => {
                let ids: BTreeSet<usize> = self
                    .assign_by_rsrc
                    .get(&self.resources[r].id)
                    .map_or(vec![], |v| {
                        v.iter().map(|a| self.assigns[*a].task).collect()
                    })
                    .into_iter()
                    .collect();
                ids.into_iter().map(Rec::Task).collect()
            }
            (Rec::Rsrc(r), "RSRC") => self.rsrc_kids[r].iter().copied().map(Rec::Rsrc).collect(),
            (Rec::Assign(a), "TASK") => vec![Rec::Task(self.assigns[a].task)],
            (Rec::Assign(a), "RSRC") => self
                .rsrc_pos
                .get(&self.assigns[a].rsrc)
                .map(|r| Rec::Rsrc(*r))
                .into_iter()
                .collect(),
            (Rec::Assign(a), "PROJWBS") => self.task_wbs[self.assigns[a].task]
                .map(Rec::Wbs)
                .into_iter()
                .collect(),
            (Rec::Link(l), "TASK" | "SUCCTASK") => {
                vec![Rec::Task(self.p.model.links[l].succ)]
            }
            (Rec::Link(l), "PREDTASK") => vec![Rec::Task(self.p.model.links[l].pred)],
            (Rec::Code(i), "TASK") => vec![Rec::Task(self.codes()[i].task)],
            (Rec::Udf(i), "TASK") => vec![Rec::Task(self.udfs()[i].task)],
            (Rec::Memo(i), "TASK") => vec![Rec::Task(self.memos()[i].task)],
            _ => return None,
        })
    }

    fn extra_of(&self, list: &[Extra], task: usize, make: fn(usize) -> Rec) -> Vec<Rec> {
        list.iter()
            .enumerate()
            .filter(|(_, e)| e.task == task)
            .map(|(i, _)| make(i))
            .collect()
    }

    /// Children of a record in its own hierarchy (WBS and resources).
    pub fn tree_children(&self, r: Rec) -> Vec<Rec> {
        match r {
            Rec::Wbs(w) => self.wbs_kids[w].iter().copied().map(Rec::Wbs).collect(),
            Rec::Rsrc(i) => self.rsrc_kids[i].iter().copied().map(Rec::Rsrc).collect(),
            _ => vec![],
        }
    }

    /// Whether a record has no parent in its hierarchy.
    pub fn is_root(&self, r: Rec) -> bool {
        match r {
            Rec::Wbs(w) => self.wbs[w].parent.is_none(),
            Rec::Rsrc(i) => self.resources[i]
                .parent
                .and_then(|id| self.rsrc_pos.get(&id))
                .is_none_or(|par| *par == i),
            _ => true,
        }
    }

    /// The parent of a record in its own hierarchy.
    pub fn parent_of(&self, r: Rec) -> Option<Rec> {
        match r {
            Rec::Wbs(w) => self.wbs[w].parent.map(Rec::Wbs),
            Rec::Rsrc(i) => self.resources[i]
                .parent
                .and_then(|id| self.rsrc_pos.get(&id))
                .filter(|par| **par != i)
                .map(|par| Rec::Rsrc(*par)),
            _ => None,
        }
    }

    /// The shown text of a plain field of a record, or `None` when the
    /// record has no such field (no warning).
    pub fn probe(&self, r: Rec, name: &str) -> Option<String> {
        let v = self.field(r, name.trim())?;
        Some(self.show(r, &v))
    }

    /// Records of table `target` below `from`: a WBS element holds the
    /// activities of its whole subtree.
    pub fn below(&self, from: Rec, target: &str) -> Option<Vec<Rec>> {
        let target = target.to_ascii_uppercase();
        if from.table() == target {
            return Some(vec![from]);
        }
        match from {
            Rec::Wbs(w) => {
                let mut subtree = vec![];
                let mut stack = vec![w];
                while let Some(n) = stack.pop() {
                    subtree.push(n);
                    stack.extend(self.wbs_kids[n].iter().copied());
                }
                match target.as_str() {
                    "TASK" => Some(
                        subtree
                            .iter()
                            .flat_map(|n| self.wbs_tasks[*n].iter().copied().map(Rec::Task))
                            .collect(),
                    ),
                    "TASKRSRC" => Some(
                        subtree
                            .iter()
                            .flat_map(|n| self.wbs_tasks[*n].iter())
                            .flat_map(|t| self.assign_by_task[*t].iter().copied().map(Rec::Assign))
                            .collect(),
                    ),
                    _ => self.children(from, &target),
                }
            }
            _ => self.children(from, &target),
        }
    }

    /// The record reached from `rec` through the tables of a field path.
    pub fn navigate(&self, rec: Rec, tables: &[&str]) -> Option<Rec> {
        let mut at = rec;
        for t in tables {
            if t.is_empty() || t.eq_ignore_ascii_case(at.table()) {
                continue;
            }
            at = self.children(at, t)?.into_iter().next()?;
        }
        Some(at)
    }

    // ------------------------------------------------------------- values

    fn column(&self, id: &str) -> Option<&Column> {
        columns::find(&self.catalog, id)
    }

    /// Shown text of a value.
    pub fn show(&self, rec: Rec, v: &Value) -> String {
        let activity = match rec {
            Rec::Task(i) => Some(i),
            Rec::Assign(a) => Some(self.assigns[a].task),
            Rec::Code(i) => Some(self.codes()[i].task),
            _ => None,
        };
        columns::display(self.p, activity, v)
    }

    /// The value of a field path (`column`, `TABLE|column`,
    /// `TABLE.TABLE|column`) for a record.
    pub fn value(&self, rec: Rec, path: &str) -> Value {
        let (tables, name) = match path.rsplit_once('|') {
            Some((t, n)) => (t.split('.').collect::<Vec<_>>(), n.trim()),
            None => (vec![], path.trim()),
        };
        let Some(target) = self.navigate(rec, &tables) else {
            self.warn(format!(
                "No {} related to {}",
                tables.join("."),
                rec.table()
            ));
            return Value::Empty;
        };
        match self.field(target, name) {
            Some(v) => v,
            None => {
                self.warn(format!("Unknown field {name} on {}", target.table()));
                Value::Empty
            }
        }
    }

    pub fn text(&self, rec: Rec, path: &str) -> String {
        let (tables, _) = match path.rsplit_once('|') {
            Some((t, _)) => (t.split('.').collect::<Vec<_>>(), 0),
            None => (vec![], 0),
        };
        let at = self.navigate(rec, &tables).unwrap_or(rec);
        self.show(at, &self.value(rec, path))
    }

    fn hours(minutes: i64) -> f64 {
        minutes as f64 / 60.
    }

    /// One field of one record; `None` for a name the source does not have.
    fn field(&self, rec: Rec, name: &str) -> Option<Value> {
        let lower = name.to_ascii_lowercase();
        let n = lower.as_str();
        match rec {
            Rec::Project => self.project_field(n),
            Rec::Wbs(w) => self.wbs_field(w, n),
            Rec::Task(t) => self.task_field(t, n),
            Rec::Assign(a) => self.assign_field(a, n),
            Rec::Rsrc(r) => self.rsrc_field(r, n),
            Rec::Link(l) => self.link_field(l, n),
            Rec::Code(i) => {
                let e = &self.codes()[i];
                match n {
                    "actv_code_type" => Some(text(&e.a)),
                    "actv_code_name" | "short_name" => Some(text(&e.b)),
                    "actv_code_descr" => Some(text(if e.c.is_empty() { &e.b } else { &e.c })),
                    _ => None,
                }
            }
            Rec::Udf(i) => {
                let e = &self.udfs()[i];
                match n {
                    "udf_type_label" => Some(text(&e.a)),
                    "udf_value" | "udf_text" => Some(text(&e.b)),
                    _ => None,
                }
            }
            Rec::Memo(i) => {
                let e = &self.memos()[i];
                match n {
                    "memo_type" => Some(text(&e.a)),
                    "task_memo" | "memo" => Some(text(&e.b)),
                    _ => None,
                }
            }
        }
    }

    fn project_field(&self, n: &str) -> Option<Value> {
        let m = &self.p.model;
        let date = |v: Option<i64>| {
            v.map_or(Value::Empty, |minute| Value::Date {
                minute,
                mark: crate::format::DateMark::Plain,
            })
        };
        Some(match n {
            "proj_short_name" | "project_code" => text(&m.project.name),
            "proj_name" | "project_name" => text(&crate::views::project_title(self.p)),
            "data_date" | "last_recalc_date" => date(m.project.data_date),
            "plan_start_date" => date(m.project.plan_start),
            "scd_end_date" | "finish_date" => date(m.project.scd_end),
            "plan_end_date" | "must_finish_date" => date(m.project.plan_end),
            "last_schedule_date" => date(m.project.last_schedule),
            "activity_count" | "task_cnt" => Value::Number(m.activities.len() as f64),
            "proj_id" => Value::Number(m.project.proj_id as f64),
            _ => return None,
        })
    }

    fn wbs_field(&self, w: usize, n: &str) -> Option<Value> {
        let node = &self.wbs[w];
        let roll = self.rollups().get(&Some(node.id));
        let date = |v: Option<i64>| {
            v.map_or(Value::Empty, |minute| Value::Date {
                minute,
                mark: crate::format::DateMark::Plain,
            })
        };
        match n {
            "wbs_id" => return Some(Value::Number(node.id as f64)),
            "wbs_short_name" | "wbs_code" => return Some(text(&node.code)),
            "wbs_name" => return Some(text(&node.name)),
            "wbs_concat_name" | "full_code" => return Some(text(&node.full)),
            "parent_wbs_id" => {
                return Some(
                    node.parent
                        .map_or(Value::Empty, |p| Value::Number(self.wbs[p].id as f64)),
                )
            }
            "seq_num" => return Some(Value::Number(node.seq as f64)),
            "level" => return Some(Value::Number(f64::from(node.level))),
            "proj_node_flag" => return Some(text(if node.parent.is_none() { "Y" } else { "N" })),
            "node_type" => {
                return Some(text(if node.parent.is_none() {
                    "PROJECT"
                } else {
                    "WBS"
                }))
            }
            "task_cnt" | "activity_count" => {
                return Some(Value::Number(roll.map_or(0, |r| r.activities) as f64))
            }
            "start_date" => return Some(date(roll.and_then(|r| r.start))),
            "end_date" => return Some(date(roll.and_then(|r| r.finish))),
            "total_float_hr_cnt" => {
                return Some(
                    roll.and_then(|r| r.total_float)
                        .map_or(Value::Empty, Value::Duration),
                )
            }
            _ => {}
        }
        // Costs and quantities add up over the subtree.
        let assigns = self.below(Rec::Wbs(w), "TASKRSRC")?;
        self.sum_assignments(&assigns, n)
    }

    /// `target_cost`, `act_qty` ... summed over assignments; `None` for any
    /// other name.
    fn sum_assignments(&self, recs: &[Rec], n: &str) -> Option<Value> {
        let pick: fn(&Assignment) -> f64 = match n {
            "target_cost" | "bac" => |a| a.budget_cost,
            "act_cost" => |a| a.actual_cost,
            "remain_cost" => |a| a.remaining_cost,
            "total_cost" => |a| a.actual_cost + a.remaining_cost,
            "target_qty" | "target_work_qty" => |a| a.budget,
            "act_qty" | "act_work_qty" => |a| a.actual,
            "remain_qty" | "remain_work_qty" => |a| a.remaining,
            "total_qty" => |a| a.actual + a.remaining,
            _ => return None,
        };
        let sum: f64 = recs
            .iter()
            .filter_map(|r| match r {
                Rec::Assign(a) => Some(pick(&self.assigns[*a])),
                _ => None,
            })
            .sum();
        Some(if n.ends_with("cost") || n == "bac" {
            Value::Cost(sum)
        } else {
            Value::Units(sum)
        })
    }

    fn task_field(&self, t: usize, n: &str) -> Option<Value> {
        let a = &self.p.model.activities[t];
        let leveled = self.p.model.options.do_level;
        if let Some(v) = self.sum_assignments(&self.children(Rec::Task(t), "TASKRSRC")?, n) {
            return Some(v);
        }
        let mapped = match n {
            "task_code" => "activity_id",
            "task_name" => "activity_name",
            "task_type" => "activity_type",
            "start_date" => "start",
            "end_date" => "finish",
            "early_start_date" => "early_start",
            "early_end_date" => "early_finish",
            "late_start_date" => "late_start",
            "late_end_date" => "late_finish",
            "act_start_date" => "actual_start",
            "act_end_date" => "actual_finish",
            "target_start_date" => "planned_start",
            "target_end_date" => "planned_finish",
            "restart_date" => "remaining_early_start",
            "reend_date" => "remaining_early_finish",
            "target_drtn_hr_cnt" => "original_duration",
            "remain_drtn_hr_cnt" => "remaining_duration",
            "act_drtn_hr_cnt" => "actual_duration",
            "total_float_hr_cnt" => "total_float",
            "free_float_hr_cnt" => "free_float",
            "phys_complete_pct" => "physical_percent_complete",
            "complete_pct" => "activity_percent_complete",
            "drtn_complete_pct" => "duration_percent_complete",
            "calendar_name" | "clndr_name" => "calendar",
            "wbs_code" => "wbs",
            "cstr_type" => "primary_constraint",
            "cstr_date" => "primary_constraint_date",
            "primary_rsrc" | "primary_resource" => "primary_resource",
            "resource_list" => "resources",
            "pred_list" => "predecessors",
            "succ_list" => "successors",
            other => other,
        };
        match n {
            "status_code" => {
                return Some(text(match a.status {
                    StatusType::NotStarted => "TK_NotStart",
                    StatusType::InProgress => "TK_Active",
                    StatusType::Completed => "TK_Complete",
                }))
            }
            "critical_flag" => {
                let c = columns::is_critical(a, self.ix.critical);
                return Some(text(if c { "Y" } else { "N" }));
            }
            "wbs_concat_name" => {
                return Some(text(
                    &self.task_wbs[t]
                        .map(|w| self.wbs[w].full.clone())
                        .unwrap_or_default(),
                ))
            }
            "wbs_id" => {
                return Some(
                    self.task_wbs[t].map_or(Value::Empty, |w| Value::Number(self.wbs[w].id as f64)),
                )
            }
            "task_id" => return Some(Value::Number(a.task_id as f64)),
            "seq_num" => return Some(Value::Number(t as f64)),
            "start_date" => return Some(crate::grid::marks::start_value(a, leveled)),
            "end_date" => return Some(crate::grid::marks::finish_value(a, leveled)),
            "predecessor_count" | "pred_cnt" => {
                return Some(Value::Number(self.links_in[t].len() as f64))
            }
            "successor_count" | "succ_cnt" => {
                return Some(Value::Number(self.links_out[t].len() as f64))
            }
            "float_days" => {
                return Some(match a.total_float {
                    Some(f) => Value::Duration(f),
                    None => Value::Empty,
                })
            }
            _ => {}
        }
        if let Some(c) = self.column(mapped) {
            return Some(columns::value(self.p, self.ix, c, t));
        }
        if let Some(c) = self.column(n) {
            return Some(columns::value(self.p, self.ix, c, t));
        }
        self.raw("TASK", "task_id", a.task_id, n)
    }

    fn assign_field(&self, i: usize, n: &str) -> Option<Value> {
        let a = &self.assigns[i];
        if let Some(v) = self.sum_assignments(&[Rec::Assign(i)], n) {
            return Some(v);
        }
        Some(match n {
            "cost_per_qty" => Value::Cost(a.cost_per_qty),
            "acct_id" => text(&a.account),
            "rate_type" => text(&a.rate_type),
            "role_id" => text(&a.role),
            "taskrsrc_id" => Value::Number(a.id as f64),
            "rsrc_id" => Value::Number(a.rsrc as f64),
            "task_id" => Value::Number(self.p.model.activities[a.task].task_id as f64),
            "target_qty_per_hr" => Value::Units(a.budget_per_hour),
            "remain_qty_per_hr" => Value::Units(a.remaining_per_hour),
            "start_date" | "end_date" => return self.task_field(a.task, n),
            _ => {
                return self.raw_row("TASKRSRC", a.row, n);
            }
        })
    }

    fn rsrc_field(&self, i: usize, n: &str) -> Option<Value> {
        let r = &self.resources[i];
        if let Some(v) = self.sum_assignments(&self.children(Rec::Rsrc(i), "TASKRSRC")?, n) {
            return Some(v);
        }
        Some(match n {
            "rsrc_short_name" => text(&r.code),
            "rsrc_name" => text(&r.name),
            "rsrc_id" => Value::Number(r.id as f64),
            "rsrc_type" => text(r.kind.label()),
            "unit_id" | "unit" => text(&r.unit),
            "def_qty_per_hr" => Value::Units(r.default_units),
            "max_qty_per_hr" => r.max_units.map_or(Value::Empty, Value::Units),
            "cost_per_qty" | "price" => r.price.map_or(Value::Empty, Value::Cost),
            "clndr_name" | "calendar" => text(&r.calendar),
            "active_flag" => text(if r.active { "Y" } else { "N" }),
            "assignment_count" | "assignments" => Value::Number(r.assignments as f64),
            "role_id" => text(&r.role),
            "seq_num" => Value::Number(r.sequence as f64),
            "parent_rsrc_id" => r.parent.map_or(Value::Empty, |p| Value::Number(p as f64)),
            _ => {
                let row = self.p.document.table("RSRC")?.rows.iter().position(|row| {
                    self.p
                        .document
                        .table("RSRC")
                        .is_some_and(|t| num(t.value(row, "rsrc_id")) as i64 == r.id)
                })?;
                return self.raw_row("RSRC", row, n);
            }
        })
    }

    fn link_field(&self, l: usize, n: &str) -> Option<Value> {
        let k = &self.p.model.links[l];
        Some(match n {
            "pred_type" => text(match k.link_type {
                LinkType::FS => "FS",
                LinkType::SS => "SS",
                LinkType::FF => "FF",
                LinkType::SF => "SF",
                LinkType::None => "",
            }),
            "lag_hr_cnt" | "lag" => Value::Duration(k.lag),
            "lag_hours" => Value::Number(Self::hours(k.lag)),
            "driving_flag" => text(if k.driving_path { "Y" } else { "N" }),
            "pred_task_id" => Value::Number(self.p.model.activities[k.pred].task_id as f64),
            "task_id" => Value::Number(self.p.model.activities[k.succ].task_id as f64),
            _ => return None,
        })
    }

    /// A column of the project's own XER table row, typed by its name.
    fn raw(&self, table: &str, key: &str, id: i64, column: &str) -> Option<Value> {
        let t = self.p.document.table(table)?;
        t.field(column)?;
        let row = t
            .rows
            .iter()
            .position(|r| num(t.value(r, key)) as i64 == id)?;
        self.raw_row(table, row, column)
    }

    fn raw_row(&self, table: &str, row: usize, column: &str) -> Option<Value> {
        let t = self.p.document.table(table)?;
        t.field(column)?;
        let s = t.value(t.rows.get(row)?, column).trim();
        if s.is_empty() {
            return Some(Value::Empty);
        }
        let col = column.to_ascii_lowercase();
        Some(if col.ends_with("_date") {
            match openxer_xer::parse_absolute(s) {
                Some(abs) => Value::Date {
                    minute: abs - self.origin() * DAY,
                    mark: if col.starts_with("act_") {
                        crate::format::DateMark::Actual
                    } else {
                        crate::format::DateMark::Plain
                    },
                },
                None => text(s),
            }
        } else if col.ends_with("_cost") {
            s.parse().map_or_else(|_| text(s), Value::Cost)
        } else if col.ends_with("_qty") || col.ends_with("_per_hr") {
            s.parse().map_or_else(|_| text(s), Value::Units)
        } else if col.ends_with("_hr_cnt") {
            s.parse::<f64>()
                .map_or_else(|_| text(s), |h| Value::Duration((h * 60.).round() as i64))
        } else if col.ends_with("_pct") {
            s.parse().map_or_else(|_| text(s), Value::Percent)
        } else {
            text(s)
        })
    }

    // ----------------------------------------------------------- compare

    /// A number for comparing and totals: durations in hours.
    pub fn number(v: &Value) -> Option<f64> {
        match v {
            Value::Duration(m) => Some(Self::hours(*m)),
            other => other.number(),
        }
    }

    /// Orders two values: blanks first, numbers by value, text without case.
    pub fn compare(a: &Value, b: &Value) -> std::cmp::Ordering {
        columns::compare(a, b)
    }

    /// A date expression: `2026-11-05`, `2026-11-05 08:00`, `DD` (the data
    /// date), `CW` (the start of the week of the run), each with an optional
    /// `+n` or `-n` followed by `D` (days) or `W` (weeks).
    pub fn date_literal(&self, s: &str) -> Option<i64> {
        let s = s.trim();
        let upper = s.to_ascii_uppercase();
        let (base, offset) = if let Some(rest) = upper.strip_prefix("DD") {
            (self.data_date()?, rest)
        } else if let Some(rest) = upper.strip_prefix("CW") {
            use chrono::Datelike;
            let today = self.now.date();
            let back = i64::from(today.weekday().num_days_from_sunday());
            let start = today - chrono::Duration::days(back);
            (self.minute_of(start.and_hms_opt(0, 0, 0)?), rest)
        } else if let Some(rest) = upper.strip_prefix("TODAY") {
            (self.minute_of(self.now.date().and_hms_opt(0, 0, 0)?), rest)
        } else {
            return openxer_xer::parse_absolute(s).map(|abs| abs - self.origin() * DAY);
        };
        let offset = offset.trim();
        if offset.is_empty() {
            return Some(base);
        }
        let (sign, digits): (i64, &str) = match offset.chars().next()? {
            '+' => (1, &offset[1..]),
            '-' => (-1, &offset[1..]),
            _ => return None,
        };
        let unit = digits.chars().last()?;
        let n: i64 = digits[..digits.len() - unit.len_utf8()]
            .trim()
            .parse()
            .ok()?;
        let days = match unit {
            'D' => n.checked_mul(1)?,
            'W' => n.checked_mul(7)?,
            _ => return None,
        };
        base.checked_add(sign.checked_mul(days)?.checked_mul(DAY)?)
    }

    // ---------------------------------------------------------- variables

    /// A report variable such as `[project_name]`; `None` if unknown.
    pub fn variable(&self, name: &str) -> Option<String> {
        let m = &self.p.model;
        let date = |v: Option<i64>| crate::format::date(self.origin(), v);
        Some(match name {
            "project_name" => crate::views::project_title(self.p),
            "project_code" | "project_id" => m.project.name.clone(),
            "data_date" => date(m.project.data_date),
            "project_start_date" | "start_date" => date(m.project.plan_start),
            "project_end_date" | "end_date" => date(m.project.scd_end),
            "date" => crate::format::day(self.now.date()),
            "time" => self.now.format("%H:%M").to_string(),
            _ => return None,
        })
    }
}

fn text(s: &str) -> Value {
    if s.is_empty() {
        Value::Empty
    } else {
        Value::Text(s.to_string())
    }
}

/// Words for a field name: `remain_cost` becomes `Remaining Cost`.
pub fn title(field: &str) -> String {
    let name = field
        .rsplit('|')
        .next()
        .unwrap_or(field)
        .to_ascii_lowercase();
    let known = match name.as_str() {
        "task_code" => "Activity ID",
        "task_name" => "Activity Name",
        "task_type" => "Activity Type",
        "status_code" => "Activity Status",
        "start_date" => "Start",
        "end_date" => "Finish",
        "early_start_date" => "Early Start",
        "early_end_date" => "Early Finish",
        "late_start_date" => "Late Start",
        "late_end_date" => "Late Finish",
        "act_start_date" => "Actual Start",
        "act_end_date" => "Actual Finish",
        "target_start_date" => "Planned Start",
        "target_end_date" => "Planned Finish",
        "target_drtn_hr_cnt" => "Original Duration",
        "remain_drtn_hr_cnt" => "Remaining Duration",
        "total_float_hr_cnt" => "Total Float",
        "free_float_hr_cnt" => "Free Float",
        "complete_pct" => "% Complete",
        "wbs_concat_name" => "WBS",
        "wbs_name" => "WBS Name",
        "wbs_short_name" => "WBS Code",
        "rsrc_short_name" => "Resource ID",
        "rsrc_name" => "Resource Name",
        "target_qty" => "Budgeted Units",
        "act_qty" => "Actual Units",
        "remain_qty" => "Remaining Units",
        "total_qty" => "At Completion Units",
        "target_cost" => "Budgeted Cost",
        "act_cost" => "Actual Cost",
        "remain_cost" => "Remaining Cost",
        "total_cost" => "At Completion Cost",
        "pred_type" => "Type",
        "lag_hr_cnt" => "Lag",
        "driving_flag" => "Driving",
        "critical_flag" => "Critical",
        "proj_short_name" => "Project ID",
        "proj_name" => "Project Name",
        "actv_code_type" => "Code Type",
        "actv_code_name" => "Code Value",
        "actv_code_descr" => "Code Description",
        "udf_type_label" => "Field",
        "udf_value" => "Value",
        "memo_type" => "Topic",
        "task_memo" => "Notes",
        _ => "",
    };
    if !known.is_empty() {
        return known.to_string();
    }
    name.split(['_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or(String::new(), |f| {
                f.to_uppercase().collect::<String>() + c.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Fields offered per source for the editor's pickers: (field, title).
pub fn vocabulary(table: &str) -> Vec<(&'static str, String)> {
    let names: &[&str] = match table.to_ascii_uppercase().as_str() {
        "PROJECT" => &[
            "proj_short_name",
            "proj_name",
            "data_date",
            "plan_start_date",
            "scd_end_date",
            "activity_count",
        ],
        "PROJWBS" => &[
            "wbs_short_name",
            "wbs_name",
            "wbs_concat_name",
            "start_date",
            "end_date",
            "task_cnt",
            "target_cost",
            "act_cost",
            "remain_cost",
            "target_qty",
            "act_qty",
            "remain_qty",
        ],
        "TASK" => &[
            "task_code",
            "task_name",
            "task_type",
            "status_code",
            "wbs_concat_name",
            "wbs_name",
            "start_date",
            "end_date",
            "early_start_date",
            "early_end_date",
            "late_start_date",
            "late_end_date",
            "act_start_date",
            "act_end_date",
            "target_start_date",
            "target_end_date",
            "target_drtn_hr_cnt",
            "remain_drtn_hr_cnt",
            "total_float_hr_cnt",
            "free_float_hr_cnt",
            "complete_pct",
            "critical_flag",
            "calendar_name",
            "primary_rsrc",
            "resource_list",
            "target_cost",
            "act_cost",
            "remain_cost",
            "target_qty",
            "act_qty",
            "remain_qty",
            "predecessor_count",
            "successor_count",
        ],
        "TASKRSRC" => &[
            "TASK|task_code",
            "TASK|task_name",
            "RSRC|rsrc_short_name",
            "RSRC|rsrc_name",
            "start_date",
            "end_date",
            "target_qty",
            "act_qty",
            "remain_qty",
            "total_qty",
            "target_cost",
            "act_cost",
            "remain_cost",
            "total_cost",
            "cost_per_qty",
            "acct_id",
        ],
        "RSRC" => &[
            "rsrc_short_name",
            "rsrc_name",
            "rsrc_type",
            "unit_id",
            "def_qty_per_hr",
            "max_qty_per_hr",
            "cost_per_qty",
            "clndr_name",
            "active_flag",
            "assignment_count",
            "target_qty",
            "act_qty",
            "remain_qty",
        ],
        "TASKPRED" => &[
            "PREDTASK|task_code",
            "PREDTASK|task_name",
            "PREDTASK|end_date",
            "TASK|task_code",
            "TASK|task_name",
            "TASK|start_date",
            "pred_type",
            "lag_hr_cnt",
            "driving_flag",
        ],
        "TASKACTV" => &["actv_code_type", "actv_code_name", "actv_code_descr"],
        "UDFVALUE" => &["udf_type_label", "udf_value"],
        "TASKMEMO" => &["memo_type", "task_memo"],
        _ => &[],
    };
    names.iter().map(|n| (*n, title(n))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reports::test_support::scheduled_starter;

    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 5)
            .unwrap()
            .and_hms_opt(8, 30, 0)
            .unwrap()
    }

    #[test]
    fn titles() {
        assert_eq!(title("remain_cost"), "Remaining Cost");
        assert_eq!(title("TASK|task_code"), "Activity ID");
        assert_eq!(title("some_new_field"), "Some New Field");
    }

    #[test]
    fn starter_project_binds_every_source() {
        let p = scheduled_starter();
        let ix = Index::new((false, 0.));
        let d = Data::new(&p, &ix, now());
        for (table, _) in SOURCES {
            let recs = d.records(table).unwrap();
            if recs.is_empty() {
                continue;
            }
            for (field, _) in vocabulary(table) {
                let _ = d.text(recs[0], field);
            }
        }
        assert!(d.take_warnings().is_empty(), "{:?}", d.take_warnings());
        let tasks = d.records("TASK").unwrap();
        assert_eq!(d.text(tasks[0], "task_code"), "RB-1000");
        assert!(!d.text(tasks[0], "wbs_concat_name").is_empty());
        let w = d.records("PROJWBS").unwrap();
        assert!(!w.is_empty());
        // The top WBS elements hold every activity between them.
        let held: usize = w
            .iter()
            .filter(|r| d.is_root(**r))
            .map(|r| d.below(*r, "TASK").unwrap().len())
            .sum();
        assert_eq!(held, p.model.activities.len());
        // An unknown field is an empty cell plus a warning.
        assert_eq!(d.text(tasks[0], "no_such_field"), "");
        assert_eq!(d.take_warnings(), ["Unknown field no_such_field on TASK"]);
    }

    #[test]
    fn navigation_and_relations() {
        let p = scheduled_starter();
        let ix = Index::new((false, 0.));
        let d = Data::new(&p, &ix, now());
        if let Some(a) = d.records("TASKRSRC").unwrap().first() {
            assert!(!d.text(*a, "TASK|task_code").is_empty());
            assert!(!d.text(*a, "RSRC|rsrc_name").is_empty());
        }
        let links = d.records("TASKPRED").unwrap();
        assert!(!links.is_empty());
        let succ = d.text(links[0], "TASK|task_code");
        let pred = d.text(links[0], "PREDTASK|task_code");
        assert_ne!(succ, pred);
        // The relationships of an activity from either side.
        let t = p.model.links[0].succ;
        let preds = d.children(Rec::Task(t), "TASKPREDPREDS").unwrap();
        assert!(preds.contains(&Rec::Link(0)));
    }

    #[test]
    fn date_expressions() {
        let p = scheduled_starter();
        let ix = Index::new((false, 0.));
        let d = Data::new(&p, &ix, now());
        let dd = d.data_date().unwrap();
        assert_eq!(d.date_literal("DD"), Some(dd));
        assert_eq!(d.date_literal("DD+2W"), Some(dd + 14 * DAY));
        assert_eq!(d.date_literal("dd-3d"), Some(dd - 3 * DAY));
        assert_eq!(d.date_literal("DD+x"), None);
        assert!(d.date_literal("2026-11-05").is_some());
        assert!(d.date_literal("CW").unwrap() <= d.date_literal("TODAY").unwrap());
        assert_eq!(d.date_literal("nonsense"), None);
        // Non-ASCII text and huge numbers are refused, not a panic.
        for bad in [
            "DD+2\u{e9}",
            "DD+\u{4e2d}",
            "DD+\u{1f600}",
            "DD-\u{e9}W",
            "DD+",
            "DD+W",
            "CW+99999999999999999999D",
            "DD+9223372036854775807W",
            "\u{e9}",
            "DD\u{e9}",
        ] {
            let _ = d.date_literal(bad);
        }
        assert_eq!(d.date_literal("DD+9223372036854775807W"), None);
    }
}
