//! Data operations behind the Edit commands: activity ID numbering, copying
//! activities with their detail rows, removing and dissolving activities,
//! renumbering, and the tab separated text used with spreadsheets. Everything
//! here works on a `ProjectData`; `mod.rs` adds selection, dialogs and undo.
use crate::records::{self, num};
use openxer_engine::model::{Activity, Link, LinkType};
use openxer_xer::{ActivityDetails, ProjectData};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ------------------------------------------------------------- numbering

// The auto-numbering Add, Paste and Renumber share lives in the edit layer.
pub use openxer_xer::edit::new_activity::{compose, numbering, split_id, Ids};

// ------------------------------------------------------------ copy/paste

/// What Paste copies besides the activities themselves (Copy Activity
/// Options). User defined fields and notes to resources always come along.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CopyOptions {
    pub relationships: bool,
    pub only_between: bool,
    pub assignments: bool,
    pub codes: bool,
    pub notebooks: bool,
    pub steps: bool,
    pub expenses: bool,
    pub documents: bool,
    pub risks: bool,
    /// False after "Skip this dialog next time".
    pub show_dialog: bool,
}

impl Default for CopyOptions {
    fn default() -> Self {
        Self {
            relationships: true,
            only_between: true,
            assignments: true,
            codes: true,
            notebooks: true,
            steps: true,
            expenses: true,
            documents: true,
            risks: true,
            show_dialog: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Always,
    Assignments,
    Codes,
    Notebooks,
    Steps,
    Expenses,
    Documents,
    Risks,
}

/// A table whose rows belong to an activity.
struct Child {
    table: &'static str,
    /// Its own key, renumbered for copies.
    id: Option<&'static str>,
    /// The column holding the activity's `task_id`.
    key: &'static str,
    part: Part,
    /// (column, dictionary table, dictionary key): a copy into another
    /// project keeps the row only when the value it refers to exists there.
    refs: &'static [(&'static str, &'static str, &'static str)],
}

const CHILDREN: &[Child] = &[
    Child {
        table: "TASKRSRC",
        id: Some("taskrsrc_id"),
        key: "task_id",
        part: Part::Assignments,
        refs: &[
            ("rsrc_id", "RSRC", "rsrc_id"),
            ("role_id", "ROLES", "role_id"),
        ],
    },
    Child {
        table: "TASKACTV",
        id: None,
        key: "task_id",
        part: Part::Codes,
        refs: &[
            ("actv_code_type_id", "ACTVTYPE", "actv_code_type_id"),
            ("actv_code_id", "ACTVCODE", "actv_code_id"),
        ],
    },
    Child {
        table: "TASKMEMO",
        id: Some("memo_id"),
        key: "task_id",
        part: Part::Notebooks,
        refs: &[("memo_type_id", "MEMOTYPE", "memo_type_id")],
    },
    Child {
        table: "TASKPROC",
        id: Some("proc_id"),
        key: "task_id",
        part: Part::Steps,
        refs: &[],
    },
    Child {
        table: "PROJCOST",
        id: Some("cost_item_id"),
        key: "task_id",
        part: Part::Expenses,
        refs: &[
            ("acct_id", "ACCOUNT", "acct_id"),
            ("cost_type_id", "COSTTYPE", "cost_type_id"),
        ],
    },
    Child {
        table: "TASKDOC",
        id: Some("taskdoc_id"),
        key: "task_id",
        part: Part::Documents,
        refs: &[("doc_id", "DOCUMENT", "doc_id")],
    },
    Child {
        table: "TASKRISK",
        id: None,
        key: "task_id",
        part: Part::Risks,
        refs: &[("risk_id", "RISK", "risk_id")],
    },
    Child {
        table: "TASKNOTE",
        id: None,
        key: "task_id",
        part: Part::Always,
        refs: &[],
    },
    Child {
        table: "UDFVALUE",
        id: None,
        key: "fk_id",
        part: Part::Always,
        refs: &[("udf_type_id", "UDFTYPE", "udf_type_id")],
    },
];

impl CopyOptions {
    fn includes(&self, part: Part) -> bool {
        match part {
            Part::Always => true,
            Part::Assignments => self.assignments,
            Part::Codes => self.codes,
            Part::Notebooks => self.notebooks,
            Part::Steps => self.steps,
            Part::Expenses => self.expenses,
            Part::Documents => self.documents,
            Part::Risks => self.risks,
        }
    }
}

type Row = Vec<(String, String)>;

/// One copied activity with its rows, as they were when copied.
#[derive(Clone, Debug)]
pub struct CopiedActivity {
    pub activity: Activity,
    pub details: ActivityDetails,
    /// `clndr_id` of its calendar.
    pub calendar: String,
    /// Its TASK row (empty for an activity not yet exported).
    pub task_row: Row,
    /// Rows of the detail tables, by table.
    pub children: Vec<(&'static str, Row)>,
}

/// A relationship end: a copied activity (by position) or another activity
/// of the source project (by `task_id`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum End {
    Copied(usize),
    Other(i64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CopiedLink {
    pub pred: End,
    pub succ: End,
    pub link_type: LinkType,
    pub lag: i64,
}

/// Activities copied or cut, ready to paste into this or another project.
#[derive(Clone, Debug, Default)]
pub struct Clipboard {
    /// The source project (see `project_key`).
    pub source: String,
    pub activities: Vec<CopiedActivity>,
    pub links: Vec<CopiedLink>,
    /// The text put on the system clipboard with the copy.
    pub text: String,
    pub cut: bool,
}

/// Identifies a project across the workspace: its id plus GUID or name.
pub fn project_key(p: &ProjectData) -> String {
    let pid = p.model.project.proj_id;
    let guid = p
        .document
        .table("PROJECT")
        .and_then(|t| {
            t.rows
                .iter()
                .find(|r| num(t.value(r, "proj_id")) == pid)
                .map(|r| t.value(r, "guid").to_string())
        })
        .unwrap_or_default();
    format!("{pid}|{guid}|{}", p.model.project.name)
}

fn task_udf_types(p: &ProjectData) -> HashSet<i64> {
    p.document
        .table("UDFTYPE")
        .map(|t| {
            t.rows
                .iter()
                .filter(|r| t.value(r, "table_name").eq_ignore_ascii_case("TASK"))
                .map(|r| num(t.value(r, "udf_type_id")))
                .collect()
        })
        .unwrap_or_default()
}

fn pairs(fields: &[String], row: &[String]) -> Row {
    fields.iter().cloned().zip(row.iter().cloned()).collect()
}

/// Copy activities (model indices, in paste order) with every detail row
/// and the relationships that touch them.
pub fn copy(p: &ProjectData, indices: &[usize]) -> Clipboard {
    let pid = p.model.project.proj_id;
    let doc = &p.document;
    let udfs = task_udf_types(p);
    let position: HashMap<usize, usize> =
        indices.iter().enumerate().map(|(k, &i)| (i, k)).collect();
    let activities = indices
        .iter()
        .map(|&i| {
            let a = &p.model.activities[i];
            let task_row = doc
                .table("TASK")
                .and_then(|t| {
                    t.rows
                        .iter()
                        .find(|r| {
                            num(t.value(r, "task_id")) == a.task_id
                                && num(t.value(r, "proj_id")) == pid
                        })
                        .map(|r| pairs(&t.fields, r))
                })
                .unwrap_or_default();
            let mut children = vec![];
            for c in CHILDREN {
                let Some(t) = doc.table(c.table) else {
                    continue;
                };
                let has_project = t.field("proj_id").is_some();
                for r in &t.rows {
                    if num(t.value(r, c.key)) != a.task_id
                        || (has_project && num(t.value(r, "proj_id")) != pid)
                        || (c.table == "UDFVALUE"
                            && !udfs.contains(&num(t.value(r, "udf_type_id"))))
                    {
                        continue;
                    }
                    children.push((c.table, pairs(&t.fields, r)));
                }
            }
            CopiedActivity {
                activity: a.clone(),
                details: p.details.get(&a.task_id).cloned().unwrap_or_default(),
                calendar: p
                    .model
                    .calendars
                    .get(a.cal)
                    .map(|c| c.id.clone())
                    .unwrap_or_default(),
                task_row,
                children,
            }
        })
        .collect();
    let links = p
        .model
        .links
        .iter()
        .filter_map(|l| {
            let end = |i: usize| {
                position
                    .get(&i)
                    .map_or(End::Other(p.model.activities[i].task_id), |k| {
                        End::Copied(*k)
                    })
            };
            let (pred, succ) = (end(l.pred), end(l.succ));
            (matches!(pred, End::Copied(_)) || matches!(succ, End::Copied(_))).then_some(
                CopiedLink {
                    pred,
                    succ,
                    link_type: l.link_type,
                    lag: l.lag,
                },
            )
        })
        .collect();
    Clipboard {
        source: project_key(p),
        activities,
        links,
        text: String::new(),
        cut: false,
    }
}

/// Is the row's value for `column` present in the target project's
/// dictionary? Blank references and absent columns always pass. A dictionary
/// row that belongs to another project (a project code type, risk or
/// document) does not count.
fn reference_exists(p: &ProjectData, row: &Row, column: &str, table: &str, key: &str) -> bool {
    let Some(value) = row
        .iter()
        .find(|(f, _)| f.eq_ignore_ascii_case(column))
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty())
    else {
        return true;
    };
    let pid = p.model.project.proj_id;
    p.document.table(table).is_some_and(|t| {
        let has_project = t.field("proj_id").is_some();
        t.rows.iter().any(|r| {
            t.value(r, key).trim() == value && {
                let owner = t.value(r, "proj_id").trim();
                !has_project || owner.is_empty() || num(owner) == pid
            }
        })
    })
}

/// Highest `task_id` in the model or anywhere in the file (activity IDs are
/// unique across a database, so other projects count too).
fn highest_task_id(p: &ProjectData) -> i64 {
    let in_file = p.document.table("TASK").map_or(0, |t| {
        t.rows
            .iter()
            .map(|r| num(t.value(r, "task_id")))
            .max()
            .unwrap_or(0)
    });
    p.model
        .activities
        .iter()
        .map(|a| a.task_id)
        .max()
        .unwrap_or(0)
        .max(in_file)
}

/// Is `task_id` unused by this project's activities and the file's TASK rows?
fn task_id_free(p: &ProjectData, task_id: i64) -> bool {
    !p.model.activities.iter().any(|a| a.task_id == task_id)
        && p.document
            .table("TASK")
            .is_none_or(|t| !t.rows.iter().any(|r| num(t.value(r, "task_id")) == task_id))
}

/// TASK columns a pasted row must not inherit. A blank `task_type` makes the
/// export fill the row as a new activity: every owned column, and the GUID
/// and stamps minted into the pasted activity's details.
const TASK_RESET: [&str; 7] = [
    "task_type",
    "guid",
    "tmpl_guid",
    "create_date",
    "update_date",
    "create_user",
    "update_user",
];

/// What a paste made, and what it left out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pasted {
    /// The new model indices.
    pub rows: Vec<usize>,
    /// Pasted milestones whose copies carried resource assignments, with
    /// how many were left out: milestones cannot have resource or role
    /// assignments (the shared milestone rule).
    pub milestone_assignments: Vec<(String, usize)>,
}

impl Pasted {
    /// The sentence telling the user which assignment rows were not pasted
    /// and why, if any were left out.
    pub fn note(&self) -> Option<String> {
        if self.milestone_assignments.is_empty() {
            return None;
        }
        let total: usize = self.milestone_assignments.iter().map(|(_, n)| n).sum();
        let codes: Vec<&str> = self
            .milestone_assignments
            .iter()
            .map(|(c, _)| c.as_str())
            .collect();
        Some(format!(
            "{total} resource assignment{} not pasted because {} ({}).",
            if total == 1 { " was" } else { "s were" },
            openxer_xer::assignments::MILESTONE_RULE,
            codes.join(", ")
        ))
    }
}

/// Paste copied activities into WBS `wbs`. A copied Activity ID is kept when
/// it is free in this project; otherwise the project's auto-numbering gives
/// a new one (from `anchor`, the selected activity, when the project numbers
/// from the selected activity). Returns the new model indices.
#[cfg(test)]
pub fn paste(
    p: &mut ProjectData,
    clip: &Clipboard,
    o: &CopyOptions,
    wbs: i64,
    anchor: Option<&str>,
) -> Result<Vec<usize>, String> {
    paste_activities(p, clip, o, wbs, anchor).map(|pasted| pasted.rows)
}

/// [`paste`], with what it left out.
pub fn paste_activities(
    p: &mut ProjectData,
    clip: &Clipboard,
    o: &CopyOptions,
    wbs: i64,
    anchor: Option<&str>,
) -> Result<Pasted, String> {
    paste_with(p, clip, o, wbs, anchor, None, false)
}

/// [`paste_activities`] with the Activity IDs the Renumber Activity IDs step
/// chose, one per copied activity in clipboard order. The caller has checked
/// they are free (`paste_conflicts`).
pub fn paste_renumbered(
    p: &mut ProjectData,
    clip: &Clipboard,
    o: &CopyOptions,
    wbs: i64,
    codes: &[String],
) -> Result<Pasted, String> {
    if codes.len() != clip.activities.len() {
        return Err("Every pasted activity needs an Activity ID.".into());
    }
    paste_with(p, clip, o, wbs, None, Some(codes), false)
}

/// The first half of moving cut activities into another project: every
/// part of them pasted into WBS `wbs`, with their Activity IDs (the caller
/// has checked they are free) and their internal keys where this project
/// does not use them, and the relationships among them.
pub fn paste_moved(p: &mut ProjectData, clip: &Clipboard, wbs: i64) -> Result<Pasted, String> {
    let all = CopyOptions {
        relationships: true,
        only_between: true,
        ..CopyOptions::default()
    };
    paste_with(p, clip, &all, wbs, None, None, true)
}

fn paste_with(
    p: &mut ProjectData,
    clip: &Clipboard,
    o: &CopyOptions,
    wbs: i64,
    anchor: Option<&str>,
    codes: Option<&[String]>,
    keep_keys: bool,
) -> Result<Pasted, String> {
    if clip.activities.is_empty() {
        return Err("There are no copied activities to paste.".into());
    }
    // As Add does: a project without calendars gets the standard calendar
    // first, so every pasted activity has one to work on and to save.
    p.ensure_a_calendar();
    let same = clip.source == project_key(p);
    let mut milestone_assignments: Vec<(String, usize)> = vec![];
    let pid = p.model.project.proj_id;
    let n = numbering(p);
    let mut ids = Ids::new(p);
    let mut anchor = anchor.map(str::to_string);
    let mut next_task = highest_task_id(p) + 1;
    let mut next_row: HashMap<&'static str, i64> = HashMap::new();
    let first = p.model.activities.len();
    let target_calendar = |p: &ProjectData, id: &str| {
        p.model
            .calendars
            .iter()
            .position(|c| c.id == id)
            .unwrap_or(p.model.project_calendar)
    };
    for (k, c) in clip.activities.iter().enumerate() {
        let code = if let Some(code) = codes.and_then(|codes| codes.get(k)) {
            code.clone()
        } else if ids.take(&c.activity.task_code) {
            c.activity.task_code.clone()
        } else if let (true, Some(from)) = (n.from_selected, anchor.as_deref()) {
            let code = ids.after(from, n.step);
            anchor = Some(code.clone());
            code
        } else {
            ids.auto(&n)
        };
        let own = c.activity.task_id;
        let task_id = if keep_keys && task_id_free(p, own) {
            next_task = next_task.max(own + 1);
            own
        } else {
            next_task += 1;
            next_task - 1
        };
        let cal = target_calendar(p, &c.calendar);
        let calendar_id = p.model.calendars[cal].id.clone();
        let mut a = c.activity.clone();
        a.task_id = task_id;
        a.task_code = code.clone();
        a.cal = cal;
        a.driving_path = false;
        p.model.activities.push(a);
        let mut d = c.details.clone();
        d.wbs = wbs;
        // A pasted activity is a new one: its own GUID and create date, once.
        d.minted = Some(p.mint(task_id));
        p.details.insert(task_id, d);
        if !c.task_row.is_empty() {
            let rsrc_ok = same || reference_exists(p, &c.task_row, "rsrc_id", "RSRC", "rsrc_id");
            let values: Vec<(String, String)> = c
                .task_row
                .iter()
                .map(|(f, v)| {
                    let f_lower = f.to_ascii_lowercase();
                    let v = match f_lower.as_str() {
                        "task_id" => task_id.to_string(),
                        "proj_id" => pid.to_string(),
                        "wbs_id" => wbs.to_string(),
                        "task_code" => code.clone(),
                        "clndr_id" => calendar_id.clone(),
                        "rsrc_id" if !rsrc_ok => String::new(),
                        k if TASK_RESET.contains(&k) => String::new(),
                        _ => v.clone(),
                    };
                    (f.clone(), v)
                })
                .filter(|(_, v)| !v.is_empty())
                .collect();
            let borrowed: Vec<(&str, String)> = values
                .iter()
                .map(|(f, v)| (f.as_str(), v.clone()))
                .collect();
            p.document.table_mut("TASK").add(&borrowed);
        }
        for (table, row) in &c.children {
            let Some(spec) = CHILDREN.iter().find(|s| s.table == *table) else {
                continue;
            };
            if !o.includes(spec.part) {
                continue;
            }
            // Milestones cannot have resource or role assignments of any
            // resource type (the shared milestone rule); a copy of an
            // imported one that has them is pasted without them, and the
            // status message says so.
            if spec.table == "TASKRSRC"
                && openxer_xer::assignments::is_milestone(c.activity.task_type)
            {
                match milestone_assignments.last_mut() {
                    Some((last, n)) if *last == code => *n += 1,
                    _ => milestone_assignments.push((code.clone(), 1)),
                }
                continue;
            }
            if !same
                && !spec
                    .refs
                    .iter()
                    .all(|(col, t, k)| reference_exists(p, row, col, t, k))
            {
                continue;
            }
            let own_id = spec.id.map(|id| {
                let next = next_row
                    .entry(spec.table)
                    .or_insert_with(|| records::next_id(&p.document, spec.table, id));
                let value = *next;
                *next += 1;
                value
            });
            let values: Vec<(String, String)> = row
                .iter()
                .map(|(f, v)| {
                    let f_lower = f.to_ascii_lowercase();
                    let v = if Some(f_lower.as_str()) == spec.id {
                        own_id.map(|x| x.to_string()).unwrap_or_default()
                    } else if f_lower == spec.key {
                        task_id.to_string()
                    } else {
                        match f_lower.as_str() {
                            "proj_id" => pid.to_string(),
                            "wbs_id" if !v.is_empty() => wbs.to_string(),
                            "guid" if !v.is_empty() => {
                                openxer_xer::new_guid(&format!("{table}/{task_id}/{v}"))
                            }
                            _ => v.clone(),
                        }
                    };
                    (f.clone(), v)
                })
                .filter(|(_, v)| !v.is_empty())
                .collect();
            let borrowed: Vec<(&str, String)> = values
                .iter()
                .map(|(f, v)| (f.as_str(), v.clone()))
                .collect();
            records::ensure_table(&mut p.document, table).add(&borrowed);
        }
    }
    if o.relationships {
        let mut by_task: HashMap<i64, usize> = HashMap::new();
        if same && !o.only_between {
            by_task = p
                .model
                .activities
                .iter()
                .enumerate()
                .take(first)
                .map(|(i, a)| (a.task_id, i))
                .collect();
        }
        for l in &clip.links {
            let resolve = |e: End| match e {
                End::Copied(k) => Some(first + k),
                End::Other(id) => by_task.get(&id).copied(),
            };
            let (Some(pred), Some(succ)) = (resolve(l.pred), resolve(l.succ)) else {
                continue;
            };
            let exists = p
                .model
                .links
                .iter()
                .any(|x| x.pred == pred && x.succ == succ && x.link_type == l.link_type);
            if exists || crate::relationships::can_connect(&p.model, pred, succ).is_err() {
                continue;
            }
            p.model.links.push(Link {
                pred,
                succ,
                link_type: l.link_type,
                lag: l.lag,
                driving_path: false,
            });
        }
        p.model.rebuild_links()?;
    }
    // Copies of activities that had no dates yet get the dates of an
    // activity without logic, as Add gives them.
    let undated: Vec<usize> = (first..p.model.activities.len())
        .filter(|&i| p.model.activities[i].early_start.is_none())
        .collect();
    p.date_unscheduled(&undated);
    Ok(Pasted {
        rows: (first..p.model.activities.len()).collect(),
        milestone_assignments,
    })
}

// --------------------------------------------------------------- dissolve

/// Dissolve lives in `openxer_xer::edit::dissolve`, shared with the command
/// line and MCP.
pub use openxer_xer::edit::dissolve::{can_dissolve, dissolve, Dissolved};

// --------------------------------------------------------------- renumber

/// The three Renumber Activity IDs choices.
#[derive(Clone, Debug, PartialEq)]
pub enum Renumber {
    /// Keep the first selected ID; each next one is its number plus the step.
    Increment { step: i64 },
    /// Prefix plus suffix, then suffix plus the step for each next one.
    Auto {
        prefix: String,
        suffix: String,
        step: i64,
    },
    /// Replace the first `count` characters with `with`.
    Replace { count: usize, with: String },
}

/// New IDs for `codes` (in table order).
pub fn renumber_ids(codes: &[String], how: &Renumber) -> Result<Vec<String>, String> {
    let out: Vec<String> = match how {
        Renumber::Increment { step } => {
            if *step <= 0 {
                return Err("The increment value must be a positive whole number.".into());
            }
            let Some(first) = codes.first() else {
                return Ok(vec![]);
            };
            let (prefix, number, width) = split_id(first);
            let start = number.unwrap_or(0);
            (0..codes.len() as i64)
                .map(|k| {
                    if k == 0 {
                        first.clone()
                    } else {
                        compose(prefix, start.saturating_add(k.saturating_mul(*step)), width)
                    }
                })
                .collect()
        }
        Renumber::Auto {
            prefix,
            suffix,
            step,
        } => {
            if *step <= 0 {
                return Err("The increment value must be a positive whole number.".into());
            }
            let suffix = suffix.trim();
            if suffix.len() > 18 || !suffix.bytes().all(|b| b.is_ascii_digit()) {
                return Err("The suffix must be a whole number.".into());
            }
            let start = suffix.parse::<i64>().unwrap_or(0);
            (0..codes.len() as i64)
                .map(|k| {
                    compose(
                        prefix.trim(),
                        start.saturating_add(k.saturating_mul(*step)),
                        suffix.len(),
                    )
                })
                .collect()
        }
        Renumber::Replace { count, with } => codes
            .iter()
            .map(|c| {
                format!(
                    "{}{}",
                    with.trim(),
                    c.chars().skip(*count).collect::<String>()
                )
            })
            .collect(),
    };
    if out.iter().any(|c| c.trim().is_empty()) {
        return Err("Every activity needs a nonempty Activity ID.".into());
    }
    Ok(out.into_iter().map(|c| records::clean(&c)).collect())
}

/// Activity IDs for pasting copies of `codes` (clipboard order) by the
/// Renumber Activity IDs choice. `existing` holds the IDs already used in
/// the target project. Increment counts up from the first copied ID by the
/// increment; where that ID is in use the copy takes the highest number in
/// use with the same prefix plus the increment, as observed in the
/// reference (free numbers in between are not used), so it never clashes.
/// Auto-number counts up from prefix and suffix by the increment and passes
/// over IDs in use. Replacing the beginning characters works on each copied
/// ID and may give IDs in use (the Duplicates step settles them).
pub fn paste_ids(
    codes: &[String],
    how: &Renumber,
    existing: &HashSet<String>,
) -> Result<Vec<String>, String> {
    let taken = |code: &str| existing.contains(code);
    let counting = |prefix: &str, start: i64, width: usize, step: i64| {
        let mut used: HashSet<String> = HashSet::new();
        let mut next = start;
        let mut out = vec![];
        for _ in codes {
            let mut tries = 0;
            loop {
                let code = records::clean(&compose(prefix, next, width));
                next = next.saturating_add(step);
                tries += 1;
                if (!taken(&code) && used.insert(code.clone())) || tries > 1_000_000 {
                    out.push(code);
                    break;
                }
            }
        }
        out
    };
    match how {
        Renumber::Increment { step } => {
            if *step <= 0 {
                return Err("The increment value must be a positive whole number.".into());
            }
            let Some(first) = codes.first() else {
                return Ok(vec![]);
            };
            let (prefix, number, width) = split_id(first);
            // The highest number in use with this prefix (copies included).
            let mut highest = existing
                .iter()
                .filter_map(|c| match split_id(c) {
                    (p, Some(n), _) if p == prefix => Some(n),
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            let mut used: HashSet<String> = HashSet::new();
            let mut next = number.unwrap_or(0).saturating_add(*step);
            let mut out = vec![];
            for _ in codes {
                let mut code = records::clean(&compose(prefix, next, width));
                let mut tries = 0;
                while (taken(&code) || used.contains(&code)) && tries < 1_000_000 {
                    next = highest.max(next).saturating_add(*step);
                    code = records::clean(&compose(prefix, next, width));
                    tries += 1;
                }
                highest = highest.max(next);
                used.insert(code.clone());
                out.push(code);
                next = next.saturating_add(*step);
            }
            Ok(out)
        }
        Renumber::Auto {
            prefix,
            suffix,
            step,
        } => {
            // The checks of the Renumber command.
            renumber_ids(&codes[..codes.len().min(1)], how)?;
            let suffix = suffix.trim();
            let start = suffix.parse::<i64>().unwrap_or(0);
            Ok(counting(prefix.trim(), start, suffix.len(), *step))
        }
        Renumber::Replace { .. } => renumber_ids(codes, how),
    }
}

/// Positions in `targets` (new Activity IDs for pasted activities) that are
/// blank, already used in the project or given twice.
pub fn paste_conflicts(p: &ProjectData, targets: &[String]) -> Vec<usize> {
    let used: HashSet<&str> = p
        .model
        .activities
        .iter()
        .map(|a| a.task_code.as_str())
        .collect();
    let mut count: HashMap<&str, usize> = HashMap::new();
    for code in targets {
        *count.entry(code.as_str()).or_default() += 1;
    }
    targets
        .iter()
        .enumerate()
        .filter(|(_, code)| {
            code.trim().is_empty() || used.contains(code.as_str()) || count[code.as_str()] > 1
        })
        .map(|(k, _)| k)
        .collect()
}

/// Positions in `targets` whose new ID collides with an activity that keeps
/// its ID or with another new ID.
pub fn renumber_conflicts(p: &ProjectData, targets: &[(usize, String)]) -> Vec<usize> {
    let moving: HashSet<usize> = targets.iter().map(|(i, _)| *i).collect();
    let fixed: HashSet<&str> = p
        .model
        .activities
        .iter()
        .enumerate()
        .filter(|(i, _)| !moving.contains(i))
        .map(|(_, a)| a.task_code.as_str())
        .collect();
    let mut count: HashMap<&str, usize> = HashMap::new();
    for (_, code) in targets {
        *count.entry(code.as_str()).or_default() += 1;
    }
    targets
        .iter()
        .enumerate()
        .filter(|(_, (_, code))| {
            code.trim().is_empty() || fixed.contains(code.as_str()) || count[code.as_str()] > 1
        })
        .map(|(k, _)| k)
        .collect()
}

// -------------------------------------------------------------------- text

fn quote(cell: &str) -> String {
    if cell.contains(['\t', '\n', '\r', '"']) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_string()
    }
}

/// Tab separated text as spreadsheets put on the clipboard: CR LF line ends,
/// cells holding tabs, line breaks or quotes are quoted.
pub fn to_tsv(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    for row in rows {
        let cells: Vec<String> = row.iter().map(|c| quote(c)).collect();
        out.push_str(&cells.join("\t"));
        out.push_str("\r\n");
    }
    out
}

/// Read tab separated clipboard text into rows of cells (quoted cells may
/// hold tabs and line breaks; a final line break ends the last row).
pub fn parse_tsv(text: &str) -> Vec<Vec<String>> {
    let mut rows = vec![];
    let mut row: Vec<String> = vec![];
    let mut cell = String::new();
    let mut quoted = false;
    let mut at_start = true;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    quoted = false;
                }
            } else {
                cell.push(c);
            }
            continue;
        }
        match c {
            '"' if at_start => {
                quoted = true;
                at_start = false;
            }
            '\t' => {
                row.push(std::mem::take(&mut cell));
                at_start = true;
            }
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
                at_start = true;
            }
            _ => {
                cell.push(c);
                at_start = false;
            }
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}

/// Compare clipboard texts ignoring CR characters and trailing line breaks.
pub fn same_text(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace('\r', "").trim_end_matches('\n').to_string();
    norm(a) == norm(b)
}
