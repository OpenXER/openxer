//! Typing into the Resource Usage Spreadsheet's period cells, and the
//! spreadsheet's cell commands: Fill Down (Ctrl+E), Fill Across (Ctrl+H),
//! Range Copy (Shift+Ctrl+C) and Range Paste (Shift+Ctrl+V). The same code
//! serves the Resource Assignments window's spreadsheet (`SheetHost`): the
//! spreadsheet drawn last holds the cell selection.
//!
//! As the reference was seen to work: a future period cell on an
//! assignment's Remaining Units line takes a typed value. F2 opens the
//! cell's editor; a key typed on the selected cell opens it too, without
//! typing that key. Enter commits with no question, Escape drops the text.
//! The value goes to that period alone through the shared edit
//! (`ProjectData::set_period_units`), one undo step; the assignment becomes
//! Manual. At a week or month timescale the value spreads evenly over the
//! period's working time inside the assignment's remaining dates (only day
//! cells were seen typed into).
//!
//! Cells that take no value: resource and role lines, other projects'
//! lines, fields other than remaining units (cumulative ones too), costs,
//! the cumulative display, and periods before the data date or outside the
//! assignment's remaining work. Fill and Range Paste skip such cells; each
//! command is one undo step.
use super::time_fields::Series;
use super::usage::ResRow;
use super::usage_fields as fields;
use super::*;
use std::ops::RangeInclusive;
use std::rc::Rc;

/// The spreadsheet's cell selection and its in-place editor.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SheetCells {
    /// The cell a selection started on and the one it ends on: (line,
    /// period start). Shift+click moves the end.
    pub anchor: Option<(usize, i64)>,
    pub cursor: Option<(usize, i64)>,
    /// The cells have the keyboard: one was clicked last.
    pub focused: bool,
    /// The editor's text while a cell is typed into.
    pub editing: Option<String>,
    /// The editor takes the keyboard on its next frame.
    pub focus_editor: bool,
    /// Range Copy's values, one row per line.
    pub copied: Option<Vec<Vec<f64>>>,
}

/// Which spreadsheet the cell selection and the cell commands work on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SheetHost {
    /// The Resource Usage Spreadsheet (bottom or top of the Activities
    /// window).
    #[default]
    Usage,
    /// The Resource Assignments window's spreadsheet.
    Assignments,
}

/// The selected lines and periods (indexes), first to last.
pub(crate) type Range = (RangeInclusive<usize>, RangeInclusive<usize>);

impl SheetCells {
    /// The selection over `periods`, when its cells are on show.
    pub fn range(&self, periods: &[(i64, i64)]) -> Option<Range> {
        let (line, start) = self.cursor?;
        let (first, from) = self.anchor.unwrap_or((line, start));
        let index = |s: i64| periods.iter().position(|p| p.0 == s);
        let (a, b) = (index(from)?, index(start)?);
        Some((first.min(line)..=first.max(line), a.min(b)..=a.max(b)))
    }
}

/// Units typed into a cell: blank is zero.
fn parse_cell(text: &str) -> Option<f64> {
    if text.trim().is_empty() {
        return Some(0.);
    }
    crate::grid::columns::parse_units(text)
        .ok()
        .filter(|v| *v >= 0.)
}

impl App {
    /// The spreadsheet that holds the cell selection is on show: the
    /// Resource Usage Spreadsheet on top or at the bottom, or the Resource
    /// Assignments window's.
    pub(crate) fn resource_sheet_shown(&self) -> bool {
        match self.panes.sheet_host {
            SheetHost::Usage => {
                self.panes.top == TopView::ResourceUsageSheet
                    || self.panes.bottom == BottomView::ResourceUsageSheet
            }
            SheetHost::Assignments => {
                self.views.main == crate::views::MainView::Assignments
                    && self.views.assignments.show_sheet
            }
        }
    }

    /// Spreadsheet `host` is being drawn: the cell selection is its from
    /// now on (another spreadsheet's selection is dropped; Range Copy's
    /// values are kept).
    pub(crate) fn claim_sheet(&mut self, host: SheetHost) {
        if self.panes.sheet_host != host {
            self.panes.sheet_host = host;
            let copied = self.panes.sheet_cells.copied.take();
            self.panes.sheet_cells = SheetCells {
                copied,
                ..Default::default()
            };
        }
    }

    /// The time-phased fields of the spreadsheet that holds the selection.
    pub(crate) fn sheet_time_fields(&self) -> Vec<time_fields::TimeField> {
        match self.panes.sheet_host {
            SheetHost::Usage => self.resource_time_fields(),
            SheetHost::Assignments => self.views.assignments.time_fields(),
        }
    }

    /// Units or costs, and the cumulative display, of that spreadsheet.
    fn sheet_display(&self) -> (bool, bool) {
        match self.panes.sheet_host {
            SheetHost::Usage => (self.panes.usage.costs, self.panes.usage.cumulative),
            SheetHost::Assignments => (false, false),
        }
    }

    /// The spreadsheet's lines as drawn last.
    pub(crate) fn sheet_lines(&self) -> Option<Rc<Vec<ResRow>>> {
        if self.panes.sheet_host == SheetHost::Assignments {
            return self.views.assignments.sheet_lines.clone();
        }
        let afields = fields::fields_of(
            &self.project,
            fields::Which::Assignment,
            &self.panes.usage.assignment_fields,
        );
        let periods = self.panes.sheet_periods.clone();
        self.selection_sheet_rows(&afields, &periods)
    }

    /// The TASKRSRC row a cell of `line` sets for `period`, or why the cell
    /// takes no value.
    pub(crate) fn sheet_cell_target(
        &self,
        line: &ResRow,
        period: (i64, i64),
    ) -> Result<usize, String> {
        let (costs, cumulative) = self.sheet_display();
        if costs {
            return Err("Costs follow the units; show units to type a value".into());
        }
        if cumulative {
            return Err("Turn the cumulative display off to type a value".into());
        }
        let field = self.sheet_time_fields().get(line.field).copied();
        if !field.is_some_and(|f| f.series == Series::Remaining && !f.cumulative) {
            return Err("Only remaining units take a typed value".into());
        }
        let row = line
            .assignment
            .ok_or("Values are typed on an assignment's line")?;
        self.project.check_period_units(row, period)?;
        Ok(row)
    }

    /// A click on cell (`line`, `period`): it becomes the selected cell, or
    /// with Shift the selection's end; a double-click opens its editor. An
    /// open editor commits first.
    pub(crate) fn sheet_cell_click(
        &mut self,
        line: usize,
        period: (i64, i64),
        shift: bool,
        double: bool,
        time: f64,
    ) {
        if self.panes.sheet_cells.editing.is_some() {
            self.sheet_commit_edit(time);
        }
        let c = &mut self.panes.sheet_cells;
        c.cursor = Some((line, period.0));
        if !shift || c.anchor.is_none() {
            c.anchor = Some((line, period.0));
        }
        c.focused = true;
        if double {
            self.sheet_begin_edit();
        }
    }

    /// F2, a double-click or a typed key: the selected cell's editor opens
    /// with its value, or the status line says why it takes none.
    pub(crate) fn sheet_begin_edit(&mut self) {
        let Some((line, start)) = self.panes.sheet_cells.cursor else {
            return;
        };
        let periods = self.panes.sheet_periods.clone();
        let Some(k) = periods.iter().position(|p| p.0 == start) else {
            return;
        };
        let Some(lines) = self.sheet_lines() else {
            return;
        };
        let Some(l) = lines.get(line) else {
            return;
        };
        match self.sheet_cell_target(l, periods[k]) {
            Ok(_) => {
                let v = l.values.get(k).copied().unwrap_or(0.);
                let text = if v.abs() < 0.005 {
                    String::new()
                } else {
                    crate::format::units(v)
                };
                self.panes.sheet_cells.anchor = Some((line, start));
                self.panes.sheet_cells.editing = Some(text);
                self.panes.sheet_cells.focus_editor = true;
            }
            Err(e) => self.status = e,
        }
    }

    /// The selected cell: framed while the cells have the keyboard, or its
    /// editor while one is open (Enter or a click away commits, Escape
    /// drops the text). `cell` is where it is drawn; `None` when scrolled
    /// out of view, which closes an editor without a change.
    pub(crate) fn sheet_cell_editor(
        &mut self,
        ui: &mut egui::Ui,
        cell: Option<Rect>,
        font: &FontId,
        time: f64,
    ) {
        let Some(cell) = cell else {
            self.panes.sheet_cells.editing = None;
            return;
        };
        let Some(mut text) = self.panes.sheet_cells.editing.clone() else {
            if self.panes.sheet_cells.focused {
                let stroke = egui::Stroke::new(1_f32, crate::chrome::INK);
                let frame = cell.shrink(0.5);
                ui.painter()
                    .rect_stroke(frame, 0., stroke, egui::StrokeKind::Inside);
            }
            return;
        };
        ui.painter().rect_filled(cell, 0., Color32::WHITE);
        let edit = egui::TextEdit::singleline(&mut text)
            .id(egui::Id::new("usage_sheet_cell_editor"))
            .frame(false)
            .margin(egui::Margin::symmetric(1, 0))
            .font(font.clone())
            .horizontal_align(egui::Align::Max);
        let r = ui.put(cell.shrink2(Vec2::new(2., 1.)), edit);
        self.action("usage_sheet_cell_editor", &r);
        if std::mem::take(&mut self.panes.sheet_cells.focus_editor) {
            // The value is selected, so what is typed replaces it.
            let mut state = egui::TextEdit::load_state(ui.ctx(), r.id).unwrap_or_default();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(text.chars().count()),
                )));
            state.store(ui.ctx(), r.id);
            r.request_focus();
            self.panes.sheet_cells.editing = Some(text);
            return;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.panes.sheet_cells.editing = None;
            return;
        }
        self.panes.sheet_cells.editing = Some(text);
        if r.lost_focus() {
            self.sheet_commit_edit(time);
        }
    }

    /// Enter in the editor: the typed units go to the selected cell.
    pub(crate) fn sheet_commit_edit(&mut self, time: f64) {
        let Some(text) = self.panes.sheet_cells.editing.take() else {
            return;
        };
        let Some((line, start)) = self.panes.sheet_cells.cursor else {
            return;
        };
        let periods = &self.panes.sheet_periods;
        let Some(k) = periods.iter().position(|p| p.0 == start) else {
            return;
        };
        match parse_cell(&text) {
            Some(v) => {
                self.set_sheet_cells(&[((line, k), v)], time);
            }
            None => self.status = format!("{} is not a number of units", text.trim()),
        }
    }

    /// Set each of `cells` ((line, period index), units) that takes a value,
    /// as one undo step; the number set. A refused edit leaves the project
    /// as it was.
    pub(crate) fn set_sheet_cells(&mut self, cells: &[((usize, usize), f64)], time: f64) -> usize {
        let Some(lines) = self.sheet_lines() else {
            return 0;
        };
        let periods = self.panes.sheet_periods.clone();
        let mut refusal = None;
        let mut targets = vec![];
        for &((l, k), v) in cells {
            let (Some(line), Some(&period)) = (lines.get(l), periods.get(k)) else {
                continue;
            };
            match self.sheet_cell_target(line, period) {
                Ok(row) => targets.push((row, period, v)),
                Err(e) => {
                    if refusal.is_none() {
                        refusal = Some(e);
                    }
                }
            }
        }
        if targets.is_empty() {
            if let Some(e) = refusal {
                self.status = e;
            }
            return 0;
        }
        let before = self.project.clone();
        let revision = self.records_revision;
        for &(row, period, v) in &targets {
            if let Err(e) = self.project.set_period_units(row, period, v) {
                self.project = before;
                self.status = e;
                return 0;
            }
        }
        self.edit_step(before, revision, time);
        targets.len()
    }

    /// The selected cells' values, one row per line.
    fn sheet_block(&self) -> Option<(Range, Vec<Vec<f64>>)> {
        let lines = self.sheet_lines()?;
        let range = self.panes.sheet_cells.range(&self.panes.sheet_periods)?;
        let (rows, ks) = range.clone();
        let value = |l: usize, k: usize| lines.get(l).and_then(|r| r.values.get(k)).copied();
        let block: Vec<Vec<f64>> = rows
            .map(|l| ks.clone().map(|k| value(l, k).unwrap_or(0.)).collect())
            .collect();
        Some((range, block))
    }

    /// Whether Fill Down (`down`) or Fill Across has a range to fill: two
    /// lines or two periods selected.
    pub(crate) fn sheet_fill_ready(&self, down: bool) -> bool {
        let range = self.panes.sheet_cells.range(&self.panes.sheet_periods);
        range.is_some_and(|(lines, ks)| {
            if down {
                lines.count() > 1
            } else {
                ks.count() > 1
            }
        })
    }

    /// Range Copy: the selected cells, kept for Range Paste and put on the
    /// clipboard as tab separated numbers.
    pub(crate) fn sheet_range_copy(&mut self, ctx: &egui::Context) {
        let Some((_, block)) = self.sheet_block() else {
            return;
        };
        let text = block
            .iter()
            .map(|row| {
                row.iter()
                    .map(|v| openxer_xer::assignments::quantity_text(*v))
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n");
        ctx.copy_text(text);
        self.panes.sheet_cells.copied = Some(block);
    }

    /// Range Paste: the copied cells from the selection's first cell on;
    /// cells that take no value are skipped.
    pub(crate) fn sheet_range_paste(&mut self, time: f64) {
        let Some(block) = self.panes.sheet_cells.copied.clone() else {
            return;
        };
        let Some((lines, ks)) = self.panes.sheet_cells.range(&self.panes.sheet_periods) else {
            return;
        };
        let (l0, k0) = (*lines.start(), *ks.start());
        let mut cells = vec![];
        for (dl, row) in block.iter().enumerate() {
            for (dk, v) in row.iter().enumerate() {
                cells.push(((l0 + dl, k0 + dk), *v));
            }
        }
        let n = self.set_sheet_cells(&cells, time);
        self.status = format!("Range Paste: {n} cells set");
    }

    /// Fill Down (`down`) or Fill Across: the first line's (or first
    /// period's) values copied over the rest of the selection.
    pub(crate) fn sheet_fill(&mut self, down: bool, time: f64) {
        if !self.sheet_fill_ready(down) {
            return;
        }
        let Some(((lines, ks), block)) = self.sheet_block() else {
            return;
        };
        let mut cells = vec![];
        for (dl, l) in lines.clone().enumerate() {
            for (dk, k) in ks.clone().enumerate() {
                if (down && dl == 0) || (!down && dk == 0) {
                    continue;
                }
                let v = if down { block[0][dk] } else { block[dl][0] };
                cells.push(((l, k), v));
            }
        }
        let n = self.set_sheet_cells(&cells, time);
        let name = if down { "Fill Down" } else { "Fill Across" };
        self.status = format!("{name}: {n} cells set");
    }

    /// The cells' own keys, ahead of the window's: F2 and a typed key open
    /// the editor, Ctrl+E and Ctrl+H fill, Shift+Ctrl+C and Shift+Ctrl+V
    /// copy and paste a range (on Windows these two arrive as copy and
    /// paste events).
    pub(crate) fn sheet_cell_keys(&mut self, ctx: &egui::Context) {
        let c = &self.panes.sheet_cells;
        if !c.focused || c.editing.is_some() || c.cursor.is_none() {
            return;
        }
        if !self.resource_sheet_shown() {
            self.panes.sheet_cells.focused = false;
            return;
        }
        let both = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
        let (copy, paste, down, across, f2, typed) = ctx.input_mut(|i| {
            let shift_ctrl = i.modifiers.command && i.modifiers.shift;
            let mut copy = i.consume_key(both, egui::Key::C);
            let mut paste = i.consume_key(both, egui::Key::V);
            let down = i.consume_key(egui::Modifiers::COMMAND, egui::Key::E);
            let across = i.consume_key(egui::Modifiers::COMMAND, egui::Key::H);
            let f2 = i.consume_key(egui::Modifiers::NONE, egui::Key::F2);
            let mut typed = false;
            i.events.retain(|e| match e {
                egui::Event::Copy if shift_ctrl => {
                    copy = true;
                    false
                }
                egui::Event::Paste(_) if shift_ctrl => {
                    paste = true;
                    false
                }
                egui::Event::Text(_) => {
                    typed = true;
                    false
                }
                _ => true,
            });
            (copy, paste, down, across, f2, typed)
        });
        let time = ctx.input(|i| i.time);
        if copy {
            self.sheet_range_copy(ctx);
        }
        if paste {
            self.sheet_range_paste(time);
        }
        if down {
            self.sheet_fill(true, time);
        }
        if across {
            self.sheet_fill(false, time);
        }
        if f2 || typed {
            self.sheet_begin_edit();
        }
    }
}
