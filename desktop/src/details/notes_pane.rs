//! The one notes pane: a captioned text pane with Modify, Print and Copy
//! under it, shared by every Notebook tab (activity, WBS element, project)
//! and the Steps tab, and the one Notebook tab body (topic list with Add
//! and Delete beside that pane) shared by Activity, WBS and Project Details.
//!
//! The text shows with its formatting (bold, italic, underline, bulleted and
//! numbered lists) as the file stores it. Modify opens it for editing in
//! place, with a toolbar that switches those formats; each change writes the
//! memo back as HTML of the same kind (`openxer_xer::edit::rich_memo`). A
//! memo holding a picture, a table or a link is shown but cannot be edited
//! here, so nothing in it is lost.
use super::*;
use crate::chrome::{self, Icon};
use egui::text::{LayoutJob, TextFormat};
use openxer_xer::edit::notebooks::{self, Owner};
use openxer_xer::edit::rich_memo::{self, Format, LineKind, RichMemo, Style};

/// Room left of a list item for its bullet or number.
const INDENT: f32 = 18.;
/// Height of the formatting toolbar over the text while it is edited.
const TOOLBAR_H: f32 = chrome::TOOL_HEIGHT + 2.;

/// Why Modify is off on a memo this editor would damage.
pub const NOT_EDITABLE_HINT: &str =
    "This text holds pictures, tables or links that only the program that made it can edit.";

/// The notes panes' state.
#[derive(Default)]
pub struct NotesState {
    buffers: BTreeMap<String, Buffer>,
    /// The memo open for editing (Modify), by key.
    pub editing: Option<String>,
    /// A formatting command was the last change: the next typing starts a
    /// new undo step.
    seal_next: bool,
    /// Assign Notebook Topic for a WBS element or the project.
    pub topic_picker: Option<super::owner_pickers::TopicPicker>,
    /// Assign Project Codes.
    pub code_picker: Option<super::owner_pickers::CodePicker>,
}

/// A memo as the pane shows and edits it.
struct Buffer {
    /// The stored text it was read from.
    source: String,
    memo: RichMemo,
    editable: bool,
    /// The style typed text takes after a format command with no
    /// selection, until the caret moves.
    typing: Option<Style>,
    cursor: Option<(usize, usize)>,
}

/// One toolbar command.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Command {
    Format(Format),
    List(LineKind),
    Clear,
}

/// The toolbar: icon, tooltip, action key suffix and command.
pub const TOOLBAR: [(Icon, &str, &str, Command); 6] = [
    (Icon::Bold, "Bold", "bold", Command::Format(Format::Bold)),
    (
        Icon::Italic,
        "Italic",
        "italic",
        Command::Format(Format::Italic),
    ),
    (
        Icon::Underline,
        "Underline",
        "underline",
        Command::Format(Format::Underline),
    ),
    (
        Icon::BulletList,
        "Bullets",
        "bullets",
        Command::List(LineKind::Bullet),
    ),
    (
        Icon::NumberedList,
        "Numbering",
        "numbering",
        Command::List(LineKind::Numbered),
    ),
    (
        Icon::ClearFormatting,
        "Clear Formatting",
        "clear_formatting",
        Command::Clear,
    ),
];

fn text_format(style: Style) -> TextFormat {
    TextFormat {
        font_id: if style.bold {
            chrome::bold_font()
        } else {
            form::font()
        },
        color: INK,
        italics: style.italic,
        underline: if style.underline {
            Stroke::new(1_f32, INK)
        } else {
            Stroke::NONE
        },
        ..Default::default()
    }
}

/// The text of `memo` laid out with its formatting, at most `limit`
/// characters, list items indented for their markers.
pub fn layout_job(memo: &RichMemo, limit: usize, wrap: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    let mut text = String::new();
    let mut style = Style::default();
    let mut lead = 0.;
    let mut line = 0;
    let mut start = true;
    let flush = |job: &mut LayoutJob, text: &mut String, style: Style, lead: f32| {
        if !text.is_empty() {
            job.append(text, lead, text_format(style));
            text.clear();
        }
    };
    for (i, c) in memo.text.chars().enumerate().take(limit) {
        let this = if c == '\n' {
            style
        } else {
            memo.styles.get(i).copied().unwrap_or_default()
        };
        if start {
            flush(&mut job, &mut text, style, lead);
            let list = memo.lines.get(line).is_some_and(|k| *k != LineKind::Plain);
            lead = if list { INDENT } else { 0. };
            style = if c == '\n' { Style::default() } else { this };
            start = false;
        } else if this != style {
            flush(&mut job, &mut text, style, lead);
            lead = 0.;
            style = this;
        }
        text.push(c);
        if c == '\n' {
            line += 1;
            start = true;
        }
    }
    flush(&mut job, &mut text, style, lead);
    if job.sections.is_empty() {
        job.append("", 0., text_format(Style::default()));
    }
    job
}

/// The bullets and numbers of the list items of `memo` laid out in
/// `galley` at `origin`.
fn paint_markers(painter: &egui::Painter, galley: &egui::Galley, origin: Pos2, memo: &RichMemo) {
    let mut line = 0;
    let mut start = true;
    let mut number = 0;
    for row in &galley.rows {
        if start {
            let kind = memo.lines.get(line).copied().unwrap_or_default();
            number = if kind == LineKind::Numbered {
                number + 1
            } else {
                0
            };
            let marker = match kind {
                LineKind::Plain => None,
                LineKind::Bullet => Some("\u{2022}".to_string()),
                LineKind::Numbered => Some(format!("{number}.")),
            };
            if let Some(marker) = marker {
                let at = origin + row.pos.to_vec2() + Vec2::new(INDENT - 4., row.size.y / 2.);
                painter.text(at, Align2::RIGHT_CENTER, marker, form::font(), INK);
            }
        }
        start = row.ends_with_newline;
        if row.ends_with_newline {
            line += 1;
        }
    }
}

impl App {
    /// The next project change starts its own undo step: an Add, Delete,
    /// Assign or Remove of the notes panes and their pickers, or a
    /// formatting command (even while the text keeps the keyboard).
    pub(crate) fn notes_step(&mut self) {
        self.history.seal();
        self.touch_records();
    }

    /// The buffer of memo `key`, read again when the stored text changed
    /// (an edit elsewhere, undo).
    fn notes_buffer(&mut self, key: &str, html: &str) -> &mut Buffer {
        let buffers = &mut self.detail.notes.buffers;
        let fresh = buffers.get(key).is_none_or(|b| b.source != html);
        if fresh {
            buffers.insert(
                key.to_string(),
                Buffer {
                    source: html.to_string(),
                    memo: RichMemo::parse(html),
                    editable: rich_memo::editable(html),
                    typing: None,
                    cursor: None,
                },
            );
        }
        buffers.get_mut(key).expect("just inserted")
    }

    /// The text of memo `key` shown with its formatting, read only, laying
    /// out no more than can fill `rect` (a long memo stays fast).
    fn notes_view(&mut self, ui: &mut egui::Ui, rect: Rect, key: &str) {
        let body = rect.shrink2(Vec2::new(6., 4.));
        let fits = ((body.width() / 2.).ceil() * (body.height() / 10. + 2.).ceil()) as usize;
        let memo = &self.detail.notes.buffers[key].memo;
        let job = layout_job(memo, fits.max(64), body.width());
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        let painter = ui.painter().with_clip_rect(body.intersect(ui.clip_rect()));
        painter.galley(body.min, galley.clone(), INK);
        paint_markers(&painter, &galley, body.min, memo);
    }

    /// The text of memo `key` open for editing, with the formatting toolbar
    /// over it. Returns the new HTML when the text or its formatting
    /// changed.
    fn notes_editor(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        key: &str,
        prefix: &str,
    ) -> Option<String> {
        let text_id = egui::Id::new(("memo_text", key));
        let ctx = ui.ctx().clone();
        let range = egui::text_edit::TextEditState::load(&ctx, text_id)
            .and_then(|s| s.cursor.char_range())
            .map(|r| {
                let (a, b) = (r.primary.index, r.secondary.index);
                (a.min(b), a.max(b))
            });
        let (from, to) = range.unwrap_or((0, 0));
        if std::mem::take(&mut self.detail.notes.seal_next) {
            self.history.seal();
        }
        let mut buffer = self
            .detail
            .notes
            .buffers
            .remove(key)
            .expect("the buffer is read first");
        let mut changed = false;
        // The toolbar.
        let bar = Rect::from_min_size(rect.min, Vec2::new(rect.width(), TOOLBAR_H));
        let mut command = None;
        let mut responses = vec![];
        {
            let ui = &mut area(ui, bar.shrink2(Vec2::new(2., 1.)));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                for (icon, tip, name, cmd) in TOOLBAR {
                    let on = match cmd {
                        Command::Format(f) => match buffer.typing.filter(|_| from == to) {
                            Some(s) => s.has(f),
                            None => buffer.memo.has(from, to, f),
                        },
                        Command::List(kind) => {
                            let line = buffer.memo.line_of(from);
                            buffer.memo.lines.get(line) == Some(&kind)
                        }
                        Command::Clear => false,
                    };
                    let r = chrome::tool_button(ui, icon, tip, true, on, false);
                    if r.clicked() {
                        command = Some(cmd);
                    }
                    responses.push((format!("{prefix}_{name}"), r));
                }
            });
        }
        for (name, r) in &responses {
            self.action(name, r);
        }
        let line = Stroke::new(1_f32, LINE);
        ui.painter().hline(bar.x_range(), bar.bottom() - 0.5, line);
        if let Some(cmd) = command {
            let before = buffer.memo.clone();
            let memo = &mut buffer.memo;
            match cmd {
                Command::Format(f) if from < to => memo.toggle(from, to, f),
                Command::Format(f) => {
                    let now = buffer.typing.unwrap_or_else(|| {
                        let mut s = Style::default();
                        for f in [Format::Bold, Format::Italic, Format::Underline] {
                            s = s.with(f, memo.has(from, from, f));
                        }
                        s
                    });
                    buffer.typing = Some(now.with(f, !now.has(f)));
                    buffer.cursor = range;
                }
                Command::List(kind) => memo.toggle_lines(from, to, kind),
                Command::Clear => {
                    for s in memo.styles.iter_mut().take(to).skip(from) {
                        *s = Style::default();
                    }
                    let (a, b) = (memo.line_of(from), memo.line_of(to));
                    for k in memo.lines.iter_mut().take(b + 1).skip(a) {
                        *k = LineKind::Plain;
                    }
                    if from == to {
                        buffer.typing = Some(Style::default());
                        buffer.cursor = range;
                    }
                }
            }
            if buffer.memo != before {
                changed = true;
                self.history.seal();
                self.detail.notes.seal_next = true;
            }
            ui.memory_mut(|m| m.request_focus(text_id));
        }
        // The text.
        let text_rect = Rect::from_min_max(Pos2::new(rect.left(), bar.bottom()), rect.max);
        let mut text = buffer.memo.text.clone();
        let base = buffer.memo.clone();
        let typing = buffer.typing;
        let mut layouter = |ui: &egui::Ui, b: &dyn egui::TextBuffer, wrap: f32| {
            let s = b.as_str();
            let job = if s == base.text {
                layout_job(&base, usize::MAX, wrap)
            } else {
                layout_job(&base.after_edit(s, typing), usize::MAX, wrap)
            };
            ui.fonts_mut(|f| f.layout_job(job))
        };
        let output = {
            let ui = &mut area(ui, text_rect);
            egui::ScrollArea::vertical()
                .id_salt(("memo", key))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::TextEdit::multiline(&mut text)
                        .id(text_id)
                        .frame(false)
                        .desired_width(text_rect.width() - 12.)
                        .desired_rows(((text_rect.height() - 4.) / 14.).max(1.) as usize)
                        .layouter(&mut layouter)
                        .show(ui)
                })
                .inner
        };
        crate::dialog_keys::note_keeps_enter(&output.response);
        let cursor = output
            .cursor_range
            .map(|r| (r.primary.index, r.secondary.index));
        if text != buffer.memo.text {
            buffer.memo = buffer.memo.after_edit(&text, buffer.typing);
            changed = true;
        } else if command.is_none() && cursor.is_some() && cursor != buffer.cursor {
            buffer.typing = None;
        }
        if cursor.is_some() {
            buffer.cursor = cursor;
        }
        let painter = ui.painter().with_clip_rect(output.text_clip_rect);
        paint_markers(&painter, &output.galley, output.galley_pos, &buffer.memo);
        let html = changed.then(|| buffer.memo.to_html());
        if let Some(html) = &html {
            buffer.source = html.clone();
        }
        self.detail.notes.buffers.insert(key.to_string(), buffer);
        html
    }

    /// A captioned memo section (Notebook, Steps): heading bar, the text
    /// with its formatting, and Modify, Print and Copy under it, their
    /// action keys `{prefix}_modify`, `{prefix}_print` and `{prefix}_copy`.
    /// Modify (when `enabled`) opens the text for editing with its
    /// formatting toolbar. Returns the new HTML when the text changed.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn memo_section(
        &mut self,
        ui: &mut egui::Ui,
        r: Rect,
        caption: &str,
        caption_x: f32,
        key: String,
        html: &str,
        enabled: bool,
        prefix: &str,
    ) -> Option<String> {
        let p = ui.painter().clone();
        form::section(&p, r, caption, 21., caption_x);
        let memo = Rect::from_min_max(
            Pos2::new(r.left() + 1., r.top() + 24.),
            Pos2::new(r.right() - 1., r.bottom() - 34.),
        );
        form::fill(&p, memo, Color32::WHITE);
        let editable = self.notes_buffer(&key, html).editable;
        let can_edit = enabled && editable;
        let notes = &mut self.detail.notes;
        let other = notes.editing.as_deref() != Some(key.as_str());
        if other || !can_edit {
            notes.editing = None;
        }
        let editing = notes.editing.is_some();
        let result = if editing {
            self.notes_editor(ui, memo, &key, prefix)
        } else {
            self.notes_view(ui, memo, &key);
            None
        };
        let buttons = Rect::from_min_max(
            Pos2::new(r.left() + 5., r.bottom() - 29.),
            Pos2::new(r.right() - 5., r.bottom() - 8.),
        );
        let mut child = area(ui, buttons);
        let text = self.detail.notes.buffers[&key].memo.text.clone();
        let [modify, print, copy] = form::notes_buttons(can_edit, !text.trim().is_empty());
        let (m, pr, c) = child
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(form::GAP, 0.);
                let mut m = form::command_button(ui, modify.0, None, modify.1);
                if enabled && !can_edit {
                    m = m.on_disabled_hover_text(NOT_EDITABLE_HINT);
                }
                let pr = form::command_button(ui, print.0, None, print.1)
                    .on_disabled_hover_text(form::NOTES_PRINT_HINT);
                let c = form::command_button(ui, copy.0, None, copy.1);
                (m, pr, c)
            })
            .inner;
        self.action(&format!("{prefix}_modify"), &m);
        self.action(&format!("{prefix}_print"), &pr);
        self.action(&format!("{prefix}_copy"), &c);
        if m.clicked() && can_edit {
            self.detail.notes.editing = Some(key.clone());
            ui.memory_mut(|mem| mem.request_focus(egui::Id::new(("memo_text", &key))));
        }
        if c.clicked() {
            ui.ctx().copy_text(text);
        }
        result
    }

    /// The Notebook tab of Activity, WBS and Project Details: the topics of
    /// `owner` with Add and Delete under them, and the selected topic's
    /// text in the notes pane. `prefix` keys the actions (`{prefix}_add`,
    /// `{prefix}_delete`, `{prefix}_grid` and the pane's), `split` the
    /// splitter. Delete and text edits are written here, each one undo
    /// step; Add is the caller's (its topic picker).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn notebook_pane(
        &mut self,
        ui: &mut egui::Ui,
        content: Rect,
        prefix: &str,
        split_key: &str,
        owner: Option<Owner>,
        selected: Option<usize>,
        editable: bool,
    ) -> NotebookOut {
        let pid = self.project.model.project.proj_id;
        let memos = owner.map_or(vec![], |o| notebooks::memos(&self.project.document, pid, o));
        let editable = editable && owner.is_some();
        let selected = selected
            .filter(|&r| r < memos.len())
            .or((!memos.is_empty()).then_some(0));
        let at = self.detail_split(ui, split_key, content, 186., [140., 260.]);
        let (left, right) = split(ui, content, at);
        let mut out = NotebookOut::default();
        let delete;
        {
            let ui = &mut area(ui, left);
            let height = ui.available_height() - form::BUTTON_BAR;
            let grid_out = grid(
                ui,
                &format!("{prefix}_grid"),
                &[("Notebook Topic", left.width() - 2.)],
                memos.len(),
                selected,
                Vec2::new(left.width(), height),
                &mut |ui, ev, _| {
                    if let Ev::Cell(r, _) = ev {
                        cell(ui, &memos[r].topic);
                    }
                },
            );
            out.select = grid_out.clicked;
            let (add, del) = form::button_row(ui, |ui| {
                let add = form::add_button(ui, "Add", editable);
                let del = form::command_button(ui, "Delete", None, editable && selected.is_some());
                (add, del)
            });
            self.action(&format!("{prefix}_add"), &add);
            self.action(&format!("{prefix}_delete"), &del);
            out.add = add.clicked() && editable;
            delete = del.clicked();
        }
        let shown = selected.and_then(|r| memos.get(r));
        let changed = match (shown, owner) {
            (Some(m), Some(o)) => self
                .memo_section(
                    ui,
                    right,
                    &m.topic,
                    23.,
                    format!("{prefix}-{}", m.id),
                    &m.text,
                    editable,
                    prefix,
                )
                .map(|html| (o, m.id, html)),
            _ => {
                let caption = form::none_selected("Notebook Topic");
                let key = format!("{prefix}-none");
                let _ = self.memo_section(ui, right, &caption, 23., key, "", false, prefix);
                None
            }
        };
        if let Some((o, id, html)) = changed {
            match notebooks::set_text(&mut self.project.document, o, id, &html) {
                Ok(true) => self.touch_records(),
                Ok(false) => {}
                Err(e) => self.error(e),
            }
        }
        if let (true, Some(m), Some(o)) = (delete && editable, shown, owner) {
            match notebooks::delete(&mut self.project.document, o, m.id) {
                Ok(()) => {
                    self.notes_step();
                    out.cleared = true;
                }
                Err(e) => self.error(e),
            }
        }
        out
    }
}

/// What the Notebook tab asks of its caller.
#[derive(Default, Debug)]
pub struct NotebookOut {
    /// The topic row clicked.
    pub select: Option<usize>,
    /// A topic was deleted: the selection goes back to the first.
    pub cleared: bool,
    /// Add was pressed: open the topic picker.
    pub add: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every Notebook tab draws the one notebook pane: the empty pane's
    /// caption is written in this file only.
    #[test]
    fn the_notebook_tabs_share_one_pane() {
        fn visit(dir: &std::path::Path, found: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    visit(&path, found);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    if text.contains(concat!("none_selected(", "\"Notebook Topic\")")) {
                        found.push(path.file_name().unwrap().to_string_lossy().into());
                    }
                }
            }
        }
        let mut found = vec![];
        visit(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut found,
        );
        assert_eq!(found, vec!["notes_pane.rs".to_string()]);
    }

    #[test]
    fn list_items_are_indented_and_bold_text_takes_the_bold_face() {
        let mut memo = RichMemo::plain("Crew\nPump");
        memo.toggle(0, 4, Format::Bold);
        memo.toggle_lines(5, 5, LineKind::Bullet);
        let job = layout_job(&memo, usize::MAX, 300.);
        assert_eq!(job.text, "Crew\nPump");
        let first = &job.sections[0];
        assert_eq!(first.format.font_id, chrome::bold_font());
        assert_eq!(first.leading_space, 0.);
        let item = job
            .sections
            .iter()
            .find(|s| job.text[s.byte_range.clone()].starts_with("Pump"))
            .expect("the list line has its own section");
        assert_eq!(item.leading_space, INDENT);
        assert_eq!(item.format.font_id, form::font());
        // A long memo lays out only what was asked for.
        let long = RichMemo::plain(&"x".repeat(10_000));
        assert_eq!(layout_job(&long, 64, 300.).text.len(), 64);
    }
}
