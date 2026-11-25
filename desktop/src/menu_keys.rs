//! Keyboard for the main menu bar: Alt focuses the bar, Alt+letter opens the
//! menu whose letter is underlined, Up and Down move through a dropdown
//! (skipping disabled items), Right and Left open and close submenus or move
//! to the next menu, Enter runs the highlighted item, Escape closes the menu,
//! and a letter runs the item underlined with it.
//!
//! Menus are drawn immediately by `chrome::menu_bar_title`, `menu_item` and
//! `submenu`, which note each title and item they draw. The keys of a frame
//! are read before the menus draw, against the notes of the frame before, and
//! act through the same paths the pointer uses: an item is run by giving it
//! the keyboard focus together with an Enter press, which egui reads as a
//! click on that widget (a submenu button opens its submenu the same way).
use super::*;
use egui::{Event, Id, Modifiers};

/// One menu-bar title as drawn.
#[derive(Clone)]
pub(crate) struct TitleNote {
    pub popup: Id,
    pub mnemonic: Option<char>,
    /// The caption without its `&`; the tests read it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub label: String,
}

/// One dropdown entry as drawn.
#[derive(Clone)]
pub(crate) struct ItemNote {
    pub id: Id,
    pub enabled: bool,
    pub mnemonic: Option<char>,
    pub submenu: bool,
    /// The caption without its `&`; the tests read it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub label: String,
}

/// One open dropdown: the titles' menu or a submenu, in nesting order.
#[derive(Clone, Default)]
pub(crate) struct Level {
    /// The menu's root widget id, where egui keeps its open submenu.
    pub menu: Option<Id>,
    /// The submenu item of the level below that opened this one.
    pub parent: Option<Id>,
    pub items: Vec<ItemNote>,
}

#[derive(Clone, Default)]
struct Notes {
    titles: Vec<TitleNote>,
    levels: Vec<Level>,
}

#[derive(Clone, Default)]
struct State {
    /// What the last frame drew.
    prev: Notes,
    /// What this frame is drawing.
    cur: Notes,
    /// Levels being drawn right now (indices into `cur.levels`).
    stack: Vec<usize>,
    /// The item the keyboard highlights.
    hot: Option<Id>,
    /// The bar has the keyboard (after Alt) with no dropdown open: the title.
    bar: Option<usize>,
    /// The highlight goes to the first enabled item of the level that has
    /// this depth once it is drawn.
    first_at_depth: Option<usize>,
    /// Alt is held down.
    alt_down: bool,
    /// Alt went down and nothing else was pressed since.
    alt_clean: bool,
}

fn state_id() -> Id {
    Id::new("openxer_menu_keys")
}

fn with_state<R>(ctx: &egui::Context, f: impl FnOnce(&mut State) -> R) -> R {
    ctx.data_mut(|d| f(d.get_temp_mut_or_default::<State>(state_id())))
}

// ------------------------------------------------------- notes from drawing

/// Called by the title drawing code, once per title per frame.
pub(crate) fn note_title(ctx: &egui::Context, popup: Id, label: &str) {
    let (text, index) = chrome::mnemonic(label);
    let mnemonic = index.and_then(|i| text.chars().nth(i)).map(lower);
    with_state(ctx, |s| {
        s.cur.titles.push(TitleNote {
            popup,
            mnemonic,
            label: text,
        })
    });
}

/// A dropdown's content starts: `parent` is the submenu item it opens from.
/// Contexts that are not bar menus (nothing open from a title) are ignored.
/// True when the level is being noted, which `leave_level` then closes.
pub(crate) fn enter_level(ui: &egui::Ui, parent: Option<Id>) -> bool {
    let menu = egui::containers::menu::find_menu_root(ui).id;
    with_state(ui.ctx(), |s| {
        if parent.is_some() && s.stack.is_empty() {
            return false;
        }
        s.cur.levels.push(Level {
            menu: Some(menu),
            parent,
            items: vec![],
        });
        s.stack.push(s.cur.levels.len() - 1);
        true
    })
}

/// The dropdown's content ends.
pub(crate) fn leave_level(ctx: &egui::Context) {
    with_state(ctx, |s| {
        s.stack.pop();
    });
}

/// Called for every item drawn inside a level; true when the keyboard
/// highlights it.
pub(crate) fn note_item(
    ctx: &egui::Context,
    id: Id,
    label: &str,
    enabled: bool,
    submenu: bool,
) -> bool {
    let (text, index) = chrome::mnemonic(label);
    let mnemonic = index.and_then(|i| text.chars().nth(i)).map(lower);
    with_state(ctx, |s| {
        if let Some(level) = s.stack.last().copied() {
            s.cur.levels[level].items.push(ItemNote {
                id,
                enabled,
                mnemonic,
                submenu,
                label: text,
            });
        }
        enabled && s.hot == Some(id)
    })
}

/// The title the keyboard has focused with its dropdown closed.
pub(crate) fn bar_focus(ctx: &egui::Context, popup: Id) -> bool {
    with_state(ctx, |s| {
        s.bar
            .and_then(|i| s.prev.titles.get(i))
            .is_some_and(|t| t.popup == popup)
    })
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

// ------------------------------------------------------------------- tests

/// The caption of the highlighted item, for tests.
#[cfg(test)]
pub(crate) fn hot_label(ctx: &egui::Context) -> Option<String> {
    with_state(ctx, |s| {
        let hot = s.hot?;
        s.prev
            .levels
            .iter()
            .flat_map(|l| l.items.iter())
            .chain(s.cur.levels.iter().flat_map(|l| l.items.iter()))
            .find(|i| i.id == hot)
            .map(|i| i.label.clone())
    })
}

/// Whether the bar holds the keyboard on a title, for tests.
#[cfg(test)]
pub(crate) fn bar_title(ctx: &egui::Context) -> Option<String> {
    with_state(ctx, |s| {
        s.bar
            .and_then(|i| s.prev.titles.get(i))
            .map(|t| t.label.clone())
    })
}

// ----------------------------------------------------------------- the keys

/// Next enabled item from `from` in direction `step` (wrapping), or the
/// first (last) when nothing is highlighted.
fn step(level: &Level, from: Option<Id>, step: isize) -> Option<Id> {
    let n = level.items.len() as isize;
    if n == 0 || !level.items.iter().any(|i| i.enabled) {
        return None;
    }
    let mut at = match from.and_then(|id| level.items.iter().position(|i| i.id == id)) {
        Some(p) => p as isize,
        None if step > 0 => -1,
        None => n,
    };
    for _ in 0..n {
        at = (at + step).rem_euclid(n);
        if level.items[at as usize].enabled {
            return Some(level.items[at as usize].id);
        }
    }
    None
}

fn first_enabled(level: &Level) -> Option<Id> {
    level.items.iter().find(|i| i.enabled).map(|i| i.id)
}

fn last_enabled(level: &Level) -> Option<Id> {
    level.items.iter().rev().find(|i| i.enabled).map(|i| i.id)
}

/// The key a letter stands for.
fn letter_key(c: char) -> Option<Key> {
    Key::from_name(&c.to_uppercase().to_string())
}

/// The character of a letter or digit key.
fn key_char(key: Key) -> Option<char> {
    let name = key.name();
    let mut chars = name.chars();
    let c = chars.next()?;
    (chars.next().is_none() && c.is_ascii_alphanumeric()).then(|| lower(c))
}

/// Gives `id` the keyboard focus and an Enter press, which egui reads as a
/// click on it this frame.
pub(crate) fn press(ctx: &egui::Context, id: Id) {
    ctx.memory_mut(|m| m.request_focus(id));
    let has_enter = ctx.input(|i| i.key_pressed(Key::Enter));
    if !has_enter {
        ctx.input_mut(|i| {
            i.events.push(Event::Key {
                key: Key::Enter,
                physical_key: Some(Key::Enter),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            });
        });
    }
}

impl App {
    /// Reads this frame's menu keys. Runs before anything else looks at the
    /// keyboard, and consumes the keys it uses.
    pub(crate) fn menu_keys(&mut self, ctx: &egui::Context) {
        let modal = self.modal_open(ctx);
        let (alt, keys, clicked, moved) = ctx.input(|i| {
            let keys = i
                .events
                .iter()
                .any(|e| matches!(e, Event::Key { pressed: true, .. }));
            let clicked = i
                .events
                .iter()
                .any(|e| matches!(e, Event::PointerButton { pressed: true, .. }));
            let moved = i.events.iter().any(|e| matches!(e, Event::PointerMoved(_)));
            (i.modifiers.alt, keys, clicked, moved)
        });
        let mut st = with_state(ctx, |s| {
            s.prev = std::mem::take(&mut s.cur);
            s.stack.clear();
            s.clone()
        });
        let open_title = st
            .prev
            .titles
            .iter()
            .position(|t| egui::Popup::is_id_open(ctx, t.popup));
        // Alt pressed and released with nothing between takes the bar.
        let mut tapped = false;
        if alt {
            if !st.alt_down {
                st.alt_clean = true;
            }
            if keys || clicked {
                st.alt_clean = false;
            }
        } else {
            tapped = st.alt_down && st.alt_clean;
            st.alt_clean = false;
        }
        st.alt_down = alt;
        // A dialog owns the keyboard; the menus keep nothing.
        if modal {
            st.bar = None;
            st.hot = None;
            self.store_keys(ctx, st);
            return;
        }
        if clicked {
            st.bar = None;
            st.hot = None;
        }
        if moved && st.hot.is_some() {
            st.hot = None;
        }
        if tapped {
            if let Some(t) = open_title {
                egui::Popup::close_id(ctx, st.prev.titles[t].popup);
                st.bar = Some(t);
                st.hot = None;
            } else if st.bar.is_some() {
                st.bar = None;
            } else if !st.prev.titles.is_empty() {
                st.bar = Some(0);
            }
        }
        // Alt+letter opens the menu with that letter underlined.
        let titles = st.prev.titles.clone();
        for (index, title) in titles.iter().enumerate() {
            let Some(key) = title.mnemonic.and_then(letter_key) else {
                continue;
            };
            if ctx.input_mut(|i| i.consume_key(Modifiers::ALT, key)) {
                self.open_title(ctx, &mut st, index);
            }
        }
        let open_title = st
            .prev
            .titles
            .iter()
            .position(|t| egui::Popup::is_id_open(ctx, t.popup));
        if let Some(title) = open_title {
            self.dropdown_keys(ctx, &mut st, title);
        } else if let Some(bar) = st.bar {
            self.bar_keys(ctx, &mut st, bar);
        } else {
            st.hot = None;
        }
        self.store_keys(ctx, st);
    }

    fn store_keys(&mut self, ctx: &egui::Context, st: State) {
        with_state(ctx, |s| {
            s.bar = st.bar;
            s.hot = st.hot;
            s.first_at_depth = st.first_at_depth;
            s.alt_down = st.alt_down;
            s.alt_clean = st.alt_clean;
        });
    }

    /// Opens title `index`'s dropdown (closing the others) with nothing
    /// highlighted.
    fn open_title(&mut self, ctx: &egui::Context, st: &mut State, index: usize) {
        let Some(title) = st.prev.titles.get(index) else {
            return;
        };
        egui::Popup::open_id(ctx, title.popup);
        st.bar = None;
        st.hot = None;
        st.first_at_depth = None;
        // The pointer no longer shows what the keyboard does.
        ctx.input_mut(|i| i.events.push(Event::PointerGone));
    }

    /// Keys with the bar highlighted and no dropdown open.
    fn bar_keys(&mut self, ctx: &egui::Context, st: &mut State, bar: usize) {
        let count = st.prev.titles.len();
        if count == 0 {
            st.bar = None;
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            st.bar = None;
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowRight)) {
            st.bar = Some((bar + 1) % count);
        } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowLeft)) {
            st.bar = Some((bar + count - 1) % count);
        } else if ctx.input_mut(|i| {
            i.consume_key(Modifiers::NONE, Key::ArrowDown)
                | i.consume_key(Modifiers::NONE, Key::Enter)
                | i.consume_key(Modifiers::NONE, Key::ArrowUp)
        }) {
            self.open_title(ctx, st, bar);
        } else {
            let titles = st.prev.titles.clone();
            for (index, title) in titles.iter().enumerate() {
                if let Some(key) = title.mnemonic.and_then(letter_key) {
                    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, key)) {
                        self.open_title(ctx, st, index);
                    }
                }
            }
        }
        ctx.input_mut(|i| i.events.retain(|e| !matches!(e, Event::Text(_))));
    }

    /// Keys with a dropdown open.
    fn dropdown_keys(&mut self, ctx: &egui::Context, st: &mut State, title: usize) {
        let count = st.prev.titles.len();
        let depth = st.prev.levels.len();
        // A submenu opened from the keyboard last frame: highlight its first
        // item now that it has been drawn.
        if st.first_at_depth == Some(depth) {
            st.hot = st.prev.levels.last().and_then(first_enabled);
            st.first_at_depth = None;
        }
        let Some(level) = st.prev.levels.last().cloned() else {
            return;
        };
        if st
            .hot
            .is_some_and(|h| !level.items.iter().any(|i| i.id == h))
        {
            st.hot = None;
        }
        let hot = st
            .hot
            .and_then(|h| level.items.iter().find(|i| i.id == h))
            .cloned();
        let mut moved = false;
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown)) {
            st.hot = step(&level, st.hot, 1);
            moved = true;
        } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp)) {
            st.hot = step(&level, st.hot, -1);
            moved = true;
        } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Home)) {
            st.hot = first_enabled(&level);
            moved = true;
        } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::End)) {
            st.hot = last_enabled(&level);
            moved = true;
        } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowRight)) {
            match hot {
                Some(item) if item.submenu => {
                    press(ctx, item.id);
                    st.first_at_depth = Some(depth + 1);
                    moved = true;
                }
                _ if depth <= 1 => {
                    self.open_title(ctx, st, (title + 1) % count);
                    moved = true;
                }
                _ => {}
            }
        } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowLeft)) {
            if depth > 1 {
                self.close_submenu(ctx, st, &level);
            } else {
                self.open_title(ctx, st, (title + count - 1) % count);
            }
            moved = true;
        } else if ctx.input(|i| i.key_pressed(Key::Enter) || i.key_pressed(Key::Space)) {
            match hot {
                Some(item) if item.submenu => {
                    ctx.input_mut(|i| {
                        i.consume_key(Modifiers::NONE, Key::Enter);
                        i.consume_key(Modifiers::NONE, Key::Space);
                    });
                    press(ctx, item.id);
                    st.first_at_depth = Some(depth + 1);
                }
                // Enter stays in the frame: with the focus on the item it is
                // the click.
                Some(item) => {
                    ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Space));
                    press(ctx, item.id);
                }
                None => {
                    ctx.input_mut(|i| {
                        i.consume_key(Modifiers::NONE, Key::Enter);
                        i.consume_key(Modifiers::NONE, Key::Space);
                    });
                }
            }
            moved = true;
        } else {
            self.letter_keys(ctx, st, &level, depth);
        }
        if moved {
            ctx.input_mut(|i| i.events.push(Event::PointerGone));
        }
        // A menu in use takes the typed text too.
        ctx.input_mut(|i| i.events.retain(|e| !matches!(e, Event::Text(_))));
    }

    /// A letter runs the item underlined with it; letters shared by several
    /// items move the highlight from one to the next.
    fn letter_keys(&mut self, ctx: &egui::Context, st: &mut State, level: &Level, depth: usize) {
        let pressed: Vec<(Key, char)> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } if !modifiers.alt && !modifiers.command => key_char(*key).map(|c| (*key, c)),
                    _ => None,
                })
                .collect()
        });
        let Some((key, c)) = pressed.first().copied() else {
            return;
        };
        ctx.input_mut(|i| {
            i.consume_key(Modifiers::NONE, key);
            i.consume_key(Modifiers::SHIFT, key);
        });
        let matches: Vec<&ItemNote> = level
            .items
            .iter()
            .filter(|i| i.enabled && i.mnemonic == Some(c))
            .collect();
        match matches.as_slice() {
            [] => {}
            [item] => {
                press(ctx, item.id);
                if item.submenu {
                    st.first_at_depth = Some(depth + 1);
                }
                ctx.input_mut(|i| i.events.push(Event::PointerGone));
            }
            many => {
                let from = st.hot.and_then(|h| many.iter().position(|i| i.id == h));
                let next = from.map_or(0, |p| (p + 1) % many.len());
                st.hot = Some(many[next].id);
                ctx.input_mut(|i| i.events.push(Event::PointerGone));
            }
        }
    }

    /// Closes the deepest submenu; its item stays highlighted.
    fn close_submenu(&mut self, ctx: &egui::Context, st: &mut State, level: &Level) {
        let Some(parent) = level.parent else { return };
        let owner = st
            .prev
            .levels
            .iter()
            .find(|l| l.items.iter().any(|i| i.id == parent))
            .and_then(|l| l.menu);
        if let Some(menu) = owner {
            egui::containers::menu::MenuState::from_id(ctx, menu, |state| {
                state.open_item = None;
            });
        }
        st.hot = Some(parent);
    }
}

#[cfg(test)]
#[path = "menu_keys_tests.rs"]
mod tests;
