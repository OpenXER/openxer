//! The tree list of the enterprise dialogs, on the fixed-layout [`Canvas`]
//! of `dialog_kit`: [`kit::list`] with an icon and a 14 px indent per level
//! in the first column, check box cells and in-place editing. Every other
//! control of these dialogs is the kit's own.
use crate::chrome::{self, Icon, INK};
use crate::dialog_kit::{self as kit, Col};
use crate::schedule_dialogs::{dialog_font, Canvas};
use eframe::egui::{self, Color32, Key, Pos2, Rect, Sense, Vec2};

/// An in-place edit of one grid cell: row ID, column, text, take focus.
#[derive(Clone, Debug, PartialEq)]
pub struct CellEdit {
    pub id: i64,
    pub column: usize,
    pub text: String,
    pub focus: bool,
}

/// A row of a [`grid`].
#[derive(Clone, Debug, Default)]
pub struct RowView {
    pub id: i64,
    pub depth: usize,
    pub icon: Option<Icon>,
    pub cells: Vec<String>,
    /// Columns a click on the selected row edits in place.
    pub editable: Vec<bool>,
    /// Columns drawn as a check box: Some(checked).
    pub checks: Vec<Option<bool>>,
    pub bold: bool,
}

#[derive(Default)]
pub struct GridOut {
    pub clicked: Option<usize>,
    pub double: Option<usize>,
    /// (row, column) of a click.
    pub cell: Option<(usize, usize)>,
    /// An in-place edit that ended: (row ID, column, text).
    pub committed: Option<(i64, usize, String)>,
    /// A check box clicked: (row, column).
    pub toggled: Option<(usize, usize)>,
    /// A right-click on a row: the row and where the pointer was.
    pub secondary: Option<(usize, Pos2)>,
}

/// A grid in a lowered frame under a `Display:` bar: column 0 indents 14 px
/// per level after the row's icon; a click on a cell of the selected row
/// starts an in-place edit when the column allows it; Enter or a click
/// elsewhere commits, Escape cancels.
#[allow(clippy::too_many_arguments)]
pub fn grid(
    c: &mut Canvas,
    key: &str,
    rect: (f32, f32, f32, f32),
    display: Option<&str>,
    cols: &[(&str, f32)],
    rows: &[RowView],
    selected: Option<usize>,
    edit: &mut Option<CellEdit>,
    enabled: bool,
) -> GridOut {
    let mut out = GridOut::default();
    let columns: Vec<Col> = cols
        .iter()
        .map(|(title, width)| Col {
            title,
            width: *width,
        })
        .collect();
    let escape = c.ui.input(|i| i.key_pressed(Key::Escape));
    let mut cell_rects: Vec<(usize, usize, Rect)> = vec![];
    let mut committed = None;
    let mut toggled = None;
    let key_owned = key.to_string();
    let display = display.map(|d| format!("Display: {d}"));
    let list = kit::list(
        c,
        key,
        rect,
        display.as_deref(),
        &columns,
        rows.len(),
        selected,
        None,
        &mut |c, r, col, cell, sel| {
            let row = &rows[r];
            cell_rects.push((r, col, cell));
            c.act(&format!("{key_owned}_cell_{r}_{col}"), cell);
            let mut x_pad = 3.;
            if col == 0 {
                x_pad += 14. * row.depth as f32;
                if let Some(icon) = row.icon {
                    chrome::paint_icon_at(
                        c.ui.painter(),
                        icon,
                        Pos2::new(cell.left() + x_pad, cell.top() + 1.),
                        true,
                    );
                    x_pad += 19.;
                }
            }
            if let Some(Some(on)) = row.checks.get(col) {
                let x = (cell.center().x - 6.5).floor();
                let local = Pos2::new(x, cell.top() + 2.) - c.origin.to_vec2();
                kit::check_glyph(c, local.x, local.y, *on, enabled);
                if enabled {
                    let hit = c.ui.interact(
                        Rect::from_min_size(Pos2::new(x, cell.top() + 2.), Vec2::splat(13.)),
                        c.id(&key_owned).with(("check", r, col)),
                        Sense::click(),
                    );
                    if hit.clicked() {
                        toggled = Some((r, col));
                    }
                }
                return;
            }
            match edit {
                Some(e) if e.id == row.id && e.column == col && enabled => {
                    let inner = Rect::from_min_max(
                        Pos2::new(cell.left() + x_pad - 1., cell.top() + 2.),
                        cell.max - Vec2::new(2., 2.),
                    );
                    c.ui.painter().rect_filled(inner, 0., Color32::WHITE);
                    let response = c.ui.put(
                        inner,
                        egui::TextEdit::singleline(&mut e.text)
                            .id(c.id(&key_owned).with("cell_edit"))
                            .frame(false)
                            .font(dialog_font())
                            .text_color(INK)
                            .margin(egui::Margin::ZERO)
                            .desired_width(inner.width()),
                    );
                    crate::dialog_keys::note_keeps_enter(&response);
                    if std::mem::take(&mut e.focus) {
                        response.request_focus();
                    }
                    if response.lost_focus() {
                        if !escape {
                            committed = Some((e.id, e.column, e.text.clone()));
                        }
                        *edit = None;
                    }
                }
                _ => {
                    let shown = row.cells.get(col).map_or("", String::as_str);
                    if row.bold && !sel {
                        c.ui.painter()
                            .with_clip_rect(cell.shrink2(Vec2::new(1., 0.)))
                            .text(
                                Pos2::new(cell.left() + x_pad, cell.center().y),
                                egui::Align2::LEFT_CENTER,
                                shown,
                                chrome::bold_font(),
                                INK,
                            );
                    } else {
                        kit::cell_text(c, cell, shown, sel, x_pad);
                    }
                }
            }
        },
    );
    out.committed = committed;
    out.toggled = toggled;
    if !enabled {
        return out;
    }
    out.clicked = list.clicked;
    out.double = list.double;
    // A right-click is not a click of the list; the caller selects the row
    // and shows its menu. The pointer must be over this window's cell.
    if c.ui
        .input(|i| i.pointer.button_clicked(egui::PointerButton::Secondary))
    {
        if let Some(pos) = c.ui.input(|i| i.pointer.interact_pos()) {
            out.secondary = cell_rects
                .iter()
                .find(|(_, _, cell)| cell.contains(pos) && c.ui.rect_contains_pointer(*cell))
                .map(|(row, _, _)| (*row, pos));
        }
    }
    if let Some(r) = list.clicked {
        let pos = c.ui.input(|i| i.pointer.interact_pos());
        let column = pos.and_then(|p| {
            cell_rects
                .iter()
                .find(|(row, _, rect)| *row == r && rect.contains(p))
                .map(|(_, col, _)| *col)
        });
        if let Some(col) = column {
            out.cell = Some((r, col));
            let row = &rows[r];
            let toggled_here = out.toggled == Some((r, col));
            if selected == Some(r)
                && list.double.is_none()
                && !toggled_here
                && row.editable.get(col).copied().unwrap_or(false)
                && edit
                    .as_ref()
                    .is_none_or(|e| e.id != row.id || e.column != col)
            {
                *edit = Some(CellEdit {
                    id: row.id,
                    column: col,
                    text: row.cells.get(col).cloned().unwrap_or_default(),
                    focus: true,
                });
            }
        }
    }
    out
}
