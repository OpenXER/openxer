//! Upgrade of stored settings that still hold the colors earlier builds used
//! as defaults.
//!
//! The working layout, the Gantt settings and the pane options are written to
//! the workspace as soon as the program starts, so a workspace that never had
//! a color changed still holds the old defaults. Each color that equals the
//! old default is replaced by the current theme default; every color that
//! differs was chosen by the user and stays exactly as stored. Saved named
//! layouts and imported layout files are never rewritten.
use crate::gantt::settings::{
    bars_with, default_bars, BarDef, BarPalette, ChartOptions, GanttSettings, Rgb,
};
use crate::grid::layout::{bands_from, default_bands, GroupSort};
use serde_json::Value;

/// Band levels 1 to 17 of earlier builds: `(background, text, font size)`.
const BANDS: [(u32, u32, f32); 17] = [
    (0x0000FF, 0xFFFF00, 12.),
    (0x80FF80, 0x000000, 11.),
    (0xFFFF00, 0x0000FF, 9.),
    (0x0000FF, 0xFFFFFF, 8.),
    (0xFF0000, 0xFFFFFF, 8.),
    (0x80FFFF, 0x000000, 8.),
    (0xFF80FF, 0x000000, 8.),
    (0xFFFF80, 0x000000, 8.),
    (0x000000, 0xFFFFFF, 8.),
    (0xC0C0C0, 0xFFFFFF, 8.),
    (0x008000, 0xFFFFFF, 8.),
    (0x0000A0, 0xFFFFFF, 8.),
    (0x804000, 0xFFFFFF, 8.),
    (0x800080, 0xFFFFFF, 8.),
    (0xFF8040, 0xFFFFFF, 8.),
    (0x8080C0, 0xFFFFFF, 8.),
    (0x808040, 0xFFFFFF, 8.),
];

const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb(r, g, b)
}

/// The default bar colors of earlier builds.
fn bar_palette() -> BarPalette {
    let (blue, red, yellow, white, black) = (
        rgb(0, 0, 255),
        rgb(255, 0, 0),
        rgb(255, 255, 0),
        rgb(255, 255, 255),
        rgb(0, 0, 0),
    );
    BarPalette {
        remaining_loe: rgb(0, 0x80, 0),
        actual_loe: rgb(0, 0, 0x80),
        baseline_project: yellow,
        baseline_primary: yellow,
        baseline_secondary: white,
        baseline_tertiary: white,
        actual: blue,
        remaining: rgb(0x80, 0xff, 0x80),
        critical: red,
        constraint: black,
        baseline_milestone: yellow,
        milestone: black,
        percent: black,
        summary: black,
        float: black,
        negative_float: yellow,
    }
}

fn swap(value: &mut Rgb, old: Rgb, new: Rgb) {
    if *value == old {
        *value = new;
    }
}

/// Band styles: a color or a font size equal to the old default for its
/// level becomes the current default for that level.
pub fn upgrade_bands(group_sort: &mut GroupSort) {
    let old = bands_from(&BANDS);
    let new = default_bands();
    for ((band, old), new) in group_sort.bands.iter_mut().zip(&old).zip(&new) {
        if band.background == old.background {
            band.background = new.background;
        }
        if band.text == old.text {
            band.text = new.text;
        }
        if band.font_size == old.font_size && band.row_height == old.row_height {
            band.font_size = new.font_size;
            band.row_height = new.row_height;
        }
    }
}

fn upgrade_bar(bar: &mut BarDef, old: &BarDef, new: &BarDef) {
    swap(&mut bar.middle.color, old.middle.color, new.middle.color);
    swap(&mut bar.start.color, old.start.color, new.start.color);
    swap(&mut bar.end.color, old.end.color, new.end.color);
}

fn upgrade_options(options: &mut ChartOptions) {
    let sight = rgb(0xC0, 0xC0, 0xC0);
    let now = ChartOptions::default();
    swap(
        &mut options.relationship_colors.critical,
        rgb(255, 0, 0),
        now.relationship_colors.critical,
    );
    swap(
        &mut options.data_date.color,
        rgb(0, 0, 255),
        now.data_date.color,
    );
    swap(
        &mut options.progress_line.color,
        rgb(255, 0, 0),
        now.progress_line.color,
    );
    swap(
        &mut options.nonwork.color,
        rgb(0xF1, 0xF0, 0xF0),
        now.nonwork.color,
    );
    let lines = &mut options.sight_lines;
    swap(
        &mut lines.horizontal_major.color,
        sight,
        now.sight_lines.horizontal_major.color,
    );
    swap(
        &mut lines.horizontal_minor.color,
        sight,
        now.sight_lines.horizontal_minor.color,
    );
    swap(
        &mut lines.vertical_major.color,
        sight,
        now.sight_lines.vertical_major.color,
    );
    swap(
        &mut lines.vertical_minor.color,
        sight,
        now.sight_lines.vertical_minor.color,
    );
}

/// Bars and chart options: colors equal to the old defaults become the
/// current defaults; bars are matched by name.
pub fn upgrade_gantt(settings: &mut GanttSettings) {
    let old = bars_with(&bar_palette());
    let new = default_bars();
    for bar in &mut settings.bars {
        let found = old.iter().zip(&new).find(|(o, _)| o.name == bar.name);
        if let Some((old, new)) = found {
            upgrade_bar(bar, old, new);
        }
    }
    upgrade_options(&mut settings.options);
}

/// Pane options: the Activity Network box fills. A fill equal to an old
/// default is dropped, so the current default applies.
pub fn upgrade_panes(panes: &mut Value) {
    let Some(network) = panes.get_mut("network").and_then(Value::as_object_mut) else {
        return;
    };
    for (key, old) in [("normal_fill", "#FFFF00"), ("critical_fill", "#FF0000")] {
        if network.get(key).and_then(Value::as_str) == Some(old) {
            network.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_default_bands_become_the_current_defaults() {
        let mut group_sort = GroupSort {
            bands: bands_from(&BANDS),
            ..GroupSort::default()
        };
        upgrade_bands(&mut group_sort);
        assert_eq!(group_sort.bands, default_bands());
    }

    #[test]
    fn chosen_band_colors_stay() {
        let mut group_sort = GroupSort {
            bands: bands_from(&BANDS),
            ..GroupSort::default()
        };
        group_sort.bands[0].background = [10, 20, 30];
        group_sort.bands[1].text = [1, 2, 3];
        group_sort.bands[2].font_size = 10.;
        upgrade_bands(&mut group_sort);
        let now = default_bands();
        assert_eq!(group_sort.bands[0].background, [10, 20, 30]);
        assert_eq!(group_sort.bands[0].text, now[0].text);
        assert_eq!(group_sort.bands[1].text, [1, 2, 3]);
        assert_eq!(group_sort.bands[1].background, now[1].background);
        assert_eq!(group_sort.bands[2].font_size, 10.);
        assert_eq!(group_sort.bands[2].background, now[2].background);
        assert_eq!(group_sort.bands[3], now[3]);
    }

    #[test]
    fn old_default_bars_and_options_become_the_current_defaults() {
        let mut settings = GanttSettings {
            bars: bars_with(&bar_palette()),
            ..GanttSettings::default()
        };
        // The old options: only the colors differ from the current ones.
        let now = ChartOptions::default();
        let o = &mut settings.options;
        o.data_date.color = rgb(0, 0, 255);
        o.progress_line.color = rgb(255, 0, 0);
        o.relationship_colors.critical = rgb(255, 0, 0);
        o.nonwork.color = rgb(0xF1, 0xF0, 0xF0);
        o.sight_lines.horizontal_minor.color = rgb(0xC0, 0xC0, 0xC0);
        // Toggles a user made do not hold the upgrade back.
        settings.bars[0].display = false;
        upgrade_gantt(&mut settings);
        assert!(!settings.bars[0].display);
        settings.bars[0].display = true;
        assert_eq!(settings.bars, default_bars());
        assert_eq!(settings.options, now);
    }

    #[test]
    fn chosen_bar_colors_stay() {
        let mut settings = GanttSettings {
            bars: bars_with(&bar_palette()),
            ..GanttSettings::default()
        };
        let remaining = settings
            .bars
            .iter_mut()
            .find(|b| b.name == "Remaining Work")
            .unwrap();
        remaining.middle.color = rgb(1, 2, 3);
        settings.options.data_date.color = rgb(9, 9, 9);
        upgrade_gantt(&mut settings);
        let remaining = settings
            .bars
            .iter()
            .find(|b| b.name == "Remaining Work")
            .unwrap();
        assert_eq!(remaining.middle.color, rgb(1, 2, 3));
        assert_eq!(settings.options.data_date.color, rgb(9, 9, 9));
        let critical = settings
            .bars
            .iter()
            .find(|b| b.name == "Critical Remaining Work")
            .unwrap();
        assert_eq!(critical.middle.color, BarPalette::theme().critical);
    }

    #[test]
    fn network_fills_are_dropped_only_when_they_are_the_old_defaults() {
        let mut panes = serde_json::json!({"network": {
            "normal_fill": "#FFFF00", "critical_fill": "#00FF00", "font_size": 9.0}});
        upgrade_panes(&mut panes);
        assert!(panes["network"].get("normal_fill").is_none());
        assert_eq!(panes["network"]["critical_fill"], "#00FF00");
        assert_eq!(panes["network"]["font_size"], 9.0);
    }
}
