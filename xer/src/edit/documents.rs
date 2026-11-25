//! Work products and documents: the project's `DOCUMENT` rows (an outline by
//! `parent_doc_id`) and their links to activities and WBS elements
//! (`TASKDOC` rows, with a task or only a WBS element). The desktop's WPs &
//! Docs view, the command line and the MCP server all list and edit
//! documents through these functions, and they share [`local_file`], the
//! check a document's file location must pass before anything opens or
//! reads it.
use super::records::{self, num};
use crate::ProjectData;
use std::collections::HashSet;

pub const TABLE: &str = "DOCUMENT";
pub const LINKS: &str = "TASKDOC";
pub const STATUSES: &str = "DOCSTAT";
pub const CATEGORIES: &str = "DOCCATG";

type R<T> = Result<T, String>;

/// One work product or document of the project.
#[derive(Clone, Debug, PartialEq)]
pub struct Doc {
    pub id: i64,
    /// The parent document, when it is a document of the project.
    pub parent: Option<i64>,
    pub sequence: i64,
    pub title: String,
    pub reference: String,
    pub deliverable: bool,
    pub status: String,
    pub category: String,
    pub author: String,
    pub version: String,
    /// As stored, `YYYY-MM-DD HH:MM`.
    pub date: String,
    pub private_loc: String,
    pub public_loc: String,
    /// The Description tab as plain text.
    pub description: String,
}

/// A link of a document to an activity, or to a WBS element only.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    /// Row of `TASKDOC`.
    pub row: usize,
    /// 0 when the document is assigned to the WBS element itself.
    pub task_id: i64,
    pub wbs_id: i64,
    pub work_product: bool,
}

/// Rows of `table` for the project (all rows when it has no `proj_id`).
fn project_rows(p: &ProjectData, table: &str) -> Vec<usize> {
    let pid = p.model.project.proj_id;
    let Some(t) = p.document.table(table) else {
        return vec![];
    };
    let scoped = t.field("proj_id").is_some();
    (0..t.rows.len())
        .filter(|&r| !scoped || num(t.value(&t.rows[r], "proj_id")) == pid)
        .collect()
}

/// `(id, name)` of a dictionary table, in file order.
fn dictionary(p: &ProjectData, table: &str, key: &str, name: &str) -> Vec<(i64, String)> {
    p.document.table(table).map_or(vec![], |t| {
        t.rows
            .iter()
            .map(|r| (num(t.value(r, key)), t.value(r, name).to_string()))
            .filter(|(id, _)| *id != 0)
            .collect()
    })
}

fn name_of(list: &[(i64, String)], id: &str) -> String {
    if id.trim().is_empty() {
        return String::new();
    }
    list.iter()
        .find(|(k, _)| *k == num(id))
        .map_or(String::new(), |(_, n)| n.clone())
}

/// The document statuses (`DOCSTAT`), `(id, code)` in file order.
pub fn statuses(p: &ProjectData) -> Vec<(i64, String)> {
    dictionary(p, STATUSES, "doc_status_id", "doc_status_code")
}

/// The document categories (`DOCCATG`), `(id, name)` in file order.
pub fn categories(p: &ProjectData) -> Vec<(i64, String)> {
    dictionary(p, CATEGORIES, "doc_catg_id", "doc_catg_name")
}

/// The project's documents in file order.
pub fn list(p: &ProjectData) -> Vec<Doc> {
    let doc = &p.document;
    let statuses = statuses(p);
    let categories = categories(p);
    let rows = project_rows(p, TABLE);
    let ids: HashSet<i64> = rows
        .iter()
        .map(|&r| num(&records::get(doc, TABLE, r, "doc_id")))
        .collect();
    rows.into_iter()
        .map(|r| {
            let v = |k: &str| records::get(doc, TABLE, r, k);
            let parent = num(&v("parent_doc_id"));
            Doc {
                id: num(&v("doc_id")),
                parent: (parent != 0 && ids.contains(&parent)).then_some(parent),
                sequence: num(&v("doc_seq_num")),
                title: v("doc_name"),
                reference: v("doc_short_name"),
                deliverable: v("deliv_flag") == "Y",
                status: name_of(&statuses, &v("doc_status_id")),
                category: name_of(&categories, &v("doc_catg_id")),
                author: v("author_name"),
                version: v("version_name"),
                date: v("doc_date"),
                private_loc: v("private_loc"),
                public_loc: v("public_loc"),
                description: records::memo_to_text(&v("doc_content")),
            }
        })
        .collect()
}

/// The row of document `id` of the project.
fn row(p: &ProjectData, id: i64) -> Option<usize> {
    project_rows(p, TABLE)
        .into_iter()
        .find(|&r| num(&records::get(&p.document, TABLE, r, "doc_id")) == id)
}

/// A stored column of document `id`, or an empty string.
pub fn value(p: &ProjectData, id: i64, column: &str) -> String {
    row(p, id)
        .map(|r| records::get(&p.document, TABLE, r, column))
        .unwrap_or_default()
}

/// The document an operation names: its reference number, else its title
/// (each exact, then ignoring case), else its document id.
pub fn find(p: &ProjectData, name: &str) -> R<i64> {
    let all = list(p);
    let name = name.trim();
    if name.is_empty() {
        return Err("A document is required (its reference number, title or id).".into());
    }
    let pick = |hits: Vec<i64>| -> Option<R<i64>> {
        match hits.as_slice() {
            [] => None,
            [id] => Some(Ok(*id)),
            _ => Some(Err(format!(
                "Document {name:?} is not unique; name it by its document id."
            ))),
        }
    };
    let by = |f: &dyn Fn(&Doc) -> bool| all.iter().filter(|d| f(d)).map(|d| d.id).collect();
    let tries: [&dyn Fn(&Doc) -> bool; 4] = [
        &|d| d.reference == name,
        &|d| d.reference.eq_ignore_ascii_case(name),
        &|d| d.title == name,
        &|d| d.title.eq_ignore_ascii_case(name),
    ];
    for f in tries {
        if let Some(found) = pick(by(f)) {
            return found;
        }
    }
    if let Ok(id) = name.trim_start_matches('#').parse::<i64>() {
        if all.iter().any(|d| d.id == id) {
            return Ok(id);
        }
    }
    Err(format!(
        "No document {name:?} in the project. List them with the documents command."
    ))
}

/// The links of document `doc_id`, in file order.
pub fn links(p: &ProjectData, doc_id: i64) -> Vec<Link> {
    let doc = &p.document;
    project_rows(p, LINKS)
        .into_iter()
        .filter(|&r| num(&records::get(doc, LINKS, r, "doc_id")) == doc_id)
        .map(|r| Link {
            row: r,
            task_id: num(&records::get(doc, LINKS, r, "task_id")),
            wbs_id: num(&records::get(doc, LINKS, r, "wbs_id")),
            work_product: records::get(doc, LINKS, r, "wp_flag") == "Y",
        })
        .collect()
}

/// Add a document under `parent` (top level when `None`), after its last
/// sibling; returns its id. It is titled "(New Document)" as in the view.
pub fn add(p: &mut ProjectData, parent: Option<i64>) -> i64 {
    let pid = p.model.project.proj_id;
    let parent = parent.filter(|id| row(p, *id).is_some());
    let sequence = list(p)
        .iter()
        .filter(|d| d.parent == parent)
        .map(|d| d.sequence)
        .max()
        .unwrap_or(0)
        + 1;
    let doc = &mut p.document;
    let id = records::next_id(doc, TABLE, "doc_id");
    let guid = crate::new_guid(&format!("document-{pid}-{id}"));
    records::add_row(
        doc,
        TABLE,
        &[
            ("doc_id", id.to_string()),
            ("doc_seq_num", sequence.to_string()),
            ("deliv_flag", "N".into()),
            ("doc_name", "(New Document)".into()),
            (
                "parent_doc_id",
                parent.map_or(String::new(), |p| p.to_string()),
            ),
            ("proj_id", pid.to_string()),
            ("guid", guid),
        ],
    );
    id
}

/// A document and every document under it, the document first.
pub fn subtree(p: &ProjectData, id: i64) -> Vec<i64> {
    let all = list(p);
    let mut doomed = vec![id];
    let mut k = 0;
    while k < doomed.len() {
        let parent = doomed[k];
        let kids: Vec<i64> = all
            .iter()
            .filter(|d| d.parent == Some(parent) && !doomed.contains(&d.id))
            .map(|d| d.id)
            .collect();
        doomed.extend(kids);
        k += 1;
    }
    doomed
}

/// Delete a document with the documents under it and all their links;
/// returns how many documents went.
pub fn delete(p: &mut ProjectData, id: i64) -> usize {
    let doomed = subtree(p, id);
    let mut count = 0;
    for id in &doomed {
        if let Some(r) = row(p, *id) {
            records::remove_row(&mut p.document, TABLE, r);
            count += 1;
        }
        let gone: HashSet<usize> = links(p, *id).iter().map(|l| l.row).collect();
        if !gone.is_empty() {
            let t = p.document.table_mut(LINKS);
            let mut n = 0;
            t.rows.retain(|_| {
                let keep = !gone.contains(&n);
                n += 1;
                keep
            });
        }
    }
    count
}

/// Set a stored column of document `id` as given (the view's fields hold
/// stored values already); true when it changed.
pub fn set_column(p: &mut ProjectData, id: i64, column: &str, text: String) -> bool {
    let Some(r) = row(p, id) else {
        return false;
    };
    if records::get(&p.document, TABLE, r, column) == text {
        return false;
    }
    records::set(&mut p.document, TABLE, r, column, text);
    true
}

/// A document field an operation sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Title,
    Reference,
    Deliverable,
    Status,
    Category,
    Author,
    Version,
    Date,
    PrivateLocation,
    PublicLocation,
    Description,
}

impl Field {
    pub const NAMES: &'static str = "title, reference, deliverable, status, category, author, \
        version, date, private_location, public_location or description";

    pub fn parse(name: &str) -> R<Self> {
        Ok(match name.trim().to_ascii_lowercase().as_str() {
            "title" | "name" | "doc_name" => Field::Title,
            "reference" | "reference_no" | "doc_short_name" => Field::Reference,
            "deliverable" | "deliv_flag" => Field::Deliverable,
            "status" | "doc_status_id" => Field::Status,
            "category" | "doc_catg_id" => Field::Category,
            "author" | "author_name" => Field::Author,
            "version" | "version_name" => Field::Version,
            "date" | "doc_date" => Field::Date,
            "private_location" | "private_loc" => Field::PrivateLocation,
            "public_location" | "public_loc" | "location" => Field::PublicLocation,
            "description" | "doc_content" => Field::Description,
            other => {
                return Err(format!(
                    "Unknown document field {other:?}; use {}.",
                    Self::NAMES
                ))
            }
        })
    }

    /// The `DOCUMENT` column it writes.
    pub fn column(self) -> &'static str {
        match self {
            Field::Title => "doc_name",
            Field::Reference => "doc_short_name",
            Field::Deliverable => "deliv_flag",
            Field::Status => "doc_status_id",
            Field::Category => "doc_catg_id",
            Field::Author => "author_name",
            Field::Version => "version_name",
            Field::Date => "doc_date",
            Field::PrivateLocation => "private_loc",
            Field::PublicLocation => "public_loc",
            Field::Description => "doc_content",
        }
    }
}

fn dictionary_id(list: &[(i64, String)], name: &str, what: &str) -> R<String> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(String::new());
    }
    let hit = list
        .iter()
        .find(|(_, n)| n == name)
        .or_else(|| list.iter().find(|(_, n)| n.eq_ignore_ascii_case(name)));
    match hit {
        Some((id, _)) => Ok(id.to_string()),
        None => {
            let known: Vec<&str> = list.iter().map(|(_, n)| n.as_str()).collect();
            Err(if known.is_empty() {
                format!("No document {what} {name:?}; the project file defines none.")
            } else {
                format!(
                    "No document {what} {name:?}; use one of: {}.",
                    known.join(", ")
                )
            })
        }
    }
}

/// A date as `YYYY-MM-DD` or `YYYY-MM-DD HH:MM`, stored with the time.
fn date_text(text: &str) -> R<String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(String::new());
    }
    let (day, time) = t.split_once(' ').unwrap_or((t, "00:00"));
    let ok_day = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").is_ok();
    let ok_time = chrono::NaiveTime::parse_from_str(time.trim(), "%H:%M").is_ok();
    if ok_day && ok_time {
        Ok(format!("{day} {}", time.trim()))
    } else {
        Err(format!(
            "Date {text:?} is not YYYY-MM-DD or YYYY-MM-DD HH:MM."
        ))
    }
}

fn flag(text: &str) -> R<String> {
    match text.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" | "true" | "1" => Ok("Y".into()),
        "n" | "no" | "false" | "0" | "" => Ok("N".into()),
        other => Err(format!("Deliverable takes yes or no, not {other:?}.")),
    }
}

/// Set a field of document `id` from an operation's text: statuses and
/// categories by name, dates as `YYYY-MM-DD[ HH:MM]`, the description as
/// plain text. An empty text clears. Returns whether it changed.
pub fn set(p: &mut ProjectData, id: i64, field: Field, text: &str) -> R<bool> {
    if row(p, id).is_none() {
        return Err(format!("No document with id {id}."));
    }
    let stored = match field {
        Field::Status => dictionary_id(&statuses(p), text, "status")?,
        Field::Category => dictionary_id(&categories(p), text, "category")?,
        Field::Deliverable => flag(text)?,
        Field::Date => date_text(text)?,
        Field::Description => {
            if text.trim().is_empty() {
                String::new()
            } else {
                records::text_to_memo(text)
            }
        }
        Field::Title if text.trim().is_empty() => {
            return Err("A document needs a title.".into());
        }
        _ => records::clean(text),
    };
    Ok(set_column(p, id, field.column(), stored))
}

/// Link document `doc_id` to an activity (`task`) or, with no activity, to
/// a WBS element itself. False when that link exists already.
pub fn link(p: &mut ProjectData, doc_id: i64, task: Option<i64>, wbs: Option<i64>) -> bool {
    let pid = p.model.project.proj_id;
    let taken = links(p, doc_id).iter().any(|a| match task {
        Some(t) => a.task_id == t,
        None => a.task_id == 0 && Some(a.wbs_id) == wbs,
    });
    if taken || (task.is_none() && wbs.is_none()) {
        return false;
    }
    let doc = &mut p.document;
    let id = records::next_id(doc, LINKS, "taskdoc_id");
    let mut values = vec![
        ("taskdoc_id", id.to_string()),
        ("doc_id", doc_id.to_string()),
        ("proj_id", pid.to_string()),
        ("wp_flag", "N".into()),
    ];
    match (task, wbs) {
        (Some(t), _) => values.push(("task_id", t.to_string())),
        (None, Some(w)) => {
            values.push(("wbs_id", w.to_string()));
            values.push(("task_id", String::new()));
        }
        (None, None) => {}
    }
    records::add_row(doc, LINKS, &values);
    true
}

/// The link of document `doc_id` to activity `task`, or to WBS element
/// `wbs` itself.
pub fn link_row(
    p: &ProjectData,
    doc_id: i64,
    task: Option<i64>,
    wbs: Option<i64>,
) -> Option<usize> {
    links(p, doc_id)
        .into_iter()
        .find(|a| match task {
            Some(t) => a.task_id == t,
            None => a.task_id == 0 && Some(a.wbs_id) == wbs,
        })
        .map(|a| a.row)
}

/// Remove a link by its `TASKDOC` row.
pub fn remove_link(p: &mut ProjectData, row: usize) {
    records::remove_row(&mut p.document, LINKS, row);
}

/// Remove the link of `doc_id` to an activity or WBS element; false when
/// there is none.
pub fn unlink(p: &mut ProjectData, doc_id: i64, task: Option<i64>, wbs: Option<i64>) -> bool {
    match link_row(p, doc_id, task, wbs) {
        Some(row) => {
            remove_link(p, row);
            true
        }
        None => false,
    }
}

/// Mark a link (by `TASKDOC` row) as a work product or not.
pub fn set_work_product(p: &mut ProjectData, row: usize, on: bool) {
    let text = if on { "Y" } else { "N" };
    records::set(&mut p.document, LINKS, row, "wp_flag", text.into());
}

/// The file extension of `path`, lower case.
pub fn extension(path: &str) -> Option<String> {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let (_, ext) = name.rsplit_once('.')?;
    (!ext.is_empty()).then(|| ext.to_ascii_lowercase())
}

/// Checks a document location and returns the canonical path of the local
/// file it names, or why it is refused. Only an absolute local path to an
/// existing regular file passes: no relative paths, no network (UNC) or
/// device paths, no URIs, no control characters. On Windows the path must
/// start with a drive letter, and alternate data streams and trailing dots
/// or spaces are refused (Windows drops them, so the real name would differ
/// from the one checked). On other systems the path must start with `/`,
/// and a file that resolves under `/dev`, `/proc` or `/sys` is refused as a
/// device. `allowed` checks the file type, on the location as written and
/// again on the canonical path, so a link to another type is refused.
pub fn local_file(location: &str, allowed: &dyn Fn(&str) -> Result<(), String>) -> R<String> {
    if location.trim().is_empty() {
        return Err("the document has no location.".into());
    }
    if location.chars().any(char::is_control) {
        return Err("the location has control characters.".into());
    }
    if location.contains("://") {
        return Err("the location is a web or other address, not a local file.".into());
    }
    if location.starts_with("\\\\?\\") || location.starts_with("\\\\.\\") {
        return Err("device paths are not opened.".into());
    }
    if location.starts_with("\\\\") || location.starts_with("//") {
        return Err("network (UNC) paths are not opened.".into());
    }
    absolute_local(location)?;
    allowed(location)?;
    let canonical = std::fs::canonicalize(location)
        .map_err(|_| "the file does not exist or cannot be read.".to_string())?;
    let shown = local_canonical(&canonical)?;
    allowed(&shown).map_err(|_| {
        "the file resolves to a type that is not opened from a document.".to_string()
    })?;
    let meta = std::fs::metadata(&shown)
        .map_err(|_| "the file does not exist or cannot be read.".to_string())?;
    if !meta.is_file() {
        return Err("the location is not a regular file.".into());
    }
    Ok(shown)
}

/// The written location is an absolute path on a local drive letter, with
/// no alternate data stream and no trailing dot or space.
#[cfg(windows)]
fn absolute_local(location: &str) -> R<()> {
    let bytes = location.as_bytes();
    let on_drive = bytes.len() > 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/');
    if !on_drive {
        let scheme = location
            .split_once(':')
            .is_some_and(|(s, _)| s.len() > 1 && s.chars().all(|c| c.is_ascii_alphanumeric()));
        return Err(if scheme {
            "the location is a web or other address, not a local file.".into()
        } else {
            "the location is not an absolute path on a local drive (such as C:\\...).".into()
        });
    }
    if location[2..].contains(':') {
        return Err("the location names an alternate data stream.".into());
    }
    if location.ends_with(['.', ' ']) {
        return Err("the file name ends with a dot or a space.".into());
    }
    Ok(())
}

/// The written location is an absolute path (it starts with `/`).
#[cfg(not(windows))]
fn absolute_local(location: &str) -> R<()> {
    if !location.starts_with('/') {
        let scheme = location
            .split_once(':')
            .is_some_and(|(s, _)| s.len() > 1 && s.chars().all(|c| c.is_ascii_alphanumeric()));
        return Err(if scheme {
            "the location is a web or other address, not a local file.".into()
        } else {
            "the location is not an absolute path (such as /home/...).".into()
        });
    }
    Ok(())
}

/// The canonical path as shown: a local drive path without the `\\?\`
/// prefix.
#[cfg(windows)]
fn local_canonical(canonical: &std::path::Path) -> R<String> {
    let shown = canonical.to_string_lossy().into_owned();
    // The canonical form of a drive path carries the `\\?\` prefix; any
    // other form (a network share reached through a link) is refused.
    let shown = match shown.strip_prefix("\\\\?\\") {
        Some(rest) if rest.len() > 2 && rest.as_bytes()[1] == b':' => rest.to_string(),
        Some(_) => return Err("the file resolves to a network or device path.".into()),
        None => shown,
    };
    let b = shown.as_bytes();
    if !(b.len() > 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\')
        || shown[2..].contains(':')
    {
        return Err("the file resolves outside the local drives.".into());
    }
    Ok(shown)
}

/// The canonical path as shown: an absolute path that is not under the
/// device and system folders.
#[cfg(not(windows))]
fn local_canonical(canonical: &std::path::Path) -> R<String> {
    let shown = canonical.to_string_lossy().into_owned();
    if !shown.starts_with('/') || shown.starts_with("//") {
        return Err("the file resolves outside the local file system.".into());
    }
    if ["/dev", "/proc", "/sys"]
        .iter()
        .any(|dir| canonical.starts_with(dir))
    {
        return Err("the file resolves to a device or system path.".into());
    }
    Ok(shown)
}

/// The code path of WBS element `id` (codes from the top level down, below
/// the project node, joined by `.`), as the operations name it.
pub fn wbs_path(p: &ProjectData, id: i64) -> String {
    let project = super::wbs::project_node(p);
    let mut codes = vec![];
    let mut at = Some(id);
    let mut guard = 0;
    while let Some(w) = at.and_then(|id| p.wbs.iter().find(|w| w.id == id)) {
        if Some(w.id) == project || guard > p.wbs.len() {
            break;
        }
        codes.push(w.code.clone());
        at = w.parent;
        guard += 1;
    }
    codes.reverse();
    codes.join(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> ProjectData {
        let mut p = crate::starter();
        let pid = p.model.project.proj_id;
        p.document.table_mut(STATUSES).add(&[
            ("doc_status_id", "1".into()),
            ("doc_status_code", "Approved".into()),
        ]);
        let a = add(&mut p, None);
        set(&mut p, a, Field::Title, "Pump datasheet").unwrap();
        set(&mut p, a, Field::Reference, "DS-100").unwrap();
        let b = add(&mut p, Some(a));
        set(&mut p, b, Field::Title, "Pump datasheet rev B").unwrap();
        assert_eq!(pid, p.model.project.proj_id);
        p
    }

    #[test]
    fn add_set_find_and_delete_follow_the_outline() {
        let mut p = project();
        let all = list(&p);
        assert_eq!(all.len(), 2);
        let a = find(&p, "DS-100").unwrap();
        assert_eq!(find(&p, "ds-100").unwrap(), a);
        assert_eq!(find(&p, "pump datasheet").unwrap(), a);
        let b = find(&p, "Pump datasheet rev B").unwrap();
        assert_eq!(list(&p).iter().find(|d| d.id == b).unwrap().parent, Some(a));
        assert!(find(&p, "nothing").is_err());
        assert!(set(&mut p, a, Field::Status, "approved").unwrap());
        assert_eq!(list(&p)[0].status, "Approved");
        assert!(set(&mut p, a, Field::Status, "Rejected").is_err());
        assert!(set(&mut p, a, Field::Date, "2026-03-04").unwrap());
        assert_eq!(value(&p, a, "doc_date"), "2026-03-04 00:00");
        assert!(set(&mut p, a, Field::Date, "March 4").is_err());
        assert!(set(&mut p, a, Field::Deliverable, "yes").unwrap());
        assert!(set(&mut p, a, Field::Description, "Line one").unwrap());
        assert_eq!(list(&p)[0].description, "Line one");
        assert!(set(&mut p, a, Field::Title, " ").is_err());
        let task = p.model.activities[0].task_id;
        assert!(link(&mut p, b, Some(task), None));
        assert!(!link(&mut p, b, Some(task), None));
        assert_eq!(delete(&mut p, a), 2);
        assert!(list(&p).is_empty());
        assert!(links(&p, b).is_empty());
    }

    #[test]
    fn links_to_activities_and_wbs_elements() {
        let mut p = project();
        let a = find(&p, "DS-100").unwrap();
        let task = p.model.activities[0].task_id;
        let wbs = p.wbs[0].id;
        assert!(link(&mut p, a, Some(task), None));
        assert!(link(&mut p, a, None, Some(wbs)));
        assert!(!link(&mut p, a, None, None));
        let l = links(&p, a);
        assert_eq!(l.len(), 2);
        assert_eq!(l[1].task_id, 0);
        set_work_product(&mut p, l[0].row, true);
        assert!(links(&p, a)[0].work_product);
        assert!(unlink(&mut p, a, Some(task), None));
        assert!(!unlink(&mut p, a, Some(task), None));
        assert!(unlink(&mut p, a, None, Some(wbs)));
        assert!(links(&p, a).is_empty());
    }

    #[test]
    fn local_file_refuses_everything_but_an_existing_local_file() {
        let any = |_: &str| Ok(());
        for (location, why) in [
            ("", "no location"),
            ("notes.txt", "not an absolute path"),
            ("..\\notes.txt", "not an absolute path"),
            ("https://example.com/a.txt", "web or other address"),
            ("file:///C:/a.txt", "web or other address"),
            ("\\\\server\\share\\a.txt", "network"),
            ("//server/share/a.txt", "network"),
            ("\\\\?\\C:\\a.txt", "device"),
        ] {
            let e = local_file(location, &any).unwrap_err();
            assert!(e.contains(why), "{location}: {e}");
        }
        #[cfg(windows)]
        let platform = [
            ("C:\\a.txt:stream", "alternate data stream"),
            ("C:\\a.txt.", "dot or a space"),
        ];
        #[cfg(not(windows))]
        let platform = [
            ("C:\\a.txt", "not an absolute path"),
            ("/dev/null", "device"),
            ("/proc/self/status", "device"),
        ];
        for (location, why) in platform {
            let e = local_file(location, &any).unwrap_err();
            assert!(e.contains(why), "{location}: {e}");
        }
        let dir = tempfile_dir();
        let path = dir.join("a.txt");
        std::fs::write(&path, "x").unwrap();
        let shown = local_file(&path.to_string_lossy(), &any).unwrap();
        assert!(shown.ends_with("a.txt"));
        let none = |_: &str| Err("no".to_string());
        assert_eq!(
            local_file(&path.to_string_lossy(), &none).unwrap_err(),
            "no"
        );
        assert!(local_file(&dir.to_string_lossy(), &any).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// On Unix a symbolic link is followed and checked as a Windows link is:
    /// the type is checked again on the target, and the target must be a
    /// regular file outside the device folders.
    #[cfg(unix)]
    #[test]
    fn local_file_checks_where_a_unix_link_points() {
        let dir = std::env::temp_dir().join(format!("openxer-links-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let text = |path: &std::path::Path| path.to_string_lossy().into_owned();
        let only_txt = |path: &str| match extension(path).as_deref() {
            Some("txt") => Ok(()),
            _ => Err("type".to_string()),
        };
        let target = dir.join("real.txt");
        std::fs::write(&target, "x").unwrap();
        let link = dir.join("link.txt");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let shown = local_file(&text(&link), &only_txt).unwrap();
        assert!(shown.ends_with("real.txt"), "{shown}");
        let program = dir.join("run.sh");
        std::fs::write(&program, "x").unwrap();
        let to_program = dir.join("notes.txt");
        std::os::unix::fs::symlink(&program, &to_program).unwrap();
        let e = local_file(&text(&to_program), &only_txt).unwrap_err();
        assert!(e.contains("resolves to a type"), "{e}");
        let to_device = dir.join("null.txt");
        std::os::unix::fs::symlink("/dev/null", &to_device).unwrap();
        let e = local_file(&text(&to_device), &only_txt).unwrap_err();
        assert!(e.contains("device"), "{e}");
        let folder = dir.join("folder.txt");
        std::fs::create_dir_all(&folder).unwrap();
        let e = local_file(&text(&folder), &only_txt).unwrap_err();
        assert!(e.contains("regular file"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn tempfile_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("openxer-docs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
