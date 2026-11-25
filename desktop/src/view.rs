use super::*;
use timescale::{TimeScale, Unit, ZOOM_PRESETS};

const VIEW_KEY: &str = "activities_view_v1";
pub const MAX_SCALE: f64 = 13_440.;

#[derive(Clone, PartialEq)]
pub struct ViewSettings {
    pub upper: Unit,
    pub lower: Unit,
    pub automatic_units: bool,
    pub row_height: f32,
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self {
            upper: Unit::Month,
            lower: Unit::Week,
            automatic_units: true,
            row_height: gantt_geometry::ROW_HEIGHT,
        }
    }
}

impl App {
    pub(super) fn update_timescale_units(&mut self) {
        if self.view.automatic_units {
            (self.view.upper, self.view.lower) =
                timescale::bands_for_scale(self.pixels_per_day as f64 * 7.);
        }
    }

    pub(super) fn zoom_at(&mut self, scale: f64, anchor: f32) {
        let mut axis = TimeScale::new(
            self.timeline_origin as f64,
            self.pixels_per_day as f64 * 7.,
            self.gantt_left,
        );
        axis.zoom_at(scale.clamp(self.minimum_scale(), MAX_SCALE), anchor);
        self.timeline_origin = axis.first_minute.round() as i64;
        self.pixels_per_day = (axis.pixels_per_week / 7.) as f32;
        self.update_timescale_units();
    }

    pub(super) fn minimum_scale(&self) -> f64 {
        if self.view.automatic_units {
            1.
        } else {
            self.view.lower.preset_scale(2.)
        }
    }

    pub(super) fn view_json(&self) -> String {
        serde_json::json!({
            "upper": self.view.upper.label(), "lower": self.view.lower.label(),
            "automatic_units": self.view.automatic_units, "row_height": self.view.row_height,
            "pixels_per_day": self.pixels_per_day,
            "grid_fraction": self.grid_fraction,
            "show_details": self.show_details
        })
        .to_string()
    }

    pub(super) fn restore_view(&mut self) {
        if let Ok(Some(json)) = self.store.preference(VIEW_KEY) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) {
                apply_view_value(
                    &value,
                    &mut self.view,
                    &mut self.pixels_per_day,
                    &mut self.grid_fraction,
                );
                if let Some(saved) = value["show_details"].as_bool() {
                    self.show_details = saved;
                }
            }
        }
        self.view_snapshot = self.view_json();
        self.pixels_per_day = self.pixels_per_day.max((self.minimum_scale() / 7.) as f32);
    }

    pub(super) fn save_view(&mut self) {
        let json = self.view_json();
        match self.store.set_preference(VIEW_KEY, &json) {
            Ok(()) => {
                self.view_snapshot = json;
                self.view_save_at = None;
            }
            Err(error) => self.error(format!("Cannot save view preferences: {error}")),
        }
    }

    pub(super) fn persist_view_when_idle(&mut self, ctx: &egui::Context) {
        self.persist_gantt_when_idle();
        let json = self.view_json();
        let now = ctx.input(|i| i.time);
        if self.view_snapshot != json {
            self.view_snapshot = json;
            self.view_save_at = Some(now + 0.4);
        }
        if let Some(deadline) = self.view_save_at {
            if now >= deadline {
                self.save_view();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(400));
            }
        }
    }

    pub(super) fn zoom_menu(&mut self, ui: &mut egui::Ui) {
        // Menu items like the rest of View > Zoom (Zoom To Best Fit is above).
        for preset in ZOOM_PRESETS {
            let action = format!("zoom_preset_{}", preset.unit.label());
            if self.item(ui, &action, chrome::Item::new(preset.label))
                && !self.focused_zoom_to(preset.pixels_per_week)
            {
                self.view.automatic_units = true;
                self.gantt.timescale.interval = gantt::settings::DateInterval::Automatic;
                self.zoom_at(
                    preset.pixels_per_week,
                    self.gantt_left + self.chart_width / 2.,
                );
                self.view.upper = preset.major;
                self.view.lower = preset.minor;
            }
        }
    }

    pub(super) fn view_dialog(&mut self, ctx: &egui::Context) {
        self.gantt_dialogs(ctx);
    }
}

/// Apply a stored view (`view_json`): bands, row height, zoom and the
/// table/chart split. Invalid parts are ignored.
pub fn apply_view_value(
    value: &serde_json::Value,
    view: &mut ViewSettings,
    pixels_per_day: &mut f32,
    grid_fraction: &mut f32,
) {
    let unit = |key: &str| {
        value[key]
            .as_str()
            .and_then(|name| Unit::all().into_iter().find(|unit| unit.label() == name))
    };
    if let (Some(upper), Some(lower)) = (unit("upper"), unit("lower")) {
        if unit_index(upper) <= unit_index(lower) {
            view.upper = upper;
            view.lower = lower;
        }
    }
    if let Some(saved) = value["automatic_units"].as_bool() {
        view.automatic_units = saved;
    }
    for (key, field, min, max) in [
        ("row_height", &mut view.row_height, 17., 48.),
        (
            "pixels_per_day",
            pixels_per_day,
            1. / 7.,
            MAX_SCALE as f32 / 7.,
        ),
        ("grid_fraction", grid_fraction, 0., 1.),
    ] {
        if let Some(saved) = value[key].as_f64().filter(|v| v.is_finite()) {
            *field = (saved as f32).clamp(min, max);
        }
    }
}

fn unit_index(unit: Unit) -> usize {
    Unit::all().iter().position(|u| *u == unit).unwrap_or(0)
}
