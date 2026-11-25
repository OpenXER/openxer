//! WPs & Docs view (Project > WPs & Docs): the project's work products and
//! documents (DOCUMENT) as an outline by parent document, with status,
//! category, author, version and locations, the details tabs, and their
//! assignments to activities and WBS elements (TASKDOC rows with a task or
//! only a WBS element). Add, Delete and every edit go through the detail
//! records, so undo and export carry them.
use super::grid::Row;
use super::record_view::{self, ListSpec, PickTarget, RowKey};
use super::sheet;
use super::*;
use crate::context_menu::{Entry, Spec};
use crate::records::{self, num};
use crate::schedule_dialogs::Canvas;
use openxer_xer::edit::documents as shared;
use openxer_xer::Document;
use std::collections::{HashMap, HashSet};

pub const TITLES: [&str; 10] = [
    "Title",
    "Reference No.",
    "Deliverable",
    "Status",
    "Document Category",
    "Author",
    "Version",
    "Date",
    "Private Location",
    "Public Location",
];
pub const WIDTHS: [f32; 10] = [250., 110., 70., 100., 130., 120., 70., 95., 160., 160.];
pub const TABS: [&str; 5] = [
    "General",
    "Description",
    "Files",
    "Assignments",
    "User Defined Fields",
];

/// One work product or document.
#[derive(Clone, Debug, PartialEq)]
pub struct Doc {
    pub id: i64,
    pub parent: Option<i64>,
    pub sequence: i64,
    pub title: String,
    pub reference: String,
    pub deliverable: bool,
    pub status: String,
    pub category: String,
    pub author: String,
    pub version: String,
    pub date: String,
    pub private_loc: String,
    pub public_loc: String,
}

/// The open project's documents in file order (the shared edit layer's
/// list, `openxer_xer::edit::documents`).
pub fn documents(p: &ProjectData) -> Vec<Doc> {
    shared::list(p)
        .into_iter()
        .map(|d| Doc {
            id: d.id,
            parent: d.parent,
            sequence: d.sequence,
            title: d.title,
            reference: d.reference,
            deliverable: d.deliverable,
            status: d.status,
            category: d.category,
            author: d.author,
            version: d.version,
            date: d.date,
            private_loc: d.private_loc,
            public_loc: d.public_loc,
        })
        .collect()
}

fn doc_order(a: &Doc, b: &Doc) -> std::cmp::Ordering {
    a.sequence
        .cmp(&b.sequence)
        .then(a.title.cmp(&b.title))
        .then(a.id.cmp(&b.id))
}

/// Outline order: children under their parent, siblings by sequence then
/// title. Returns (id, depth). The walk is iterative and the parent-to-children
/// map is built once, so a very deep chain or a very large table costs
/// n log n. A document whose parent is missing is shown at the top level, and
/// documents caught in a parent loop are shown at the top level as well (the
/// rest of the loop sits under the first member met).
pub fn outline(list: &[Doc]) -> Vec<(i64, usize)> {
    let ids: HashSet<i64> = list.iter().map(|d| d.id).collect();
    let mut children: HashMap<Option<i64>, Vec<&Doc>> = HashMap::new();
    for d in list {
        let parent = d.parent.filter(|p| *p != d.id && ids.contains(p));
        children.entry(parent).or_default().push(d);
    }
    for kids in children.values_mut() {
        kids.sort_by(|a, b| doc_order(a, b));
    }
    // Depth-first with an explicit stack of (parent, depth, next child).
    fn walk(
        root: Option<i64>,
        depth: usize,
        children: &HashMap<Option<i64>, Vec<&Doc>>,
        seen: &mut HashSet<i64>,
        out: &mut Vec<(i64, usize)>,
    ) {
        let mut stack: Vec<(Option<i64>, usize, usize)> = vec![(root, depth, 0)];
        while let Some((parent, depth, next)) = stack.pop() {
            let Some(d) = children.get(&parent).and_then(|k| k.get(next)) else {
                continue;
            };
            stack.push((parent, depth, next + 1));
            if !seen.insert(d.id) {
                continue;
            }
            out.push((d.id, depth));
            stack.push((Some(d.id), depth + 1, 0));
        }
    }
    let mut out = Vec::with_capacity(list.len());
    let mut seen: HashSet<i64> = HashSet::with_capacity(list.len());
    walk(None, 0, &children, &mut seen, &mut out);
    if seen.len() < ids.len() {
        let mut ordered: Vec<&Doc> = list.iter().collect();
        ordered.sort_by(|a, b| doc_order(a, b));
        for d in ordered {
            if seen.insert(d.id) {
                out.push((d.id, 0));
                walk(Some(d.id), 1, &children, &mut seen, &mut out);
            }
        }
    }
    out
}

/// One assignment of a document (a TASKDOC row).
#[derive(Clone, Debug, PartialEq)]
pub struct Assignment {
    pub row: usize,
    pub task_id: i64,
    pub wbs_id: i64,
    pub work_product: bool,
}

pub fn assignments(doc: &Document, pid: i64, doc_id: i64) -> Vec<Assignment> {
    record_view::project_rows(doc, "TASKDOC", pid)
        .into_iter()
        .filter(|&r| num(&records::get(doc, "TASKDOC", r, "doc_id")) == doc_id)
        .map(|r| Assignment {
            row: r,
            task_id: num(&records::get(doc, "TASKDOC", r, "task_id")),
            wbs_id: num(&records::get(doc, "TASKDOC", r, "wbs_id")),
            work_product: records::get(doc, "TASKDOC", r, "wp_flag") == "Y",
        })
        .collect()
}

/// A WBS element's own document assignments (TASKDOC rows with no
/// activity), sorted by title, with the document id, title and status code.
pub fn wbs_assignments(
    doc: &Document,
    pid: i64,
    wbs_id: i64,
) -> Vec<(Assignment, i64, String, String)> {
    let mut out: Vec<(Assignment, i64, String, String)> =
        record_view::project_rows(doc, "TASKDOC", pid)
            .into_iter()
            .filter(|&r| {
                num(&records::get(doc, "TASKDOC", r, "task_id")) == 0
                    && num(&records::get(doc, "TASKDOC", r, "wbs_id")) == wbs_id
            })
            .map(|r| {
                let doc_id = num(&records::get(doc, "TASKDOC", r, "doc_id"));
                let status = num(&records::lookup(
                    doc,
                    "DOCUMENT",
                    "doc_id",
                    doc_id,
                    "doc_status_id",
                ));
                (
                    Assignment {
                        row: r,
                        task_id: 0,
                        wbs_id,
                        work_product: records::get(doc, "TASKDOC", r, "wp_flag") == "Y",
                    },
                    doc_id,
                    records::lookup(doc, "DOCUMENT", "doc_id", doc_id, "doc_name"),
                    records::lookup(doc, "DOCSTAT", "doc_status_id", status, "doc_status_code"),
                )
            })
            .collect();
    out.sort_by(|a, b| {
        a.2.to_lowercase()
            .cmp(&b.2.to_lowercase())
            .then(a.0.row.cmp(&b.0.row))
    });
    out
}

/// The project's documents not yet assigned to the WBS element itself, as
/// (id, title, reference number), in outline order.
pub fn unassigned_for_wbs(doc: &Document, pid: i64, wbs_id: i64) -> Vec<(i64, String, String)> {
    let taken: Vec<i64> = wbs_assignments(doc, pid, wbs_id)
        .iter()
        .map(|a| a.1)
        .collect();
    record_view::project_rows(doc, "DOCUMENT", pid)
        .into_iter()
        .map(|r| {
            (
                num(&records::get(doc, "DOCUMENT", r, "doc_id")),
                records::get(doc, "DOCUMENT", r, "doc_name"),
                records::get(doc, "DOCUMENT", r, "doc_short_name"),
            )
        })
        .filter(|(id, _, _)| !taken.contains(id))
        .collect()
}

/// Add a document after the selected one, at its level; returns its id.
pub fn add_document(p: &mut ProjectData, after: Option<i64>) -> i64 {
    let parent = after
        .and_then(|id| documents(p).into_iter().find(|d| d.id == id))
        .and_then(|d| d.parent);
    shared::add(p, parent)
}

/// Delete a document with the documents under it and their assignments;
/// returns how many documents went.
pub fn delete_document(p: &mut ProjectData, id: i64) -> usize {
    shared::delete(p, id)
}

/// Assign a document to an activity or (with no activity) a WBS element.
pub fn assign(p: &mut ProjectData, doc_id: i64, task: Option<i64>, wbs: Option<i64>) -> bool {
    shared::link(p, doc_id, task, wbs)
}

fn set_document(p: &mut ProjectData, id: i64, column: &str, text: String) -> bool {
    shared::set_column(p, id, column, text)
}

/// File types a document location may open: documents, drawings, pictures
/// and mail. Archives are left out (they can hide programs), and so is
/// everything else.
const LAUNCH_TYPES: [&str; 25] = [
    "pdf", "doc", "docx", "xls", "xlsx", "xlsm", "csv", "txt", "rtf", "ppt", "pptx", "png", "jpg",
    "jpeg", "gif", "bmp", "tif", "tiff", "dwg", "dxf", "msg", "eml", "html", "htm", "xml",
];

/// The file extension of `path`, lower case, if it is a launchable type.
fn launch_type(path: &str) -> Option<String> {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let extension = name.rsplit_once('.')?.1.to_ascii_lowercase();
    LAUNCH_TYPES
        .contains(&extension.as_str())
        .then_some(extension)
}

/// Checks a document location and returns the canonical path to open, or
/// why it is refused: the shared check (`documents::local_file`: an
/// existing regular file named by an absolute local path, no network,
/// device or web locations) with the launchable document types, checked on
/// the location and again on the canonical path, so a link to a program is
/// refused.
pub(crate) fn launch_target(location: &str) -> Result<String, String> {
    let allowed = |path: &str| {
        match launch_type(path) {
        Some(_) => Ok(()),
        None => Err("this file type is not opened from a document (documents, drawings, pictures and mail only).".to_string()),
    }
    };
    shared::local_file(location, &allowed)
}

impl App {
    pub(super) fn documents_view(&mut self, host: &mut egui::Ui) {
        let list = documents(&self.project);
        let state = &self.views.records.documents;
        let collapsed = state.collapsed.clone();
        let parents: BTreeSet<i64> = list.iter().filter_map(|d| d.parent).collect();
        let origin = self.project.model.origin_serial;
        let mut rows = vec![];
        let mut keys = vec![];
        let by_id: HashMap<i64, &Doc> = list.iter().map(|d| (d.id, d)).collect();
        let mut hidden_below: Option<usize> = None;
        for (id, depth) in outline(&list) {
            if hidden_below.is_some_and(|d| depth > d) {
                continue;
            }
            hidden_below = None;
            let closed = collapsed.contains(&id);
            if closed {
                hidden_below = Some(depth);
            }
            let d = by_id[&id];
            keys.push(RowKey::Record(id));
            rows.push(Row {
                depth,
                expander: parents.contains(&id).then_some(!closed),
                icon: Some(chrome::Icon::WorkProducts),
                cells: vec![
                    d.title.clone(),
                    d.reference.clone(),
                    if d.deliverable { "Yes" } else { "No" }.into(),
                    d.status.clone(),
                    d.category.clone(),
                    d.author.clone(),
                    d.version.clone(),
                    record_view::show_date(origin, &d.date),
                    d.private_loc.clone(),
                    d.public_loc.clone(),
                ],
                ..Default::default()
            });
        }
        record_view::keep_selection(&mut self.views.records.documents, &keys);
        let selected = self.views.records.documents.selected;
        let spec = ListSpec {
            id: "documents",
            display: "All Documents".into(),
            titles: &TITLES,
            right: &[],
            tabs: &TABS,
            commands: vec![
                ("Add", chrome::Icon::Add, true),
                ("Delete", chrome::Icon::Delete, selected.is_some()),
            ],
            empty: "This project has no work products or documents. Add one with the Add button.",
            details: "WP && Doc Details",
            customize: "Document",
            extras: self.document_menu_extras(selected, &list),
        };
        let chosen = selected.and_then(|id| list.iter().find(|d| d.id == id).cloned());
        let command = self.record_list_view(
            host,
            &spec,
            |v| &mut v.records.documents,
            rows,
            keys,
            Box::new(move |app, c, size, tab| app.document_details(c, size, tab, chosen.as_ref())),
            &mut |_, _| {},
        );
        match command {
            Some("Add") => self.document_command_add(),
            Some("Delete") => self.document_command_delete(),
            Some("assign_activity") => {
                if let Some(id) = selected {
                    self.open_picker(PickTarget::DocumentActivity(id));
                }
            }
            Some("assign_wbs") => {
                if let Some(id) = selected {
                    self.open_picker(PickTarget::DocumentWbs(id));
                }
            }
            Some("remove") => self.document_remove_assignment(),
            Some("launch_private") => self.document_launch(selected, false),
            Some("launch_public") => self.document_launch(selected, true),
            _ => {}
        }
    }

    /// The right-click items that are the view's own: assign to an activity
    /// or a WBS element, remove the assignment selected on the Assignments
    /// tab, and launch the document's file.
    fn document_menu_extras(&self, selected: Option<i64>, list: &[Doc]) -> Vec<Entry> {
        let pid = self.project.model.project.proj_id;
        let chosen = selected.and_then(|id| list.iter().find(|d| d.id == id));
        let assigned = selected.map_or(0, |id| assignments(&self.project.document, pid, id).len());
        let removable = self
            .views
            .records
            .documents
            .sub_row
            .is_some_and(|r| r < assigned);
        let has = |loc: fn(&Doc) -> &str| chosen.is_some_and(|d| !loc(d).trim().is_empty());
        vec![
            Entry::Item(Spec::new("assign_activity", "Assign Activity").enabled(chosen.is_some())),
            Entry::Item(Spec::new("assign_wbs", "Assign WBS").enabled(chosen.is_some())),
            Entry::Item(Spec::new("remove", "Remove").enabled(removable)),
            Entry::Separator,
            Entry::Submenu(
                Spec::new("launch", "Launch").enabled(chosen.is_some()),
                vec![
                    Entry::Item(
                        Spec::new("launch_private", "Private Document")
                            .enabled(has(|d| d.private_loc.as_str())),
                    ),
                    Entry::Item(
                        Spec::new("launch_public", "Public Document")
                            .enabled(has(|d| d.public_loc.as_str())),
                    ),
                ],
            ),
        ]
    }

    /// Removes the assignment selected on the Assignments tab.
    fn document_remove_assignment(&mut self) {
        let Some(id) = self.views.records.documents.selected else {
            return;
        };
        let pid = self.project.model.project.proj_id;
        let list = assignments(&self.project.document, pid, id);
        let Some(a) = self
            .views
            .records
            .documents
            .sub_row
            .and_then(|r| list.get(r))
        else {
            return;
        };
        shared::remove_link(&mut self.project, a.row);
        self.views.records.documents.sub_row = None;
        self.touch_records();
        self.status = "Assignment removed".into();
    }

    /// Launch > Private / Public Document. The location comes from the
    /// project file, which may be imported and so is not trusted: it is
    /// checked by [`launch_target`] and then confirmed with the full path
    /// before anything is opened.
    fn document_launch(&mut self, id: Option<i64>, public: bool) {
        let doc = &self.project.document;
        let column = if public { "public_loc" } else { "private_loc" };
        let location = id
            .map(|id| record_view::value(doc, "DOCUMENT", "doc_id", id, column))
            .unwrap_or_default();
        match launch_target(&location) {
            Ok(path) => self.views.records.launch = Some(path),
            Err(why) => self.status = format!("Not opened: {why}"),
        }
    }

    /// The confirmation before a document is opened: the whole canonical
    /// path with Open and Cancel. Cancel, Escape and closing do nothing;
    /// Enter does not confirm.
    pub(crate) fn launch_dialog(&mut self, ctx: &egui::Context) {
        let Some(path) = self.views.records.launch.clone() else {
            return;
        };
        let mut rects = vec![];
        let clicked = crate::kit_window::message_box(
            ctx,
            "launch",
            "Open Document",
            crate::kit_window::MessageIcon::Question,
            &format!(
                "Open this file with its default program?\n\n{path}\n\nThe location is stored in the project file."
            ),
            &["Open", "Cancel"],
            // Starting another program takes a click: Enter is idle.
            crate::kit_window::Question::OpenDocument.default_in(&["Open", "Cancel"]),
            &mut rects,
        );
        self.record_dialog_actions(rects);
        let cancel = clicked == Some(1) || ctx.input(|i| i.key_pressed(Key::Escape));
        if clicked == Some(0) {
            self.views.records.launch = None;
            // The file may have changed while the box was up: check again,
            // and open only the path that was shown.
            self.status = match launch_target(&path) {
                Ok(again) if again == path => {
                    match crate::util::open_in_default_app(std::path::Path::new(&again)) {
                        Ok(()) => format!("Opened {again}"),
                        Err(e) => e,
                    }
                }
                Ok(_) => "Not opened: the file is no longer the one that was shown.".into(),
                Err(why) => format!("Not opened: {why}"),
            };
        } else if cancel {
            self.views.records.launch = None;
        }
    }

    pub(super) fn document_command_add(&mut self) {
        let after = self.views.records.documents.selected;
        let id = add_document(&mut self.project, after);
        self.views.records.documents.selected = Some(id);
        self.touch_records();
        self.status = "Work product or document added".into();
    }

    pub(super) fn document_command_delete(&mut self) {
        self.record_delete(MainView::Documents, false);
    }

    fn set_document_field(&mut self, id: i64, column: &str, text: String) {
        if set_document(&mut self.project, id, column, text) {
            self.touch_records();
        }
    }

    fn document_details(&mut self, c: &mut Canvas, size: Vec2, tab: usize, d: Option<&Doc>) {
        let (w, h) = (size.x, size.y);
        let Some(d) = d else {
            c.label(12., 12., 13., "Select a work product or document.", true);
            return;
        };
        let id = d.id;
        let origin = self.project.model.origin_serial;
        let half = ((w - 60.) / 2.).clamp(160., 420.);
        let x2 = 20. + half + 20.;
        let doc = &self.project.document;
        let v = |k: &str| record_view::value(doc, "DOCUMENT", "doc_id", id, k);
        match TABS[tab.min(TABS.len() - 1)] {
            "General" => {
                let statuses =
                    record_view::dictionary(doc, "DOCSTAT", "doc_status_id", "doc_status_code");
                let categories =
                    record_view::dictionary(doc, "DOCCATG", "doc_catg_id", "doc_catg_name");
                let (status, category, date) =
                    (v("doc_status_id"), v("doc_catg_id"), v("doc_date"));
                let mut deliverable = d.deliverable;
                if let Some(t) = sheet::text_field(
                    c,
                    "document_title",
                    20.,
                    25.,
                    w - 40.,
                    "Title",
                    &d.title,
                    true,
                ) {
                    self.set_document_field(id, "doc_name", records::clean(&t));
                }
                for (x, y, column, caption, value) in [
                    (20., 70., "doc_short_name", "Reference No.", &d.reference),
                    (x2, 70., "version_name", "Version", &d.version),
                    (20., 115., "author_name", "Author", &d.author),
                ] {
                    if let Some(t) = sheet::text_field(
                        c,
                        &format!("document_{column}"),
                        x,
                        y,
                        half,
                        caption,
                        value,
                        true,
                    ) {
                        self.set_document_field(id, column, records::clean(&t));
                    }
                }
                if let Some(t) = record_view::date_text(
                    c,
                    "document_date",
                    x2,
                    115.,
                    half.min(217.),
                    "Revision Date",
                    origin,
                    &date,
                    true,
                ) {
                    self.set_document_field(id, "doc_date", t);
                }
                if let Some(code) = record_view::dictionary_field(
                    c,
                    "document_status",
                    20.,
                    160.,
                    half,
                    "Status",
                    &statuses,
                    &status,
                    true,
                ) {
                    self.set_document_field(id, "doc_status_id", code);
                }
                if let Some(code) = record_view::dictionary_field(
                    c,
                    "document_category",
                    x2,
                    160.,
                    half,
                    "Document Category",
                    &categories,
                    &category,
                    true,
                ) {
                    self.set_document_field(id, "doc_catg_id", code);
                }
                if c.checkbox(
                    "document_deliverable",
                    20.,
                    195.,
                    150.,
                    21.,
                    "Deliverable",
                    &mut deliverable,
                    true,
                )
                .clicked()
                {
                    self.set_document_field(
                        id,
                        "deliv_flag",
                        if deliverable { "Y" } else { "N" }.into(),
                    );
                }
            }
            "Description" => {
                let stored = v("doc_content");
                if let Some(t) = record_view::memo(
                    c,
                    "document_description",
                    6.,
                    6.,
                    w - 12.,
                    h - 12.,
                    &stored,
                    true,
                ) {
                    self.set_document_field(id, "doc_content", t);
                }
            }
            "Files" => {
                for (y, column, caption, value, public) in [
                    (
                        25.,
                        "private_loc",
                        "Private Location",
                        &d.private_loc,
                        false,
                    ),
                    (70., "public_loc", "Public Location", &d.public_loc, true),
                ] {
                    if let Some(t) = sheet::text_field(
                        c,
                        &format!("document_{column}"),
                        20.,
                        y,
                        w - 140.,
                        caption,
                        value,
                        true,
                    ) {
                        self.set_document_field(id, column, records::clean(&t));
                    }
                    // Opens the file through the same checks and confirmation
                    // as Launch on the right-click menu.
                    if c.button(
                        &format!("document_{column}_launch"),
                        w - 110.,
                        y,
                        90.,
                        21.,
                        "Launch...",
                        false,
                        !value.trim().is_empty(),
                    ) {
                        self.document_launch(Some(id), public);
                    }
                }
            }
            "Assignments" => self.document_assignments(c, w, h, id),
            _ => {
                let rows = record_view::udf_rows(doc, "DOCUMENT", id);
                record_view::udf_tab(c, "document_udf", w, h, &rows);
            }
        }
    }

    fn document_assignments(&mut self, c: &mut Canvas, w: f32, h: f32, id: i64) {
        let pid = self.project.model.project.proj_id;
        let list = assignments(&self.project.document, pid, id);
        let rows: Vec<Vec<String>> = list
            .iter()
            .map(|a| {
                let (code, name) = if a.task_id != 0 {
                    record_view::activity_label(&self.project, a.task_id)
                } else {
                    let name = self
                        .project
                        .wbs
                        .iter()
                        .find(|w| w.id == a.wbs_id)
                        .map_or(String::new(), |w| w.name.clone());
                    (record_view::wbs_path(&self.project, a.wbs_id), name)
                };
                let wbs = if a.task_id != 0 {
                    String::new()
                } else {
                    "WBS".into()
                };
                vec![
                    code,
                    name,
                    wbs,
                    if a.work_product { "Yes" } else { "No" }.into(),
                ]
            })
            .collect();
        let cols = [
            ("Activity ID", 120., false),
            ("Activity Name", 260., false),
            ("Assigned To", 90., false),
            ("Work Product", 90., false),
        ];
        let sub = self
            .views
            .records
            .documents
            .sub_row
            .filter(|&r| r < rows.len());
        if let Some(r) = sheet::table(
            c,
            "document_assignments",
            6.,
            6.,
            w - 12.,
            h - 40.,
            &cols,
            &rows,
            sub,
        ) {
            self.views.records.documents.sub_row = Some(r);
        }
        match sheet::buttons(
            c,
            "document_assign_buttons",
            6.,
            h - 28.,
            &[
                ("Assign Activity", true),
                ("Assign WBS", true),
                ("Remove", sub.is_some()),
                ("Work Product", sub.is_some()),
            ],
        ) {
            Some(0) => self.open_picker(PickTarget::DocumentActivity(id)),
            Some(1) => self.open_picker(PickTarget::DocumentWbs(id)),
            Some(2) => {
                if let Some(a) = sub.and_then(|r| list.get(r)) {
                    shared::remove_link(&mut self.project, a.row);
                    self.views.records.documents.sub_row = None;
                    self.touch_records();
                }
            }
            Some(3) => {
                if let Some(a) = sub.and_then(|r| list.get(r)) {
                    shared::set_work_product(&mut self.project, a.row, !a.work_product);
                    self.touch_records();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn doc(id: i64, parent: Option<i64>) -> Doc {
        Doc {
            id,
            parent,
            sequence: id,
            title: format!("Document {id}"),
            reference: String::new(),
            deliverable: false,
            status: String::new(),
            category: String::new(),
            author: String::new(),
            version: String::new(),
            date: String::new(),
            private_loc: String::new(),
            public_loc: String::new(),
        }
    }

    #[test]
    fn outline_orders_siblings_and_shows_missing_parents_at_the_top() {
        let list = vec![
            doc(1, None),
            doc(2, Some(1)),
            doc(3, Some(1)),
            doc(4, Some(2)),
            doc(5, Some(99)),
        ];
        assert_eq!(
            outline(&list),
            vec![(1, 0), (2, 1), (4, 2), (3, 1), (5, 0)],
            "a parent that is not in the list puts the document at the top"
        );
    }

    #[test]
    fn outline_shows_a_20000_level_chain_without_a_depth_cut() {
        let n = 20_000;
        let mut list = vec![doc(1, None)];
        for id in 2..=n {
            list.push(doc(id, Some(id - 1)));
        }
        let started = crate::util::CpuTimer::start();
        let rows = outline(&list);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(rows.len(), n as usize, "nothing past 64 levels is dropped");
        assert_eq!(rows[0], (1, 0));
        assert_eq!(rows[n as usize - 1], (n, n as usize - 1));
    }

    #[test]
    fn outline_shows_loop_members_and_self_parents_once_each() {
        // 1 -> 2 -> 3 -> 1 is a loop; 4 hangs off 3; 5 is its own parent.
        let list = vec![
            doc(1, Some(3)),
            doc(2, Some(1)),
            doc(3, Some(2)),
            doc(4, Some(3)),
            doc(5, Some(5)),
            doc(6, None),
        ];
        let rows = outline(&list);
        let mut ids: Vec<i64> = rows.iter().map(|r| r.0).collect();
        ids.sort_unstable();
        assert_eq!(ids, [1, 2, 3, 4, 5, 6], "every document is shown once");
        let top: Vec<i64> = rows.iter().filter(|r| r.1 == 0).map(|r| r.0).collect();
        assert_eq!(
            top,
            [5, 6, 1],
            "a self-parent is a root and the loop shows from its first member"
        );
    }

    #[test]
    fn outline_of_a_20000_row_loop_and_a_wide_table_is_fast() {
        let n = 20_000;
        let mut ring: Vec<Doc> = (1..=n)
            .map(|id| doc(id, Some(if id == 1 { n } else { id - 1 })))
            .collect();
        // Processor time, so a busy machine cannot fail the bound.
        let started = crate::util::CpuTimer::start();
        let rows = outline(&ring);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(rows.len(), n as usize);
        assert_eq!(rows[0], (1, 0));
        // A wide table: 20,000 children under one parent.
        ring = vec![doc(1, None)];
        ring.extend((2..=n).map(|id| doc(id, Some(1))));
        let started = crate::util::CpuTimer::start();
        assert_eq!(outline(&ring).len(), n as usize);
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
