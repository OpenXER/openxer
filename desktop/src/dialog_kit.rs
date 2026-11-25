//! The dialog kit: the one window shell and the one set of controls every
//! OpenXER dialog is built from. Controls are placed at client coordinates
//! on a [`Canvas`] (fixed layout); flow-layout windows ([`KitWindow`]) share
//! the same frame and push button painter.
//!
//! # Rulebook (read before touching a dialog)
//!
//! Do not paint a control yourself. Call the kit function for it; if the kit
//! lacks a control, add it here once and call it from every dialog.
//!
//! | Control | Call |
//! |---|---|
//! | Fixed-layout window, non-modal | [`window`] |
//! | Fixed-layout window, modal | [`modal`] / [`modal_at`] (re-exported by `schedule_dialogs`) |
//! | Flow-layout window | `chrome::dialog_window` (no margin) / `chrome::plain_window` (8 px margin) |
//! | Any other shell need | [`shell`] with a [`Shell`]; never `egui::Window`, `egui::Modal` or a bare `egui::Area` |
//! | Push button | [`button`]; the dialog's default button: [`default_button`] |
//! | Right-hand button column | [`button_column`] (+ [`strip`]); flow windows: `chrome::dialog_with_buttons` |
//! | Bottom button row | [`button_row`] |
//! | Message or confirmation box | [`message_box`] / [`message_box_with_lines`] |
//! | Check box | `Canvas::checkbox`, caption left of the box: [`check_right`] |
//! | Radio button | [`radio`] (mnemonic, multi-line) or `Canvas::radio` |
//! | Edit | `Canvas::edit`; one that reports on focus loss: [`commit_edit`]; several lines: [`text_area`] |
//! | Read-only field | [`field`] |
//! | Combo box | `Canvas::combo`; items drawn instead of named: [`drawn_combo`] |
//! | Spin edit | `Canvas::spin`; of a number value: [`number_spin`] |
//! | Color picker | [`swatch`] (in flow layout: [`color_button`]) |
//! | Section / group box | [`section`], a bar alone: [`heading`] |
//! | List with scroll bar | [`list`]; rows of your own: [`rows_in`] + [`cell_text`] |
//! | Horizontal tabs over a page | [`tabs`] (the views' tab strip, `tab_strip::show`) |
//! | Vertical tab list | [`vertical_tabs`] (with icons: [`vertical_tabs_with_icons`]) + [`page`] |
//! | Text | [`text`], [`mnemonic_text`], [`lines`], [`secondary_text`], [`error_text`] |
//! | Glyphs outside a Canvas | [`paint_check_glyph`], [`paint_radio_glyph`], [`paint_sunken`], [`paint_push_button`] |
//!
//! Standard sizes and order:
//!
//! - Push button [`BUTTON`] 108 x 25. Captions in [`button_font`] (Arial
//!   8 pt), the face turns `HOT_FILL` under the pointer, the default button
//!   carries a black frame and a dotted focus rectangle.
//! - Button column: a [`STRIP_WIDTH`] (112 px) #D0D0D0 strip at the right
//!   edge, buttons at its left edge from y [`COLUMN_TOP`] (6) on a
//!   [`BUTTON_PITCH`] (30 px) pitch, [`GROUP_GAP`] (10 px) more before a
//!   grouped button. Order: OK, Cancel, Apply, others, Help (Help last).
//!   A verb that commits (Schedule, Level, Open, or Apply at the top)
//!   stands in OK's place; Close stands in Cancel's. A dialog whose own
//!   reference shot shows another order declares it with
//!   [`observed_button_column`].
//! - Bottom row: the same buttons left to right, [`ROW_GAP`] (5 px) apart,
//!   right aligned, except Help, which [`button_row`] draws first (at the
//!   left). Callers list the entries in the column order. A dialog whose
//!   own reference shot shows another row order declares it with
//!   [`observed_button_row`].
//! - Text, labels and captions: one font, [`body_font`] (Arial 8 pt,
//!   `chrome::table_font`). Section and heading bar captions are bold
//!   ([`heading_font`], `chrome::bold_font`). Disabled text is
//!   plain #6D6D6D. A read-only field is filled `pal::READONLY` (#F8F8F8).
//! - Palette: `chrome::pal`. The constants below are aliases of it.
//! - Every dialog closes on Escape (the shell reports it as `closed`),
//!   Cancel changes nothing, Enter runs the default button
//!   (`dialog_keys::default_button`: the marked one, else OK, Next, Finish,
//!   else the first of the right-hand column). A message box names its
//!   default (or none, leaving Enter idle). A confirmation takes it from
//!   `kit_window::Question`: the default follows the observed reference
//!   behaviour, even Yes on a delete; where not observed, No. Enter and the
//!   accelerators commit what was typed in a kit edit; Escape drops it.
//!
//! The sweep (`dialog_sweep`) checks standard button sizes, the order and
//! that every dialog is drawn through [`shell`]; dialogs not yet moved onto
//! the kit are listed by name in its `NOT_YET_CONVERTED` and must leave it
//! when they are converted.
use super::*;
use crate::gantt::settings::Rgb;

// The kit's one message box; batch 2 moves `enterprise::ui::message` here.
#[allow(unused_imports)]
pub(crate) use crate::kit_window::{message_box, message_box_with_lines, MessageIcon, Question};

/// Remembers where a dialog's client area was drawn this frame, so the
/// dialog renders can crop it. Costs one map insert per open dialog.
pub(crate) fn note_client(ctx: &egui::Context, layer: egui::LayerId, name: &str, client: Rect) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| {
        d.insert_temp(client_id(name), client);
        let notes = pass_notes(d, pass);
        notes.clients.push((name.to_owned(), client));
        notes.layers.push(layer);
    });
}

/// The layer's dialog frame was painted by the kit's shell ([`frame_shapes`]).
fn note_frame(ctx: &egui::Context, layer: egui::LayerId) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| pass_notes(d, pass).framed.push(layer));
}

fn pass_notes(d: &mut egui::util::IdTypeMap, pass: u64) -> &mut PassNotes {
    let notes = d.get_temp_mut_or_default::<PassNotes>(egui::Id::new("dialog_pass_notes"));
    if notes.pass != pass {
        notes.pass = pass;
        notes.clients.clear();
        notes.layers.clear();
        notes.framed.clear();
        notes.declared.clear();
    }
    notes
}

/// The dialogs drawn in the current pass, for the formatting sweep.
#[derive(Clone, Default)]
struct PassNotes {
    pass: u64,
    clients: Vec<(String, Rect)>,
    layers: Vec<egui::LayerId>,
    framed: Vec<egui::LayerId>,
    /// Button columns (false) and rows (true) drawn in a declared observed
    /// order, by layer.
    declared: Vec<(egui::LayerId, bool, Vec<String>)>,
}

/// Layers of the dialogs drawn in the pass before this one (or in this
/// one, once drawn): what a key handler that runs ahead of the dialogs
/// sees as open.
pub(crate) fn open_dialog_layers(ctx: &egui::Context) -> Vec<egui::LayerId> {
    let now = ctx.cumulative_pass_nr();
    ctx.data(|d| {
        d.get_temp::<PassNotes>(egui::Id::new("dialog_pass_notes"))
            .filter(|n| n.pass + 1 >= now)
            .map(|n| n.layers)
            .unwrap_or_default()
    })
}

/// Names of the dialogs open in the pass before this one (or in this one,
/// once drawn), the topmost first: what F1 asks for help on.
pub(crate) fn open_dialogs(ctx: &egui::Context) -> Vec<String> {
    let now = ctx.cumulative_pass_nr();
    let notes = ctx.data(|d| {
        d.get_temp::<PassNotes>(egui::Id::new("dialog_pass_notes"))
            .filter(|n| n.pass + 1 >= now)
            .unwrap_or_default()
    });
    let order: Vec<egui::LayerId> = ctx.memory(|m| m.layer_ids().collect());
    let mut named: Vec<(usize, usize, String)> = notes
        .clients
        .into_iter()
        .zip(notes.layers)
        .enumerate()
        .map(|(drawn, ((name, _), layer))| {
            let z = order.iter().position(|l| *l == layer).unwrap_or(0);
            (z, drawn, name)
        })
        .collect();
    // Higher in egui's paint order first; on one layer, the later drawn.
    named.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    named.into_iter().map(|(_, _, n)| n).collect()
}

/// Names and client rectangles of the dialogs open in the pass before this
/// one (or in this one, once drawn): where the help window keeps clear of.
pub(crate) fn open_dialog_clients(ctx: &egui::Context) -> Vec<(String, Rect)> {
    let now = ctx.cumulative_pass_nr();
    ctx.data(|d| {
        d.get_temp::<PassNotes>(egui::Id::new("dialog_pass_notes"))
            .filter(|n| n.pass + 1 >= now)
            .map(|n| n.clients)
            .unwrap_or_default()
    })
}

/// Names and client rectangles of the dialogs that noted themselves in the
/// pass being drawn.
#[cfg(test)]
pub(crate) fn drawn_clients(ctx: &egui::Context) -> Vec<(String, Rect)> {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|d| {
        d.get_temp::<PassNotes>(egui::Id::new("dialog_pass_notes"))
            .filter(|n| n.pass == pass)
            .map(|n| n.clients)
            .unwrap_or_default()
    })
}

/// Layers whose frame the kit's shell painted in the pass being drawn.
#[cfg(test)]
pub(crate) fn framed_layers(ctx: &egui::Context) -> Vec<egui::LayerId> {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|d| {
        d.get_temp::<PassNotes>(egui::Id::new("dialog_pass_notes"))
            .filter(|n| n.pass == pass)
            .map(|n| n.framed)
            .unwrap_or_default()
    })
}

fn client_id(name: &str) -> egui::Id {
    egui::Id::new(("dialog_client", name.to_owned()))
}

/// The client rectangle noted for `name`, if the dialog was drawn.
#[cfg(test)]
pub(crate) fn noted_client(ctx: &egui::Context, name: &str) -> Option<Rect> {
    ctx.data(|d| d.get_temp::<Rect>(client_id(name)))
}

// ---------------------------------------------------------- palette, sizes

/// Dialog colors: aliases of `chrome::pal`, the one palette.
pub(crate) const STRIP: Color32 = chrome::pal::DIALOG_STRIP;
pub(crate) const FACE: Color32 = chrome::pal::FACE;
pub(crate) const SHADOW: Color32 = chrome::pal::SHADOW;
pub(crate) const DARK: Color32 = chrome::pal::DARK;
pub(crate) const LIGHT_EDGE: Color32 = chrome::pal::LIGHT;
pub(crate) const READONLY: Color32 = chrome::pal::READONLY;
/// Outer lines of vertical tabs and their page.
pub(crate) const TAB_LINE: Color32 = chrome::pal::TAB_LINE;
pub(crate) const DISABLED: Color32 = chrome::pal::TEXT_DISABLED;

/// The standard push button, 108 x 25.
pub(crate) const BUTTON: Vec2 = chrome::DIALOG_BUTTON;
/// Width of the right-hand button strip; its buttons start at its left edge.
pub(crate) const STRIP_WIDTH: f32 = chrome::DIALOG_STRIP_WIDTH;
/// Top of the first button of a column.
pub(crate) const COLUMN_TOP: f32 = 6.;
/// Distance from one column button's top to the next.
pub(crate) const BUTTON_PITCH: f32 = 30.;
/// Extra space before a grouped button (Apply, Help, a new group).
pub(crate) const GROUP_GAP: f32 = 10.;
/// Space between the buttons of a bottom row.
#[allow(dead_code)] // first callers arrive with the later dialog batches
pub(crate) const ROW_GAP: f32 = 5.;

/// The check box that turns a dialog off for later times (Copy Activity
/// Options, Renumber Activity IDs on a paste, Copy WBS Options).
pub(crate) const SKIP_DIALOG: &str = "Skip this dialog next time";
/// The same check box on a wizard page.
pub(crate) const SKIP_WIZARD: &str = "Skip this wizard next time";

/// Arial 8 pt, the one dialog font: labels, edits, lists, captions.
pub(crate) fn body_font() -> FontId {
    chrome::table_font()
}
/// The dialog body font (kept for the many callers; same as [`body_font`]).
pub(crate) fn dialog_font() -> FontId {
    body_font()
}
/// Push button captions: the body font.
pub(crate) fn button_font() -> FontId {
    body_font()
}
/// Section bar captions: the body face, bold.
pub(crate) fn heading_font() -> FontId {
    chrome::bold_font()
}
fn title_font() -> FontId {
    FontId::new(12., egui::FontFamily::Name("Segoe UI".into()))
}

/// A number as a spin edit shows it: up to two decimals, no trailing zeros.
pub(crate) fn format_number(value: f64) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

// ------------------------------------------------------------ window shell

/// Height of the title bar.
pub(crate) const TITLE_H: f32 = 31.;
const CLOSE_W: f32 = 46.;

/// How a dialog window sits among the others.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Layering {
    /// An ordinary window: child windows opened from it stack above it.
    Window,
    /// Modal: blocks the windows beneath it until closed.
    Modal,
}

/// What [`shell`] needs to know about a fixed-size dialog.
pub(crate) struct Shell<'a> {
    /// Name the client area is noted under (tests, help, renders).
    pub name: &'a str,
    pub id: egui::Id,
    pub title: &'a str,
    /// Client area size, under the title bar.
    pub client: Vec2,
    pub layering: Layering,
    /// Top-left corner to open at (kept on the screen); centred when `None`.
    pub at: Option<Pos2>,
    /// Roll-up, resizing and tool-window behaviour; `Extras::default()` for
    /// an ordinary dialog.
    pub extras: Extras,
}

/// What a shell can do beyond the plain dialog frame. Any window gains these
/// by setting them on its [`Shell`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Extras {
    /// A roll-up button left of the close box (and a double-click on the
    /// title bar) folds the window to its title bar and back.
    pub roll_up: bool,
    /// The bottom-right grip resizes the client area, no smaller than this.
    /// The size is kept with the window's place ([`geometry`]).
    pub min_client: Option<Vec2>,
    /// A tool window that stays open while the user works elsewhere: it
    /// holds the keyboard like a dialog (Escape, Enter, the main window's
    /// accelerators wait) only while it is active, from when it opens or is
    /// clicked until the next click outside it.
    pub tool: bool,
}

/// What [`shell`] reports.
pub(crate) struct ShellOut<R> {
    pub inner: R,
    /// The close box was clicked, or Escape belongs to this dialog.
    pub closed: bool,
    /// The close box rectangle.
    pub close: Rect,
    /// The roll-up button, when the shell has one.
    pub roll: Option<Rect>,
    /// The part of the title bar that drags the window (and, with roll-up,
    /// rolls it on a double-click).
    pub grip: Rect,
    /// The resize grip, when the window can be resized and is not rolled up.
    pub size_grip: Option<Rect>,
    /// The window is rolled up to its title bar.
    pub rolled: bool,
    /// The client area's size this frame.
    pub client: Vec2,
    /// A tool window is active (always true for other windows).
    pub active: bool,
}

/// Where a movable window is, its client size when it can be resized, and
/// whether it is rolled up: what a caller keeps between sessions.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Geometry {
    pub pos: Option<[f32; 2]>,
    pub client: Option<[f32; 2]>,
    pub rolled: bool,
}

fn size_id(id: egui::Id) -> egui::Id {
    id.with("client_size")
}

fn rolled_id(id: egui::Id) -> egui::Id {
    id.with("rolled")
}

/// The geometry of the non-modal window `id` as the shell keeps it.
pub(crate) fn geometry(ctx: &egui::Context, id: egui::Id) -> Geometry {
    ctx.data(|d| Geometry {
        pos: d.get_temp::<Pos2>(id).map(|p| [p.x, p.y]),
        client: d.get_temp::<Vec2>(size_id(id)).map(|v| [v.x, v.y]),
        rolled: d.get_temp::<bool>(rolled_id(id)).unwrap_or(false),
    })
}

/// Puts the non-modal window `id` where `g` says (a stored place, before the
/// window is first shown). Values that are not finite are ignored.
pub(crate) fn set_geometry(ctx: &egui::Context, id: egui::Id, g: Geometry) {
    let finite = |v: [f32; 2]| v.iter().all(|x| x.is_finite());
    ctx.data_mut(|d| {
        match g.pos.filter(|p| finite(*p)) {
            Some(p) => d.insert_temp(id, Pos2::new(p[0], p[1])),
            None => d.remove::<Pos2>(id),
        }
        match g.client.filter(|c| finite(*c) && c[0] > 0. && c[1] > 0.) {
            Some(c) => d.insert_temp(size_id(id), Vec2::new(c[0], c[1])),
            None => d.remove::<Vec2>(size_id(id)),
        }
        d.insert_temp(rolled_id(id), g.rolled);
    });
}

/// Width of the roll-up button in the title bar.
const ROLL_W: f32 = 34.;
/// Side of the resize grip in a window's bottom-right corner.
pub(crate) const SIZE_GRIP: f32 = 14.;

/// The resize grip: three diagonal lines in the bottom-right corner of
/// `grip`.
pub(crate) fn paint_size_grip(p: &egui::Painter, grip: Rect) {
    for n in 1..=3 {
        let d = n as f32 * 4.;
        p.line_segment(
            [grip.max - Vec2::new(d, 2.), grip.max - Vec2::new(2., d)],
            Stroke::new(1_f32, SHADOW),
        );
    }
}

/// The roll-up button's face: a chevron pointing up (roll up) or down (roll
/// down), on a light fill under the pointer.
fn paint_roll(p: &egui::Painter, rect: Rect, rolled: bool, hot: bool) {
    if hot {
        p.rect_filled(rect, 0., chrome::pal::LIGHT);
    }
    let c = rect.center();
    let (tip, wing) = if rolled { (2., -2.) } else { (-2., 2.) };
    let stroke = Stroke::new(1.2_f32, INK);
    p.line_segment([c + Vec2::new(-4., wing), c + Vec2::new(0., tip)], stroke);
    p.line_segment([c + Vec2::new(0., tip), c + Vec2::new(4., wing)], stroke);
}

/// The title bar and the close box of a window whose outer rectangle is
/// `rect`.
pub(crate) fn frame_rects(rect: Rect) -> (Rect, Rect) {
    let bar = Rect::from_min_size(
        rect.min + Vec2::splat(1.),
        Vec2::new(rect.width() - 2., TITLE_H),
    );
    let close = Rect::from_min_max(Pos2::new(bar.right() - CLOSE_W, bar.top()), bar.max);
    (bar, close)
}

/// The one window frame: drop shadow, face, 1 px border, 31 px title bar
/// with the title 12 px in and the close box (red under the pointer).
pub(crate) fn frame_shapes(
    p: &egui::Painter,
    rect: Rect,
    title: &str,
    close_hot: bool,
) -> Vec<egui::Shape> {
    use egui::Shape;
    let (bar, close) = frame_rects(rect);
    let mut shapes = vec![
        Shape::rect_filled(
            rect.translate(Vec2::new(2., 3.)),
            0.,
            Color32::from_black_alpha(40),
        ),
        Shape::rect_filled(rect, 0., FACE),
        Shape::rect_stroke(
            rect.shrink(0.5),
            0.,
            Stroke::new(1_f32, chrome::pal::WINDOW_BORDER),
            egui::StrokeKind::Middle,
        ),
        Shape::rect_filled(bar, 0., chrome::pal::TITLE_BG),
    ];
    let galley = p.layout_no_wrap(title.to_owned(), title_font(), INK);
    let at = Pos2::new(bar.left() + 12., bar.center().y - galley.size().y / 2.);
    shapes.push(Shape::galley(at, galley, INK));
    if close_hot {
        shapes.push(Shape::rect_filled(close, 0., chrome::pal::CLOSE_HOT));
    }
    let ink = if close_hot { Color32::WHITE } else { INK };
    let c = close.center();
    for (a, b) in [
        (Vec2::new(-5., -5.), Vec2::new(5., 5.)),
        (Vec2::new(-5., 5.), Vec2::new(5., -5.)),
    ] {
        shapes.push(Shape::line_segment([c + a, c + b], Stroke::new(1_f32, ink)));
    }
    shapes
}

/// [`frame_shapes`] painted now, and the layer noted as kit-framed.
fn paint_frame(ui: &egui::Ui, rect: Rect, title: &str, close_hot: bool) {
    let shapes = frame_shapes(ui.painter(), rect, title, close_hot);
    ui.painter().extend(shapes);
    note_frame(ui.ctx(), ui.layer_id());
}

/// Notes a frame painted through [`frame_shapes`] by a shell of its own
/// (the flow window).
pub(crate) fn note_framed(ui: &egui::Ui) {
    note_frame(ui.ctx(), ui.layer_id());
}

/// The top-left corner that keeps a box of `size` wholly on `screen` (its
/// top-left on the screen when it is larger).
pub(crate) fn clamp_on_screen(pos: Pos2, size: Vec2, screen: Rect) -> Pos2 {
    let max = (screen.max - size).max(screen.min);
    pos.max(screen.min).min(max)
}

/// The one dialog window shell for a fixed client area: frame, title bar
/// dragging (kept on the screen), the close box, Escape, and a vertical
/// scroll area when the dialog is taller than the screen. `content` draws
/// the client area with its top-left at the given point.
pub(crate) fn shell<R>(
    ctx: &egui::Context,
    s: Shell,
    content: impl FnOnce(&mut egui::Ui, Pos2) -> R,
) -> ShellOut<R> {
    let out = shell_with(ctx, s, |ui, origin, _| content(ui, origin));
    ShellOut {
        inner: out
            .inner
            .expect("a shell without roll-up always draws its content"),
        closed: out.closed,
        close: out.close,
        roll: out.roll,
        grip: out.grip,
        size_grip: out.size_grip,
        rolled: out.rolled,
        client: out.client,
        active: out.active,
    }
}

/// [`shell`] with the [`Extras`]: `content` also gets the client size this
/// frame (a resizable window's current size), and is not called while the
/// window is rolled up.
pub(crate) fn shell_with<R>(
    ctx: &egui::Context,
    s: Shell,
    content: impl FnOnce(&mut egui::Ui, Pos2, Vec2) -> R,
) -> ShellOut<Option<R>> {
    let extras = s.extras;
    let screen = ctx.content_rect();
    let room = (screen.size() - Vec2::new(2., TITLE_H + 2.)).max(Vec2::ZERO);
    let client = match extras.min_client {
        Some(min) => ctx
            .data(|d| d.get_temp::<Vec2>(size_id(s.id)))
            .unwrap_or(s.client)
            .max(min)
            .min(room.max(min)),
        None => s.client,
    };
    let rolled = extras.roll_up
        && ctx
            .data(|d| d.get_temp::<bool>(rolled_id(s.id)))
            .unwrap_or(false);
    let size = if rolled {
        Vec2::new(client.x + 2., TITLE_H + 2.)
    } else {
        client + Vec2::new(2., TITLE_H + 2.)
    };
    let fits = size.y <= screen.height();
    let shown = Vec2::new(size.x, size.y.min(screen.height()));
    let order = match s.layering {
        Layering::Window => egui::Order::Middle,
        Layering::Modal => egui::Order::Foreground,
    };
    let centred = || (s.at.unwrap_or(screen.center() - shown / 2.)).round();
    // Modal dialogs open where they are asked to each time and remember how
    // far they were dragged from there; windows remember where they were.
    let moved_id = s.id.with("moved");
    let pos = if s.layering == Layering::Modal {
        let moved: Vec2 = ctx.data(|d| d.get_temp(moved_id)).unwrap_or_default();
        clamp_on_screen(centred() + moved, shown, screen)
    } else {
        let stored = ctx.data(|d| d.get_temp::<Pos2>(s.id));
        clamp_on_screen(stored.unwrap_or_else(centred), shown, screen)
    };
    // A tool window is active from when it opens or is clicked until the
    // next click outside it (a click in one of its open drop-down lists
    // counts as inside).
    let active = if extras.tool {
        let now = ctx.cumulative_pass_nr();
        let seen_id = s.id.with("tool_seen");
        let active_id = s.id.with("tool_active");
        let (seen, was) = ctx.data(|d| {
            (
                d.get_temp::<u64>(seen_id),
                d.get_temp::<bool>(active_id).unwrap_or(true),
            )
        });
        let opening = seen.is_none_or(|p| p + 1 < now);
        let rect = Rect::from_min_size(pos, shown);
        let press = ctx.input(|i| {
            i.pointer
                .any_pressed()
                .then(|| i.pointer.press_origin())
                .flatten()
        });
        let active = match press {
            _ if opening => true,
            Some(at) => rect.contains(at) || (was && crate::kit_window::combo_open(ctx)),
            None => was,
        };
        ctx.data_mut(|d| {
            d.insert_temp(seen_id, now);
            d.insert_temp(active_id, active);
        });
        active
    } else {
        true
    };
    let escape = active && crate::kit_window::escape_goes_to(ctx, s.id, order);
    let (id, name, title, layering) = (s.id, s.name, s.title, s.layering);
    let draw = move |ui: &mut egui::Ui| {
        let (rect, _) = ui.allocate_exact_size(shown, Sense::hover());
        let (bar, close) = frame_rects(rect);
        let close_response = ui.interact(close, id.with("close"), Sense::click());
        paint_frame(ui, rect, title, close_response.hovered());
        let roll = extras.roll_up.then(|| {
            Rect::from_min_max(
                Pos2::new(close.left() - ROLL_W, bar.top()),
                Pos2::new(close.left(), bar.bottom()),
            )
        });
        let mut toggle = false;
        if let Some(r) = roll {
            let response = ui.interact(r, id.with("roll"), Sense::click());
            paint_roll(ui.painter(), r, rolled, response.hovered());
            let tip = if rolled { "Roll down" } else { "Roll up" };
            toggle |= response.on_hover_text(tip).clicked();
        }
        let grip = Rect::from_min_max(
            bar.min,
            Pos2::new(roll.map_or(close.left(), |r| r.left()), bar.bottom()),
        );
        let sense = if extras.roll_up {
            Sense::click_and_drag()
        } else {
            Sense::drag()
        };
        let drag = ui.interact(grip, id.with("title"), sense);
        if extras.roll_up && drag.double_clicked() {
            toggle = true;
        }
        if toggle {
            ui.ctx().data_mut(|d| d.insert_temp(rolled_id(id), !rolled));
            ui.ctx().request_repaint();
        }
        if drag.dragged() {
            let to = clamp_on_screen(pos + drag.drag_delta(), shown, screen);
            if layering == Layering::Modal {
                ui.ctx()
                    .data_mut(|d| d.insert_temp(moved_id, to - centred()));
            } else {
                ui.ctx().data_mut(|d| d.insert_temp(id, to));
            }
            ui.ctx().request_repaint();
        } else if layering != Layering::Modal && ui.ctx().data(|d| d.get_temp::<Pos2>(id)).is_none()
        {
            ui.ctx().data_mut(|d| d.insert_temp(id, pos));
        }
        let origin = Pos2::new(rect.left() + 1., bar.bottom());
        let mut size_grip = None;
        let inner = if rolled {
            if active {
                note_client(
                    ui.ctx(),
                    ui.layer_id(),
                    name,
                    Rect::from_min_size(origin, Vec2::new(client.x, 0.)),
                );
            }
            None
        } else {
            let body = |ui: &mut egui::Ui, origin: Pos2| {
                if active {
                    note_client(
                        ui.ctx(),
                        ui.layer_id(),
                        name,
                        Rect::from_min_size(origin, client),
                    );
                }
                content(ui, origin, client)
            };
            let inner = if fits {
                body(ui, origin)
            } else {
                let area = Rect::from_min_max(origin, rect.max - Vec2::new(1., 1.));
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
                egui::ScrollArea::vertical()
                    .id_salt((id, "client_scroll"))
                    .max_height(area.height())
                    .auto_shrink([false, false])
                    .show(&mut child, |ui| {
                        let (client_rect, _) = ui.allocate_exact_size(client, Sense::hover());
                        body(ui, client_rect.min)
                    })
                    .inner
            };
            if let Some(min) = extras.min_client {
                let g = Rect::from_min_size(
                    rect.max - Vec2::splat(SIZE_GRIP + 1.),
                    Vec2::splat(SIZE_GRIP),
                );
                let r = ui.interact(g, id.with("size_grip"), Sense::drag());
                if r.hovered() || r.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
                }
                // The size follows the pointer from where the drag began.
                let start_id = id.with("size_grip_start");
                if r.drag_started() {
                    let from = r.interact_pointer_pos().unwrap_or(g.center());
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(start_id, (client, from)));
                }
                let start = ui.ctx().data(|d| d.get_temp::<(Vec2, Pos2)>(start_id));
                if let (true, Some((base, from)), Some(at)) =
                    (r.dragged(), start, r.interact_pointer_pos())
                {
                    let most = (screen.max - rect.min - Vec2::new(2., TITLE_H + 2.)).max(min);
                    let to = (base + (at - from)).max(min).min(most);
                    ui.ctx().data_mut(|d| d.insert_temp(size_id(id), to));
                    ui.ctx().request_repaint();
                }
                paint_size_grip(ui.painter(), g);
                size_grip = Some(g);
            }
            Some(inner)
        };
        ShellOut {
            inner,
            closed: close_response.clicked() || escape,
            close,
            roll,
            grip,
            size_grip,
            rolled,
            client,
            active,
        }
    };
    match layering {
        Layering::Modal => {
            let area = egui::Modal::default_area(id).anchor(Align2::LEFT_TOP, pos - screen.min);
            egui::Modal::new(id)
                .area(area)
                .backdrop_color(Color32::TRANSPARENT)
                .frame(egui::Frame::NONE)
                .show(ctx, draw)
                .inner
        }
        Layering::Window => {
            let mut out = None;
            egui::Window::new(name.to_owned())
                .id(id)
                .title_bar(false)
                .frame(egui::Frame::NONE)
                .resizable(false)
                .collapsible(false)
                .movable(false)
                .order(order)
                .current_pos(pos)
                .show(ctx, |ui| out = Some(draw(ui)));
            out.expect("dialog window content runs every frame")
        }
    }
}

/// [`shell`] with a [`Canvas`] over the client area, optionally pre-filled
/// with `fill`; the close box is recorded first as `{salt}_title_close`.
fn canvas_shell<R>(
    ctx: &egui::Context,
    s: Shell,
    salt: &'static str,
    fill: Option<Color32>,
    content: impl FnOnce(&mut Canvas) -> R,
) -> (R, bool, Vec<(String, Rect)>) {
    let client = s.client;
    let out = shell(ctx, s, |ui, origin| {
        let mut canvas = Canvas {
            ui,
            origin,
            salt,
            actions: vec![],
        };
        if let Some(fill) = fill {
            canvas.fill(0., 0., client.x, client.y, fill);
        }
        let inner = content(&mut canvas);
        (inner, canvas.actions)
    });
    let (inner, mut actions) = out.inner;
    actions.insert(0, (format!("{salt}_title_close"), out.close));
    (inner, out.closed, actions)
}

/// A movable, non-modal dialog window with a fixed client area, drawn on a
/// [`Canvas`]. Child windows opened from it stack above it. Returns the
/// content's value, whether the close box was clicked or Escape pressed, and
/// the control rectangles.
pub(crate) fn window<R>(
    ctx: &egui::Context,
    salt: &'static str,
    title: &str,
    client: Vec2,
    content: impl FnOnce(&mut Canvas) -> R,
) -> (R, bool, Vec<(String, Rect)>) {
    canvas_shell(
        ctx,
        Shell {
            name: salt,
            id: egui::Id::new(("dialog_window", salt)),
            title,
            client,
            layering: Layering::Window,
            at: None,
            extras: Extras::default(),
        },
        salt,
        None,
        content,
    )
}

/// [`window`] as a modal dialog. The client area starts filled #D0D0D0
/// (deprecated: later batches fill their own body and drop this).
pub(crate) fn modal<R>(
    ctx: &egui::Context,
    salt: &'static str,
    title: &str,
    client: Vec2,
    content: impl FnOnce(&mut Canvas) -> R,
) -> (R, bool, Vec<(String, Rect)>) {
    modal_at(ctx, salt, title, client, None, content)
}

/// [`modal`] with its top-left corner at `at` (kept on the screen) instead
/// of the screen's centre, for a small picker that drops down from a field.
pub(crate) fn modal_at<R>(
    ctx: &egui::Context,
    salt: &'static str,
    title: &str,
    client: Vec2,
    at: Option<Pos2>,
    content: impl FnOnce(&mut Canvas) -> R,
) -> (R, bool, Vec<(String, Rect)>) {
    canvas_shell(
        ctx,
        Shell {
            name: salt,
            id: egui::Id::new(("schedule_dialog", salt)),
            title,
            client,
            layering: Layering::Modal,
            at,
            extras: Extras::default(),
        },
        salt,
        Some(STRIP),
        content,
    )
}

// ------------------------------------------------------------------ canvas

/// Absolute placement inside a dialog client area.
pub(crate) struct Canvas<'u> {
    pub(crate) ui: &'u mut egui::Ui,
    pub(crate) origin: Pos2,
    pub(crate) salt: &'static str,
    pub(crate) actions: Vec<(String, Rect)>,
}

/// The two-pixel sunken border of an edit and check box, also for a field
/// drawn outside a [`Canvas`] (wizard pages laid out by egui).
pub(crate) fn paint_sunken(p: &egui::Painter, rect: Rect, face: Color32) {
    p.rect_filled(rect, 0., face);
    let (l, t, r, b) = (rect.left(), rect.top(), rect.right(), rect.bottom());
    let line = |a: Pos2, z: Pos2, c: Color32| {
        p.rect_filled(Rect::from_min_max(a, z), 0., c);
    };
    line(Pos2::new(l, t), Pos2::new(r, t + 1.), SHADOW);
    line(Pos2::new(l, t), Pos2::new(l + 1., b), SHADOW);
    line(Pos2::new(l + 1., t + 1.), Pos2::new(r - 1., t + 2.), DARK);
    line(Pos2::new(l + 1., t + 1.), Pos2::new(l + 2., b - 1.), DARK);
    line(Pos2::new(l, b - 1.), Pos2::new(r, b), Color32::WHITE);
    line(Pos2::new(r - 1., t), Pos2::new(r, b), Color32::WHITE);
    line(
        Pos2::new(l + 1., b - 2.),
        Pos2::new(r - 1., b - 1.),
        LIGHT_EDGE,
    );
    line(
        Pos2::new(r - 2., t + 1.),
        Pos2::new(r - 1., b - 1.),
        LIGHT_EDGE,
    );
}

/// The 13 px sunken check box with its seven-pixel mark, top-left at `min`.
pub(crate) fn paint_check_glyph(p: &egui::Painter, min: Pos2, on: bool, enabled: bool) {
    let boxed = Rect::from_min_size(min, Vec2::splat(13.));
    paint_sunken(p, boxed, if enabled { Color32::WHITE } else { FACE });
    if on {
        let ink = if enabled { INK } else { DISABLED };
        // Seven-pixel check mark, as an unfeathered mesh.
        let mut mesh = egui::Mesh::default();
        for (i, dy) in [2., 3., 4., 3., 2., 1., 0.].iter().enumerate() {
            mesh.add_colored_rect(
                Rect::from_min_size(
                    boxed.min + Vec2::new(3. + i as f32, 3. + dy),
                    Vec2::new(1., 3.),
                ),
                ink,
            );
        }
        p.add(egui::Shape::mesh(mesh));
    }
}

/// The 12 x 12 sunken radio circle, top-left at `min`.
pub(crate) fn paint_radio_glyph(p: &egui::Painter, min: Pos2, selected: bool, enabled: bool) {
    // One character per pixel.
    const RADIO: [&str; 12] = [
        "....SSSS....",
        "..SSDDDDSS..",
        ".SDDWWWWDDH.",
        ".SDWWWWWWLH.",
        "SDWWWKKWWWLH",
        "SDWWKKKKWWLH",
        "SDWWKKKKWWLH",
        "SDWWWKKWWWLH",
        ".SDWWWWWWLH.",
        ".SLLWWWWLLH.",
        "..HHLLLLHH..",
        "....HHHH....",
    ];
    let well = if enabled { Color32::WHITE } else { FACE };
    // A plain mesh keeps single pixels crisp (no anti-aliasing feather).
    let mut mesh = egui::Mesh::default();
    for (row, line) in RADIO.iter().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            let color = match ch {
                'S' => SHADOW,
                'D' => DARK,
                'L' => LIGHT_EDGE,
                'H' => Color32::WHITE,
                'W' => well,
                'K' if selected => {
                    if enabled {
                        INK
                    } else {
                        DISABLED
                    }
                }
                'K' => well,
                _ => continue,
            };
            mesh.add_colored_rect(
                Rect::from_min_size(min + Vec2::new(col as f32, row as f32), Vec2::splat(1.)),
                color,
            );
        }
    }
    p.add(egui::Shape::mesh(mesh));
}

/// How a push button is drawn this frame.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Push {
    pub enabled: bool,
    pub pressed: bool,
    pub hovered: bool,
    /// The dialog's default button: black frame and dotted focus rectangle.
    pub default: bool,
}

impl Push {
    /// The state of a button from its response.
    pub(crate) fn of(response: &egui::Response, enabled: bool, default: bool) -> Self {
        Self {
            enabled,
            pressed: enabled && response.is_pointer_button_down_on(),
            hovered: response.hovered(),
            default,
        }
    }
}

/// The one push button painter: #F0F0F0 face (`HOT_FILL` under the
/// pointer), 1 px white / #A0A0A0 bevel (inverted while pressed), the
/// default button's black frame and dotted focus rectangle, then the icon
/// and the caption (with its `&` mnemonic) in [`button_font`].
pub(crate) fn paint_push_button(
    p: &egui::Painter,
    rect: Rect,
    caption: &str,
    icon: Option<(chrome::Icon, bool)>,
    s: Push,
) {
    let pressed = s.enabled && s.pressed;
    let hot = s.enabled && s.hovered && !pressed;
    p.rect_filled(rect, 0., if hot { chrome::pal::HOT_FILL } else { FACE });
    let inner = if s.default {
        p.rect_stroke(
            rect.shrink(0.5),
            0.,
            Stroke::new(1_f32, INK),
            egui::StrokeKind::Middle,
        );
        rect.shrink(1.)
    } else {
        rect
    };
    let (light, dark) = if pressed {
        (SHADOW, Color32::WHITE)
    } else {
        (Color32::WHITE, SHADOW)
    };
    let edge = |r: Rect, c: Color32| p.rect_filled(r, 0., c);
    edge(
        Rect::from_min_size(inner.min, Vec2::new(inner.width(), 1.)),
        light,
    );
    edge(
        Rect::from_min_size(inner.min, Vec2::new(1., inner.height())),
        light,
    );
    edge(
        Rect::from_min_size(
            Pos2::new(inner.left(), inner.bottom() - 1.),
            Vec2::new(inner.width(), 1.),
        ),
        dark,
    );
    edge(
        Rect::from_min_size(
            Pos2::new(inner.right() - 1., inner.top()),
            Vec2::new(1., inner.height()),
        ),
        dark,
    );
    if s.default {
        // Dotted focus rectangle, inset three pixels, as an unfeathered mesh.
        let f = rect.shrink(3.);
        let mut dots = egui::Mesh::default();
        let mut dot = |x: f32, y: f32| {
            dots.add_colored_rect(Rect::from_min_size(Pos2::new(x, y), Vec2::splat(1.)), INK)
        };
        let mut x = f.left();
        while x < f.right() {
            dot(x, f.top());
            dot(x, f.bottom() - 1.);
            x += 2.;
        }
        let mut y = f.top();
        while y < f.bottom() {
            dot(f.left(), y);
            dot(f.right() - 1., y);
            y += 2.;
        }
        p.add(egui::Shape::mesh(dots));
    }
    let color = if s.enabled { INK } else { DISABLED };
    chrome::paint_command_face(
        p,
        rect,
        caption,
        icon,
        button_font(),
        color,
        s.enabled,
        if pressed { 1. } else { 0. },
    );
}

/// `text` with an `&` before character `index` (literal `&` doubled), the
/// form [`chrome::mnemonic`] reads.
pub(crate) fn with_mnemonic(text: &str, index: Option<usize>) -> String {
    let mut out = String::with_capacity(text.len() + 1);
    for (i, ch) in text.chars().enumerate() {
        if Some(i) == index {
            out.push('&');
        }
        if ch == '&' {
            out.push('&');
        }
        out.push(ch);
    }
    out
}

impl Canvas<'_> {
    pub(crate) fn r(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(self.origin + Vec2::new(x, y), Vec2::new(w, h))
    }
    pub(crate) fn id(&self, key: &str) -> egui::Id {
        egui::Id::new((self.salt, key))
    }
    pub(crate) fn act(&mut self, name: &str, rect: Rect) {
        self.actions.push((name.into(), rect));
    }
    pub(crate) fn fill(&self, x: f32, y: f32, w: f32, h: f32, color: Color32) {
        self.ui.painter().rect_filled(self.r(x, y, w, h), 0., color);
    }
    pub(crate) fn hline(&self, x: f32, y: f32, w: f32, color: Color32) {
        self.fill(x, y, w, 1., color);
    }
    pub(crate) fn vline(&self, x: f32, y: f32, h: f32, color: Color32) {
        self.fill(x, y, 1., h, color);
    }
    /// Single lowered bevel: shadow top-left, highlight bottom-right.
    pub(crate) fn lowered(&self, x: f32, y: f32, w: f32, h: f32) {
        self.hline(x, y, w, SHADOW);
        self.vline(x, y, h, SHADOW);
        self.hline(x, y + h - 1., w, Color32::WHITE);
        self.vline(x + w - 1., y, h, Color32::WHITE);
    }
    pub(crate) fn raised(&self, x: f32, y: f32, w: f32, h: f32) {
        self.hline(x, y, w, Color32::WHITE);
        self.vline(x, y, h, Color32::WHITE);
        self.hline(x, y + h - 1., w, SHADOW);
        self.vline(x + w - 1., y, h, SHADOW);
    }
    /// Etched separator.
    pub(crate) fn etched(&self, x: f32, y: f32, w: f32) {
        self.hline(x, y, w, SHADOW);
        self.hline(x, y + 1., w, Color32::WHITE);
    }
    /// Two-pixel sunken border used by edits and check boxes.
    pub(crate) fn sunken(&self, rect: Rect, face: Color32) {
        paint_sunken(self.ui.painter(), rect, face);
    }
    /// Text with its top-left at `pos`; disabled text is plain #6D6D6D.
    pub(crate) fn text_at(&self, pos: Pos2, text: &str, enabled: bool, font: FontId) {
        let color = if enabled { INK } else { DISABLED };
        self.ui
            .painter()
            .text(pos, Align2::LEFT_TOP, text, font, color);
    }
    /// A label whose control rectangle starts at (x, y) with height h.
    pub(crate) fn label(&self, x: f32, y: f32, h: f32, text: &str, enabled: bool) {
        let pos = self.origin + Vec2::new(x, y + ((h - 13.) / 2.).floor());
        self.text_at(pos, text, enabled, dialog_font());
    }
    /// A group caption, bold like the section captions.
    pub(crate) fn bold(&self, x: f32, y: f32, h: f32, text: &str) {
        let pos = self.origin + Vec2::new(x, y + ((h - 13.) / 2.).floor());
        self.text_at(pos, text, true, heading_font());
    }
    pub(crate) fn hit(&mut self, key: &str, rect: Rect, enabled: bool) -> egui::Response {
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let response = self.ui.interact(rect, self.id(key), sense);
        self.act(key, rect);
        response
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn checkbox(
        &mut self,
        key: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        text: &str,
        value: &mut bool,
        enabled: bool,
    ) -> egui::Response {
        let rect = self.r(x, y, w, h);
        let response = self.hit(key, rect, enabled);
        if response.clicked() {
            *value = !*value;
        }
        let top = y + ((h - 13.) / 2.).floor();
        check_glyph(self, x, top, *value, enabled);
        self.label(x + 17., y, h, text, enabled);
        response
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn radio(
        &mut self,
        key: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        text: &str,
        selected: bool,
        enabled: bool,
    ) -> bool {
        let rect = self.r(x, y, w, h);
        let clicked = self.hit(key, rect, enabled).clicked();
        let top = self.origin + Vec2::new(x + 1., y + ((h - 12.) / 2.).floor());
        paint_radio_glyph(self.ui.painter(), top, selected, enabled);
        self.label(x + 17., y, h, text, enabled);
        clicked
    }
    /// A push button. `default` marks the dialog's default button (Enter
    /// runs it); the button Enter would run is framed either way. A command
    /// caption gets its icon ([`chrome::command_icon`]).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn button(
        &mut self,
        key: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        text: &str,
        default: bool,
        enabled: bool,
    ) -> bool {
        let icon = chrome::command_icon(text).filter(|_| w >= chrome::ICON_BUTTON_MIN_WIDTH);
        self.button_with_icon(key, (x, y, w, h), text, None, icon, default, enabled)
    }
    /// A push button with an explicit icon (or none, for message boxes) and
    /// an optional mnemonic underline under character `underline` of `text`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn button_with_icon(
        &mut self,
        key: &str,
        (x, y, w, h): (f32, f32, f32, f32),
        text: &str,
        underline: Option<usize>,
        icon: Option<(chrome::Icon, bool)>,
        default: bool,
        enabled: bool,
    ) -> bool {
        let rect = self.r(x, y, w, h);
        let response = self.hit(key, rect, enabled);
        let caption = with_mnemonic(text, underline);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, text));
        crate::dialog_keys::note_button(self.ui, &response, &caption, enabled);
        if default {
            crate::dialog_keys::mark_default(self.ui, &response);
        }
        let framed = default || crate::dialog_keys::is_default(self.ui, response.id);
        paint_push_button(
            self.ui.painter(),
            rect,
            &caption,
            icon,
            Push::of(&response, enabled, framed),
        );
        enabled && response.clicked()
    }
    /// Single-line edit with a sunken frame. Returns the text response.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn edit(
        &mut self,
        key: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        text: &mut String,
        enabled: bool,
        align: Align,
    ) -> egui::Response {
        self.edit_masked(key, (x, y, w, h), text, enabled, align, false)
    }
    /// [`Canvas::edit`] reserving `right_inset` pixels inside the right edge
    /// of the sunken frame (for an embedded `...` button) so the `TextEdit`
    /// never overlaps the button or steals its hover cursor.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn edit_with_button(
        &mut self,
        key: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        text: &mut String,
        enabled: bool,
        align: Align,
        right_inset: f32,
    ) -> egui::Response {
        let rect = self.r(x, y, w, h);
        self.sunken(rect, if enabled { Color32::WHITE } else { READONLY });
        let inner = Rect::from_min_max(
            Pos2::new(rect.left() + 3., rect.top() + 2.),
            Pos2::new(
                (rect.right() - right_inset).max(rect.left() + 6.),
                rect.bottom() - 2.,
            ),
        );
        let edit = egui::TextEdit::singleline(text)
            .id(self.id(key))
            .frame(false)
            .font(body_font())
            .text_color(INK)
            .horizontal_align(align)
            .vertical_align(Align::Center)
            .margin(egui::Margin::ZERO)
            .desired_width(inner.width());
        let response = self.ui.put(inner, edit.interactive(enabled));
        self.act(key, rect);
        response
    }
    /// [`Canvas::edit`] that shows bullets in place of the text (`masked`):
    /// a password or key is never drawn.
    pub(crate) fn edit_masked(
        &mut self,
        key: &str,
        (x, y, w, h): (f32, f32, f32, f32),
        text: &mut String,
        enabled: bool,
        align: Align,
        masked: bool,
    ) -> egui::Response {
        let rect = self.r(x, y, w, h);
        self.sunken(rect, if enabled { Color32::WHITE } else { READONLY });
        let inner = rect.shrink2(Vec2::new(3., 2.));
        let edit = egui::TextEdit::singleline(text)
            .password(masked)
            .id(self.id(key))
            .frame(false)
            .font(body_font())
            .text_color(INK)
            .horizontal_align(align)
            .vertical_align(Align::Center)
            .margin(egui::Margin::ZERO)
            .desired_width(inner.width());
        // Disabled edits keep black text on the read-only fill.
        let response = self.ui.put(inner, edit.interactive(enabled));
        self.act(key, rect);
        response
    }
    /// The small `...` button docked inside the right edge of an edit.
    pub(crate) fn ellipsis(&mut self, key: &str, edit: Rect, enabled: bool) -> egui::Response {
        let rect = Rect::from_min_size(
            Pos2::new(edit.right() - 19., edit.top() + 2.),
            Vec2::new(17., edit.height() - 4.),
        );
        let response = self.hit(key, rect, enabled);
        if enabled && response.hovered() {
            self.ui
                .ctx()
                .set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        self.ellipsis_face(rect, enabled);
        response
    }
    pub(crate) fn ellipsis_face(&self, rect: Rect, enabled: bool) {
        let p = self.ui.painter();
        p.rect_filled(rect, 0., FACE);
        p.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(rect.width(), 1.)),
            0.,
            Color32::WHITE,
        );
        p.rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.left(), rect.bottom() - 1.),
                Vec2::new(rect.width(), 1.),
            ),
            0.,
            SHADOW,
        );
        p.rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.right() - 1., rect.top()),
                Vec2::new(1., rect.height()),
            ),
            0.,
            SHADOW,
        );
        p.text(
            rect.center() + Vec2::new(0., -2.),
            Align2::CENTER_CENTER,
            "...",
            dialog_font(),
            if enabled { INK } else { DISABLED },
        );
    }
    /// Drop-down list. Items carry an enabled flag; returns true on change.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn combo(
        &mut self,
        key: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        items: &[(&str, bool)],
        selected: &mut usize,
        enabled: bool,
    ) -> bool {
        drop_down(
            self,
            key,
            (x, y, w, h),
            DropList {
                count: items.len(),
                row: 15.,
                min_width: 0.,
                available: &|i| items[i].1,
                field_tip: &|_| String::new(),
                item_tip: &|i| {
                    if items[i].1 {
                        String::new()
                    } else {
                        "Not supported by the scheduling engine.".into()
                    }
                },
                field: &|p, inner, i, enabled| {
                    let galley = p.layout_no_wrap(
                        items[i].0.to_string(),
                        body_font(),
                        if enabled { INK } else { DISABLED },
                    );
                    p.galley(
                        Pos2::new(inner.left() + 1., inner.center().y - galley.size().y / 2.),
                        galley,
                        INK,
                    );
                },
                item: &|p, r, i, hot, _| {
                    let (label, available) = items[i];
                    let lit = hot && available;
                    if lit {
                        p.rect_filled(r, 0., SELECT);
                    }
                    p.text(
                        r.left_center() + Vec2::new(3., 0.),
                        Align2::LEFT_CENTER,
                        label,
                        body_font(),
                        if !available {
                            DISABLED
                        } else if lit {
                            Color32::WHITE
                        } else {
                            INK
                        },
                    );
                },
            },
            selected,
            enabled,
        )
    }
    /// Numeric spin edit (text plus up/down arrows). Returns whether the
    /// value was committed this frame: an arrow stepped it, or the edit lost
    /// the keyboard (Enter, Tab, a click elsewhere or Escape).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn spin(
        &mut self,
        key: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        text: &mut String,
        step: f64,
        range: std::ops::RangeInclusive<f64>,
        enabled: bool,
    ) -> bool {
        let mut committed = self
            .edit(key, x, y, w - 18., h, text, enabled, Align::Min)
            .lost_focus();
        let frame = self.r(x, y, w, h);
        self.sunken(
            Rect::from_min_max(Pos2::new(frame.right() - 20., frame.top()), frame.max),
            FACE,
        );
        let half = (h - 4.) / 2.;
        for (i, delta) in [(0, step), (1, -step)] {
            let r = Rect::from_min_size(
                Pos2::new(frame.right() - 17., frame.top() + 2. + half * i as f32),
                Vec2::new(15., half),
            );
            let name = format!("{key}_{}", if i == 0 { "up" } else { "down" });
            let response = self.hit(&name, r, enabled);
            let p = self.ui.painter();
            p.rect_filled(r, 0., FACE);
            p.rect_stroke(
                r.shrink(0.5),
                0.,
                Stroke::new(1_f32, SHADOW),
                egui::StrokeKind::Middle,
            );
            let c = r.center();
            let points = if i == 0 {
                vec![
                    Pos2::new(c.x - 3., c.y + 1.5),
                    Pos2::new(c.x + 3., c.y + 1.5),
                    Pos2::new(c.x, c.y - 1.5),
                ]
            } else {
                vec![
                    Pos2::new(c.x - 3., c.y - 1.5),
                    Pos2::new(c.x + 3., c.y - 1.5),
                    Pos2::new(c.x, c.y + 1.5),
                ]
            };
            p.add(egui::Shape::convex_polygon(
                points,
                if enabled { INK } else { DISABLED },
                Stroke::NONE,
            ));
            if response.clicked() {
                let value = text.trim().parse::<f64>().unwrap_or(0.) + delta;
                *text = format_number(value.clamp(*range.start(), *range.end()));
                committed = true;
            }
        }
        committed
    }
}

// ------------------------------------------------------------- drop-downs

/// What a drop-down shows: its items, how each is painted, and tooltips.
struct DropList<'a> {
    count: usize,
    /// Height of one row of the open list.
    row: f32,
    /// The open list is at least this wide (else as wide as the field).
    min_width: f32,
    available: &'a dyn Fn(usize) -> bool,
    /// Tooltip over the field for the chosen item ("" for none).
    field_tip: &'a dyn Fn(usize) -> String,
    /// Tooltip over one item of the open list ("" for none).
    item_tip: &'a dyn Fn(usize) -> String,
    /// Paints the chosen item inside the field (painter clipped to the
    /// field's inner rectangle, that rectangle, item, enabled).
    field: &'a dyn Fn(&egui::Painter, Rect, usize, bool),
    /// Paints one row of the open list, its highlight included (painter,
    /// row rectangle, item, under the pointer, the chosen one).
    item: &'a dyn Fn(&egui::Painter, Rect, usize, bool, bool),
}

/// The one drop-down: a sunken field with the arrow button, and on a click
/// a list below it. Items are recorded as `{key}_item_{n}`; Escape or a
/// click elsewhere closes the list. Returns true when the choice changed.
fn drop_down(
    c: &mut Canvas,
    key: &str,
    (x, y, w, h): (f32, f32, f32, f32),
    list: DropList,
    selected: &mut usize,
    enabled: bool,
) -> bool {
    let rect = c.r(x, y, w, h);
    let id = c.id(key);
    let mut response = c.hit(key, rect, enabled);
    if *selected < list.count {
        let tip = (list.field_tip)(*selected);
        if !tip.is_empty() {
            response = response.on_hover_text(tip);
        }
    }
    c.sunken(rect, if enabled { Color32::WHITE } else { READONLY });
    let button = Rect::from_min_size(
        Pos2::new(rect.right() - 2. - 16., rect.top() + 2.),
        Vec2::new(16., h - 4.),
    );
    let p = c.ui.painter();
    p.rect_filled(button, 0., FACE);
    p.rect_filled(
        Rect::from_min_size(button.min, Vec2::new(button.width(), 1.)),
        0.,
        Color32::WHITE,
    );
    p.rect_filled(
        Rect::from_min_size(
            Pos2::new(button.left(), button.bottom() - 1.),
            Vec2::new(button.width(), 1.),
        ),
        0.,
        DARK,
    );
    p.rect_filled(
        Rect::from_min_size(
            Pos2::new(button.right() - 1., button.top()),
            Vec2::new(1., button.height()),
        ),
        0.,
        DARK,
    );
    let m = button.center();
    p.add(egui::Shape::convex_polygon(
        vec![
            Pos2::new(m.x - 3.5, m.y - 1.5),
            Pos2::new(m.x + 3.5, m.y - 1.5),
            Pos2::new(m.x, m.y + 2.),
        ],
        if enabled { INK } else { DISABLED },
        Stroke::NONE,
    ));
    if *selected < list.count {
        let inner = Rect::from_min_max(
            rect.min + Vec2::new(3., 2.),
            Pos2::new(button.left(), rect.bottom() - 2.),
        );
        (list.field)(&p.with_clip_rect(inner), inner, *selected, enabled);
    }
    let ctx = c.ui.ctx().clone();
    let mut open = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    if response.clicked() {
        open = !open;
    }
    let mut changed = false;
    if open && enabled {
        const MAX_DROP_HEIGHT: f32 = 240.;
        let row = list.row;
        let actions = &mut c.actions;
        let full_h = row * list.count as f32;
        let scrollable = full_h > MAX_DROP_HEIGHT;
        let inner_h = full_h.min(MAX_DROP_HEIGHT);
        let width = rect.width().max(list.min_width);
        let shown = egui::Area::new(id.with("list"))
            .order(egui::Order::Tooltip)
            .fixed_pos(rect.left_bottom())
            .show(&ctx, |ui| {
                let (area, _) =
                    ui.allocate_exact_size(Vec2::new(width, inner_h + 2.), Sense::hover());
                ui.painter().rect_filled(area, 0., Color32::WHITE);
                ui.painter().rect_stroke(
                    area.shrink(0.5),
                    0.,
                    Stroke::new(1_f32, INK),
                    egui::StrokeKind::Middle,
                );
                if !scrollable {
                    for i in 0..list.count {
                        let r = Rect::from_min_size(
                            area.min + Vec2::new(1., 1. + row * i as f32),
                            Vec2::new(area.width() - 2., row),
                        );
                        let available = (list.available)(i);
                        let hit = ui.interact(
                            r,
                            id.with(("item", i)),
                            if available {
                                Sense::click()
                            } else {
                                Sense::hover()
                            },
                        );
                        actions.push((format!("{key}_item_{i}"), r));
                        (list.item)(
                            &ui.painter().with_clip_rect(r),
                            r,
                            i,
                            hit.hovered(),
                            i == *selected,
                        );
                        if hit.clicked() {
                            if *selected != i {
                                *selected = i;
                                changed = true;
                            }
                            open = false;
                        }
                        let tip = (list.item_tip)(i);
                        if !tip.is_empty() {
                            hit.on_hover_text(tip);
                        }
                    }
                } else {
                    let inner = area.shrink(1.);
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(inner)
                            .layout(Layout::top_down(Align::Min)),
                    );
                    child.set_clip_rect(inner);
                    child.spacing_mut().item_spacing = Vec2::ZERO;
                    egui::ScrollArea::vertical()
                        .id_salt(id.with("scroll"))
                        .max_height(inner.height())
                        .auto_shrink([false, false])
                        .show_rows(&mut child, row, list.count, |ui, range| {
                            let row_w = ui.available_width();
                            for i in range {
                                let (r, _) =
                                    ui.allocate_exact_size(Vec2::new(row_w, row), Sense::hover());
                                let available = (list.available)(i);
                                let hit = ui.interact(
                                    r,
                                    id.with(("item", i)),
                                    if available {
                                        Sense::click()
                                    } else {
                                        Sense::hover()
                                    },
                                );
                                actions.push((format!("{key}_item_{i}"), r));
                                (list.item)(
                                    &ui.painter().with_clip_rect(r),
                                    r,
                                    i,
                                    hit.hovered(),
                                    i == *selected,
                                );
                                if hit.clicked() {
                                    if *selected != i {
                                        *selected = i;
                                        changed = true;
                                    }
                                    open = false;
                                }
                                let tip = (list.item_tip)(i);
                                if !tip.is_empty() {
                                    hit.on_hover_text(tip);
                                }
                            }
                        });
                }
            });
        let clicked_elsewhere = ctx.input(|i| i.pointer.any_pressed())
            && !shown.response.contains_pointer()
            && !response.contains_pointer();
        // Escape closes the list and nothing else.
        if clicked_elsewhere || ctx.input(|i| i.key_pressed(Key::Escape)) {
            open = false;
        }
        if open {
            crate::kit_window::note_combo_open(&ctx);
        }
    }
    ctx.data_mut(|d| d.insert_temp(id, open));
    changed
}

/// A drop-down whose field and list show drawn previews (bar shapes,
/// fill patterns) instead of names: `draw` paints item `n` into a
/// rectangle, `tip` names it as a tooltip. List rows are 18 px. Items are
/// recorded as `{key}_item_{n}`; returns true when the choice changed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn drawn_combo(
    c: &mut Canvas,
    key: &str,
    rect: (f32, f32, f32, f32),
    count: usize,
    selected: &mut usize,
    enabled: bool,
    tip: &dyn Fn(usize) -> String,
    draw: &dyn Fn(&egui::Painter, Rect, usize),
) -> bool {
    drop_down(
        c,
        key,
        rect,
        DropList {
            count,
            row: ROW,
            min_width: 60.,
            available: &|_| true,
            field_tip: tip,
            item_tip: tip,
            field: &|p, inner, i, _| draw(p, inner.shrink(1.), i),
            item: &|p, r, i, hot, chosen| {
                if hot || chosen {
                    p.rect_filled(r, 0., SELECT);
                }
                let swatch = r.shrink2(Vec2::new(4., 2.));
                p.rect_filled(swatch, 0., Color32::WHITE);
                draw(&p.with_clip_rect(swatch), swatch, i);
            },
        },
        selected,
        enabled,
    )
}

/// A spin edit of a number at client (x, y, w, h): the text is kept as
/// typed while the edit has the keyboard and is read when an arrow is
/// clicked or the edit loses the keyboard (Escape drops it). The value is
/// clamped to `range`. Returns true when the value changed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn number_spin(
    c: &mut Canvas,
    key: &str,
    (x, y, w, h): (f32, f32, f32, f32),
    value: &mut f64,
    step: f64,
    range: std::ops::RangeInclusive<f64>,
    enabled: bool,
) -> bool {
    let id = c.id(key);
    let shown = format_number(*value);
    let mut text = Draft::take(c.ui.ctx(), id, &shown);
    let committed = c.spin(key, x, y, w, h, &mut text, step, range.clone(), enabled);
    if c.ui.ctx().memory(|m| m.has_focus(id)) {
        Draft::keep(c.ui.ctx(), id, &shown, &text);
    }
    let escape = c.ui.input(|i| i.key_pressed(Key::Escape));
    if !committed || escape {
        return false;
    }
    match text.trim().parse::<f64>() {
        Ok(v) => {
            let v = v.clamp(*range.start(), *range.end());
            let changed = v != *value;
            *value = v;
            changed
        }
        Err(_) => false,
    }
}

// ------------------------------------------------------------------- text

/// Text whose top-left (egui text origin) is at client (x, y). Ink of a
/// capital starts one pixel right and two pixels down.
pub(crate) fn text(c: &Canvas, x: f32, y: f32, s: &str, enabled: bool) {
    c.text_at(c.origin + Vec2::new(x, y), s, enabled, dialog_font());
}

/// A validation message beside the field it is about, in the error color,
/// clipped to `w` pixels from `x`.
pub(crate) fn error_text(c: &Canvas, x: f32, y: f32, w: f32, s: &str) {
    let clip = Rect::from_min_size(c.origin + Vec2::new(x, y), Vec2::new(w, 16.));
    c.ui.painter().with_clip_rect(clip).text(
        clip.min,
        egui::Align2::LEFT_TOP,
        s,
        dialog_font(),
        crate::theme::color(crate::theme::ERROR_TEXT),
    );
}

/// Secondary live information (not a disabled control) at client (x, y).
pub(crate) fn secondary_text(c: &Canvas, x: f32, y: f32, s: &str) {
    c.ui.painter().text(
        c.origin + Vec2::new(x, y),
        egui::Align2::LEFT_TOP,
        s,
        dialog_font(),
        crate::theme::color(crate::theme::SECONDARY_TEXT),
    );
}

/// Lines of text on a fixed pitch.
pub(crate) fn lines(c: &Canvas, x: f32, y: f32, pitch: f32, s: &[&str], enabled: bool) {
    for (i, line) in s.iter().enumerate() {
        text(c, x, y + pitch * i as f32, line, enabled);
    }
}

/// A caption with an `&` mnemonic, the marked letter underlined.
pub(crate) fn mnemonic_text(c: &Canvas, x: f32, y: f32, label: &str, enabled: bool) {
    let (plain, index) = chrome::mnemonic(label);
    text(c, x, y, &plain, enabled);
    let Some(index) = index else {
        return;
    };
    let galley =
        c.ui.painter()
            .layout_no_wrap(plain.clone(), dialog_font(), INK);
    let left = galley
        .pos_from_cursor(egui::text::CCursor::new(index))
        .left();
    let right = galley
        .pos_from_cursor(egui::text::CCursor::new(index + 1))
        .left();
    let color = if enabled { INK } else { DISABLED };
    c.ui.painter().rect_filled(
        Rect::from_min_size(
            c.origin + Vec2::new(x + left.round(), y + 11.),
            Vec2::new((right - left).round().max(1.), 1.),
        ),
        0.,
        color,
    );
}

// --------------------------------------------------------- sections, tabs

/// A section: a lowered frame with a raised 21 px heading bar across its
/// top, the caption 6 px in.
pub(crate) fn section(c: &Canvas, x: f32, y: f32, w: f32, h: f32, title: &str) {
    c.lowered(x, y, w, h);
    heading(c, x + 1., y + 1., w - 2., title);
}

/// A raised 21 px heading bar with its caption in [`heading_font`] (bold),
/// vertically centred.
pub(crate) fn heading(c: &Canvas, x: f32, y: f32, w: f32, title: &str) {
    c.raised(x, y, w, 21.);
    c.ui.painter().text(
        c.origin + Vec2::new(x + 6., y + 10.5),
        Align2::LEFT_CENTER,
        title,
        heading_font(),
        INK,
    );
}

/// The page beside a vertical tab list: dark outer lines, white inner
/// top/left, #A0A0A0 inner bottom/right.
pub(crate) fn page(c: &Canvas, x: f32, y: f32, w: f32, h: f32) {
    c.fill(x, y, w, h, FACE);
    c.hline(x, y, w, TAB_LINE);
    c.vline(x, y, h, TAB_LINE);
    c.hline(x, y + h - 1., w, TAB_LINE);
    c.vline(x + w - 1., y, h, TAB_LINE);
    c.hline(x + 1., y + 1., w - 2., Color32::WHITE);
    c.vline(x + 1., y + 1., h - 2., Color32::WHITE);
    c.hline(x + 1., y + h - 2., w - 2., SHADOW);
    c.vline(x + w - 2., y + 1., h - 2., SHADOW);
}

/// One tab of [`vertical_tabs`] or [`tabs`].
pub(crate) struct VTab<'a> {
    /// Caption with an optional `&` mnemonic.
    pub label: &'a str,
    /// Action name suffix.
    pub key: &'a str,
    /// Why the tab's settings cannot be used; shown as a tooltip and the
    /// caption is grayed. The tab still opens.
    pub unavailable: Option<&'a str>,
}

/// A vertical tab list at (x, y), `w` wide including its left line, tabs on
/// a 25 px pitch; the page must start at x + w - 1 (the shared line). The
/// active tab merges with the page. Returns the clicked tab.
pub(crate) fn vertical_tabs(
    c: &mut Canvas,
    prefix: &str,
    x: f32,
    y: f32,
    w: f32,
    tabs: &[VTab],
    active: usize,
) -> Option<usize> {
    vertical_tabs_with_icons(c, prefix, x, y, w, tabs, &[], active)
}

/// [`vertical_tabs`] with a 16 px icon 6 px in on each tab that has one in
/// `icons` (same order as `tabs`); the caption then starts 26 px in.
#[allow(clippy::too_many_arguments)]
pub(crate) fn vertical_tabs_with_icons(
    c: &mut Canvas,
    prefix: &str,
    x: f32,
    y: f32,
    w: f32,
    tabs: &[VTab],
    icons: &[Option<chrome::Icon>],
    active: usize,
) -> Option<usize> {
    const PITCH: f32 = 25.;
    let mut clicked = None;
    let right = x + w - 1.;
    for (i, tab) in tabs.iter().enumerate() {
        let top = y + PITCH * i as f32;
        let bottom = top + PITCH;
        let rect = c.r(x, top, w, PITCH);
        let mut response = c.hit(&format!("{prefix}{}", tab.key), rect, true);
        if let Some(reason) = tab.unavailable {
            response = response.on_hover_text(reason);
        }
        if response.clicked() {
            clicked = Some(i);
        }
        if i == active {
            c.fill(x + 1., top + 1., w + 6., PITCH - 1., FACE);
            c.hline(x, top, w, TAB_LINE);
            c.hline(x, bottom, w, TAB_LINE);
            c.vline(x, top, PITCH + 1., TAB_LINE);
            c.hline(x + 1., top + 1., w, Color32::WHITE);
            c.vline(x + 1., top + 1., PITCH - 1., Color32::WHITE);
            c.hline(x + 2., bottom - 1., w - 1., SHADOW);
        } else {
            c.fill(x + 1., top + 1., w - 2., PITCH - 1., FACE);
            c.hline(x, top, w, TAB_LINE);
            c.hline(x, bottom, w, TAB_LINE);
            c.vline(x, top, PITCH + 1., TAB_LINE);
            c.vline(right, top, PITCH + 1., TAB_LINE);
        }
        let enabled = tab.unavailable.is_none();
        let mut left = x + 6.;
        if let Some(Some(icon)) = icons.get(i) {
            chrome::paint_icon_at(
                c.ui.painter(),
                *icon,
                c.origin + Vec2::new(left, top + 4.),
                enabled,
            );
            left += 20.;
        }
        mnemonic_text(c, left, top + 6., tab.label, enabled);
    }
    clicked
}

/// Horizontal tabs over the page at client (x, y, w, h): the tab strip of
/// the views' detail tabs, painted by the same code (`tab_strip::show`),
/// so a dialog's tabs look like the Activities page's. The strip takes the
/// 21 px above `y`; the page is filled and framed, and its content is
/// the caller's. Each tab is recorded as `{prefix}{key}`; an `unavailable`
/// reason shows as its tooltip. Tabs that do not fit scroll with arrows.
/// Returns the clicked tab.
pub(crate) fn tabs(
    c: &mut Canvas,
    prefix: &str,
    (x, y, w, h): (f32, f32, f32, f32),
    tabs: &[VTab],
    active: usize,
) -> Option<usize> {
    tab_strip_on(c, prefix, (x, y, w, h), tabs, active, false)
}

/// [`tabs`] padded to share the page's width, captions centred, as the
/// reference's Bars, Bar Chart Options and Page Setup dialogs draw them.
pub(crate) fn tabs_spread(
    c: &mut Canvas,
    prefix: &str,
    rect: (f32, f32, f32, f32),
    tabs: &[VTab],
    active: usize,
) -> Option<usize> {
    tab_strip_on(c, prefix, rect, tabs, active, true)
}

fn tab_strip_on(
    c: &mut Canvas,
    prefix: &str,
    (x, y, w, h): (f32, f32, f32, f32),
    tabs: &[VTab],
    active: usize,
    spread: bool,
) -> Option<usize> {
    let page = c.r(x, y, w, h);
    let first_id = c.id(prefix).with("first_tab");
    let mut first =
        c.ui.ctx()
            .data(|d| d.get_temp::<usize>(first_id))
            .unwrap_or(0);
    let captions: Vec<String> = tabs.iter().map(|t| chrome::mnemonic(t.label).0).collect();
    let captions: Vec<&str> = captions.iter().map(String::as_str).collect();
    let strip = crate::tab_strip::show_spread(
        c.ui,
        (c.salt, "kit_tabs"),
        page,
        &captions,
        active,
        &mut first,
        spread,
    );
    c.ui.ctx().data_mut(|d| d.insert_temp(first_id, first));
    for (i, response) in strip.tabs {
        // A tab's hit area takes the divider it shares with the next one;
        // the recorded rectangle stops before it.
        let r = response.rect;
        c.act(
            &format!("{prefix}{}", tabs[i].key),
            Rect::from_min_max(r.min, Pos2::new(r.right() - 1., r.bottom())),
        );
        if let Some(reason) = tabs[i].unavailable {
            let _ = response.on_hover_text(reason);
        }
    }
    strip.clicked
}

// ---------------------------------------------------------- check, radio

/// A checkbox whose caption is at the left and box at the right edge.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_right(
    c: &mut Canvas,
    key: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    label: &str,
    value: &mut bool,
    enabled: bool,
) -> egui::Response {
    let rect = c.r(x, y, w, h);
    let response = c.hit(key, rect, enabled);
    if response.clicked() {
        *value = !*value;
    }
    let top = y + ((h - 13.) / 2.).floor();
    check_glyph(c, x + w - 13., top, *value, enabled);
    mnemonic_text(c, x + 1., top, label, enabled);
    response
}

/// The 13 px sunken check box with its seven-pixel mark at client (x, y).
pub(crate) fn check_glyph(c: &Canvas, x: f32, y: f32, on: bool, enabled: bool) {
    paint_check_glyph(c.ui.painter(), c.origin + Vec2::new(x, y), on, enabled);
}

/// A radio button with a mnemonic caption of one or more lines (14 px
/// apart), the glyph centred on the block.
#[allow(clippy::too_many_arguments)]
pub(crate) fn radio(
    c: &mut Canvas,
    key: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    label: &str,
    selected: bool,
    enabled: bool,
) -> bool {
    let clicked = c.radio(key, x, y, w, h, "", selected, enabled);
    let rows: Vec<&str> = label.split('\n').collect();
    let block = 13. + 14. * (rows.len() as f32 - 1.);
    let top = y + ((h - block) / 2.).floor();
    for (i, row) in rows.iter().enumerate() {
        mnemonic_text(c, x + 17., top + 14. * i as f32, row, enabled);
    }
    clicked
}

// ------------------------------------------------------------ fields, edits

/// A read-only field: sunken frame filled #F8F8F8 (white when `white`),
/// text 3 px in.
pub(crate) fn field(c: &Canvas, x: f32, y: f32, w: f32, h: f32, s: &str, white: bool) {
    field_aligned(c, (x, y, w, h), s, white, false);
}

/// [`field`] for a number: the value 3 px in from the right edge, as every
/// number box is (`views::sheet::number_field` is the editable one).
pub(crate) fn number_field(c: &Canvas, x: f32, y: f32, w: f32, h: f32, s: &str, white: bool) {
    field_aligned(c, (x, y, w, h), s, white, true);
}

fn field_aligned(
    c: &Canvas,
    (x, y, w, h): (f32, f32, f32, f32),
    s: &str,
    white: bool,
    right: bool,
) {
    c.sunken(
        c.r(x, y, w, h),
        if white { Color32::WHITE } else { READONLY },
    );
    let galley =
        c.ui.painter()
            .layout_no_wrap(s.to_owned(), dialog_font(), INK);
    let top = y + ((h - galley.size().y) / 2.).round();
    let left = if right {
        x + w - 3. - galley.size().x
    } else {
        x + 3.
    };
    c.ui.painter()
        .with_clip_rect(c.r(x + 2., y + 2., w - 4., h - 4.))
        .galley(c.origin + Vec2::new(left, top), galley, INK);
}

/// A 21 px edit of `current` that reports its text when it loses focus
/// with a change (Escape drops the change). While focused it keeps its own
/// draft, so the value shown is the document's whenever it is not being
/// typed in. Losing the keyboard to Enter (the default button) or to an
/// accelerator commits the draft too; see [`Draft`].
#[allow(clippy::too_many_arguments)]
#[allow(dead_code)] // first callers arrive with the later dialog batches
pub(crate) fn commit_edit(
    c: &mut Canvas,
    key: &str,
    x: f32,
    y: f32,
    w: f32,
    current: &str,
    enabled: bool,
    align: Align,
) -> Option<String> {
    let id = c.id(key);
    let mut text = Draft::take(c.ui.ctx(), id, current);
    let response = c.edit(key, x, y, w, 21., &mut text, enabled, align);
    if response.has_focus() {
        Draft::keep(c.ui.ctx(), id, current, &text);
    } else if response.lost_focus() {
        let escape = c.ui.input(|i| i.key_pressed(Key::Escape));
        if !escape && text != current {
            return Some(text);
        }
    }
    None
}

/// The text typed in a kit edit ([`commit_edit`], [`number_spin`]) that
/// has not been committed yet, kept from one pass to the next.
///
/// It is read whether or not the edit still has the keyboard: Enter
/// pressing the default button and an accelerator both take the keyboard
/// away before the edit draws (`dialog_keys`, `frame.rs`), and the edit
/// must commit what was typed, not the stored value. A draft counts only
/// for the value it was typed over, and only in the pass after it was
/// kept: one left by an edit that stopped drawing is dropped.
#[derive(Clone)]
struct Draft {
    base: String,
    typed: String,
    pass: u64,
}

impl Draft {
    fn id(edit: egui::Id) -> egui::Id {
        edit.with("draft")
    }

    /// The text to show in the edit: the draft typed over `shown` last
    /// pass, else `shown`. The draft is used up.
    fn take(ctx: &egui::Context, edit: egui::Id, shown: &str) -> String {
        let now = ctx.cumulative_pass_nr();
        ctx.data_mut(|d| {
            let draft = d.get_temp::<Draft>(Self::id(edit));
            d.remove::<Draft>(Self::id(edit));
            draft
        })
        .filter(|d| d.base == shown && d.pass + 1 >= now)
        .map_or_else(|| shown.to_string(), |d| d.typed)
    }

    /// The edit has the keyboard: keep `typed` for the next pass.
    fn keep(ctx: &egui::Context, edit: egui::Id, shown: &str, typed: &str) {
        let draft = Draft {
            base: shown.to_string(),
            typed: typed.to_string(),
            pass: ctx.cumulative_pass_nr(),
        };
        ctx.data_mut(|d| d.insert_temp(Self::id(edit), draft));
    }
}

/// A multi-line edit at client (x, y, w, h): the sunken white field with
/// the text 3 px in, in `font` and `color` (the font of what is being
/// written, such as a text attachment). Enter starts a new line rather
/// than pressing the default button. Text taller than the field scrolls,
/// with a scroll bar only then. Recorded as `key`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn text_area(
    c: &mut Canvas,
    key: &str,
    (x, y, w, h): (f32, f32, f32, f32),
    text: &mut String,
    font: FontId,
    color: Color32,
) -> egui::Response {
    text_area_with(c, key, (x, y, w, h), text, font, color, TextArea::default())
}

/// How a [`text_area_with`] behaves beyond the plain multi-line edit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextArea<'a> {
    /// Grey text shown while the edit is empty.
    pub hint: &'a str,
    /// Enter is the caller's (a message composer sends on it) and
    /// Shift+Enter starts a new line; otherwise Enter starts a new line.
    pub enter_is_callers: bool,
    pub enabled: bool,
    /// Text taller than the field scrolls inside it.
    pub scroll: bool,
}

impl Default for TextArea<'_> {
    fn default() -> Self {
        Self {
            hint: "",
            enter_is_callers: false,
            enabled: true,
            scroll: false,
        }
    }
}

/// [`text_area`] with a hint, a disabled state (read-only fill), text that
/// scrolls inside the field, and Enter left to the caller.
#[allow(clippy::too_many_arguments)]
pub(crate) fn text_area_with(
    c: &mut Canvas,
    key: &str,
    (x, y, w, h): (f32, f32, f32, f32),
    text: &mut String,
    font: FontId,
    color: Color32,
    opts: TextArea,
) -> egui::Response {
    let frame = c.r(x, y, w, h);
    c.sunken(
        frame,
        if opts.enabled {
            Color32::WHITE
        } else {
            READONLY
        },
    );
    let mut edit = egui::TextEdit::multiline(text)
        .id(c.id(key))
        .frame(false)
        .font(font)
        .text_color(color)
        .margin(egui::Margin::ZERO)
        .desired_width(frame.width() - 6.)
        .interactive(opts.enabled);
    if !opts.hint.is_empty() {
        edit = edit.hint_text(egui::RichText::new(opts.hint).color(DISABLED));
    }
    if opts.enter_is_callers {
        edit = edit.return_key(Some(egui::KeyboardShortcut::new(
            egui::Modifiers::SHIFT,
            Key::Enter,
        )));
    }
    let inner = frame.shrink2(Vec2::new(3., 3.));
    let response = if opts.scroll {
        let mut child = c.ui.new_child(egui::UiBuilder::new().max_rect(inner));
        child.set_clip_rect(inner.intersect(c.ui.clip_rect()));
        egui::ScrollArea::vertical()
            .id_salt(c.id(key).with("scroll"))
            .max_height(inner.height())
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(&mut child, |ui| {
                ui.add_sized(
                    Vec2::new(inner.width(), inner.height()),
                    edit.desired_rows(1),
                )
            })
            .inner
    } else {
        c.ui.put(inner, edit)
    };
    crate::dialog_keys::note_keeps_enter(&response);
    c.act(key, frame);
    response
}

/// Adds a tooltip over a client rectangle (for disabled groups).
pub(crate) fn tooltip(c: &mut Canvas, key: &str, x: f32, y: f32, w: f32, h: f32, tip: &str) {
    let rect = c.r(x, y, w, h);
    let _ =
        c.ui.interact(rect, c.id(key), Sense::hover())
            .on_hover_text(tip);
}

/// A text link at (x, y) in the link color of replies and help, underlined
/// under the mouse with the pointing hand; true when clicked. Returns its
/// width through the recorded rectangle `key`.
pub(crate) fn link(c: &mut Canvas, key: &str, x: f32, y: f32, text: &str) -> bool {
    let ink = crate::theme::color(crate::theme::DATA_DATE);
    let galley =
        c.ui.painter()
            .layout_no_wrap(text.to_owned(), body_font(), ink);
    let rect = Rect::from_min_size(c.origin + Vec2::new(x, y), galley.size());
    let response = c.ui.interact(rect, c.id(key), Sense::click());
    c.ui.painter().galley(rect.min, galley, ink);
    if response.hovered() {
        c.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        c.ui.painter().hline(
            rect.left()..=rect.right(),
            rect.bottom() - 1.5,
            Stroke::new(1_f32, ink),
        );
    }
    c.act(key, rect);
    response.clicked()
}

impl App {
    /// Stores a fixed-layout dialog's control rectangles for tests.
    pub(crate) fn record_dialog_actions(&mut self, actions: Vec<(String, Rect)>) {
        for (name, r) in actions {
            self.action_rects
                .insert(name, [r.left(), r.top(), r.right(), r.bottom()]);
        }
    }
}

// ----------------------------------------------------------- push buttons

/// A push button whose caption may carry an `&` mnemonic. A command
/// caption gets its icon ([`chrome::command_icon`]). The button Enter runs
/// is framed as the default (see [`default_button`] to name it).
#[allow(clippy::too_many_arguments)]
pub(crate) fn button(
    c: &mut Canvas,
    key: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    label: &str,
    enabled: bool,
) -> bool {
    push(c, key, (x, y, w, h), label, false, enabled)
}

/// [`button`] named as the dialog's default: Enter runs it.
#[allow(clippy::too_many_arguments)]
#[allow(dead_code)] // first callers arrive with the later dialog batches
pub(crate) fn default_button(
    c: &mut Canvas,
    key: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    label: &str,
    enabled: bool,
) -> bool {
    push(c, key, (x, y, w, h), label, true, enabled)
}

fn push(
    c: &mut Canvas,
    key: &str,
    rect: (f32, f32, f32, f32),
    label: &str,
    default: bool,
    enabled: bool,
) -> bool {
    let (plain, index) = chrome::mnemonic(label);
    let icon = chrome::command_icon(label).filter(|_| rect.2 >= chrome::ICON_BUTTON_MIN_WIDTH);
    c.button_with_icon(key, rect, &plain, index, icon, default, enabled)
}

/// Where a caption belongs in the standard order: OK (or the committing
/// verb), Cancel or Close, Apply, the others, Help last.
#[cfg_attr(not(test), allow(dead_code))] // read by the dialog sweep
fn standard_rank(caption: &str) -> Option<u8> {
    let (text, _) = chrome::mnemonic(caption);
    match text
        .trim()
        .trim_end_matches("...")
        .trim()
        .to_lowercase()
        .as_str()
    {
        "ok" => Some(0),
        "cancel" | "close" => Some(1),
        "apply" => Some(2),
        "help" => Some(3),
        _ => None,
    }
}

/// A standard button that must be [`BUTTON`] sized: OK, Cancel, Close,
/// Apply, Help.
#[cfg_attr(not(test), allow(dead_code))] // read by the dialog sweep
pub(crate) fn is_standard_caption(caption: &str) -> bool {
    standard_rank(caption).is_some()
}

/// Why buttons in reading order break the standard order, if they do: in
/// a column OK, Cancel, Apply, others, Help last; in a row (`row`) Help
/// first, then the same order.
#[cfg_attr(not(test), allow(dead_code))] // read by the dialog sweep
pub(crate) fn order_problem(captions: &[&str], row: bool) -> Option<String> {
    let help = |c: &&str| standard_rank(c) == Some(3);
    if row {
        return match captions.iter().position(help) {
            Some(i) if i > 0 => Some(format!("Help after {}", captions[..i].join(", "))),
            Some(_) => order_problem(&captions[1..], false),
            None => order_problem(captions, false),
        };
    }
    let mut last: Option<(u8, &str)> = None;
    for (i, caption) in captions.iter().enumerate() {
        // Apply heading the column is the committing verb (Update
        // Progress, Apply Actuals): it stands in OK's place.
        let rank = match standard_rank(caption) {
            Some(2) if i == 0 => Some(0),
            rank => rank,
        };
        if let Some(rank) = rank {
            if let Some((before, name)) = last {
                if rank < before {
                    return Some(format!("{caption} after {name}"));
                }
            }
            last = Some((rank, caption));
        }
    }
    match captions.iter().position(help) {
        Some(i) if i + 1 < captions.len() => {
            Some(format!("Help before {}", captions[i + 1..].join(", ")))
        }
        _ => None,
    }
}

/// One entry of a [`button_column`] or [`button_row`].
#[derive(Clone, Copy)]
#[allow(dead_code)] // first callers arrive with the later dialog batches
pub(crate) enum Entry<'a> {
    /// Action name, caption, enabled, starts a group (10 px more space).
    Button(&'a str, &'a str, bool, bool),
    /// The move pad (columns only): left, up, down, right enabled.
    Pad(&'a str, [bool; 4]),
}

impl<'a> Entry<'a> {
    /// The standard OK button (`{prefix}_ok`).
    pub(crate) fn ok(key: &'a str) -> Self {
        Entry::Button(key, "OK", true, false)
    }
    pub(crate) fn cancel(key: &'a str) -> Self {
        Entry::Button(key, "Cancel", true, false)
    }
    pub(crate) fn close(key: &'a str) -> Self {
        Entry::Button(key, "Close", true, false)
    }
    pub(crate) fn apply(key: &'a str, enabled: bool) -> Self {
        Entry::Button(key, "Apply", enabled, true)
    }
    pub(crate) fn help(key: &'a str) -> Self {
        Entry::Button(key, "Help", true, true)
    }
    fn caption(&self) -> Option<&'a str> {
        match self {
            Entry::Button(_, label, _, _) => Some(label),
            Entry::Pad(..) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Move {
    Left,
    Up,
    Down,
    Right,
}

/// What a button column or row reported.
#[derive(Default)]
#[allow(dead_code)] // first callers arrive with the later dialog batches
pub(crate) struct Column {
    /// Action name of the clicked button.
    pub clicked: Option<String>,
    pub moved: Option<Move>,
}

/// The #D0D0D0 strip down the right edge of a `w` x `h` client, where
/// [`button_column`] puts its buttons.
pub(crate) fn strip(c: &Canvas, w: f32, h: f32) {
    c.fill(w - STRIP_WIDTH, 0., STRIP_WIDTH, h, STRIP);
}

/// Callers list entries in the column order (Help last), rows too.
fn check_order(entries: &[Entry]) {
    let captions: Vec<&str> = entries.iter().filter_map(Entry::caption).collect();
    debug_assert!(
        order_problem(&captions, false).is_none(),
        "buttons out of the standard order: {:?}",
        order_problem(&captions, false)
    );
}

/// The standard right-hand button column of a `w` wide client: [`BUTTON`]
/// buttons at the strip's left edge (x = w - 112) from y 6 on a 30 px
/// pitch, 10 px more before a grouped entry; the move pad is 108 x 43.
/// Entries must be in the standard order (checked in debug builds).
pub(crate) fn button_column(c: &mut Canvas, w: f32, entries: &[Entry]) -> Column {
    check_order(entries);
    lay_column(c, w, entries)
}

/// A [`button_column`] in the order observed on the reference's own
/// dialog where that order differs from the standard one (the Calendar
/// editor's Help third, the Schedule dialog's Cancel first). The order is
/// declared, not checked: the dialog sweep accepts exactly this order on
/// this dialog and still checks every other column against the standard.
pub(crate) fn observed_button_column(c: &mut Canvas, w: f32, entries: &[Entry]) -> Column {
    declare_order(c, false, entries);
    lay_column(c, w, entries)
}

/// A [`button_row`] in the order observed on the reference's own dialog
/// where it differs from the standard one (Calendars in Use: Help, Cancel,
/// OK). Entries are drawn left to right exactly as listed; the order is
/// declared, not checked, as for [`observed_button_column`].
pub(crate) fn observed_button_row(c: &mut Canvas, right: f32, y: f32, entries: &[Entry]) -> Column {
    declare_order(c, true, entries);
    lay_row(c, right, y, entries)
}

fn declare_order(c: &Canvas, row: bool, entries: &[Entry]) {
    let captions: Vec<String> = entries
        .iter()
        .filter_map(Entry::caption)
        .map(|caption| chrome::mnemonic(caption).0)
        .collect();
    let (layer, pass) = (c.ui.layer_id(), c.ui.ctx().cumulative_pass_nr());
    c.ui.ctx().data_mut(|d| {
        let notes = pass_notes(d, pass);
        notes.declared.push((layer, row, captions));
    });
}

/// Button orders declared through [`observed_button_column`] (false) and
/// [`observed_button_row`] (true) on `layer` in the pass being drawn,
/// captions without their access keys.
#[cfg(test)]
pub(crate) fn declared_orders(
    ctx: &egui::Context,
    layer: egui::LayerId,
) -> Vec<(bool, Vec<String>)> {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|d| {
        d.get_temp::<PassNotes>(egui::Id::new("dialog_pass_notes"))
            .filter(|n| n.pass == pass)
            .map(|n| {
                n.declared
                    .into_iter()
                    .filter(|(l, _, _)| *l == layer)
                    .map(|(_, row, captions)| (row, captions))
                    .collect()
            })
            .unwrap_or_default()
    })
}

fn lay_column(c: &mut Canvas, w: f32, entries: &[Entry]) -> Column {
    let x = w - STRIP_WIDTH;
    let mut y = COLUMN_TOP;
    let mut out = Column::default();
    for (k, e) in entries.iter().enumerate() {
        match *e {
            Entry::Button(key, label, enabled, group) => {
                if group && k > 0 {
                    y += GROUP_GAP;
                }
                if button(c, key, x, y, BUTTON.x, BUTTON.y, label, enabled) {
                    out.clicked = Some(key.to_string());
                }
                y += BUTTON_PITCH;
            }
            Entry::Pad(prefix, enabled) => {
                if k > 0 {
                    y += GROUP_GAP;
                }
                out.moved = move_pad(c, prefix, x, y, enabled);
                y += 43. + 15.;
            }
        }
    }
    out
}

/// The standard bottom row: [`BUTTON`] buttons left to right ending at
/// client x `right`, top at `y`, [`ROW_GAP`] apart and [`GROUP_GAP`] more
/// before a grouped one. Entries are listed in the standard (column) order;
/// the row draws Help first, at its left, as the reference does, with no
/// group gap after it. A pad is not drawn in a row.
#[allow(dead_code)] // first callers arrive with the later dialog batches
pub(crate) fn button_row(c: &mut Canvas, right: f32, y: f32, entries: &[Entry]) -> Column {
    check_order(entries);
    let is_help = |e: &Entry| e.caption().is_some_and(|c| standard_rank(c) == Some(3));
    let mut row: Vec<Entry> = entries.iter().copied().filter(is_help).collect();
    row.extend(entries.iter().copied().filter(|e| !is_help(e)));
    // The button that led the row follows Help at the plain gap.
    if row.first().is_some_and(is_help) {
        if let Some(Entry::Button(_, _, _, group)) = row.get_mut(1) {
            *group = false;
        }
    }
    lay_row(c, right, y, &row)
}

fn lay_row(c: &mut Canvas, right: f32, y: f32, entries: &[Entry]) -> Column {
    let mut widths = 0.;
    for (k, e) in entries.iter().enumerate() {
        if let Entry::Button(_, _, _, group) = e {
            if k > 0 {
                widths += ROW_GAP + if *group { GROUP_GAP } else { 0. };
            }
            widths += BUTTON.x;
        }
    }
    let mut x = right - widths;
    let mut out = Column::default();
    for (k, e) in entries.iter().enumerate() {
        if let Entry::Button(key, label, enabled, group) = *e {
            if group && k > 0 {
                x += GROUP_GAP;
            }
            if button(c, key, x, y, BUTTON.x, BUTTON.y, label, enabled) {
                out.clicked = Some(key.to_string());
            }
            x += BUTTON.x + ROW_GAP;
        }
    }
    out
}

/// The four-arrow move pad: left and right arrows at the sides, up and down
/// stacked between them.
pub(crate) fn move_pad(
    c: &mut Canvas,
    prefix: &str,
    x: f32,
    y: f32,
    enabled: [bool; 4],
) -> Option<Move> {
    let parts = [
        (Move::Left, "left", (x, y, 30., 43.)),
        (Move::Up, "up", (x + 30., y, 49., 22.)),
        (Move::Down, "down", (x + 30., y + 22., 49., 21.)),
        (Move::Right, "right", (x + 79., y, 29., 43.)),
    ];
    let mut moved = None;
    for (k, (dir, name, (px, py, pw, ph))) in parts.into_iter().enumerate() {
        let rect = c.r(px, py, pw, ph);
        let response = c.hit(&format!("{prefix}_{name}"), rect, enabled[k]);
        let pressed = enabled[k] && response.is_pointer_button_down_on();
        let hot = enabled[k] && response.hovered() && !pressed;
        c.fill(
            px,
            py,
            pw,
            ph,
            if hot { chrome::pal::HOT_FILL } else { FACE },
        );
        if pressed {
            c.lowered(px, py, pw, ph);
        } else {
            c.raised(px, py, pw, ph);
        }
        let m = rect.center();
        let (a, b, d) = match dir {
            Move::Up => (Vec2::new(-5., 3.), Vec2::new(5., 3.), Vec2::new(0., -3.)),
            Move::Down => (Vec2::new(-5., -3.), Vec2::new(5., -3.), Vec2::new(0., 3.)),
            Move::Left => (Vec2::new(3., -5.), Vec2::new(3., 5.), Vec2::new(-3., 0.)),
            Move::Right => (Vec2::new(-3., -5.), Vec2::new(-3., 5.), Vec2::new(3., 0.)),
        };
        let color = if enabled[k] { INK } else { DISABLED };
        c.ui.painter().add(egui::Shape::convex_polygon(
            vec![m + a, m + b, m + d],
            color,
            Stroke::NONE,
        ));
        if enabled[k] && response.clicked() {
            moved = Some(dir);
        }
    }
    moved
}

/// One 33 x 33 move button of a two-list picker (Columns: add, add all,
/// remove, remove all, up, down): the move pad's face and drawn arrow,
/// doubled for the "all" moves. True when clicked.
pub(crate) fn move_button(
    c: &mut Canvas,
    key: &str,
    (x, y): (f32, f32),
    dir: Move,
    double: bool,
    enabled: bool,
) -> bool {
    let (w, h) = (33., 33.);
    let rect = c.r(x, y, w, h);
    let response = c.hit(key, rect, enabled);
    let pressed = enabled && response.is_pointer_button_down_on();
    let hot = enabled && response.hovered() && !pressed;
    c.fill(x, y, w, h, if hot { chrome::pal::HOT_FILL } else { FACE });
    if pressed {
        c.lowered(x, y, w, h);
    } else {
        c.raised(x, y, w, h);
    }
    let (a, b, d) = match dir {
        Move::Up => (Vec2::new(-5., 3.), Vec2::new(5., 3.), Vec2::new(0., -3.)),
        Move::Down => (Vec2::new(-5., -3.), Vec2::new(5., -3.), Vec2::new(0., 3.)),
        Move::Left => (Vec2::new(3., -5.), Vec2::new(3., 5.), Vec2::new(-3., 0.)),
        Move::Right => (Vec2::new(-3., -5.), Vec2::new(-3., 5.), Vec2::new(3., 0.)),
    };
    let color = if enabled { INK } else { DISABLED };
    let offsets: &[f32] = if double { &[-3., 4.] } else { &[0.] };
    for dx in offsets {
        let m = rect.center() + Vec2::new(*dx, 0.);
        c.ui.painter().add(egui::Shape::convex_polygon(
            vec![m + a, m + b, m + d],
            color,
            Stroke::NONE,
        ));
    }
    enabled && response.clicked()
}

// ------------------------------------------------------------- list grid

/// Selected row and focused cell colors of the reference grids.
pub(crate) const SELECTED: Color32 = chrome::pal::SELECTED;
pub(crate) const FOCUS: Color32 = crate::theme::color(crate::theme::AMBER);
pub(crate) const ROW_LINE: Color32 = Color32::from_rgb(0xC0, 0xC0, 0xC0);
pub(crate) const ROW: f32 = 18.;
/// Width of a list's scroll bar.
pub(crate) const SCROLLBAR: f32 = 17.;

/// One column of a [`list`]: caption, width and whether text is centred.
pub(crate) struct Col<'a> {
    pub title: &'a str,
    pub width: f32,
}

#[derive(Default)]
pub(crate) struct ListOut {
    pub clicked: Option<usize>,
    pub double: Option<usize>,
}

/// A grid in a lowered frame at (x, y, w, h): an optional `v Display: ...`
/// header button, a column header row, then 18 px rows with #C0C0C0 lines,
/// full-row selection and a 2 px focus box on `focus_col` of the selected
/// row. The mouse wheel and a scroll bar move it by whole rows. `cell` paints
/// one cell (canvas, row, column, absolute cell rectangle, selected).
#[allow(clippy::too_many_arguments)]
pub(crate) fn list(
    c: &mut Canvas,
    key: &str,
    (x, y, w, h): (f32, f32, f32, f32),
    display: Option<&str>,
    cols: &[Col],
    rows: usize,
    selected: Option<usize>,
    focus_col: Option<usize>,
    cell: &mut dyn FnMut(&mut Canvas, usize, usize, Rect, bool),
) -> ListOut {
    c.lowered(x, y, w, h);
    c.fill(x + 1., y + 1., w - 2., h - 2., Color32::WHITE);
    let mut top = y + 1.;
    if let Some(display) = display {
        c.fill(x + 1., top, w - 2., 18., FACE);
        c.raised(x + 1., top, w - 2., 18.);
        let o = c.origin + Vec2::new(x + 16., top + 6.);
        let mut mesh = egui::Mesh::default();
        for i in 0..3 {
            for dx in [i as f32, 4. - i as f32] {
                mesh.add_colored_rect(
                    Rect::from_min_size(o + Vec2::new(dx, i as f32), Vec2::splat(1.)),
                    INK,
                );
            }
        }
        c.ui.painter().add(egui::Shape::mesh(mesh));
        text(c, x + 25., top + 3., display, true);
        top += 18.;
    }
    // Column header.
    c.fill(x + 1., top, w - 2., 18., FACE);
    c.hline(x + 1., top, w - 2., Color32::WHITE);
    c.hline(x + 1., top + 17., w - 2., INK);
    let mut left = x + 1.;
    for col in cols {
        c.vline(left, top, 17., Color32::WHITE);
        c.vline(left + col.width - 1., top, 17., INK);
        text(c, left + 3., top + 3., col.title, true);
        left += col.width;
    }
    top += 18.;
    let body = c.r(x + 1., top, w - 2., y + h - 1. - top);
    let widths: Vec<f32> = cols.iter().map(|c| c.width).collect();
    rows_in(
        c,
        key,
        body,
        ROW,
        rows,
        selected,
        &mut |c, r, row_rect, out| {
            let sel = selected == Some(r);
            let painter = c.ui.painter().with_clip_rect(body);
            let mut left = row_rect.left();
            for width in &widths {
                let cell_rect =
                    Rect::from_min_size(Pos2::new(left, row_rect.top()), Vec2::new(*width, ROW));
                if sel {
                    painter.rect_filled(cell_rect, 0., SELECTED);
                }
                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(cell_rect.right() - 1., row_rect.top()),
                        Vec2::new(1., ROW),
                    ),
                    0.,
                    ROW_LINE,
                );
                left += width;
            }
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(row_rect.left(), row_rect.top() + ROW - 1.),
                    Vec2::new(left - row_rect.left(), 1.),
                ),
                0.,
                ROW_LINE,
            );
            let mut left = row_rect.left();
            for (k, width) in widths.iter().enumerate() {
                let cell_rect =
                    Rect::from_min_size(Pos2::new(left, row_rect.top()), Vec2::new(*width, ROW));
                cell(c, r, k, cell_rect, sel);
                if sel && focus_col == Some(k) {
                    c.ui.painter().with_clip_rect(body).rect_stroke(
                        cell_rect.shrink(1.),
                        0.,
                        Stroke::new(2_f32, FOCUS),
                        egui::StrokeKind::Middle,
                    );
                }
                left += width;
            }
            let _ = out;
        },
    )
}

/// Rows on a fixed pitch inside `body` (absolute), scrolled by whole rows
/// with the wheel and a classic scroll bar when they do not fit. When
/// `reveal` names a different row than last frame, it is scrolled into view
/// (a new or picked item). `row` paints one row in its absolute rectangle;
/// clicks are reported in the result and each row's rectangle is recorded as
/// `{key}_row_{n}`, the scroll bar's arrows as `{key}_scroll_up` and
/// `{key}_scroll_down`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rows_in(
    c: &mut Canvas,
    key: &str,
    body: Rect,
    pitch: f32,
    rows: usize,
    reveal: Option<usize>,
    row: &mut dyn FnMut(&mut Canvas, usize, Rect, &mut ListOut),
) -> ListOut {
    let mut out = ListOut::default();
    let visible = (body.height() / pitch).floor().max(1.) as usize;
    let scroll_id = c.id(key).with("scroll");
    let reveal_id = c.id(key).with("reveal");
    let max_first = rows.saturating_sub(visible);
    let mut first =
        c.ui.ctx()
            .data(|d| d.get_temp::<usize>(scroll_id))
            .unwrap_or(0)
            .min(max_first);
    let last_reveal =
        c.ui.ctx()
            .data(|d| d.get_temp::<Option<usize>>(reveal_id))
            .flatten();
    if reveal != last_reveal {
        if let Some(r) = reveal.filter(|r| *r < rows) {
            if r < first {
                first = r;
            } else if r >= first + visible {
                first = (r + 1 - visible).min(max_first);
            }
        }
        c.ui.ctx().data_mut(|d| d.insert_temp(reveal_id, reveal));
    }
    if c.ui.rect_contains_pointer(body) {
        let wheel = c.ui.ctx().input(|i| i.smooth_scroll_delta.y);
        if wheel.abs() >= 1. {
            let step = (wheel / pitch).round() as i64;
            let step = if step == 0 {
                wheel.signum() as i64
            } else {
                step
            };
            first = (first as i64 - step).clamp(0, max_first as i64) as usize;
        }
    }
    let bar = rows > visible;
    let width = body.width() - if bar { SCROLLBAR } else { 0. };
    // The row cut by the bottom edge is drawn too; everything a row paints,
    // its cell text included, is clipped to the body so nothing shows
    // below the list.
    let clip = c.ui.clip_rect();
    c.ui.set_clip_rect(clip.intersect(body));
    for r in first..rows.min(first + visible + 1) {
        let row_top = body.top() + pitch * (r - first) as f32;
        let row_rect =
            Rect::from_min_size(Pos2::new(body.left(), row_top), Vec2::new(width, pitch));
        let hit = row_rect.intersect(body);
        let response =
            c.ui.interact(hit, c.id(key).with(("row", r)), Sense::click());
        c.act(&format!("{key}_row_{r}"), hit);
        if response.clicked() {
            out.clicked = Some(r);
        }
        if response.double_clicked() {
            out.double = Some(r);
        }
        row(c, r, row_rect, &mut out);
    }
    c.ui.set_clip_rect(clip);
    if bar {
        let track = Rect::from_min_max(Pos2::new(body.right() - SCROLLBAR, body.top()), body.max);
        c.ui.painter().rect_filled(track, 0., FACE);
        let up = Rect::from_min_size(track.min, Vec2::splat(SCROLLBAR));
        let down = Rect::from_min_size(
            Pos2::new(track.left(), track.bottom() - SCROLLBAR),
            Vec2::splat(SCROLLBAR),
        );
        for (i, arrow) in [up, down].into_iter().enumerate() {
            c.act(
                &format!("{key}_scroll_{}", if i == 0 { "up" } else { "down" }),
                arrow,
            );
            let local = Rect::from_min_size(arrow.min - c.origin.to_vec2(), arrow.size());
            c.fill(
                local.left(),
                local.top(),
                local.width(),
                local.height(),
                FACE,
            );
            c.raised(local.left(), local.top(), local.width(), local.height());
            let m = arrow.center();
            let pts = if i == 0 {
                vec![
                    Pos2::new(m.x - 4., m.y + 2.),
                    Pos2::new(m.x + 4., m.y + 2.),
                    Pos2::new(m.x, m.y - 2.),
                ]
            } else {
                vec![
                    Pos2::new(m.x - 4., m.y - 2.),
                    Pos2::new(m.x + 4., m.y - 2.),
                    Pos2::new(m.x, m.y + 2.),
                ]
            };
            c.ui.painter()
                .add(egui::Shape::convex_polygon(pts, INK, Stroke::NONE));
            if c.ui
                .interact(arrow, c.id(key).with(("arrow", i)), Sense::click())
                .clicked()
            {
                first = if i == 0 {
                    first.saturating_sub(1)
                } else {
                    (first + 1).min(max_first)
                };
            }
        }
        let span = (track.height() - 2. * SCROLLBAR).max(1.);
        let thumb_h = (span * visible as f32 / rows as f32).max(8.);
        let thumb_top =
            track.top() + SCROLLBAR + (span - thumb_h) * first as f32 / max_first.max(1) as f32;
        let thumb = Rect::from_min_size(
            Pos2::new(track.left(), thumb_top),
            Vec2::new(SCROLLBAR, thumb_h),
        );
        let local = Rect::from_min_size(thumb.min - c.origin.to_vec2(), thumb.size());
        c.fill(
            local.left(),
            local.top(),
            local.width(),
            local.height(),
            FACE,
        );
        c.raised(local.left(), local.top(), local.width(), local.height());
        let drag = c.ui.interact(thumb, c.id(key).with("thumb"), Sense::drag());
        if let Some(pointer) = drag.interact_pointer_pos().filter(|_| drag.dragged()) {
            let fraction = ((pointer.y - track.top() - SCROLLBAR - thumb_h / 2.)
                / (span - thumb_h).max(1.))
            .clamp(0., 1.);
            first = (fraction * max_first as f32).round() as usize;
        }
    }
    c.ui.ctx().data_mut(|d| d.insert_temp(scroll_id, first));
    out
}

/// Cell text 3 px in, vertically centred, clipped to the cell.
pub(crate) fn cell_text(c: &Canvas, rect: Rect, s: &str, selected: bool, x_pad: f32) {
    let color = if selected { Color32::WHITE } else { INK };
    let galley =
        c.ui.painter()
            .layout_no_wrap(s.to_owned(), dialog_font(), color);
    let pos = Pos2::new(
        rect.left() + x_pad,
        (rect.center().y - galley.size().y / 2.).round(),
    );
    c.ui.painter()
        .with_clip_rect(rect.shrink2(Vec2::new(1., 0.)))
        .galley(pos, galley, color);
}

/// A selectable row cell in a flow-layout list (the If / Then / Else
/// cells of Modify Global Change): the kit list's row
/// height, selection color and cell text, `width` wide.
pub(crate) fn flow_row_cell(
    ui: &mut egui::Ui,
    text: &str,
    selected: bool,
    width: f32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, ROW), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, 0., SELECTED);
    }
    ui.painter().text(
        rect.left_center() + Vec2::new(3., 0.),
        Align2::LEFT_CENTER,
        text,
        dialog_font(),
        if selected { Color32::WHITE } else { INK },
    );
    response
}

/// A wizard page's title: Arial Bold 21 px, top-left at client (x, y).
pub(crate) fn wizard_title(c: &Canvas, x: f32, y: f32, title: &str) {
    c.ui.painter().text(
        c.origin + Vec2::new(x, y),
        Align2::LEFT_TOP,
        title,
        FontId::new(21., egui::FontFamily::Name("Arial Bold".into())),
        INK,
    );
}

/// The button a wizard's bottom row reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WizardNav {
    Back,
    Next,
    Finish,
    Cancel,
}

/// A wizard page on a `w` x `h` client: the #D0D0D0 client, the page panel
/// lowered at 10,10 down to client y `panel_bottom`, and the page's title
/// ([`wizard_title`]) at its top left. The bottom row is [`wizard_buttons`].
pub(crate) fn wizard_page(c: &Canvas, w: f32, h: f32, panel_bottom: f32, title: &str) {
    c.fill(0., 0., w, h, STRIP);
    c.fill(10., 10., w - 15., panel_bottom - 10., FACE);
    c.lowered(10., 10., w - 15., panel_bottom - 10.);
    wizard_title(c, 12., 12., title);
}

/// Client x of a wizard's Cancel button: alone at the bottom left, under
/// the page panel's left edge, as every reference wizard has it.
pub(crate) const WIZARD_CANCEL_X: f32 = 10.;

/// A wizard's bottom row ending at client x `right`, top at `y`: Cancel
/// alone at the left ([`WIZARD_CANCEL_X`]), then Prev, Next and the
/// committing Finish at the right; keys `{salt}_back`, `{salt}_next`,
/// `{salt}_finish` and `{salt}_cancel`.
pub(crate) fn wizard_buttons(
    c: &mut Canvas,
    salt: &str,
    right: f32,
    y: f32,
    (back, next, finish): (bool, bool, bool),
) -> Option<WizardNav> {
    let keys = ["back", "next", "finish", "cancel"].map(|k| format!("{salt}_{k}"));
    let entries = [
        Entry::Button(&keys[0], "&Prev", back, false),
        Entry::Button(&keys[1], "&Next", next, false),
        Entry::Button(&keys[2], "Finish", finish, false),
    ];
    let mut clicked = None;
    let x = WIZARD_CANCEL_X;
    if button(c, &keys[3], x, y, BUTTON.x, BUTTON.y, "Cancel", true) {
        clicked = Some(keys[3].clone());
    }
    let row = button_row(c, right, y, &entries);
    let clicked = row.clicked.or(clicked)?;
    [
        WizardNav::Back,
        WizardNav::Next,
        WizardNav::Finish,
        WizardNav::Cancel,
    ]
    .into_iter()
    .zip(&keys)
    .find(|(_, k)| **k == clicked)
    .map(|(nav, _)| nav)
}

// ------------------------------------------------------------ color picker

/// The 48 basic colors of the color grid.
const BASIC_COLORS: [u32; 48] = [
    0xFF8080, 0xFFFF80, 0x80FF80, 0x00FF80, 0x80FFFF, 0x0080FF, 0xFF80C0, 0xFF80FF, //
    0xFF0000, 0xFFFF00, 0x80FF00, 0x00FF40, 0x00FFFF, 0x0080C0, 0x8080C0, 0xFF00FF, //
    0x804040, 0xFF8040, 0x00FF00, 0x008080, 0x004080, 0x8080FF, 0x800040, 0xFF0080, //
    0x800000, 0xFF8000, 0x008000, 0x008040, 0x0000FF, 0x0000A0, 0x800080, 0x8000FF, //
    0x400000, 0x804000, 0x004000, 0x004040, 0x000080, 0x000040, 0x400040, 0x400080, //
    0x000000, 0x808000, 0x808040, 0x808080, 0x408080, 0xC0C0C0, 0x400040, 0xFFFFFF, //
];

/// The one color picker: a swatch of `size` that drops down the 48 basic
/// colors; a pick replaces `color`. In a fixed-layout dialog place it with
/// [`swatch`].
pub(crate) fn color_button(
    ui: &mut egui::Ui,
    id: &str,
    color: &mut Rgb,
    size: Vec2,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 0., color.color());
    p.rect_stroke(rect, 0., Stroke::new(1_f32, INK), egui::StrokeKind::Inside);
    let _ = id;
    let mut picked = None;
    let popup = egui::Popup::from_toggle_button_response(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    let popup_id = popup.get_id();
    popup.show(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(3.);
        egui::Grid::new(("basic_colors", popup_id)).show(ui, |ui| {
            for (k, value) in BASIC_COLORS.iter().enumerate() {
                let c = Rgb((value >> 16) as u8, (value >> 8) as u8, *value as u8);
                let (cell, r) = ui.allocate_exact_size(Vec2::new(18., 14.), Sense::click());
                ui.painter().rect_filled(cell, 0., c.color());
                ui.painter().rect_stroke(
                    cell,
                    0.,
                    Stroke::new(1_f32, Color32::from_gray(if c == *color { 0 } else { 160 })),
                    egui::StrokeKind::Inside,
                );
                if r.clicked() {
                    picked = Some(c);
                }
                if k % 8 == 7 {
                    ui.end_row();
                }
            }
        });
    });
    if let Some(c) = picked {
        *color = c;
        egui::Popup::close_id(ui.ctx(), popup_id);
    }
    response
}

/// [`color_button`] at client (x, y, w, h), recorded as `key`. Returns
/// whether the color changed.
pub(crate) fn swatch(
    c: &mut Canvas,
    key: &str,
    (x, y, w, h): (f32, f32, f32, f32),
    color: &mut Rgb,
    enabled: bool,
) -> bool {
    let rect = c.r(x, y, w, h);
    let before = *color;
    let mut ui = c.ui.new_child(egui::UiBuilder::new().max_rect(rect));
    if !enabled {
        ui.disable();
    }
    let response = color_button(&mut ui, key, color, rect.size());
    c.act(key, response.rect);
    *color != before
}

/// A [`Canvas`] over a `size` block allocated in a flow layout, so a
/// `chrome::dialog_window` body places kit controls (the button column,
/// headings, tab lists) at client coordinates of that block. Returns the
/// content's value and the control rectangles it recorded.
pub(crate) fn flow_canvas<R>(
    ui: &mut egui::Ui,
    salt: &'static str,
    size: Vec2,
    content: impl FnOnce(&mut Canvas) -> R,
) -> (R, Vec<(String, Rect)>) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    let mut canvas = Canvas {
        ui: &mut child,
        origin: rect.min,
        salt,
        actions: vec![],
    };
    let inner = content(&mut canvas);
    (inner, canvas.actions)
}

/// Names the button a [`button_column`] or [`button_row`] drew as `key`
/// the dialog's default: Enter runs it and it carries the default frame.
/// Call it after the column or row, for a default other than OK, Next or
/// Finish (which Enter runs anyway).
pub(crate) fn mark_default(c: &Canvas, key: &str) {
    if let Some(response) = c.ui.ctx().read_response(c.id(key)) {
        crate::dialog_keys::mark_default(c.ui, &response);
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_order_accepts_the_reference_and_names_what_breaks_it() {
        assert_eq!(
            order_problem(&["OK", "Cancel", "Apply", "Delete", "Help"], false),
            None
        );
        assert_eq!(
            order_problem(&["Close", "Add", "Delete", "&Help"], false),
            None
        );
        assert_eq!(
            order_problem(&["Schedule", "Cancel", "View Log", "Help"], false),
            None
        );
        assert_eq!(
            order_problem(&["Cancel", "OK"], false),
            Some("OK after Cancel".into())
        );
        assert_eq!(
            order_problem(&["OK", "Cancel", "Help", "Work"], false),
            Some("Help before Work".into())
        );
        assert_eq!(order_problem(&["Apply", "Cancel", "Help"], false), None);
        // A row reads Help first, then the same order.
        assert_eq!(order_problem(&["Help", "Close"], true), None);
        assert_eq!(order_problem(&["Help", "OK", "Cancel"], true), None);
        assert_eq!(
            order_problem(&["Close", "Help"], true),
            Some("Help after Close".into())
        );
        assert_eq!(
            order_problem(&["Help", "Cancel", "OK"], true),
            Some("OK after Cancel".into())
        );
        assert!(is_standard_caption("&Cancel"));
        assert!(!is_standard_caption("Delete"));
    }

    /// The color picker and the drop-down are written once, here; and the
    /// dialog files moved onto the kit build no control from raw egui
    /// widgets. A second copy, or an egui check box slipped back into a
    /// converted dialog, fails here.
    #[test]
    fn controls_are_written_once_and_converted_dialogs_use_them() {
        let src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        fn visit(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    visit(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    out.push((path.to_string_lossy().replace('\\', "/"), text));
                }
            }
        }
        let mut files = vec![];
        visit(&src, &mut files);
        let mut hits = vec![];
        for (path, text) in &files {
            if path.ends_with("/dialog_kit.rs") {
                continue;
            }
            for copy in ["BASIC_COLORS", "fn color_button(", "struct DropList"] {
                if text.contains(copy) {
                    hits.push(format!("{path}: second copy of {copy}"));
                }
            }
        }
        // Dialog files whose every control comes from the kit.
        const CONVERTED: &[&str] = &[
            "/gantt/dialogs.rs",
            "/baselines/dialogs.rs",
            "/admin_prefs.rs",
            "/user_prefs.rs",
            "/print/dialog.rs",
            "/print/ts_picker.rs",
            "/compare/mod.rs",
            "/spell/mod.rs",
            "/loops/dialog.rs",
            "/open_set/dialogs.rs",
            "/critical_path.rs",
            "/estimation.rs",
            "/share_dictionaries.rs",
            "/layout_opts/dialogs.rs",
            "/panes/network.rs",
            "/panes/trace.rs",
            "/panes/usage_fields.rs",
            "/toolbars.rs",
            "/external_apps.rs",
            "/new_project.rs",
            "/progress_tools.rs",
            "/views/resource_merge.rs",
            "/views/project_ops.rs",
            "/reports/batch_run.rs",
            "/reports/checker_ui.rs",
            "/reports/groups_ui.rs",
            "/reports/progress_ui.rs",
            "/reports/reports_view.rs",
            "/reports/rpt_editor.rs",
            "/reports/rpt_io.rs",
            "/reports/rpt_props.rs",
            "/reports/rpt_run.rs",
            "/reports/wizard_ui.rs",
            "/ai_agents/skills_ui.rs",
        ];
        const RAW: &[&str] = &[
            "egui::TextEdit",
            "egui::DragValue",
            "egui::Slider",
            "egui::ComboBox",
            "egui::Checkbox",
            "egui::Grid",
            "Frame::group",
            "ui.checkbox(",
            "ui.radio",
            "ui.button(",
            "selectable_label(",
            "command_button(",
            "dialog_button(",
            "plain_window(",
            "dialog_window(",
        ];
        for (path, text) in &files {
            if !CONVERTED.iter().any(|c| path.ends_with(c)) {
                continue;
            }
            for raw in RAW {
                if text.contains(raw) {
                    hits.push(format!("{path}: {raw}"));
                }
            }
        }
        assert!(hits.is_empty(), "{}", hits.join("\n"));
    }

    #[test]
    fn mnemonics_survive_the_round_trip_to_the_painter() {
        assert_eq!(with_mnemonic("Help", Some(0)), "&Help");
        assert_eq!(with_mnemonic("Fonts & Colors", Some(8)), "Fonts && &Colors");
        assert_eq!(
            chrome::mnemonic(&with_mnemonic("A & B", Some(4))),
            ("A & B".into(), Some(4))
        );
        assert_eq!(with_mnemonic("OK", None), "OK");
    }

    /// The column and the row put standard buttons at the standard size and
    /// pitch, and the column starts at the strip's left edge.
    #[test]
    fn column_and_row_place_standard_buttons_on_the_standard_pitch() {
        let ctx = egui::Context::default();
        chrome::configure(&ctx);
        let mut column = vec![];
        let mut row = vec![];
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let origin = ui.min_rect().min;
                let mut c = Canvas {
                    ui,
                    origin,
                    salt: "kit_test",
                    actions: vec![],
                };
                button_column(
                    &mut c,
                    500.,
                    &[
                        Entry::ok("t_ok"),
                        Entry::cancel("t_cancel"),
                        Entry::apply("t_apply", true),
                        Entry::Button("t_delete", "Delete", true, false),
                        Entry::help("t_help"),
                    ],
                );
                column = c.actions.clone();
                c.actions.clear();
                button_row(
                    &mut c,
                    500.,
                    300.,
                    &[
                        Entry::ok("r_ok"),
                        Entry::cancel("r_cancel"),
                        Entry::help("r_help"),
                    ],
                );
                row = c.actions.clone();
                for r in column.iter().chain(&row) {
                    let local = r.1.translate(-origin.to_vec2());
                    assert_eq!(local.size(), BUTTON, "{}", r.0);
                }
                let tops: Vec<f32> = column.iter().map(|r| r.1.top() - origin.y).collect();
                assert_eq!(tops, vec![6., 36., 76., 106., 146.]);
                assert!(column.iter().all(|r| r.1.left() - origin.x == 500. - 112.));
                // The row draws Help first, then OK and Cancel, all at the
                // plain gap.
                let names: Vec<&str> = row.iter().map(|r| r.0.as_str()).collect();
                assert_eq!(names, vec!["r_help", "r_ok", "r_cancel"]);
                let lefts: Vec<f32> = row.iter().map(|r| r.1.left() - origin.x).collect();
                assert_eq!(lefts, vec![500. - 334., 500. - 221., 500. - 108.]);
            });
        });
    }
}
