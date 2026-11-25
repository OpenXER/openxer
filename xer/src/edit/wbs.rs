//! WBS structure edits shared by the desktop's WBS view and the command
//! line: the tree of `ProjectData::wbs`, adding, renaming and shifting
//! elements. Every edit also rewrites the matching PROJWBS rows so the change
//! survives save, reopen and XER export. Sequence numbers are renumbered in
//! tree pre-order after each structural edit.
use crate::{ProjectData, Wbs};
use std::collections::{BTreeMap, BTreeSet};

pub fn num(s: &str) -> i64 {
    s.trim().parse().unwrap_or(0)
}

/// The PROJWBS row flagged as the project node, if the XER carries one.
pub fn project_node(p: &ProjectData) -> Option<i64> {
    let pid = p.model.project.proj_id;
    let table = p.document.table("PROJWBS")?;
    table
        .rows
        .iter()
        .find(|r| num(table.value(r, "proj_id")) == pid && table.value(r, "proj_node_flag") == "Y")
        .map(|r| num(table.value(r, "wbs_id")))
        .filter(|id| p.wbs.iter().any(|w| w.id == *id))
}

/// Parent/child lists in display order. `None` holds the top-level nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct Tree {
    pub children: BTreeMap<Option<i64>, Vec<i64>>,
    pub parent: BTreeMap<i64, Option<i64>>,
}

impl Tree {
    pub fn build(p: &ProjectData) -> Self {
        let ids: BTreeSet<i64> = p.wbs.iter().map(|w| w.id).collect();
        let mut nodes: Vec<&Wbs> = p.wbs.iter().collect();
        nodes.sort_by_key(|w| (w.sequence, w.id));
        let mut tree = Self {
            children: BTreeMap::new(),
            parent: BTreeMap::new(),
        };
        for w in nodes {
            let parent = w.parent.filter(|id| ids.contains(id) && *id != w.id);
            tree.parent.insert(w.id, parent);
            tree.children.entry(parent).or_default().push(w.id);
        }
        // A corrupt parent cycle would hide nodes; lift any unreachable node to the top.
        let reachable: BTreeSet<i64> = tree.preorder().into_iter().map(|(id, _)| id).collect();
        let orphans: Vec<i64> = tree
            .parent
            .keys()
            .copied()
            .filter(|id| !reachable.contains(id))
            .collect();
        for id in orphans {
            tree.detach(id);
            tree.parent.insert(id, None);
            tree.children.entry(None).or_default().push(id);
        }
        tree
    }
    pub fn kids(&self, parent: Option<i64>) -> &[i64] {
        self.children.get(&parent).map_or(&[], Vec::as_slice)
    }
    pub fn preorder(&self) -> Vec<(i64, usize)> {
        let mut out = vec![];
        let mut stack: Vec<(i64, usize)> =
            self.kids(None).iter().rev().map(|&id| (id, 0)).collect();
        let mut seen = BTreeSet::new();
        while let Some((id, depth)) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            out.push((id, depth));
            for &child in self.kids(Some(id)).iter().rev() {
                stack.push((child, depth + 1));
            }
        }
        out
    }
    pub fn descendants(&self, id: i64) -> Vec<i64> {
        let mut out = vec![id];
        let mut i = 0;
        while i < out.len() {
            out.extend_from_slice(self.kids(Some(out[i])));
            i += 1;
        }
        out
    }
    pub fn detach(&mut self, id: i64) -> Option<(Option<i64>, usize)> {
        let parent = *self.parent.get(&id)?;
        let list = self.children.get_mut(&parent)?;
        let pos = list.iter().position(|&x| x == id)?;
        list.remove(pos);
        Some((parent, pos))
    }
    pub fn attach(&mut self, id: i64, parent: Option<i64>, pos: usize) {
        let list = self.children.entry(parent).or_default();
        list.insert(pos.min(list.len()), id);
        self.parent.insert(id, parent);
    }
}

pub fn code_of(p: &ProjectData, id: i64) -> &str {
    p.wbs
        .iter()
        .find(|w| w.id == id)
        .map_or("", |w| w.code.as_str())
}

pub fn code_taken(
    p: &ProjectData,
    tree: &Tree,
    parent: Option<i64>,
    code: &str,
    except: i64,
) -> bool {
    tree.kids(parent)
        .iter()
        .any(|&sibling| sibling != except && code_of(p, sibling).eq_ignore_ascii_case(code))
}

/// Writes tree order and parents back into `p.wbs` (pre-order, renumbered)
/// and the PROJWBS table.
pub fn commit(p: &mut ProjectData, tree: &Tree) {
    let order = tree.preorder();
    let position: BTreeMap<i64, usize> = order
        .iter()
        .enumerate()
        .map(|(i, (id, _))| (*id, i))
        .collect();
    for w in &mut p.wbs {
        if let Some(&i) = position.get(&w.id) {
            w.sequence = (i as i64 + 1) * 10;
            w.parent = match tree.parent.get(&w.id) {
                Some(Some(parent)) => Some(*parent),
                // Keep an outside parent (the EPS) for top-level nodes that had one.
                _ => w.parent.filter(|id| !position.contains_key(id)),
            };
        }
    }
    p.wbs
        .sort_by_key(|w| position.get(&w.id).copied().unwrap_or(usize::MAX));
    sync_projwbs(p);
}

/// Bring PROJWBS rows for this project in line with `p.wbs`.
pub fn sync_projwbs(p: &mut ProjectData) {
    let pid = p.model.project.proj_id;
    let ids: BTreeSet<i64> = p.wbs.iter().map(|w| w.id).collect();
    let table = p.document.table_mut("PROJWBS");
    let (proj_col, id_col) = (table.field("proj_id"), table.field("wbs_id"));
    table.rows.retain(|r| {
        let cell = |c: Option<usize>| c.and_then(|c| r.get(c)).map_or(0, |v| num(v));
        cell(proj_col) != pid || ids.contains(&cell(id_col))
    });
    let mut index: BTreeMap<i64, usize> = table
        .rows
        .iter()
        .enumerate()
        .filter(|(_, r)| num(table.value(r, "proj_id")) == pid)
        .map(|(i, r)| (num(table.value(r, "wbs_id")), i))
        .collect();
    for w in &p.wbs {
        let i = match index.get(&w.id).copied() {
            Some(i) => i,
            None => {
                let template = w
                    .parent
                    .and_then(|parent| {
                        table
                            .rows
                            .iter()
                            .find(|r| num(table.value(r, "wbs_id")) == parent)
                    })
                    .cloned();
                let obs = template
                    .as_ref()
                    .map_or(String::new(), |r| table.value(r, "obs_id").to_string());
                table.add(&[
                    ("wbs_id", w.id.to_string()),
                    ("proj_id", pid.to_string()),
                    ("obs_id", obs),
                    ("proj_node_flag", "N".into()),
                    ("sum_data_flag", "N".into()),
                    ("status_code", "WS_Open".into()),
                ]);
                index.insert(w.id, table.rows.len() - 1);
                table.rows.len() - 1
            }
        };
        table.set(i, "wbs_short_name", w.code.clone());
        table.set(i, "wbs_name", w.name.clone());
        table.set(
            i,
            "parent_wbs_id",
            w.parent.map_or(String::new(), |id| id.to_string()),
        );
        table.set(i, "seq_num", w.sequence.to_string());
    }
}

/// Set the project's name (its project node's WBS name, which the Projects
/// window and Project Details show). Returns whether it changed. The
/// command line writes the same column with `set_wbs` on the project node.
pub fn set_project_name(p: &mut ProjectData, name: &str) -> Result<bool, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("A project needs a name.".into());
    }
    let node = project_node(p).ok_or("The project has no project node.")?;
    let Some(w) = p.wbs.iter_mut().find(|w| w.id == node) else {
        return Err("The project has no project node.".into());
    };
    if w.name == name {
        return Ok(false);
    }
    w.name = name.to_string();
    sync_projwbs(p);
    Ok(true)
}

/// Set one PROJWBS column of element `id` (status, responsible manager,
/// anticipated dates, budget figures, estimated weight and the other plain
/// columns), as the WBS details and the WBS table cells write it. The short
/// code and the name go through the structure (`rename_code`, `Wbs::name`);
/// the columns that place the element are refused. Returns whether the
/// value changed.
pub fn set_column(p: &mut ProjectData, id: i64, field: &str, value: &str) -> Result<bool, String> {
    if !p.wbs.iter().any(|w| w.id == id) {
        return Err("The WBS element is no longer there.".into());
    }
    match field {
        "wbs_id" | "parent_wbs_id" | "proj_id" | "seq_num" | "proj_node_flag" => {
            return Err(format!("{field} cannot be set here."));
        }
        "wbs_short_name" => {
            if code_of(p, id) == value {
                return Ok(false);
            }
            rename_code(p, id, value)?;
            return Ok(true);
        }
        "wbs_name" => {
            let Some(w) = p.wbs.iter_mut().find(|w| w.id == id) else {
                return Ok(false);
            };
            if w.name == value {
                return Ok(false);
            }
            w.name = value.to_string();
            sync_projwbs(p);
            return Ok(true);
        }
        _ => {}
    }
    let table = p.document.table_mut("PROJWBS");
    let Some(row) = table
        .rows
        .iter()
        .position(|r| num(table.value(r, "wbs_id")) == id)
    else {
        return Err("The WBS element has no PROJWBS row.".into());
    };
    if table.value(&table.rows[row], field) == value {
        return Ok(false);
    }
    table.set(row, field, value.to_string());
    Ok(true)
}

pub fn is_project_node(p: &ProjectData, id: i64) -> bool {
    project_node(p) == Some(id)
}

pub fn guard(p: &ProjectData, id: i64) -> Result<(), String> {
    if !p.wbs.iter().any(|w| w.id == id) {
        return Err("Select a WBS element first.".into());
    }
    if is_project_node(p, id) {
        return Err("The project node cannot be moved or deleted.".into());
    }
    Ok(())
}

pub fn shift_up(p: &mut ProjectData, id: i64) -> Result<(), String> {
    shift_within(p, id, -1)
}

pub fn shift_down(p: &mut ProjectData, id: i64) -> Result<(), String> {
    shift_within(p, id, 1)
}

pub fn shift_within(p: &mut ProjectData, id: i64, step: isize) -> Result<(), String> {
    guard(p, id)?;
    let mut tree = Tree::build(p);
    let parent = tree.parent[&id];
    let list = tree.children.get_mut(&parent).unwrap();
    let pos = list.iter().position(|&x| x == id).unwrap();
    let target = pos as isize + step;
    if target < 0 || target as usize >= list.len() {
        return Err(if step < 0 {
            "This WBS element is already first at its level.".into()
        } else {
            "This WBS element is already last at its level.".into()
        });
    }
    list.swap(pos, target as usize);
    commit(p, &tree);
    Ok(())
}

/// Make the element a sibling of its parent, placed just after the parent.
pub fn shift_left(p: &mut ProjectData, id: i64) -> Result<(), String> {
    guard(p, id)?;
    let mut tree = Tree::build(p);
    let Some(parent) = tree.parent[&id] else {
        return Err("This WBS element is already at the top level.".into());
    };
    if is_project_node(p, parent) {
        return Err("WBS elements cannot be shifted outside the project.".into());
    }
    let grandparent = tree.parent[&parent];
    if code_taken(p, &tree, grandparent, code_of(p, id), id) {
        return Err(format!(
            "WBS code {} is already used at the destination level.",
            code_of(p, id)
        ));
    }
    tree.detach(id);
    let pos = tree
        .kids(grandparent)
        .iter()
        .position(|&x| x == parent)
        .map_or(0, |i| i + 1);
    tree.attach(id, grandparent, pos);
    commit(p, &tree);
    Ok(())
}

/// Make the element the last child of the sibling above it.
pub fn shift_right(p: &mut ProjectData, id: i64) -> Result<(), String> {
    guard(p, id)?;
    let mut tree = Tree::build(p);
    let parent = tree.parent[&id];
    let siblings = tree.kids(parent);
    let pos = siblings.iter().position(|&x| x == id).unwrap();
    if pos == 0 {
        return Err("Shift right needs a WBS element above at the same level.".into());
    }
    let new_parent = siblings[pos - 1];
    if code_taken(p, &tree, Some(new_parent), code_of(p, id), id) {
        return Err(format!(
            "WBS code {} is already used at the destination level.",
            code_of(p, id)
        ));
    }
    tree.detach(id);
    let end = tree.kids(Some(new_parent)).len();
    tree.attach(id, Some(new_parent), end);
    commit(p, &tree);
    Ok(())
}

pub fn next_wbs_id(p: &ProjectData) -> i64 {
    let table_max = p.document.table("PROJWBS").map_or(0, |t| {
        t.rows
            .iter()
            .map(|r| num(t.value(r, "wbs_id")))
            .max()
            .unwrap_or(0)
    });
    table_max.max(p.wbs.iter().map(|w| w.id).max().unwrap_or(0)) + 1
}

/// A code unique among `parent`'s children: the seed's trailing number is
/// incremented until free, or a plain counter is used.
pub fn unique_code(p: &ProjectData, tree: &Tree, parent: Option<i64>, seed: &str) -> String {
    let digits = seed.len() - seed.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let (stem, mut n) = if digits > 0 {
        let stem = &seed[..seed.len() - digits];
        (
            stem.to_string(),
            seed[stem.len()..].parse::<u64>().unwrap_or(0),
        )
    } else if seed.is_empty() {
        (String::new(), 0)
    } else {
        (format!("{seed}-"), 0)
    };
    if !seed.is_empty() && !code_taken(p, tree, parent, seed, i64::MIN) {
        return seed.to_string();
    }
    loop {
        n += 1;
        let candidate = format!("{stem}{n}");
        if !code_taken(p, tree, parent, &candidate, i64::MIN) {
            return candidate;
        }
    }
}

/// Add a WBS element below the selection at the same level, or as the first
/// level under the project when the project row is selected.
pub fn add(p: &mut ProjectData, selected: Option<i64>) -> Result<i64, String> {
    let mut tree = Tree::build(p);
    let project = project_node(p);
    let (parent, pos, seed) = match selected.filter(|id| Some(*id) != project) {
        Some(id) if tree.parent.contains_key(&id) => {
            let parent = tree.parent[&id];
            let pos = tree.kids(parent).iter().position(|&x| x == id).unwrap() + 1;
            (parent, pos, code_of(p, id).to_string())
        }
        _ => {
            let parent = project;
            let seed = tree
                .kids(parent)
                .last()
                .map_or(String::new(), |&last| code_of(p, last).to_string());
            (parent, tree.kids(parent).len(), seed)
        }
    };
    let id = next_wbs_id(p);
    let code = unique_code(p, &tree, parent, &seed);
    p.wbs.push(Wbs {
        id,
        parent,
        code,
        name: "(New WBS)".into(),
        sequence: 0,
    });
    tree.attach(id, parent, pos);
    commit(p, &tree);
    Ok(id)
}

pub fn rename_code(p: &mut ProjectData, id: i64, code: &str) -> Result<(), String> {
    let code = code.trim();
    if code.is_empty() {
        return Err("WBS code cannot be empty.".into());
    }
    if is_project_node(p, id) {
        return Err("The project node's code is the Project ID.".into());
    }
    let tree = Tree::build(p);
    let parent = tree.parent.get(&id).copied().flatten();
    if code_taken(p, &tree, parent, code, id) {
        return Err(format!("WBS code {code} is already used at this level."));
    }
    if let Some(w) = p.wbs.iter_mut().find(|w| w.id == id) {
        w.code = code.into();
    }
    sync_projwbs(p);
    Ok(())
}

pub fn separator(p: &ProjectData) -> String {
    let pid = p.model.project.proj_id;
    p.document
        .table("PROJECT")
        .and_then(|t| {
            t.rows
                .iter()
                .find(|r| num(t.value(r, "proj_id")) == pid)
                .map(|r| t.value(r, "name_sep_char").to_string())
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| ".".into())
}

/// Longest full code kept whole; past it a code restarts as `...`.
pub const MAX_FULL_CODE: usize = 200;

/// Sequence of the first project band node the desktop adds when several
/// projects are open together (the others follow).
pub const PROJECT_BAND_SEQUENCE: i64 = i64::MIN;

/// Is `w` a project band node of several open projects?
pub fn is_project_band(w: &Wbs) -> bool {
    w.sequence < PROJECT_BAND_SEQUENCE + (1 << 20)
}

/// Full codes for every element in one pass over the tree: the project ID,
/// then the short codes from the top level down, joined by the project's
/// separator. Each code is its parent's code plus the separator and the
/// short code; once a parent's code passes `MAX_FULL_CODE` bytes the path
/// restarts as `...`, so a very deep WBS neither recurses nor builds codes
/// whose total size grows with the square of the depth.
pub fn full_codes(p: &ProjectData, tree: &Tree) -> BTreeMap<i64, String> {
    full_codes_in(p, tree, &tree.preorder())
}

/// [`full_codes`] over an already computed pre-order.
pub fn full_codes_in(
    p: &ProjectData,
    tree: &Tree,
    preorder: &[(i64, usize)],
) -> BTreeMap<i64, String> {
    let project = project_node(p);
    let sep = separator(p);
    let short: BTreeMap<i64, &str> = p.wbs.iter().map(|w| (w.id, w.code.as_str())).collect();
    let bands: BTreeSet<i64> = p
        .wbs
        .iter()
        .filter(|w| is_project_band(w))
        .map(|w| w.id)
        .collect();
    let mut out: BTreeMap<i64, String> = BTreeMap::new();
    for &(id, _) in preorder {
        if Some(id) == project || bands.contains(&id) {
            let name = if bands.contains(&id) {
                short.get(&id).copied().unwrap_or("").to_string()
            } else {
                p.model.project.name.clone()
            };
            out.insert(id, name);
            continue;
        }
        let prefix = match tree.parent.get(&id).copied().flatten() {
            Some(parent) => out.get(&parent).map_or("", String::as_str),
            None => p.model.project.name.as_str(),
        };
        let code = short.get(&id).copied().unwrap_or("");
        let full = if prefix.len() > MAX_FULL_CODE {
            format!("...{sep}{code}")
        } else {
            format!("{prefix}{sep}{code}")
        };
        out.insert(id, full);
    }
    out
}

/// Every WBS element's code path: short codes from the top level down,
/// below the project node, joined by `.`.
pub fn paths(p: &ProjectData) -> BTreeMap<String, i64> {
    let tree = Tree::build(p);
    let project = project_node(p);
    let mut full: BTreeMap<i64, String> = BTreeMap::new();
    let mut out = BTreeMap::new();
    for (id, _) in tree.preorder() {
        if Some(id) == project {
            continue;
        }
        let code = code_of(p, id);
        let path = match tree.parent.get(&id).copied().flatten() {
            Some(parent) if Some(parent) != project => {
                format!("{}.{code}", full.get(&parent).map_or("", String::as_str))
            }
            _ => code.to_string(),
        };
        full.insert(id, path.clone());
        out.entry(path.to_ascii_lowercase()).or_insert(id);
    }
    out
}

/// The WBS element at `path`: short codes from the top level down, below
/// the project node, joined by `.` (or the project's separator).
pub fn find_path(p: &ProjectData, path: &str) -> Option<i64> {
    paths(p).get(&normal(p, path)).copied()
}

/// A path with the project's separator written as `.`, trimmed, lowercase.
fn normal(p: &ProjectData, path: &str) -> String {
    let sep = separator(p);
    let mut text = path.trim().to_string();
    if sep != "." && !sep.is_empty() {
        text = text.replace(&sep, ".");
    }
    text.split('.')
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(".")
        .to_ascii_lowercase()
}

/// The WBS element at `path`, adding the missing elements below the
/// longest part of the path that exists (named by their codes, or `name`
/// for the last one). Returns its id and how many were added.
pub fn ensure_path(
    p: &mut ProjectData,
    path: &str,
    name: Option<&str>,
) -> Result<(i64, usize), String> {
    let wanted = normal(p, path);
    if wanted.is_empty() || wanted.split('.').any(str::is_empty) {
        return Err(format!("{path:?} is not a WBS code path."));
    }
    let known = paths(p);
    if let Some(id) = known.get(&wanted) {
        return Ok((*id, 0));
    }
    // The original spelling of the codes still to add.
    let original: Vec<String> = path
        .trim()
        .replace(&separator(p), ".")
        .split('.')
        .map(|s| s.trim().to_string())
        .collect();
    let segments: Vec<&str> = wanted.split('.').collect();
    let mut parent = None;
    let mut first = 0;
    for k in (1..segments.len()).rev() {
        if let Some(id) = known.get(&segments[..k].join(".")) {
            parent = Some(*id);
            first = k;
            break;
        }
    }
    let mut added = 0;
    for (n, code) in original.iter().enumerate().skip(first) {
        let id = match parent {
            Some(id) => {
                let new = add(p, Some(id))?;
                shift_right(p, new)?;
                new
            }
            None => add(p, None)?,
        };
        rename_code(p, id, code)?;
        let label = if n + 1 == original.len() {
            name.unwrap_or(code)
        } else {
            code
        };
        if let Some(w) = p.wbs.iter_mut().find(|w| w.id == id) {
            w.name = label.to_string();
        }
        sync_projwbs(p);
        added += 1;
        parent = Some(id);
    }
    Ok((parent.expect("at least one code added"), added))
}

/// Activities assigned directly to each WBS element.
pub fn activity_counts(p: &ProjectData) -> BTreeMap<i64, usize> {
    let mut counts = BTreeMap::new();
    for d in p.details.values() {
        *counts.entry(d.wbs).or_default() += 1;
    }
    counts
}

/// What deleting or merging one WBS element involves.
#[derive(Clone, Debug, PartialEq)]
pub struct DeletePlan {
    pub id: i64,
    /// Activities assigned directly to the element.
    pub activities: usize,
    /// Child elements directly under it.
    pub children: usize,
}

/// Check that a WBS element can be merged into its parent: an element with
/// activities or child elements needs a parent to take them.
pub fn plan_delete(p: &ProjectData, id: i64) -> Result<DeletePlan, String> {
    guard(p, id)?;
    let tree = Tree::build(p);
    let activities = activity_counts(p).get(&id).copied().unwrap_or(0);
    let children = tree.kids(Some(id)).len();
    if (activities > 0 || children > 0) && tree.parent[&id].is_none() {
        return Err(
            "This WBS element has no parent to merge into, so its activities and child elements cannot be moved."
                .into(),
        );
    }
    Ok(DeletePlan {
        id,
        activities,
        children,
    })
}

/// The activities under a WBS element and every element below it, as
/// `(task_id, "ID - Name")` in table order.
pub fn subtree_activities(p: &ProjectData, id: i64) -> Vec<(i64, String)> {
    let tree = Tree::build(p);
    let nodes: BTreeSet<i64> = tree.descendants(id).into_iter().collect();
    p.model
        .activities
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            p.details
                .get(&a.task_id)
                .is_some_and(|d| nodes.contains(&d.wbs))
        })
        .map(|(i, a)| (a.task_id, format!("{} - {}", a.task_code, p.name(i))))
        .collect()
}

/// Rows that belong to a WBS element through `wbs_id`: milestones, spending
/// plan and budget log.
const OWNED: [&str; 3] = ["WBSSTEP", "WBSBUDG", "BUDGCHNG"];
/// Project records that name a WBS element: issues and thresholds.
const REFERRING: [&str; 2] = ["PROJISSU", "PROJTHRS"];

/// Rows outside PROJWBS and TASK that hang off a set of WBS elements.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Related {
    /// WBS milestones (WBSSTEP).
    pub milestones: usize,
    /// Spending plan rows (WBSBUDG).
    pub spending: usize,
    /// Budget log entries (BUDGCHNG).
    pub budget_log: usize,
    /// Work products and documents assigned to the element itself (TASKDOC
    /// rows with no activity).
    pub documents: usize,
    /// Notebook topics (WBSMEMO).
    pub notebooks: usize,
    /// User defined field values of the elements.
    pub udf_values: usize,
    /// Issues and thresholds that name one of the elements.
    pub issues: usize,
    pub thresholds: usize,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

impl Related {
    /// The rows that go with the elements, as phrases ("2 WBS milestones").
    pub fn removed(&self) -> Vec<String> {
        [
            (self.milestones, "WBS milestone", "WBS milestones"),
            (self.spending, "spending plan row", "spending plan rows"),
            (self.budget_log, "budget log entry", "budget log entries"),
            (
                self.documents,
                "work product or document",
                "work products and documents",
            ),
            (self.notebooks, "notebook topic", "notebook topics"),
            (
                self.udf_values,
                "user defined field value",
                "user defined field values",
            ),
        ]
        .into_iter()
        .filter(|(n, _, _)| *n > 0)
        .map(|(n, one, many)| plural(n, one, many))
        .collect()
    }

    /// The issues and thresholds that name the elements, as phrases.
    pub fn referring(&self) -> Vec<String> {
        [
            (self.issues, "issue", "issues"),
            (self.thresholds, "threshold", "thresholds"),
        ]
        .into_iter()
        .filter(|(n, _, _)| *n > 0)
        .map(|(n, one, many)| plural(n, one, many))
        .collect()
    }
}

/// UDFTYPE ids of WBS user defined fields.
fn wbs_udf_types(p: &ProjectData) -> BTreeSet<i64> {
    p.document.table("UDFTYPE").map_or_else(BTreeSet::new, |t| {
        t.rows
            .iter()
            .filter(|r| t.value(r, "table_name").eq_ignore_ascii_case("PROJWBS"))
            .map(|r| num(t.value(r, "udf_type_id")))
            .collect()
    })
}

/// A TASKDOC row assigned to a WBS element rather than an activity.
fn wbs_document(t: &crate::Table, r: &[String]) -> bool {
    num(t.value(r, "task_id")) == 0
}

/// Count the rows that hang off the WBS elements `nodes`.
pub fn related(p: &ProjectData, nodes: &BTreeSet<i64>) -> Related {
    let udfs = wbs_udf_types(p);
    let mut out = Related::default();
    for t in &p.document.tables {
        let on = |r: &Vec<String>| nodes.contains(&num(t.value(r, "wbs_id")));
        let count = |keep: &dyn Fn(&Vec<String>) -> bool| t.rows.iter().filter(|r| keep(r)).count();
        match t.name.as_str() {
            "WBSSTEP" => out.milestones += count(&on),
            "WBSBUDG" => out.spending += count(&on),
            "BUDGCHNG" => out.budget_log += count(&on),
            "TASKDOC" => out.documents += count(&|r| on(r) && wbs_document(t, r)),
            "WBSMEMO" => out.notebooks += count(&on),
            "PROJISSU" => out.issues += count(&on),
            "PROJTHRS" => out.thresholds += count(&on),
            "UDFVALUE" => {
                out.udf_values += count(&|r| {
                    udfs.contains(&num(t.value(r, "udf_type_id")))
                        && nodes.contains(&num(t.value(r, "fk_id")))
                })
            }
            _ => {}
        }
    }
    out
}

/// After WBS elements `nodes` are removed: their notebook topics and user
/// field values go. With `keep_owned` (a merge) their milestones, spending
/// plan, budget log and documents move to `merge_into`; without it those go
/// too. Issues and thresholds that named a removed element name
/// `merge_into` (`None`: no WBS element).
fn cascade(p: &mut ProjectData, nodes: &BTreeSet<i64>, merge_into: Option<i64>, keep_owned: bool) {
    let udfs = wbs_udf_types(p);
    let target = merge_into.map_or(String::new(), |id| id.to_string());
    let cell = |r: &[String], c: Option<usize>| c.and_then(|c| r.get(c)).map_or(0, |v| num(v));
    for t in p.document.tables.iter_mut() {
        if t.name == "UDFVALUE" {
            let (kind, key) = (t.field("udf_type_id"), t.field("fk_id"));
            t.rows
                .retain(|r| !(udfs.contains(&cell(r, kind)) && nodes.contains(&cell(r, key))));
            continue;
        }
        let name = t.name.as_str();
        let owned = OWNED.contains(&name);
        let documents = name == "TASKDOC";
        let memo = name == "WBSMEMO";
        if !(owned || documents || memo || REFERRING.contains(&name)) {
            continue;
        }
        let col = t.field("wbs_id");
        let task = t.field("task_id");
        let on = |r: &[String]| col.is_some() && nodes.contains(&cell(r, col));
        // Removed: notebook topics always; on a delete, the owned rows and
        // the documents assigned to the element itself (activity documents
        // went with their activities).
        let remove = |r: &[String]| {
            on(r) && (memo || (!keep_owned && (owned || (documents && cell(r, task) == 0))))
        };
        t.rows.retain(|r| !remove(r));
        // The rest name the surviving element.
        if let Some(col) = col {
            for r in t.rows.iter_mut() {
                if r.get(col).is_some_and(|v| nodes.contains(&num(v))) {
                    r[col] = target.clone();
                }
            }
        }
    }
}

/// Delete a WBS element, every element below it and all their activities
/// (with each activity's relationships and child rows, as Edit > Delete
/// removes them), and the rows the elements own: milestones, spending plan,
/// budget log, their documents, notebook topics and user field values.
/// Issues and thresholds that named a deleted element name its parent.
/// Returns the number of activities deleted.
pub fn delete_with_activities(p: &mut ProjectData, id: i64) -> Result<usize, String> {
    guard(p, id)?;
    let mut tree = Tree::build(p);
    let parent = tree.parent.get(&id).copied().flatten();
    let nodes: BTreeSet<i64> = tree.descendants(id).into_iter().collect();
    let tasks: Vec<i64> = subtree_activities(p, id)
        .into_iter()
        .map(|(t, _)| t)
        .collect();
    if !tasks.is_empty() {
        p.delete_activities(&tasks)?;
    }
    tree.detach(id);
    for node in &nodes {
        tree.children.remove(&Some(*node));
        tree.parent.remove(node);
    }
    p.wbs.retain(|w| !nodes.contains(&w.id));
    cascade(p, &nodes, parent, false);
    commit(p, &tree);
    Ok(tasks.len())
}

/// Delete a WBS element. Its activities, milestones, spending plan, budget
/// log and documents move to the parent element, issues and thresholds that
/// named it name the parent, and its child elements take its place under
/// the parent. Its notebook topics and user field values go. An element
/// without a parent (it has no activities or children) loses its rows.
pub fn delete_merge(p: &mut ProjectData, id: i64) -> Result<(), String> {
    let plan = plan_delete(p, id)?;
    let mut tree = Tree::build(p);
    let (parent, pos) = tree.detach(id).ok_or("Select a WBS element first.")?;
    let kids = tree.children.remove(&Some(id)).unwrap_or_default();
    for (offset, child) in kids.into_iter().enumerate() {
        if code_taken(p, &tree, parent, code_of(p, child), child) {
            let code = unique_code(p, &tree, parent, code_of(p, child));
            if let Some(w) = p.wbs.iter_mut().find(|w| w.id == child) {
                w.code = code;
            }
        }
        tree.attach(child, parent, pos + offset);
    }
    tree.parent.remove(&id);
    if plan.activities > 0 {
        if let Some(target) = parent {
            for d in p.details.values_mut() {
                if d.wbs == id {
                    d.wbs = target;
                }
            }
        }
    }
    p.wbs.retain(|w| w.id != id);
    cascade(p, &BTreeSet::from([id]), parent, parent.is_some());
    commit(p, &tree);
    Ok(())
}

/// What a WBS copy brings along besides the elements themselves (the Copy
/// WBS Options choices other than Activities, which the desktop pastes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CopyParts {
    /// Notebook topics (WBSMEMO).
    pub notebook: bool,
    /// WBS milestones (WBSSTEP).
    pub milestones: bool,
    /// Work products and documents assigned to the elements themselves
    /// (TASKDOC rows without an activity).
    pub documents: bool,
}

impl CopyParts {
    /// The elements alone.
    pub const NONE: CopyParts = CopyParts {
        notebook: false,
        milestones: false,
        documents: false,
    };
}

/// Copy the element `root` and every element below it as the last child of
/// `parent` (`None`: the top level). The copy of `root` gets a code unique
/// among its new siblings; the elements below keep their codes. Each copy
/// keeps its name and responsible manager, and takes the rows `parts`
/// names with new keys. Returns the new id of every copied element, by old
/// id.
pub fn copy_subtree(
    p: &mut ProjectData,
    root: i64,
    parent: Option<i64>,
    parts: CopyParts,
) -> Result<BTreeMap<i64, i64>, String> {
    let mut tree = Tree::build(p);
    if !tree.parent.contains_key(&root) {
        return Err("The copied WBS element no longer exists.".into());
    }
    if is_project_node(p, root) {
        return Err("The project node cannot be cut or copied.".into());
    }
    if parent.is_some_and(|id| !tree.parent.contains_key(&id)) {
        return Err("The WBS element to paste into no longer exists.".into());
    }
    let subtree = tree.descendants(root);
    let first = next_wbs_id(p);
    let mapping: BTreeMap<i64, i64> = subtree
        .iter()
        .enumerate()
        .map(|(k, &old)| (old, first + k as i64))
        .collect();
    for &old in &subtree {
        let source = p.wbs.iter().find(|w| w.id == old).unwrap().clone();
        let new_parent = if old == root {
            parent
        } else {
            tree.parent[&old].map(|x| mapping[&x])
        };
        let code = if old == root {
            unique_code(p, &tree, new_parent, &source.code)
        } else {
            source.code.clone()
        };
        p.wbs.push(Wbs {
            id: mapping[&old],
            parent: new_parent,
            code,
            name: source.name.clone(),
            sequence: 0,
        });
        let pos = tree.kids(new_parent).len();
        tree.attach(mapping[&old], new_parent, pos);
    }
    // The responsible manager of each copied element.
    let managers: Vec<(i64, String)> = p
        .document
        .table("PROJWBS")
        .map(|t| {
            subtree
                .iter()
                .filter_map(|old| {
                    t.rows
                        .iter()
                        .find(|r| num(t.value(r, "wbs_id")) == *old)
                        .map(|r| (mapping[old], t.value(r, "obs_id").to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    commit(p, &tree);
    let table = p.document.table_mut("PROJWBS");
    for (id, obs) in managers {
        if let Some(i) = table
            .rows
            .iter()
            .position(|r| num(table.value(r, "wbs_id")) == id)
        {
            table.set(i, "obs_id", obs);
        }
    }
    let pid = p.model.project.proj_id;
    for (on, name, key) in [
        (parts.notebook, "WBSMEMO", "wbs_memo_id"),
        (parts.milestones, "WBSSTEP", "wbs_step_id"),
        (parts.documents, "TASKDOC", "taskdoc_id"),
    ] {
        if on {
            copy_rows(p, name, key, &mapping, pid);
        }
    }
    Ok(mapping)
}

/// Copies of the rows of `name` that hang off the old elements of
/// `mapping`, each with a new `key` and the new element's id. Documents
/// assigned to an activity stay with the activity.
fn copy_rows(p: &mut ProjectData, name: &str, key: &str, mapping: &BTreeMap<i64, i64>, pid: i64) {
    let Some(t) = p.document.table(name) else {
        return;
    };
    let first = super::records::next_id(&p.document, name, key);
    let copies: Vec<Vec<String>> = t
        .rows
        .iter()
        .filter_map(|r| {
            let new = mapping.get(&num(t.value(r, "wbs_id")))?;
            (name != "TASKDOC" || wbs_document(t, r)).then_some((r, *new))
        })
        .enumerate()
        .map(|(k, (r, new))| {
            let mut row = r.clone();
            for (column, value) in [
                ("wbs_id", new.to_string()),
                ("proj_id", pid.to_string()),
                (key, (first + k as i64).to_string()),
            ] {
                if let Some(cell) = t.field(column).and_then(|c| row.get_mut(c)) {
                    *cell = value;
                }
            }
            row
        })
        .collect();
    p.document.table_mut(name).rows.extend(copies);
}

/// The sample WBS (`crate::starter`) as builds up to 39221ef copied it into
/// every project made with the Projects view Add: `(wbs_id, code, name)`.
/// Kept as shipped, so a later change to the sample cannot widen the repair.
pub const ADD_SAMPLE_WBS: [(i64, &str, &str); 4] = [
    (10, "RB.1", "Engineering & approvals"),
    (20, "RB.2", "Procurement"),
    (30, "RB.3", "Construction"),
    (40, "RB.4", "Commissioning & turnover"),
];

/// Repair a project that the Projects view Add made with the sample's WBS.
///
/// It acts only when all of this holds, so it never touches a WBS someone
/// built or used: the project has no project node; it is not the sample
/// itself or a copy of it (Project ID not starting with RIVERBEND); at least
/// one of the sample's four elements is there and every one that is there
/// still has the sample's ID, code and name at the top level; and none of
/// them has activities, child elements or any other row (notebook, UDF
/// value, budget, step or other table) pointing at it. Then it removes those
/// sample elements, adds a project node named after the project (as the New
/// Project wizard does) and puts any other top-level elements under it.
/// Returns the codes it removed, or `None` when it changed nothing.
pub fn repair_add_sample_wbs(p: &mut ProjectData) -> Option<Vec<String>> {
    if project_node(p).is_some()
        || p.model
            .project
            .name
            .to_ascii_uppercase()
            .starts_with("RIVERBEND")
    {
        return None;
    }
    let pid = p.model.project.proj_id;
    let mut found = vec![];
    for (id, code, name) in ADD_SAMPLE_WBS {
        let Some(w) = p.wbs.iter().find(|w| w.id == id) else {
            continue;
        };
        if w.code != code || w.name != name || w.parent.is_some() {
            return None;
        }
        found.push(id);
    }
    if found.is_empty() {
        return None;
    }
    let ids: BTreeSet<i64> = found.iter().copied().collect();
    let used_by_activity = p.details.values().any(|d| ids.contains(&d.wbs));
    let has_child = p
        .wbs
        .iter()
        .any(|w| w.parent.is_some_and(|parent| ids.contains(&parent)));
    let wbs_udfs: BTreeSet<i64> = p.document.table("UDFTYPE").map_or_else(BTreeSet::new, |t| {
        t.rows
            .iter()
            .filter(|r| t.value(r, "table_name").eq_ignore_ascii_case("PROJWBS"))
            .map(|r| num(t.value(r, "udf_type_id")))
            .collect()
    });
    let referenced = p.document.tables.iter().any(|t| {
        if t.name.eq_ignore_ascii_case("PROJWBS") {
            return false;
        }
        let wbs_col = t.field("wbs_id").or_else(|| t.field("parent_wbs_id"));
        let proj_col = t.field("proj_id");
        t.rows.iter().any(|r| {
            let mine = proj_col
                .and_then(|c| r.get(c))
                .is_none_or(|v| v.trim().is_empty() || num(v) == pid);
            let wbs = wbs_col
                .and_then(|c| r.get(c))
                .is_some_and(|v| ids.contains(&num(v)));
            let udf = t.name.eq_ignore_ascii_case("UDFVALUE")
                && wbs_udfs.contains(&num(t.value(r, "udf_type_id")))
                && ids.contains(&num(t.value(r, "fk_id")));
            mine && wbs || udf
        })
    });
    if used_by_activity || has_child || referenced {
        return None;
    }
    let removed: Vec<String> = ADD_SAMPLE_WBS
        .iter()
        .filter(|(id, _, _)| ids.contains(id))
        .map(|(_, code, _)| code.to_string())
        .collect();
    p.wbs.retain(|w| !ids.contains(&w.id));
    let taken = p
        .document
        .table("PROJWBS")
        .map_or(0, |t| {
            t.rows
                .iter()
                .map(|r| num(t.value(r, "wbs_id")))
                .max()
                .unwrap_or(0)
        })
        .max(p.wbs.iter().map(|w| w.id).max().unwrap_or(0));
    let node = taken.max(ids.iter().copied().max().unwrap_or(0)) + 1;
    for w in &mut p.wbs {
        if w.parent.is_none() {
            w.parent = Some(node);
        }
    }
    let name = p.model.project.name.clone();
    p.wbs.insert(
        0,
        Wbs {
            id: node,
            parent: None,
            code: name.clone(),
            name,
            sequence: 0,
        },
    );
    let tree = Tree::build(p);
    commit(p, &tree);
    let table = p.document.table_mut("PROJWBS");
    if let Some(i) = table.rows.iter().position(|r| {
        num(table.value(r, "wbs_id")) == node && num(table.value(r, "proj_id")) == pid
    }) {
        table.set(i, "proj_node_flag", "Y".into());
    }
    Some(removed)
}
