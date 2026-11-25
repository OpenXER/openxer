//! Cell commands of the view tables besides the Activities window, through
//! the one table they share (`grid`): the focus cell (the cell last
//! clicked), Fill Down (Ctrl+E and the Edit and right-click menus) and Find,
//! Find Next and Replace. They follow the activity table's: Fill Down copies
//! the focused cell of the row the selection was started on to the other
//! selected rows, above it as well as below; Find looks through the cells as
//! shown and selects the next row with a match; Replace changes matching
//! text in the cells that can be typed in.
//!
//! Each table notes its rows when it is drawn (`note_view_cells`), and a
//! value changes through the view's own setter (`set_view_cell`), so the
//! details pane and these commands always agree. The WBS window offers Find
//! and Replace; its Fill Down stays unavailable, as in the reference. The
//! Projects window offers Fill Down on the Project Name column only (the
//! reference keeps it greyed on Project ID); it has no Find or Replace.
//! The Resource Assignments window offers all three on its assignment
//! rows, through the Resources tab's edit layer calls.
use super::grid::Row;
use super::projects::ProjectRow;
use super::*;
use crate::grid::dialogs::{find_span, replace_matches, FindState};
use std::rc::Rc;

/// A row of a view table that its cell commands can act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CellKey {
    /// A WBS element (`None`: the project row).
    Wbs(Option<i64>),
    Resource(i64),
    /// A project row: its place in `ProjectsState::cell_rows`.
    Project(usize),
    /// A resource assignment (TASKRSRC id).
    Assignment(i64),
}

/// The rows a view table drew, as built (kept, not copied).
pub(crate) trait RowCells {
    fn rows(&self) -> &[Row];
}

impl<A> RowCells for (A, Vec<Row>) {
    fn rows(&self) -> &[Row] {
        &self.1
    }
}

impl<A, B> RowCells for (A, Vec<Row>, B) {
    fn rows(&self) -> &[Row] {
        &self.1
    }
}

/// What the shown view table drew last, and the focus cell of each table.
#[derive(Default)]
pub(crate) struct TableCells {
    /// The table id (`grid::Table::id`) the note is for.
    pub table: &'static str,
    /// Field ids and column titles, by field number.
    pub fields: Vec<String>,
    pub titles: Vec<String>,
    /// The row keys (`None`: a band, which takes no command).
    pub keys: Vec<Option<CellKey>>,
    pub rows: Option<Rc<dyn RowCells>>,
}

impl TableCells {
    fn cell(&self, row: usize, field: usize) -> &str {
        self.rows
            .as_ref()
            .and_then(|r| r.rows().get(row))
            .and_then(|r| r.cells.get(field))
            .map_or("", String::as_str)
    }

    fn row_of(&self, key: CellKey) -> Option<usize> {
        self.keys.iter().position(|k| *k == Some(key))
    }
}

/// The table id of a view whose table has the cell commands.
fn cell_table(view: MainView) -> Option<&'static str> {
    match view {
        MainView::Wbs => Some("wbs"),
        MainView::Resources => Some("resources"),
        MainView::Projects => Some("projects"),
        MainView::Assignments => Some(super::assignments::TABLE),
        _ => None,
    }
}

/// The Projects table's Project Name field.
const PROJECT_NAME: usize = 1;

impl App {
    /// A view table drew `rows` (keys and cells) with fields `fields`.
    pub(crate) fn note_view_cells(
        &mut self,
        table: &'static str,
        fields: Vec<String>,
        titles: Vec<String>,
        keys: Vec<Option<CellKey>>,
        rows: Rc<dyn RowCells>,
    ) {
        let c = &mut self.views.cells;
        c.table = table;
        c.fields = fields;
        c.titles = titles;
        c.keys = keys;
        c.rows = Some(rows);
    }

    /// The columns of view table `table`, which keep its focus cell.
    fn view_columns(&mut self, table: &str) -> Option<&mut grid::Columns> {
        match table {
            "wbs" => Some(&mut self.views.wbs.columns),
            "resources" => Some(&mut self.views.resources.columns),
            "projects" => Some(&mut self.views.projects.columns),
            "assignments" => Some(&mut self.views.assignments.columns),
            _ => None,
        }
    }

    /// The focused field of table `table` (the first column until a cell
    /// is clicked).
    fn view_focus(&self, table: &str) -> Option<usize> {
        match table {
            "wbs" => self.views.wbs.columns.focus,
            "resources" => self.views.resources.columns.focus,
            "projects" => self.views.projects.columns.focus,
            "assignments" => self.views.assignments.columns.focus,
            _ => None,
        }
    }

    /// The shown view's table with cell commands, when it takes them now
    /// and its note is current.
    fn cells_table(&self) -> Option<&'static str> {
        let table = cell_table(self.views.main)?;
        let ready = self.grid_view_keys_ready() && self.views.cells.table == table;
        ready.then_some(table)
    }

    /// Whether Find, Find Next and Replace work in the shown view.
    pub(crate) fn view_find_ready(&self) -> bool {
        self.cells_table().is_some_and(|t| t != "projects")
    }

    /// The selected rows of the shown view table in display order, and the
    /// row the selection was started on.
    fn view_picked(&self) -> (Vec<CellKey>, Option<CellKey>) {
        let v = &self.views;
        match v.main {
            MainView::Resources => {
                let s = &v.resources;
                let picked = s.rows.picked(s.selected.as_ref());
                let first = s.rows.first(s.selected.as_ref());
                (
                    picked.into_iter().map(CellKey::Resource).collect(),
                    first.map(CellKey::Resource),
                )
            }
            MainView::Wbs => {
                let s = &v.wbs;
                let current = Some(&s.selected);
                (
                    s.rows
                        .picked(current)
                        .into_iter()
                        .map(CellKey::Wbs)
                        .collect(),
                    s.rows.first(current).map(CellKey::Wbs),
                )
            }
            MainView::Projects => {
                let s = &v.projects;
                let current = match &s.selected {
                    Some((source, key)) => Some(ProjectRow::Project(source.clone(), *key)),
                    None => s.selected_eps.map(ProjectRow::Eps),
                };
                let at = |row: &ProjectRow| {
                    s.cell_rows
                        .iter()
                        .position(|r| r.0 == *row)
                        .map(CellKey::Project)
                };
                (
                    s.rows
                        .picked(current.as_ref())
                        .iter()
                        .filter_map(at)
                        .collect(),
                    s.rows.first(current.as_ref()).as_ref().and_then(at),
                )
            }
            MainView::Assignments => {
                let s = &v.assignments;
                let key = |k: super::assignments::Key| k.assignment().map(CellKey::Assignment);
                (
                    s.rows
                        .picked(s.selected.as_ref())
                        .into_iter()
                        .filter_map(key)
                        .collect(),
                    s.rows.first(s.selected.as_ref()).and_then(key),
                )
            }
            _ => (vec![], None),
        }
    }

    /// Whether Fill Down can run in the shown view table: two or more rows
    /// selected in a table that fills (not the WBS window's); in the
    /// Projects window only on the Project Name column, as observed.
    pub(crate) fn view_fill_ready(&self) -> bool {
        let rows = self.view_picked().0.len() > 1;
        match self.cells_table() {
            Some("resources" | "assignments") => rows,
            Some("projects") => rows && self.view_focus("projects") == Some(PROJECT_NAME),
            _ => false,
        }
    }

    /// Fill Down in the shown view table, as one undo step: the focused
    /// cell of the row the selection was started on goes to the other
    /// selected rows.
    pub(crate) fn view_fill_down(&mut self, time: f64) {
        let Some(table) = self.cells_table().filter(|_| self.view_fill_ready()) else {
            return;
        };
        let (picked, first) = self.view_picked();
        let Some(source) = first.or_else(|| picked.first().copied()) else {
            return;
        };
        let field = self.view_focus(table).unwrap_or(0);
        let c = &self.views.cells;
        let Some(id) = c.fields.get(field).cloned() else {
            return;
        };
        let Some(text) = c.row_of(source).map(|r| c.cell(r, field).to_string()) else {
            return;
        };
        let before = self.project.clone();
        let before_revision = self.records_revision;
        let (mut changed, mut refused) = (0, None);
        for key in picked.into_iter().filter(|k| *k != source) {
            match self.set_view_cell(key, &id, &text) {
                Ok(true) => changed += 1,
                Ok(false) => {}
                Err(e) => refused = Some(e),
            }
        }
        self.finish_view_cells(before, before_revision, changed, time);
        self.status = match refused {
            Some(e) => format!("Fill Down: {changed} changed. {e}"),
            None => format!("Fill Down: {changed} changed"),
        };
    }

    /// One undo step when `changed` cells changed, else the project as it was.
    fn finish_view_cells(
        &mut self,
        before: ProjectData,
        before_revision: u64,
        changed: usize,
        time: f64,
    ) {
        if changed > 0 {
            self.edit_step(before, before_revision, time);
        } else {
            self.project = before;
            self.records_revision = before_revision;
        }
    }

    /// Set field `id` of view table row `key` to `text` through the view's
    /// setter. Returns whether the value changed; a field that cannot be
    /// typed in here is refused.
    pub(crate) fn set_view_cell(
        &mut self,
        key: CellKey,
        id: &str,
        text: &str,
    ) -> Result<bool, String> {
        let text = text.trim();
        match (key, id) {
            (CellKey::Resource(rsrc), "resource_name" | "resource_id") => {
                let column = if id == "resource_name" {
                    "rsrc_name"
                } else {
                    "rsrc_short_name"
                };
                self.set_resource_text(rsrc, column, text)
            }
            // The WBS table's own fields that edit in place (code, name,
            // status, responsible manager, anticipated dates).
            (CellKey::Wbs(Some(element)), id) if super::wbs_columns::field_of(id).is_some() => {
                self.set_wbs_cell(element, id, text)
            }
            // User Defined columns: the WBS table's (`udf_<id>`), the
            // Resources table's resource fields (`rudf_<id>`) and the
            // Resource Assignments table's (`udf_<id>`).
            (CellKey::Wbs(Some(element)), id) if super::udf_columns::type_of(id).is_some() => {
                let type_id = super::udf_columns::type_of(id).unwrap_or_default();
                self.set_udf_cell(type_id, element, text)
            }
            (CellKey::Resource(rsrc), id) if id.starts_with("rudf_") => {
                let type_id = super::udf_columns::type_of(&id[1..]).unwrap_or_default();
                self.set_udf_cell(type_id, rsrc, text)
            }
            // A project's fields that edit in the table, through the edit
            // layer. OpenXER edits the current project only (as Project
            // Details does).
            (CellKey::Project(k), id) if super::projects::EDITED_FIELDS.contains(&id) => {
                let current = self.views.projects.cell_rows.get(k).is_some_and(|r| r.1);
                if !current {
                    return Err("Only the current project's values can change here.".into());
                }
                self.set_project_cell(id, text)
            }
            (CellKey::Assignment(id), field) if super::udf_columns::type_of(field).is_some() => {
                let type_id = super::udf_columns::type_of(field).unwrap_or_default();
                self.set_udf_cell(type_id, id, text)
            }
            (CellKey::Assignment(id), field) => self.set_assignment_cell(id, field, text),
            _ => Err("That column cannot be typed in here.".into()),
        }
    }

    /// The fields Find looks in for the shown view table: the chosen one
    /// (`column`, a field id) or every shown field.
    fn view_find_fields(&self, f: &FindState) -> Vec<usize> {
        let c = &self.views.cells;
        let shown: Vec<usize> = (0..c.fields.len()).collect();
        let at = |id: &String| c.fields.iter().position(|x| x == id);
        match f.column.as_ref().and_then(at) {
            Some(k) => vec![k],
            None => shown,
        }
    }

    /// The shown view table's columns for Find's Look in: (field id, title).
    pub(crate) fn view_find_columns(&self) -> Vec<(String, String)> {
        let c = &self.views.cells;
        c.fields
            .iter()
            .cloned()
            .zip(c.titles.iter().cloned())
            .collect()
    }

    /// The current row of the shown view table, as a position in its note.
    fn view_current_row(&self) -> Option<usize> {
        let c = &self.views.cells;
        match self.views.main {
            MainView::Resources => self
                .views
                .resources
                .selected
                .and_then(|id| c.row_of(CellKey::Resource(id))),
            MainView::Wbs => c.row_of(CellKey::Wbs(self.views.wbs.selected)),
            MainView::Assignments => self
                .views
                .assignments
                .selected
                .and_then(|k| k.assignment())
                .and_then(|id| c.row_of(CellKey::Assignment(id))),
            _ => None,
        }
    }

    /// Make row `row` of the note the current row alone, its field `field`
    /// the focus cell.
    fn view_go_to(&mut self, row: usize, field: usize) {
        let c = &self.views.cells;
        let table = c.table;
        let Some(key) = c.keys.get(row).copied().flatten() else {
            return;
        };
        match key {
            CellKey::Resource(id) => {
                let s = &mut self.views.resources;
                s.selected = s.rows.select(&[id]);
            }
            CellKey::Wbs(element) => {
                let s = &mut self.views.wbs;
                if let Some(next) = s.rows.select(&[element]) {
                    s.selected = next;
                }
                s.code_edit = None;
                s.scroll_to_selected = true;
            }
            // Find does not run in the Projects window.
            CellKey::Project(_) => {}
            CellKey::Assignment(id) => {
                let s = &mut self.views.assignments;
                s.selected = s.rows.select(&[super::assignments::Key::Assignment(id)]);
            }
        }

        if let Some(columns) = self.view_columns(table) {
            columns.focus = Some(field);
        }
    }

    /// Find Next in the shown view table: the next row (after the current
    /// one, wrapping) with a cell holding the text.
    pub(crate) fn view_find_next(&mut self, f: &FindState) -> bool {
        if f.what.is_empty() || self.cells_table().is_none() {
            return false;
        }
        let fields = self.view_find_fields(f);
        let count = self.views.cells.keys.len();
        let start = self.view_current_row().map_or(0, |r| r + 1);
        for step in 0..count {
            let row = (start + step) % count;
            if self.views.cells.keys[row].is_none() {
                continue;
            }
            for &field in &fields {
                let c = &self.views.cells;
                if find_span(c.cell(row, field), &f.what, f.match_case).is_some() {
                    self.view_go_to(row, field);
                    self.status = format!("Found {}", f.what);
                    return true;
                }
            }
        }
        self.status = format!("{} not found.", f.what);
        false
    }

    /// Replace in the focus cell of the current row when it matches, then
    /// find the next match.
    pub(crate) fn view_replace_current(&mut self, f: &FindState, time: f64) {
        if let (Some(table), Some(row)) = (self.cells_table(), self.view_current_row()) {
            let field = self.view_focus(table).unwrap_or(0);
            let c = &self.views.cells;
            let key = c.keys.get(row).copied().flatten();
            let text = c.cell(row, field).to_string();
            let id = c.fields.get(field).cloned();
            if let (Some(key), Some(id)) = (key, id) {
                if find_span(&text, &f.what, f.match_case).is_some() {
                    let new = replace_matches(&text, &f.what, &f.with, f.match_case);
                    let before = self.project.clone();
                    let before_revision = self.records_revision;
                    let changed = match self.set_view_cell(key, &id, &new) {
                        Ok(changed) => usize::from(changed),
                        Err(e) => {
                            self.status = e;
                            0
                        }
                    };
                    self.finish_view_cells(before, before_revision, changed, time);
                }
            }
        }
        self.view_find_next(f);
    }

    /// Replace every match in the shown view table's rows, as one undo
    /// step. Returns (replaced, refused).
    pub(crate) fn view_replace_all(&mut self, f: &FindState, time: f64) -> (usize, usize) {
        if f.what.is_empty() || self.cells_table().is_none() {
            return (0, 0);
        }
        let fields = self.view_find_fields(f);
        let mut work = vec![];
        let c = &self.views.cells;
        for (row, key) in c.keys.iter().enumerate() {
            let Some(key) = *key else {
                continue;
            };
            for &field in &fields {
                let text = c.cell(row, field);
                if find_span(text, &f.what, f.match_case).is_some() {
                    let new = replace_matches(text, &f.what, &f.with, f.match_case);
                    work.push((key, c.fields[field].clone(), new));
                }
            }
        }
        let before = self.project.clone();
        let before_revision = self.records_revision;
        let (mut done, mut refused) = (0, 0);
        for (key, id, new) in work {
            match self.set_view_cell(key, &id, &new) {
                Ok(true) => done += 1,
                Ok(false) => {}
                Err(_) => refused += 1,
            }
        }
        self.finish_view_cells(before, before_revision, done, time);
        self.status = if refused == 0 {
            format!("Replaced {done}")
        } else {
            format!("Replaced {done}; {refused} rejected")
        };
        (done, refused)
    }
}
