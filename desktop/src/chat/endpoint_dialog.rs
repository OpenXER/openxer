//! Set Up... of the API key or local model route: the preset, the address
//! (editable for Custom), the API key (masked, hidden for local presets),
//! the model (typed, or picked from the server's list), Test with its
//! plain result line, and the privacy, key caution, preset and Ask only
//! notes from `openxer_chat::compat`. One connection is kept: its plain
//! settings in the user's preferences, its key only in the credential
//! store, never drawn and never copied.
use super::backend::EndpointCheck;
use crate::dialog_kit::{self as kit};
use crate::App;
use eframe::egui::{self, Align, Vec2};
use openxer_chat::compat::{self as cc, Connection};
use std::sync::mpsc::{Receiver, TryRecvError};

/// The user preference holding the connection (no key).
pub(crate) const PREF_KEY: &str = "chat_endpoint_v1";
/// The dialog's kit name and title.
pub(crate) const SALT: &str = "chat_endpoint";
pub(crate) const TITLE: &str = "API Key or Local Model";

/// The dialog while open.
pub(crate) struct EndpointDialog {
    pub conn: Connection,
    /// The key being typed (empty: keep the stored one).
    pub key: String,
    pub key_stored: bool,
    pub models: Vec<String>,
    pub test: Option<Receiver<EndpointCheck>>,
    pub result: Option<String>,
    /// OK or Remove Key working on a worker thread (the credential store
    /// is never written on a frame); `true` once it closes the dialog.
    pub saving: Option<(Receiver<Result<(), String>>, bool)>,
}

fn preset_index(conn: &Connection) -> usize {
    cc::presets()
        .iter()
        .position(|p| p.preset == conn.preset)
        .unwrap_or(0)
}

/// The notes under the fields: privacy, the key caution when a key is
/// used (`key`: one is typed or stored), the preset's own note, Ask only
/// for a local model.
pub(crate) fn notes(conn: &Connection, key: bool) -> Vec<String> {
    let mut out = vec![cc::privacy_text(conn)];
    if conn.needs_key || key {
        out.push(cc::KEY_CAUTION.to_string());
    }
    if let Some(note) = conn.preset.info().note {
        out.push(note.to_string());
    }
    if conn.ask_only() {
        out.push(cc::ASK_ONLY_NOTE.to_string());
    }
    out
}

impl App {
    /// Reads the stored connection into the backend (when Chat first opens).
    pub(crate) fn restore_endpoint(&mut self) {
        let conn = self
            .store
            .preference(PREF_KEY)
            .ok()
            .flatten()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| Connection::from_json(&v));
        if conn.is_some() {
            let _ = self.chat.backend.set_endpoint(conn, None);
        }
    }

    /// Set Up...: the stored connection, or a new one from the first preset.
    pub(crate) fn open_endpoint_dialog(&mut self) {
        self.ensure_chat_backend();
        let conn = self
            .chat
            .backend
            .endpoint()
            .unwrap_or_else(|| Connection::new(cc::presets()[0].preset));
        let key_stored = self.chat.backend.endpoint_has_key(&conn);
        self.chat.endpoint = Some(EndpointDialog {
            conn,
            key: String::new(),
            key_stored,
            models: vec![],
            test: None,
            result: None,
            saving: None,
        });
    }

    /// The dialog, each frame.
    pub(crate) fn chat_endpoint_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.chat.endpoint.take() else {
            return;
        };
        if let Some((rx, close)) = &d.saving {
            match rx.try_recv() {
                Ok(Ok(())) if *close => {
                    // Saved: the plain settings go to the preferences.
                    let text = d.conn.to_json().to_string();
                    if let Err(e) = self.store.set_preference(PREF_KEY, &text) {
                        self.status = e;
                    }
                    // A model on this PC starts in Ask only.
                    if d.conn.ask_only() {
                        self.chat.mode = super::backend::Mode::Ask;
                    }
                    return;
                }
                Ok(Ok(())) => {
                    d.saving = None;
                    d.key.clear();
                    d.result = Some("The key was removed.".into());
                }
                Ok(Err(e)) => {
                    d.saving = None;
                    d.result = Some(e);
                }
                Err(TryRecvError::Disconnected) => d.saving = None,
                Err(TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(50));
                }
            }
        }
        // A key is shown as stored only for the address it was entered for.
        d.key_stored = self.chat.backend.endpoint_has_key(&d.conn);
        let idle = d.saving.is_none();
        if let Some(rx) = &d.test {
            match rx.try_recv() {
                Ok(check) => {
                    d.result = Some(check.line);
                    d.models = check.models;
                    d.test = None;
                }
                Err(TryRecvError::Disconnected) => d.test = None,
                Err(TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
            }
        }
        const W: f32 = 600.;
        let main = W - kit::STRIP_WIDTH;
        let text_w = main - 20.;
        let info = d.conn.preset.info();
        let show_key = info.needs_key || !info.local;
        let labels: Vec<&str> = cc::presets().iter().map(|p| p.label).collect();
        // The notes are measured on the first pass; their height sets the
        // dialog's (a frame late at most, as the text barely changes).
        let id = egui::Id::new((SALT, "notes_h"));
        let notes_h: f32 = ctx.data(|m| m.get_temp(id)).unwrap_or(90.);
        let notes_y = 182.;
        let h = (notes_y + notes_h + 12.).max(260.);
        let mut pick = preset_index(&d.conn);
        let mut model_pick = usize::MAX;
        let (clicked, closed, actions) = kit::modal(ctx, SALT, TITLE, Vec2::new(W, h), |c| {
            c.fill(0., 0., main, h, kit::FACE);
            let lx = 10.;
            let fx = 90.;
            let fw = main - fx - 10.;
            c.label(lx, 12., 21., "Preset", true);
            let items: Vec<(&str, bool)> = labels.iter().map(|l| (*l, true)).collect();
            c.combo(
                "chat_endpoint_preset",
                fx,
                12.,
                fw,
                21.,
                &items,
                &mut pick,
                true,
            );
            c.label(lx, 42., 21., "Address", true);
            if d.conn.preset == cc::Preset::Custom {
                c.edit(
                    "chat_endpoint_address",
                    fx,
                    42.,
                    fw,
                    21.,
                    &mut d.conn.base_url,
                    true,
                    Align::Min,
                );
            } else {
                kit::field(c, fx, 42., fw, 21., &d.conn.base_url, false);
            }
            if show_key {
                c.label(lx, 72., 21., "API key", true);
                c.edit_masked(
                    "chat_endpoint_key",
                    (fx, 72., fw, 21.),
                    &mut d.key,
                    true,
                    Align::Min,
                    true,
                );
                if d.key_stored && d.key.is_empty() {
                    kit::secondary_text(
                        c,
                        fx,
                        96.,
                        "A key is stored. Type a new one to replace it.",
                    );
                }
            }
            c.label(lx, 114., 21., "Model", true);
            let model_w = if d.models.is_empty() { fw } else { 200. };
            c.edit(
                "chat_endpoint_model",
                fx,
                114.,
                model_w,
                21.,
                &mut d.conn.model,
                true,
                Align::Min,
            );
            if !d.models.is_empty() {
                let listed: Vec<(&str, bool)> =
                    d.models.iter().map(|m| (m.as_str(), true)).collect();
                let mut chosen = d
                    .models
                    .iter()
                    .position(|m| *m == d.conn.model)
                    .unwrap_or(0);
                if c.combo(
                    "chat_endpoint_models",
                    fx + model_w + 6.,
                    114.,
                    fw - model_w - 6.,
                    21.,
                    &listed,
                    &mut chosen,
                    true,
                ) {
                    model_pick = chosen;
                }
            }
            let test = kit::button(
                c,
                "chat_endpoint_test",
                fx,
                146.,
                kit::BUTTON.x,
                kit::BUTTON.y,
                "&Test",
                d.test.is_none(),
            );
            let line = if d.saving.is_some() {
                "Saving...".to_string()
            } else if d.test.is_some() {
                "Testing...".to_string()
            } else {
                d.result.clone().unwrap_or_default()
            };
            let line = crate::ai_agents::fit(c, &line, main - fx - kit::BUTTON.x - 20.);
            kit::text(c, fx + kit::BUTTON.x + 8., 151., &line, true);
            let mut y = notes_y;
            for note in notes(&d.conn, d.key_stored || !d.key.is_empty()) {
                for l in crate::ai_agents::wrap(c, &note, text_w) {
                    kit::secondary_text(c, lx, y, &l);
                    y += 15.;
                }
                y += 4.;
            }
            c.ui.ctx().data_mut(|m| m.insert_temp(id, y - notes_y));
            let column = kit::button_column(
                c,
                W,
                &[
                    kit::Entry::Button("chat_endpoint_ok", "OK", idle, false),
                    kit::Entry::cancel("chat_endpoint_cancel"),
                    kit::Entry::Button(
                        "chat_endpoint_remove_key",
                        "&Remove Key",
                        idle && d.key_stored,
                        true,
                    ),
                    kit::Entry::help("chat_endpoint_help"),
                ],
            );
            (column.clicked, test)
        });
        self.record_dialog_actions(actions);
        let (clicked, test) = clicked;
        if pick != preset_index(&d.conn) {
            let info = &cc::presets()[pick];
            d.conn.preset = info.preset;
            d.conn.name = info.label.to_string();
            d.conn.base_url = info.base_url.to_string();
            d.conn.needs_key = info.needs_key;
            d.models.clear();
            d.result = None;
        }
        // Another compatible server on this PC needs no key.
        // Another compatible server needs a key unless it is on this PC
        // and none is given.
        if d.conn.preset == cc::Preset::Custom {
            d.conn.needs_key = !d.conn.is_local() || !d.key.trim().is_empty() || d.key_stored;
        }
        if let Some(m) = d.models.get(model_pick) {
            d.conn.model = m.clone();
        }
        if test {
            let key = (!d.key.trim().is_empty()).then(|| openxer_chat::Secret::new(d.key.clone()));
            d.result = None;
            d.test = self.chat.backend.check_endpoint(d.conn.clone(), key);
        }
        match clicked.as_deref() {
            Some("chat_endpoint_ok") => {
                if let Err(e) = cc::check_address(&d.conn.base_url) {
                    d.result = Some(format!("The address cannot be used: {e}."));
                    self.chat.endpoint = Some(d);
                    return;
                }
                let key =
                    (!d.key.trim().is_empty()).then(|| openxer_chat::Secret::new(d.key.clone()));
                let rx = self.chat.backend.set_endpoint(Some(d.conn.clone()), key);
                d.saving = Some((rx, true));
                self.chat.endpoint = Some(d);
            }
            Some("chat_endpoint_remove_key") => {
                if let Some(rx) = self.chat.backend.remove_endpoint_key() {
                    d.saving = Some((rx, false));
                }
                self.chat.endpoint = Some(d);
            }
            Some("chat_endpoint_cancel") => {}
            Some("chat_endpoint_help") => {
                self.open_help_for(SALT);
                self.chat.endpoint = Some(d);
            }
            _ if closed => {}
            _ => self.chat.endpoint = Some(d),
        }
    }
}
