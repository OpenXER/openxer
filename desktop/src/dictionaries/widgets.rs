//! Pieces of the dictionary dialogs' flow layout that sit on the details
//! grid (`details::grid`), which the dialog kit's list does not cover: the
//! `Display:` bar over the grid, a tree cell and the code value color
//! swatch. Buttons, headings, tabs and message boxes are the kit's.
use crate::chrome::{self, pal, Icon, INK};
use eframe::egui::{self, Align2, Color32, Pos2, Response, Sense, Stroke, Ui, Vec2};

/// The 18 px `Display: <name>` bar over a grid, with its small down arrow.
pub fn display_bar(ui: &mut Ui, width: f32, caption: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 18.), Sense::click());
    let p = ui.painter();
    chrome::fill(p, rect, pal::FACE);
    chrome::hline(p, rect.left(), rect.right(), rect.top(), pal::WHITE);
    chrome::vline(p, rect.left(), rect.top(), rect.bottom(), pal::WHITE);
    chrome::hline(
        p,
        rect.left(),
        rect.right(),
        rect.bottom() - 1.,
        pal::SHADOW,
    );
    chrome::vline(p, rect.right() - 1., rect.top(), rect.bottom(), pal::SHADOW);
    let c = Pos2::new(rect.left() + 10., rect.center().y);
    p.add(egui::Shape::line(
        vec![
            c + Vec2::new(-4., -2.),
            c + Vec2::new(0., 2.),
            c + Vec2::new(4., -2.),
        ],
        Stroke::new(1_f32, INK),
    ));
    p.text(
        Pos2::new(rect.left() + 19., rect.center().y),
        Align2::LEFT_CENTER,
        format!("Display: {caption}"),
        chrome::table_font(),
        INK,
    );
    response
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
