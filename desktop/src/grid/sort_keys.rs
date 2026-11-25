//! The one sort-key table: a Field Name and Sort Order grid on the kit list,
//! one row per key, the selected row's two cells as drop-downs. The Sort
//! dialog of Group and Sort and the leveling priorities of Level Resources
//! both draw it, so the two look and behave alike.
use super::cell_combo::cell_combo;
use crate::dialog_kit::{self as kit, Canvas, Col};
use eframe::egui;

/// A key the table edits: a field and its direction.
pub(crate) trait SortRow {
    fn field(&self) -> &str;
    fn set_field(&mut self, key: &str);
    fn descending(&self) -> bool;
    fn set_descending(&mut self, descending: bool);
}

impl SortRow for super::layout::SortKey {
    fn field(&self) -> &str {
        &self.column
    }
    fn set_field(&mut self, key: &str) {
        self.column = key.to_string();
    }
    fn descending(&self) -> bool {
        self.descending
    }
    fn set_descending(&mut self, descending: bool) {
        self.descending = descending;
    }
}

/// The two directions, in the order the Sort Order drop-down lists them.
const ORDERS: [&str; 2] = ["Ascending", "Descending"];

/// Draw the table in `rect` (client coordinates) with columns `widths`
/// (Field Name, Sort Order). `choices` are the fields (stored key, title);
/// a key not among them shows `{key} (not supported)`. A click selects a
/// row; the selected row's cells pick from their lists. Actions:
/// `{salt}_row_{n}` per row, `{salt}_field` and `{salt}_order` for the
/// selected row's cells, `{salt}_field_item_{n}` and `{salt}_order_item_{n}`
/// for the open list's items.
pub(crate) fn table<R: SortRow>(
    c: &mut Canvas,
    salt: &str,
    rect: (f32, f32, f32, f32),
    widths: [f32; 2],
    choices: &[(String, String)],
    rows: &mut [R],
    selected: &mut Option<usize>,
) {
    let titles: Vec<String> = choices.iter().map(|(_, t)| t.clone()).collect();
    let orders: Vec<String> = ORDERS.iter().map(|o| o.to_string()).collect();
    let current = selected.filter(|s| *s < rows.len());
    let body = c.r(rect.0, rect.1 + 19., rect.2, rect.3 - 20.);
    let mut pick = None;
    let out = kit::list(
        c,
        &format!("{salt}_keys"),
        rect,
        None,
        &[
            Col {
                title: "Field Name",
                width: widths[0],
            },
            Col {
                title: "Sort Order",
                width: widths[1],
            },
        ],
        rows.len(),
        current,
        None,
        &mut |c, n, col, cell, sel| {
            let row = &mut rows[n];
            let inner = egui::Rect::from_min_max(cell.min, cell.max - egui::vec2(1., 1.));
            if col == 0 {
                c.act(&format!("{salt}_row_{n}"), cell);
            }
            let (key, text, items) = if col == 0 {
                let known = choices.iter().find(|(k, _)| k == row.field());
                let text = match known {
                    Some((_, title)) => title.clone(),
                    None => format!("{} (not supported)", row.field()),
                };
                (format!("{salt}_field"), text, &titles)
            } else {
                let text = ORDERS[usize::from(row.descending())].to_string();
                (format!("{salt}_order"), text, &orders)
            };
            if !sel {
                kit::cell_text(c, cell, &text, false, 3.);
                return;
            }
            let r = cell_combo(c, &key, inner, body, &text, items, true, true, true);
            if let Some(k) = r.picked {
                if col == 0 {
                    row.set_field(&choices[k].0);
                } else {
                    row.set_descending(k == 1);
                }
            }
            if r.clicked {
                pick = Some(n);
            }
        },
    );
    if let Some(n) = out.clicked.or(pick) {
        *selected = Some(n);
    }
}
