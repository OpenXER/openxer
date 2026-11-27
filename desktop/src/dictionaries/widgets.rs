//! Pieces of the dictionary dialogs' flow layout that sit on the details
//! grid (`details::grid`), which the dialog kit's list does not cover: the
//! `Display:` bar over the grid, a tree cell and the code value color
//! swatch. Buttons, headings, tabs and message boxes are the kit's.
use crate::chrome::{self, Icon};
use eframe::egui::{Color32, Response, Sense, Ui, Vec2};

/// The 18 px `Display: <name>` bar over a grid, delegating to `views::sheet::header_button`.
pub fn display_bar(ui: &mut Ui, width: f32, caption: &str) -> Response {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 18.), Sense::hover());
    crate::views::sheet::header_button(ui, "dict_display", rect, &format!("Display: {caption}"))
}

/// A tree cell: indent 14 px per level, the dictionary icon, then the text.
pub fn tree_cell(ui: &mut Ui, depth: usize, icon: Option<Icon>, text: &str) {
    ui.add_space(depth as f32 * 14.);
    if let Some(icon) = icon {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(18., 16.), Sense::hover());
        chrome::paint_icon_at(ui.painter(), icon, rect.min, true);
    }
    crate::details::cell(ui, text);
}

/// A painted color swatch filling the cell.
pub fn swatch(ui: &mut Ui, rgb: Option<Color32>) {
    let r = ui.max_rect().shrink2(Vec2::new(2., 2.));
    if let Some(color) = rgb {
        ui.painter().rect_filled(r, 0., color);
    }
}

pub fn parse_rgb(text: &str) -> Option<Color32> {
    let t = text.trim();
    if t.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(t, 16).ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

pub fn rgb_text(c: [u8; 3]) -> String {
    format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}
