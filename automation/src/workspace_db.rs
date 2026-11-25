//! The workspace dictionary of an OpenXER database (workspace schema 3).
//!
//! A version 3 workspace keeps the workspace rows of every project (global
//! calendars, resources, roles, global and EPS codes, UDFs and the other enterprise
//! tables, see `openxer_xer::workspace_scope`) once, in `workspace_dictionary`,
//! and each project's stored text without them. Opening a project hydrates
//! the dictionary into its document; saving merges the project's workspace
//! rows back and stores the rest. `workspace_keys` holds the highest id each
//! workspace table has used, so a new row never takes an id another project
//! holds. A version 1 workspace (no `workspace_schema` preference) keeps
//! whole XER text per project and is read and written as before. A version
//! 2 workspace (earlier builds: EPS codes stayed in each project's text) is
//! upgraded by [`migrate`] before it is used.
//!
//! The desktop store and the command line share these functions.
use openxer_xer::workspace::Workspace;
use openxer_xer::workspace_scope::merge::{Choice, Event, EventKind, Merger, Outcome, Policy};
use openxer_xer::workspace_scope::{self, referenced_ids, table_highs, EpsPlace};
use openxer_xer::Document;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{BTreeMap, BTreeSet};

/// Preference naming the workspace schema; absent means version 1.
pub const SCHEMA_KEY: &str = "workspace_schema";
/// The schema with a workspace dictionary that also holds EPS code types.
pub const DICTIONARY_SCHEMA: i64 = 3;
/// The schema earlier builds wrote: a workspace dictionary without the EPS
/// code types, which stay in each project's text. Read only to upgrade it
/// ([`migrate`]); a build that knows only this schema refuses schema 3.
pub const EARLIER_DICTIONARY_SCHEMA: i64 = 2;

type R<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// The workspace schema of a database: 1 unless the preference says otherwise.
pub fn schema(c: &Connection) -> R<i64> {
    let has_preferences: bool = c
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='preferences'",
            [],
            |_| Ok(()),
        )
        .optional()
        .map_err(err)?
        .is_some();
    if !has_preferences {
        return Ok(1);
    }
    let value: Option<String> = c
        .query_row(
            "SELECT value FROM preferences WHERE key=?1",
            [SCHEMA_KEY],
            |r| r.get(0),
        )
        .optional()
        .map_err(err)?;
    match value {
        None => Ok(1),
        Some(v) => v
            .trim()
            .parse()
            .map_err(|_| format!("The workspace schema \"{v}\" is not a number.")),
    }
}

/// Make a database that has just been created a schema 3 workspace: new
/// workspaces keep one shared dictionary from the start. Call it only when
/// the `projects` table did not exist before; an existing version 1
/// workspace stays as it is until it is converted ([`migrate`]).
pub fn start_new(c: &Connection) -> R<()> {
    ensure(c)?;
    c.execute(
        "INSERT INTO preferences(key,value) VALUES(?1,?2) ON CONFLICT(key) DO NOTHING",
        params![SCHEMA_KEY, DICTIONARY_SCHEMA.to_string()],
    )
    .map(|_| ())
    .map_err(err)
}

/// Create the dictionary tables when missing.
pub fn ensure(c: &Connection) -> R<()> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspace_dictionary(id INTEGER PRIMARY KEY CHECK(id=1), xer_content TEXT NOT NULL, revision INTEGER NOT NULL DEFAULT 0);
         CREATE TABLE IF NOT EXISTS workspace_keys(table_name TEXT PRIMARY KEY, high INTEGER NOT NULL);",
    )
    .map_err(err)
}

/// Header of a dictionary that has no text yet.
const HEADER: &str =
    "ERMHDR\t25.12\t2026-11-08\tProject\tOpenXER\tOpenXER\tdbxDatabaseNoName\tProject Management\tUSD";

fn empty_dictionary() -> Document {
    Document::parse(&format!("{HEADER}\r\n%E\r\n")).expect("the empty dictionary parses")
}

/// A database that has no dictionary tables yet reads as an empty one.
fn missing_table<T>(e: rusqlite::Error) -> Result<Option<T>, rusqlite::Error> {
    match e {
        rusqlite::Error::SqliteFailure(_, Some(ref m)) if m.contains("no such table") => Ok(None),
        e => Err(e),
    }
}

/// The stored dictionary and its revision (an empty one at revision 0).
pub fn read_dictionary(c: &Connection) -> R<(Document, i64)> {
    let row: Option<(String, i64)> = c
        .query_row(
            "SELECT xer_content,revision FROM workspace_dictionary WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .or_else(missing_table)
        .map_err(err)?;
    match row {
        Some((text, revision)) => Ok((
            Document::parse(&text).map_err(|e| format!("The workspace dictionary: {e}"))?,
            revision,
        )),
        None => Ok((empty_dictionary(), 0)),
    }
}

/// The dictionary's revision alone (0 when there is none yet).
pub fn read_revision(c: &Connection) -> R<i64> {
    c.query_row(
        "SELECT revision FROM workspace_dictionary WHERE id=1",
        [],
        |r| r.get(0),
    )
    .optional()
    .or_else(|e| match e {
        rusqlite::Error::SqliteFailure(..) => Ok(None),
        e => Err(e),
    })
    .map(|r| r.unwrap_or(0))
    .map_err(err)
}

/// The highest id each workspace table has used.
pub fn read_keys(c: &Connection) -> R<BTreeMap<String, i64>> {
    let Some(mut stmt) = c
        .prepare("SELECT table_name,high FROM workspace_keys")
        .map(Some)
        .or_else(missing_table)
        .map_err(err)?
    else {
        return Ok(BTreeMap::new());
    };
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .map_err(err)?;
    rows.collect::<Result<BTreeMap<_, _>, _>>().map_err(err)
}

/// Raise the stored highs to at least `highs`.
fn raise_keys(c: &Connection, highs: &BTreeMap<String, i64>) -> R<()> {
    for (table, high) in highs {
        c.execute(
            "INSERT INTO workspace_keys(table_name,high) VALUES(?1,?2) ON CONFLICT(table_name) DO UPDATE SET high=MAX(high,excluded.high)",
            params![table, high],
        )
        .map_err(err)?;
    }
    Ok(())
}

fn write_dictionary(c: &Connection, dict: &Document, revision: i64) -> R<()> {
    c.execute(
        "INSERT INTO workspace_dictionary(id,xer_content,revision) VALUES(1,?1,?2) ON CONFLICT(id) DO UPDATE SET xer_content=excluded.xer_content,revision=excluded.revision",
        params![dict.text(), revision],
    )
    .map(|_| ())
    .map_err(err)
}

/// Write `dict` as the dictionary at the next revision (when it differs
/// from the stored one) and raise the key highs to cover it. Returns the
/// revision it is stored at.
pub fn replace_dictionary(c: &Connection, dict: &Document) -> R<i64> {
    ensure(c)?;
    let (stored, revision) = read_dictionary(c)?;
    if stored.text() == dict.text() {
        return Ok(revision);
    }
    write_dictionary(c, dict, revision + 1)?;
    raise_keys(c, &table_highs(dict))?;
    Ok(revision + 1)
}

/// What a project read from the dictionary when it was opened.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mark {
    /// The dictionary revision it was read at.
    pub revision: i64,
    /// The dictionary rows it holds, as (table, id).
    pub rows: BTreeSet<(String, i64)>,
}

/// The workspace EPS of a database (its preference row; the default tree
/// when there is none).
pub fn read_eps(c: &Connection) -> R<Workspace> {
    let text: Option<String> = c
        .query_row(
            "SELECT value FROM preferences WHERE key=?1",
            [openxer_xer::workspace::KEY],
            |r| r.get(0),
        )
        .optional()
        .or_else(|e| match e {
            rusqlite::Error::SqliteFailure(..) => Ok(None),
            e => Err(e),
        })
        .map_err(err)?;
    Ok(Workspace::from_text(&text.unwrap_or_default()))
}

/// Where workspace project `row` sits in the EPS (the first top-level node
/// for a project not stored yet).
pub fn eps_place(c: &Connection, row: Option<i64>) -> R<EpsPlace> {
    Ok(place_in(&read_eps(c)?, row))
}

/// [`eps_place`] in an EPS already read.
pub fn place_in(ws: &Workspace, row: Option<i64>) -> EpsPlace {
    let node = ws.node_of(row);
    EpsPlace {
        node,
        visible: ws.lineage(node).into_iter().collect(),
        nodes: ws.eps().iter().map(|n| n.id).collect(),
    }
}

/// Add the dictionary's rows to a project's document read from its stored
/// text, and set its id floors. `row` is the project's workspace row: it sees
/// only the EPS code types of its EPS node and those above (every one when
/// `None`). Returns what it read.
pub fn hydrate(c: &Connection, doc: &mut Document, row: Option<i64>) -> R<Mark> {
    let (dict, revision) = read_dictionary(c)?;
    hydrate_with(c, doc, row, &dict, revision)
}

/// [`hydrate`] with the dictionary already read.
pub fn hydrate_with(
    c: &Connection,
    doc: &mut Document,
    row: Option<i64>,
    dict: &Document,
    revision: i64,
) -> R<Mark> {
    let keys = read_keys(c)?;
    let visible = match row {
        Some(_) => Some(eps_place(c, row)?.visible),
        None => None,
    };
    let report = workspace_scope::hydrate_for(doc, dict, visible.as_ref());
    let mut floor = keys;
    for (t, high) in table_highs(dict) {
        let e = floor.entry(t).or_insert(high);
        *e = (*e).max(high);
    }
    doc.id_floor = floor;
    if !report.shadowed.is_empty() {
        let shown: Vec<String> = report
            .shadowed
            .iter()
            .take(5)
            .map(|(t, id)| format!("{t} {id}"))
            .collect();
        doc.warnings.push(format!(
            "{} workspace rows share an id with this project's own rows; the project's rows are used ({}).",
            report.shadowed.len(),
            shown.join(", ")
        ));
    }
    Ok(Mark {
        revision,
        rows: report.added,
    })
}

/// A project as `store` wrote it.
#[derive(Clone, Debug)]
pub struct Stored {
    /// The project text without workspace rows, to store in `projects`.
    pub text: String,
    /// What the project now holds of the dictionary, by the ids of the
    /// document that was saved.
    pub mark: Mark,
    pub outcome: Outcome,
    /// Rows the project deleted that another project still uses, kept.
    pub kept_in_use: Vec<(String, i64)>,
    /// EPS code types moved to the project's EPS node ([`EpsPlace`]).
    pub placed: usize,
}

impl Stored {
    /// Whether the saved document differs from the one given to [`store`]
    /// (rows renumbered, renamed, matched to stored rows or moved), so an
    /// open project should take `outcome.document`.
    pub fn changed_document(&self) -> bool {
        self.placed > 0
            || self
                .outcome
                .events
                .iter()
                .any(|e| e.kind != openxer_xer::workspace_scope::merge::EventKind::Added)
    }
}

/// Each workspace row the stored projects other than `except` refer to,
/// with the workspace ids of those projects.
pub fn users(c: &Connection, except: Option<i64>) -> R<BTreeMap<(String, i64), BTreeSet<i64>>> {
    let mut stmt = c
        .prepare("SELECT id,xer_content FROM projects")
        .map_err(err)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(err)?;
    let mut out: BTreeMap<(String, i64), BTreeSet<i64>> = BTreeMap::new();
    for row in rows {
        let (id, text) = row.map_err(err)?;
        if Some(id) == except {
            continue;
        }
        if let Ok(doc) = Document::parse(&text) {
            for k in referenced_ids(&doc) {
                out.entry(k).or_default().insert(id);
            }
        }
    }
    Ok(out)
}

/// Workspace project titles by id.
pub fn titles(c: &Connection) -> R<BTreeMap<i64, String>> {
    let mut stmt = c.prepare("SELECT id,title FROM projects").map_err(err)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(err)?;
    rows.collect::<Result<BTreeMap<_, _>, _>>().map_err(err)
}

/// The stored projects (workspace id and title) that use dictionary row
/// `table` `id`, other than `except`.
pub fn dictionary_usage(
    c: &Connection,
    table: &str,
    id: i64,
    except: Option<i64>,
) -> R<Vec<(i64, String)>> {
    let users = users(c, except)?;
    let titles = titles(c)?;
    Ok(users
        .get(&(table.to_ascii_uppercase(), id))
        .map(|ids| {
            ids.iter()
                .map(|i| (*i, titles.get(i).cloned().unwrap_or_default()))
                .collect()
        })
        .unwrap_or_default())
}

/// What kind of dictionary row a table holds, for messages.
pub fn kind_label(table: &str) -> &'static str {
    match table.to_ascii_uppercase().as_str() {
        "CALENDAR" => "Calendar",
        "RSRC" => "Resource",
        "ROLES" => "Role",
        "ACTVTYPE" => "Activity code",
        "ACTVCODE" => "Activity code value",
        "UDFTYPE" => "User defined field",
        "ACCOUNT" => "Cost account",
        "OBS" => "Responsible manager",
        "CURRTYPE" => "Currency",
        "UMEASURE" => "Unit of measure",
        "SHIFT" => "Shift",
        "RSRCCURVDATA" => "Resource curve",
        "FUNDSRC" => "Funding source",
        "FINTMPL" => "Financial period calendar",
        "RCATTYPE" => "Resource code",
        "PCATTYPE" => "Project code",
        "ROLECATTYPE" => "Role code",
        "ASGNMNTCATTYPE" => "Assignment code",
        "MEMOTYPE" => "Notebook topic",
        "COSTTYPE" => "Expense category",
        "DOCCATG" => "Document category",
        "DOCSTAT" => "Document status",
        "RISKTYPE" => "Risk category",
        "BASETYPE" => "Baseline type",
        "THRSPARM" => "Threshold parameter",
        "NONWORK" => "Nonwork type",
        "PROCGROUP" => "Step template",
        _ => "Dictionary row",
    }
}

/// One line per dictionary decision of an import a person should know
/// about: a row whose name the workspace has with different content (added
/// as a copy, kept or updated, with the fields that differ), then a count of
/// the rows that were the workspace's own already.
pub fn import_report(events: &[Event]) -> Vec<String> {
    let mut out = vec![];
    let differs = |e: &Event| {
        if e.differences.is_empty() {
            "its rates, periods or values".to_string()
        } else {
            e.differences.join(", ")
        }
    };
    for e in events {
        let kind = kind_label(&e.table);
        match e.kind {
            EventKind::Copied => out.push(format!(
                "{kind} \"{}\" is added as \"{}\": it differs from the workspace's in {}.",
                e.name,
                e.renamed.as_deref().unwrap_or(&e.name),
                differs(e)
            )),
            EventKind::Kept => out.push(format!(
                "{kind} \"{}\": the workspace's is kept (the file's differs in {}).",
                e.name,
                differs(e)
            )),
            EventKind::Updated => out.push(format!(
                "{kind} \"{}\": the workspace's takes the file's values ({}); every project that uses it sees them.",
                e.name,
                differs(e)
            )),
            _ => {}
        }
    }
    let shared = events
        .iter()
        .filter(|e| e.kind == EventKind::Merged)
        .count();
    let added = events
        .iter()
        .filter(|e| {
            matches!(e.kind, EventKind::Added | EventKind::Renumbered) && !e.name.is_empty()
        })
        .count();
    if shared + added > 0 {
        out.push(format!(
            "{added} new dictionary row(s) added, {shared} already the workspace's."
        ));
    }
    out
}

/// The tables whose rows an import would add as copies (or keep or update),
/// from its events: where a Keep or Update choice applies.
pub fn clash_tables(events: &[Event]) -> BTreeSet<String> {
    events
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                EventKind::Copied | EventKind::Kept | EventKind::Updated
            )
        })
        .map(|e| e.table.to_ascii_uppercase())
        .collect()
}

/// What storing these projects one after another would decide, without
/// writing anything: each project's merge events. Each entry is the
/// project's document, what it read from the dictionary and its workspace
/// row.
pub fn preview(
    c: &Connection,
    projects: &[(&Document, Option<&Mark>, Option<i64>)],
    choices: &Choices,
) -> R<Vec<Vec<Event>>> {
    c.execute_batch("SAVEPOINT openxer_preview").map_err(err)?;
    let result = (|| {
        let mut out = vec![];
        for (doc, mark, row) in projects {
            let stored = store_with(c, doc, *mark, *row, &Policy::Save, choices)?;
            out.push(stored.outcome.events);
        }
        Ok(out)
    })();
    c.execute_batch("ROLLBACK TO openxer_preview; RELEASE openxer_preview")
        .map_err(err)?;
    result
}

/// The name a person knows dictionary row `table` `id` by (its id when it
/// has no name), with what kind of row it is.
pub fn row_label(dict: &Document, table: &str, id: i64) -> String {
    let kind = kind_label(table);
    let name = dict
        .table(table)
        .and_then(|t| {
            let k = workspace_scope::key_column(table).and_then(|k| t.field(k))?;
            let r = t.rows.iter().find(|r| r[k].trim() == id.to_string())?;
            let column = match table.to_ascii_uppercase().as_str() {
                "ACTVCODE" => "short_name",
                "UDFTYPE" => "udf_type_label",
                _ => "",
            };
            let n = if column.is_empty() {
                workspace_scope::merge::display_name(t, r)
            } else {
                t.value(r, column).to_string()
            };
            (!n.is_empty()).then_some(n)
        })
        .unwrap_or_else(|| format!("{table} {id}"));
    format!("{kind} \"{name}\"")
}

/// The rows `delta`, applied to `dict` `forward` (redo) or backward (undo),
/// would remove that something still uses, with who uses each by name: a
/// stored project other than `except`, one of `open` (open projects with
/// their workspace ids, which may hold unsaved uses; one whose id is
/// `except` is skipped), or a dictionary row the delta keeps.
pub fn removals_in_use(
    c: &Connection,
    dict: &Document,
    delta: &workspace_scope::delta::Delta,
    forward: bool,
    except: Option<i64>,
    open: &[(Option<i64>, &Document)],
) -> R<BTreeMap<(String, i64), Vec<String>>> {
    let removes: BTreeSet<(String, i64)> = delta.removes(forward).into_iter().collect();
    let mut out: BTreeMap<(String, i64), Vec<String>> = BTreeMap::new();
    if removes.is_empty() {
        return Ok(out);
    }
    let titles = titles(c)?;
    let mut users = users(c, except)?;
    for (id, doc) in open {
        if id.is_some() && *id == except {
            continue;
        }
        for k in referenced_ids(doc) {
            users.entry(k).or_default().insert(id.unwrap_or(-1));
        }
    }
    for k in &removes {
        if let Some(ids) = users.get(k) {
            let mut names: Vec<String> = ids
                .iter()
                .map(|i| {
                    titles
                        .get(i)
                        .cloned()
                        .unwrap_or_else(|| "an open project".into())
                })
                .collect();
            names.dedup();
            out.insert(k.clone(), names);
        }
    }
    let mut after = dict.clone();
    delta.apply(&mut after, forward);
    let kept = workspace_scope::references_outside(&after, &removes);
    for k in &removes {
        if kept.contains(k) && !out.contains_key(k) {
            out.insert(k.clone(), vec!["another workspace dictionary row".into()]);
        }
    }
    Ok(out)
}

/// Workspace rows the stored projects other than `except` refer to.
pub fn usage(c: &Connection, except: Option<i64>) -> R<BTreeSet<(String, i64)>> {
    let mut stmt = c
        .prepare("SELECT id,xer_content FROM projects")
        .map_err(err)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(err)?;
    let mut out = BTreeSet::new();
    for row in rows {
        let (id, text) = row.map_err(err)?;
        if Some(id) == except {
            continue;
        }
        if let Ok(doc) = Document::parse(&text) {
            out.extend(referenced_ids(&doc));
        }
    }
    Ok(out)
}

/// Merge a project's workspace rows into the dictionary and return the text
/// to store for it. Run inside the transaction that writes the project row
/// (`row` is its id when it already exists). A dictionary row the project
/// read and has since deleted is removed unless a stored project, or another
/// dictionary row, still uses it.
pub fn store(c: &Connection, doc: &Document, mark: Option<&Mark>, row: Option<i64>) -> R<Stored> {
    store_with(c, doc, mark, row, &Policy::Save, &Choices::new())
}

/// What a row whose name the dictionary already has with different content
/// does, per owner table (upper case); a table not named makes a copy.
pub type Choices = BTreeMap<String, Choice>;

/// [`store`] naming copies by `policy`, and with `choices` for rows whose
/// name is taken by different content (an import's Keep or Update).
pub fn store_with(
    c: &Connection,
    doc: &Document,
    mark: Option<&Mark>,
    row: Option<i64>,
    policy: &Policy,
    choices: &Choices,
) -> R<Stored> {
    ensure(c)?;
    let (dict, revision) = read_dictionary(c)?;
    let keys = read_keys(c)?;
    let mut merger = Merger::new(dict);
    merger.set_floor(&keys);
    merger.choices = choices.clone();
    let empty = BTreeSet::new();
    let marked = mark.map_or(&empty, |m| &m.rows);
    // EPS code types the project brought keep being seen by it.
    let place = eps_place(c, row)?;
    let placed;
    let moved = workspace_scope::misplaced_eps_codes(doc, &place, marked);
    let doc = if moved > 0 {
        let mut d = doc.clone();
        workspace_scope::place_eps_codes(&mut d, &place, marked);
        placed = d;
        &placed
    } else {
        doc
    };
    let outcome = merger.merge(doc, policy, marked);
    note_shared_sample_rows(c, row, &outcome.events)?;
    let present = workspace_rows(doc);
    let gone: Vec<(String, i64)> = marked
        .iter()
        .filter(|k| !present.contains(*k))
        .cloned()
        .collect();
    let mut kept_in_use = vec![];
    if !gone.is_empty() {
        // Rows the deleted rows themselves refer to (a resource's rates)
        // do not keep them.
        let skip: BTreeSet<(String, i64)> = gone.iter().cloned().collect();
        let mut used = usage(c, row)?;
        used.extend(workspace_scope::references_outside(&merger.dict, &skip));
        used.extend(workspace_scope::references_outside(
            &outcome.document,
            &skip,
        ));
        for (table, id) in gone {
            if used.contains(&(table.clone(), id)) {
                kept_in_use.push((table, id));
            } else {
                merger.remove(&table, id);
            }
        }
    }
    let revision = if merger.changed {
        write_dictionary(c, &merger.dict, revision + 1)?;
        revision + 1
    } else {
        revision
    };
    let mut highs = table_highs(&merger.dict);
    for (t, h) in table_highs(&outcome.document) {
        let e = highs.entry(t).or_insert(h);
        *e = (*e).max(h);
    }
    raise_keys(c, &highs)?;
    let text = workspace_scope::split(&outcome.document).0.text();
    Ok(Stored {
        text,
        mark: Mark {
            revision,
            rows: outcome.mark.clone(),
        },
        outcome,
        kept_in_use,
        placed: moved,
    })
}

// ------------------------------------------------------------ the sample

/// Preference naming the workspace row of the first-run sample project and
/// the dictionary rows only it brought (JSON: `row`, `rows`). It goes when
/// the sample is removed.
pub const SAMPLE_KEY: &str = "sample_project";

/// The first-run sample in a workspace: its project row and the dictionary
/// rows that are the sample's alone.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SampleRecord {
    pub row: i64,
    pub rows: BTreeSet<(String, i64)>,
}

/// The workspace's sample record, if it still has its sample.
pub fn read_sample(c: &Connection) -> R<Option<SampleRecord>> {
    let text: Option<String> = c
        .query_row(
            "SELECT value FROM preferences WHERE key=?1",
            [SAMPLE_KEY],
            |r| r.get(0),
        )
        .optional()
        .or_else(missing_table)
        .map_err(err)?;
    let Some(text) = text else { return Ok(None) };
    let v: serde_json::Value = serde_json::from_str(&text).map_err(err)?;
    let rows = v["rows"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| Some((x[0].as_str()?.to_string(), x[1].as_i64()?)))
                .collect()
        })
        .unwrap_or_default();
    Ok(v["row"].as_i64().map(|row| SampleRecord { row, rows }))
}

/// Write the sample record, or remove it (`None`).
pub fn write_sample(c: &Connection, s: Option<&SampleRecord>) -> R<()> {
    match s {
        Some(s) => {
            let rows: Vec<serde_json::Value> = s
                .rows
                .iter()
                .map(|(t, id)| serde_json::json!([t, id]))
                .collect();
            let text = serde_json::json!({"row": s.row, "rows": rows}).to_string();
            c.execute(
                "INSERT INTO preferences(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![SAMPLE_KEY, text],
            )
        }
        None => c.execute("DELETE FROM preferences WHERE key=?1", [SAMPLE_KEY]),
    }
    .map(|_| ())
    .map_err(err)
}

/// Note that workspace row `row` is the first-run sample and that the
/// dictionary rows it holds now and `before` (the dictionary before the
/// sample was stored) lacked are the sample's own.
pub fn record_sample(c: &Connection, row: i64, before: &Document) -> R<()> {
    let (now, _) = read_dictionary(c)?;
    let had = workspace_rows(before);
    let rows = workspace_rows(&now)
        .into_iter()
        .filter(|k| !had.contains(k))
        .collect();
    write_sample(c, Some(&SampleRecord { row, rows }))
}

/// A project other than the sample (workspace row `row`) was merged into
/// the dictionary: a sample row it matched or chose (same name and content,
/// Keep or Update) is now its row too, and stays when the sample goes.
fn note_shared_sample_rows(c: &Connection, row: Option<i64>, events: &[Event]) -> R<()> {
    let Some(mut s) = read_sample(c)? else {
        return Ok(());
    };
    if row == Some(s.row) {
        return Ok(());
    }
    let before = s.rows.len();
    for e in events {
        if matches!(
            e.kind,
            EventKind::Merged | EventKind::Kept | EventKind::Updated
        ) {
            s.rows.remove(&(e.table.to_ascii_uppercase(), e.to));
        }
    }
    if s.rows.len() != before {
        write_sample(c, Some(&s))?;
    }
    Ok(())
}

/// What removing the sample does to the dictionary.
#[derive(Clone, Debug, Default)]
pub struct SampleRemoval {
    /// The dictionary change: the sample's rows out, renamed copies back.
    pub delta: workspace_scope::delta::Delta,
    /// How many rows go.
    pub removed: usize,
    /// Rows of other projects named `<name>_<n>` because the sample held
    /// `<name>`, which take their own name back.
    pub renamed: Vec<String>,
}

/// The dictionary change that removing the sample project (stored as
/// workspace row `sample`) brings, without writing anything: the rows only
/// the sample brought go, except those another stored project, one of
/// `open` (open projects with their workspace ids) or a remaining row still
/// uses; a row another project named `<name>_<n>` because the sample held
/// `<name>` takes its name back. `None` when the workspace has no sample
/// record for that row.
pub fn plan_sample_removal(
    c: &Connection,
    sample: i64,
    open: &[(Option<i64>, &Document)],
) -> R<Option<SampleRemoval>> {
    use openxer_xer::workspace_scope::merge::{NAMES, VALUE_GROUPS};
    use openxer_xer::workspace_scope::{key_column, references_outside, OWNED};
    let Some(s) = read_sample(c)?.filter(|s| s.row == sample) else {
        return Ok(None);
    };
    let (dict, _) = read_dictionary(c)?;
    let mut used = usage(c, Some(sample))?;
    for (id, doc) in open {
        if *id != Some(sample) {
            used.extend(referenced_ids(doc));
        }
    }
    let present = workspace_rows(&dict);
    let mut gone: BTreeSet<(String, i64)> = s
        .rows
        .iter()
        .filter(|k| present.contains(*k) && !used.contains(*k))
        .cloned()
        .collect();
    // A row that stays keeps what it refers to, its rates and periods, and
    // a code or field type that stays keeps its values.
    loop {
        let before = gone.len();
        let kept = references_outside(&dict, &gone);
        gone.retain(|k| !kept.contains(k));
        let owner_kept = |table: &str, column: &str, owner: &str, k: &(String, i64)| {
            k.0 == table
                && dict.table(table).is_some_and(|t| {
                    key_column(table)
                        .and_then(|kc| t.field(kc))
                        .is_some_and(|kc| {
                            t.rows.iter().any(|r| {
                                r[kc].trim().parse::<i64>() == Ok(k.1)
                                    && !gone.contains(&(
                                        owner.to_string(),
                                        t.value(r, column).trim().parse().unwrap_or(0),
                                    ))
                            })
                        })
                })
        };
        let keep: Vec<(String, i64)> = gone
            .iter()
            .filter(|k| {
                OWNED
                    .iter()
                    .any(|(table, column, owner)| owner_kept(table, column, owner, k))
                    || VALUE_GROUPS
                        .iter()
                        .any(|(ty, value, column, _, _)| owner_kept(value, column, ty, k))
            })
            .cloned()
            .collect();
        for k in keep {
            gone.remove(&k);
        }
        if gone.len() == before {
            break;
        }
    }
    let mut merger = Merger::new(dict.clone());
    for (table, id) in &gone {
        merger.remove(table, *id);
    }
    let mut after = merger.dict;
    // Copies named after a sample row take that name back.
    let mut renamed = vec![];
    for (table, id) in &gone {
        let Some(column) = NAMES
            .iter()
            .find(|(t, _)| t == table)
            .and_then(|(_, n)| n.first())
        else {
            continue;
        };
        let Some(name) = dict.table(table).and_then(|t| {
            let k = t.field(key_column(table)?)?;
            let r = t
                .rows
                .iter()
                .find(|r| r[k].trim().parse::<i64>() == Ok(*id))?;
            Some(t.value(r, column).to_string())
        }) else {
            continue;
        };
        let Some(t) = after
            .tables
            .iter_mut()
            .find(|t| t.name.eq_ignore_ascii_case(table))
        else {
            continue;
        };
        let Some(col) = t.field(column) else { continue };
        if t.rows.iter().any(|r| r[col] == name) {
            continue;
        }
        let copies: Vec<usize> = (0..t.rows.len())
            .filter(|&i| {
                t.rows[i][col]
                    .strip_prefix(name.as_str())
                    .and_then(|rest| rest.strip_prefix('_'))
                    .is_some_and(|n| !n.is_empty() && n.chars().all(|ch| ch.is_ascii_digit()))
            })
            .collect();
        if let [i] = copies[..] {
            renamed.push(format!(
                "{} \"{}\" is \"{name}\" again",
                kind_label(table),
                t.rows[i][col]
            ));
            t.rows[i][col] = name;
        }
    }
    Ok(Some(SampleRemoval {
        delta: workspace_scope::delta::diff(&dict, &after),
        removed: gone.len(),
        renamed,
    }))
}

/// The (table, id) of the keyed workspace rows of `doc`.
pub fn workspace_rows(doc: &Document) -> BTreeSet<(String, i64)> {
    let scopes = workspace_scope::Scopes::of(doc);
    let mut out = BTreeSet::new();
    for t in &doc.tables {
        let Some(k) = workspace_scope::key_column(&t.name).and_then(|k| t.field(k)) else {
            continue;
        };
        for r in &t.rows {
            if scopes.shared(t, r) {
                if let Ok(id) = r[k].trim().parse() {
                    out.insert((t.name.to_ascii_uppercase(), id));
                }
            }
        }
    }
    out
}

mod migrate;
pub use migrate::migrate;

#[cfg(test)]
mod tests;
