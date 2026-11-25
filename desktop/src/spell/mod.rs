//! Edit > Spell Check (F7): activity names, step names and notebooks of the
//! open project against the built-in English list and the user dictionary,
//! with Ignore, Ignore All, Change, Change All and Add. Changes collect in a
//! working copy of the texts and reach the project as one undo step when the
//! dialog closes.
pub mod dict;
#[cfg(test)]
mod tests;

use crate::dialog_kit::{self as kit, Entry};
use crate::records;
use crate::schedule_dialogs::modal;
use crate::App;
use eframe::egui::{self, Align, Key, Vec2};
use std::collections::{HashMap, HashSet};

pub const USER_DICTIONARY_KEY: &str = "spell_user_dictionary_v1";

/// Where a checked text lives.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Name(i64),
    /// A TASKPROC row and its `proc_id`.
    Step(usize, String),
    /// A TASKMEMO row and its `memo_id`.
    Notebook(usize, String),
}

#[derive(Clone, Debug)]
pub struct Item {
    pub source: Source,
    pub activity: usize,
    pub label: String,
    pub text: String,
    pub changed: bool,
}

impl Item {
    fn html(&self) -> bool {
        matches!(self.source, Source::Notebook(..))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Miss {
    pub item: usize,
    pub start: usize,
    pub end: usize,
    pub word: String,
}

pub struct SpellState {
    pub items: Vec<Item>,
    pub item: usize,
    pub pos: usize,
    pub ignore_all: HashSet<String>,
    pub change_all: HashMap<String, String>,
    pub user: HashSet<String>,
    pub current: Option<Miss>,
    pub suggestions: Vec<String>,
    pub suggestion: Option<usize>,
    pub change_to: String,
    pub changes: usize,
}

impl SpellState {
    pub fn new(items: Vec<Item>, user: HashSet<String>) -> Self {
        let mut s = Self {
            items,
            item: 0,
            pos: 0,
            ignore_all: HashSet::new(),
            change_all: HashMap::new(),
            user,
            current: None,
            suggestions: vec![],
            suggestion: None,
            change_to: String::new(),
            changes: 0,
        };
        s.advance();
        s
    }

    pub fn done(&self) -> bool {
        self.current.is_none()
    }

    fn replace(&mut self, item: usize, start: usize, end: usize, with: &str) {
        let it = &mut self.items[item];
        it.text.replace_range(start..end, with);
        it.changed = true;
        self.changes += 1;
    }

    /// Find the next word not in a dictionary, applying Change All
    /// replacements on the way.
    pub fn advance(&mut self) {
        let d = dict::builtin();
        self.current = None;
        self.suggestions.clear();
        self.suggestion = None;
        while self.item < self.items.len() {
            let (text, html) = (
                self.items[self.item].text.clone(),
                self.items[self.item].html(),
            );
            let mut next = None;
            for (s, e) in dict::words(&text, html) {
                if s < self.pos {
                    continue;
                }
                let word = &text[s..e];
                let low = dict::clip(word).to_lowercase();
                if self.ignore_all.contains(&low) || d.known(word, &self.user) {
                    continue;
                }
                next = Some((s, e, dict::clip(word).to_string(), low));
                break;
            }
            match next {
                Some((s, e, word, low)) => {
                    if let Some(to) = self.change_all.get(&low).cloned() {
                        let item = self.item;
                        self.replace(item, s, e, &to);
                        self.pos = s + to.len();
                        continue;
                    }
                    self.suggestions = d.suggest(&word, &self.user, 8);
                    self.change_to = self
                        .suggestions
                        .first()
                        .cloned()
                        .unwrap_or_else(|| word.clone());
                    self.suggestion = (!self.suggestions.is_empty()).then_some(0);
                    self.current = Some(Miss {
                        item: self.item,
                        start: s,
                        end: e,
                        word,
                    });
                    return;
                }
                None => {
                    self.item += 1;
                    self.pos = 0;
                }
            }
        }
    }

    pub fn ignore(&mut self) {
        if let Some(m) = self.current.take() {
            self.pos = m.end;
            self.advance();
        }
    }

    pub fn ignore_all(&mut self) {
        if let Some(m) = self.current.take() {
            self.ignore_all.insert(m.word.to_lowercase());
            self.pos = m.end;
            self.advance();
        }
    }

    pub fn change(&mut self) {
        let Some(m) = self.current.take() else {
            return;
        };
        let to = self.change_to.clone();
        self.replace(m.item, m.start, m.end, &to);
        self.pos = m.start + to.len();
        self.advance();
    }

    pub fn change_all(&mut self) {
        if let Some(m) = &self.current {
            self.change_all
                .insert(m.word.to_lowercase(), self.change_to.clone());
        }
        self.change();
    }

    /// Add the word to the user dictionary; returns it for saving.
    pub fn add(&mut self) -> Option<String> {
        let m = self.current.take()?;
        let low = m.word.to_lowercase();
        self.user.insert(low.clone());
        self.pos = m.end;
        self.advance();
        Some(low)
    }
}

impl App {
    pub(crate) fn spell_menu_item(&mut self, ui: &mut egui::Ui) {
        if self.item(
            ui,
            "menu_spell_check",
            crate::chrome::Item::new("&Spell Check...")
                .key("F7")
                .icon(crate::chrome::Icon::SpellCheck),
        ) {
            self.open_spell_check();
        }
    }

    /// A tools dialog that should keep the main window's keys off is open.
    pub(crate) fn tools_modal_open(&self) -> bool {
        self.spell.is_some()
            || self.compare.dialog.is_some()
            || self.reports.modal_open()
            || self.estimation.is_some()
    }

    fn user_dictionary(&self) -> HashSet<String> {
        self.store
            .preference(USER_DICTIONARY_KEY)
            .ok()
            .flatten()
            .unwrap_or_default()
            .lines()
            .map(|l| l.trim().to_lowercase())
            .filter(|l| !l.is_empty())
            .collect()
    }

    /// The texts to check: per activity (from the selected one, wrapping)
    /// its name, step names and notebooks.
    pub(crate) fn spell_items(&self) -> Vec<Item> {
        let p = &self.project;
        let pid = p.model.project.proj_id;
        let n = p.model.activities.len();
        let first = self.selected.filter(|s| *s < n).unwrap_or(0);
        let topics: HashMap<String, String> =
            p.document.table("MEMOTYPE").map_or_else(HashMap::new, |t| {
                t.rows
                    .iter()
                    .map(|r| {
                        (
                            t.value(r, "memo_type_id").to_string(),
                            t.value(r, "memo_type").to_string(),
                        )
                    })
                    .collect()
            });
        let mut out = vec![];
        for k in 0..n {
            let i = (first + k) % n;
            let a = &p.model.activities[i];
            out.push(Item {
                source: Source::Name(a.task_id),
                activity: i,
                label: format!("{}  Activity Name", a.task_code),
                text: p.name(i).to_string(),
                changed: false,
            });
            for r in records::rows_for(&p.document, "TASKPROC", a.task_id, pid) {
                out.push(Item {
                    source: Source::Step(r, records::get(&p.document, "TASKPROC", r, "proc_id")),
                    activity: i,
                    label: format!("{}  Step", a.task_code),
                    text: records::get(&p.document, "TASKPROC", r, "proc_name"),
                    changed: false,
                });
            }
            for r in records::rows_for(&p.document, "TASKMEMO", a.task_id, pid) {
                let topic = records::get(&p.document, "TASKMEMO", r, "memo_type_id");
                out.push(Item {
                    source: Source::Notebook(
                        r,
                        records::get(&p.document, "TASKMEMO", r, "memo_id"),
                    ),
                    activity: i,
                    label: format!(
                        "{}  Notebook: {}",
                        a.task_code,
                        topics.get(&topic).cloned().unwrap_or_default()
                    ),
                    text: records::get(&p.document, "TASKMEMO", r, "task_memo"),
                    changed: false,
                });
            }
        }
        out
    }

    pub(crate) fn open_spell_check(&mut self) {
        let items = self.spell_items();
        let state = SpellState::new(items, self.user_dictionary());
        if let Some(m) = &state.current {
            let a = state.items[m.item].activity;
            self.selected = Some(a);
        }
        self.spell = Some(state);
    }

    /// Write the changed texts to the project as one undo step.
    pub(crate) fn close_spell_check(&mut self, time: f64) {
        let Some(state) = self.spell.take() else {
            return;
        };
        let changed: Vec<&Item> = state.items.iter().filter(|i| i.changed).collect();
        if changed.is_empty() {
            self.status = "Spell Check: no changes".into();
            return;
        }
        let before = self.project.clone();
        let before_revision = self.records_revision;
        let mut applied = 0;
        for item in changed {
            let doc = &mut self.project.document;
            let details = &mut self.project.details;
            match &item.source {
                Source::Name(task_id) => {
                    if let Some(d) = details.get_mut(task_id) {
                        d.name = item.text.clone();
                        applied += 1;
                    }
                }
                Source::Step(row, id) => {
                    if records::get(doc, "TASKPROC", *row, "proc_id") == *id {
                        records::set(doc, "TASKPROC", *row, "proc_name", item.text.clone());
                        applied += 1;
                    }
                }
                Source::Notebook(row, id) => {
                    if records::get(doc, "TASKMEMO", *row, "memo_id") == *id {
                        records::set(doc, "TASKMEMO", *row, "task_memo", item.text.clone());
                        applied += 1;
                    }
                }
            }
        }
        self.records_serial += 1;
        self.records_revision = self.records_serial;
        self.history.record_bulk(
            &before,
            before_revision,
            &self.project,
            self.records_revision,
            time,
        );
        self.dirty = true;
        self.detail.rows = [None, None];
        self.status = format!(
            "Spell Check: {} word{} changed in {applied} text{}",
            state.changes,
            if state.changes == 1 { "" } else { "s" },
            if applied == 1 { "" } else { "s" }
        );
    }

    /// End the check without applying the held changes. Words added to the
    /// user dictionary stay added: that list is saved as each word is added.
    pub(crate) fn cancel_spell_check(&mut self) {
        if self.spell.take().is_some() {
            self.status = "Spell Check cancelled: no changes made".into();
        }
    }

    fn save_user_word(&mut self, word: &str) {
        let mut words: Vec<String> = self.user_dictionary().into_iter().collect();
        if !words.iter().any(|w| w == word) {
            words.push(word.to_string());
        }
        words.sort();
        if let Err(e) = self
            .store
            .set_preference(USER_DICTIONARY_KEY, &words.join("\n"))
        {
            self.error(e);
        }
    }

    pub(crate) fn spell_dialog(&mut self, ctx: &egui::Context) {
        if self.spell.is_none() {
            if ctx.input(|i| i.key_pressed(Key::F7))
                && !ctx.wants_keyboard_input()
                && !self.tools_modal_open()
                && self.top_schedule_dialog().is_none()
            {
                self.open_spell_check();
            }
            return;
        }
        let time = ctx.input(|i| i.time);
        let Some(mut s) = self.spell.take() else {
            return;
        };
        const W: f32 = 500.;
        const H: f32 = 300.;
        #[derive(Default)]
        struct Clicks {
            ignore: bool,
            ignore_all: bool,
            change: bool,
            change_all: bool,
            add: bool,
            close: bool,
            cancel: bool,
            help: bool,
        }
        let (k, closed, actions) = modal(ctx, "spell", "Spell Check", Vec2::new(W, H), |c| {
            let mut k = Clicks::default();
            c.fill(0., 0., W, H, kit::FACE);
            c.lowered(5., 5., W - 122., H - 10.);
            let active = !s.done();
            match &s.current {
                Some(m) => {
                    let item = &s.items[m.item];
                    kit::text(c, 14., 14., &item.label, true);
                    // The word in its sentence, trimmed around it.
                    let text = &item.text;
                    let from = text[..m.start]
                        .char_indices()
                        .rev()
                        .nth(30)
                        .map_or(0, |(i, _)| i);
                    let to = text[m.end..]
                        .char_indices()
                        .nth(30)
                        .map_or(text.len(), |(i, _)| m.end + i);
                    let shown = if m.end - m.start > m.word.len() {
                        format!("{}...", m.word)
                    } else {
                        m.word.clone()
                    };
                    let context = format!(
                        "{}{}{shown}{}{}",
                        if from > 0 { "..." } else { "" },
                        &text[from..m.start],
                        &text[m.end..to],
                        if to < text.len() { "..." } else { "" }
                    );
                    kit::text(c, 14., 32., &context.replace(['\n', '\r'], " "), true);
                    c.label(14., 58., 17., "Not in Dictionary", true);
                    kit::field(c, 120., 56., 250., 22., &m.word, false);
                }
                None => {
                    kit::text(c, 14., 14., "Spell checking complete.", true);
                    c.label(14., 58., 17., "Not in Dictionary", true);
                    kit::field(c, 120., 56., 250., 22., "", false);
                }
            }
            c.label(14., 90., 17., "Change To", true);
            c.edit(
                "spell_change_to",
                120.,
                88.,
                250.,
                22.,
                &mut s.change_to,
                active,
                Align::Min,
            );
            c.label(14., 122., 17., "Suggestions", true);
            let bottom = c.origin.y + 120. + 160. - 1.;
            let suggestions = &s.suggestions;
            let out = kit::list(
                c,
                "spell_suggestions",
                (120., 120., 250., 160.),
                None,
                &[kit::Col {
                    title: "Word",
                    width: 230.,
                }],
                suggestions.len(),
                s.suggestion,
                None,
                &mut |c, i, _, rect, sel| {
                    crate::reports::cell_text(c, rect, &suggestions[i], sel, 3., bottom)
                },
            );
            if let Some(i) = out.clicked {
                s.suggestion = Some(i);
                s.change_to = s.suggestions[i].clone();
            }
            if let Some(i) = out.double {
                s.change_to = s.suggestions[i].clone();
                k.change = true;
            }
            if s.changes > 0 {
                kit::text(c, 14., 270., &format!("{} changed", s.changes), true);
            }
            kit::strip(c, W, H);
            // The word commands first (Enter ignores the word); Close keeps
            // the changes made, Cancel drops them.
            let column = kit::button_column(
                c,
                W,
                &[
                    Entry::Button("spell_ignore", "&Ignore", active, false),
                    Entry::Button("spell_ignore_all", "I&gnore All", active, false),
                    Entry::Button("spell_change", "&Change", active, true),
                    Entry::Button("spell_change_all", "Change Al&l", active, false),
                    Entry::Button("spell_add", "Add", active, true),
                    Entry::Button("spell_close", "Close", true, true),
                    Entry::cancel("spell_cancel"),
                    Entry::help("spell_help"),
                ],
            );
            let clicked = |key: &str| column.clicked.as_deref() == Some(key);
            k.ignore = clicked("spell_ignore");
            k.ignore_all = clicked("spell_ignore_all");
            k.change |= clicked("spell_change");
            k.change_all = clicked("spell_change_all");
            k.add = clicked("spell_add");
            k.close = clicked("spell_close");
            k.cancel = clicked("spell_cancel");
            k.help = clicked("spell_help");
            k
        });
        self.record_dialog_actions(actions);
        if k.ignore {
            s.ignore();
        } else if k.ignore_all {
            s.ignore_all();
        } else if k.change {
            s.change();
        } else if k.change_all {
            s.change_all();
        } else if k.add {
            if let Some(word) = s.add() {
                self.save_user_word(&word);
            }
        }
        if let Some(m) = &s.current {
            let a = s.items[m.item].activity;
            if self.selected != Some(a) {
                self.selected = Some(a);
                self.scroll_to_selected = true;
            }
        }
        if k.help {
            self.open_help_for("spell");
        }
        self.spell = Some(s);
        if k.close {
            self.close_spell_check(time);
        } else if k.cancel || closed {
            // Cancel, the close box and Escape drop the held changes.
            self.cancel_spell_check();
        }
    }
}
