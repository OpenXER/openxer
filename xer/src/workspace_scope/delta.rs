//! Row changes between two versions of a workspace dictionary, so a change
//! can be undone and redone on its own: undoing one project's change to a
//! shared calendar restores that calendar and leaves alone what other
//! projects changed since.
use super::*;

/// One row as column name and value pairs.
pub type Row = Vec<(String, String)>;

/// Which rows a change covers.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RowKey {
    /// The row of a keyed table with this id.
    Id(i64),
    /// The rows of an unkeyed child table that belong to this owner.
    Owner(i64),
}

/// The rows of one key before and after.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// Table name, upper case.
    pub table: String,
    pub key: RowKey,
    pub before: Vec<Row>,
    pub after: Vec<Row>,
}

/// Every row that differs between two dictionaries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Delta {
    pub changes: Vec<Change>,
}

fn row_pairs(t: &Table, r: &[String]) -> Row {
    t.fields
        .iter()
        .enumerate()
        .map(|(i, f)| (f.clone(), r.get(i).cloned().unwrap_or_default()))
        .collect()
}

/// Rows compare by their values, columns in any order and a missing
/// column taken as empty.
fn normalized(row: &Row) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = row
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(f, v)| (f.to_ascii_lowercase(), v.clone()))
        .collect();
    v.sort();
    v
}

fn same_rows(a: &[Row], b: &[Row]) -> bool {
    let mut x: Vec<_> = a.iter().map(normalized).collect();
    let mut y: Vec<_> = b.iter().map(normalized).collect();
    x.sort();
    y.sort();
    x == y
}

/// The column of an unkeyed child table naming its owner.
fn owner_column(table: &str) -> Option<&'static str> {
    OWNED
        .iter()
        .find(|(c, _, _)| c.eq_ignore_ascii_case(table))
        .map(|(_, col, _)| *col)
}

/// Rows of one table by key.
fn grouped(t: &Table) -> BTreeMap<RowKey, Vec<Row>> {
    let mut out: BTreeMap<RowKey, Vec<Row>> = BTreeMap::new();
    let key = key_index(t);
    let owner = owner_column(&t.name).and_then(|c| t.field(c));
    for r in &t.rows {
        let k = match (key, owner) {
            (Some(k), _) => RowKey::Id(num(&r[k])),
            (None, Some(o)) => RowKey::Owner(num(&r[o])),
            (None, None) => continue,
        };
        out.entry(k).or_default().push(row_pairs(t, r));
    }
    out
}

/// The rows of the workspace tables that differ between `before` and `after`.
pub fn diff(before: &Document, after: &Document) -> Delta {
    let mut names: BTreeSet<String> = BTreeSet::new();
    for d in [before, after] {
        for t in &d.tables {
            if is_workspace_table(&t.name) {
                names.insert(upper(&t.name));
            }
        }
    }
    let mut changes = vec![];
    for name in names {
        let a = before.table(&name).map(grouped).unwrap_or_default();
        let b = after.table(&name).map(grouped).unwrap_or_default();
        let keys: BTreeSet<&RowKey> = a.keys().chain(b.keys()).collect();
        for k in keys {
            let x = a.get(k).cloned().unwrap_or_default();
            let y = b.get(k).cloned().unwrap_or_default();
            if !same_rows(&x, &y) {
                changes.push(Change {
                    table: name.clone(),
                    key: k.clone(),
                    before: x,
                    after: y,
                });
            }
        }
    }
    Delta { changes }
}

impl Change {
    /// The keyed row this change belongs to, as (table, id): its own row, or
    /// for the rows of an owned child table ([`OWNED`]) their owner's.
    pub fn unit(&self) -> (String, i64) {
        match self.key {
            RowKey::Id(id) => (self.table.clone(), id),
            RowKey::Owner(id) => {
                let owner = OWNED
                    .iter()
                    .find(|(c, _, _)| c.eq_ignore_ascii_case(&self.table))
                    .map_or_else(|| self.table.clone(), |(_, _, o)| o.to_string());
                (owner, id)
            }
        }
    }
}

impl Delta {
    /// The rows (as [`Change::unit`]) that `dict` no longer holds as this
    /// delta left them: changed by someone else since it was made. Applying
    /// the delta `forward` (redo) expects its `before` rows, backward (undo)
    /// its `after` rows; putting a stale row back would silently revert the
    /// later change. A row that already is as the delta would make it is
    /// not stale.
    pub fn stale(&self, dict: &Document, forward: bool) -> BTreeSet<(String, i64)> {
        let mut tables: BTreeMap<String, BTreeMap<RowKey, Vec<Row>>> = BTreeMap::new();
        let mut out = BTreeSet::new();
        for c in &self.changes {
            let rows = tables
                .entry(c.table.clone())
                .or_insert_with(|| dict.table(&c.table).map(grouped).unwrap_or_default());
            let now = rows.get(&c.key).cloned().unwrap_or_default();
            let (expected, target) = if forward {
                (&c.before, &c.after)
            } else {
                (&c.after, &c.before)
            };
            if !same_rows(&now, expected) && !same_rows(&now, target) {
                out.insert(c.unit());
            }
        }
        out
    }

    /// This delta without the changes of the rows in `units` (each a keyed
    /// row with its owned child rows).
    pub fn without(&self, units: &BTreeSet<(String, i64)>) -> Delta {
        Delta {
            changes: self
                .changes
                .iter()
                .filter(|c| !units.contains(&c.unit()))
                .cloned()
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// One delta doing this and then `later`.
    pub fn then(&mut self, later: Delta) {
        for c in later.changes {
            match self
                .changes
                .iter_mut()
                .find(|x| x.table == c.table && x.key == c.key)
            {
                Some(x) => x.after = c.after,
                None => self.changes.push(c),
            }
        }
        self.changes.retain(|c| !same_rows(&c.before, &c.after));
    }

    /// Keyed rows that the delta, applied `forward` or backward, removes:
    /// (table, id).
    pub fn removes(&self, forward: bool) -> Vec<(String, i64)> {
        self.changes
            .iter()
            .filter_map(|c| {
                let (from, to) = if forward {
                    (&c.before, &c.after)
                } else {
                    (&c.after, &c.before)
                };
                match c.key {
                    RowKey::Id(id) if !from.is_empty() && to.is_empty() => {
                        Some((c.table.clone(), id))
                    }
                    _ => None,
                }
            })
            .collect()
    }

    /// Put the rows of `after` (`forward`) or of `before` into `dict`.
    pub fn apply(&self, dict: &mut Document, forward: bool) {
        for c in &self.changes {
            let rows = if forward { &c.after } else { &c.before };
            let t = dict.table_mut(&c.table);
            for r in rows {
                for (f, _) in r {
                    if t.field(f).is_none() {
                        t.fields.push(f.clone());
                        for x in &mut t.rows {
                            x.push(String::new());
                        }
                    }
                }
            }
            let key = key_index(t);
            let owner = owner_column(&c.table).and_then(|col| t.field(col));
            t.rows.retain(|r| match (&c.key, key, owner) {
                (RowKey::Id(id), Some(k), _) => num(&r[k]) != *id,
                (RowKey::Owner(id), None, Some(o)) => num(&r[o]) != *id,
                _ => true,
            });
            for r in rows {
                let mut row = vec![String::new(); t.fields.len()];
                for (f, v) in r {
                    if let Some(i) = t.field(f) {
                        row[i] = v.clone();
                    }
                }
                t.rows.push(row);
            }
            if let Some(k) = key {
                t.rows.sort_by_key(|r| num(&r[k]));
            }
        }
        dict.tables
            .retain(|t| !(t.rows.is_empty() && t.fields.is_empty()));
    }
}

/// A hash of the workspace rows of `doc` (both shared scopes), to tell
/// cheaply whether an edit touched them.
pub fn fingerprint(doc: &Document) -> u64 {
    use std::hash::{Hash, Hasher};
    let scopes = Scopes::of(doc);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for t in &doc.tables {
        if !is_workspace_table(&t.name) {
            continue;
        }
        upper(&t.name).hash(&mut h);
        t.fields.hash(&mut h);
        for r in &t.rows {
            if scopes.shared(t, r) {
                r.hash(&mut h);
            }
        }
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict(rows: &str) -> Document {
        Document::parse(&format!(
            "ERMHDR\t25.12\r\n%T\tRSRC\r\n%F\trsrc_id\trsrc_short_name\trsrc_name\r\n{rows}%T\tRSRCROLE\r\n%F\trsrc_id\trole_id\r\n%R\t1\t7\r\n%E\r\n"
        ))
        .unwrap()
    }

    #[test]
    fn a_delta_undoes_and_redoes_only_its_own_rows() {
        let before = dict("%R\t1\tWLD\tWelder\r\n%R\t2\tCRN\tCrane\r\n");
        let after =
            dict("%R\t1\tWLD\tWelder, night\r\n%R\t2\tCRN\tCrane\r\n%R\t3\tPIP\tPipefitter\r\n");
        let delta = diff(&before, &after);
        assert_eq!(delta.changes.len(), 2);
        assert_eq!(delta.removes(false), vec![("RSRC".to_string(), 3)]);
        // Another project renames the crane meanwhile; undo keeps that.
        let mut now = dict(
            "%R\t1\tWLD\tWelder, night\r\n%R\t2\tCRN\tCrane 50t\r\n%R\t3\tPIP\tPipefitter\r\n",
        );
        delta.apply(&mut now, false);
        let t = now.table("RSRC").unwrap();
        let names: Vec<&str> = t.rows.iter().map(|r| t.value(r, "rsrc_name")).collect();
        assert_eq!(names, ["Welder", "Crane 50t"]);
        delta.apply(&mut now, true);
        let t = now.table("RSRC").unwrap();
        let names: Vec<&str> = t.rows.iter().map(|r| t.value(r, "rsrc_name")).collect();
        assert_eq!(names, ["Welder, night", "Crane 50t", "Pipefitter"]);
        assert_eq!(now.table("RSRCROLE").unwrap().rows.len(), 1);
    }

    #[test]
    fn a_row_changed_since_the_delta_is_stale_with_its_child_rows() {
        let before = dict("%R\t1\tWLD\tWelder\r\n%R\t2\tCRN\tCrane\r\n");
        let after = dict("%R\t1\tWLD\tWelder, night\r\n%R\t2\tCRN\tCrane 50t\r\n");
        let delta = diff(&before, &after);
        assert!(delta.stale(&after, false).is_empty());
        assert!(delta.stale(&before, true).is_empty());
        // Someone renames the welder again: undo would revert that.
        let now = dict("%R\t1\tWLD\tWelder, day\r\n%R\t2\tCRN\tCrane 50t\r\n");
        let stale = delta.stale(&now, false);
        assert_eq!(stale, [("RSRC".to_string(), 1)].into());
        let rest = delta.without(&stale);
        assert_eq!(rest.changes.len(), 1);
        let mut undone = now.clone();
        rest.apply(&mut undone, false);
        let t = undone.table("RSRC").unwrap();
        let names: Vec<&str> = t.rows.iter().map(|r| t.value(r, "rsrc_name")).collect();
        assert_eq!(names, ["Welder, day", "Crane"]);
        // A child row belongs to its owner.
        let c = Change {
            table: "RSRCROLE".into(),
            key: RowKey::Owner(1),
            before: vec![],
            after: vec![],
        };
        assert_eq!(c.unit(), ("RSRC".to_string(), 1));
    }

    #[test]
    fn deltas_compose_and_the_fingerprint_sees_workspace_rows_only() {
        let a = dict("%R\t1\tWLD\tWelder\r\n");
        let b = dict("%R\t1\tWLD\tWelder 2\r\n");
        let c = dict("%R\t1\tWLD\tWelder\r\n");
        let mut d = diff(&a, &b);
        d.then(diff(&b, &c));
        assert!(d.is_empty(), "a change and its reversal cancel");
        assert_ne!(fingerprint(&a), fingerprint(&b));
        assert_eq!(fingerprint(&a), fingerprint(&c));
    }
}
