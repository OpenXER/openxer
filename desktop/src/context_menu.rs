//! The shared context-menu system. Every right-click menu in the app is
//! built here, in the native popup style, from one place:
//!
//! * A menu is built fresh inside the popup closure each time it is open,
//!   so enablement is always read from live state (never cached).
//! * [`Caps`] is the capability set of the view being clicked: whether it
//!   can edit, cut or paste, how many rows are selected, whether it has a
//!   Gantt chart, whether it has rows at all. Each view computes it and
//!   hands it to the builder.
//! * [`Menu`] adds items, check items, separators and submenus, records an
//!   action rectangle for each under `<prefix>_<key>` (for tests), and
//!   shows a disabled item with a hint when its command is not built.
//! * [`Entry`] and [`Spec`] describe a menu as plain data, for views that
//!   draw their rows where the application state is out of reach (the
//!   shared table of the WBS and Projects views).
//! * Keyboard: Shift+F10 and the Menu key open the focused view's menu at
//!   the selected row. egui has no key for the Menu key, so a Windows
//!   shim polls it ([`MenuKey`]).
use super::*;
use crate::chrome::{self, Icon, Item};
use egui::{Popup, PopupCloseBehavior};

/// What the view under the pointer can do right now. Computed per view and
/// per open; items read their enabled state from it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Caps {
    /// The data can be changed at all.
    pub can_edit: bool,
    /// The view has a Cut command.
    pub can_cut: bool,
    /// The clipboard holds something this view can paste.
    pub can_paste: bool,
    /// Rows selected in the view.
    pub selection_count: usize,
    /// The view draws a Gantt chart beside its table.
    pub view_has_gantt: bool,
    /// The view has at least one row.
    pub has_rows: bool,
}

impl Caps {
    /// At least one row is selected.
    pub fn has_selection(&self) -> bool {
        self.selection_count > 0
    }
    /// Two or more rows are selected.
    pub fn several(&self) -> bool {
        self.selection_count > 1
    }
    /// Cut applies: editable, cuttable and something selected.
    pub fn cut_enabled(&self) -> bool {
        self.can_edit && self.can_cut && self.has_selection()
    }
    /// Copy applies: something is selected.
    pub fn copy_enabled(&self) -> bool {
        self.has_selection()
    }
    /// Paste applies: editable and the clipboard holds something.
    pub fn paste_enabled(&self) -> bool {
        self.can_edit && self.can_paste
    }
    /// A command that changes the selected rows applies.
    pub fn edit_selection_enabled(&self) -> bool {
        self.can_edit && self.has_selection()
    }
}

/// Where the rectangles of drawn menu items wait until the frame ends.
fn rects_id() -> egui::Id {
    egui::Id::new("openxer_context_menu_rects")
}

/// Moves the action rectangles of the menu items drawn this frame into
/// `actions`, and this frame's entry log to where [`drawn`] reads it.
/// Called once at the end of every frame.
pub fn drain_rects(ctx: &egui::Context, actions: &mut BTreeMap<String, [f32; 4]>) {
    let drawn: Vec<(String, [f32; 4])> = ctx.data_mut(|d| {
        std::mem::take(d.get_temp_mut_or_default::<Vec<(String, [f32; 4])>>(rects_id()))
    });
    actions.extend(drawn);
    ctx.data_mut(|d| {
        let log = std::mem::take(d.get_temp_mut_or_default::<Vec<Drawn>>(log_id()));
        d.insert_temp(last_log_id(), log);
    });
}

/// One entry of a context menu as drawn, in drawing order: what a menu
/// shows, for the tests that compare a menu with the reference.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub struct Drawn {
    /// The menu's prefix (`ctx`, `gantt_menu`, `wbs_menu`, ...).
    pub prefix: String,
    /// Empty for a separator.
    pub key: String,
    /// The caption without its `&`; empty for a separator.
    pub label: String,
    pub enabled: bool,
    pub checked: bool,
    pub submenu: bool,
    pub separator: bool,
    /// 0 for the menu itself, 1 inside one of its submenus, and so on.
    pub depth: usize,
}

fn log_id() -> egui::Id {
    egui::Id::new("openxer_context_menu_log")
}

fn last_log_id() -> egui::Id {
    egui::Id::new("openxer_context_menu_log_last")
}

/// The context-menu entries the last finished frame drew, in order.
#[cfg_attr(not(test), allow(dead_code))]
pub fn drawn(ctx: &egui::Context) -> Vec<Drawn> {
    ctx.data(|d| d.get_temp::<Vec<Drawn>>(last_log_id()))
        .unwrap_or_default()
}

/// One menu item as plain data.
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub key: &'static str,
    pub label: &'static str,
    pub shortcut: &'static str,
    pub icon: Option<Icon>,
    pub enabled: bool,
    pub checked: bool,
    /// Hint shown over the item while it is disabled.
    pub hint: Option<&'static str>,
}

// Data menus, used by the WBS and Projects views.
impl Spec {
    pub fn new(key: &'static str, label: &'static str) -> Self {
        Self {
            key,
            label,
            shortcut: "",
            icon: None,
            enabled: true,
            checked: false,
            hint: None,
        }
    }
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
    /// The shortcut text drawn at the right (`Ctrl+X`).
    pub fn key(mut self, shortcut: &'static str) -> Self {
        self.shortcut = shortcut;
        self
    }
    fn item(&self) -> Item<'static> {
        let mut item = Item::new(self.label)
            .key(self.shortcut)
            .enabled(self.enabled)
            .checked(self.checked);
        item.icon = self.icon;
        item
    }
}

/// A menu as data: items, separators and submenus.
#[derive(Clone, Debug)]
pub enum Entry {
    Item(Spec),
    Separator,
    Submenu(Spec, Vec<Entry>),
}

impl From<Spec> for Entry {
    fn from(spec: Spec) -> Self {
        Entry::Item(spec)
    }
}

/// The items of an open context menu. Every item records its rectangle
/// under `<prefix>_<key>`.
pub struct Menu<'u> {
    ui: &'u mut egui::Ui,
    prefix: String,
    caps: Caps,
    depth: usize,
}

impl<'u> Menu<'u> {
    /// A menu over `ui` (a popup or submenu content ui).
    pub fn on(ui: &'u mut egui::Ui, prefix: &str) -> Self {
        Self {
            ui,
            prefix: prefix.to_string(),
            caps: Caps::default(),
            depth: 0,
        }
    }

    /// Notes one drawn entry in this frame's log ([`drawn`]).
    fn log(&self, key: &str, item: Option<&Item>, submenu: bool) {
        let entry = Drawn {
            prefix: self.prefix.clone(),
            key: key.to_string(),
            label: item.map_or(String::new(), |i| i.label.replace('&', "")),
            enabled: item.is_some_and(|i| i.enabled),
            checked: item.is_some_and(|i| i.checked),
            submenu,
            separator: item.is_none(),
            depth: self.depth,
        };
        self.ui.ctx().data_mut(|d| {
            d.get_temp_mut_or_default::<Vec<Drawn>>(log_id())
                .push(entry)
        });
    }

    pub fn with_caps(mut self, caps: Caps) -> Self {
        self.caps = caps;
        self
    }

    pub fn caps(&self) -> Caps {
        self.caps
    }

    /// Sets the capability set from inside the builder, so a menu whose
    /// capabilities cost something to work out computes them only while it
    /// is open.
    pub fn set_caps(&mut self, caps: Caps) {
        self.caps = caps;
    }

    /// The popup's ui, for content the builder has no item for.
    pub fn ui(&mut self) -> &mut egui::Ui {
        self.ui
    }

    fn record(&self, key: &str, response: &egui::Response) {
        if key.is_empty() {
            return;
        }
        let r = response.rect;
        let entry = (
            format!("{}_{}", self.prefix, key),
            [r.left(), r.top(), r.right(), r.bottom()],
        );
        self.ui.ctx().data_mut(|d| {
            d.get_temp_mut_or_default::<Vec<(String, [f32; 4])>>(rects_id())
                .push(entry)
        });
    }

    /// An item that closes the menu when clicked; true on click. A disabled
    /// item never reports a click.
    pub fn item(&mut self, key: &str, item: Item) -> bool {
        self.log(key, Some(&item), false);
        let response = chrome::menu_item(self.ui, item);
        self.record(key, &response);
        let clicked = item.enabled && response.clicked();
        if clicked {
            self.ui.close();
        }
        clicked
    }

    /// An item whose enabled state comes from the view's capabilities.
    pub fn item_if(&mut self, key: &str, item: Item, enabled: bool) -> bool {
        self.item(key, item.enabled(enabled && item.enabled))
    }

    /// A disabled item that tells why when hovered.
    pub fn disabled_hint(&mut self, key: &str, item: Item, hint: &str) {
        let item = item.enabled(false);
        self.log(key, Some(&item), false);
        let response = chrome::menu_item(self.ui, item);
        self.record(key, &response);
        // The item is drawn disabled but its ui is not, so the plain hover
        // text is the one that shows.
        response.on_hover_text(hint);
    }

    /// A check item (a checkmark when `checked`); true on click.
    pub fn check(&mut self, key: &str, label: &str, checked: bool) -> bool {
        self.item(key, Item::new(label).checked(checked))
    }

    /// A check item with an icon that leaves the menu open when clicked, so
    /// several can be switched in a row; true on click.
    pub fn toggle_icon(
        &mut self,
        key: &str,
        label: &str,
        icon: Option<Icon>,
        checked: bool,
    ) -> bool {
        self.log(key, Some(&Item::new(label).checked(checked)), false);
        let response = chrome::check_icon_item(self.ui, label, icon, checked);
        self.record(key, &response);
        response.clicked()
    }

    pub fn separator(&mut self) {
        self.log("", None, false);
        chrome::menu_separator(self.ui);
    }

    /// A submenu opened by hover that stays open while its items are
    /// clicked; a click outside the menus closes it.
    pub fn submenu_kept_open<R>(
        &mut self,
        key: &str,
        item: Item,
        content: impl FnOnce(&mut Menu) -> R,
    ) -> Option<R> {
        self.log(key, Some(&item), true);
        let (prefix, caps, depth) = (self.prefix.clone(), self.caps, self.depth + 1);
        let (response, inner) = chrome::submenu_with(
            self.ui,
            item,
            PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                content(&mut Menu {
                    ui,
                    prefix,
                    caps,
                    depth,
                })
            },
        );
        self.record(key, &response);
        inner
    }

    /// A submenu opened by hover. `content` runs only while it is open.
    pub fn submenu<R>(
        &mut self,
        key: &str,
        item: Item,
        content: impl FnOnce(&mut Menu) -> R,
    ) -> Option<R> {
        self.log(key, Some(&item), true);
        let (prefix, caps, depth) = (self.prefix.clone(), self.caps, self.depth + 1);
        let (response, inner) = chrome::submenu(self.ui, item, |ui| {
            content(&mut Menu {
                ui,
                prefix,
                caps,
                depth,
            })
        });
        self.record(key, &response);
        inner
    }

    /// One data item; true on click.
    pub fn spec(&mut self, spec: &Spec) -> bool {
        match spec.hint.filter(|_| !spec.enabled) {
            Some(hint) => {
                self.disabled_hint(spec.key, spec.item(), hint);
                false
            }
            None => self.item(spec.key, spec.item()),
        }
    }

    /// Draws `entries`; the key of the item clicked, if any.
    pub fn entries(&mut self, entries: &[Entry]) -> Option<&'static str> {
        let mut chosen = None;
        for entry in entries {
            match entry {
                Entry::Separator => self.separator(),
                Entry::Item(spec) => {
                    if self.spec(spec) {
                        chosen = Some(spec.key);
                    }
                }
                Entry::Submenu(spec, children) => {
                    if let Some(Some(key)) =
                        self.submenu(spec.key, spec.item(), |m| m.entries(children))
                    {
                        chosen = Some(key);
                    }
                }
            }
        }
        chosen
    }
}

/// Opens a context menu on a response: on its right-click, or at a given
/// position when the keyboard asked for it.
pub struct ContextMenu {
    prefix: String,
    caps: Caps,
    min_width: f32,
    open_at: Option<Pos2>,
}

impl ContextMenu {
    /// `prefix` names the item rectangles: `<prefix>_<key>`.
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
            caps: Caps::default(),
            min_width: 0.,
            open_at: None,
        }
    }
    pub fn caps(mut self, caps: Caps) -> Self {
        self.caps = caps;
        self
    }
    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }
    /// Opens the menu at `at` now (Shift+F10 or the Menu key).
    pub fn open_at(mut self, at: Option<Pos2>) -> Self {
        self.open_at = at;
        self
    }

    /// Shows the menu while it is open. `build` runs inside the popup, once
    /// per frame, so everything it reads is live. A click on a disabled item
    /// or a separator leaves the menu open.
    pub fn show<S>(
        self,
        response: &egui::Response,
        state: &mut S,
        build: impl FnOnce(&mut S, &mut Menu),
    ) {
        let ctx = &response.ctx;
        let id = Popup::default_response_id(response);
        // A menu the keyboard opened keeps its anchor while it is open; a
        // right-click (or closing) forgets it, so the next one opens at the
        // pointer again.
        let remembered = egui::Id::new(("openxer_context_menu_at", id));
        let mut popup = Popup::context_menu(response)
            .style(chrome::popup_menu_style)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside);
        if response.secondary_clicked() || !Popup::is_id_open(ctx, id) {
            ctx.data_mut(|d| d.remove_temp::<Pos2>(remembered));
        }
        if let Some(at) = self.open_at {
            ctx.data_mut(|d| d.insert_temp(remembered, at));
            popup = popup.open_memory(Some(egui::SetOpenCommand::Bool(true)));
        }
        if let Some(at) = ctx.data(|d| d.get_temp::<Pos2>(remembered)) {
            popup = popup.anchor(egui::PopupAnchor::Position(at));
        }
        let Self {
            prefix,
            caps,
            min_width,
            ..
        } = self;
        popup.show(|ui| {
            if min_width > 0. {
                ui.set_min_width(min_width);
            }
            let mut menu = Menu::on(ui, &prefix).with_caps(caps);
            build(state, &mut menu);
        });
    }
}

/// Edge detector for a key that egui cannot see: true once per press.
#[derive(Debug, Default)]
pub struct MenuKey {
    down: bool,
}

impl MenuKey {
    /// Reads the key through `read` and reports a press (up to down).
    pub fn pressed(&mut self, read: impl FnOnce() -> bool) -> bool {
        let now = read();
        let edge = now && !self.down;
        self.down = now;
        edge
    }
}

/// Whether the Menu (application) key is down right now. egui has no key
/// for it and the window layer drops it, so Windows is asked directly.
#[cfg(all(windows, not(test)))]
pub fn menu_key_down() -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn GetAsyncKeyState(vkey: i32) -> i16;
    }
    const VK_APPS: i32 = 0x5D;
    // The high bit is set while the key is down.
    unsafe { GetAsyncKeyState(VK_APPS) < 0 }
}

/// No Menu key off Windows; tests never read the real keyboard.
#[cfg(any(not(windows), test))]
pub fn menu_key_down() -> bool {
    false
}

/// The keyboard side: a pending request to open the focused view's menu.
#[derive(Debug, Default)]
pub struct KeyState {
    key: MenuKey,
    request: bool,
}

impl KeyState {
    /// Reads the Menu key with `read` every frame, so a press made while
    /// another window had the focus does not fire on return; a press while
    /// `focused` raises the request.
    pub fn poll(&mut self, focused: bool, read: impl FnOnce() -> bool) {
        if self.key.pressed(read) && focused {
            self.request = true;
        }
    }
    /// Asks the focused view to open its menu this frame.
    pub fn request(&mut self) {
        self.request = true;
    }
    /// The view that draws first and has the focus takes the request.
    pub fn take(&mut self) -> bool {
        std::mem::take(&mut self.request)
    }
    /// A request nobody took this frame is dropped.
    pub fn clear(&mut self) {
        self.request = false;
    }
}

impl App {
    /// Shift+F10 and the Menu key: open the focused view's context menu at
    /// the selected row. The views that can open one take the request while
    /// they draw (`KeyState::take`).
    pub(crate) fn open_context_menu_for_focus(&mut self) {
        self.ctx_menu.request();
    }

    /// Reads the context-menu keys, once per frame. Shift+F10 is consumed
    /// here so the plain F10 (Save) never sees it.
    pub(crate) fn poll_context_menu_keys(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|i| i.focused);
        self.ctx_menu.poll(focused, menu_key_down);
        let shift = egui::Modifiers {
            shift: true,
            ..Default::default()
        };
        if ctx.input_mut(|i| i.consume_key(shift, Key::F10)) && focused {
            self.open_context_menu_for_focus();
        }
    }

    /// A text field has the keyboard or a dialog is up: the request has
    /// nowhere to go.
    pub(crate) fn context_menu_blocked(&self, ctx: &egui::Context) -> bool {
        ctx.wants_keyboard_input()
            || self.top_schedule_dialog().is_some()
            || self.link_editor.is_some()
            || self.gantt_ui.text.is_some()
            || self.grid_dialog_open()
            || self.editing_dialog_open()
            || self.tools_modal_open()
            || self.layout_dialog_open()
            || self.enterprise_open()
            || self.dictionary_open()
    }
}

#[cfg(test)]
#[path = "context_menu_tests.rs"]
mod tests;
