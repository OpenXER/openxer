//! The in-grid drop-down editor of the table dialogs (Group By, Group
//! Interval, filter Parameter, Is and Value cells): plain cell text until
//! the cell has focus, then a sunken combo filling the cell with a 17 px
//! drop button and a dotted focus rectangle, as the reference grids draw it.
//! The list opens below the cell and scrolls when it is long.
use super::*;
use crate::schedule_dialogs::Canvas;

const FACE: Color32 = Color32::from_rgb(0xF0, 0xF0, 0xF0);
const SHADOW: Color32 = Color32::from_rgb(0xA0, 0xA0, 0xA0);
const DARK: Color32 = Color32::from_rgb(0x69, 0x69, 0x69);
const ITEM: f32 = 16.;
/// Rows shown before the list scrolls.
const VISIBLE: usize = 16;

/// What happened to a cell combo this frame.
#[derive(Default)]
pub(crate) struct CellCombo {
    /// The cell was clicked (select its row).
    pub clicked: bool,
    /// The item picked from the open list.
    pub picked: Option<usize>,
}

/// A dotted 1 px rectangle (every other pixel), the focus mark.
pub(crate) fn dotted(p: &egui::Painter, r: Rect, color: Color32) {
    let mut x = r.left();
    while x < r.right() {
        p.rect_filled(
            Rect::from_min_size(Pos2::new(x, r.top()), Vec2::splat(1.)),
            0.,
            color,
        );
        p.rect_filled(
            Rect::from_min_size(Pos2::new(x, r.bottom() - 1.), Vec2::splat(1.)),
            0.,
            color,
        );
        x += 2.;
    }
    let mut y = r.top();
    while y < r.bottom() {
        p.rect_filled(
            Rect::from_min_size(Pos2::new(r.left(), y), Vec2::splat(1.)),
            0.,
            color,
        );
        p.rect_filled(
            Rect::from_min_size(Pos2::new(r.right() - 1., y), Vec2::splat(1.)),
            0.,
            color,
        );
        y += 2.;
    }
}

/// Draw one grid cell as a drop-down editor. `cell` is the cell's absolute
/// rectangle (inside its grid lines), `clip` the grid body. `focused` draws
/// the open-editor look; `selected_row` paints plain text white on the
/// selection. Actions: `{key}` for the cell, `{key}_item_{n}` for list rows.
#[allow(clippy::too_many_arguments)]
pub(crate) fn cell_combo(
    c: &mut Canvas,
    key: &str,
    cell: Rect,
    clip: Rect,
    text: &str,
    items: &[String],
    focused: bool,
    selected_row: bool,
    enabled: bool,
) -> CellCombo {
    let mut out = CellCombo::default();
    let id = c.id(key);
    let hit_rect = cell.intersect(clip);
    if hit_rect.width() <= 0. || hit_rect.height() <= 0. {
        return out;
    }
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = c.ui.interact(hit_rect, id, sense);
    c.act(key, hit_rect);
    let p = c.ui.painter().with_clip_rect(clip);
    let font = crate::schedule_dialogs::dialog_font();
    let mut open = c.ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    if response.clicked() {
        out.clicked = true;
        // The first click focuses the cell; a click on a focused cell
        // opens or closes its list.
        if focused {
            open = !open;
        }
    }
    if focused && enabled {
        // Sunken white field with the drop button at the right.
        p.rect_filled(cell, 0., Color32::WHITE);
        let (l, t, r, b) = (cell.left(), cell.top(), cell.right(), cell.bottom());
        let line = |a: Pos2, z: Pos2, color: Color32| {
            p.rect_filled(Rect::from_min_max(a, z), 0., color);
        };
        line(Pos2::new(l, t), Pos2::new(r, t + 1.), SHADOW);
        line(Pos2::new(l, t), Pos2::new(l + 1., b), SHADOW);
        line(Pos2::new(l, b - 1.), Pos2::new(r, b), Color32::WHITE);
        line(Pos2::new(r - 1., t), Pos2::new(r, b), Color32::WHITE);
        let button = Rect::from_min_size(
            Pos2::new(r - 18., t + 1.),
            Vec2::new(17., (b - t - 2.).max(4.)),
        );
        p.rect_filled(button, 0., FACE);
        line(
            button.min,
            Pos2::new(button.right(), button.top() + 1.),
            Color32::WHITE,
        );
        line(
            button.min,
            Pos2::new(button.left() + 1., button.bottom()),
            Color32::WHITE,
        );
        line(
            Pos2::new(button.left(), button.bottom() - 1.),
            button.max,
            DARK,
        );
        line(
            Pos2::new(button.right() - 1., button.top()),
            button.max,
            DARK,
        );
        let m = button.center();
        p.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(m.x - 3.5, m.y - 1.5),
                Pos2::new(m.x + 3.5, m.y - 1.5),
                Pos2::new(m.x, m.y + 2.),
            ],
            INK,
            Stroke::NONE,
        ));
        let field = Rect::from_min_max(
            Pos2::new(l + 2., t + 2.),
            Pos2::new(button.left() - 1., b - 2.),
        );
        dotted(&p, field, INK);
        p.with_clip_rect(field.intersect(clip)).text(
            Pos2::new(field.left() + 2., field.center().y),
            Align2::LEFT_CENTER,
            text,
            font.clone(),
            INK,
        );
    } else {
        let color = if !enabled {
            Color32::from_gray(0xA0)
        } else if selected_row {
            Color32::WHITE
        } else {
            INK
        };
        p.with_clip_rect(cell.intersect(clip)).text(
            Pos2::new(cell.left() + 3., cell.center().y),
            Align2::LEFT_CENTER,
            text,
            font.clone(),
            color,
        );
    }
    if !(focused && enabled) {
        open = false;
    }
    if open && !items.is_empty() {
        let (picked, elsewhere) = popup(c, key, cell, items, text, &response);
        out.picked = picked;
        if picked.is_some() || elsewhere {
            open = false;
        }
    }
    c.ui.ctx().data_mut(|d| d.insert_temp(id, open));
    out
}

/// Open or close a cell combo's list (a second click on a selected cell
/// opens its editor with the list showing).
pub(crate) fn set_open(c: &Canvas, key: &str, open: bool) {
    let id = c.id(key);
    c.ui.ctx().data_mut(|d| d.insert_temp(id, open));
}

/// The drop-down list under `anchor`: returns the picked item and whether a
/// click landed outside the list and the control.
fn popup(
    c: &mut Canvas,
    key: &str,
    anchor: Rect,
    items: &[String],
    current: &str,
    control: &egui::Response,
) -> (Option<usize>, bool) {
    let id = c.id(key);
    let font = crate::schedule_dialogs::dialog_font();
    let rows = items.len().min(VISIBLE);
    let width = anchor.width().max(140.);
    let mut item_rects = vec![];
    let mut picked = None;
    let area = egui::Area::new(id.with("list"))
        .order(egui::Order::Tooltip)
        .fixed_pos(anchor.left_bottom())
        .show(c.ui.ctx(), |ui| {
            let frame = egui::Frame::NONE
                .fill(Color32::WHITE)
                .stroke(Stroke::new(1_f32, INK))
                .inner_margin(1.);
            frame.show(ui, |ui| {
                ui.set_width(width - 2.);
                egui::ScrollArea::vertical()
                    .max_height(ITEM * rows as f32)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        for (n, label) in items.iter().enumerate() {
                            let (r, hit) =
                                ui.allocate_exact_size(Vec2::new(width - 2., ITEM), Sense::click());
                            let lit = hit.hovered() || label == current;
                            if lit {
                                ui.painter().rect_filled(r, 0., SELECT);
                            }
                            ui.painter().text(
                                r.left_center() + Vec2::new(3., 0.),
                                Align2::LEFT_CENTER,
                                label,
                                font.clone(),
                                if lit { Color32::WHITE } else { INK },
                            );
                            item_rects.push((n, r));
                            if hit.clicked() {
                                picked = Some(n);
                            }
                        }
                    });
            });
        });
    for (n, r) in item_rects {
        c.act(&format!("{key}_item_{n}"), r);
    }
    let elsewhere = c.ui.ctx().input(|i| i.pointer.any_pressed())
        && !area.response.contains_pointer()
        && !control.contains_pointer();
    (picked, elsewhere)
}

/// A combo drop button on its own (inside an edit), opening a list of
/// `items`; returns the picked item. Action `{key}` is the button.
pub(crate) fn drop_button(
    c: &mut Canvas,
    key: &str,
    button: Rect,
    clip: Rect,
    list_anchor: Rect,
    items: &[String],
) -> Option<usize> {
    let id = c.id(key);
    let hit = button.intersect(clip);
    if hit.width() <= 0. || hit.height() <= 0. {
        return None;
    }
    let response = c.ui.interact(hit, id, Sense::click());
    c.act(key, hit);
    let p = c.ui.painter().with_clip_rect(clip);
    p.rect_filled(button, 0., FACE);
    let line = |a: Pos2, z: Pos2, color: Color32| {
        p.rect_filled(Rect::from_min_max(a, z), 0., color);
    };
    line(
        button.min,
        Pos2::new(button.right(), button.top() + 1.),
        Color32::WHITE,
    );
    line(
        button.min,
        Pos2::new(button.left() + 1., button.bottom()),
        Color32::WHITE,
    );
    line(
        Pos2::new(button.left(), button.bottom() - 1.),
        button.max,
        DARK,
    );
    line(
        Pos2::new(button.right() - 1., button.top()),
        button.max,
        DARK,
    );
    let m = button.center();
    p.add(egui::Shape::convex_polygon(
        vec![
            Pos2::new(m.x - 3.5, m.y - 1.5),
            Pos2::new(m.x + 3.5, m.y - 1.5),
            Pos2::new(m.x, m.y + 2.),
        ],
        INK,
        Stroke::NONE,
    ));
    let mut open = c.ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    if response.clicked() {
        open = !open;
    }
    let mut picked = None;
    if open {
        let (p, elsewhere) = popup(c, key, list_anchor, items, "", &response);
        picked = p;
        if picked.is_some() || elsewhere {
            open = false;
        }
    }
    c.ui.ctx().data_mut(|d| d.insert_temp(id, open));
    picked
}
