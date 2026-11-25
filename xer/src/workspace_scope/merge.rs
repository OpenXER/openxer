//! Merging a project's workspace rows into the workspace dictionary.
//!
//! A row the project read from the dictionary (its id is in the project's
//! hydrate mark) is the same row: the project's version replaces the stored
//! one. Any other workspace row is matched by what it is: a row with the same
//! name and the same content (ids, GUIDs and usage counts aside, references
//! compared after renumbering) is the stored row and the project's
//! references move to it; a row whose name is taken by different content is
//! added as a renamed copy; anything else is added, keeping its id when that
//! id is free. Resources, roles, shifts, financial period templates and
//! step templates are compared together with their child rows (rates,
//! categories, periods), so a match never mixes two sets of resource limits.
//! Code types and category types share their values: a matching type takes
//! the project's new values, and a value whose path is taken by different
//! content makes the whole type a copy, so no value is ever renamed.
use super::*;

/// Value tables of the dictionary types: (type table, value table, column
/// naming the type, column naming the parent value, short name column).
pub const VALUE_GROUPS: &[(&str, &str, &str, Option<&str>, &str)] = &[
    (
        "ACTVTYPE",
        "ACTVCODE",
        "actv_code_type_id",
        Some("parent_actv_code_id"),
        "short_name",
    ),
    (
        "RCATTYPE",
        "RCATVAL",
        "rsrc_catg_type_id",
        Some("parent_rsrc_catg_id"),
        "rsrc_catg_short_name",
    ),
    (
        "PCATTYPE",
        "PCATVAL",
        "proj_catg_type_id",
        Some("parent_proj_catg_id"),
        "proj_catg_short_name",
    ),
    (
        "ROLECATTYPE",
        "ROLECATVAL",
        "role_catg_type_id",
        Some("parent_role_catg_id"),
        "role_catg_short_name",
    ),
    (
        "ASGNMNTCATTYPE",
        "ASGNMNTCATVAL",
        "asgnmnt_catg_type_id",
        Some("parent_asgnmnt_catg_id"),
        "asgnmnt_catg_short_name",
    ),
    ("UDFTYPE", "UDFCODE", "udf_type_id", None, "short_name"),
];

/// Owner tables in the order they are merged: every table a row refers to
/// comes before it.
pub const ORDER: &[&str] = &[
    "CURRTYPE",
    "UMEASURE",
    "SHIFT",
    "OBS",
    "CALENDAR",
    "RCATTYPE",
    "ROLECATTYPE",
    "PCATTYPE",
    "ASGNMNTCATTYPE",
    "ROLES",
    "RSRC",
    "ACTVTYPE",
    "UDFTYPE",
    "ACCOUNT",
    "FUNDSRC",
    "FINTMPL",
    "RSRCCURVDATA",
    "MEMOTYPE",
    "COSTTYPE",
    "DOCCATG",
    "DOCSTAT",
    "RISKTYPE",
    "BASETYPE",
    "THRSPARM",
    "NONWORK",
    "PROCGROUP",
];

/// Owner tables whose rows point at a parent row of the same table.
const SELF_PARENT: &[(&str, &str)] = &[
    ("CALENDAR", "base_clndr_id"),
    ("RSRC", "parent_rsrc_id"),
    ("ROLES", "parent_role_id"),
    ("OBS", "parent_obs_id"),
    ("ACCOUNT", "parent_acct_id"),
    ("FUNDSRC", "parent_fund_id"),
    ("RISKTYPE", "parent_risk_type_id"),
];

/// The columns that name an owner row; the first is the one a copy renames.
pub const NAMES: &[(&str, &[&str])] = &[
    ("CALENDAR", &["clndr_name", "clndr_type"]),
    ("RSRC", &["rsrc_short_name"]),
    ("ROLES", &["role_short_name"]),
    ("ACTVTYPE", &["actv_code_type"]),
    ("UDFTYPE", &["udf_type_label", "table_name"]),
    ("ACCOUNT", &["acct_short_name"]),
    ("OBS", &["obs_name"]),
    ("CURRTYPE", &["curr_short_name"]),
    ("UMEASURE", &["unit_abbrev"]),
    ("SHIFT", &["shift_name"]),
    ("MEMOTYPE", &["memo_type"]),
    ("COSTTYPE", &["cost_type"]),
    ("RCATTYPE", &["rsrc_catg_type"]),
    ("PCATTYPE", &["proj_catg_type"]),
    ("ROLECATTYPE", &["role_catg_type"]),
    ("ASGNMNTCATTYPE", &["asgnmnt_catg_type"]),
    ("FUNDSRC", &["fund_name"]),
    ("FINTMPL", &["fintmpl_name"]),
    ("RSRCCURVDATA", &["curv_name"]),
    ("DOCCATG", &["doc_catg_name"]),
    ("DOCSTAT", &["doc_status_code"]),
    ("RISKTYPE", &["risk_type"]),
    ("BASETYPE", &["base_type"]),
    ("NONWORK", &["nonwork_type"]),
    ("PROCGROUP", &["proc_group_name"]),
];

/// Columns left out when deciding whether two rows are the same: they
/// identify a copy or count its uses rather than say what it is.
pub const IGNORED: &[&str] = &["guid", "total_assignments"];

/// Whether column `field` of `table` is left out when comparing rows:
/// [`IGNORED`], and a calendar's `default_flag` (it says which calendar new
/// projects take, not what the calendar is; the stored row keeps its flag).
pub fn ignored(table: &str, field: &str) -> bool {
    IGNORED.iter().any(|s| s.eq_ignore_ascii_case(field))
        || (table.eq_ignore_ascii_case("CALENDAR") && field.eq_ignore_ascii_case("default_flag"))
}

/// What a project's row does when a stored row has its name and different
/// content (an import's choice per kind of row).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Choice {
    /// Add it as a renamed copy (`<name>_<n>`).
    #[default]
    Copy,
    /// Use the stored row; the project's version is dropped.
    Keep,
    /// The project's version replaces the stored row for every project.
    Update,
}

impl Choice {
    pub const ALL: [Choice; 3] = [Choice::Copy, Choice::Keep, Choice::Update];
    pub fn key(self) -> &'static str {
        match self {
            Choice::Copy => "copy",
            Choice::Keep => "keep",
            Choice::Update => "update",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Choice::Copy => "Add as a copy",
            Choice::Keep => "Keep the workspace's",
            Choice::Update => "Update the workspace's",
        }
    }
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "copy" | "insert" | "insert_new" => Some(Choice::Copy),
            "keep" | "keep_existing" => Some(Choice::Keep),
            "update" | "update_existing" => Some(Choice::Update),
            _ => None,
        }
    }
}

/// How a copy is named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Policy {
    /// `<name> (<project short name>)`: moving existing projects into one
    /// workspace dictionary.
    Migration { label: String },
    /// `<name>_<n>`: saving a project whose new row's name is taken.
    Save,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventKind {
    /// The project's row is the stored row (same name and content).
    Merged,
    /// Added under its own id.
    Added,
    /// Added under a new id because its id was taken.
    Renumbered,
    /// Its name was taken by different content: added as a renamed copy.
    Copied,
    /// Its name was taken by different content; the stored row is used
    /// ([`Choice::Keep`]).
    Kept,
    /// Its name was taken by different content; its values replace the
    /// stored row's ([`Choice::Update`]).
    Updated,
    /// A project row renumbered because a workspace row took its id.
    ProjectRenumbered,
}

/// One decision of a merge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub kind: EventKind,
    pub table: String,
    pub name: String,
    pub from: i64,
    pub to: i64,
    /// For a copy: its new name and the columns that differ from the stored
    /// row of the same name.
    pub renamed: Option<String>,
    pub differences: Vec<String>,
}

/// The result of merging one project.
#[derive(Clone, Debug)]
pub struct Outcome {
    /// The project's document with the merge applied: ids renumbered,
    /// references rewritten, copies renamed.
    pub document: Document,
    pub remap: Remap,
    pub events: Vec<Event>,
    /// Rows whose id the project kept and the dictionary now holds: the
    /// project's next hydrate mark, by the ids of the document merged.
    pub mark: BTreeSet<(String, i64)>,
}

/// The dictionary being built or updated.
#[derive(Clone, Debug)]
pub struct Merger {
    pub dict: Document,
    /// Ids the dictionary must not take: rows other projects keep for
    /// themselves in the tables of both kinds.
    reserved: BTreeMap<String, BTreeSet<i64>>,
    /// A new row keeps its id only above this (ids up to it may be taken by
    /// a project the merge cannot see).
    floor: BTreeMap<String, i64>,
    next: BTreeMap<String, i64>,
    /// Whether the dictionary changed.
    pub changed: bool,
    /// Per owner table (upper case): what a row whose name is taken by
    /// different content does. A table not named makes copies.
    pub choices: BTreeMap<String, Choice>,
}

fn uname(s: &str) -> String {
    s.to_ascii_uppercase()
}

fn owner_of(child: &str) -> Option<(&'static str, &'static str)> {
    OWNED
        .iter()
        .find(|(c, _, _)| c.eq_ignore_ascii_case(child))
        .map(|(_, col, owner)| (*col, *owner))
}

fn value_group(
    table: &str,
) -> Option<&'static (
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
    &'static str,
)> {
    VALUE_GROUPS
        .iter()
        .find(|g| g.1.eq_ignore_ascii_case(table))
}

fn type_group(
    table: &str,
) -> Option<&'static (
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
    &'static str,
)> {
    VALUE_GROUPS
        .iter()
        .find(|g| g.0.eq_ignore_ascii_case(table))
}

fn names_of(table: &str) -> &'static [&'static str] {
    NAMES
        .iter()
        .find(|(t, _)| t.eq_ignore_ascii_case(table))
        .map_or(&[], |(_, n)| n)
}

fn parent_column(table: &str) -> Option<&'static str> {
    SELF_PARENT
        .iter()
        .find(|(t, _)| t.eq_ignore_ascii_case(table))
        .map(|(_, c)| *c)
        .or_else(|| value_group(table).and_then(|g| g.3))
}

/// The name identity of an owner row (`None` when its table has none).
fn identity(table: &Table, row: &[String]) -> Option<String> {
    let names = names_of(&table.name);
    if names.is_empty() {
        return None;
    }
    let parts: Vec<String> = names
        .iter()
        .map(|c| {
            let v = table.value(row, c);
            if uname(&table.name) == "ACTVTYPE" {
                normalized_code_type(v)
            } else {
                v.to_string()
            }
        })
        .collect();
    Some(parts.join("\u{1f}"))
}

/// A code type name as leveling priorities name it (`ACTVCODE_<this>`).
fn normalized_code_type(name: &str) -> String {
    name.to_ascii_uppercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

/// Whether `stored` is a copy made from `ours`: the same other name columns
/// and a name of the forms copies take (`<name>_<n>`, `<name> (<label>)`).
fn derived_from(table: &Table, ours: &[String], stored: &[String]) -> bool {
    let names = names_of(&table.name);
    let Some((first, rest)) = names.split_first() else {
        return false;
    };
    if rest
        .iter()
        .any(|c| table.value(ours, c) != table.value(stored, c))
    {
        return false;
    }
    let mine = table.value(ours, first);
    let Some(tail) = table.value(stored, first).strip_prefix(mine) else {
        return false;
    };
    tail.strip_prefix('_')
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        || (tail.starts_with(" (") && tail.ends_with(')'))
}

/// The name an event shows: the row's first name column, else its id.
pub fn display_name(table: &Table, row: &[String]) -> String {
    names_of(&table.name)
        .first()
        .map(|c| table.value(row, c).to_string())
        .unwrap_or_default()
}

/// The row of `table` with id `id`.
fn row_by_id(table: &Table, id: i64) -> Option<&Vec<String>> {
    let k = key_column(&table.name).and_then(|k| table.field(k))?;
    table.rows.iter().find(|r| int(&r[k]) == Some(id))
}

/// Whether `ours` (laid out by `mine`) says the same as `stored` (laid out by
/// `theirs`) in every column of ours except `skip` and [`IGNORED`].
fn same(mine: &Table, ours: &[String], theirs: &Table, stored: &[String], skip: &[&str]) -> bool {
    differences(mine, ours, theirs, stored, skip).is_empty()
}

fn differences(
    mine: &Table,
    ours: &[String],
    theirs: &Table,
    stored: &[String],
    skip: &[&str],
) -> Vec<String> {
    mine.fields
        .iter()
        .enumerate()
        .filter(|(_, f)| !skip.iter().any(|s| s.eq_ignore_ascii_case(f)) && !ignored(&mine.name, f))
        .filter(|(i, f)| ours.get(*i).map_or("", String::as_str) != theirs.value(stored, f))
        .map(|(_, f)| f.clone())
        .collect()
}

/// Rows of a table (by index) in parent-before-child order.
fn by_depth(table: &Table, rows: &[usize]) -> Vec<usize> {
    let Some(parent) = parent_column(&table.name).and_then(|c| table.field(c)) else {
        return rows.to_vec();
    };
    let Some(k) = key_index(table) else {
        return rows.to_vec();
    };
    let index: HashMap<i64, usize> = rows.iter().map(|&i| (num(&table.rows[i][k]), i)).collect();
    let depth = |mut i: usize| {
        let mut d = 0;
        while let Some(&p) = table.rows[i]
            .get(parent)
            .and_then(|v| int(v))
            .and_then(|p| index.get(&p))
        {
            d += 1;
            if d > rows.len() {
                break;
            }
            i = p;
        }
        d
    };
    let mut out = rows.to_vec();
    out.sort_by_key(|&i| depth(i));
    out
}

/// The short-name path of a value row: its parents' short names and its own.
fn value_path(table: &Table, row: &[String]) -> String {
    let Some(g) = value_group(&table.name) else {
        return String::new();
    };
    let mut parts = vec![table.value(row, g.4).to_string()];
    if let Some(parent) = g.3 {
        let mut seen = HashSet::new();
        let mut current = int(table.value(row, parent));
        while let Some(id) = current.filter(|id| seen.insert(*id)) {
            let Some(r) = row_by_id(table, id) else { break };
            parts.push(table.value(r, g.4).to_string());
            current = int(table.value(r, parent));
        }
    }
    parts.reverse();
    parts.join("\u{1f}")
}

/// A row with every reference rewritten by `map` (the project's ids so far).
fn mapped(table: &Table, row: &[String], map: &Remap) -> Vec<String> {
    let name = uname(&table.name);
    let mut out = row.to_vec();
    for (src, col, target) in REFERENCES {
        if *src != name {
            continue;
        }
        if let Some(i) = table.field(col) {
            if let Some(to) = out
                .get(i)
                .and_then(|v| int(v))
                .and_then(|id| map.get(target, id))
            {
                out[i] = to.to_string();
            }
        }
    }
    out
}

/// What became of an owner row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fate {
    /// The project's own stored row.
    Owned,
    /// The stored row with this id.
    Merged(i64),
    /// A new dictionary row with this id.
    New(i64),
    /// The stored row with this id, which takes the project's values.
    Updated(i64),
}

impl Merger {
    pub fn new(dict: Document) -> Self {
        Self {
            dict,
            reserved: BTreeMap::new(),
            floor: BTreeMap::new(),
            next: BTreeMap::new(),
            changed: false,
            choices: BTreeMap::new(),
        }
    }

    /// Keep `id` of `table` out of the dictionary.
    pub fn reserve(&mut self, table: &str, id: i64) {
        self.reserved.entry(uname(table)).or_default().insert(id);
    }

    /// New rows keep their ids only above these.
    pub fn set_floor(&mut self, floor: &BTreeMap<String, i64>) {
        for (t, v) in floor {
            let e = self.floor.entry(uname(t)).or_insert(*v);
            *e = (*e).max(*v);
        }
    }

    fn dict_has(&self, table: &str, id: i64) -> bool {
        self.dict
            .table(table)
            .and_then(|t| row_by_id(t, id))
            .is_some()
    }

    fn free(&self, table: &str, id: i64, pending: &BTreeSet<(String, i64)>) -> bool {
        let t = uname(table);
        id > 0
            && id > self.floor.get(&t).copied().unwrap_or(0)
            && !self.dict_has(table, id)
            && !self.reserved.get(&t).is_some_and(|r| r.contains(&id))
            && !pending.contains(&(t, id))
    }

    /// A new id for `table` above everything the dictionary, the reserved
    /// rows, the floor and `doc` hold.
    fn allocate(&mut self, table: &str, doc: &Document) -> i64 {
        let t = uname(table);
        let highs = [
            self.dict.table(table).and_then(table_highs_of),
            self.reserved.get(&t).and_then(|r| r.last().copied()),
            self.floor.get(&t).copied(),
            doc.table(table).and_then(table_highs_of),
            self.next.get(&t).copied(),
        ];
        let id = highs.into_iter().flatten().max().unwrap_or(0) + 1;
        self.next.insert(t, id);
        id
    }

    fn id_for(
        &mut self,
        table: &str,
        id: i64,
        doc: &Document,
        pending: &mut BTreeSet<(String, i64)>,
    ) -> i64 {
        let new = if self.free(table, id, pending) {
            id
        } else {
            self.allocate(table, doc)
        };
        pending.insert((uname(table), new));
        new
    }

    /// A name for a copy that no stored row of the table has.
    fn copy_name(
        &self,
        table: &Table,
        row: &[String],
        policy: &Policy,
    ) -> Option<(String, String)> {
        let names = names_of(&table.name);
        let column = *names.first()?;
        let original = table.value(row, column).to_string();
        let taken = |candidate: &str| {
            let mut probe = row.to_vec();
            if let Some(i) = table.field(column) {
                probe[i] = candidate.to_string();
            }
            let wanted = identity(table, &probe);
            self.dict.table(&table.name).is_some_and(|d| {
                d.rows.iter().any(|r| {
                    let mut stored = vec![String::new(); table.fields.len()];
                    for (i, f) in table.fields.iter().enumerate() {
                        stored[i] = d.value(r, f).to_string();
                    }
                    identity(table, &stored) == wanted
                })
            })
        };
        let candidates = (1..).map(|n| match policy {
            Policy::Migration { label } if n == 1 => format!("{original} ({label})"),
            Policy::Migration { label } => format!("{original} ({label} {n})"),
            Policy::Save => format!("{original}_{n}"),
        });
        for c in candidates.take(10_000) {
            if !taken(&c) {
                return Some((column.to_string(), c));
            }
        }
        None
    }

    /// Merge the workspace rows of `doc` into the dictionary. `mark` holds
    /// the (table, id) of the rows the project read from the dictionary.
    pub fn merge(
        &mut self,
        doc: &Document,
        policy: &Policy,
        mark: &BTreeSet<(String, i64)>,
    ) -> Outcome {
        let scopes = Scopes::of(doc);
        let mut map = Remap::default();
        let mut events = vec![];
        let mut pending: BTreeSet<(String, i64)> = BTreeSet::new();
        let mut renames: Vec<Rename> = vec![];
        // Rows to write, by table and the id they will have.
        let mut write: BTreeSet<(String, i64)> = BTreeSet::new();
        // Owners whose unkeyed child rows (category and role assignments)
        // are written from the project.
        let mut write_children: BTreeSet<(String, i64)> = BTreeSet::new();
        // Owners the project does not have whose unkeyed child rows it adds to.
        let mut add_children: BTreeSet<(String, i64)> = BTreeSet::new();
        let mut fates: HashMap<(String, i64), Fate> = HashMap::new();
        let shared_rows = |t: &Table| -> Vec<usize> {
            (0..t.rows.len())
                .filter(|&i| scopes.shared(t, &t.rows[i]))
                .collect()
        };
        // Project rows of the mixed tables whose id a stored row took.
        for table in MIXED_TABLES {
            let Some(t) = doc.table(table) else { continue };
            let Some(k) = key_index(t) else { continue };
            for r in &t.rows {
                if scopes.shared(t, r) {
                    continue;
                }
                let id = num(&r[k]);
                if self.dict_has(table, id) {
                    let to = self.allocate(table, doc);
                    pending.insert((uname(table), to));
                    map.insert(table, id, to);
                    events.push(Event {
                        kind: EventKind::ProjectRenumbered,
                        table: table.to_string(),
                        name: display_name(t, r),
                        from: id,
                        to,
                        renamed: None,
                        differences: vec![],
                    });
                }
            }
        }
        for owner in ORDER {
            let Some(t) = doc.table(owner) else { continue };
            let Some(k) = key_index(t) else { continue };
            for i in by_depth(t, &shared_rows(t)) {
                let row = &t.rows[i];
                let id = num(&row[k]);
                let name = display_name(t, row);
                if mark.contains(&(uname(owner), id)) {
                    fates.insert((uname(owner), id), Fate::Owned);
                    pending.insert((uname(owner), id));
                    write.insert((uname(owner), id));
                    write_children.insert((uname(owner), id));
                    continue;
                }
                let ours = mapped(t, row, &map);
                let wanted = identity(t, &ours);
                let mut same_name = vec![];
                let mut found = None;
                if let Some(d) = self.dict.table(owner) {
                    let dk = key_index(d);
                    for stored in &d.rows {
                        let sid = dk.map_or(0, |k| num(&stored[k]));
                        // The project's own stored rows are replaced by its
                        // versions, so nothing else of it matches them.
                        if mark.contains(&(uname(owner), sid)) {
                            continue;
                        }
                        let theirs = {
                            let mut v = vec![String::new(); t.fields.len()];
                            for (i, f) in t.fields.iter().enumerate() {
                                v[i] = d.value(stored, f).to_string();
                            }
                            v
                        };
                        let named = wanted.is_some() && identity(t, &theirs) == wanted;
                        // A stored copy made from a row of this name (an
                        // earlier save or another project's import of it).
                        let derived = !named && wanted.is_some() && derived_from(t, &ours, &theirs);
                        if wanted.is_some() && !named && !derived {
                            continue;
                        }
                        let key = key_column(owner).unwrap_or("");
                        let name_column = names_of(owner).first().copied().unwrap_or("");
                        if same(t, &ours, d, stored, &[key, name_column])
                            && self.children_match(doc, &scopes, owner, id, sid, &map)
                            && self.values_match(doc, &scopes, owner, id, sid, &map)
                        {
                            let stored_name = d.value(stored, name_column).to_string();
                            if named {
                                found = Some((sid, None));
                                break;
                            }
                            if found.is_none() {
                                found = Some((sid, Some((name_column.to_string(), stored_name))));
                            }
                            continue;
                        }
                        if named {
                            same_name.push((sid, differences(t, &ours, d, stored, &[key])));
                        }
                    }
                }
                if let Some((sid, renamed)) = found {
                    map.insert(owner, id, sid);
                    fates.insert((uname(owner), id), Fate::Merged(sid));
                    self.map_children(doc, &scopes, owner, id, sid, &mut map);
                    if let Some((column, value)) = &renamed {
                        renames.push(Rename {
                            table: uname(owner),
                            id: sid,
                            column: column.clone(),
                            from: t.value(row, column).to_string(),
                            to: value.clone(),
                        });
                    }
                    events.push(Event {
                        kind: EventKind::Merged,
                        table: owner.to_string(),
                        name,
                        from: id,
                        to: sid,
                        renamed: renamed.map(|r| r.1),
                        differences: vec![],
                    });
                    continue;
                }
                let choice = self.choices.get(&uname(owner)).copied().unwrap_or_default();
                if let (Some((sid, diffs)), true) = (same_name.first(), choice != Choice::Copy) {
                    let (sid, diffs) = (*sid, diffs.clone());
                    map.insert(owner, id, sid);
                    if choice == Choice::Keep {
                        fates.insert((uname(owner), id), Fate::Merged(sid));
                        self.map_children(doc, &scopes, owner, id, sid, &mut map);
                    } else {
                        fates.insert((uname(owner), id), Fate::Updated(sid));
                        pending.insert((uname(owner), sid));
                        write.insert((uname(owner), sid));
                        write_children.insert((uname(owner), sid));
                        // The project's keyed children (rates, periods)
                        // replace the stored ones.
                        self.remove_keyed_children(owner, sid);
                    }
                    events.push(Event {
                        kind: if choice == Choice::Keep {
                            EventKind::Kept
                        } else {
                            EventKind::Updated
                        },
                        table: owner.to_string(),
                        name,
                        from: id,
                        to: sid,
                        renamed: None,
                        differences: diffs,
                    });
                    continue;
                }
                let to = self.id_for(owner, id, doc, &mut pending);
                map.insert(owner, id, to);
                fates.insert((uname(owner), id), Fate::New(to));
                write.insert((uname(owner), to));
                write_children.insert((uname(owner), to));
                let (kind, renamed, diffs) = if let Some((_, diffs)) = same_name.first() {
                    let renamed = self.copy_name(t, &ours, policy);
                    if let Some((column, value)) = &renamed {
                        renames.push(Rename {
                            table: uname(owner),
                            id: to,
                            column: column.clone(),
                            from: t.value(row, column).to_string(),
                            to: value.clone(),
                        });
                    }
                    (EventKind::Copied, renamed.map(|r| r.1), diffs.clone())
                } else if to == id {
                    (EventKind::Added, None, vec![])
                } else {
                    (EventKind::Renumbered, None, vec![])
                };
                events.push(Event {
                    kind,
                    table: owner.to_string(),
                    name,
                    from: id,
                    to,
                    renamed,
                    differences: diffs,
                });
            }
            // The type's values.
            if let Some(g) = type_group(owner) {
                self.merge_values(
                    doc,
                    &scopes,
                    g,
                    mark,
                    &fates,
                    &mut map,
                    &mut pending,
                    &mut write,
                    &mut events,
                );
            }
        }
        // Keyed child rows, and values and rows whose owner or type the
        // project lacks.
        for (table, key) in WORKSPACE_TABLES {
            let Some(key) = key else { continue };
            if ORDER.contains(table) {
                continue;
            }
            let upper_table = uname(table);
            let Some(t) = doc.table(table) else { continue };
            let Some(k) = t.field(key) else { continue };
            for i in by_depth(t, &shared_rows(t)) {
                let row = &t.rows[i];
                let id = num(&row[k]);
                if let Some(g) = value_group(table) {
                    if fates.contains_key(&(uname(g.0), num(t.value(row, g.2)))) {
                        continue;
                    }
                }
                if mark.contains(&(upper_table.clone(), id)) {
                    pending.insert((upper_table.clone(), id));
                    write.insert((upper_table.clone(), id));
                    continue;
                }
                // A child of a merged owner was matched to the stored child.
                if let Some((col, owner)) = owner_of(table) {
                    if let Some(Fate::Merged(_)) =
                        fates.get(&(owner.to_string(), num(t.value(row, col))))
                    {
                        continue;
                    }
                }
                let to = self.id_for(table, id, doc, &mut pending);
                map.insert(table, id, to);
                write.insert((upper_table.clone(), to));
                events.push(Event {
                    kind: if to == id {
                        EventKind::Added
                    } else {
                        EventKind::Renumbered
                    },
                    table: table.to_string(),
                    name: String::new(),
                    from: id,
                    to,
                    renamed: None,
                    differences: vec![],
                });
            }
        }
        // Unkeyed children of owners the project lacks are written as they are.
        for (child, col, owner) in OWNED {
            if key_column(child).is_some() {
                continue;
            }
            let Some(t) = doc.table(child) else { continue };
            for r in &t.rows {
                let o = num(t.value(r, col));
                if !fates.contains_key(&(owner.to_string(), o)) {
                    let to = map.get(owner, o).unwrap_or(o);
                    add_children.insert((owner.to_string(), to));
                }
            }
        }
        // Apply to the project's document.
        let mut document = doc.clone();
        remap(&mut document, &map);
        for r in &renames {
            rename(&mut document, &r.table, r.id, &r.column, &r.to);
        }
        level_priority_renames(&mut document, &renames);
        // Write the rows into the dictionary.
        let dscopes = Scopes::of(&document);
        for t in &document.tables {
            let name = uname(&t.name);
            if !is_workspace_table(&name) {
                continue;
            }
            match key_index(t) {
                Some(k) => {
                    for r in &t.rows {
                        if !dscopes.shared(t, r) {
                            continue;
                        }
                        let id = num(&r[k]);
                        if write.contains(&(name.clone(), id)) {
                            self.put(t, r, Some(id));
                        }
                    }
                }
                None => {
                    let Some((col, owner)) = owner_of(&name) else {
                        continue;
                    };
                    let Some(c) = t.field(col) else { continue };
                    let owners: BTreeSet<i64> = t
                        .rows
                        .iter()
                        .map(|r| num(&r[c]))
                        .filter(|o| write_children.contains(&(owner.to_string(), *o)))
                        .collect();
                    for o in &owners {
                        self.replace_children(t, *o, col);
                    }
                    for r in &t.rows {
                        let o = num(&r[c]);
                        if add_children.contains(&(owner.to_string(), o))
                            && !write_children.contains(&(owner.to_string(), o))
                        {
                            self.add_unkeyed(t, r);
                        }
                    }
                }
            }
        }
        // Owned owners whose unkeyed children the project removed entirely.
        for (child, col, owner) in OWNED {
            if key_column(child).is_some() {
                continue;
            }
            let empty = Table {
                name: child.to_string(),
                fields: document
                    .table(child)
                    .map_or(vec![col.to_string()], |t| t.fields.clone()),
                rows: vec![],
            };
            for ((o, id), fate) in &fates {
                let id = match fate {
                    Fate::Owned => id,
                    Fate::Updated(sid) => sid,
                    _ => continue,
                };
                if o == owner {
                    let has = document
                        .table(child)
                        .is_some_and(|t| t.rows.iter().any(|r| num(t.value(r, col)) == *id));
                    if !has {
                        self.replace_children(&empty, *id, col);
                    }
                }
            }
        }
        self.sort();
        let mut kept = BTreeSet::new();
        for t in &doc.tables {
            let Some(k) = key_index(t) else { continue };
            for r in &t.rows {
                if scopes.shared(t, r) {
                    let id = num(&r[k]);
                    if map.get(&t.name, id).is_none() {
                        kept.insert((uname(&t.name), id));
                    }
                }
            }
        }
        Outcome {
            document,
            remap: map,
            events,
            mark: kept,
        }
    }

    /// Children of the project's owner `id` and of stored owner `sid`, as the
    /// same multiset of rows (references mapped, own ids and the owner aside).
    fn children_match(
        &self,
        doc: &Document,
        scopes: &Scopes,
        owner: &str,
        id: i64,
        sid: i64,
        map: &Remap,
    ) -> bool {
        for (child, col, o) in OWNED {
            if !o.eq_ignore_ascii_case(owner) {
                continue;
            }
            let skip = [key_column(child).unwrap_or(""), *col];
            let ours: Vec<Vec<String>> = doc.table(child).map_or(vec![], |t| {
                t.rows
                    .iter()
                    .filter(|r| scopes.shared(t, r) && num(t.value(r, col)) == id)
                    .map(|r| mapped(t, r, map))
                    .collect()
            });
            let Some(mine) = doc.table(child) else {
                if self
                    .dict
                    .table(child)
                    .is_some_and(|d| d.rows.iter().any(|r| num(d.value(r, col)) == sid))
                {
                    return false;
                }
                continue;
            };
            let theirs: Vec<&Vec<String>> = self.dict.table(child).map_or(vec![], |d| {
                d.rows
                    .iter()
                    .filter(|r| num(d.value(r, col)) == sid)
                    .collect()
            });
            if ours.len() != theirs.len() {
                return false;
            }
            let Some(d) = self.dict.table(child) else {
                if ours.is_empty() {
                    continue;
                }
                return false;
            };
            let mut used = vec![false; theirs.len()];
            for r in &ours {
                let hit =
                    (0..theirs.len()).find(|&j| !used[j] && same(mine, r, d, theirs[j], &skip));
                match hit {
                    Some(j) => used[j] = true,
                    None => return false,
                }
            }
        }
        true
    }

    /// Map the project's keyed children of `id` to the stored children of `sid`.
    fn map_children(
        &self,
        doc: &Document,
        scopes: &Scopes,
        owner: &str,
        id: i64,
        sid: i64,
        map: &mut Remap,
    ) {
        for (child, col, o) in OWNED {
            if !o.eq_ignore_ascii_case(owner) {
                continue;
            }
            let Some(key) = key_column(child) else {
                continue;
            };
            let (Some(mine), Some(d)) = (doc.table(child), self.dict.table(child)) else {
                continue;
            };
            let skip = [key, *col];
            let theirs: Vec<&Vec<String>> = d
                .rows
                .iter()
                .filter(|r| num(d.value(r, col)) == sid)
                .collect();
            let mut used = vec![false; theirs.len()];
            for r in mine
                .rows
                .iter()
                .filter(|r| scopes.shared(mine, r) && num(mine.value(r, col)) == id)
            {
                let ours = mapped(mine, r, map);
                if let Some(j) =
                    (0..theirs.len()).find(|&j| !used[j] && same(mine, &ours, d, theirs[j], &skip))
                {
                    used[j] = true;
                    map.insert(child, num(mine.value(r, key)), num(d.value(theirs[j], key)));
                }
            }
        }
    }

    /// Whether every value of the project's type `id` whose path the stored
    /// type `sid` also has says the same there.
    fn values_match(
        &self,
        doc: &Document,
        scopes: &Scopes,
        owner: &str,
        id: i64,
        sid: i64,
        map: &Remap,
    ) -> bool {
        let Some(g) = type_group(owner) else {
            return true;
        };
        let (Some(mine), Some(d)) = (doc.table(g.1), self.dict.table(g.1)) else {
            return true;
        };
        let stored: HashMap<String, &Vec<String>> = d
            .rows
            .iter()
            .filter(|r| num(d.value(r, g.2)) == sid)
            .map(|r| (value_path(d, r), r))
            .collect();
        let skip = [key_column(g.1).unwrap_or(""), g.2, g.3.unwrap_or("")];
        mine.rows
            .iter()
            .filter(|r| scopes.shared(mine, r) && num(mine.value(r, g.2)) == id)
            .all(|r| {
                stored
                    .get(&value_path(mine, r))
                    .is_none_or(|s| same(mine, &mapped(mine, r, map), d, s, &skip))
            })
    }

    #[allow(clippy::too_many_arguments)]
    fn merge_values(
        &mut self,
        doc: &Document,
        scopes: &Scopes,
        g: &(&str, &str, &str, Option<&str>, &str),
        mark: &BTreeSet<(String, i64)>,
        fates: &HashMap<(String, i64), Fate>,
        map: &mut Remap,
        pending: &mut BTreeSet<(String, i64)>,
        write: &mut BTreeSet<(String, i64)>,
        events: &mut Vec<Event>,
    ) {
        let Some(t) = doc.table(g.1) else { return };
        let Some(k) = key_index(t) else { return };
        let rows: Vec<usize> = (0..t.rows.len())
            .filter(|&i| scopes.shared(t, &t.rows[i]))
            .collect();
        let value_table = uname(g.1);
        let stored: HashMap<(i64, String), i64> =
            self.dict.table(g.1).map_or(HashMap::new(), |d| {
                let dk = key_index(d);
                d.rows
                    .iter()
                    .map(|r| {
                        (
                            (num(d.value(r, g.2)), value_path(d, r)),
                            dk.map_or(0, |k| num(&r[k])),
                        )
                    })
                    .collect()
            });
        for i in by_depth(t, &rows) {
            let row = &t.rows[i];
            let id = num(&row[k]);
            let ty = num(t.value(row, g.2));
            let Some(fate) = fates.get(&(uname(g.0), ty)).copied() else {
                // The type is not in the project; handled as a loose row.
                continue;
            };
            if mark.contains(&(value_table.clone(), id)) {
                pending.insert((value_table.clone(), id));
                write.insert((value_table.clone(), id));
                continue;
            }
            if let Fate::Merged(sty) | Fate::Updated(sty) = fate {
                if let Some(&sid) = stored.get(&(sty, value_path(t, row))) {
                    map.insert(g.1, id, sid);
                    if matches!(fate, Fate::Updated(_)) {
                        // The stored value takes the project's values.
                        pending.insert((value_table.clone(), sid));
                        write.insert((value_table.clone(), sid));
                    }
                    continue;
                }
            }
            let to = self.id_for(g.1, id, doc, pending);
            map.insert(g.1, id, to);
            write.insert((value_table.clone(), to));
            if to != id {
                events.push(Event {
                    kind: EventKind::Renumbered,
                    table: g.1.to_string(),
                    name: value_path(t, row).replace('\u{1f}', "."),
                    from: id,
                    to,
                    renamed: None,
                    differences: vec![],
                });
            }
        }
    }

    /// Write one keyed row (laid out by `src`) into the dictionary under `id`.
    fn put(&mut self, src: &Table, row: &[String], id: Option<i64>) {
        let fresh = self.dict.table(&src.name).is_none();
        let d = self.dict.table_mut(&src.name);
        if fresh && d.rows.is_empty() {
            d.fields = src.fields.clone();
        }
        for f in &src.fields {
            if d.field(f).is_none() {
                d.fields.push(f.clone());
                for r in &mut d.rows {
                    r.push(String::new());
                }
            }
        }
        let new: Vec<String> = d
            .fields
            .iter()
            .map(|f| src.value(row, f).to_string())
            .collect();
        let k = key_column(&src.name).and_then(|k| d.field(k));
        let at = match (k, id) {
            (Some(k), Some(id)) => d.rows.iter().position(|r| int(&r[k]) == Some(id)),
            _ => None,
        };
        match at {
            Some(i) => {
                // Columns the project's table lacks keep their stored values.
                let mut merged = d.rows[i].clone();
                for (j, f) in d.fields.iter().enumerate() {
                    if src.field(f).is_some() {
                        merged[j] = new[j].clone();
                    }
                }
                if d.rows[i] != merged {
                    d.rows[i] = merged;
                    self.changed = true;
                }
            }
            None => {
                d.rows.push(new);
                self.changed = true;
            }
        }
    }

    /// Add an unkeyed row unless the dictionary already has it.
    fn add_unkeyed(&mut self, src: &Table, row: &[String]) {
        let present = self.dict.table(&src.name).is_some_and(|d| {
            d.rows.iter().any(|r| {
                src.fields
                    .iter()
                    .all(|f| d.value(r, f) == src.value(row, f))
            })
        });
        if !present {
            self.put(src, row, None);
        }
    }

    /// Replace the stored unkeyed child rows of owner `id` with those of `src`.
    fn replace_children(&mut self, src: &Table, id: i64, col: &str) {
        let before: Vec<Vec<String>> = self.dict.table(&src.name).map_or(vec![], |d| {
            d.rows
                .iter()
                .filter(|r| num(d.value(r, col)) == id)
                .cloned()
                .collect()
        });
        if before.is_empty() && !src.rows.iter().any(|r| num(src.value(r, col)) == id) {
            return;
        }
        let snapshot = self.dict.clone();
        if let Some(d) = self
            .dict
            .tables
            .iter_mut()
            .find(|t| t.name.eq_ignore_ascii_case(&src.name))
        {
            let c = d.field(col);
            d.rows.retain(|r| c.is_none_or(|c| num(&r[c]) != id));
        }
        for r in src.rows.iter().filter(|r| num(src.value(r, col)) == id) {
            self.put(src, r, None);
        }
        if self.dict.text() != snapshot.text() {
            self.changed = true;
        }
    }

    /// Remove the stored keyed child rows (rates, periods) of owner `id`.
    fn remove_keyed_children(&mut self, owner: &str, id: i64) {
        for (child, col, o) in OWNED {
            if !o.eq_ignore_ascii_case(owner) || key_column(child).is_none() {
                continue;
            }
            if let Some(d) = self
                .dict
                .tables
                .iter_mut()
                .find(|t| t.name.eq_ignore_ascii_case(child))
            {
                if let Some(c) = d.field(col) {
                    let before = d.rows.len();
                    d.rows.retain(|r| num(&r[c]) != id);
                    if d.rows.len() != before {
                        self.changed = true;
                    }
                }
            }
        }
    }

    /// Remove a stored row (and, for an owner, its unkeyed child rows).
    pub fn remove(&mut self, table: &str, id: i64) {
        let Some(k) = key_column(table) else { return };
        let mut removed = false;
        if let Some(d) = self
            .dict
            .tables
            .iter_mut()
            .find(|t| t.name.eq_ignore_ascii_case(table))
        {
            if let Some(c) = d.field(k) {
                let before = d.rows.len();
                d.rows.retain(|r| int(&r[c]) != Some(id));
                removed = d.rows.len() != before;
            }
        }
        if !removed {
            return;
        }
        self.changed = true;
        for (child, col, owner) in OWNED {
            if owner.eq_ignore_ascii_case(table) && key_column(child).is_none() {
                if let Some(d) = self
                    .dict
                    .tables
                    .iter_mut()
                    .find(|t| t.name.eq_ignore_ascii_case(child))
                {
                    if let Some(c) = d.field(col) {
                        d.rows.retain(|r| num(&r[c]) != id);
                    }
                }
            }
        }
    }

    /// Stored rows in id order (owner order for unkeyed rows), as hydrate
    /// places them.
    fn sort(&mut self) {
        for t in &mut self.dict.tables {
            let k = key_index(t);
            t.rows.sort_by_key(|r| match k {
                Some(k) => num(&r[k]),
                None => r.first().map_or(0, |v| num(v)),
            });
        }
        self.dict.tables.retain(|t| !t.rows.is_empty());
    }
}

fn table_highs_of(t: &Table) -> Option<i64> {
    let k = key_index(t)?;
    t.rows.iter().filter_map(|r| int(&r[k])).max()
}

fn rename(doc: &mut Document, table: &str, id: i64, column: &str, value: &str) {
    let Some(t) = doc
        .tables
        .iter_mut()
        .find(|t| t.name.eq_ignore_ascii_case(table))
    else {
        return;
    };
    let Some(k) = key_index(t) else { return };
    if let Some(i) = t.rows.iter().position(|r| int(&r[k]) == Some(id)) {
        t.set(i, column, value.to_string());
    }
}

/// A copy's new name.
struct Rename {
    table: String,
    id: i64,
    column: String,
    from: String,
    to: String,
}

/// A renamed global code type keeps its place in the leveling priorities,
/// which name code types by name (`ACTVCODE_<name>`).
fn level_priority_renames(doc: &mut Document, renames: &[Rename]) {
    let pairs: Vec<(String, String)> = renames
        .iter()
        .filter(|r| r.table == "ACTVTYPE")
        .map(|r| {
            (
                format!("ACTVCODE_{}", normalized_code_type(&r.from)),
                format!("ACTVCODE_{}", normalized_code_type(&r.to)),
            )
        })
        .collect();
    if pairs.is_empty() {
        return;
    }
    let Some(t) = doc
        .tables
        .iter_mut()
        .find(|t| t.name.eq_ignore_ascii_case("SCHEDOPTIONS"))
    else {
        return;
    };
    let Some(c) = t.field("LevelPriorityList") else {
        return;
    };
    for r in &mut t.rows {
        let items: Vec<String> = r[c]
            .split('\u{7f}')
            .map(|item| {
                let end = item.find([',', '|']).unwrap_or(item.len());
                let (key, rest) = item.split_at(end);
                match pairs.iter().find(|(from, _)| from == key) {
                    Some((_, to)) => format!("{to}{rest}"),
                    None => item.to_string(),
                }
            })
            .collect();
        r[c] = items.join("\u{7f}");
    }
}

#[cfg(test)]
mod tests;
