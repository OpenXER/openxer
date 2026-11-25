//! One app row of the AI Agents setup lists: the app's name in bold, a
//! state mark with its state, grey detail lines, and at the right the one
//! thing to do with an optional second button beneath. Connect AI Agents
//! and Chat's "Choose how to connect" list both draw their rows with
//! [`paint`]; there is no other copy.
use crate::chrome::INK;
use crate::dialog_kit::{self as kit, Canvas};
use eframe::egui::{self, Align2, FontId, Stroke, Vec2};

/// The mark before a row's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mark {
    /// A teal tick: connected, ready.
    Ready,
    /// An amber warning triangle: it needs a look.
    Attention,
    /// A grey ring: not set up.
    Off,
}

/// A row's button: action key, caption, enabled.
pub(crate) struct Button<'a> {
    pub key: &'a str,
    pub label: &'a str,
    pub enabled: bool,
}

pub(crate) struct Row<'a> {
    pub name: &'a str,
    pub mark: Mark,
    pub state: &'a str,
    /// Grey state text (an app that is not there).
    pub state_dim: bool,
    /// The detail lines, already fitted or wrapped to [`text_width`].
    pub detail: &'a [String],
    pub primary: Option<Button<'a>>,
    /// Beneath the first button.
    pub secondary: Option<Button<'a>>,
}

/// The height of a row with `lines` detail lines: 62 with one.
pub(crate) fn height(lines: usize) -> f32 {
    47. + 15. * lines.max(1) as f32
}

/// The room the detail lines have in a row `w` wide.
pub(crate) fn text_width(w: f32) -> f32 {
    w - kit::BUTTON.x - 10. - 10. - 10.
}

/// Paints `row` at (x, y), `w` wide; the key of the button clicked.
pub(crate) fn paint(c: &mut Canvas, x: f32, y: f32, w: f32, row: &Row) -> Option<String> {
    let mut clicked = None;
    c.bold(x + 10., y + 5., 16., row.name);
    mark(c, x + 10., y + 25., row.mark);
    if row.state_dim {
        kit::secondary_text(c, x + 28., y + 25., row.state);
    } else {
        kit::text(c, x + 28., y + 25., row.state, true);
    }
    for (i, line) in row.detail.iter().enumerate() {
        kit::secondary_text(c, x + 10., y + 43. + 15. * i as f32, line);
    }
    let bx = x + w - kit::BUTTON.x - 10.;
    for (b, by) in [(&row.primary, y + 5.), (&row.secondary, y + 33.)] {
        if let Some(b) = b {
            if kit::button(
                c,
                b.key,
                bx,
                by,
                kit::BUTTON.x,
                kit::BUTTON.y,
                b.label,
                b.enabled,
            ) {
                clicked = Some(b.key.to_string());
            }
        }
    }
    clicked
}

/// The mark at (x, y): a teal tick, an amber warning triangle or a grey
/// ring.
pub(crate) fn mark(c: &Canvas, x: f32, y: f32, mark: Mark) {
    let p = c.ui.painter();
    let center = c.origin + Vec2::new(x + 6., y + 7.);
    match mark {
        Mark::Ready => {
            let teal = Stroke::new(2_f32, crate::theme::color(crate::theme::TEAL));
            let a = center + Vec2::new(-5., 0.);
            let b = center + Vec2::new(-1.5, 3.5);
            let e = center + Vec2::new(5., -4.);
            p.line_segment([a, b], teal);
            p.line_segment([b, e], teal);
        }
        Mark::Attention => {
            let top = center + Vec2::new(0., -6.);
            let points = vec![
                top,
                center + Vec2::new(6.5, 5.),
                center + Vec2::new(-6.5, 5.),
            ];
            p.add(egui::Shape::convex_polygon(
                points,
                crate::theme::color(crate::theme::AMBER),
                Stroke::new(1_f32, crate::theme::color(crate::theme::AMBER_DARK)),
            ));
            p.text(
                center + Vec2::new(0., 0.5),
                Align2::CENTER_CENTER,
                "!",
                FontId::new(9., egui::FontFamily::Name("Arial".into())),
                INK,
            );
        }
        Mark::Off => {
            p.circle_stroke(center, 4.5, Stroke::new(1_f32, kit::SHADOW));
        }
    }
}
