//! Dialog keys: Enter runs the default button of the dialog on top, and
//! Alt+letter presses the button underlined with that letter.
//!
//! Every dialog push button notes itself here as it draws (caption with its
//! `&` mnemonic, its widget id and rectangle). The keys of a frame are read
//! before the dialogs draw, against the notes of the frame before; a button
//! is pressed by giving it the keyboard focus together with an Enter press,
//! which egui reads as a click on that widget.
//!
//! The default button is the one a dialog names with [`mark_default`], else
//! `OK`, else the first button of the column that is not Cancel, Help, No or
//! a step back. A multi-line field, an open drop-down list or a focused
//! button keeps Enter for itself.
use super::*;
use egui::{Event, Id, LayerId, Modifiers};

#[derive(Clone)]
pub(crate) struct ButtonNote {
    pub layer: LayerId,
    pub id: Id,
    /// The caption without its `&`.
    pub label: String,
    pub mnemonic: Option<char>,
    pub rect: Rect,
    pub enabled: bool,
}

#[derive(Clone, Default)]
pub(crate) struct Notes {
    pass: u64,
    buttons: Vec<ButtonNote>,
    /// Layers whose dialog named its default button, and which.
    defaults: Vec<(LayerId, Id)>,
    /// A field that keeps Enter and has the keyboard this pass.
    multiline: Option<Id>,
    /// Layers whose dialog leaves Enter idle (a confirmation that must be
    /// answered with a click).
    idle: Vec<LayerId>,
    /// The buttons Enter ran in each dialog of the pass before: the kit
    /// frames them as the default button.
    prev_defaults: Vec<Id>,
}

fn notes_id() -> Id {
    Id::new("openxer_dialog_key_notes")
}

fn enter_handled_id() -> Id {
    Id::new("openxer_dialog_enter_handled_pass")
}

fn note_enter_handled(ctx: &egui::Context) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(enter_handled_id(), pass));
}

/// Whether `dialog_keys` already routed this pass's Enter press to the
/// topmost dialog's default or focused button.
pub(crate) fn enter_handled(ctx: &egui::Context) -> bool {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|d| d.get_temp::<u64>(enter_handled_id())) == Some(pass)
}

fn with_notes<R>(ctx: &egui::Context, f: impl FnOnce(&mut Notes) -> R) -> R {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| {
        let notes = d.get_temp_mut_or_default::<Notes>(notes_id());
        if notes.pass != pass {
            let mut layers: Vec<LayerId> = vec![];
            for b in &notes.buttons {
                if !layers.contains(&b.layer) {
                    layers.push(b.layer);
                }
            }
            let defaults = layers
                .iter()
                .filter_map(|l| default_button(notes, *l).map(|b| b.id))
                .collect();
            notes.prev_defaults = defaults;
            notes.pass = pass;
            notes.buttons.clear();
            notes.defaults.clear();
            notes.multiline = None;
            notes.idle.clear();
        }
        f(notes)
    })
}

/// A dialog push button was drawn. `caption` may carry an `&` mnemonic.
pub(crate) fn note_button(ui: &egui::Ui, response: &egui::Response, caption: &str, enabled: bool) {
    let (label, index) = chrome::mnemonic(caption);
    let mnemonic = index
        .and_then(|i| label.chars().nth(i))
        .and_then(|c| c.to_lowercase().next());
    let note = ButtonNote {
        layer: ui.layer_id(),
        id: response.id,
        label,
        mnemonic,
        rect: response.rect,
        enabled,
    };
    with_notes(ui.ctx(), |n| n.buttons.push(note));
}

/// The dialog on this layer names this button as its default.
pub(crate) fn mark_default(ui: &egui::Ui, response: &egui::Response) {
    let layer = ui.layer_id();
    with_notes(ui.ctx(), |n| n.defaults.push((layer, response.id)));
}

/// The button with this widget id is the one Enter ran in its dialog last
/// pass: the push button painter frames it as the default.
pub(crate) fn is_default(ui: &egui::Ui, id: Id) -> bool {
    with_notes(ui.ctx(), |n| n.prev_defaults.contains(&id))
}

/// Captions and rectangles of the push buttons drawn on `layer` this pass,
/// in drawing order, for the sweep.
#[cfg(test)]
pub(crate) fn layer_buttons(ctx: &egui::Context, layer: LayerId) -> Vec<(String, Rect)> {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|d| d.get_temp::<Notes>(notes_id()))
        .filter(|n| n.pass == pass)
        .map(|n| {
            n.buttons
                .iter()
                .filter(|b| b.layer == layer)
                .map(|b| (b.label.clone(), b.rect))
                .collect()
        })
        .unwrap_or_default()
}

/// Enter does nothing on this layer's dialog.
pub(crate) fn mark_enter_idle(ui: &egui::Ui) {
    let layer = ui.layer_id();
    with_notes(ui.ctx(), |n| n.idle.push(layer));
}

/// A field that keeps Enter is drawn (a multi-line field, or an editor inside
/// a list that commits on Enter): while it has the keyboard it keeps Enter.
pub(crate) fn note_keeps_enter(response: &egui::Response) {
    if response.has_focus() {
        with_notes(&response.ctx, |n| n.multiline = Some(response.id));
    }
}

/// [`note_keeps_enter`] for a field known by its widget id.
pub(crate) fn note_keeps_enter_id(ctx: &egui::Context, id: Id) {
    if ctx.memory(|m| m.has_focus(id)) {
        with_notes(ctx, |n| n.multiline = Some(id));
    }
}

/// Notes of the pass before this one (or of this one, once drawn).
fn last_notes(ctx: &egui::Context) -> Notes {
    let now = ctx.cumulative_pass_nr();
    ctx.data(|d| d.get_temp::<Notes>(notes_id()))
        .filter(|n| n.pass + 1 >= now)
        .unwrap_or_default()
}

/// The topmost dialog layer drawn last pass.
fn top_layer(ctx: &egui::Context) -> Option<LayerId> {
    let layers = crate::dialog_kit::open_dialog_layers(ctx);
    ctx.memory(|m| {
        m.layer_ids()
            .filter(|l| layers.contains(l) && m.areas().is_visible(l))
            .last()
    })
}

fn excluded(label: &str) -> bool {
    matches!(
        label.trim_end_matches('.').trim().to_lowercase().as_str(),
        "cancel" | "help" | "no" | "prev" | "back" | ""
    )
}

/// Every push button of the layer that noted itself.
fn buttons_in(notes: &Notes, layer: LayerId) -> Vec<ButtonNote> {
    notes
        .buttons
        .iter()
        .filter(|b| b.layer == layer)
        .cloned()
        .collect()
}

/// The button Enter runs in this layer: the one the dialog marked, else
/// OK, Next, Finish, else the first enabled button of the command column
/// (the buttons sharing the rightmost edge).
pub(crate) fn default_button(notes: &Notes, layer: LayerId) -> Option<ButtonNote> {
    if notes.idle.contains(&layer) {
        return None;
    }
    let here = buttons_in(notes, layer);
    if let Some((_, id)) = notes.defaults.iter().rev().find(|(l, _)| *l == layer) {
        if let Some(b) = here.iter().find(|b| b.id == *id) {
            // A named default that cannot be pressed leaves Enter idle.
            return b.enabled.then(|| b.clone());
        }
    }
    let live: Vec<&ButtonNote> = here
        .iter()
        .filter(|b| b.enabled && !excluded(&b.label))
        .collect();
    for name in ["ok", "next", "finish"] {
        if let Some(b) = live.iter().find(|b| b.label.eq_ignore_ascii_case(name)) {
            return Some((*b).clone());
        }
    }
    let edge = live.iter().map(|b| b.rect.right()).fold(f32::MIN, f32::max);
    live.iter()
        .filter(|b| b.rect.right() >= edge - 12.)
        .min_by(|a, b| {
            (a.rect.top(), a.rect.left())
                .partial_cmp(&(b.rect.top(), b.rect.left()))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|b| (*b).clone())
}

/// The default button's caption of the dialog on top, for the sweep.
#[cfg(test)]
pub(crate) fn top_default_label(ctx: &egui::Context) -> Option<String> {
    let layer = top_layer(ctx)?;
    default_button(&last_notes(ctx), layer).map(|b| b.label)
}

/// Captions of the enabled buttons (but Cancel, Help and the like) of the
/// dialog on top, and how many dialogs are up, for the sweep.
#[cfg(test)]
pub(crate) fn top_live_buttons(ctx: &egui::Context) -> (Vec<String>, usize) {
    let layers = crate::dialog_kit::open_dialog_layers(ctx).len();
    let Some(layer) = top_layer(ctx) else {
        return (vec![], layers);
    };
    let live = buttons_in(&last_notes(ctx), layer)
        .into_iter()
        .filter(|b| b.enabled && !excluded(&b.label))
        .map(|b| b.label)
        .collect();
    (live, layers)
}

impl App {
    /// Reads this frame's dialog keys. Runs after the menu keys and before
    /// the main window's accelerators.
    pub(crate) fn dialog_keys(&mut self, ctx: &egui::Context) {
        let Some(layer) = top_layer(ctx) else { return };
        if egui::Popup::is_any_open(ctx) || crate::kit_window::combo_open(ctx) {
            return;
        }
        let notes = last_notes(ctx);
        let buttons = buttons_in(&notes, layer);
        // Alt+letter presses the button underlined with it.
        let alt: Vec<(Key, char)> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } if modifiers.alt && !modifiers.command => {
                        let name = key.name();
                        let mut chars = name.chars();
                        let c = chars.next()?;
                        chars
                            .next()
                            .is_none()
                            .then(|| (*key, c.to_lowercase().next().unwrap_or(c)))
                    }
                    _ => None,
                })
                .collect()
        });
        for (key, c) in alt {
            let hit = buttons.iter().find(|b| b.enabled && b.mnemonic == Some(c));
            if let Some(button) = hit {
                ctx.input_mut(|i| i.consume_key(Modifiers::ALT, key));
                menu_keys::press(ctx, button.id);
                return;
            }
        }
        let enter = ctx.input(|i| {
            i.events.iter().any(|e| {
                matches!(
                    e,
                    Event::Key {
                        key: Key::Enter,
                        pressed: true,
                        modifiers,
                        ..
                    } if modifiers.is_none()
                )
            })
        });
        if !enter {
            return;
        }
        let focused = ctx.memory(|m| m.focused());
        if focused.is_some() && focused == notes.multiline {
            return;
        }
        // A button that has the keyboard answers Enter itself.
        if focused.is_some_and(|f| buttons.iter().any(|b| b.id == f)) {
            note_enter_handled(ctx);
            return;
        }
        if let Some(button) = default_button(&notes, layer) {
            note_enter_handled(ctx);
            menu_keys::press(ctx, button.id);
        }
    }
}
