//! Activity editing commands: multi-row selection, Copy / Cut / Paste of
//! activities (with Copy Activity Options) and of cell text to and from
//! spreadsheets, Fill Down, Dissolve, Renumber Activity IDs and Global
//! Change. Every command that changes the project is one undo step holding
//! the whole project. The Edit and Tools menus call `menu_item`/`menu_item_icon`
//! directly; the table context menu lives in `table_menu`.
use super::*;
use crate::grid::columns::{self, Column, Editor, Source};
use crate::grid::Row;
pub mod dialogs;
pub mod gc;
pub mod gc_report;
pub mod links;
mod move_across;
pub mod ops;
mod table_menu;
#[cfg(test)]
mod tests;

/// Preference keys.
pub const OPTIONS_KEY: &str = "editing_options_v1";
pub const GLOBAL_CHANGES_KEY: &str = openxer_xer::edit::global_change::PREFERENCE_KEY;

/// A command chosen from a menu, run after the panels are drawn so the row
/// list being painted never changes under the painter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Cut,
    Copy,
    Paste,
    FillDown,
    SelectAll,
    Dissolve,
    Renumber,
    GlobalChange,
    // Table context menu commands.
    ActivityDetails,
    Add,
    Delete,
    ExportExcel,
    Assign(DetailTab),
    /// Assign > Resources, Resources by Role and Roles.
    AssignResource(details::assignments::ResourceAssign),
    Link,
    Unlink,
    Columns,
    Filters,
    GroupSort,
    ExpandAll,
    CollapseAll,
    LineNumbers,
    TableFont,
    CollapseTo,
}

/// Settings kept between sessions.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct EditingOptions {
    pub copy: ops::CopyOptions,
    /// User Preferences, Calculations: Dissolving Activities, Retain Lag.
    /// Off by default: the new relationships get no lag.
    pub retain_lag: bool,
    pub renumber: dialogs::RenumberSettings,
    /// User Preferences, Assistance: warn when progress or a relationship
    /// change puts an activity out of sequence. Off by default.
    pub warn_out_of_sequence: bool,
}

/// One cell a paste, fill or global change could not set.
#[derive(Clone, Debug, PartialEq)]
pub struct Rejected {
    pub activity: String,
    pub column: String,
    pub value: String,
    pub reason: String,
}

#[derive(Default)]
pub struct EditingState {
    /// Selected activities by `task_id`. It applies while it holds the
    /// current activity (`App::selected`); otherwise only that one is selected.
    pub selection: BTreeSet<i64>,
    /// Where a Shift+click range starts.
    pub(crate) anchor: Option<i64>,
    /// The row the selection was started on (a plain click, or the start of
    /// a Shift range); Ctrl+click adds rows without changing it. Fill Down
    /// copies from it, upward too.
    pub(crate) first: Option<i64>,
    /// First column of a Shift+click column range (the focused column is
    /// the other end); no range while it is `None` or the focused column.
    pub col_anchor: Option<String>,
    pub clipboard: Option<ops::Clipboard>,
    pub options: EditingOptions,
    pub global_changes: Vec<gc::GlobalChange>,
    pub dialogs: dialogs::EditDialogs,
    pub pending: Option<Command>,
    /// The last menu command run (tests read it).
    pub last: Option<Command>,
}

impl App {
    // ------------------------------------------------------------ selection

    fn task_id_of(&self, i: usize) -> Option<i64> {
        self.project.model.activities.get(i).map(|a| a.task_id)
    }

    /// Does the multi-row selection apply (it holds the current activity)?
    fn multi_selection(&self) -> bool {
        self.selected
            .and_then(|i| self.task_id_of(i))
            .is_some_and(|t| self.editing.selection.contains(&t))
    }

    /// Is activity `i` selected (painted as selected)?
    pub(crate) fn is_selected(&self, i: usize) -> bool {
        self.selected == Some(i)
            || (self.multi_selection()
                && self
                    .task_id_of(i)
                    .is_some_and(|t| self.editing.selection.contains(&t)))
    }

    /// A click on activity row `i`: plain click selects it alone, Ctrl+click
    /// toggles it, Shift+click selects the rows from the anchor to it.
    pub(crate) fn click_row(&mut self, i: usize, modifiers: egui::Modifiers) {
        // A row of another open project makes that project the active one.
        let active = self.open_set.active;
        let Some(i) = self.local_row(i) else {
            return;
        };
        self.grid.cells.selected_band = None;
        let modifiers = if self.open_set.active == active {
            modifiers
        } else {
            egui::Modifiers::default()
        };
        let Some(task_id) = self.task_id_of(i) else {
            return;
        };
        if !self.multi_selection() {
            self.editing.selection = self
                .selected
                .and_then(|s| self.task_id_of(s))
                .into_iter()
                .collect();
            self.editing.first = self.selected.and_then(|s| self.task_id_of(s));
        }
        if modifiers.shift {
            let anchor = self
                .editing
                .anchor
                .filter(|a| {
                    self.project
                        .model
                        .activities
                        .iter()
                        .any(|x| x.task_id == *a)
                })
                .or_else(|| self.selected.and_then(|s| self.task_id_of(s)))
                .unwrap_or(task_id);
            let order = self.table_activities();
            let at = |t: i64| {
                order
                    .iter()
                    .position(|&k| self.project.model.activities[k].task_id == t)
            };
            self.editing.selection = match (at(anchor), at(task_id)) {
                (Some(a), Some(b)) => order[a.min(b)..=a.max(b)]
                    .iter()
                    .map(|&k| self.project.model.activities[k].task_id)
                    .collect(),
                _ => [task_id].into(),
            };
            self.editing.anchor = Some(anchor);
            self.editing.first = Some(anchor);
            if self.editing.col_anchor.is_none() {
                self.editing.col_anchor = self.grid.focus_column.clone();
            }
            self.selected = Some(i);
        } else if modifiers.command {
            if !self.editing.selection.remove(&task_id) {
                self.editing.selection.insert(task_id);
                self.selected = Some(i);
            } else if self.selected == Some(i) {
                // The current row moves to another selected row, if any.
                let next = self.editing.selection.iter().next().copied();
                match next.and_then(|t| {
                    self.project
                        .model
                        .activities
                        .iter()
                        .position(|a| a.task_id == t)
                }) {
                    Some(k) => self.selected = Some(k),
                    None => {
                        self.editing.selection.insert(task_id);
                    }
                }
            }
            self.editing.anchor = Some(task_id);
            self.editing.col_anchor = None;
            if self
                .editing
                .first
                .is_none_or(|f| !self.editing.selection.contains(&f))
            {
                self.editing.first = None;
            }
        } else {
            self.editing.selection = [task_id].into();
            self.editing.anchor = Some(task_id);
            self.editing.first = Some(task_id);
            self.editing.col_anchor = None;
            self.selected = Some(i);
        }
        self.date_edits.clear();
    }

    /// Activity rows in table order (collapsed bands hide their rows).
    pub(crate) fn table_activities(&self) -> Vec<usize> {
        self.rows()
            .into_iter()
            .filter_map(|r| match r {
                Row::Activity(i) => Some(i),
                Row::Group(_) => None,
            })
            .collect()
    }

    /// Selected activities in table order; selected rows hidden by a filter
    /// or a collapsed band follow in model order.
    pub(crate) fn selected_in_order(&self) -> Vec<usize> {
        let count = self.project.model.activities.len();
        let Some(current) = self.selected.filter(|&i| i < count) else {
            return vec![];
        };
        if !self.multi_selection() {
            return vec![current];
        }
        let mut out: Vec<usize> = self
            .table_activities()
            .into_iter()
            .filter(|&i| self.is_selected(i))
            .collect();
        let shown: BTreeSet<usize> = out.iter().copied().collect();
        out.extend((0..count).filter(|i| !shown.contains(i) && self.is_selected(*i)));
        out
    }

    /// Select activities (model indices); the first becomes current.
    pub(crate) fn select_activities(&mut self, rows: &[usize]) {
        let ids: BTreeSet<i64> = rows.iter().filter_map(|&i| self.task_id_of(i)).collect();
        if ids.is_empty() {
            return;
        }
        self.editing.selection = ids;
        self.grid.cells.selected_band = None;
        self.editing.anchor = rows.first().and_then(|&i| self.task_id_of(i));
        self.editing.first = self.editing.anchor;
        self.editing.col_anchor = None;
        self.selected = rows.first().copied();
        self.date_edits.clear();
    }

    /// The table's row keys: Up and Down move one row, PageUp and PageDown
    /// by the rows in view, Home and End (with or without Ctrl) to the first
    /// and last row. Shift with any of them extends the selection from its
    /// anchor. With no activity selected (a band, or nothing) a key starts
    /// at the first row.
    pub(crate) fn row_keys(&mut self, ctx: &egui::Context) {
        let page = self.grid.page_rows.max(1) as isize;
        let (step, home, end, modifiers) = ctx.input(|i| {
            let step = if i.key_pressed(Key::ArrowDown) {
                1
            } else if i.key_pressed(Key::ArrowUp) {
                -1
            } else if i.key_pressed(Key::PageDown) {
                page
            } else if i.key_pressed(Key::PageUp) {
                -page
            } else {
                0
            };
            (
                step,
                i.key_pressed(Key::Home),
                i.key_pressed(Key::End),
                i.modifiers,
            )
        });
        if step == 0 && !home && !end {
            return;
        }
        let visible: Vec<usize> = self
            .rows()
            .into_iter()
            .filter_map(|r| match r {
                Row::Activity(i) => Some(i),
                _ => None,
            })
            .collect();
        if visible.is_empty() {
            return;
        }
        let last = visible.len() - 1;
        let current = visible.iter().position(|i| Some(*i) == self.selected);
        let next = if home {
            0
        } else if end {
            last
        } else {
            match current {
                Some(c) => (c as isize + step).clamp(0, last as isize) as usize,
                None => 0,
            }
        };
        if modifiers.shift && !modifiers.command && current.is_some() {
            // Shift extends the selection from its anchor.
            self.click_row(visible[next], modifiers);
        } else {
            self.grid.cells.selected_band = None;
            self.selected = Some(visible[next]);
        }
        self.scroll_to_selected = true;
        self.date_edits.clear();
    }

    /// The activity to select once `ids` are removed: the next table row
    /// after the first of them that stays, else the nearest one before it
    /// (by task_id; None when none stays).
    pub(crate) fn next_after_removal(&self, ids: &[i64]) -> Option<i64> {
        let gone: BTreeSet<i64> = ids.iter().copied().collect();
        let order: Vec<i64> = self
            .table_activities()
            .into_iter()
            .filter_map(|i| self.task_id_of(i))
            .collect();
        let first = order.iter().position(|id| gone.contains(id))?;
        order[first..]
            .iter()
            .find(|id| !gone.contains(id))
            .or_else(|| order[..first].iter().rev().find(|id| !gone.contains(id)))
            .copied()
    }

    /// Select the activity `next_after_removal` chose and scroll it into view.
    pub(crate) fn select_after_removal(&mut self, next: Option<i64>) {
        self.selected = next.and_then(|id| {
            self.project
                .model
                .activities
                .iter()
                .position(|a| a.task_id == id)
        });
        if self.selected.is_some() {
            self.scroll_to_selected = true;
        }
    }

    /// Ctrl+A: every activity row the table shows.
    pub(crate) fn select_all(&mut self) {
        let rows = self.table_activities();
        let current = self.selected.filter(|i| rows.contains(i));
        self.select_activities(&rows);
        if current.is_some() {
            self.selected = current;
        }
        self.status = format!("{} activities selected", rows.len());
    }

    /// Visible columns of the current layout.
    fn shown_columns(&self) -> Vec<Column> {
        self.visible_columns(&columns::catalog(&self.project))
    }

    /// The columns a cell command works on: the Shift+click column range,
    /// else the focused column.
    pub(crate) fn command_columns(&self) -> Vec<Column> {
        let cols = self.shown_columns();
        let at = |id: &str| cols.iter().position(|c| c.id == id);
        let focus = self
            .grid
            .focus_column
            .as_deref()
            .and_then(at)
            .unwrap_or(0)
            .min(cols.len().saturating_sub(1));
        match self.editing.col_anchor.as_deref().and_then(at) {
            Some(a) => cols[a.min(focus)..=a.max(focus)].to_vec(),
            None => cols.into_iter().skip(focus).take(1).collect(),
        }
    }

    // --------------------------------------------------------------- undo

    /// Record the change since `before` as one undo step.
    pub(crate) fn edit_step(&mut self, before: ProjectData, before_revision: u64, time: f64) {
        self.unpin_serial();
        self.schedule_bulk_edit(&before.model);
        self.records_serial += 1;
        self.records_revision = self.records_serial;
        self.history.record_bulk(
            &before,
            before_revision,
            &self.project,
            self.records_revision,
            time,
        );
        self.dirty = true;
        self.edit_cell = None;
        self.date_edits.clear();
        self.detail.rows = [None, None];
    }

    // ------------------------------------------------------------ commands

    pub(crate) fn editing_dialog_open(&self) -> bool {
        self.editing.dialogs.any_open()
    }

    /// Keyboard commands of the activity table. Called only when no text
    /// field has keyboard focus and no other dialog is open.
    pub(crate) fn editing_shortcuts(&mut self, ctx: &egui::Context) {
        if self.editing_dialog_open() {
            return;
        }
        let time = ctx.input(|i| i.time);
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            match event {
                egui::Event::Copy => self.copy_activities(ctx),
                egui::Event::Cut => self.cut_activities(ctx, time),
                egui::Event::Paste(text) => self.paste_from_clipboard(&text, time),
                _ => {}
            }
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::E)) {
            self.fill_down(time);
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::A)) {
            self.select_all();
        }
    }

    /// Run a command chosen from a menu.
    pub(crate) fn run_edit_command(&mut self, ctx: &egui::Context, command: Command) {
        let time = ctx.input(|i| i.time);
        match command {
            Command::Cut => self.cut_activities(ctx, time),
            Command::Copy => self.copy_activities(ctx),
            Command::Paste => self.request_paste(time),
            Command::FillDown if self.views.main.is_activities() => self.fill_down(time),
            // A view table's Fill Down (`views::table_cells`).
            Command::FillDown => self.view_fill_down(time),
            Command::SelectAll if self.views.main.is_activities() => self.select_all(),
            Command::SelectAll => self.grid_view_select_all(),
            Command::Dissolve => self.request_dissolve(),
            Command::Renumber => self.open_renumber(),
            Command::GlobalChange => self.open_global_change(),
            Command::ActivityDetails => self.set_bottom(Some(panes::BottomView::Details)),
            Command::Add => self.add_activity_command(),
            Command::Delete => self.request_delete(),
            Command::ExportExcel => self.table_export_excel(),
            Command::Assign(tab) => self.open_assign(tab),
            Command::AssignResource(kind) => self.open_assign_resource(kind),
            Command::Link => self.link_selected(time),
            Command::Unlink => self.unlink_selected(time),
            Command::Columns => self.open_columns_dialog(),
            Command::Filters => self.open_filters_dialog(),
            Command::GroupSort => self.open_group_sort_dialog(),
            Command::ExpandAll => self.expand_all_bands(),
            Command::CollapseAll => self.collapse_all_bands(),
            Command::LineNumbers => self.toggle_line_numbers(),
            Command::TableFont => self.open_table_font(),
            Command::CollapseTo => self.open_collapse_to(),
        }
        self.editing.last = Some(command);
    }

    /// Per frame, after the panels: run a menu command and draw the dialogs.
    pub(crate) fn editing_frame(&mut self, ctx: &egui::Context) {
        if let Some(command) = self.editing.pending.take() {
            self.run_edit_command(ctx, command);
        }
        self.editing_dialogs(ctx);
    }

    /// Tab separated text of the selected rows: the Shift+click column range
    /// alone, or every visible column under a header line.
    pub(crate) fn selection_text(&self, rows: &[usize]) -> String {
        let ix = self.grid_index();
        let range = self
            .editing
            .col_anchor
            .as_ref()
            .is_some_and(|a| Some(a) != self.grid.focus_column.as_ref());
        let (cols, header) = if range {
            (self.command_columns(), false)
        } else {
            (self.shown_columns(), true)
        };
        let mut lines = vec![];
        if header {
            lines.push(cols.iter().map(|c| c.title.clone()).collect());
        }
        for &i in rows {
            lines.push(
                cols.iter()
                    .map(|c| {
                        columns::display(
                            &self.project,
                            Some(i),
                            &columns::value(&self.project, &ix, c, i),
                        )
                    })
                    .collect(),
            );
        }
        ops::to_tsv(&lines)
    }

    /// Ctrl+C: copy the selected activities for Paste and their cells as
    /// text for spreadsheets.
    pub(crate) fn copy_activities(&mut self, ctx: &egui::Context) {
        let rows = self.selected_in_order();
        if rows.is_empty() {
            self.status = "Select the activities to copy.".into();
            return;
        }
        let mut clip = ops::copy(&self.project, &rows);
        clip.text = self.selection_text(&rows);
        ctx.copy_text(clip.text.clone());
        self.editing.clipboard = Some(clip);
        self.status = match rows.len() {
            1 => "1 activity copied".into(),
            n => format!("{n} activities copied"),
        };
    }

    /// Ctrl+X: mark the selected activities for a move. Nothing asks and
    /// nothing changes until Paste, which moves them (`move_cut_activities`).
    /// A later Copy or Cut replaces the mark.
    pub(crate) fn cut_activities(&mut self, ctx: &egui::Context, _time: f64) {
        let rows = self.selected_in_order();
        if rows.is_empty() {
            self.status = "Select the activities to cut.".into();
            return;
        }
        self.copy_activities(ctx);
        if let Some(clip) = self.editing.clipboard.as_mut() {
            clip.cut = true;
        }
        self.status = match rows.len() {
            1 => "1 activity cut. Paste moves it.".into(),
            n => format!("{n} activities cut. Paste moves them."),
        };
    }

    /// Paste of a cut selection in its own project: move the activities to
    /// the paste target's WBS through the shared WBS setter, keeping their
    /// IDs, relationships and everything else. No dialogs. Activities that
    /// are already there stay as they are; when none moves nothing is
    /// recorded and the cut activities are just selected.
    fn move_cut_activities(&mut self, time: f64) {
        let Some(clip) = self.editing.clipboard.clone() else {
            return;
        };
        let rows: Vec<usize> = clip
            .activities
            .iter()
            .filter_map(|c| {
                self.project
                    .model
                    .activities
                    .iter()
                    .position(|a| a.task_id == c.activity.task_id)
            })
            .collect();
        if rows.is_empty() {
            self.error("The cut activities are no longer in this project.".to_string());
            return;
        }
        let wbs = self.target_wbs();
        let before = self.project.clone();
        let before_revision = self.records_revision;
        let mut moved = 0;
        for &i in &rows {
            match self.edit_activity(i, details::setters::Edit::Wbs(wbs)) {
                Ok(true) => moved += 1,
                Ok(false) => {}
                Err(e) => {
                    self.project = before;
                    self.records_revision = before_revision;
                    self.error(e);
                    return;
                }
            }
        }
        if moved > 0 {
            self.edit_step(before, before_revision, time);
        }
        self.select_activities(&rows);
        self.reveal_activity(rows[0]);
        self.scroll_to_selected = true;
        self.status = match moved {
            0 => "Already in that WBS: nothing moved.".into(),
            1 => "1 activity moved".into(),
            n => format!("{n} activities moved"),
        };
    }

    /// Ctrl+V: activities copied here paste as activities; any other text
    /// is pasted into the cells from the focused cell down and right.
    pub(crate) fn paste_from_clipboard(&mut self, text: &str, time: f64) {
        let ours = self
            .editing
            .clipboard
            .as_ref()
            .is_some_and(|c| ops::same_text(&c.text, text));
        if ours {
            self.request_paste(time);
        } else {
            self.paste_text(text, time);
        }
    }

    /// Paste activities, through Copy Activity Options unless it is turned
    /// off. A cut selection pasted in its own project is moved instead.
    pub(crate) fn request_paste(&mut self, time: f64) {
        let Some(clip) = self.editing.clipboard.as_ref() else {
            self.status = "Copy or cut activities first.".into();
            return;
        };
        if clip.cut && clip.source == ops::project_key(&self.project) {
            self.move_cut_activities(time);
            return;
        }
        // Cut in another open project: a move too (`move_across`).
        if clip.cut && self.move_cut_across(time) {
            return;
        }
        if self.editing.options.copy.show_dialog {
            self.editing.dialogs.copy_options = Some(self.editing.options.copy.clone());
        } else {
            self.paste_after_options(time);
        }
    }

    /// The WBS pasted activities go to: the current activity's, else the
    /// selected band's, else the WBS filter's, else the first.
    fn target_wbs(&self) -> i64 {
        self.selected
            .and_then(|i| self.project.model.activities.get(i))
            .and_then(|a| self.project.details.get(&a.task_id))
            .map(|d| d.wbs)
            .or_else(|| self.band_assignments().and_then(|(wbs, _)| wbs))
            .or(self.wbs_filter)
            .or_else(|| self.project.wbs.first().map(|w| w.id))
            .unwrap_or(0)
    }

    /// Paste the copied activities with free IDs (each keeps its own when it
    /// is free, else the project's auto-numbering gives one).
    pub(crate) fn paste_activities(&mut self, time: f64) {
        self.paste_activities_with(None, time);
    }

    /// Paste the copied activities with the IDs the Renumber Activity IDs
    /// step chose (`codes`, clipboard order).
    pub(crate) fn paste_activities_as(&mut self, codes: &[String], time: f64) {
        self.paste_activities_with(Some(codes), time);
    }

    fn paste_activities_with(&mut self, codes: Option<&[String]>, time: f64) {
        let Some(clip) = self.editing.clipboard.clone() else {
            return;
        };
        let before = self.project.clone();
        let before_revision = self.records_revision;
        let wbs = self.target_wbs();
        let anchor = self
            .selected
            .and_then(|i| self.project.model.activities.get(i))
            .map(|a| a.task_code.clone());
        let options = self.editing.options.copy.clone();
        let pasted = match codes {
            Some(codes) => ops::paste_renumbered(&mut self.project, &clip, &options, wbs, codes),
            None => {
                ops::paste_activities(&mut self.project, &clip, &options, wbs, anchor.as_deref())
            }
        };
        match pasted {
            Ok(pasted) => {
                let rows = &pasted.rows;
                self.project.rebuild_assignments();
                self.edit_step(before, before_revision, time);
                self.select_activities(rows);
                if let Some(&first) = rows.first() {
                    self.reveal_activity(first);
                    self.scroll_to_selected = true;
                }
                let count = match rows.len() {
                    1 => "1 activity pasted".to_string(),
                    n => format!("{n} activities pasted"),
                };
                let mut status = match pasted.note() {
                    Some(note) => format!("{count}. {note}"),
                    None => count,
                };
                // A cut from a project that is no longer open pastes as
                // copies: that project is not changed.
                if clip.cut && clip.source != ops::project_key(&self.project) {
                    if !status.ends_with('.') {
                        status.push('.');
                    }
                    status.push_str(
                        " Their project is not open, so the cut activities were copied and \
                         remain there.",
                    );
                }
                self.status = status;
            }
            Err(e) => {
                self.project = before;
                self.error(e);
            }
        }
    }

    /// Text for a value as the in-place editor would show it.
    pub(crate) fn input_text(&self, i: usize, column: &Column) -> String {
        let ix = self.grid_index();
        gc::input_text(
            &self.project,
            i,
            &columns::value(&self.project, &ix, column, i),
        )
    }

    /// Set cells; returns how many changed and the rejected ones. A cell
    /// given the value it already holds is not counted. The caller records
    /// the undo step.
    fn set_cells(&mut self, cells: Vec<(usize, Column, String)>) -> (usize, Vec<Rejected>) {
        let mut applied = 0;
        let mut rejected = vec![];
        for (i, column, text) in cells {
            // The same cells the in-place editor edits, user defined fields
            // included.
            let editable = match column.source {
                Source::Notebook(_) => Err(format!("{} is not editable.", column.title)),
                _ if column.editor == Editor::ReadOnly => {
                    Err(format!("{} is not editable.", column.title))
                }
                _ => Ok(()),
            };
            match editable.and_then(|_| self.try_apply_cell(i, &column.id, &text)) {
                Ok(true) => applied += 1,
                Ok(false) => {}
                Err(reason) => rejected.push(Rejected {
                    activity: self.project.model.activities[i].task_code.clone(),
                    column: column.title.clone(),
                    value: text,
                    reason,
                }),
            }
        }
        (applied, rejected)
    }

    /// Finish a cell command: one undo step when anything changed, and the
    /// summary window when cells were rejected.
    fn finish_cells(
        &mut self,
        title: &str,
        before: ProjectData,
        before_revision: u64,
        applied: usize,
        rejected: Vec<Rejected>,
        time: f64,
    ) {
        if applied > 0 {
            self.edit_step(before, before_revision, time);
        } else {
            self.project = before;
            self.records_revision = before_revision;
        }
        self.error = None;
        self.status = format!(
            "{title}: {applied} cell{} changed, {} rejected",
            if applied == 1 { "" } else { "s" },
            rejected.len()
        );
        if !rejected.is_empty() {
            self.editing.dialogs.summary = Some(dialogs::Summary {
                title: title.into(),
                applied,
                rejected,
            });
        }
    }

    /// Paste spreadsheet text into the table: from the first selected row
    /// and the first column of the column range (or the focused column),
    /// down the rows in table order and right across the visible columns. A
    /// single value fills every selected row. A first line repeating the
    /// column titles is skipped.
    pub(crate) fn paste_text(&mut self, text: &str, time: f64) {
        let mut block = ops::parse_tsv(text);
        let selected = self.selected_in_order();
        let Some(&first) = selected.first() else {
            self.status = "Select the cell to paste into.".into();
            return;
        };
        let cols = self.shown_columns();
        let start_col = self
            .command_columns()
            .first()
            .and_then(|c| cols.iter().position(|x| x.id == c.id))
            .unwrap_or(0);
        let titles_match = block.first().is_some_and(|line| {
            line.len() > 1
                && line.iter().enumerate().all(|(k, cell)| {
                    cols.get(start_col + k)
                        .is_some_and(|c| c.title.eq_ignore_ascii_case(cell.trim()))
                })
        });
        if titles_match {
            block.remove(0);
        }
        if block.is_empty() {
            return;
        }
        let rows: Vec<usize> = if block.len() == 1 && block[0].len() == 1 && selected.len() > 1 {
            block = vec![block[0].clone(); selected.len()];
            selected
        } else {
            let order = self.table_activities();
            let from = order.iter().position(|&i| i == first).unwrap_or(0);
            order[from..].to_vec()
        };
        let mut cells = vec![];
        let mut rejected = vec![];
        for (r, line) in block.iter().enumerate() {
            for (k, value) in line.iter().enumerate() {
                match (rows.get(r), cols.get(start_col + k)) {
                    (Some(&i), Some(c)) => cells.push((i, c.clone(), value.clone())),
                    _ => rejected.push(Rejected {
                        activity: String::new(),
                        column: cols
                            .get(start_col + k)
                            .map(|c| c.title.clone())
                            .unwrap_or_default(),
                        value: value.clone(),
                        reason: "Outside the table.".into(),
                    }),
                }
            }
        }
        let before = self.project.clone();
        let before_revision = self.records_revision;
        let (applied, mut more) = self.set_cells(cells);
        more.extend(rejected);
        self.finish_cells("Paste", before, before_revision, applied, more, time);
    }

    /// The row Fill Down copies from: the row the selection was started on
    /// when it is still selected, else the top-most selected row.
    pub(crate) fn fill_source(&self, rows: &[usize]) -> Option<usize> {
        let first = self.editing.first.and_then(|t| {
            rows.iter()
                .copied()
                .find(|&i| self.task_id_of(i) == Some(t))
        });
        first.or_else(|| rows.first().copied())
    }

    /// Ctrl+E: copy the value of the row the selection was started on (see
    /// `fill_source`) in the focused column (or column range) to the other
    /// selected rows, above it as well as below. Rows between them that are
    /// not selected keep their values.
    pub(crate) fn fill_down(&mut self, time: f64) {
        let rows = self.selected_in_order();
        if rows.len() < 2 {
            self.status = "Select two or more activities to fill down.".into();
            return;
        }
        let Some(source) = self.fill_source(&rows) else {
            return;
        };
        let mut cells = vec![];
        for column in self.command_columns() {
            let text = self.input_text(source, &column);
            for &i in rows.iter().filter(|&&i| i != source) {
                cells.push((i, column.clone(), text.clone()));
            }
        }
        let before = self.project.clone();
        let before_revision = self.records_revision;
        let (applied, rejected) = self.set_cells(cells);
        self.finish_cells(
            "Fill Down",
            before,
            before_revision,
            applied,
            rejected,
            time,
        );
    }

    /// Whether Dissolve can run: an activity is selected and every selected
    /// activity has at least one predecessor and one successor. The menus
    /// show Dissolve disabled otherwise.
    pub(crate) fn can_dissolve_selection(&self) -> bool {
        let rows = self.selected_in_order();
        !rows.is_empty()
            && rows
                .iter()
                .all(|&i| ops::can_dissolve(&self.project.model, i))
    }

    /// Ask before dissolving the selected activities.
    pub(crate) fn request_dissolve(&mut self) {
        let ids: Vec<i64> = self
            .selected_in_order()
            .into_iter()
            .map(|i| self.project.model.activities[i].task_id)
            .collect();
        if ids.is_empty() {
            self.status = "Select the activity to dissolve.".into();
            return;
        }
        if !self.can_dissolve_selection() {
            self.status =
                "Dissolve needs an activity with at least one predecessor and one successor."
                    .into();
            return;
        }
        self.editing.dialogs.dissolve = Some(ids);
    }

    /// Dissolve activities as one undo step, with the Retain Lag preference.
    /// Nothing is rescheduled.
    pub(crate) fn dissolve_activities(&mut self, ids: &[i64], time: f64) {
        let before = self.project.clone();
        let before_revision = self.records_revision;
        let next = self.next_after_removal(ids);
        match ops::dissolve(&mut self.project, ids, self.editing.options.retain_lag) {
            Ok(ops::Dissolved { added, .. }) => {
                self.edit_step(before, before_revision, time);
                self.select_after_removal(next);
                self.editing.selection.clear();
                self.link_drag = None;
                self.link_editor = None;
                self.status = format!(
                    "{} dissolved, {added} relationship{} added",
                    if ids.len() == 1 {
                        "1 activity".to_string()
                    } else {
                        format!("{} activities", ids.len())
                    },
                    if added == 1 { "" } else { "s" }
                );
            }
            Err(e) => {
                self.project = before;
                self.error(e);
            }
        }
    }

    // ------------------------------------------------------------- menus

    /// One menu item in the shared menu style that queues an editing command.
    pub(crate) fn menu_item(
        &mut self,
        ui: &mut egui::Ui,
        action: &str,
        caption: &str,
        key: &str,
        enabled: bool,
        command: Command,
    ) {
        self.menu_item_icon(ui, action, caption, key, enabled, command, None);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn menu_item_icon(
        &mut self,
        ui: &mut egui::Ui,
        action: &str,
        caption: &str,
        key: &str,
        enabled: bool,
        command: Command,
        icon: Option<chrome::Icon>,
    ) {
        let mut item = chrome::Item::new(caption).enabled(enabled);
        if !key.is_empty() {
            item = item.key(key);
        }
        item.icon = icon;
        if self.item(ui, action, item) {
            self.editing.pending = Some(command);
        }
    }

    // ------------------------------------------------------- preferences

    pub(crate) fn restore_editing(&mut self) {
        if let Ok(Some(text)) = self.store.preference(OPTIONS_KEY) {
            match serde_json::from_str::<EditingOptions>(&text) {
                Ok(o) => self.editing.options = o,
                Err(e) => self.status = format!("Saved editing options ignored: {e}"),
            }
        }
        if let Ok(Some(text)) = self.store.preference(GLOBAL_CHANGES_KEY) {
            match gc::from_json(&text) {
                Ok(list) => self.editing.global_changes = list,
                Err(e) => self.status = format!("Saved global changes ignored: {e}"),
            }
        }
    }

    pub(crate) fn save_editing_options(&mut self) {
        let text = serde_json::to_string(&self.editing.options).unwrap_or_default();
        if let Err(e) = self.store.set_preference(OPTIONS_KEY, &text) {
            self.error(format!("Cannot save the editing options: {e}"));
        }
    }

    pub(crate) fn save_global_changes(&mut self) {
        let text = gc::to_json(&self.editing.global_changes);
        if let Err(e) = self.store.set_preference(GLOBAL_CHANGES_KEY, &text) {
            self.error(format!("Cannot save global changes: {e}"));
        }
    }
}
