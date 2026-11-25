//! Tree tables (EPS, OBS, roles, role code values, funding sources): rows
//! linked by a parent column and ordered by a sequence column among their
//! siblings. Every function works on a `Document`, so the same rules serve
//! the project's XER tables and the workspace's own table of EPS nodes.
use crate::edit::records::{self, num};
use crate::{Document, Table};
use std::collections::{BTreeMap, BTreeSet};

pub type Result<T> = std::result::Result<T, String>;

/// How a tree table is laid out and validated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeSpec {
    pub table: &'static str,
    pub id: &'static str,
    pub parent: &'static str,
    pub seq: &'static str,
    /// The name column: required, unique among siblings.
    pub name: &'static str,
    /// An ID column unique in the whole tree (EPS ID, Role ID, code value).
    pub code: Option<&'static str>,
    /// The column that splits one table into several trees (code type).
    pub owner: Option<&'static str>,
    /// Deepest level allowed, counting the top level as 1.
    pub max_depth: usize,
    /// Longest name or ID accepted.
    pub max_len: usize,
    /// What one row is called in messages.
    pub noun: &'static str,
    /// Name given to a new row (a number is added when it is taken).
    pub new_name: &'static str,
    /// Start of the ID given to a new row.
    pub code_prefix: &'static str,
    /// Where Add and Paste put a new row: under the selected row (EPS, OBS,
    /// roles, funding sources) or after it (code values).
    pub add_child: bool,
}

/// One row of a tree, in display order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub id: i64,
    pub parent: Option<i64>,
    pub depth: usize,
    pub row: usize,
}

fn table<'a>(doc: &'a Document, spec: &TreeSpec) -> Option<&'a Table> {
    doc.table(spec.table)
}

fn owned(t: &Table, r: &[String], spec: &TreeSpec, owner: Option<i64>) -> bool {
    match (spec.owner, owner) {
        (Some(column), Some(owner)) => num(t.value(r, column)) == owner,
        _ => true,
    }
}

/// Rows of the tree (one owner's tree when the table has several) in
/// display order: siblings by sequence then ID, children under parents.
/// Rows whose parent is missing are shown at the top level; a parent loop is
/// broken where it is met.
pub fn nodes(doc: &Document, spec: &TreeSpec, owner: Option<i64>) -> Vec<Node> {
    let Some(t) = table(doc, spec) else {
        return vec![];
    };
    let mut rows: Vec<(i64, Option<i64>, i64, usize)> = vec![];
    for (i, r) in t.rows.iter().enumerate() {
        if !owned(t, r, spec, owner) {
            continue;
        }
        let id = num(t.value(r, spec.id));
        if id == 0 {
            continue;
        }
        let parent = Some(num(t.value(r, spec.parent))).filter(|p| *p != 0 && *p != id);
        rows.push((id, parent, num(t.value(r, spec.seq)), i));
    }
    let ids: BTreeSet<i64> = rows.iter().map(|r| r.0).collect();
    let mut children: BTreeMap<Option<i64>, Vec<(i64, i64, usize)>> = BTreeMap::new();
    for (id, parent, seq, row) in &rows {
        let parent = parent.filter(|p| ids.contains(p));
        children.entry(parent).or_default().push((*seq, *id, *row));
    }
    for list in children.values_mut() {
        list.sort();
    }
    let mut out = vec![];
    let mut seen = BTreeSet::new();
    // Depth-first with an explicit stack (an imported file can nest deeply),
    // children in display order under their parent.
    fn walk(
        root: Option<i64>,
        depth: usize,
        children: &BTreeMap<Option<i64>, Vec<(i64, i64, usize)>>,
        seen: &mut BTreeSet<i64>,
        out: &mut Vec<Node>,
    ) {
        let mut stack: Vec<(Option<i64>, usize, usize)> = vec![(root, depth, 0)];
        while let Some((parent, depth, next)) = stack.pop() {
            let Some(list) = children.get(&parent) else {
                continue;
            };
            let Some(&(_, id, row)) = list.get(next) else {
                continue;
            };
            stack.push((parent, depth, next + 1));
            if !seen.insert(id) {
                continue;
            }
            out.push(Node {
                id,
                parent,
                depth,
                row,
            });
            stack.push((Some(id), depth + 1, 0));
        }
    }
    walk(None, 0, &children, &mut seen, &mut out);
    // Rows caught in a parent loop: show them at the top level.
    for (id, _, _, row) in &rows {
        if !seen.contains(id) {
            seen.insert(*id);
            out.push(Node {
                id: *id,
                parent: None,
                depth: 0,
                row: *row,
            });
            walk(Some(*id), 1, &children, &mut seen, &mut out);
        }
    }
    out
}

/// A row's value by ID.
pub fn value(doc: &Document, spec: &TreeSpec, id: i64, column: &str) -> String {
    table(doc, spec)
        .and_then(|t| {
            t.rows
                .iter()
                .find(|r| num(t.value(r, spec.id)) == id)
                .map(|r| t.value(r, column).to_string())
        })
        .unwrap_or_default()
}

fn row_of(doc: &Document, spec: &TreeSpec, id: i64) -> Option<usize> {
    let t = table(doc, spec)?;
    t.rows.iter().position(|r| num(t.value(r, spec.id)) == id)
}

fn set(doc: &mut Document, spec: &TreeSpec, id: i64, column: &str, text: String) {
    if let Some(i) = row_of(doc, spec, id) {
        doc.table_mut(spec.table).set(i, column, text);
    }
}

/// The node, its children and their children, in display order.
pub fn subtree(doc: &Document, spec: &TreeSpec, owner: Option<i64>, id: i64) -> Vec<i64> {
    let all = nodes(doc, spec, owner);
    let Some(start) = all.iter().position(|n| n.id == id) else {
        return vec![];
    };
    let depth = all[start].depth;
    std::iter::once(all[start].id)
        .chain(
            all[start + 1..]
                .iter()
                .take_while(|n| n.depth > depth)
                .map(|n| n.id),
        )
        .collect()
}

fn depth_of(all: &[Node], id: i64) -> usize {
    all.iter().find(|n| n.id == id).map_or(0, |n| n.depth)
}

/// Levels below the node, counting the node itself as 1.
fn height(all: &[Node], id: i64) -> usize {
    let Some(start) = all.iter().position(|n| n.id == id) else {
        return 0;
    };
    let depth = all[start].depth;
    all[start..]
        .iter()
        .take_while(|n| n.id == id || n.depth > depth)
        .map(|n| n.depth - depth + 1)
        .max()
        .unwrap_or(1)
}

/// Checks a name or ID typed for a row and returns it trimmed: not empty, no
/// control characters, not too long.
pub fn clean_text(text: &str, max_len: usize, what: &str) -> Result<String> {
    let t = text.trim();
    if t.is_empty() {
        return Err(format!("The {what} cannot be blank."));
    }
    if t.chars().any(char::is_control) {
        return Err(format!("The {what} cannot contain control characters."));
    }
    if t.chars().count() > max_len {
        return Err(format!(
            "The {what} cannot be longer than {max_len} characters."
        ));
    }
    Ok(t.to_string())
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// Checks a new name for row `id` (or a new row when `id` is `None`) under
/// `parent`: unique among its siblings.
pub fn check_name(
    doc: &Document,
    spec: &TreeSpec,
    owner: Option<i64>,
    parent: Option<i64>,
    id: Option<i64>,
    text: &str,
) -> Result<String> {
    let name = clean_text(text, spec.max_len, &format!("{} name", spec.noun))?;
    let clash = nodes(doc, spec, owner).iter().any(|n| {
        n.parent == parent && Some(n.id) != id && same(&value(doc, spec, n.id, spec.name), &name)
    });
    if clash {
        return Err(format!(
            "Another {} at this level is already named \"{name}\".",
            spec.noun
        ));
    }
    Ok(name)
}

/// Checks a new ID for row `id`: unique in the whole tree.
pub fn check_code(
    doc: &Document,
    spec: &TreeSpec,
    owner: Option<i64>,
    id: Option<i64>,
    text: &str,
) -> Result<String> {
    let Some(column) = spec.code else {
        return Ok(text.trim().to_string());
    };
    let code = clean_text(text, spec.max_len, &format!("{} ID", spec.noun))?;
    let clash = nodes(doc, spec, owner)
        .iter()
        .any(|n| Some(n.id) != id && same(&value(doc, spec, n.id, column), &code));
    if clash {
        return Err(format!(
            "The {} ID \"{code}\" is already in use.",
            spec.noun
        ));
    }
    Ok(code)
}

/// The first of `base`, `base 1`, `base 2`... that `taken` refuses.
fn unique(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_string();
    }
    (1..)
        .map(|k| format!("{base} {k}"))
        .find(|s| !taken(s))
        .unwrap()
}

fn siblings(all: &[Node], parent: Option<i64>) -> Vec<i64> {
    all.iter()
        .filter(|n| n.parent == parent)
        .map(|n| n.id)
        .collect()
}

/// Write the sequence numbers of `parent`'s children in the given order.
fn renumber(doc: &mut Document, spec: &TreeSpec, order: &[i64]) {
    for (k, id) in order.iter().enumerate() {
        let seq = ((k + 1) * 10).to_string();
        if value(doc, spec, *id, spec.seq) != seq {
            set(doc, spec, *id, spec.seq, seq);
        }
    }
}

/// Where Add puts a new row next to `selected`: its parent and the sibling
/// it follows.
fn place(all: &[Node], spec: &TreeSpec, selected: Option<i64>) -> (Option<i64>, Option<i64>) {
    match selected.filter(|id| all.iter().any(|n| n.id == *id)) {
        Some(id) if spec.add_child => (Some(id), siblings(all, Some(id)).last().copied()),
        Some(id) => (
            all.iter().find(|n| n.id == id).and_then(|n| n.parent),
            Some(id),
        ),
        None => (None, siblings(all, None).last().copied()),
    }
}

/// Insert `id` among `parent`'s children right after `after` (first when
/// `None`) and renumber them.
fn insert_after(
    doc: &mut Document,
    spec: &TreeSpec,
    owner: Option<i64>,
    parent: Option<i64>,
    after: Option<i64>,
    id: i64,
) {
    let all = nodes(doc, spec, owner);
    let mut order: Vec<i64> = siblings(&all, parent)
        .into_iter()
        .filter(|s| *s != id)
        .collect();
    let at = after
        .and_then(|a| order.iter().position(|s| *s == a))
        .map_or(0, |k| k + 1);
    order.insert(at.min(order.len()), id);
    renumber(doc, spec, &order);
}

fn parent_text(parent: Option<i64>) -> String {
    parent.map_or(String::new(), |p| p.to_string())
}

/// Add a row next to `selected` with a generated unique name (and ID), plus
/// `extra` values. Returns the new row's ID.
pub fn add(
    doc: &mut Document,
    spec: &TreeSpec,
    owner: Option<i64>,
    selected: Option<i64>,
    extra: &[(&str, String)],
) -> Result<i64> {
    let all = nodes(doc, spec, owner);
    let (parent, after) = place(&all, spec, selected);
    let depth = parent.map_or(0, |p| depth_of(&all, p) + 1);
    if depth + 1 > spec.max_depth {
        return Err(format!(
            "The {} tree cannot be deeper than {} levels.",
            spec.noun, spec.max_depth
        ));
    }
    let id = records::next_id(doc, spec.table, spec.id);
    let name = unique(spec.new_name, |s| {
        all.iter()
            .any(|n| n.parent == parent && same(&value(doc, spec, n.id, spec.name), s))
    });
    let mut values = vec![
        (spec.id, id.to_string()),
        (spec.parent, parent_text(parent)),
        (spec.seq, "0".to_string()),
        (spec.name, name),
    ];
    if let Some(code) = spec.code {
        let base = format!("{}{id}", spec.code_prefix);
        let text = unique(&base, |s| {
            all.iter().any(|n| same(&value(doc, spec, n.id, code), s))
        });
        values.push((code, text));
    }
    if let (Some(column), Some(owner)) = (spec.owner, owner) {
        values.push((column, owner.to_string()));
    }
    for (k, v) in extra {
        values.retain(|(c, _)| c != k);
        values.push((k, v.clone()));
    }
    records::add_row(doc, spec.table, &values);
    insert_after(doc, spec, owner, parent, after, id);
    Ok(id)
}

/// Set the name (checked among siblings) or ID (checked tree-wide) of a row.
/// Returns false when the text is already the value.
pub fn rename(
    doc: &mut Document,
    spec: &TreeSpec,
    owner: Option<i64>,
    id: i64,
    column: &str,
    text: &str,
) -> Result<bool> {
    let all = nodes(doc, spec, owner);
    let Some(node) = all.iter().find(|n| n.id == id) else {
        return Ok(false);
    };
    if column != spec.name && Some(column) != spec.code {
        return Err(format!("{column} is not a name column."));
    }
    let mut clean = text.trim().to_string();
    if column == spec.name {
        clean = check_name(doc, spec, owner, node.parent, Some(id), text)?;
    }
    if Some(column) == spec.code {
        clean = check_code(doc, spec, owner, Some(id), text)?;
    }
    if value(doc, spec, id, column) == clean {
        return Ok(false);
    }
    set(doc, spec, id, column, clean);
    Ok(true)
}

/// Move a row up (`by` = -1) or down (+1) among its siblings.
pub fn shift(doc: &mut Document, spec: &TreeSpec, owner: Option<i64>, id: i64, by: i32) -> bool {
    let all = nodes(doc, spec, owner);
    let Some(node) = all.iter().find(|n| n.id == id) else {
        return false;
    };
    let mut order = siblings(&all, node.parent);
    let k = order.iter().position(|s| *s == id).unwrap();
    let to = k as i64 + by as i64;
    if to < 0 || to >= order.len() as i64 {
        return false;
    }
    order.swap(k, to as usize);
    renumber(doc, spec, &order);
    true
}

/// Make a row the last child of the sibling above it.
pub fn indent(doc: &mut Document, spec: &TreeSpec, owner: Option<i64>, id: i64) -> Result<bool> {
    let all = nodes(doc, spec, owner);
    let Some(node) = all.iter().find(|n| n.id == id).cloned() else {
        return Ok(false);
    };
    let order = siblings(&all, node.parent);
    let k = order.iter().position(|s| *s == id).unwrap();
    if k == 0 {
        return Ok(false);
    }
    let parent = order[k - 1];
    if node.depth + 1 + height(&all, id) > spec.max_depth {
        return Err(format!(
            "The {} tree cannot be deeper than {} levels.",
            spec.noun, spec.max_depth
        ));
    }
    let name = value(doc, spec, id, spec.name);
    check_name(doc, spec, owner, Some(parent), Some(id), &name)?;
    set(doc, spec, id, spec.parent, parent.to_string());
    let last = siblings(&all, Some(parent)).last().copied();
    insert_after(doc, spec, owner, Some(parent), last, id);
    renumber(
        doc,
        spec,
        &order
            .iter()
            .copied()
            .filter(|s| *s != id)
            .collect::<Vec<_>>(),
    );
    Ok(true)
}

/// Make a row the sibling right after its parent.
pub fn outdent(doc: &mut Document, spec: &TreeSpec, owner: Option<i64>, id: i64) -> Result<bool> {
    let all = nodes(doc, spec, owner);
    let Some(node) = all.iter().find(|n| n.id == id).cloned() else {
        return Ok(false);
    };
    let Some(parent) = node.parent else {
        return Ok(false);
    };
    let grand = all.iter().find(|n| n.id == parent).and_then(|n| n.parent);
    let name = value(doc, spec, id, spec.name);
    check_name(doc, spec, owner, grand, Some(id), &name)?;
    set(doc, spec, id, spec.parent, parent_text(grand));
    insert_after(doc, spec, owner, grand, Some(parent), id);
    let rest: Vec<i64> = siblings(&all, Some(parent))
        .into_iter()
        .filter(|s| *s != id)
        .collect();
    renumber(doc, spec, &rest);
    Ok(true)
}

/// Delete a row and everything under it; returns the deleted IDs.
pub fn delete(doc: &mut Document, spec: &TreeSpec, owner: Option<i64>, id: i64) -> Vec<i64> {
    let gone = subtree(doc, spec, owner, id);
    if gone.is_empty() {
        return gone;
    }
    let set: BTreeSet<i64> = gone.iter().copied().collect();
    let column = spec.id;
    let t = doc.table_mut(spec.table);
    let k = t.field(column);
    t.rows.retain(|r| {
        !k.and_then(|k| r.get(k))
            .is_some_and(|v| set.contains(&num(v)))
    });
    gone
}

/// A copied or cut subtree.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub spec: TreeSpec,
    pub owner: Option<i64>,
    pub root: i64,
    /// Columns and rows of the subtree as copied, root first.
    pub fields: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub cut: bool,
}

pub fn copy(
    doc: &Document,
    spec: &TreeSpec,
    owner: Option<i64>,
    id: i64,
    cut: bool,
) -> Option<Clip> {
    let t = table(doc, spec)?;
    let ids = subtree(doc, spec, owner, id);
    if ids.is_empty() {
        return None;
    }
    let rows = ids
        .iter()
        .filter_map(|i| {
            t.rows
                .iter()
                .find(|r| num(t.value(r, spec.id)) == *i)
                .cloned()
        })
        .collect();
    Some(Clip {
        spec: *spec,
        owner,
        root: id,
        fields: t.fields.clone(),
        rows,
        cut,
    })
}

/// Paste a clip next to `selected` (under it for trees that add children).
/// A cut subtree moves and keeps its IDs; a copy gets new IDs, and names (and
/// IDs) that clash get a number added. Returns the pasted root.
#[allow(clippy::explicit_counter_loop)]
pub fn paste(
    doc: &mut Document,
    spec: &TreeSpec,
    owner: Option<i64>,
    selected: Option<i64>,
    clip: &Clip,
) -> Result<i64> {
    if clip.spec != *spec || clip.owner != owner {
        return Err(format!("The clipboard does not hold a {}.", spec.noun));
    }
    let all = nodes(doc, spec, owner);
    let (parent, after) = place(&all, spec, selected);
    let depth = parent.map_or(0, |p| depth_of(&all, p) + 1);
    let clip_height = {
        let mut depth_of_row: BTreeMap<i64, usize> = BTreeMap::new();
        let id_col = clip.fields.iter().position(|f| f == spec.id);
        let parent_col = clip.fields.iter().position(|f| f == spec.parent);
        let get =
            |r: &Vec<String>, c: Option<usize>| c.and_then(|c| r.get(c)).map_or(0, |v| num(v));
        for r in &clip.rows {
            let d = depth_of_row.get(&get(r, parent_col)).map_or(1, |d| d + 1);
            depth_of_row.insert(get(r, id_col), d);
        }
        depth_of_row.values().copied().max().unwrap_or(1)
    };
    if depth + clip_height > spec.max_depth {
        return Err(format!(
            "The {} tree cannot be deeper than {} levels.",
            spec.noun, spec.max_depth
        ));
    }
    if clip.cut {
        if !all.iter().any(|n| n.id == clip.root) {
            return Err(format!("The cut {} no longer exists.", spec.noun));
        }
        let inside = subtree(doc, spec, owner, clip.root);
        if parent.is_some_and(|p| inside.contains(&p)) {
            return Err(format!("A {} cannot be moved under itself.", spec.noun));
        }
        let name = value(doc, spec, clip.root, spec.name);
        check_name(doc, spec, owner, parent, Some(clip.root), &name)?;
        let old_parent = all
            .iter()
            .find(|n| n.id == clip.root)
            .and_then(|n| n.parent);
        set(doc, spec, clip.root, spec.parent, parent_text(parent));
        insert_after(doc, spec, owner, parent, after, clip.root);
        let rest: Vec<i64> = siblings(&nodes(doc, spec, owner), old_parent);
        renumber(doc, spec, &rest);
        return Ok(clip.root);
    }
    let id_col = clip.fields.iter().position(|f| f == spec.id);
    let parent_col = clip.fields.iter().position(|f| f == spec.parent);
    let cell = |r: &Vec<String>, c: &str| {
        clip.fields
            .iter()
            .position(|f| f == c)
            .and_then(|k| r.get(k))
            .cloned()
            .unwrap_or_default()
    };
    let mut next = records::next_id(doc, spec.table, spec.id);
    let mut map: BTreeMap<i64, i64> = BTreeMap::new();
    let mut root = None;
    for (k, r) in clip.rows.iter().enumerate() {
        let old = id_col.and_then(|c| r.get(c)).map_or(0, |v| num(v));
        let new = next;
        next += 1;
        map.insert(old, new);
        let new_parent = if k == 0 {
            parent
        } else {
            parent_col
                .and_then(|c| r.get(c))
                .and_then(|v| map.get(&num(v)).copied())
        };
        let mut values: Vec<(&str, String)> = clip
            .fields
            .iter()
            .zip(r)
            .filter(|(f, _)| *f != spec.id && *f != spec.parent && f.as_str() != "guid")
            .map(|(f, v)| (f.as_str(), v.clone()))
            .collect();
        values.push((spec.id, new.to_string()));
        values.push((spec.parent, parent_text(new_parent)));
        if k == 0 {
            let siblings_now = nodes(doc, spec, owner);
            let base = cell(r, spec.name);
            let name = unique(&base, |s| {
                siblings_now
                    .iter()
                    .any(|n| n.parent == parent && same(&value(doc, spec, n.id, spec.name), s))
            });
            values.retain(|(c, _)| *c != spec.name);
            values.push((spec.name, name));
            root = Some(new);
        }
        if let Some(code) = spec.code {
            let now = nodes(doc, spec, owner);
            let base = cell(r, code);
            let text = unique(&base, |s| {
                now.iter().any(|n| same(&value(doc, spec, n.id, code), s))
            });
            values.retain(|(c, _)| *c != code);
            values.push((code, text));
        }
        records::add_row(doc, spec.table, &values);
    }
    let root = root.ok_or_else(|| "Nothing to paste.".to_string())?;
    insert_after(doc, spec, owner, parent, after, root);
    Ok(root)
}

/// Point the rows of `table` whose `column` holds one of `gone` at `to`
/// (blank when `None`). Returns the rows changed.
pub fn retarget(
    doc: &mut Document,
    table: &str,
    column: &str,
    gone: &[i64],
    to: Option<i64>,
) -> usize {
    let Some(t) = doc
        .tables
        .iter_mut()
        .find(|t| t.name.eq_ignore_ascii_case(table))
    else {
        return 0;
    };
    let Some(k) = t.field(column) else {
        return 0;
    };
    let mut changed = 0;
    for r in &mut t.rows {
        if let Some(v) = r.get_mut(k) {
            if !v.is_empty() && gone.contains(&num(v)) {
                *v = parent_text(to);
                changed += 1;
            }
        }
    }
    changed
}

/// Rows of `table` whose `column` points at one of `ids`.
pub fn uses(doc: &Document, table: &str, column: &str, ids: &[i64]) -> usize {
    doc.table(table).map_or(0, |t| {
        t.rows
            .iter()
            .filter(|r| {
                let v = t.value(r, column);
                !v.is_empty() && ids.contains(&num(v))
            })
            .count()
    })
}

/// Remove the rows of `table` whose `column` points at one of `ids`.
pub fn remove_uses(doc: &mut Document, table: &str, column: &str, ids: &[i64]) -> usize {
    let Some(t) = doc
        .tables
        .iter_mut()
        .find(|t| t.name.eq_ignore_ascii_case(table))
    else {
        return 0;
    };
    let Some(k) = t.field(column) else {
        return 0;
    };
    let before = t.rows.len();
    t.rows.retain(|r| {
        !r.get(k)
            .is_some_and(|v| !v.is_empty() && ids.contains(&num(v)))
    });
    before - t.rows.len()
}
