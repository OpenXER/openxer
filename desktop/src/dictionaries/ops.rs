//! Dictionary edits on the project document: code types and code values for
//! activities, projects and resources, user defined fields, cost accounts and
//! the admin categories. Everything here works on the XER tables the document
//! keeps, so edits survive save, reopen and export.
//!
//! Delete rule when an item is in use (the dialogs ask first):
//! - Code type: its values and every assignment of them are removed.
//! - Code value: the value, its child values and every assignment of any of
//!   them are removed.
//! - User defined field: the field and all of its values are removed.
//! - Cost account: the account and its child accounts are removed. Expenses,
//!   resource assignments and the project default that used them either move
//!   to a replacement account the user picks or are left without an account.
//! - Notebook topic: the notebooks written under it (activity and WBS) are
//!   removed with it.
//! - Expense category, document category, document status, risk category and
//!   unit of measure: the rows that used it keep everything else and are left
//!   without that value.
use crate::records::{self, num};
use openxer_xer::Document;
use std::collections::{BTreeMap, BTreeSet};

pub type Result<T> = std::result::Result<T, String>;

// ------------------------------------------------------------ ordered trees

/// A dictionary table kept in sibling order by a sequence column, optionally
/// nested through a parent column and split into owners (code types).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeSpec {
    pub table: &'static str,
    pub id: &'static str,
    pub parent: Option<&'static str>,
    pub seq: &'static str,
    /// Column that must be filled and unique among siblings.
    pub key: &'static str,
    /// Column holding the owning code type, if the table is shared by types.
    pub owner: Option<&'static str>,
    /// Longest allowed key, when the spec fixes one.
    pub key_len: Option<usize>,
}

/// One row of a tree in display order.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: i64,
    pub parent: Option<i64>,
    pub depth: usize,
    pub row: usize,
    pub has_children: bool,
}

fn table_rows(doc: &Document, spec: &TreeSpec, owner: Option<i64>) -> Vec<usize> {
    let Some(t) = doc.table(spec.table) else {
        return vec![];
    };
    t.rows
        .iter()
        .enumerate()
        .filter(|(_, r)| match (spec.owner, owner) {
            (Some(column), Some(owner)) => num(t.value(r, column)) == owner,
            _ => true,
        })
        .map(|(i, _)| i)
        .collect()
}

fn cell(doc: &Document, table: &str, row: usize, column: &str) -> String {
    records::get(doc, table, row, column)
}

/// The tree in display order: depth first, siblings by sequence then key.
/// Rows whose parent is missing are shown at the top level.
pub fn tree(doc: &Document, spec: &TreeSpec, owner: Option<i64>) -> Vec<Node> {
    let rows = table_rows(doc, spec, owner);
    let ids: BTreeSet<i64> = rows
        .iter()
        .map(|&r| num(&cell(doc, spec.table, r, spec.id)))
        .collect();
    let parent_of = |r: usize| -> Option<i64> {
        let column = spec.parent?;
        let p = cell(doc, spec.table, r, column);
        let p = num(&p);
        (p != 0 && ids.contains(&p)).then_some(p)
    };
    let mut entries: Vec<(Option<i64>, i64, String, i64, usize)> = rows
        .iter()
        .map(|&r| {
            (
                parent_of(r),
                num(&cell(doc, spec.table, r, spec.seq)),
                cell(doc, spec.table, r, spec.key).to_lowercase(),
                num(&cell(doc, spec.table, r, spec.id)),
                r,
            )
        })
        .collect();
    entries.sort_by(|a, b| (a.1, &a.2, a.3).cmp(&(b.1, &b.2, b.3)));
    // Children of each parent in display order (entries are already sorted).
    let mut children: BTreeMap<Option<i64>, Vec<usize>> = BTreeMap::new();
    for (i, e) in entries.iter().enumerate() {
        children.entry(e.0).or_default().push(i);
    }
    let mut out = Vec::with_capacity(entries.len());
    let mut seen = BTreeSet::new();
    // Depth first with an explicit stack (an imported file can nest deeply):
    // each frame is a parent, its depth and the next child to visit.
    let mut stack: Vec<(Option<i64>, usize, usize)> = vec![(None, 0, 0)];
    while let Some((parent, depth, next)) = stack.pop() {
        let Some(&i) = children.get(&parent).and_then(|list| list.get(next)) else {
            continue;
        };
        stack.push((parent, depth, next + 1));
        let e = &entries[i];
        if !seen.insert(e.3) {
            continue;
        }
        out.push(Node {
            id: e.3,
            parent,
            depth,
            row: e.4,
            has_children: false,
        });
        stack.push((Some(e.3), depth + 1, 0));
    }
    // A node's children, if any, follow it directly.
    for i in 1..out.len() {
        if out[i].depth > out[i - 1].depth {
            out[i - 1].has_children = true;
        }
    }
    // Rows caught in a parent cycle are shown at the top level.
    for e in &entries {
        if !seen.contains(&e.3) {
            seen.insert(e.3);
            out.push(Node {
                id: e.3,
                parent: None,
                depth: 0,
                row: e.4,
                has_children: false,
            });
        }
    }
    out
}

fn row_of(doc: &Document, spec: &TreeSpec, id: i64) -> Option<usize> {
    let t = doc.table(spec.table)?;
    t.rows.iter().position(|r| num(t.value(r, spec.id)) == id)
}

pub fn value(doc: &Document, spec: &TreeSpec, id: i64, column: &str) -> String {
    row_of(doc, spec, id)
        .map(|r| cell(doc, spec.table, r, column))
        .unwrap_or_default()
}

fn set_value(doc: &mut Document, spec: &TreeSpec, id: i64, column: &str, text: String) {
    if let Some(r) = row_of(doc, spec, id) {
        records::set(doc, spec.table, r, column, text);
    }
}

/// The item and everything below it.
pub fn subtree(doc: &Document, spec: &TreeSpec, owner: Option<i64>, id: i64) -> Vec<i64> {
    let nodes = tree(doc, spec, owner);
    let Some(at) = nodes.iter().position(|n| n.id == id) else {
        return vec![];
    };
    let depth = nodes[at].depth;
    std::iter::once(id)
        .chain(
            nodes[at + 1..]
                .iter()
                .take_while(|n| n.depth > depth)
                .map(|n| n.id),
        )
        .collect()
}

fn siblings(nodes: &[Node], parent: Option<i64>) -> Vec<i64> {
    nodes
        .iter()
        .filter(|n| n.parent == parent)
        .map(|n| n.id)
        .collect()
}

/// Give `order` (siblings) increasing sequence numbers, keeping every
/// number that is already in order so an edit rewrites as few rows as it can.
fn renumber(doc: &mut Document, spec: &TreeSpec, order: &[i64]) {
    let mut previous: Option<i64> = None;
    for &id in order {
        let current = value(doc, spec, id, spec.seq);
        let keep = !current.trim().is_empty() && previous.is_none_or(|p| num(&current) > p);
        let seq = if keep {
            num(&current)
        } else {
            previous.map_or(0, |p| p + 1)
        };
        if !keep {
            set_value(doc, spec, id, spec.seq, seq.to_string());
        }
        previous = Some(seq);
    }
}

fn check_key(
    doc: &Document,
    spec: &TreeSpec,
    owner: Option<i64>,
    parent: Option<i64>,
    except: Option<i64>,
    key: &str,
    noun: &str,
) -> Result<()> {
    let key = key.trim();
    if key.is_empty() {
        return Err(format!("The {noun} cannot be blank."));
    }
    if let Some(max) = spec.key_len {
        if key.chars().count() > max {
            return Err(format!("The {noun} can be at most {max} characters long."));
        }
    }
    let nodes = tree(doc, spec, owner);
    let clash = nodes.iter().any(|n| {
        n.parent == parent
            && Some(n.id) != except
            && cell(doc, spec.table, n.row, spec.key)
                .trim()
                .eq_ignore_ascii_case(key)
    });
    if clash {
        return Err(format!("{key} already exists at this level."));
    }
    Ok(())
}

/// A key not yet used among the siblings: `base`, `base 2`, `base 3`...,
/// shortened to the spec's length limit (the number replaces the end of
/// `base` when the limit would cut it off). With `n` siblings one of the
/// first `n + 1` numbered keys is free whenever the number fits, so the
/// search stops there and reports an error instead of looping.
fn unique_key(
    doc: &Document,
    spec: &TreeSpec,
    owner: Option<i64>,
    parent: Option<i64>,
    base: &str,
) -> Result<String> {
    let siblings = tree(doc, spec, owner)
        .iter()
        .filter(|n| n.parent == parent)
        .count();
    let fit = |text: &str| match spec.key_len {
        Some(max) => text.chars().take(max).collect(),
        None => text.to_string(),
    };
    for n in 1..=siblings + 2 {
        let candidate = if n == 1 {
            fit(base)
        } else {
            let digits = n.to_string();
            let room = |used: usize| spec.key_len.map_or(usize::MAX, |m| m.saturating_sub(used));
            // Keep the space before the number only while some of `base` fits.
            let spaced = base.chars().count() > 2 && room(digits.len() + 1) > 0;
            let (gap, room) = if spaced {
                (" ", room(digits.len() + 1))
            } else {
                ("", room(digits.len()))
            };
            let start: String = base.chars().take(room).collect();
            format!("{}{gap}{digits}", start.trim_end())
        };
        if check_key(doc, spec, owner, parent, None, &candidate, "value").is_ok() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "There is no free name for {} at this level.",
        base.trim()
    ))
}

/// Insert a row after `after` (as its next sibling), or at the end of the top
/// level. `values` fill the other columns. Returns the new id.
pub fn add_node(
    doc: &mut Document,
    spec: &TreeSpec,
    owner: Option<i64>,
    after: Option<i64>,
    values: &[(&str, String)],
) -> i64 {
    let nodes = tree(doc, spec, owner);
    let parent = after.and_then(|a| nodes.iter().find(|n| n.id == a).and_then(|n| n.parent));
    let id = records::next_id(doc, spec.table, spec.id);
    let mut row: Vec<(&str, String)> = vec![(spec.id, id.to_string())];
    if let (Some(column), Some(owner)) = (spec.owner, owner) {
        row.push((column, owner.to_string()));
    }
    if let (Some(column), Some(parent)) = (spec.parent, parent) {
        row.push((column, parent.to_string()));
    }
    row.extend(values.iter().cloned());
    records::add_row(doc, spec.table, &row);
    let mut order = siblings(&nodes, parent);
    let at = after
        .and_then(|a| order.iter().position(|&s| s == a))
        .map_or(order.len(), |p| p + 1);
    order.insert(at, id);
    renumber(doc, spec, &order);
    id
}

/// Shift up (`-1`) or down (`1`) among siblings.
pub fn shift(doc: &mut Document, spec: &TreeSpec, owner: Option<i64>, id: i64, by: i32) -> bool {
    let nodes = tree(doc, spec, owner);
    let Some(node) = nodes.iter().find(|n| n.id == id) else {
        return false;
    };
    let mut order = siblings(&nodes, node.parent);
    let at = order.iter().position(|&s| s == id).unwrap();
    let to = at as i64 + by as i64;
    if to < 0 || to as usize >= order.len() {
        return false;
    }
    order.swap(at, to as usize);
    renumber(doc, spec, &order);
    true
}

/// Shift right: become the last child of the previous sibling.
pub fn indent(doc: &mut Document, spec: &TreeSpec, owner: Option<i64>, id: i64) -> Result<bool> {
    let Some(column) = spec.parent else {
        return Ok(false);
    };
    let nodes = tree(doc, spec, owner);
    let Some(node) = nodes.iter().find(|n| n.id == id) else {
        return Ok(false);
    };
    let order = siblings(&nodes, node.parent);
    let at = order.iter().position(|&s| s == id).unwrap();
    if at == 0 {
        return Ok(false);
    }
    let parent = order[at - 1];
    let key = value(doc, spec, id, spec.key);
    check_key(doc, spec, owner, Some(parent), Some(id), &key, "value")?;
    let mut children = siblings(&nodes, Some(parent));
    set_value(doc, spec, id, column, parent.to_string());
    set_value(doc, spec, id, spec.seq, String::new());
    children.push(id);
    renumber(doc, spec, &children);
    Ok(true)
}

/// Shift left: move up a level, right after the former parent.
pub fn outdent(doc: &mut Document, spec: &TreeSpec, owner: Option<i64>, id: i64) -> Result<bool> {
    let Some(column) = spec.parent else {
        return Ok(false);
    };
    let nodes = tree(doc, spec, owner);
    let Some(node) = nodes.iter().find(|n| n.id == id) else {
        return Ok(false);
    };
    let Some(parent) = node.parent else {
        return Ok(false);
    };
    let grand = nodes.iter().find(|n| n.id == parent).and_then(|n| n.parent);
    let key = value(doc, spec, id, spec.key);
    check_key(doc, spec, owner, grand, Some(id), &key, "value")?;
    let mut order = siblings(&nodes, grand);
    let at = order
        .iter()
        .position(|&s| s == parent)
        .unwrap_or(order.len());
    order.insert(at + 1, id);
    set_value(
        doc,
        spec,
        id,
        column,
        grand.map(|g| g.to_string()).unwrap_or_default(),
    );
    set_value(doc, spec, id, spec.seq, String::new());
    renumber(doc, spec, &order);
    Ok(true)
}

fn remove_ids(doc: &mut Document, spec: &TreeSpec, ids: &BTreeSet<i64>) {
    if doc.table(spec.table).is_none() {
        return;
    }
    let t = doc.table_mut(spec.table);
    let Some(c) = t.field(spec.id) else {
        return;
    };
    t.rows
        .retain(|r| !ids.contains(&r.get(c).map_or(0, |v| num(v))));
}

/// Rows of `table` whose `column` holds one of `ids`.
fn referencing(doc: &Document, table: &str, column: &str, ids: &BTreeSet<i64>) -> Vec<usize> {
    let Some(t) = doc.table(table) else {
        return vec![];
    };
    if t.field(column).is_none() {
        return vec![];
    }
    t.rows
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            let v = t.value(r, column);
            !v.trim().is_empty() && ids.contains(&num(v))
        })
        .map(|(i, _)| i)
        .collect()
}

fn delete_rows(doc: &mut Document, table: &str, rows: &[usize]) {
    for &r in rows.iter().rev() {
        records::remove_row(doc, table, r);
    }
}

// ------------------------------------------------------------ clipboard

/// Copied or cut rows of one tree, with their children.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub spec: TreeSpec,
    pub owner: Option<i64>,
    pub cut: bool,
    /// The copied rows in display order; depth 0 marks a copied item and
    /// deeper rows are its children. Kept flat so a deep tree needs no
    /// recursion to build, copy or drop.
    pub nodes: Vec<ClipNode>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ClipNode {
    pub id: i64,
    pub depth: usize,
    pub values: Vec<(String, String)>,
}

pub fn clip(doc: &Document, spec: &TreeSpec, owner: Option<i64>, id: i64, cut: bool) -> Clip {
    let nodes = tree(doc, spec, owner);
    let skip = [Some(spec.id), spec.parent, Some(spec.seq), spec.owner];
    let values = |row: usize| -> Vec<(String, String)> {
        doc.table(spec.table)
            .map(|t| {
                t.fields
                    .iter()
                    .zip(&t.rows[row])
                    .filter(|(f, v)| !skip.contains(&Some(f.as_str())) && !v.is_empty())
                    .map(|(f, v)| (f.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default()
    };
    let copied = match nodes.iter().position(|n| n.id == id) {
        Some(at) => {
            let top = nodes[at].depth;
            std::iter::once(&nodes[at])
                .chain(nodes[at + 1..].iter().take_while(|n| n.depth > top))
                .map(|n| ClipNode {
                    id: n.id,
                    depth: n.depth - top,
                    values: values(n.row),
                })
                .collect()
        }
        None => vec![ClipNode {
            id,
            depth: 0,
            values: vec![],
        }],
    };
    Clip {
        spec: *spec,
        owner,
        cut,
        nodes: copied,
    }
}

/// Paste after `after` in `owner`'s tree. A cut moves the rows (they keep
/// their ids and assignments) and is only allowed inside the same tree; a
/// copy adds new rows, renaming keys that already exist at that level.
pub fn paste(
    doc: &mut Document,
    spec: &TreeSpec,
    owner: Option<i64>,
    after: Option<i64>,
    clip: &Clip,
) -> Result<Vec<i64>> {
    if clip.spec.table != spec.table {
        return Err("The clipboard holds a different kind of item.".into());
    }
    let nodes = tree(doc, spec, owner);
    let parent = after.and_then(|a| nodes.iter().find(|n| n.id == a).and_then(|n| n.parent));
    if clip.cut {
        if clip.owner != owner {
            return Err("Cut items can only be pasted back into the list they came from.".into());
        }
        let mut moved = vec![];
        for root in clip.nodes.iter().filter(|n| n.depth == 0) {
            let inside = subtree(doc, spec, owner, root.id);
            if inside.is_empty() {
                continue;
            }
            if parent.is_some_and(|p| inside.contains(&p)) || after == Some(root.id) {
                return Err("An item cannot be pasted under itself.".into());
            }
            let key = value(doc, spec, root.id, spec.key);
            check_key(doc, spec, owner, parent, Some(root.id), &key, "value")?;
            moved.push(root.id);
        }
        let now = tree(doc, spec, owner);
        let mut order: Vec<i64> = siblings(&now, parent)
            .into_iter()
            .filter(|s| !moved.contains(s))
            .collect();
        let at = after
            .and_then(|a| order.iter().position(|&s| s == a))
            .map_or(order.len(), |p| p + 1);
        for (k, &id) in moved.iter().enumerate() {
            if let Some(column) = spec.parent {
                set_value(
                    doc,
                    spec,
                    id,
                    column,
                    parent.map(|p| p.to_string()).unwrap_or_default(),
                );
            }
            set_value(doc, spec, id, spec.seq, String::new());
            order.insert(at + k, id);
        }
        renumber(doc, spec, &order);
        return Ok(moved);
    }
    // A copy adds rows one by one; if one fails the document is put back.
    let before = doc.clone();
    let pasted = paste_copies(doc, spec, owner, parent, after, clip);
    if pasted.is_err() {
        *doc = before;
    }
    pasted
}

fn paste_copies(
    doc: &mut Document,
    spec: &TreeSpec,
    owner: Option<i64>,
    parent: Option<i64>,
    after: Option<i64>,
    clip: &Clip,
) -> Result<Vec<i64>> {
    let mut added = vec![];
    let mut previous = after;
    // The new id of each copied ancestor of the current row and the last
    // child pasted under it, indexed by depth.
    let mut open: Vec<(i64, Option<i64>)> = vec![];
    for node in &clip.nodes {
        open.truncate(node.depth);
        let id = match open.last_mut() {
            Some((under, last)) if node.depth > 0 => {
                let id = paste_copy(doc, spec, owner, Some(*under), *last, node)?;
                *last = Some(id);
                id
            }
            _ => {
                let id = paste_copy(doc, spec, owner, parent, previous, node)?;
                added.push(id);
                previous = Some(id);
                id
            }
        };
        open.push((id, None));
    }
    Ok(added)
}

/// Add one copied row (without its children) under `parent` after `after`.
fn paste_copy(
    doc: &mut Document,
    spec: &TreeSpec,
    owner: Option<i64>,
    parent: Option<i64>,
    after: Option<i64>,
    node: &ClipNode,
) -> Result<i64> {
    let key = node
        .values
        .iter()
        .find(|(f, _)| f == spec.key)
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    let key = unique_key(doc, spec, owner, parent, &key)?;
    let mut values: Vec<(&str, String)> = node
        .values
        .iter()
        .filter(|(f, _)| f != spec.key && f != "total_assignments")
        .map(|(f, v)| (f.as_str(), v.clone()))
        .collect();
    values.push((spec.key, key));
    if spec.table == "ACTVCODE" {
        values.push(("total_assignments", "0".into()));
    }
    Ok(match (after, parent) {
        (None, Some(p)) => add_child(doc, spec, owner, p, &values),
        _ => add_node(doc, spec, owner, after, &values),
    })
}

fn add_child(
    doc: &mut Document,
    spec: &TreeSpec,
    owner: Option<i64>,
    parent: i64,
    values: &[(&str, String)],
) -> i64 {
    let nodes = tree(doc, spec, owner);
    let children = siblings(&nodes, Some(parent));
    match children.last() {
        Some(&last) => add_node(doc, spec, owner, Some(last), values),
        None => {
            let id = records::next_id(doc, spec.table, spec.id);
            let mut row: Vec<(&str, String)> =
                vec![(spec.id, id.to_string()), (spec.seq, "0".into())];
            if let (Some(column), Some(owner)) = (spec.owner, owner) {
                row.push((column, owner.to_string()));
            }
            if let Some(column) = spec.parent {
                row.push((column, parent.to_string()));
            }
            row.extend(values.iter().cloned());
            records::add_row(doc, spec.table, &row);
            id
        }
    }
}

// ------------------------------------------------------------ code dictionaries

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeKind {
    Activity,
    Project,
    Resource,
    /// Resource assignment codes.
    Assignment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Global,
    Eps,
    Project,
}

/// Default and largest code value length (the ID length limit).
pub const CODE_LENGTH: usize = 20;

/// Tables and columns of one code dictionary.
pub struct CodeTables {
    pub types: &'static str,
    pub type_id: &'static str,
    pub type_name: &'static str,
    pub type_len: &'static str,
    pub value_name: &'static str,
    pub value: TreeSpec,
    pub assign: &'static str,
    pub assign_value: &'static str,
}

impl CodeKind {
    pub fn title(self) -> &'static str {
        match self {
            CodeKind::Activity => "Activity Codes",
            CodeKind::Project => "Project Codes",
            CodeKind::Resource => "Resource Codes",
            CodeKind::Assignment => "Assignment Codes",
        }
    }
    pub fn singular(self) -> &'static str {
        match self {
            CodeKind::Activity => "Activity Code",
            CodeKind::Project => "Project Code",
            CodeKind::Resource => "Resource Code",
            CodeKind::Assignment => "Assignment Code",
        }
    }
    /// What an assignment is attached to, for messages.
    pub fn owners(self) -> &'static str {
        match self {
            CodeKind::Activity => "activities",
            CodeKind::Project => "projects",
            CodeKind::Resource => "resources",
            CodeKind::Assignment => "resource assignments",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            CodeKind::Activity => "activity",
            CodeKind::Project => "project",
            CodeKind::Resource => "resource",
            CodeKind::Assignment => "assignment",
        }
    }
    pub fn tables(self) -> CodeTables {
        match self {
            CodeKind::Activity => CodeTables {
                types: "ACTVTYPE",
                type_id: "actv_code_type_id",
                type_name: "actv_code_type",
                type_len: "actv_short_len",
                value_name: "actv_code_name",
                value: TreeSpec {
                    table: "ACTVCODE",
                    id: "actv_code_id",
                    parent: Some("parent_actv_code_id"),
                    seq: "seq_num",
                    key: "short_name",
                    owner: Some("actv_code_type_id"),
                    key_len: None,
                },
                assign: "TASKACTV",
                assign_value: "actv_code_id",
            },
            CodeKind::Project => CodeTables {
                types: "PCATTYPE",
                type_id: "proj_catg_type_id",
                type_name: "proj_catg_type",
                type_len: "proj_catg_short_len",
                value_name: "proj_catg_name",
                value: TreeSpec {
                    table: "PCATVAL",
                    id: "proj_catg_id",
                    parent: Some("parent_proj_catg_id"),
                    seq: "seq_num",
                    key: "proj_catg_short_name",
                    owner: Some("proj_catg_type_id"),
                    key_len: None,
                },
                assign: "PROJPCAT",
                assign_value: "proj_catg_id",
            },
            CodeKind::Resource => CodeTables {
                types: "RCATTYPE",
                type_id: "rsrc_catg_type_id",
                type_name: "rsrc_catg_type",
                type_len: "rsrc_catg_short_len",
                value_name: "rsrc_catg_name",
                value: TreeSpec {
                    table: "RCATVAL",
                    id: "rsrc_catg_id",
                    parent: Some("parent_rsrc_catg_id"),
                    seq: "seq_num",
                    key: "rsrc_catg_short_name",
                    owner: Some("rsrc_catg_type_id"),
                    key_len: None,
                },
                assign: "RSRCRCAT",
                assign_value: "rsrc_catg_id",
            },
            CodeKind::Assignment => CodeTables {
                types: "ASGNMNTCATTYPE",
                type_id: "asgnmnt_catg_type_id",
                type_name: "asgnmnt_catg_type",
                type_len: "asgnmnt_catg_short_len",
                value_name: "asgnmnt_catg_name",
                value: TreeSpec {
                    table: "ASGNMNTCATVAL",
                    id: "asgnmnt_catg_id",
                    parent: Some("parent_asgnmnt_catg_id"),
                    seq: "seq_num",
                    key: "asgnmnt_catg_short_name",
                    owner: Some("asgnmnt_catg_type_id"),
                    key_len: None,
                },
                assign: "ASGNMNTACAT",
                assign_value: "asgnmnt_catg_id",
            },
        }
    }
    /// The code types as a list with their own order.
    fn type_spec(self) -> TreeSpec {
        let t = self.tables();
        TreeSpec {
            table: t.types,
            id: t.type_id,
            parent: None,
            seq: "seq_num",
            key: t.type_name,
            owner: None,
            key_len: Some(60),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CodeType {
    pub id: i64,
    pub name: String,
    pub max_len: usize,
    pub scope: Scope,
}

fn type_scope(scope: &str, proj: &str) -> Scope {
    match scope {
        "AS_Global" => Scope::Global,
        "AS_EPS" => Scope::Eps,
        "AS_Project" => Scope::Project,
        _ if proj.trim().is_empty() => Scope::Global,
        _ => Scope::Project,
    }
}

fn make_type(doc: &Document, kind: CodeKind, row: usize, id: i64) -> CodeType {
    let t = kind.tables();
    let get = |c: &str| cell(doc, t.types, row, c);
    CodeType {
        id,
        name: get(t.type_name),
        max_len: match num(&get(t.type_len)) {
            n if n > 0 => n as usize,
            _ => CODE_LENGTH,
        },
        scope: if kind == CodeKind::Activity {
            type_scope(&get("actv_code_type_scope"), &get("proj_id"))
        } else {
            Scope::Global
        },
    }
}

/// The code types of a dictionary in their order. Activity code types are
/// filtered to one scope; project scope shows only the open project's types.
pub fn code_types(doc: &Document, kind: CodeKind, scope: Scope, pid: i64) -> Vec<CodeType> {
    let t = kind.tables();
    tree(doc, &kind.type_spec(), None)
        .into_iter()
        .map(|n| (make_type(doc, kind, n.row, n.id), n.row))
        .filter(|(c, row)| match kind {
            CodeKind::Activity => {
                c.scope == scope
                    && (scope != Scope::Project || num(&cell(doc, t.types, *row, "proj_id")) == pid)
            }
            _ => true,
        })
        .map(|(c, _)| c)
        .collect()
}

pub fn code_type(doc: &Document, kind: CodeKind, type_id: i64) -> Option<CodeType> {
    let row = row_of(doc, &kind.type_spec(), type_id)?;
    Some(make_type(doc, kind, row, type_id))
}

/// The EPS node an XER names for the project (the parent of its
/// project-level WBS node; empty for a project made in OpenXER). The app
/// files EPS codes under the workspace EPS node instead
/// ([`add_code_type_at`]).
pub fn eps_id(doc: &Document, pid: i64) -> String {
    doc.table("PROJWBS")
        .and_then(|t| {
            t.rows
                .iter()
                .find(|r| num(t.value(r, "proj_id")) == pid && t.value(r, "proj_node_flag") == "Y")
                .map(|r| t.value(r, "parent_wbs_id").to_string())
        })
        .unwrap_or_default()
}

fn type_names_clash(doc: &Document, kind: CodeKind, except: Option<i64>, name: &str) -> bool {
    let t = kind.tables();
    doc.table(t.types).is_some_and(|table| {
        table.rows.iter().any(|r| {
            Some(num(table.value(r, t.type_id))) != except
                && table
                    .value(r, t.type_name)
                    .trim()
                    .eq_ignore_ascii_case(name.trim())
        })
    })
}

fn check_type_name(doc: &Document, kind: CodeKind, except: Option<i64>, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(format!(
            "The {} name cannot be blank.",
            kind.singular().to_lowercase()
        ));
    }
    if name.chars().count() > 60 {
        return Err("The name can be at most 60 characters long.".into());
    }
    if type_names_clash(doc, kind, except, name) {
        return Err(format!("A code named {name} already exists."));
    }
    Ok(())
}

/// Add a code type at the end of the list of its scope. An EPS code goes
/// under the EPS node the file names for the project ([`eps_id`]).
#[cfg(test)]
pub fn add_code_type(
    doc: &mut Document,
    kind: CodeKind,
    scope: Scope,
    pid: i64,
    name: Option<&str>,
) -> Result<i64> {
    add_code_type_at(doc, kind, scope, pid, None, name)
}

/// [`add_code_type`] with an EPS code filed under workspace EPS node `eps`
/// (the node the workspace keeps the project under) when given.
pub fn add_code_type_at(
    doc: &mut Document,
    kind: CodeKind,
    scope: Scope,
    pid: i64,
    eps: Option<i64>,
    name: Option<&str>,
) -> Result<i64> {
    // A typed name must be free; the default one is numbered until it is.
    let mut name = name.unwrap_or("New Code").to_string();
    if name == "New Code" {
        let mut n = 1;
        while type_names_clash(doc, kind, None, &name) {
            n += 1;
            name = format!("New Code {n}");
        }
    }
    check_type_name(doc, kind, None, &name)?;
    let t = kind.tables();
    let spec = kind.type_spec();
    let last = code_types(doc, kind, scope, pid).last().map(|c| c.id);
    let mut values: Vec<(&str, String)> = vec![
        (t.type_name, name.clone()),
        (t.type_len, CODE_LENGTH.to_string()),
    ];
    if kind == CodeKind::Activity {
        let (scope_text, proj, wbs) = match scope {
            Scope::Global => ("AS_Global", String::new(), String::new()),
            Scope::Eps => (
                "AS_EPS",
                String::new(),
                eps.map_or_else(|| eps_id(doc, pid), |e| e.to_string()),
            ),
            Scope::Project => ("AS_Project", pid.to_string(), String::new()),
        };
        values.push(("actv_code_type_scope", scope_text.into()));
        values.push(("proj_id", proj));
        values.push(("wbs_id", wbs));
    }
    // New types go after the last one of the scope, or after every type.
    let after = last.or_else(|| tree(doc, &spec, None).last().map(|n| n.id));
    Ok(add_node(doc, &spec, None, after, &values))
}

pub fn rename_code_type(
    doc: &mut Document,
    kind: CodeKind,
    type_id: i64,
    name: &str,
) -> Result<()> {
    check_type_name(doc, kind, Some(type_id), name)?;
    let spec = kind.type_spec();
    set_value(doc, &spec, type_id, spec.key, records::clean(name.trim()));
    Ok(())
}

/// Change a code type's maximum value length. Values already longer stop it.
pub fn set_code_length(doc: &mut Document, kind: CodeKind, type_id: i64, len: usize) -> Result<()> {
    if !(1..=CODE_LENGTH).contains(&len) {
        return Err(format!("Max Length must be between 1 and {CODE_LENGTH}."));
    }
    let t = kind.tables();
    let longest = code_values(doc, kind, type_id)
        .iter()
        .map(|v| v.short.chars().count())
        .max()
        .unwrap_or(0);
    if longest > len {
        return Err(format!(
            "Code values of {longest} characters exist; Max Length cannot be less."
        ));
    }
    let spec = kind.type_spec();
    set_value(doc, &spec, type_id, t.type_len, len.to_string());
    Ok(())
}

/// Make Global / Make EPS for an activity code type. Assignments stay.
#[cfg(test)]
pub fn set_code_scope(
    doc: &mut Document,
    kind: CodeKind,
    type_id: i64,
    scope: Scope,
    pid: i64,
) -> Result<()> {
    set_code_scope_at(doc, kind, type_id, scope, pid, None)
}

/// [`set_code_scope`] with Make EPS filing the code under workspace EPS
/// node `eps` when given.
pub fn set_code_scope_at(
    doc: &mut Document,
    kind: CodeKind,
    type_id: i64,
    scope: Scope,
    pid: i64,
    eps: Option<i64>,
) -> Result<()> {
    if kind != CodeKind::Activity {
        return Ok(());
    }
    let Some(current) = code_type(doc, kind, type_id) else {
        return Err("The code no longer exists.".into());
    };
    let rank = |s: Scope| match s {
        Scope::Project => 0,
        Scope::Eps => 1,
        Scope::Global => 2,
    };
    if rank(scope) <= rank(current.scope) {
        return Err("A code can only be widened (Project to EPS to Global).".into());
    }
    let spec = kind.type_spec();
    let (text, wbs) = match scope {
        Scope::Global => ("AS_Global", String::new()),
        Scope::Eps => (
            "AS_EPS",
            eps.map_or_else(|| eps_id(doc, pid), |e| e.to_string()),
        ),
        Scope::Project => unreachable!(),
    };
    set_value(doc, &spec, type_id, "actv_code_type_scope", text.into());
    set_value(doc, &spec, type_id, "proj_id", String::new());
    if doc
        .table(spec.table)
        .is_some_and(|t| t.field("wbs_id").is_some())
        || !wbs.is_empty()
    {
        set_value(doc, &spec, type_id, "wbs_id", wbs);
    }
    Ok(())
}

pub fn shift_code_type(
    doc: &mut Document,
    kind: CodeKind,
    scope: Scope,
    pid: i64,
    type_id: i64,
    by: i32,
) -> bool {
    // Shift within the scope's own list, keeping other scopes in place.
    let list: Vec<i64> = code_types(doc, kind, scope, pid)
        .iter()
        .map(|c| c.id)
        .collect();
    let Some(at) = list.iter().position(|&i| i == type_id) else {
        return false;
    };
    let to = at as i64 + by as i64;
    if to < 0 || to as usize >= list.len() {
        return false;
    }
    let other = list[to as usize];
    let spec = kind.type_spec();
    let a = value(doc, &spec, type_id, "seq_num");
    let b = value(doc, &spec, other, "seq_num");
    if num(&a) == num(&b) {
        let all: Vec<i64> = tree(doc, &spec, None).iter().map(|n| n.id).collect();
        let mut order = all.clone();
        let (i, j) = (
            order.iter().position(|&x| x == type_id).unwrap(),
            order.iter().position(|&x| x == other).unwrap(),
        );
        order.swap(i, j);
        for (k, id) in order.iter().enumerate() {
            set_value(doc, &spec, *id, "seq_num", k.to_string());
        }
    } else {
        set_value(doc, &spec, type_id, "seq_num", b);
        set_value(doc, &spec, other, "seq_num", a);
    }
    true
}

/// How many assignments use the code type, and how many values it has.
pub fn code_type_usage(doc: &Document, kind: CodeKind, type_id: i64) -> (usize, usize) {
    let t = kind.tables();
    let values: BTreeSet<i64> = code_values(doc, kind, type_id)
        .iter()
        .map(|v| v.id)
        .collect();
    let assigned = referencing(doc, t.assign, t.type_id, &BTreeSet::from([type_id])).len();
    (assigned, values.len())
}

pub fn delete_code_type(doc: &mut Document, kind: CodeKind, type_id: i64) -> usize {
    let t = kind.tables();
    let values: BTreeSet<i64> = code_values(doc, kind, type_id)
        .iter()
        .map(|v| v.id)
        .collect();
    let rows = referencing(doc, t.assign, t.type_id, &BTreeSet::from([type_id]));
    delete_rows(doc, t.assign, &rows);
    remove_ids(doc, &t.value, &values);
    remove_ids(doc, &kind.type_spec(), &BTreeSet::from([type_id]));
    rows.len()
}

#[derive(Clone, Debug, PartialEq)]
pub struct CodeValue {
    pub id: i64,
    pub parent: Option<i64>,
    pub depth: usize,
    pub has_children: bool,
    pub short: String,
    pub name: String,
    /// `RRGGBB` (activity codes only).
    pub color: String,
}

pub fn code_values(doc: &Document, kind: CodeKind, type_id: i64) -> Vec<CodeValue> {
    let t = kind.tables();
    tree(doc, &t.value, Some(type_id))
        .into_iter()
        .map(|n| CodeValue {
            id: n.id,
            parent: n.parent,
            depth: n.depth,
            has_children: n.has_children,
            short: cell(doc, t.value.table, n.row, t.value.key),
            name: cell(doc, t.value.table, n.row, t.value_name),
            color: if kind == CodeKind::Activity {
                cell(doc, t.value.table, n.row, "color")
            } else {
                String::new()
            },
        })
        .collect()
}

/// The value tree spec with the type's length limit.
pub fn value_spec(doc: &Document, kind: CodeKind, type_id: i64) -> TreeSpec {
    let mut spec = kind.tables().value;
    spec.key_len = code_type(doc, kind, type_id).map(|t| t.max_len);
    spec
}

pub fn add_code_value(
    doc: &mut Document,
    kind: CodeKind,
    type_id: i64,
    after: Option<i64>,
) -> Result<i64> {
    let t = kind.tables();
    let spec = value_spec(doc, kind, type_id);
    let nodes = tree(doc, &spec, Some(type_id));
    let parent = after.and_then(|a| nodes.iter().find(|n| n.id == a).and_then(|n| n.parent));
    let short = unique_key(doc, &spec, Some(type_id), parent, "New")?;
    let mut values: Vec<(&str, String)> = vec![(spec.key, short), (t.value_name, String::new())];
    if kind == CodeKind::Activity {
        values.push(("color", "0000FF".into()));
        values.push(("total_assignments", "0".into()));
    }
    Ok(add_node(doc, &spec, Some(type_id), after, &values))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueField {
    Short,
    Name,
    Color,
}

pub fn set_code_value(
    doc: &mut Document,
    kind: CodeKind,
    type_id: i64,
    id: i64,
    field: ValueField,
    text: &str,
) -> Result<()> {
    let t = kind.tables();
    let spec = value_spec(doc, kind, type_id);
    let text = records::clean(text.trim());
    match field {
        ValueField::Short => {
            let parent = tree(doc, &spec, Some(type_id))
                .into_iter()
                .find(|n| n.id == id)
                .and_then(|n| n.parent);
            check_key(
                doc,
                &spec,
                Some(type_id),
                parent,
                Some(id),
                &text,
                "code value",
            )?;
            set_value(doc, &spec, id, spec.key, text);
        }
        ValueField::Name => set_value(doc, &spec, id, t.value_name, text),
        ValueField::Color => {
            let ok = text.len() == 6 && text.chars().all(|c| c.is_ascii_hexdigit());
            if !ok {
                return Err("A color is six hexadecimal digits (RRGGBB).".into());
            }
            set_value(doc, &spec, id, "color", text.to_ascii_uppercase());
        }
    }
    Ok(())
}

/// Assignments that deleting `id` (with its children) would remove.
pub fn code_value_usage(doc: &Document, kind: CodeKind, type_id: i64, id: i64) -> usize {
    let t = kind.tables();
    let ids: BTreeSet<i64> = subtree(doc, &t.value, Some(type_id), id)
        .into_iter()
        .collect();
    referencing(doc, t.assign, t.assign_value, &ids).len()
}

/// Delete a value with its children and their assignments. Returns the
/// number of assignments removed.
pub fn delete_code_value(doc: &mut Document, kind: CodeKind, type_id: i64, id: i64) -> usize {
    let t = kind.tables();
    let ids: BTreeSet<i64> = subtree(doc, &t.value, Some(type_id), id)
        .into_iter()
        .collect();
    let rows = referencing(doc, t.assign, t.assign_value, &ids);
    delete_rows(doc, t.assign, &rows);
    remove_ids(doc, &t.value, &ids);
    rows.len()
}

// ------------------------------------------------------------ user defined fields

/// Subject areas and the table each one's fields belong to.
pub const SUBJECT_AREAS: [(&str, &str); 10] = [
    ("Activities", "TASK"),
    ("Activity Expenses", "PROJCOST"),
    ("Activity Steps", "TASKPROC"),
    ("Issues", "PROJISSU"),
    ("Project Risks", "PROJRISK"),
    ("Projects", "PROJECT"),
    ("Resource Assignments", "TASKRSRC"),
    ("Resources", "RSRC"),
    ("WBS", "PROJWBS"),
    ("Work Products and Documents", "DOCUMENT"),
];

/// Data types in the order of the Data Type list, with the stored type.
pub const DATA_TYPES: [(&str, &str); 8] = [
    ("Text", "FT_TEXT"),
    ("Start Date", "FT_START_DATE"),
    ("Finish Date", "FT_END_DATE"),
    ("Cost", "FT_MONEY"),
    ("Integer", "FT_INT"),
    ("Number", "FT_FLOAT_2_DECIMALS"),
    ("Indicator", "FT_STATICTYPE"),
    ("Code", "FT_CODE"),
];

pub fn data_type_label(logical: &str) -> &'static str {
    DATA_TYPES
        .iter()
        .find(|(_, l)| *l == logical)
        .map(|(label, _)| *label)
        .unwrap_or(match logical {
            "FT_FLOAT" => "Number",
            "FT_DATE" => "Start Date",
            _ => "Text",
        })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Udf {
    pub id: i64,
    pub title: String,
    pub logical: String,
    pub values: usize,
}

const UDF_SPEC: TreeSpec = TreeSpec {
    table: "UDFTYPE",
    id: "udf_type_id",
    parent: None,
    seq: "udf_type_id",
    key: "udf_type_label",
    owner: None,
    key_len: Some(60),
};

fn udf_title(doc: &Document, row: usize) -> String {
    let label = cell(doc, "UDFTYPE", row, "udf_type_label");
    if label.is_empty() {
        cell(doc, "UDFTYPE", row, "udf_type_name")
    } else {
        label
    }
}

/// Fields of one subject area, sorted by title.
pub fn udfs(doc: &Document, table: &str) -> Vec<Udf> {
    let Some(t) = doc.table("UDFTYPE") else {
        return vec![];
    };
    let counts = udf_value_counts(doc);
    let mut out: Vec<Udf> = t
        .rows
        .iter()
        .enumerate()
        .filter(|(_, r)| t.value(r, "table_name") == table)
        .map(|(i, r)| {
            let id = num(t.value(r, "udf_type_id"));
            Udf {
                id,
                title: udf_title(doc, i),
                logical: t.value(r, "logical_data_type").to_string(),
                values: counts.get(&id).copied().unwrap_or(0),
            }
        })
        .collect();
    out.sort_by_key(|a| (a.title.to_lowercase(), a.id));
    out
}

fn udf_value_counts(doc: &Document) -> std::collections::HashMap<i64, usize> {
    let mut counts = std::collections::HashMap::new();
    if let Some(t) = doc.table("UDFVALUE") {
        for r in &t.rows {
            *counts.entry(num(t.value(r, "udf_type_id"))).or_insert(0) += 1;
        }
    }
    counts
}

fn udf_table(doc: &Document, id: i64) -> String {
    value(doc, &UDF_SPEC, id, "table_name")
}

fn check_udf_title(doc: &Document, table: &str, except: Option<i64>, title: &str) -> Result<()> {
    let title = title.trim();
    if title.is_empty() {
        return Err("The title cannot be blank.".into());
    }
    if title.chars().count() > 60 {
        return Err("The title can be at most 60 characters long.".into());
    }
    if udfs(doc, table)
        .iter()
        .any(|u| Some(u.id) != except && u.title.trim().eq_ignore_ascii_case(title))
    {
        return Err(format!("A field titled {title} already exists."));
    }
    Ok(())
}

/// Add a Text field titled `(New)` (made unique) to the subject area.
pub fn add_udf(doc: &mut Document, table: &str) -> i64 {
    let mut title = "(New)".to_string();
    let mut n = 1;
    while check_udf_title(doc, table, None, &title).is_err() {
        n += 1;
        title = format!("(New) {n}");
    }
    let id = records::next_id(doc, "UDFTYPE", "udf_type_id");
    records::add_row(
        doc,
        "UDFTYPE",
        &[
            ("udf_type_id", id.to_string()),
            ("table_name", table.to_string()),
            ("udf_type_name", format!("user_field_{id}")),
            ("udf_type_label", title),
            ("logical_data_type", "FT_TEXT".into()),
            ("super_flag", "N".into()),
        ],
    );
    id
}

pub fn set_udf_title(doc: &mut Document, id: i64, title: &str) -> Result<()> {
    let table = udf_table(doc, id);
    check_udf_title(doc, &table, Some(id), title)?;
    set_value(
        doc,
        &UDF_SPEC,
        id,
        "udf_type_label",
        records::clean(title.trim()),
    );
    Ok(())
}

/// The data type can change only while the field holds no values.
pub fn set_udf_type(doc: &mut Document, id: i64, logical: &str) -> Result<()> {
    if !DATA_TYPES.iter().any(|(_, l)| *l == logical) {
        return Err(format!("Unknown data type {logical}."));
    }
    if value(doc, &UDF_SPEC, id, "logical_data_type") == logical {
        return Ok(());
    }
    if udf_value_counts(doc).get(&id).copied().unwrap_or(0) > 0 {
        return Err("The data type cannot change once the field holds values.".into());
    }
    set_value(doc, &UDF_SPEC, id, "logical_data_type", logical.into());
    Ok(())
}

/// Delete the field, its values and its code values. Returns the values removed.
pub fn delete_udf(doc: &mut Document, id: i64) -> usize {
    let ids = BTreeSet::from([id]);
    let rows = referencing(doc, "UDFVALUE", "udf_type_id", &ids);
    delete_rows(doc, "UDFVALUE", &rows);
    let codes = referencing(doc, "UDFCODE", "udf_type_id", &ids);
    delete_rows(doc, "UDFCODE", &codes);
    remove_ids(doc, &UDF_SPEC, &ids);
    rows.len()
}

// ------------------------------------------------------------ cost accounts

pub const ACCOUNT: TreeSpec = TreeSpec {
    table: "ACCOUNT",
    id: "acct_id",
    parent: Some("parent_acct_id"),
    seq: "acct_seq_num",
    key: "acct_short_name",
    owner: None,
    key_len: Some(40),
};

/// Rows that name a cost account.
const ACCOUNT_REFS: [(&str, &str); 3] = [
    ("PROJCOST", "acct_id"),
    ("TASKRSRC", "acct_id"),
    ("PROJECT", "acct_id"),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Account {
    pub id: i64,
    pub parent: Option<i64>,
    pub depth: usize,
    pub has_children: bool,
    pub short: String,
    pub name: String,
}

pub fn accounts(doc: &Document) -> Vec<Account> {
    tree(doc, &ACCOUNT, None)
        .into_iter()
        .map(|n| Account {
            id: n.id,
            parent: n.parent,
            depth: n.depth,
            has_children: n.has_children,
            short: cell(doc, "ACCOUNT", n.row, "acct_short_name"),
            name: cell(doc, "ACCOUNT", n.row, "acct_name"),
        })
        .collect()
}

pub fn add_account(doc: &mut Document, after: Option<i64>) -> Result<i64> {
    let nodes = tree(doc, &ACCOUNT, None);
    let parent = after.and_then(|a| nodes.iter().find(|n| n.id == a).and_then(|n| n.parent));
    let short = unique_key(doc, &ACCOUNT, None, parent, "New Account")?;
    Ok(add_node(
        doc,
        &ACCOUNT,
        None,
        after,
        &[("acct_short_name", short.clone()), ("acct_name", short)],
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountField {
    Id,
    Name,
    Description,
}

pub fn set_account(doc: &mut Document, id: i64, field: AccountField, text: &str) -> Result<()> {
    match field {
        AccountField::Id => {
            let parent = tree(doc, &ACCOUNT, None)
                .into_iter()
                .find(|n| n.id == id)
                .and_then(|n| n.parent);
            let text = records::clean(text.trim());
            check_key(
                doc,
                &ACCOUNT,
                None,
                parent,
                Some(id),
                &text,
                "cost account ID",
            )?;
            set_value(doc, &ACCOUNT, id, "acct_short_name", text);
        }
        AccountField::Name => {
            let text = records::clean(text.trim());
            if text.is_empty() {
                return Err("The cost account name cannot be blank.".into());
            }
            set_value(doc, &ACCOUNT, id, "acct_name", text);
        }
        AccountField::Description => {
            let memo = if text.trim().is_empty() {
                String::new()
            } else {
                records::text_to_memo(text)
            };
            set_value(doc, &ACCOUNT, id, "acct_descr", memo);
        }
    }
    Ok(())
}

pub fn account_description(doc: &Document, id: i64) -> String {
    records::memo_to_text(&value(doc, &ACCOUNT, id, "acct_descr"))
}

/// Rows that use the account or one of its children.
pub fn account_usage(doc: &Document, id: i64) -> usize {
    let ids: BTreeSet<i64> = subtree(doc, &ACCOUNT, None, id).into_iter().collect();
    ACCOUNT_REFS
        .iter()
        .map(|(t, c)| referencing(doc, t, c, &ids).len())
        .sum()
}

/// Delete the account and its children. Rows that used them move to
/// `replacement`, or are left without a cost account.
pub fn delete_account(doc: &mut Document, id: i64, replacement: Option<i64>) -> Result<usize> {
    let ids: BTreeSet<i64> = subtree(doc, &ACCOUNT, None, id).into_iter().collect();
    if replacement.is_some_and(|r| ids.contains(&r)) {
        return Err("The replacement cannot be an account being deleted.".into());
    }
    let text = replacement.map(|r| r.to_string()).unwrap_or_default();
    let mut moved = 0;
    for (table, column) in ACCOUNT_REFS {
        for r in referencing(doc, table, column, &ids) {
            records::set(doc, table, r, column, text.clone());
            moved += 1;
        }
    }
    remove_ids(doc, &ACCOUNT, &ids);
    Ok(moved)
}

// ------------------------------------------------------------ admin categories

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Baseline,
    Expense,
    Documents,
    DocStatus,
    Risk,
    Notebook,
    Units,
}

/// What deleting a category does to the rows that use it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnDelete {
    /// Leave the rows, blank the column.
    Clear,
    /// Remove the rows (notebooks under a deleted topic).
    Remove,
}

pub struct CategorySpec {
    pub tree: TreeSpec,
    /// Grid columns: caption, table column, width. Checkbox columns are flags.
    pub columns: &'static [(&'static str, &'static str, f32)],
    pub refs: &'static [(&'static str, &'static str)],
    pub on_delete: OnDelete,
    pub new_name: &'static str,
}

impl Category {
    pub const ALL: [Category; 7] = [
        Category::Baseline,
        Category::Expense,
        Category::Documents,
        Category::DocStatus,
        Category::Risk,
        Category::Notebook,
        Category::Units,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Category::Baseline => "&Baseline Types",
            Category::Expense => "&Expense Categories",
            Category::Documents => "Document &Categories",
            Category::DocStatus => "&Document Status",
            Category::Risk => "&Risk Categories",
            Category::Notebook => "&Notebook Topics",
            Category::Units => "&Units of Measure",
        }
    }
    /// Page heading and the grid's `Display:` caption.
    pub fn heading(self) -> (&'static str, &'static str) {
        match self {
            Category::Baseline => ("Baseline Types", "Baseline Types"),
            Category::Expense => ("Expense Categories", "Expense Categories"),
            Category::Documents => ("Document Categories", "Document Categories"),
            Category::DocStatus => ("Document Status Codes", "Document Status Codes"),
            Category::Risk => ("Risk Categories", "Risk Categories"),
            Category::Notebook => ("Notebook", "Notebook Topics"),
            Category::Units => ("Units Of Measure", "Units of Measure"),
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Category::Baseline => "baseline",
            Category::Expense => "expense",
            Category::Documents => "doccatg",
            Category::DocStatus => "docstat",
            Category::Risk => "risk",
            Category::Notebook => "notebook",
            Category::Units => "units",
        }
    }
    /// The XER table, grid columns and references of the page.
    pub fn spec(self) -> CategorySpec {
        let flat = |table, id, key| TreeSpec {
            table,
            id,
            parent: None,
            seq: "seq_num",
            key,
            owner: None,
            key_len: Some(60),
        };
        match self {
            Category::Baseline => CategorySpec {
                tree: TreeSpec {
                    seq: "base_type_seq_num",
                    key_len: Some(40),
                    ..flat("BASETYPE", "base_type_id", "base_type")
                },
                columns: &[("Baseline Type", "base_type", 280.)],
                refs: &[("PROJECT", "base_type_id")],
                on_delete: OnDelete::Clear,
                new_name: "New Baseline Type",
            },
            Category::Expense => CategorySpec {
                tree: flat("COSTTYPE", "cost_type_id", "cost_type"),
                columns: &[("Expense Category", "cost_type", 280.)],
                refs: &[("PROJCOST", "cost_type_id")],
                on_delete: OnDelete::Clear,
                new_name: "New Expense Category",
            },
            Category::Documents => CategorySpec {
                tree: flat("DOCCATG", "doc_catg_id", "doc_catg_name"),
                columns: &[("Document Category", "doc_catg_name", 280.)],
                refs: &[("DOCUMENT", "doc_catg_id")],
                on_delete: OnDelete::Clear,
                new_name: "New Document Category",
            },
            Category::DocStatus => CategorySpec {
                tree: flat("DOCSTAT", "doc_status_id", "doc_status_code"),
                columns: &[("Document Status Code", "doc_status_code", 280.)],
                refs: &[("DOCUMENT", "doc_status_id")],
                on_delete: OnDelete::Clear,
                new_name: "New Status",
            },
            Category::Risk => CategorySpec {
                tree: TreeSpec {
                    parent: Some("parent_risk_type_id"),
                    ..flat("RISKTYPE", "risk_type_id", "risk_type")
                },
                columns: &[("Risk Category", "risk_type", 280.)],
                refs: &[("PROJRISK", "risk_type_id")],
                on_delete: OnDelete::Clear,
                new_name: "New Risk Category",
            },
            Category::Notebook => CategorySpec {
                tree: flat("MEMOTYPE", "memo_type_id", "memo_type"),
                columns: &[
                    ("Notebook Topic", "memo_type", 120.),
                    ("EPS", "eps_flag", 44.),
                    ("Proj", "proj_flag", 44.),
                    ("WBS", "wbs_flag", 44.),
                    ("Act", "task_flag", 44.),
                ],
                refs: &[("TASKMEMO", "memo_type_id"), ("WBSMEMO", "memo_type_id")],
                on_delete: OnDelete::Remove,
                new_name: "New Topic",
            },
            Category::Units => CategorySpec {
                tree: flat("UMEASURE", "unit_id", "unit_abbrev"),
                columns: &[
                    ("Unit Abbreviation", "unit_abbrev", 120.),
                    ("Unit Name", "unit_name", 160.),
                ],
                refs: &[("RSRC", "unit_id")],
                on_delete: OnDelete::Clear,
                new_name: "unit",
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CategoryRow {
    pub id: i64,
    pub depth: usize,
    pub cells: Vec<String>,
}

pub fn categories(doc: &Document, category: Category) -> Vec<CategoryRow> {
    let spec = category.spec();
    tree(doc, &spec.tree, None)
        .into_iter()
        .map(|n| CategoryRow {
            id: n.id,
            depth: n.depth,
            cells: spec
                .columns
                .iter()
                .map(|(_, c, _)| cell(doc, spec.tree.table, n.row, c))
                .collect(),
        })
        .collect()
}

pub fn add_category(
    doc: &mut Document,
    category: Category,
    after: Option<i64>,
) -> Result<Option<i64>> {
    let spec = category.spec();
    let nodes = tree(doc, &spec.tree, None);
    let parent = after.and_then(|a| nodes.iter().find(|n| n.id == a).and_then(|n| n.parent));
    let name = unique_key(doc, &spec.tree, None, parent, spec.new_name)?;
    let mut values: Vec<(&str, String)> = vec![(spec.tree.key, name.clone())];
    match category {
        // New topics are offered at every level by default.
        Category::Notebook => {
            for flag in ["eps_flag", "proj_flag", "wbs_flag", "task_flag"] {
                values.push((flag, "Y".into()));
            }
        }
        Category::Units => values.push(("unit_name", name)),
        _ => {}
    }
    Ok(Some(add_node(doc, &spec.tree, None, after, &values)))
}

/// Edit one cell: the name columns are checked, flag columns take `Y`/`N`.
pub fn set_category(
    doc: &mut Document,
    category: Category,
    id: i64,
    column: &str,
    text: &str,
) -> Result<()> {
    let spec = category.spec();
    let text = records::clean(text.trim());
    if column.ends_with("_flag") {
        let flag = if text == "Y" { "Y" } else { "N" };
        set_value(doc, &spec.tree, id, column, flag.into());
        return Ok(());
    }
    if column == spec.tree.key {
        let parent = tree(doc, &spec.tree, None)
            .into_iter()
            .find(|n| n.id == id)
            .and_then(|n| n.parent);
        let (_, caption) = category.heading();
        let noun = caption.trim_end_matches('s').to_lowercase();
        check_key(doc, &spec.tree, None, parent, Some(id), &text, &noun)?;
    }
    set_value(doc, &spec.tree, id, column, text);
    Ok(())
}

/// Rows that use the category or one of its children.
pub fn category_usage(doc: &Document, category: Category, id: i64) -> usize {
    let spec = category.spec();
    let ids: BTreeSet<i64> = subtree(doc, &spec.tree, None, id).into_iter().collect();
    spec.refs
        .iter()
        .map(|(t, c)| referencing(doc, ref_table(doc, t), c, &ids).len())
        .sum()
}

/// The table a category's rows live in: the risk register may be stored
/// under either of its two table names.
fn ref_table(doc: &Document, table: &'static str) -> &'static str {
    if table == "PROJRISK" {
        records::risk_table(doc)
    } else {
        table
    }
}

/// Delete the category (with its children). Returns the rows changed.
pub fn delete_category(doc: &mut Document, category: Category, id: i64) -> usize {
    let spec = category.spec();
    let ids: BTreeSet<i64> = subtree(doc, &spec.tree, None, id).into_iter().collect();
    let mut changed = 0;
    for (table, column) in spec.refs {
        let table = ref_table(doc, table);
        let rows = referencing(doc, table, column, &ids);
        changed += rows.len();
        match spec.on_delete {
            OnDelete::Remove => delete_rows(doc, table, &rows),
            OnDelete::Clear => {
                for r in rows {
                    records::set(doc, table, r, column, String::new());
                }
            }
        }
    }
    remove_ids(doc, &spec.tree, &ids);
    changed
}

pub fn category_tree(category: Category) -> TreeSpec {
    category.spec().tree
}
