//! OpenXER's own default color theme.
//!
//! Layout, density and control types follow the scheduler this program
//! replaces, but the default colors are OpenXER's: a navy and light blue
//! family (the same navy and light blue the README screenshot uses), steel and
//! slate greys, one teal for actual work and one red for the critical path.
//! Operating system colors (dialog face, selection blue, black text) are not
//! part of this theme. Users can change every color the theme supplies
//! through Group and Sort, Bars, Chart Font and Colors, Table Font and Row
//! and the Activity Network options; stored settings are never rewritten
//! except by `legacy`, which upgrades settings that still hold the colors
//! earlier builds used as defaults.
use eframe::egui::Color32;

pub mod legacy;

pub const NAVY: u32 = 0x1F3A5F;
pub const LIGHT_BLUE: u32 = 0xD6E4F2;
/// Remaining and normal work.
pub const BLUE: u32 = 0x4A86C5;
/// Border and shadow tone of the blue.
pub const BLUE_DARK: u32 = 0x2C5A8A;
/// Actual work.
pub const TEAL: u32 = 0x1B7F8C;
pub const TEAL_DARK: u32 = 0x0F4E57;
/// Level of effort bars.
pub const LOE_REMAINING: u32 = 0x8FB0D0;
pub const LOE_ACTUAL: u32 = 0x3E5C7E;
/// Baseline bars, primary first.
pub const BASELINE: u32 = 0x9FB0C2;
pub const BASELINE_LIGHT: u32 = 0xC9D3DE;
pub const BASELINE_PALE: u32 = 0xE4E9EF;
/// Critical path and overallocation.
pub const CRITICAL: u32 = 0xD32F2F;
pub const CRITICAL_DARK: u32 = 0x7A1F1F;
pub const CRITICAL_LIGHT: u32 = 0xE57373;
/// Warnings: negative float, overtime, the focused cell.
pub const AMBER: u32 = 0xF2A33A;
pub const AMBER_DARK: u32 = 0x8A5A10;
/// Float bars and other secondary lines.
pub const SLATE: u32 = 0x5C6B7C;
/// Dashed sight lines.
pub const SIGHT_LINE: u32 = 0xC5CED8;
/// Weekend and holiday shading.
pub const NONWORK: u32 = 0xEBF0F5;
/// Data date line.
pub const DATA_DATE: u32 = 0x0D47A1;
/// Progress line.
pub const PROGRESS: u32 = 0xE65100;
/// Pale blue cell fill for empty time-phased cells.
pub const CELL: u32 = 0xE9EEF3;
/// Vertical lines between day columns of spreadsheets.
pub const DAY_LINE: u32 = 0x8A98A8;
/// Light amber wash behind highlighted rows.
pub const HIGHLIGHT: u32 = 0xFFF1C2;
/// Text of a disabled control in dialogs and popups: plainly grey, still
/// 4.5:1 on the dialog face and on white.
pub const DISABLED_TEXT: u32 = 0x6D6D6D;
/// Secondary live information (a path, a detail line) on dialogs: quieter
/// than the ink, darker than disabled text.
pub const SECONDARY_TEXT: u32 = 0x505050;
/// Validation messages and other error text on the dialog face.
pub const ERROR_TEXT: u32 = 0xC62828;
/// Warnings on the dialog face: an orange dark enough to read as text.
pub const WARNING_TEXT: u32 = 0xA84C00;
/// The dialog face and the popup body, for the contrast checks.
#[cfg(test)]
const FACE: u32 = 0xF0F0F0;
#[cfg(test)]
const POPUP: u32 = 0xF9F9F9;

/// Stacked Histogram series, one per resource or role: the blue, teal,
/// amber and slate families of this theme plus a green, a plum and a
/// brown, ordered so neighbors in a stack stay apart. Red stays reserved
/// for overallocation and the critical path.
pub const STACK: [u32; 10] = [
    BLUE,
    AMBER,
    TEAL,
    0x7A6FA8,
    0x6E9F4B,
    NAVY,
    0xC98B5A,
    LOE_REMAINING,
    0x3E6A6E,
    0xB58FB8,
];
/// The "Other" series that sums the rest.
pub const STACK_OTHER: u32 = BASELINE_LIGHT;

/// The same color darkened to `percent` of its brightness, for an outline.
pub const fn shade(hex: u32, percent: u32) -> u32 {
    let r = ((hex >> 16) & 0xFF) * percent / 100;
    let g = ((hex >> 8) & 0xFF) * percent / 100;
    let b = (hex & 0xFF) * percent / 100;
    (r << 16) | (g << 8) | b
}

pub const fn color(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const fn bytes(hex: u32) -> [u8; 3] {
    [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8]
}

/// Band levels 1 to 17: background, text and font size in points. Light and
/// dark backgrounds alternate so neighbors stay apart, and every pair meets
/// the WCAG AA contrast of 4.5 to 1. Text is 12, 11 and 9 pt bold on the
/// first three levels and 8 pt below, so the top bands stand taller than
/// activity rows (`grid::layout::band_row_height`: 24, 22, then 18 px).
pub const BANDS: [(u32, u32, f32); 17] = [
    (NAVY, 0xFFFFFF, 12.),
    (LIGHT_BLUE, NAVY, 11.),
    (0x35608F, 0xFFFFFF, 9.),
    (0xBCD2E8, NAVY, 8.),
    (0x2C6E7F, 0xFFFFFF, 8.),
    (0xC6DCE0, NAVY, 8.),
    (0x4A5F7A, 0xFFFFFF, 8.),
    (0xA9BFD6, NAVY, 8.),
    (0x285B8A, 0xFFFFFF, 8.),
    (0xE3EAF2, NAVY, 8.),
    (0x3E6A6E, 0xFFFFFF, 8.),
    (0xB6C9D3, NAVY, 8.),
    (0x566A86, 0xFFFFFF, 8.),
    (0xCED9E6, NAVY, 8.),
    (0x2F4A6B, 0xFFFFFF, 8.),
    (0x9FB4C9, NAVY, 8.),
    (0x55708C, 0xFFFFFF, 8.),
];

/// Background and text of a report's group row at `level` (0 is the top
/// level; deeper levels share the fourth style): the first band levels.
pub fn report_band(level: u8) -> (u32, u32) {
    let (background, text, _) = BANDS[usize::from(level).min(3)];
    (background, text)
}

/// WCAG contrast ratio of two colors.
#[cfg(test)]
pub fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let lum = |c: [u8; 3]| {
        let f = |v: u8| {
            let v = f64::from(v) / 255.;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2])
    };
    let (x, y) = (lum(a), lum(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_band_meets_aa_contrast() {
        for (level, &(background, text, _)) in BANDS.iter().enumerate() {
            let ratio = contrast(bytes(background), bytes(text));
            assert!(ratio >= 4.5, "band {} contrast {ratio:.2}", level + 1);
        }
    }

    #[test]
    fn dialog_text_colors_meet_aa_contrast() {
        for (name, text) in [
            ("disabled", DISABLED_TEXT),
            ("secondary", SECONDARY_TEXT),
            ("error", ERROR_TEXT),
            ("warning", WARNING_TEXT),
            ("critical", CRITICAL),
        ] {
            for (ground, background) in [("white", 0xFFFFFF), ("popup", POPUP), ("face", FACE)] {
                // The critical red is drawn as text only on white rows.
                if name == "critical" && ground != "white" {
                    continue;
                }
                let ratio = contrast(bytes(background), bytes(text));
                assert!(ratio >= 4.5, "{name} on {ground}: {ratio:.2}");
            }
        }
        // Amber and the light critical red are fills and outlines, never
        // text: neither reaches 4.5:1 on white.
        assert!(contrast(bytes(0xFFFFFF), bytes(AMBER)) < 4.5);
        assert!(contrast(bytes(0xFFFFFF), bytes(CRITICAL_LIGHT)) < 4.5);
    }

    #[test]
    fn neighboring_bands_alternate_light_and_dark() {
        let lightness = |hex: u32| {
            let c = bytes(hex);
            299 * u32::from(c[0]) + 587 * u32::from(c[1]) + 114 * u32::from(c[2])
        };
        for pair in BANDS.windows(2) {
            let (a, b) = (lightness(pair[0].0), lightness(pair[1].0));
            assert!(
                a.abs_diff(b) > 40_000,
                "{:06X} next to {:06X}",
                pair[0].0,
                pair[1].0
            );
        }
    }

    #[test]
    fn first_levels_are_the_navy_and_light_blue_pair() {
        assert_eq!(BANDS[0], (0x1F3A5F, 0xFFFFFF, 12.));
        assert_eq!(BANDS[1], (0xD6E4F2, 0x1F3A5F, 11.));
        assert_eq!(BANDS[2].2, 9.);
        assert!(BANDS.iter().skip(3).all(|b| b.2 == 8.));
    }
}
