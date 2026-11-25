//! The one band look: fill, caption font and placement, and the totals'
//! font, by band level. The activity table (`grid::table`) and the view
//! tables (`views::grid`: Projects, WBS, Resources, Resource Assignments,
//! the record views, Reports) both paint their grouping bands through
//! these, so a band level looks the same in every window.
use super::layout::{band_row_height, default_bands, BandStyle};
use crate::chrome::{self, INK, SELECT};
use eframe::egui::{self, Color32, FontId, Pos2, Rect};
use std::sync::OnceLock;

/// Cap line of a band caption below the band's top, whatever its font.
pub const CAP_TOP: f32 = 4.;
/// Galley top of 8 pt total text below the band's top.
pub const TEXT_TOP: f32 = 3.;

/// The default style of band `level` (from 0): the theme's levels
/// (`crate::theme::BANDS`), the same ones a new activity layout starts with.
pub fn level_style(level: usize) -> BandStyle {
    static BANDS: OnceLock<Vec<BandStyle>> = OnceLock::new();
    let bands = BANDS.get_or_init(default_bands);
    bands[level.min(bands.len() - 1)].clone()
}

/// Pitch of a band row in `style`: its stored height, never less than its
/// font needs.
pub fn pitch(style: &BandStyle) -> f32 {
    style.row_height.max(band_row_height(style.font_size))
}

/// Fill of a band: its level's background, selection blue when selected.
pub fn fill(style: &BandStyle, selected: bool) -> Color32 {
    if selected {
        SELECT
    } else {
        crate::theme::color(rgb(style.background))
    }
}

/// Text of a band, caption and totals: its level's color, white when
/// selected.
pub fn ink(style: &BandStyle, selected: bool) -> Color32 {
    if selected {
        Color32::WHITE
    } else {
        crate::theme::color(rgb(style.text))
    }
}

fn rgb([r, g, b]: [u8; 3]) -> u32 {
    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

/// The caption font of a band in `style`.
pub fn caption_font(style: &BandStyle) -> FontId {
    let family = if style.bold {
        chrome::bold_font().family
    } else {
        egui::FontFamily::Proportional
    };
    FontId::new(style.font_size * 4. / 3., family)
}

/// Totals on a band are in the row font, never the caption's, so dates and
/// numbers fit their columns.
pub fn total_font() -> FontId {
    FontId::proportional(chrome::FONT_SIZE)
}

/// Distance from a galley's top to the cap line of `font`.
pub fn cap_offset(p: &egui::Painter, font: &FontId) -> f32 {
    let g = p.layout_no_wrap("H".into(), font.clone(), INK);
    g.rows
        .first()
        .and_then(|row| {
            row.glyphs
                .first()
                .map(|glyph| row.pos.y + glyph.pos.y + glyph.uv_rect.offset.y)
        })
        .unwrap_or(0.)
}

/// A band's caption at `x`, its cap line `CAP_TOP` below the band's `top`,
/// clipped to `clip`.
pub fn caption(
    p: &egui::Painter,
    clip: Rect,
    x: f32,
    top: f32,
    style: &BandStyle,
    label: &str,
    selected: bool,
) {
    let font = caption_font(style);
    let color = ink(style, selected);
    let y = (top + CAP_TOP - cap_offset(p, &font)).round();
    let g = p.layout_no_wrap(label.to_string(), font, color);
    p.with_clip_rect(clip.intersect(p.clip_rect()))
        .galley(Pos2::new(x, y), g, color);
}

/// Galley top of a band total, the row font's text `TEXT_TOP` below the
/// band's `top`.
pub fn total_top(top: f32) -> f32 {
    top + TEXT_TOP
}

#[cfg(test)]
mod tests {
    /// The band look is written once, here: a second band font or band
    /// color constant elsewhere fails.
    #[test]
    fn one_band_painter() {
        fn visit(dir: &std::path::Path, hits: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    visit(&path, hits);
                    continue;
                }
                if path.extension().is_none_or(|e| e != "rs") || path.ends_with("grid/band.rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                for (n, line) in text.lines().enumerate() {
                    let code = line.split("//").next().unwrap_or("");
                    if code.contains("fn band_font(") || code.contains("fn band_caption_font(") {
                        hits.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
                    }
                }
            }
        }
        let mut hits = vec![];
        visit(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut hits,
        );
        assert!(
            hits.is_empty(),
            "paint bands through grid::band:\n{}",
            hits.join("\n")
        );
    }
}
