//! Which XER rows belong to the whole workspace and which to one project.
//!
//! A workspace keeps its shared dictionaries (global calendars, resources,
//! roles, global codes, UDFs, cost accounts and the other enterprise tables)
//! once, in a workspace dictionary, and each project's stored text without
//! them. [`split`] divides a project's document into those two parts and
//! [`hydrate`] puts the dictionary rows back into a project's document when it
//! is opened, so the edit layer, the engine and every front end keep seeing
//! one whole document. [`remap`] renumbers workspace rows and rewrites every
//! reference to them; [`used_subset`] keeps only the workspace rows one
//! project uses.
//!
//! Activity code types scoped to an EPS node live in the dictionary too,
//! with `wbs_id` naming a node of the workspace EPS (`crate::workspace`). A
//! project sees the EPS code types of its own node and of the nodes above it
//! ([`hydrate_for`]), and any it already uses. An XER names the EPS nodes of
//! the database it came from, which the workspace does not share, so an EPS
//! code type a project brings (an import, a migration) is moved to the
//! project's own node first ([`place_eps_codes`]).
use crate::edit::records::num;
use crate::{Document, Table};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub mod delta;
pub mod merge;
pub mod migrate;

/// Where a row lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Scope {
    /// Shared by every project in the workspace.
    Workspace,
    /// Visible to the projects under one EPS node (the row's `wbs_id`).
    Eps(i64),
    /// Belongs to one project.
    Project,
}

/// Workspace tables and the column holding each row's id. A table without
/// one (resource and role category assignments, resource roles) has its
/// rows identified by their whole content.
pub const WORKSPACE_TABLES: &[(&str, Option<&str>)] = &[
    ("ACCOUNT", Some("acct_id")),
    ("ACTVCODE", Some("actv_code_id")),
    ("ACTVTYPE", Some("actv_code_type_id")),
    ("ASGNMNTCATTYPE", Some("asgnmnt_catg_type_id")),
    ("ASGNMNTCATVAL", Some("asgnmnt_catg_id")),
    ("BASETYPE", Some("base_type_id")),
    ("CALENDAR", Some("clndr_id")),
    ("COSTTYPE", Some("cost_type_id")),
    ("CURRTYPE", Some("curr_id")),
    ("DOCCATG", Some("doc_catg_id")),
    ("DOCSTAT", Some("doc_status_id")),
    ("FINDATES", Some("fin_dates_id")),
    ("FINTMPL", Some("fintmpl_id")),
    ("FUNDSRC", Some("fund_id")),
    ("MEMOTYPE", Some("memo_type_id")),
    ("NONWORK", Some("nonwork_type_id")),
    ("OBS", Some("obs_id")),
    ("PCATTYPE", Some("proj_catg_type_id")),
    ("PCATVAL", Some("proj_catg_id")),
    ("PROCGROUP", Some("proc_group_id")),
    ("PROCITEM", Some("proc_item_id")),
    ("RCATTYPE", Some("rsrc_catg_type_id")),
    ("RCATVAL", Some("rsrc_catg_id")),
    ("RISKTYPE", Some("risk_type_id")),
    ("ROLECATTYPE", Some("role_catg_type_id")),
    ("ROLECATVAL", Some("role_catg_id")),
    ("ROLERATE", Some("role_rate_id")),
    ("ROLERCAT", None),
    ("ROLES", Some("role_id")),
    ("RSRC", Some("rsrc_id")),
    ("RSRCCURVDATA", Some("curv_id")),
    ("RSRCRATE", Some("rsrc_rate_id")),
    ("RSRCRCAT", None),
    ("RSRCROLE", None),
    ("SHIFT", Some("shift_id")),
    ("SHIFTPER", Some("shift_period_id")),
    ("THRSPARM", Some("thresh_parm_id")),
    ("UDFCODE", Some("udf_code_id")),
    ("UDFTYPE", Some("udf_type_id")),
    ("UMEASURE", Some("unit_id")),
];

/// Tables that hold both workspace and project rows in one id space.
pub const MIXED_TABLES: &[&str] = &["CALENDAR", "ACTVTYPE", "ACTVCODE"];

/// Columns that refer to a row of a workspace table: (table, column, target).
/// A column a document lacks is skipped. `UDFVALUE.fk_id` refers to the
/// table its UDF type names and is handled apart (see [`remap`]).
pub const REFERENCES: &[(&str, &str, &str)] = &[
    ("CALENDAR", "base_clndr_id", "CALENDAR"),
    ("PROJECT", "clndr_id", "CALENDAR"),
    ("TASK", "clndr_id", "CALENDAR"),
    ("RSRC", "clndr_id", "CALENDAR"),
    ("RSRC", "parent_rsrc_id", "RSRC"),
    ("RSRCRATE", "rsrc_id", "RSRC"),
    ("RSRCRCAT", "rsrc_id", "RSRC"),
    ("RSRCROLE", "rsrc_id", "RSRC"),
    ("TASKRSRC", "rsrc_id", "RSRC"),
    ("TASK", "rsrc_id", "RSRC"),
    ("RSRCLEVELLIST", "rsrc_id", "RSRC"),
    ("ROLES", "parent_role_id", "ROLES"),
    ("ROLERATE", "role_id", "ROLES"),
    ("ROLERCAT", "role_id", "ROLES"),
    ("RSRCROLE", "role_id", "ROLES"),
    ("RSRC", "role_id", "ROLES"),
    ("TASKRSRC", "role_id", "ROLES"),
    ("RCATVAL", "rsrc_catg_type_id", "RCATTYPE"),
    ("RSRCRCAT", "rsrc_catg_type_id", "RCATTYPE"),
    ("RCATVAL", "parent_rsrc_catg_id", "RCATVAL"),
    ("RSRCRCAT", "rsrc_catg_id", "RCATVAL"),
    ("ROLECATVAL", "role_catg_type_id", "ROLECATTYPE"),
    ("ROLERCAT", "role_catg_type_id", "ROLECATTYPE"),
    ("ROLECATVAL", "parent_role_catg_id", "ROLECATVAL"),
    ("ROLERCAT", "role_catg_id", "ROLECATVAL"),
    ("ACTVCODE", "actv_code_type_id", "ACTVTYPE"),
    ("TASKACTV", "actv_code_type_id", "ACTVTYPE"),
    ("ACTVCODE", "parent_actv_code_id", "ACTVCODE"),
    ("TASKACTV", "actv_code_id", "ACTVCODE"),
    ("UDFCODE", "udf_type_id", "UDFTYPE"),
    ("UDFVALUE", "udf_type_id", "UDFTYPE"),
    ("UDFVALUE", "udf_code_id", "UDFCODE"),
    ("ACCOUNT", "parent_acct_id", "ACCOUNT"),
    ("TASKRSRC", "acct_id", "ACCOUNT"),
    ("PROJCOST", "acct_id", "ACCOUNT"),
    ("PROJECT", "acct_id", "ACCOUNT"),
    ("OBS", "parent_obs_id", "OBS"),
    ("PROJWBS", "obs_id", "OBS"),
    ("PROJTHRS", "obs_id", "OBS"),
    ("PROJISSU", "obs_id", "OBS"),
    ("RSRC", "curr_id", "CURRTYPE"),
    ("FUNDSRC", "parent_fund_id", "FUNDSRC"),
    ("PROJFUND", "fund_id", "FUNDSRC"),
    ("FINDATES", "fintmpl_id", "FINTMPL"),
    ("PROJECT", "fintmpl_id", "FINTMPL"),
    ("TASKFIN", "fin_dates_id", "FINDATES"),
    ("TRSRCFIN", "fin_dates_id", "FINDATES"),
    ("TASKRSRC", "curv_id", "RSRCCURVDATA"),
    ("RSRC", "shift_id", "SHIFT"),
    ("SHIFTPER", "shift_id", "SHIFT"),
    ("RSRCRATE", "shift_period_id", "SHIFTPER"),
    ("PCATVAL", "proj_catg_type_id", "PCATTYPE"),
    ("PROJPCAT", "proj_catg_type_id", "PCATTYPE"),
    ("PCATVAL", "parent_proj_catg_id", "PCATVAL"),
    ("PROJPCAT", "proj_catg_id", "PCATVAL"),
    ("ASGNMNTCATVAL", "asgnmnt_catg_type_id", "ASGNMNTCATTYPE"),
    ("ASGNMNTACAT", "asgnmnt_catg_type_id", "ASGNMNTCATTYPE"),
    ("ASGNMNTCATVAL", "parent_asgnmnt_catg_id", "ASGNMNTCATVAL"),
    ("ASGNMNTACAT", "asgnmnt_catg_id", "ASGNMNTCATVAL"),
    ("TASKMEMO", "memo_type_id", "MEMOTYPE"),
    ("WBSMEMO", "memo_type_id", "MEMOTYPE"),
    ("PROJCOST", "cost_type_id", "COSTTYPE"),
    ("RSRC", "unit_id", "UMEASURE"),
    ("DOCUMENT", "doc_catg_id", "DOCCATG"),
    ("DOCUMENT", "doc_status_id", "DOCSTAT"),
    ("RISKTYPE", "parent_risk_type_id", "RISKTYPE"),
    ("PROJRISK", "risk_type_id", "RISKTYPE"),
    ("PROJRISK", "rsrc_id", "RSRC"),
    // The risk register under its other table name (`records::risk_table`).
    ("RISK", "risk_type_id", "RISKTYPE"),
    ("RISK", "rsrc_id", "RSRC"),
    ("PROJISSU", "rsrc_id", "RSRC"),
    ("PROJISSU", "thresh_parm_id", "THRSPARM"),
    ("PROJECT", "base_type_id", "BASETYPE"),
    ("ROLELIMIT", "role_id", "ROLES"),
    ("PROJTHRS", "thresh_parm_id", "THRSPARM"),
    ("PROCITEM", "proc_group_id", "PROCGROUP"),
];

fn upper(name: &str) -> String {
    name.to_ascii_uppercase()
}

/// Whether `table` holds workspace rows (all or, for [`MIXED_TABLES`], some).
pub fn is_workspace_table(table: &str) -> bool {
    WORKSPACE_TABLES
        .iter()
        .any(|(t, _)| t.eq_ignore_ascii_case(table))
}

/// The id column of a workspace table, `None` for other tables and for the
/// workspace tables whose rows have no id of their own.
pub fn key_column(table: &str) -> Option<&'static str> {
    WORKSPACE_TABLES
        .iter()
        .find(|(t, _)| t.eq_ignore_ascii_case(table))
        .and_then(|(_, k)| *k)
}

/// An integer cell, `None` when the text is not exactly one.
fn int(text: &str) -> Option<i64> {
    text.trim().parse::<i64>().ok()
}

/// Scopes of the rows of one document. Activity code values take their
/// type's scope, so the types are read once.
pub struct Scopes {
    code_types: HashMap<i64, Scope>,
}

impl Scopes {
    pub fn of(doc: &Document) -> Self {
        let mut code_types = HashMap::new();
        if let Some(t) = doc.table("ACTVTYPE") {
            for r in &t.rows {
                code_types.insert(num(t.value(r, "actv_code_type_id")), type_scope(t, r));
            }
        }
        Self { code_types }
    }

    /// Whether the row is kept in the workspace dictionary: workspace rows,
    /// and EPS-scoped code types with their values.
    pub fn shared(&self, table: &Table, row: &[String]) -> bool {
        matches!(self.scope(table, row), Scope::Workspace | Scope::Eps(_))
    }

    /// The scope of `row` of `table`.
    pub fn scope(&self, table: &Table, row: &[String]) -> Scope {
        let name = upper(&table.name);
        match name.as_str() {
            "CALENDAR" => {
                let kind = table.value(row, "clndr_type");
                if kind == "CA_Project"
                    || (kind.is_empty() && !table.value(row, "proj_id").is_empty())
                {
                    Scope::Project
                } else {
                    Scope::Workspace
                }
            }
            "ACTVTYPE" => type_scope(table, row),
            // A value whose type is not in the document stays with the project.
            "ACTVCODE" => self
                .code_types
                .get(&num(table.value(row, "actv_code_type_id")))
                .copied()
                .unwrap_or(Scope::Project),
            _ if is_workspace_table(&name) => Scope::Workspace,
            _ => Scope::Project,
        }
    }
}

fn type_scope(table: &Table, row: &[String]) -> Scope {
    match table.value(row, "actv_code_type_scope") {
        "AS_Global" => Scope::Workspace,
        "AS_EPS" => Scope::Eps(num(table.value(row, "wbs_id"))),
        "AS_Project" => Scope::Project,
        _ if table.value(row, "proj_id").is_empty() => Scope::Workspace,
        _ => Scope::Project,
    }
}

/// The scope of one row of `table` in `doc`.
pub fn scope(doc: &Document, table: &Table, row: &[String]) -> Scope {
    Scopes::of(doc).scope(table, row)
}

/// Whether the row moves to the workspace dictionary (see [`Scopes::shared`]).
fn shared(scopes: &Scopes, table: &Table, row: &[String]) -> bool {
    scopes.shared(table, row)
}

fn empty_like(doc: &Document) -> Document {
    Document {
        header: doc.header.clone(),
        tables: vec![],
        windows_encoding: doc.windows_encoding,
        warnings: vec![],
        lone_quotes: doc.lone_quotes.clone(),
        id_floor: BTreeMap::new(),
    }
}

/// Divide a document into its project part and its workspace part. The
/// project part keeps every table (a table left without rows keeps its place
/// and columns, so [`hydrate`] restores the document exactly); the workspace
/// part holds the workspace tables that had workspace rows, with the same
/// columns and row order.
pub fn split(doc: &Document) -> (Document, Document) {
    let scopes = Scopes::of(doc);
    let mut project = empty_like(doc);
    project.id_floor = doc.id_floor.clone();
    let mut workspace = empty_like(doc);
    for t in &doc.tables {
        let mut mine = Table {
            name: t.name.clone(),
            fields: t.fields.clone(),
            rows: vec![],
        };
        let mut theirs = mine.clone();
        if is_workspace_table(&t.name) {
            for r in &t.rows {
                if shared(&scopes, t, r) {
                    theirs.rows.push(r.clone());
                } else {
                    mine.rows.push(r.clone());
                }
            }
        } else {
            mine.rows = t.rows.clone();
        }
        if !theirs.rows.is_empty() {
            workspace.tables.push(theirs);
        }
        project.tables.push(mine);
    }
    (project, workspace)
}

/// What [`hydrate`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hydrated {
    /// Workspace rows added, by table, as (table, id) for tables with ids.
    pub added: BTreeSet<(String, i64)>,
    /// Rows added to tables whose rows have no id.
    pub added_unkeyed: usize,
    /// Dictionary rows not added because the project's own row has the same
    /// id and different content; the project's row is kept.
    pub shadowed: Vec<(String, i64)>,
}

/// The id of a row in `table` by the table's key column, if it has one.
fn key_index(table: &Table) -> Option<usize> {
    key_column(&table.name).and_then(|k| table.field(k))
}

/// Add the dictionary's rows to a project's document. Each row goes where
/// its id sorts among the project's rows (the order an export writes, so a
/// split document hydrates back to its exact text); columns the project's
/// table lacks are added after its own. A row whose id the project already
/// uses is not added: the project's row wins, and the clash is reported when
/// the two differ. Floors for new ids are the caller's (`Document::id_floor`).
pub fn hydrate(doc: &mut Document, dict: &Document) -> Hydrated {
    let mut out = Hydrated::default();
    for source in &dict.tables {
        if source.rows.is_empty() {
            continue;
        }
        let fresh = doc.table(&source.name).is_none();
        let target = doc.table_mut(&source.name);
        if fresh && target.rows.is_empty() {
            target.fields = source.fields.clone();
        }
        for f in &source.fields {
            if target.field(f).is_none() {
                target.fields.push(f.clone());
                for r in &mut target.rows {
                    r.push(String::new());
                }
            }
        }
        let columns: Vec<usize> = target
            .fields
            .iter()
            .map(|f| source.field(f).unwrap_or(usize::MAX))
            .collect();
        let convert = |r: &[String]| -> Vec<String> {
            columns
                .iter()
                .map(|&i| r.get(i).cloned().unwrap_or_default())
                .collect()
        };
        let key = key_index(target);
        let mut taken: HashMap<i64, usize> = HashMap::new();
        let mut present: HashSet<Vec<String>> = HashSet::new();
        for (i, r) in target.rows.iter().enumerate() {
            match key {
                Some(k) => {
                    taken.insert(r.get(k).map_or(0, |v| num(v)), i);
                }
                None => {
                    present.insert(r.clone());
                }
            }
        }
        let mut incoming: Vec<Vec<String>> = vec![];
        for r in &source.rows {
            let row = convert(r);
            match key {
                Some(k) => {
                    let id = num(&row[k]);
                    if let Some(&i) = taken.get(&id) {
                        if target.rows[i] != row {
                            out.shadowed.push((upper(&source.name), id));
                        }
                        continue;
                    }
                    taken.insert(id, usize::MAX);
                    out.added.insert((upper(&source.name), id));
                }
                None => {
                    if !present.insert(row.clone()) {
                        continue;
                    }
                    out.added_unkeyed += 1;
                }
            }
            incoming.push(row);
        }
        if incoming.is_empty() {
            continue;
        }
        let order = |r: &Vec<String>| match key {
            Some(k) => num(&r[k]),
            None => r.first().map_or(0, |v| num(v)),
        };
        incoming.sort_by_key(order);
        let existing = std::mem::take(&mut target.rows);
        let mut merged = Vec::with_capacity(existing.len() + incoming.len());
        let mut new = incoming.into_iter().peekable();
        for r in existing {
            let here = order(&r);
            while new.peek().is_some_and(|n| order(n) < here) {
                merged.push(new.next().unwrap());
            }
            merged.push(r);
        }
        merged.extend(new);
        target.rows = merged;
    }
    doc.keep_lone_quotes_of(dict);
    out
}

/// `dict` without the EPS-scoped code types (and their values) a project
/// under the EPS nodes `visible` does not see; a type `doc` already refers to
/// is kept. `None` when nothing is left out.
pub fn visible_dictionary(
    dict: &Document,
    doc: &Document,
    visible: &BTreeSet<i64>,
) -> Option<Document> {
    let types = dict.table("ACTVTYPE")?;
    let k = types.field("actv_code_type_id")?;
    let used: BTreeSet<i64> = references(doc)
        .into_iter()
        .filter(|(t, _)| t == "ACTVTYPE")
        .map(|(_, id)| id)
        .collect();
    let hidden: HashSet<i64> = types
        .rows
        .iter()
        .filter_map(|r| match type_scope(types, r) {
            Scope::Eps(node) if !visible.contains(&node) => {
                let id = num(&r[k]);
                (!used.contains(&id)).then_some(id)
            }
            _ => None,
        })
        .collect();
    if hidden.is_empty() {
        return None;
    }
    let mut out = dict.clone();
    for (table, column) in [
        ("ACTVTYPE", "actv_code_type_id"),
        ("ACTVCODE", "actv_code_type_id"),
    ] {
        if let Some(t) = out.tables.iter_mut().find(|t| t.name == table) {
            if let Some(c) = t.field(column) {
                t.rows.retain(|r| !hidden.contains(&num(&r[c])));
            }
        }
    }
    Some(out)
}

/// [`hydrate`] with only the EPS code types a project under the EPS nodes
/// `visible` sees ([`visible_dictionary`]); `None` adds every row.
pub fn hydrate_for(
    doc: &mut Document,
    dict: &Document,
    visible: Option<&BTreeSet<i64>>,
) -> Hydrated {
    match visible.and_then(|v| visible_dictionary(dict, doc, v)) {
        Some(view) => hydrate(doc, &view),
        None => hydrate(doc, dict),
    }
}

/// Where a project sits in the workspace EPS, for its EPS code types.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EpsPlace {
    /// The project's own node.
    pub node: i64,
    /// The node and the nodes above it: the EPS code types it sees.
    pub visible: BTreeSet<i64>,
    /// Every node of the workspace EPS.
    pub nodes: BTreeSet<i64>,
}

impl EpsPlace {
    /// Whether an EPS code type filed under `at` must move to the project's
    /// node: it names a node the project does not see, and it is new to the
    /// dictionary (not in `keep`) or names no node of this workspace at all.
    fn misplaced(&self, at: i64, id: i64, keep: &BTreeSet<(String, i64)>) -> bool {
        !self.visible.contains(&at)
            && (!self.nodes.contains(&at) || !keep.contains(&("ACTVTYPE".to_string(), id)))
    }
}

/// Move the EPS code types of `doc` that name an EPS node the project does
/// not see (an XER's own node ids, or none) to the project's node, so the
/// project keeps seeing them once they are shared ([`EpsPlace::misplaced`]).
/// `keep` holds the rows the project read from the dictionary. Returns how
/// many moved.
pub fn place_eps_codes(
    doc: &mut Document,
    place: &EpsPlace,
    keep: &BTreeSet<(String, i64)>,
) -> usize {
    let Some(t) = doc.tables.iter_mut().find(|t| upper(&t.name) == "ACTVTYPE") else {
        return 0;
    };
    let (Some(k), Some(w)) = (t.field("actv_code_type_id"), t.field("wbs_id")) else {
        return 0;
    };
    let mut moved = 0;
    for i in 0..t.rows.len() {
        if let Scope::Eps(at) = type_scope(t, &t.rows[i]) {
            if place.misplaced(at, num(&t.rows[i][k]), keep) {
                t.rows[i][w] = place.node.to_string();
                moved += 1;
            }
        }
    }
    moved
}

/// How many EPS code types [`place_eps_codes`] would move.
pub fn misplaced_eps_codes(
    doc: &Document,
    place: &EpsPlace,
    keep: &BTreeSet<(String, i64)>,
) -> usize {
    let Some(t) = doc.table("ACTVTYPE") else {
        return 0;
    };
    t.rows
        .iter()
        .filter(|r| match type_scope(t, r) {
            Scope::Eps(at) => place.misplaced(at, num(t.value(r, "actv_code_type_id")), keep),
            _ => false,
        })
        .count()
}

/// Highest id in each workspace table of `doc`, rows of every scope counted.
pub fn table_highs(doc: &Document) -> BTreeMap<String, i64> {
    let mut out = BTreeMap::new();
    for t in &doc.tables {
        let Some(k) = key_index(t) else { continue };
        let high = t.rows.iter().filter_map(|r| int(&r[k])).max();
        if let Some(high) = high {
            let e = out.entry(upper(&t.name)).or_insert(high);
            *e = (*e).max(high);
        }
    }
    out
}

/// New ids for workspace rows: table (upper case) to old id to new id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Remap {
    pub ids: BTreeMap<String, BTreeMap<i64, i64>>,
}

impl Remap {
    pub fn insert(&mut self, table: &str, from: i64, to: i64) {
        if from != to {
            self.ids.entry(upper(table)).or_default().insert(from, to);
        }
    }
    pub fn get(&self, table: &str, id: i64) -> Option<i64> {
        self.ids
            .get(&upper(table))
            .and_then(|m| m.get(&id))
            .copied()
    }
    pub fn is_empty(&self) -> bool {
        self.ids.values().all(BTreeMap::is_empty)
    }
}

/// The workspace table each UDF type's values refer to, by UDF type id.
fn udf_targets(doc: &Document) -> HashMap<i64, String> {
    let mut out = HashMap::new();
    if let Some(t) = doc.table("UDFTYPE") {
        for r in &t.rows {
            let table = upper(t.value(r, "table_name"));
            if key_column(&table).is_some() {
                out.insert(num(t.value(r, "udf_type_id")), table);
            }
        }
    }
    out
}

/// Renumber workspace rows and rewrite every reference to them, all at once
/// (a chain such as 5 to 6 and 6 to 7 moves 5 to 6, not to 7).
pub fn remap(doc: &mut Document, map: &Remap) {
    if map.is_empty() {
        return;
    }
    let udf = udf_targets(doc);
    for t in &mut doc.tables {
        let name = upper(&t.name);
        let mut columns: Vec<(usize, &str)> = vec![];
        if let Some(k) = key_column(&name).and_then(|k| t.field(k)) {
            columns.push((
                k,
                WORKSPACE_TABLES.iter().find(|(x, _)| *x == name).unwrap().0,
            ));
        }
        for (src, col, target) in REFERENCES {
            if *src == name {
                if let Some(i) = t.field(col) {
                    columns.push((i, target));
                }
            }
        }
        let fk = (name == "UDFVALUE")
            .then(|| t.field("fk_id").zip(t.field("udf_type_id")))
            .flatten();
        if columns.is_empty() && fk.is_none() {
            continue;
        }
        for r in &mut t.rows {
            if let Some((fk, ty)) = fk {
                // The UDF type column is rewritten below; look the target up
                // by the id the row was read with.
                if let Some(target) = r.get(ty).and_then(|v| int(v)).and_then(|id| udf.get(&id)) {
                    if let Some(to) = r
                        .get(fk)
                        .and_then(|v| int(v))
                        .and_then(|id| map.get(target, id))
                    {
                        r[fk] = to.to_string();
                    }
                }
            }
            for &(i, target) in &columns {
                if let Some(to) = r
                    .get(i)
                    .and_then(|v| int(v))
                    .and_then(|id| map.get(target, id))
                {
                    r[i] = to.to_string();
                }
            }
        }
    }
}

/// Renumber one workspace row (`table` id `from` becomes `to`) and every
/// reference to it.
pub fn remap_ids(doc: &mut Document, table: &str, from: i64, to: i64) {
    let mut map = Remap::default();
    map.insert(table, from, to);
    remap(doc, &map);
}

/// Workspace rows referred to by the rows of `doc` that `include` accepts,
/// as (table, id).
fn references_from(
    doc: &Document,
    include: impl Fn(&Table, &[String]) -> bool,
) -> BTreeSet<(String, i64)> {
    let udf = udf_targets(doc);
    let mut out = BTreeSet::new();
    for t in &doc.tables {
        let name = upper(&t.name);
        let columns: Vec<(usize, &str)> = REFERENCES
            .iter()
            .filter(|(src, _, _)| *src == name)
            .filter_map(|(_, col, target)| t.field(col).map(|i| (i, *target)))
            .collect();
        let fk = (name == "UDFVALUE")
            .then(|| t.field("fk_id").zip(t.field("udf_type_id")))
            .flatten();
        if columns.is_empty() && fk.is_none() {
            continue;
        }
        for r in &t.rows {
            if !include(t, r) {
                continue;
            }
            for &(i, target) in &columns {
                if let Some(id) = r.get(i).and_then(|v| int(v)) {
                    out.insert((target.to_string(), id));
                }
            }
            if let Some((fk, ty)) = fk {
                let target = r.get(ty).and_then(|v| int(v)).and_then(|id| udf.get(&id));
                if let (Some(target), Some(id)) = (target, r.get(fk).and_then(|v| int(v))) {
                    out.insert((target.clone(), id));
                }
            }
        }
    }
    out
}

/// Workspace rows any row of `doc` refers to, as (table, id).
pub fn references(doc: &Document) -> BTreeSet<(String, i64)> {
    references_from(doc, |_, _| true)
}

/// Workspace rows referred to by the rows of `doc` other than the rows in
/// `skip` and the unkeyed child rows of owners in `skip`: what still uses
/// a row once `skip` is deleted.
pub fn references_outside(
    doc: &Document,
    skip: &BTreeSet<(String, i64)>,
) -> BTreeSet<(String, i64)> {
    if skip.is_empty() {
        return references(doc);
    }
    references_from(doc, |t, r| {
        let name = upper(&t.name);
        if let Some(k) = key_index(t) {
            return !skip.contains(&(name, r.get(k).map_or(0, |v| num(v))));
        }
        match OWNED.iter().find(|(c, _, _)| *c == name) {
            Some((_, col, owner)) => !skip.contains(&(owner.to_string(), num(t.value(r, col)))),
            None => true,
        }
    })
}

/// Workspace rows the project rows of `doc` refer to, as (table, id).
pub fn referenced_ids(doc: &Document) -> BTreeSet<(String, i64)> {
    let scopes = Scopes::of(doc);
    references_from(doc, |t, r| !scopes.shared(t, r))
}

/// Child tables kept together with their owner row: (child, column naming the owner, owner).
pub const OWNED: &[(&str, &str, &str)] = &[
    ("RSRCRATE", "rsrc_id", "RSRC"),
    ("RSRCRCAT", "rsrc_id", "RSRC"),
    ("RSRCROLE", "rsrc_id", "RSRC"),
    ("ROLERATE", "role_id", "ROLES"),
    ("ROLERCAT", "role_id", "ROLES"),
    ("SHIFTPER", "shift_id", "SHIFT"),
    ("FINDATES", "fintmpl_id", "FINTMPL"),
    ("PROCITEM", "proc_group_id", "PROCGROUP"),
];

/// Workspace tables an export trims to the rows the project uses. The other
/// workspace tables (currencies, units, memo types and similar small lists)
/// are written whole.
const TRIMMED: &[&str] = &[
    "CALENDAR",
    "RSRC",
    "RSRCRATE",
    "RSRCRCAT",
    "RSRCROLE",
    "ROLES",
    "ROLERATE",
    "ROLERCAT",
    "ACTVTYPE",
    "ACTVCODE",
    "UDFTYPE",
    "UDFCODE",
    "ACCOUNT",
    "RSRCCURVDATA",
    "FUNDSRC",
];

/// The keyed workspace rows of `doc` project `pid` uses, as (table, id):
/// those its rows refer to, what those refer to in turn, the child rows of
/// used owners ([`OWNED`]) and the values of used code types.
pub fn used_rows(doc: &Document, pid: i64) -> BTreeSet<(String, i64)> {
    let scopes = Scopes::of(doc);
    let ours = |t: &Table, r: &[String]| {
        !scopes.shared(t, r)
            && t.field("proj_id")
                .is_none_or(|i| r.get(i).is_none_or(|v| v.is_empty() || num(v) == pid))
    };
    let mut used = references_from(doc, ours);
    // Follow references between workspace rows until nothing new is reached.
    loop {
        let before = used.len();
        let reached = references_from(doc, |t, r| {
            shared(&scopes, t, r)
                && key_index(t).is_some_and(|k| {
                    used.contains(&(upper(&t.name), r.get(k).map_or(0, |v| num(v))))
                })
        });
        used.extend(reached);
        // Children of used owners, and the values of used code types.
        for (child, col, owner) in OWNED {
            let Some(t) = doc.table(child) else { continue };
            for r in &t.rows {
                if used.contains(&(owner.to_string(), num(t.value(r, col)))) {
                    if let Some(k) = key_index(t) {
                        used.insert((child.to_string(), num(&r[k])));
                    }
                }
            }
        }
        for (value, col, ty) in [
            ("ACTVCODE", "actv_code_type_id", "ACTVTYPE"),
            ("UDFCODE", "udf_type_id", "UDFTYPE"),
        ] {
            let Some(t) = doc.table(value) else { continue };
            for r in &t.rows {
                if used.contains(&(ty.to_string(), num(t.value(r, col)))) {
                    if let Some(k) = key_index(t) {
                        used.insert((value.to_string(), num(&r[k])));
                    }
                }
            }
        }
        // A UDF value refers to its type; types of used values are kept.
        if used.len() == before {
            break;
        }
    }
    used
}

/// `doc` with only the workspace rows project `pid` uses: those its rows
/// refer to, what those refer to in turn, the child rows of used owners
/// ([`OWNED`]) and the values of used code types. Tables outside
/// [`TRIMMED`] are kept whole. Rows of other projects are left as they are.
pub fn used_subset(doc: &Document, pid: i64) -> Document {
    used_subset_of(doc, &[pid])
}

/// [`used_subset`] for a file of several projects: the workspace rows any of
/// `pids` uses.
pub fn used_subset_of(doc: &Document, pids: &[i64]) -> Document {
    let scopes = Scopes::of(doc);
    let mut used = BTreeSet::new();
    for pid in pids {
        used.extend(used_rows(doc, *pid));
    }
    let owner_of: HashMap<&str, (&str, &str)> = OWNED
        .iter()
        .map(|(child, col, owner)| (*child, (*col, *owner)))
        .collect();
    let mut out = doc.clone();
    for t in &mut out.tables {
        let name = upper(&t.name);
        if !TRIMMED.contains(&name.as_str()) {
            continue;
        }
        let key = key_index(t);
        let owner = owner_of
            .get(name.as_str())
            .and_then(|(col, owner)| t.field(col).map(|i| (i, *owner)));
        let rows = std::mem::take(&mut t.rows);
        let probe = Table {
            name: t.name.clone(),
            fields: t.fields.clone(),
            rows: vec![],
        };
        t.rows = rows
            .into_iter()
            .filter(|r| {
                if !shared(&scopes, &probe, r) {
                    return true;
                }
                match (key, owner) {
                    (Some(k), _) => used.contains(&(name.clone(), num(&r[k]))),
                    (None, Some((i, owner))) => used.contains(&(owner.to_string(), num(&r[i]))),
                    (None, None) => true,
                }
            })
            .collect();
    }
    out
}

#[cfg(test)]
mod tests;
