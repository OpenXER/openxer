//! In-place edits of table cells, with the same rules as Activity Details.
use super::columns::{self, Editor, Field, Source, Value};
use super::*;
use crate::details::setters::Edit;
use crate::records;
use group::BandTarget;

/// Does a date field mark the end of work? Finish-type fields take the end
/// of the working day when a date is typed without a time; start-type
/// fields (and UDF dates) take its start. Constraint dates follow their
/// constraint type (the shared catalog's rule).
pub(crate) fn finish_type(field: Field, a: &Activity) -> bool {
    field
        .shared()
        .is_some_and(|f| openxer_xer::edit::fields::finish_type(f, a))
}

/// A band's WBS element and (code type, code value) pairs.
pub type BandValues = (Option<i64>, Vec<(i64, i64)>);

/// Table state of in-place editing, band selection and row dragging.
#[derive(Default)]
pub struct CellState {
    /// The selected band row (its collapse key); no activity is selected then.
    pub selected_band: Option<String>,
    /// Width of the table pane in the last frame, to keep the focused
    /// column in view.
    pub view_width: f32,
    /// Selected activities (`task_id`) being dragged onto a band.
    pub row_drag: Option<Vec<i64>>,
    /// The band row under the pointer during a drag, and each painted
    /// band's rect this frame.
    pub band_rects: Vec<(usize, Rect)>,
    /// The month the date editor's calendar shows while it is open.
    pub calendar: Option<chrono::NaiveDate>,
    /// Tab moved the focus cell this frame; keyboard focus is released at
    /// the end of the frame so no other control takes it.
    pub tab_used: bool,
    /// Move the focus cell after the edit that Tab committed.
    pub after_commit: Option<isize>,
    /// The editor being opened starts with all its text selected, so typing
    /// replaces it (the name of an activity Add just made).
    pub select_all: bool,
    /// The row being painted was the current activity before this frame's
    /// click (set by the Activities view for `grid_cells`).
    pub row_current: bool,
    /// The constraint behind the marked Start or Finish cell under the
    /// pointer in the row just painted, shown as the row's hover text.
    pub constraint_hint: Option<String>,
}

impl App {
    /// Keys of the activity table while no text field has keyboard focus:
    /// Left/Right and Tab/Shift+Tab move the focus cell, `Ctrl+Num +` and
    /// `Ctrl+Num -` expand and collapse every band.
    pub(crate) fn table_keys(&mut self, ctx: &egui::Context) {
        let step = ctx.input(|i| {
            let plain = !i.modifiers.ctrl && !i.modifiers.alt && !i.modifiers.command;
            if !plain {
                0
            } else if i.key_pressed(Key::ArrowRight) && !i.modifiers.shift {
                1
            } else if i.key_pressed(Key::ArrowLeft) && !i.modifiers.shift {
                -1
            } else if i.key_pressed(Key::Tab) {
                if i.modifiers.shift {
                    -1
                } else {
                    1
                }
            } else {
                0
            }
        });
        if step != 0 {
            self.grid.cells.tab_used |= ctx.input(|i| i.key_pressed(Key::Tab));
            self.move_focus_column(step);
        }
        let expand = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, Key::Plus);
        let collapse = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, Key::Minus);
        // Taken here so the window zoom does not see them.
        if ctx.input_mut(|i| i.consume_shortcut(&expand)) {
            self.expand_all_bands();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&collapse)) {
            self.collapse_all_bands();
        }
    }

    /// Move the focus cell `step` visible columns and scroll it into view.
    pub(crate) fn move_focus_column(&mut self, step: isize) {
        let cols = self.visible_columns(&columns::catalog(&self.project));
        if cols.is_empty() {
            return;
        }
        let at = self
            .grid
            .focus_column
            .as_deref()
            .and_then(|id| cols.iter().position(|c| c.id == id))
            .unwrap_or(0);
        let next = (at as isize + step).clamp(0, cols.len() as isize - 1) as usize;
        self.grid.focus_column = Some(cols[next].id.clone());
        self.editing.col_anchor = None;
        self.scroll_column_into_view(&cols, next);
    }

    /// Scroll the table sideways just far enough to show column `at` of
    /// `cols`, or to put its left edge at the left when it is wider than the
    /// view.
    pub(crate) fn scroll_column_into_view(&mut self, cols: &[columns::Column], at: usize) {
        let left: f32 = cols[..at].iter().map(|c| c.width).sum();
        let right = left + cols[at].width;
        let view = self.grid.cells.view_width;
        if view > 0. {
            if left < self.grid.scroll_x {
                self.grid.scroll_x = left;
            } else if right > self.grid.scroll_x + view {
                self.grid.scroll_x = (right - view).min(left);
            }
        }
    }

    /// Scroll the table so the focus column shows (a find, or an edit
    /// started with F2 on a cell that is off to the side).
    pub(crate) fn show_focus_column(&mut self) {
        let cols = self.visible_columns(&columns::catalog(&self.project));
        let at = self
            .grid
            .focus_column
            .as_deref()
            .and_then(|id| cols.iter().position(|c| c.id == id));
        if let Some(at) = at {
            self.scroll_column_into_view(&cols, at);
        }
    }

    /// Select a band row: the activities are deselected and Add/Insert
    /// creates the new activity under this band.
    pub(crate) fn select_band(&mut self, key: &str) {
        self.grid.cells.selected_band = Some(key.to_string());
        self.selected = None;
        self.editing.selection.clear();
        self.edit_cell = None;
        self.date_edits.clear();
    }

    /// The selected band of the last row build, if it is still shown.
    pub(crate) fn selected_band(&self) -> Option<group::Band> {
        let key = self.grid.cells.selected_band.as_ref()?;
        self.grid
            .bands
            .borrow()
            .iter()
            .find(|b| &b.key == key)
            .cloned()
    }

    /// WBS and code values a new activity takes under the selected band:
    /// the band's own and those of the bands it sits in.
    pub(crate) fn band_assignments(&self) -> Option<BandValues> {
        let band = self.selected_band()?;
        let bands = self.grid.bands.borrow();
        let mut chain: Vec<&group::Band> = bands
            .iter()
            .filter(|b| band.key.starts_with(&b.key))
            .collect();
        chain.sort_by_key(|b| b.key.len());
        let mut wbs = None;
        let mut codes = vec![];
        for b in chain {
            match b.target {
                BandTarget::Wbs(id) => wbs = Some(id),
                BandTarget::Code {
                    type_id,
                    value: Some(v),
                } => codes.push((type_id, v)),
                _ => {}
            }
        }
        Some((wbs, codes))
    }

    /// A drag starting on activity row `i`: the selection (or the row
    /// alone when it is not selected) travels with the pointer.
    pub(crate) fn start_row_drag(&mut self, i: usize) {
        if self.grid.bands.borrow().is_empty() || self.local_row(i).is_none() {
            return;
        }
        if !self.is_selected(i) {
            self.click_row(i, Default::default());
        }
        let rows = self.selected_in_order();
        let ids: Vec<i64> = rows
            .iter()
            .filter_map(|&k| self.project.model.activities.get(k).map(|a| a.task_id))
            .collect();
        if !ids.is_empty() {
            self.status = format!(
                "Drop on a WBS or activity code band to move {} activities.",
                ids.len()
            );
            self.grid.cells.row_drag = Some(ids);
        }
    }

    /// End a row drag: dropped on a band row it reassigns the activities;
    /// Escape or a release anywhere else cancels it.
    pub(crate) fn finish_row_drag(&mut self, ctx: &egui::Context) {
        if self.grid.cells.row_drag.is_none() {
            return;
        }
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.grid.cells.row_drag = None;
            self.status = "Move cancelled.".into();
            return;
        }
        if ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        let Some(ids) = self.grid.cells.row_drag.take() else {
            return;
        };
        let at = ctx.input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos()));
        let target = at.and_then(|pos| {
            self.grid
                .cells
                .band_rects
                .iter()
                .find(|(_, r)| r.contains(pos))
                .map(|(b, _)| *b)
        });
        match target {
            Some(band) => {
                self.drop_on_band(band, &ids);
            }
            None => self.status = String::new(),
        }
    }

    /// Reassign activities (`task_id`s) dropped on band `index`: a WBS band
    /// moves them to its element, a code band sets (or clears) the code
    /// value. Other bands refuse the drop with a message.
    pub(crate) fn drop_on_band(&mut self, index: usize, task_ids: &[i64]) -> bool {
        let Some(band) = self.grid.bands.borrow().get(index).cloned() else {
            return false;
        };
        let edit = match band.target {
            BandTarget::Wbs(id) => Edit::Wbs(id),
            BandTarget::Code { type_id, value } => Edit::Code { type_id, value },
            BandTarget::Other => {
                self.error(format!(
                    "Activities can be dropped only on a WBS or activity code band, not on {}.",
                    band.label
                ));
                return false;
            }
        };
        let mut moved = 0;
        for &task_id in task_ids {
            let Some(i) = self
                .project
                .model
                .activities
                .iter()
                .position(|a| a.task_id == task_id)
            else {
                continue;
            };
            match self.edit_activity(i, edit.clone()) {
                Ok(true) => moved += 1,
                Ok(false) => {}
                Err(e) => {
                    self.error(e);
                    return false;
                }
            }
        }
        self.status = match moved {
            0 => "Nothing moved.".into(),
            1 => format!("1 activity moved to {}", band.label),
            n => format!("{n} activities moved to {}", band.label),
        };
        moved > 0
    }

    /// Apply typed text to a cell. Returns false (and reports why) when the
    /// value is rejected; nothing changes then. A Start typed for a
    /// not-started activity may first ask about holding it (`start_hold`).
    pub(crate) fn apply_cell(
        &mut self,
        activity: usize,
        column: impl AsRef<str>,
        value: &str,
    ) -> bool {
        let result = match self.typed_start(activity, column.as_ref(), value) {
            Some(result) => result,
            None => self.try_apply_cell(activity, column.as_ref(), value),
        };
        match result {
            Ok(changed) => {
                if changed {
                    self.dirty = true;
                }
                true
            }
            Err(e) => {
                self.error(e);
                false
            }
        }
    }

    /// Minute of day `day` that a date typed without a time takes for
    /// activity `i`: the start of its calendar's working day, or the end for
    /// a finish-type field (`openxer_xer::edit::fields::day_edge`).
    pub(crate) fn day_edge(&self, i: usize, finish: bool, day: i64) -> i64 {
        openxer_xer::edit::fields::day_edge(&self.project, i, finish, day)
    }

    /// Read a date typed into a date field of activity `i` in the user's
    /// date format. With a value already there (`current`) a date without a
    /// time keeps that time, as before; an empty field takes the start (or
    /// for `finish`, the end) of the activity calendar's working day instead
    /// of midnight.
    pub(crate) fn parse_field_date(
        &self,
        i: usize,
        finish: bool,
        text: &str,
        current: Option<i64>,
    ) -> Result<Option<i64>, String> {
        openxer_xer::edit::fields::parse_field_date(
            &self.project,
            &columns::UserFormats,
            i,
            finish,
            text,
            current,
        )
    }

    /// Is a table cell's choice list open? It takes Up, Down, Enter and
    /// Escape itself; the main window's row keys and Delete wait.
    pub(crate) fn choice_editor_open(&self) -> bool {
        let Some((_, id)) = &self.edit_cell else {
            return false;
        };
        let catalog = columns::catalog(&self.project);
        columns::find(&catalog, id).is_some_and(|c| c.editor == Editor::Choice)
    }

    /// Can this cell be edited now? `Err` carries the reason it cannot.
    pub(crate) fn cell_editable(&self, activity: usize, column: &Column) -> Result<(), String> {
        if self.project.model.activities.get(activity).is_none() {
            return Err("No such activity.".into());
        }
        if column.editor == Editor::ReadOnly {
            return Err(format!("{} is not editable.", column.title));
        }
        match column.source {
            // The same locks as the Activity Details fields.
            Source::Field(field) => self.field_lock(activity, field),
            _ => Ok(()),
        }
    }

    /// Read typed cell text into the change Activity Details would make for
    /// the same field, with the shared catalog's rules and the user's
    /// formats. `Ok(None)` means the column is edited here directly.
    fn cell_edit(&self, i: usize, column: &Column, text: &str) -> Result<Option<Edit>, String> {
        let def = column
            .shared()
            .ok_or_else(|| format!("{} is not editable.", column.title))?;
        let ix = self.grid_index();
        openxer_xer::edit::fields::edit_for(
            &self.project,
            &ix.core,
            &columns::UserFormats,
            &def,
            i,
            text,
        )
    }

    /// A date typed into the Start cell goes through `type_start`, the
    /// question the Status tab's Start asks too; `None` for any other cell.
    fn typed_start(&mut self, i: usize, id: &str, value: &str) -> Option<Result<bool, String>> {
        let catalog = columns::catalog(&self.project);
        let column = columns::find(&catalog, id)
            .filter(|c| c.source == Source::Field(Field::Start))?
            .clone();
        let edit = self
            .cell_editable(i, &column)
            .and_then(|_| self.cell_edit(i, &column, value.trim()));
        Some(match edit {
            Ok(Some(Edit::Start(v))) => {
                self.unpin_serial();
                self.type_start(i, v)
            }
            Ok(_) => return None,
            Err(e) => Err(e),
        })
    }

    /// Apply typed text to a cell through the Activity Details setters.
    /// Returns whether anything changed; on `Err` nothing did.
    pub(crate) fn try_apply_cell(
        &mut self,
        i: usize,
        id: &str,
        value: &str,
    ) -> Result<bool, String> {
        self.unpin_serial();
        let catalog = columns::catalog(&self.project);
        let column = columns::find(&catalog, id)
            .cloned()
            .ok_or_else(|| format!("No column {id}."))?;
        self.cell_editable(i, &column)?;
        let text = value.trim();
        // Units show rounded; the text the cell opened with is not a change.
        if matches!(
            column.source,
            Source::Field(Field::Figure(..) | Field::ThisPeriod(_))
        ) {
            let ix = self.grid_index();
            let shown = columns::display(
                &self.project,
                Some(i),
                &columns::value(&self.project, &ix, &column, i),
            );
            if shown == text {
                return Ok(false);
            }
        }
        if let Some(edit) = self.cell_edit(i, &column, text)? {
            return self.edit_activity(i, edit);
        }
        match column.source {
            Source::Field(Field::ActivityId) => {
                openxer_xer::edit::activity::set_activity_id(&mut self.project, i, text)
            }
            Source::Field(Field::ActivityName) => {
                openxer_xer::edit::activity::set_activity_name(&mut self.project, i, text)
            }
            _ => Err(format!("{} is not editable.", column.title)),
        }
    }

    /// Start editing the focused cell of the selected activity (F2).
    pub(crate) fn begin_edit_focused(&mut self) {
        let Some(i) = self.selected else {
            return;
        };
        let id = self
            .grid
            .focus_column
            .clone()
            .unwrap_or_else(|| "activity_name".into());
        let catalog = columns::catalog(&self.project);
        let column = columns::find(&catalog, &id)
            .filter(|c| c.editor != Editor::ReadOnly)
            .or_else(|| columns::find(&catalog, "activity_name"))
            .cloned();
        if let Some(column) = column {
            self.begin_edit(i, &column);
        }
    }

    /// Type-to-edit: a printable key on the focused cell of the selected row
    /// opens that cell's editor holding the typed text. Cells that cannot be
    /// typed into (read-only, choice lists) leave the key alone.
    pub(crate) fn type_to_edit(&mut self, ctx: &egui::Context) {
        let (Some(i), None) = (self.selected, &self.edit_cell) else {
            return;
        };
        let typed: String = ctx.input(|input| {
            input
                .events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Text(text) => Some(text.as_str()),
                    _ => None,
                })
                .flat_map(str::chars)
                .filter(|c| !c.is_control())
                .collect()
        });
        if typed.trim().is_empty() {
            return;
        }
        let id = self
            .grid
            .focus_column
            .clone()
            .unwrap_or_else(|| "activity_name".into());
        let catalog = columns::catalog(&self.project);
        let Some(column) = columns::find(&catalog, &id)
            .filter(|c| matches!(c.editor, Editor::Text | Editor::Number | Editor::Date))
            .cloned()
        else {
            return;
        };
        self.begin_edit(i, &column);
        if self.edit_cell.is_some() {
            self.edit_text = typed;
            ctx.input_mut(|input| {
                input.events.retain(|e| !matches!(e, egui::Event::Text(_)));
            });
        }
    }

    pub(crate) fn begin_edit(&mut self, i: usize, column: &Column) {
        if let Err(e) = self.cell_editable(i, column) {
            self.status = e;
            return;
        }
        let ix = self.grid_index();
        let v = columns::value(&self.project, &ix, column, i);
        // A check box cell ticks or clears in place, one undo step.
        if column.source == Source::Field(Field::AutoComputeActuals) {
            let text = if v == Value::Flag(true) { "No" } else { "Yes" };
            self.apply_cell(i, &column.id, text);
            return;
        }
        self.edit_text = match (&v, column.kind) {
            (Value::Percent(p), _) => records::format_number(*p),
            (Value::Date { minute, .. }, _) => {
                // The user's date format, without the actual marker.
                format::date(self.project.model.origin_serial, Some(*minute))
            }
            _ => columns::display(&self.project, Some(i), &v),
        };
        self.edit_cell = Some((i, column.id.clone()));
        self.grid.focus_column = Some(column.id.clone());
        self.show_focus_column();
        self.edit_focus = true;
    }

    /// Apply an in-place edit finished during the row pass.
    pub(crate) fn commit_grid_edit(&mut self) {
        if let Some((i, column, text)) = self.grid.pending_edit.take() {
            if !self.apply_cell(i, &column, &text) {
                // Refused: the editor stays open with the typed text, so
                // it can be corrected rather than typed again.
                self.edit_cell = Some((i, column));
                self.edit_text = text;
                self.edit_focus = true;
                self.grid.cells.after_commit = None;
            }
        }
        if let Some(step) = self.grid.cells.after_commit.take() {
            self.move_focus_column(step);
        }
    }
}
