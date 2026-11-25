//! Documents for agents: the `documents` command lists a project's work
//! products and documents with their links, and `read_document` returns the
//! text of the file a document record points at. Both are read-only.
//!
//! A file is only ever addressed through a document of the project, never by
//! a free path. Its location passes the same check as the WPs & Docs view's
//! Launch (`openxer_xer::edit::documents::local_file`): an existing regular
//! file named by an absolute local path (on a drive letter on Windows, from
//! `/` elsewhere); relative, network,
//! device and web locations are refused. Programs and scripts are refused by
//! type. Plain text formats are returned as text, PDF text is extracted page
//! by page, and Word (.docx) text is read from its XML. Large files are paged.
//!
//! A schedule received from someone else can name any local file in its
//! document rows, and an agent reads without a person clicking Launch. So a
//! file is read only from under the input file's own folder, or a folder
//! named in [`ROOTS_VAR`].
use crate::{AutomationError, Result};
use openxer_xer::edit::documents::{self as docs, Doc};
use openxer_xer::ProjectData;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The environment variable naming more folders (separated as in `PATH`:
/// `;` on Windows, `:` elsewhere) whose files `read_document` may read,
/// besides the input file's own folder.
pub const ROOTS_VAR: &str = "OPENXER_DOCUMENT_ROOTS";

/// The separator between the folders in [`ROOTS_VAR`], as
/// `std::env::split_paths` reads them.
const ROOTS_SEPARATOR: char = if cfg!(windows) { ';' } else { ':' };

/// A folder or file as its canonical path (on Windows a drive path without
/// the `\\?\` prefix), as `local_file` reports the document's file.
fn canonical(path: &Path) -> Option<PathBuf> {
    let full = std::fs::canonicalize(path).ok()?;
    let text = full.to_string_lossy();
    Some(match text.strip_prefix("\\\\?\\") {
        Some(rest) => PathBuf::from(rest),
        None => full,
    })
}

/// The folders documents may be read from: the input file's folder and
/// every folder in [`ROOTS_VAR`].
fn allowed_roots(input: &Path) -> Vec<PathBuf> {
    let folder = input
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut roots: Vec<PathBuf> = canonical(folder).into_iter().collect();
    if let Some(extra) = std::env::var_os(ROOTS_VAR) {
        roots.extend(
            std::env::split_paths(&extra)
                .filter(|p| !p.as_os_str().is_empty())
                .filter_map(|p| canonical(&p)),
        );
    }
    roots
}

/// Largest file read at all.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// Characters returned by default, and at most, per call.
pub const DEFAULT_LIMIT: usize = 20_000;
pub const MAX_LIMIT: usize = 200_000;
/// PDF pages returned by default, and at most, per call.
pub const DEFAULT_PAGES: usize = 10;
pub const MAX_PAGES: usize = 100;

/// Types read as text.
pub const TEXT_TYPES: [&str; 6] = ["txt", "md", "csv", "json", "xml", "log"];
/// Programs and scripts: never read.
pub const REFUSED_TYPES: [&str; 9] = ["exe", "bat", "cmd", "ps1", "js", "vbs", "msi", "com", "scr"];

fn err(code: &str, message: impl Into<String>) -> AutomationError {
    AutomationError::new(code, message)
}

/// A WBS element's path and name.
fn wbs_json(p: &ProjectData, id: i64) -> Value {
    let name = p
        .wbs
        .iter()
        .find(|w| w.id == id)
        .map_or("", |w| w.name.as_str());
    json!({"wbs":docs::wbs_path(p, id),"name":name})
}

fn activity_of(p: &ProjectData, task_id: i64) -> Option<(String, String)> {
    let a = p.model.activities.iter().find(|a| a.task_id == task_id)?;
    let name = p.details.get(&task_id).map_or("", |d| d.name.as_str());
    Some((a.task_code.clone(), name.to_string()))
}

fn document_json(p: &ProjectData, d: &Doc, all: &[Doc]) -> Value {
    let mut activities = vec![];
    let mut wbs = vec![];
    for l in docs::links(p, d.id) {
        if l.task_id != 0 {
            if let Some((code, name)) = activity_of(p, l.task_id) {
                activities.push(json!({"activity":code,"name":name,"work_product":l.work_product}));
            }
        } else if l.wbs_id != 0 {
            let mut w = wbs_json(p, l.wbs_id);
            w["work_product"] = json!(l.work_product);
            wbs.push(w);
        }
    }
    let parent = d
        .parent
        .and_then(|id| all.iter().find(|x| x.id == id))
        .map(|x| json!({"document_id":x.id,"title":x.title,"reference":x.reference}));
    json!({
        "document_id": d.id,
        "title": d.title,
        "reference": d.reference,
        "parent": parent,
        "deliverable": d.deliverable,
        "status": d.status,
        "category": d.category,
        "author": d.author,
        "version": d.version,
        "date": d.date,
        "private_location": d.private_loc,
        "public_location": d.public_loc,
        "description": d.description,
        "activities": activities,
        "wbs": wbs,
    })
}

/// The project's documents, or only those linked to `activity` (Activity
/// ID) or to the WBS element `wbs` (path) itself.
pub fn list(p: &ProjectData, activity: Option<&str>, wbs: Option<&str>) -> Result<Value> {
    let all = docs::list(p);
    let task = match activity {
        Some(code) => Some(
            p.model
                .activities
                .iter()
                .find(|a| a.task_code == code)
                .map(|a| a.task_id)
                .ok_or_else(|| err("not_found", format!("No activity {code:?}.")))?,
        ),
        None => None,
    };
    let element = match wbs {
        Some(path) => Some(
            openxer_xer::edit::wbs::find_path(p, path)
                .ok_or_else(|| err("not_found", format!("No WBS element {path:?}.")))?,
        ),
        None => None,
    };
    let shown: Vec<Value> = all
        .iter()
        .filter(|d| {
            let links = docs::links(p, d.id);
            task.is_none_or(|t| links.iter().any(|l| l.task_id == t))
                && element.is_none_or(|w| links.iter().any(|l| l.task_id == 0 && l.wbs_id == w))
        })
        .map(|d| document_json(p, d, &all))
        .collect();
    Ok(
        json!({"count":shown.len(),"total_documents":all.len(),"documents":shown,
        "statuses":docs::statuses(p).into_iter().map(|(_, n)| n).collect::<Vec<_>>(),
        "categories":docs::categories(p).into_iter().map(|(_, n)| n).collect::<Vec<_>>()}),
    )
}

/// Linked documents by activity, for the activity rows of `load`:
/// task id to `[{document_id, title, reference, work_product}]`.
pub fn by_activity(p: &ProjectData) -> BTreeMap<i64, Vec<Value>> {
    let mut out: BTreeMap<i64, Vec<Value>> = BTreeMap::new();
    for d in docs::list(p) {
        for l in docs::links(p, d.id).into_iter().filter(|l| l.task_id != 0) {
            out.entry(l.task_id).or_default().push(json!({
                "document_id": d.id, "title": d.title, "reference": d.reference,
                "work_product": l.work_product,
            }));
        }
    }
    out
}

/// How a file's text is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Text,
    Pdf,
    Docx,
}

fn kind_of(path: &str) -> std::result::Result<Kind, (String, String)> {
    let ext = docs::extension(path).unwrap_or_default();
    if REFUSED_TYPES.contains(&ext.as_str()) {
        return Err((
            "refused_type".into(),
            format!("Programs and scripts (.{ext}) are never read."),
        ));
    }
    if TEXT_TYPES.contains(&ext.as_str()) {
        return Ok(Kind::Text);
    }
    match ext.as_str() {
        "pdf" => Ok(Kind::Pdf),
        "docx" => Ok(Kind::Docx),
        "" => Err((
            "unsupported_type".into(),
            "The file has no type (extension); only txt, md, csv, json, xml, log, pdf and docx are read.".into(),
        )),
        other => Err((
            "unsupported_type".into(),
            format!(
                "Files of type .{other} are not read; only txt, md, csv, json, xml, log, pdf and docx are."
            ),
        )),
    }
}

/// Paging asked for: characters for text, pages for PDF.
#[derive(Clone, Copy, Debug, Default)]
pub struct Paging {
    pub offset: Option<usize>,
    pub limit: Option<usize>,
    pub page: Option<usize>,
    pub pages: Option<usize>,
}

/// The text of document `document`'s linked file (`location`: private or
/// public; by default the private location when it is set, else the public
/// one).
pub fn read(
    p: &ProjectData,
    input: &Path,
    document: &str,
    location: Option<&str>,
    paging: Paging,
) -> Result<Value> {
    let id = docs::find(p, document).map_err(|e| err("document_not_found", e))?;
    let all = docs::list(p);
    let d = all.iter().find(|d| d.id == id).expect("found above");
    let (which, stored) = match location.map(str::to_ascii_lowercase).as_deref() {
        Some("private") => ("private", d.private_loc.clone()),
        Some("public") => ("public", d.public_loc.clone()),
        None if !d.private_loc.trim().is_empty() => ("private", d.private_loc.clone()),
        None => ("public", d.public_loc.clone()),
        Some(other) => {
            return Err(err(
                "invalid_arguments",
                format!("location is private or public, not {other:?}"),
            ))
        }
    };
    if stored.trim().is_empty() {
        return Err(err(
            "no_location",
            format!("Document {:?} has no {which} location.", d.title),
        ));
    }
    let limit = paging.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(err(
            "invalid_arguments",
            format!("limit must be 1..{MAX_LIMIT} characters"),
        ));
    }
    // The type is checked before the location, so a program is refused as
    // a program whatever its location.
    let kind = kind_of(stored.trim()).map_err(|(c, m)| err(&c, m))?;
    let allowed = |path: &str| kind_of(path).map(|_| ()).map_err(|(_, m)| m);
    let path = docs::local_file(&stored, &allowed).map_err(|why| {
        err(
            "location_refused",
            format!("The {which} location {stored:?} is not read: {why}"),
        )
    })?;
    // The resolved file must be of the same kind (a link to another type).
    if kind_of(&path).ok() != Some(kind) {
        return Err(err(
            "location_refused",
            "The location resolves to a file of another type.",
        ));
    }
    let roots = allowed_roots(input);
    if !roots.iter().any(|root| Path::new(&path).starts_with(root)) {
        let folder = roots
            .first()
            .map_or_else(|| "?".into(), |r| r.display().to_string());
        return Err(err(
            "location_refused",
            format!(
                "The {which} location {path:?} is outside the input file's folder ({folder}); documents are read only from there. To allow another folder, set the environment variable {ROOTS_VAR} to it (several folders separated by {ROOTS_SEPARATOR}) for the openxer-cli or MCP server process."
            ),
        ));
    }
    let size = std::fs::metadata(&path)
        .map_err(|e| err("read_failed", e.to_string()))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(err(
            "file_too_large",
            format!("The file is {size} bytes; files over {MAX_FILE_BYTES} bytes are not read."),
        ));
    }
    let bytes = std::fs::read(&path).map_err(|e| err("read_failed", e.to_string()))?;
    let head = json!({"document_id":d.id,"title":d.title,"reference":d.reference});
    match kind {
        Kind::Text | Kind::Docx => {
            if paging.page.is_some() || paging.pages.is_some() {
                return Err(err(
                    "invalid_arguments",
                    "page and pages are for PDF files; use offset and limit",
                ));
            }
            let text = if kind == Kind::Text {
                decode(&bytes)
            } else {
                docx_text(&bytes)?
            };
            let chars: Vec<char> = text.chars().collect();
            let offset = paging.offset.unwrap_or(0);
            if offset > chars.len() {
                return Err(err(
                    "invalid_arguments",
                    format!(
                        "offset {offset} is past the end ({} characters)",
                        chars.len()
                    ),
                ));
            }
            let end = offset.saturating_add(limit).min(chars.len());
            let slice: String = chars[offset..end].iter().collect();
            Ok(json!({"document":head,"location":which,"path":path,
                "format":if kind == Kind::Text { "text" } else { "docx" },
                "bytes":size,"total_characters":chars.len(),"offset":offset,
                "next_offset":(end < chars.len()).then_some(end),"text":slice}))
        }
        Kind::Pdf => {
            if paging.offset.is_some() {
                return Err(err(
                    "invalid_arguments",
                    "offset is for text files; use page and pages for PDF",
                ));
            }
            pdf_pages(&bytes, paging, limit).map(|mut v| {
                v["document"] = head;
                v["location"] = json!(which);
                v["path"] = json!(path);
                v["bytes"] = json!(size);
                v
            })
        }
    }
}

/// Text from UTF-8 (with or without a byte order mark) or UTF-16 with a
/// byte order mark; anything else as Windows-1252.
fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    let utf16 = |rest: &[u8], le: bool| {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| {
                if le {
                    u16::from_le_bytes(c)
                } else {
                    u16::from_be_bytes(c)
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(rest, true);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(rest, false);
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned(),
    }
}

/// The body text of a Word document: `word/document.xml`, one line per
/// paragraph, tabs and breaks kept.
fn docx_text(bytes: &[u8]) -> Result<String> {
    use quick_xml::events::Event;
    let bad = |e: String| err("read_failed", format!("The .docx cannot be read: {e}"));
    let mut zip =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| bad(e.to_string()))?;
    let mut entry = zip
        .by_name("word/document.xml")
        .map_err(|e| bad(e.to_string()))?;
    // The XML is bounded too: a small zip can inflate to a huge part.
    let mut xml = String::new();
    entry
        .by_ref()
        .take(MAX_FILE_BYTES * 4)
        .read_to_string(&mut xml)
        .map_err(|e| bad(e.to_string()))?;
    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut out = String::new();
    let mut in_text = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.local_name().into_inner() == "t" => in_text = true,
            Ok(Event::End(e)) => match e.local_name().into_inner() {
                "t" => in_text = false,
                "p" => out.push('\n'),
                _ => {}
            },
            Ok(Event::Empty(e)) => match e.local_name().into_inner() {
                "tab" => out.push('\t'),
                "br" | "cr" => out.push('\n'),
                "p" => out.push('\n'),
                _ => {}
            },
            Ok(Event::Text(t)) if in_text => out.push_str(&t.xml10_content()),
            Ok(Event::GeneralRef(r)) if in_text => {
                if let Ok(Some(c)) = r.resolve_char_ref() {
                    out.push(c);
                } else {
                    out.push_str(match r.xml10_content().as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => "",
                    });
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(bad(e.to_string())),
            _ => {}
        }
    }
    Ok(out)
}

/// Pages `page..page + pages` (1-based) of a PDF, each extracted on its
/// own, stopping early at `limit` characters.
fn pdf_pages(bytes: &[u8], paging: Paging, limit: usize) -> Result<Value> {
    let first = paging.page.unwrap_or(1);
    let count = paging.pages.unwrap_or(DEFAULT_PAGES);
    if first == 0 || !(1..=MAX_PAGES).contains(&count) {
        return Err(err(
            "invalid_arguments",
            format!("page starts at 1 and pages must be 1..{MAX_PAGES}"),
        ));
    }
    // The extractor is not hardened against every malformed file; a panic
    // in it is a read failure, not a crash of the server.
    let bytes = bytes.to_vec();
    let run = std::panic::catch_unwind(move || -> std::result::Result<Value, String> {
        let doc = pdf_extract::Document::load_mem(&bytes).map_err(|e| e.to_string())?;
        if doc.is_encrypted() {
            return Err("the PDF is encrypted".into());
        }
        let numbers: Vec<u32> = doc.get_pages().keys().copied().collect();
        let total = numbers.len();
        let mut pages = vec![];
        let mut used = 0;
        let mut next = None;
        let mut truncated = false;
        for (k, n) in numbers.iter().enumerate().skip(first - 1).take(count) {
            if used >= limit {
                next = Some(k + 1);
                break;
            }
            let mut text = String::new();
            {
                let mut out = pdf_extract::PlainTextOutput::new(&mut text);
                pdf_extract::output_doc_page(&doc, &mut out, *n).map_err(|e| e.to_string())?;
            }
            let room = limit - used;
            let whole = text.chars().count();
            let text: String = text.chars().take(room).collect();
            used += text.chars().count();
            if whole > room {
                // The rest of this page is not returned: say so, and
                // continue at this page (a larger limit reads it whole).
                pages.push(json!({"page":k + 1,"text":text,"truncated":true,
                    "page_characters":whole}));
                truncated = true;
                next = Some(k + 1);
                break;
            }
            pages.push(json!({"page":k + 1,"text":text}));
        }
        let last = first - 1 + pages.len();
        if next.is_none() && last < total {
            next = Some(last + 1);
        }
        Ok(
            json!({"format":"pdf","page_count":total,"page":first,"pages":pages,
            "next_page":next,"characters":used,"truncated":truncated}),
        )
    });
    match run {
        Ok(Ok(v)) => {
            if first > v["page_count"].as_u64().unwrap_or(0) as usize && first != 1 {
                return Err(err(
                    "invalid_arguments",
                    format!("page {first} is past the last page ({})", v["page_count"]),
                ));
            }
            Ok(v)
        }
        Ok(Err(e)) => Err(err(
            "read_failed",
            format!("The PDF text cannot be extracted: {e}"),
        )),
        Err(_) => Err(err(
            "read_failed",
            "The PDF text cannot be extracted (the file is malformed or uses a feature the reader does not handle).",
        )),
    }
}
