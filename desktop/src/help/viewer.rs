//! The help window: search and the table of contents (or the search
//! results) at the left, the topic at the right, and Close, Back, Forward
//! and Contents in the dialog kit's button column. Escape closes it.
//!
//! It opens beside the dialogs it documents rather than over them, can be
//! moved by its title bar and enlarged by the grip in its bottom-right
//! corner, and its topic text can be selected and copied.
use super::*;

const LIST_W: f32 = 236.;
const TOPIC_W: f32 = 500.;
const PANE_H: f32 = 452.;

/// What the user did in the window this frame.
#[derive(Default)]
struct Clicks {
    close: bool,
    back: bool,
    forward: bool,
    contents: bool,
    go: Option<String>,
}

impl App {
    pub(crate) fn help_window(&mut self, ctx: &egui::Context) {
        if !self.help.open {
            return;
        }
        let lib = library();
        if lib.topic(&self.help.topic).is_none() {
            self.help.topic = OVERVIEW.into();
        }
        let mut k = Clicks::default();
        let mut rects: Vec<(String, Rect)> = vec![];
        let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("help_viewer"));
        let opening = !ctx.memory(|m| m.areas().is_visible(&layer));
        let at = opening.then(|| {
            // The size at the starting height, keeping any width the user
            // added; the height may be cut to stay clear of the dialogs.
            let grow = self.help.grow;
            let natural = ctx
                .memory(|m| m.area_rect(layer.id))
                .map_or(START_SIZE, |r| r.size() - grow);
            let size = natural + Vec2::new(grow.x, 0.);
            let (pos, height) = beside_dialogs(ctx, size);
            self.help.grow.y = height - natural.y;
            pos
        });
        chrome::dialog_window(WINDOW_TITLE)
            .id(egui::Id::new("help_viewer"))
            .order(egui::Order::Foreground)
            .movable_at(at)
            .resizable(false)
            .show(ctx, |ui| {
                // Enter in the search field does not close the window.
                crate::dialog_keys::mark_enter_idle(ui);
                let buttons = [
                    chrome::DialogButton::new("Close", None),
                    chrome::DialogButton::new("Back", None)
                        .grouped()
                        .enabled(!self.help.back.is_empty()),
                    chrome::DialogButton::new("Forward", Some(chrome::Icon::Next))
                        .enabled(!self.help.forward.is_empty()),
                    chrome::DialogButton::new("Contents", Some(chrome::Icon::Help)).grouped(),
                ];
                let (_, clicked, responses) = chrome::dialog_with_buttons(ui, &buttons, |ui| {
                    self.help_body(ui, lib, &mut k, &mut rects);
                });
                for (name, r) in ["help_close", "help_back", "help_forward", "help_contents"]
                    .iter()
                    .zip(&responses)
                {
                    rects.push((name.to_string(), r.rect));
                }
                match clicked {
                    Some(0) => k.close = true,
                    Some(1) => k.back = true,
                    Some(2) => k.forward = true,
                    Some(3) => k.contents = true,
                    _ => {}
                }
                self.help_resize_grip(ui, &mut rects);
            });
        // Above every dialog, a modal one too, whichever was shown first.
        ctx.move_to_top(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("help_viewer"),
        ));
        for (name, r) in rects {
            self.action_rects
                .insert(name, [r.left(), r.top(), r.right(), r.bottom()]);
        }
        let alt = egui::Modifiers::ALT;
        if ctx.input_mut(|i| i.consume_key(alt, Key::ArrowLeft)) {
            k.back = true;
        }
        if ctx.input_mut(|i| i.consume_key(alt, Key::ArrowRight)) {
            k.forward = true;
        }
        if let Some(id) = k.go {
            self.help.go(&id);
        }
        if k.back {
            self.help.back();
        }
        if k.forward {
            self.help.forward();
        }
        if k.contents {
            self.help.query.clear();
            self.help.go(OVERVIEW);
        }
        // Escape (and the title bar's close box) is read in `help_keys`.
        if k.close {
            self.close_help();
        }
    }

    fn help_body(
        &mut self,
        ui: &mut egui::Ui,
        lib: &'static Library,
        k: &mut Clicks,
        rects: &mut Vec<(String, Rect)>,
    ) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Search").font(chrome::ui_font()));
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.help.query)
                    .id(egui::Id::new("help_search"))
                    .font(chrome::ui_font())
                    .hint_text("Words to find in every topic")
                    .desired_width(LIST_W - 48.),
            );
            // The sunken border of the other edit fields.
            let r = edit.rect.expand(1.);
            let p = ui.painter();
            let line = |a: Pos2, b: Pos2, color: Color32| {
                p.line_segment([a, b], Stroke::new(1_f32, color));
            };
            line(r.left_top(), r.right_top(), chrome::pal::SHADOW);
            line(r.left_top(), r.left_bottom(), chrome::pal::SHADOW);
            line(r.left_bottom(), r.right_bottom(), Color32::WHITE);
            line(r.right_top(), r.right_bottom(), Color32::WHITE);
            rects.push(("help_search".into(), edit.rect));
        });
        ui.add_space(4.);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 6.;
            let grow = self.help.grow;
            pane(ui, "help_list", LIST_W, PANE_H + grow.y, false, |ui| {
                if self.help.query.trim().is_empty() {
                    contents_list(ui, lib, &self.help.topic, k, rects);
                } else {
                    results_list(ui, lib, &self.help.query, k, rects);
                }
            });
            let top = std::mem::take(&mut self.help.scroll_top);
            pane(
                ui,
                "help_topic",
                TOPIC_W + grow.x,
                PANE_H + grow.y,
                top,
                |ui| {
                    if let Some(t) = lib.topic(&self.help.topic) {
                        topic_body(ui, t, k, rects);
                    }
                },
            );
        });
    }

    /// The grip in the bottom-right corner: dragging it resizes the panes,
    /// no narrower than at the start, no shorter than `MIN_HEIGHT`, and
    /// no larger than the screen.
    fn help_resize_grip(&mut self, ui: &mut egui::Ui, rects: &mut Vec<(String, Rect)>) {
        let full = ui.min_rect();
        let grip = Rect::from_min_size(full.max - Vec2::splat(GRIP), Vec2::splat(GRIP));
        let r = ui.interact(grip, egui::Id::new("help_resize"), Sense::drag());
        rects.push(("help_resize".into(), grip));
        if r.hovered() || r.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
        }
        if r.dragged() {
            let screen = ui.ctx().content_rect();
            // The window is `full` at the current growth.
            let least = Vec2::new(0., MIN_HEIGHT - (full.height() - self.help.grow.y));
            let most = self.help.grow + (screen.max - full.max).max(Vec2::ZERO);
            let grow = self.help.grow + r.drag_delta();
            self.help.grow = grow.max(least).min(most.max(least));
        }
        crate::dialog_kit::paint_size_grip(ui.painter(), grip);
    }
}

/// The help window's outer size at its starting pane sizes, before it has
/// been measured (the first frame, which egui does not show).
const START_SIZE: Vec2 = Vec2::new(948., PANE_H + 76.);
/// The least height the window opens with to stay clear of a dialog.
const MIN_HEIGHT: f32 = 300.;
/// The side of the resize grip.
const GRIP: f32 = crate::dialog_kit::SIZE_GRIP;

/// Where the help window opens and how tall: beside the open dialogs (right,
/// then left, then below, then above them) so it does not cover them,
/// shortened to fit above or below them when it must be; centred when no
/// dialog is open, in the corner farthest from them when nothing fits.
fn beside_dialogs(ctx: &egui::Context, size: Vec2) -> (Pos2, f32) {
    let screen = ctx.content_rect();
    let clamp = |p: Pos2| p.max(screen.min).min((screen.max - size).max(screen.min));
    let dialogs = crate::dialog_kit::open_dialog_clients(ctx)
        .into_iter()
        .filter(|(name, _)| name != WINDOW_TITLE)
        .map(|(_, r)| r)
        .reduce(|a, b| a.union(b));
    let Some(d) = dialogs else {
        return (clamp(screen.center() - size / 2.), size.y);
    };
    // The dialogs' title bars sit above their client areas.
    let d = Rect::from_min_max(d.min - Vec2::new(1., 32.), d.max + Vec2::splat(1.));
    let gap = 8.;
    let y = d.top().min(screen.bottom() - size.y).max(screen.top());
    let x = d.left().min(screen.right() - size.x).max(screen.left());
    let below = screen.bottom() - d.bottom() - gap;
    let above = d.top() - screen.top() - gap;
    if screen.right() - d.right() >= size.x + gap {
        (Pos2::new(d.right() + gap, y), size.y)
    } else if d.left() - screen.left() >= size.x + gap {
        (Pos2::new(d.left() - gap - size.x, y), size.y)
    } else if below.max(above) >= size.y.min(MIN_HEIGHT) {
        let height = size.y.min(below.max(above));
        if below >= above {
            (Pos2::new(x, d.bottom() + gap), height)
        } else {
            (Pos2::new(x, d.top() - gap - height), height)
        }
    } else {
        // No room beside them: the corner farthest from their centre.
        let right = d.center().x < screen.center().x;
        let low = d.center().y < screen.center().y;
        let corner = Pos2::new(
            if right {
                screen.right() - size.x
            } else {
                screen.left()
            },
            if low {
                screen.bottom() - size.y
            } else {
                screen.top()
            },
        );
        (clamp(corner), size.y)
    }
}

/// A white, sunken, scrolling pane of a fixed size.
fn pane(
    ui: &mut egui::Ui,
    salt: &str,
    width: f32,
    height: f32,
    to_top: bool,
    add: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::NONE
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1_f32, chrome::pal::SHADOW))
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            ui.set_width(width - 12.);
            ui.set_height(height - 12.);
            let mut area = egui::ScrollArea::vertical()
                .id_salt(salt)
                .auto_shrink([false, false])
                .max_height(height - 12.);
            if to_top {
                area = area.vertical_scroll_offset(0.);
            }
            // The panes sit side by side; their content runs top down.
            area.show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.set_width(width - 26.);
                    add(ui)
                })
            });
        });
}

fn contents_list(
    ui: &mut egui::Ui,
    lib: &Library,
    current: &str,
    k: &mut Clicks,
    rects: &mut Vec<(String, Rect)>,
) {
    ui.spacing_mut().item_spacing.y = 1.;
    for (n, section) in lib.contents.iter().enumerate() {
        if n > 0 {
            ui.add_space(6.);
        }
        ui.label(RichText::new(&section.title).font(chrome::bold_font()));
        for id in &section.topics {
            let Some(t) = lib.topic(id) else { continue };
            let r = ui
                .horizontal(|ui| {
                    ui.add_space(10.);
                    crate::dialog_kit::flow_selectable(ui, t.id == current, &t.title, 180.)
                })
                .inner;
            if r.clicked() {
                k.go = Some(t.id.to_owned());
            }
            rects.push((format!("help_toc_{}", t.id), r.rect));
        }
    }
}

fn results_list(
    ui: &mut egui::Ui,
    lib: &Library,
    query: &str,
    k: &mut Clicks,
    rects: &mut Vec<(String, Rect)>,
) {
    let hits = lib.search(query);
    let count = match hits.len() {
        0 => "No topic has all of these words.".to_owned(),
        1 => "1 topic found".to_owned(),
        n => format!("{n} topics found"),
    };
    ui.label(RichText::new(count).font(chrome::bold_font()));
    ui.add_space(4.);
    let first = query.split_whitespace().next().unwrap_or("").to_lowercase();
    for (n, t) in hits.iter().enumerate() {
        let r = crate::dialog_kit::flow_selectable(ui, false, &t.title, 260.);
        if r.clicked() {
            k.go = Some(t.id.to_owned());
        }
        rects.push((format!("help_result_{n}"), r.rect));
        if let Some(line) = snippet(t, &first) {
            ui.horizontal(|ui| {
                ui.add_space(8.);
                ui.add(
                    egui::Label::new(
                        RichText::new(line)
                            .font(FontId::new(11., egui::FontFamily::Proportional))
                            .color(chrome::pal::MENU_DISABLED),
                    )
                    .wrap(),
                );
            });
        }
        ui.add_space(3.);
    }
}

/// A short piece of the first block that holds `word`.
fn snippet(t: &Topic, word: &str) -> Option<String> {
    let text = t
        .blocks
        .iter()
        .map(block_text)
        .find(|b| b.to_lowercase().contains(word))?;
    let lower = text.to_lowercase();
    let at = lower.find(word).unwrap_or(0);
    let start = text[..at]
        .char_indices()
        .rev()
        .nth(40)
        .map(|(i, _)| i)
        .unwrap_or(0);
    let piece: String = text[start..].chars().take(90).collect();
    let lead = if start > 0 { "..." } else { "" };
    let tail = if text[start..].chars().count() > 90 {
        "..."
    } else {
        ""
    };
    Some(format!("{lead}{}{tail}", piece.trim()))
}

fn topic_body(ui: &mut egui::Ui, t: &Topic, k: &mut Clicks, rects: &mut Vec<(String, Rect)>) {
    let title = FontId::new(16., egui::FontFamily::Name("Arial Bold".into()));
    ui.add(
        egui::Label::new(RichText::new(&t.title).font(title).color(chrome::INK))
            .wrap()
            .selectable(true),
    );
    ui.add_space(4.);
    let width = ui.available_width();
    for b in &t.blocks {
        match b {
            Block::Heading(h) => {
                ui.add_space(8.);
                ui.add(
                    egui::Label::new(
                        RichText::new(h)
                            .font(chrome::bold_font())
                            .color(chrome::BLUE),
                    )
                    .selectable(true),
                );
                ui.add_space(1.);
            }
            Block::Para(s) => {
                spans_ui(ui, s, width, k, rects);
                ui.add_space(4.);
            }
            Block::Bullet(s) => marked(ui, "\u{2022}", s, width, k, rects),
            Block::Table(rows) => {
                let id = egui::Id::new(("help_table", &t.title, rects.len()));
                let table = crate::markdown::table_ui(ui, id, rows, &help_style(), width);
                if let Some(target) = table.clicked {
                    k.go = Some(target);
                }
                for (target, r) in table.links {
                    rects.push((format!("help_link_{target}"), r));
                }
                ui.add_space(4.);
            }
            Block::Step(n, s) => marked(ui, &format!("{n}."), s, width, k, rects),
            Block::Note(s) => {
                ui.add_space(2.);
                egui::Frame::NONE
                    .fill(chrome::pal::FOCUS_INNER)
                    .stroke(Stroke::new(1_f32, chrome::pal::HOT_BORDER))
                    .inner_margin(egui::Margin::same(5))
                    .show(ui, |ui| {
                        let mut all = vec![Span::Bold("Watch out: ".into())];
                        all.extend(s.iter().cloned());
                        spans_ui(ui, &all, width - 12., k, rects);
                    });
                ui.add_space(4.);
            }
        }
    }
}

/// A bullet or numbered step: the mark, then the wrapped text beside it.
fn marked(
    ui: &mut egui::Ui,
    mark: &str,
    s: &[Span],
    width: f32,
    k: &mut Clicks,
    rects: &mut Vec<(String, Rect)>,
) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.;
        let (r, _) = ui.allocate_exact_size(Vec2::new(18., 14.), Sense::hover());
        ui.painter().text(
            r.left_top() + Vec2::new(4., 0.),
            Align2::LEFT_TOP,
            mark,
            chrome::ui_font(),
            chrome::INK,
        );
        ui.vertical(|ui| spans_ui(ui, s, width - 18., k, rects));
    });
    ui.add_space(2.);
}

/// The look of a topic's text.
fn help_style() -> crate::markdown::Style {
    crate::markdown::Style {
        regular: chrome::ui_font(),
        bold: chrome::bold_font(),
        ink: chrome::INK,
        code: (chrome::bold_font(), chrome::pal::MENU_DISABLED),
        code_fill: Color32::TRANSPARENT,
        link: chrome::BLUE,
    }
}

/// One paragraph as a single wrapped label (so the text measures as one
/// control), with its links as clickable runs inside it.
fn spans_ui(
    ui: &mut egui::Ui,
    s: &[Span],
    width: f32,
    k: &mut Clicks,
    rects: &mut Vec<(String, Rect)>,
) {
    let crate::markdown::Laid { job, links } = crate::markdown::layout(s, &help_style(), width);
    let sense = if links.is_empty() {
        Sense::hover()
    } else {
        Sense::click()
    };
    // Selectable: a drag selects text for Ctrl+C; a click still follows a
    // link.
    let (pos, galley, response) = egui::Label::new(job)
        .wrap()
        .selectable(true)
        .sense(sense)
        .layout_in_ui(ui);
    egui::text_selection::LabelSelectionState::label_text_selection(
        ui,
        &response,
        pos,
        galley.clone(),
        chrome::INK,
        Stroke::NONE,
    );
    if links.is_empty() {
        return;
    }
    let link_at = |p: Pos2| crate::markdown::link_at(&galley, pos, &links, p);
    if response.hover_pos().and_then(link_at).is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if response.clicked() {
        if let Some(target) = response.interact_pointer_pos().and_then(link_at) {
            k.go = Some(target.to_owned());
        }
    }
    for (target, r) in crate::markdown::link_rects(&galley, pos, &links) {
        rects.push((format!("help_link_{target}"), r));
    }
}
