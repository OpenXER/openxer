//! The message list: the one control that shows a conversation. A white,
//! framed pane that scrolls and stays on the newest line while a reply
//! streams (unless the user scrolled up). The user's messages sit at the
//! right in the light blue fill; the agent's replies are plain text in the
//! shared Markdown subset (`crate::markdown`) with Activity IDs as links
//! (with hover previews, copy and Select All actions); tool activity and
//! proposed changes are blocks of 18 px rows on the kit's list rows
//! (`kit::rows_in`), with multi-tool folding, schedule impact summaries,
//! clickable IDs and inline Undo; status lines close a reply. With no
//! messages yet it shows the empty state: which agent is ready and the
//! starter questions.
use super::backend::Mode;
use super::links;
use super::model::{
    AskPrompt, Conversation, Item, ProposalState, QuestionState, StatusKind, ToolRow, ToolStatus,
};
use crate::chrome::{self, pal, INK};
use crate::dialog_kit::{self as kit, Canvas};
use crate::markdown::{self, Block, Span};
use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use std::collections::{HashMap, HashSet};

/// Gap between items of the list.
const GAP: f32 = 7.;
/// Height of one line of text.
const LINE: f32 = 15.;
/// The widest a user message gets.
const USER_W: f32 = 262.;
/// Width of the first column of a tool or change block.
const FIRST_COL: f32 = 74.;
/// A starter question's row and the space under it.
const STARTER_H: f32 = 22.;
const STARTER_PITCH: f32 = 28.;
/// Maximum change rows shown before a proposal folds with "Show all N changes".
const PROPOSAL_FOLD_ROWS: usize = 8;

/// Extra view state passed into [`show`].
pub(crate) struct ListView<'a> {
    pub codes: &'a HashSet<String>,
    pub tips: &'a HashMap<String, String>,
    pub plan: bool,
    pub mode: Mode,
    pub expanded_tools: &'a HashSet<usize>,
    pub expanded_proposals: &'a HashSet<usize>,
    pub undoable_proposal: Option<u64>,
    pub elapsed_secs: Option<u64>,
    pub composer_focused: bool,
}

/// Persistent click-and-drag text selection across message blocks in the list.
#[derive(Clone, Debug, Default)]
struct TextSelection {
    dragging: bool,
    press_pos: Pos2,
    anchor: Option<(usize, usize)>,
    head: Option<(usize, usize)>,
    /// Selected text assembled during the last frame.
    text: String,
}

/// Per-frame context threaded into text blocks so dragging can span paragraphs,
/// headings, bullets and user messages cleanly.
struct SelCtx {
    frame: Rect,
    sel: TextSelection,
    next_block: usize,
    collected: Vec<String>,
    frame_head: Option<(usize, usize)>,
    pointer_pos: Option<Pos2>,
    primary_pressed: bool,
    primary_down: bool,
    primary_released: bool,
    pressed_on_block: bool,
}

impl SelCtx {
    fn ordered(&self) -> Option<((usize, usize), (usize, usize))> {
        let (a, h) = (self.sel.anchor?, self.sel.head?);
        if a == h {
            None
        } else if a < h {
            Some((a, h))
        } else {
            Some((h, a))
        }
    }

    fn drag_moved(&self) -> bool {
        self.pointer_pos
            .is_some_and(|p| p.distance(self.sel.press_pos) > 4.)
    }

    /// Track pointer interaction for one painted galley, paint its selection
    /// highlight if overlapped, and collect its selected substring.
    fn interact_galley(
        &mut self,
        painter: &egui::Painter,
        galley: &egui::Galley,
        hit_rect: Rect,
        text_pos: Pos2,
    ) {
        let bid = self.next_block;
        self.next_block += 1;
        let text = galley.text();
        let char_len = text.chars().count();
        if let Some(pos) = self.pointer_pos {
            let in_list = self.frame.contains(pos);
            if self.primary_pressed && in_list && hit_rect.contains(pos) {
                let c = galley.cursor_from_pos(pos - text_pos).index.min(char_len);
                self.sel.dragging = true;
                self.sel.press_pos = pos;
                self.sel.anchor = Some((bid, c));
                self.sel.head = Some((bid, c));
                self.frame_head = Some((bid, c));
                self.pressed_on_block = true;
            } else if self.sel.dragging && self.primary_down {
                if pos.y < hit_rect.top() {
                    if self.frame_head.is_none() {
                        self.frame_head = Some((bid, 0));
                    }
                } else if pos.y <= hit_rect.bottom() {
                    let c = if pos.x < text_pos.x {
                        0
                    } else if pos.x > hit_rect.right() {
                        char_len
                    } else {
                        galley.cursor_from_pos(pos - text_pos).index.min(char_len)
                    };
                    self.frame_head = Some((bid, c));
                    self.sel.head = Some((bid, c));
                } else {
                    self.frame_head = Some((bid, char_len));
                }
            }
        }
        if let Some((start, end)) = self.ordered() {
            if bid >= start.0 && bid <= end.0 {
                let c0 = if bid == start.0 { start.1 } else { 0 }.min(char_len);
                let c1 = if bid == end.0 { end.1 } else { char_len }.min(char_len);
                if c0 < c1 {
                    paint_galley_selection(painter, galley, text_pos, c0, c1);
                    let slice = char_slice(text, c0, c1);
                    if !slice.is_empty() {
                        self.collected.push(slice.to_string());
                    }
                }
            }
        }
    }
}

fn char_slice(s: &str, c0: usize, c1: usize) -> &str {
    let mut b0 = s.len();
    let mut b1 = s.len();
    for (idx, (byte, _)) in s.char_indices().enumerate() {
        if idx == c0 {
            b0 = byte;
        }
        if idx == c1 {
            b1 = byte;
            break;
        }
    }
    if b0 > b1 {
        ""
    } else {
        &s[b0..b1]
    }
}

fn paint_galley_selection(
    painter: &egui::Painter,
    galley: &egui::Galley,
    pos: Pos2,
    c0: usize,
    c1: usize,
) {
    if c0 >= c1 {
        return;
    }
    let fill = Color32::from_rgba_unmultiplied(0, 120, 215, 76);
    let r0 = galley.pos_from_cursor(egui::text::CCursor::new(c0));
    let r1 = galley.pos_from_cursor(egui::text::CCursor::new(c1));
    if (r0.top() - r1.top()).abs() < 1. {
        let rect = Rect::from_min_max(
            pos + r0.min.to_vec2(),
            pos + Vec2::new(r1.left().max(r0.left() + 2.), r0.bottom()),
        );
        painter.rect_filled(rect, 0., fill);
        return;
    }
    for placed in &galley.rows {
        let rr = placed.rect();
        if rr.bottom() <= r0.top() + 1. || rr.top() >= r1.bottom() - 1. {
            continue;
        }
        let left = if (rr.top() - r0.top()).abs() < 2. {
            r0.left()
        } else {
            rr.left()
        };
        let right = if (rr.top() - r1.top()).abs() < 2. {
            r1.left()
        } else {
            rr.right()
        };
        if right > left {
            painter.rect_filled(
                Rect::from_min_max(
                    pos + Vec2::new(left, rr.top()),
                    pos + Vec2::new(right, rr.bottom()),
                ),
                0.,
                fill,
            );
        }
    }
}

/// What the list shows with no messages yet.
pub(crate) enum Empty<'a> {
    /// No project is open.
    NoProject,
    /// The backend offers no agent.
    NoAgent,
    NotInstalled {
        name: &'a str,
    },
    SignIn {
        name: &'a str,
        privacy: &'a str,
    },
    SigningIn {
        name: &'a str,
    },
    /// The agent is still being looked for.
    Checking {
        name: &'a str,
    },
    /// A route set up in OpenXER that is not set up yet.
    SetUp {
        name: &'a str,
        privacy: &'a str,
    },
    /// The agent is ready: what it says about itself, and the starters.
    Ready {
        line: String,
        starters: &'a [String],
    },
    /// No assistant is ready, or the user asked: the routes to choose
    /// from, each with its state and its action.
    Setup {
        rows: &'a [SetupRow],
        /// A line above the rows (no project is open).
        note: Option<&'a str>,
    },
}

/// One route of the setup list.
pub(crate) struct SetupRow {
    pub name: String,
    pub mark: crate::ai_agents::row::Mark,
    pub state: String,
    pub dim: bool,
    pub detail: String,
    /// (caption, enabled) of the one action, and of a second one beneath.
    pub primary: Option<(String, bool)>,
    pub secondary: Option<(String, bool)>,
}

/// The setup list's key of row `n`'s first or second button.
pub(crate) fn setup_key(n: usize, second: bool) -> String {
    format!("chat_setup_{n}_{}", if second { "second" } else { "first" })
}

/// What the user did in the list this frame.
#[derive(Default)]
pub(crate) struct ListOut {
    /// An Activity ID link was clicked.
    pub link: Option<String>,
    /// Select all Activity IDs mentioned in a reply.
    pub select_all: Option<Vec<String>>,
    /// Copy text to clipboard with a status bar message.
    pub copy_text: Option<(String, &'static str)>,
    /// User pressed inside selectable chat text (so the composer should
    /// surrender keyboard focus and let Ctrl+C copy the selection).
    pub text_interacted: bool,
    pub apply: Option<u64>,
    pub discard: Option<u64>,
    /// Inline Undo clicked on an applied proposal.
    pub undo_proposal: Option<u64>,
    /// User answered an interactive question card: `(question_id, answer_text)`.
    pub answer_question: Option<(u64, String)>,
    /// User skipped/dismissed an interactive question card.
    pub dismiss_question: Option<u64>,
    /// Expand/collapse a folded tool block.
    pub toggle_tools: Option<usize>,
    /// Expand/collapse a large proposal block.
    pub toggle_proposal: Option<usize>,
    /// One-click Switch to Edit & Retry after an Ask-mode refusal.
    pub switch_edit_retry: bool,
    /// A starter question was clicked.
    pub starter: Option<usize>,
    pub sign_in: bool,
    pub cancel_sign_in: bool,
    /// A setup row's button: the row, and whether it is the second one.
    pub setup: Option<(usize, bool)>,
    /// Set Up... of the API key or local model.
    pub set_up: bool,
    pub check_setup: bool,
    /// Manage usage, under a usage-limit line of the ChatGPT route.
    pub manage_usage: bool,
    /// Control rectangles, for the tests.
    pub actions: Vec<(String, Rect)>,
}

/// The text style of replies: the kit's body font.
fn style() -> markdown::Style {
    markdown::Style {
        regular: kit::body_font(),
        bold: chrome::bold_font(),
        ink: INK,
        code: (kit::body_font(), INK),
        code_fill: crate::theme::color(crate::theme::CELL),
        link: crate::theme::color(crate::theme::DATA_DATE),
    }
}

fn secondary() -> Color32 {
    crate::theme::color(crate::theme::SECONDARY_TEXT)
}

/// Animated thinking wheel shown at the bottom of the message list while a
/// reply is in progress.
fn thinking_ui(
    ui: &mut egui::Ui,
    conv: &Conversation,
    elapsed_secs: Option<u64>,
    width: f32,
    out: &mut ListOut,
) {
    let working = conv.items.iter().any(|item| match item {
        Item::Tools(rows) => rows.iter().any(|r| r.status == ToolStatus::Running),
        _ => false,
    });
    let base_label = if working { "Working..." } else { "Thinking..." };
    let label = match elapsed_secs.filter(|&s| s >= 2) {
        Some(s) => format!("{base_label} ({s}s)"),
        None => base_label.to_string(),
    };
    let row_h = LINE + 4.;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, row_h), Sense::hover());
    out.actions.push(("chat_thinking".into(), rect));
    let p = ui.painter();
    let center = Pos2::new(rect.left() + 8., rect.center().y);
    let inner_r = 3.0_f32;
    let outer_r = 6.0_f32;
    let t = ui.input(|i| i.time) as f32;
    let base = crate::theme::color(crate::theme::DATA_DATE);
    let spokes = 8usize;
    let head = ((t * 10.0) as usize) % spokes;
    for i in 0..spokes {
        let angle = (i as f32) * std::f32::consts::TAU / (spokes as f32);
        let dist = (spokes + head - i) % spokes;
        let alpha = (255_u16.saturating_sub((dist as u16) * 28)).max(45) as u8;
        let color = Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), alpha);
        let dir = Vec2::new(angle.cos(), angle.sin());
        p.line_segment(
            [center + dir * inner_r, center + dir * outer_r],
            Stroke::new(1.6_f32, color),
        );
    }
    p.text(
        Pos2::new(rect.left() + 20., rect.center().y),
        Align2::LEFT_CENTER,
        label,
        kit::body_font(),
        secondary(),
    );
}

/// The message list at client (x, y, w, h) of `c`: `conv`, or `empty`
/// when it has nothing yet.
pub(crate) fn show(
    c: &mut Canvas,
    (x, y, w, h): (f32, f32, f32, f32),
    conv: &Conversation,
    empty: Empty,
    view: &ListView,
) -> ListOut {
    let mut out = ListOut::default();
    c.lowered(x, y, w, h);
    c.fill(x + 1., y + 1., w - 2., h - 2., Color32::WHITE);
    let frame = c.r(x + 1., y + 1., w - 2., h - 2.);
    out.actions.push(("chat_list".into(), frame));
    let salt = c.salt;
    // The setup list reads from the top; a conversation stays on its
    // newest line.
    let setup = matches!(empty, Empty::Setup { .. });
    let sel_id = egui::Id::new((salt, "chat_text_sel"));
    let stored_sel =
        c.ui.ctx()
            .data(|d| d.get_temp::<TextSelection>(sel_id).unwrap_or_default());
    let (pointer_pos, primary_pressed, primary_down, primary_released, copy_requested) =
        c.ui.ctx().input(|i| {
            let pos = i.pointer.interact_pos().or_else(|| i.pointer.hover_pos());
            let copy = !view.composer_focused
                && (i.events.iter().any(|e| matches!(e, egui::Event::Copy))
                    || (i.modifiers.command && i.key_pressed(egui::Key::C)));
            (
                pos,
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.pointer.primary_released(),
                copy,
            )
        });
    let vbar = crate::hscroll::BAR_HEIGHT;
    let content_frame = Rect::from_min_max(
        frame.min,
        Pos2::new((frame.right() - vbar).max(frame.left()), frame.bottom()),
    );
    let vscroll_rect = Rect::from_min_max(Pos2::new(content_frame.right(), frame.top()), frame.max);
    let mut sel_ctx = SelCtx {
        frame: content_frame,
        sel: stored_sel,
        next_block: 0,
        collected: vec![],
        frame_head: None,
        pointer_pos,
        primary_pressed,
        primary_down,
        primary_released,
        pressed_on_block: false,
    };
    let scroll_salt = (
        salt,
        if setup {
            "chat_setup_scroll"
        } else {
            "chat_list_scroll"
        },
    );
    let vscroll_id = c.ui.id().with((scroll_salt, "vscroll"));
    let total_id = egui::Id::new((scroll_salt, "content_h"));
    let prev_total =
        c.ui.ctx()
            .data(|d| d.get_temp::<f32>(total_id).unwrap_or(0.));
    let mut child = c.ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content_frame)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(content_frame.intersect(c.ui.clip_rect()));
    let scroll_id = child.make_persistent_id(egui::Id::new(scroll_salt));
    let current_y =
        egui::scroll_area::State::load(child.ctx(), scroll_id).map_or(0., |s| s.offset.y);
    let prev_range = crate::hscroll::Range {
        total: f64::from(prev_total),
        view: f64::from(content_frame.height()),
        pos: f64::from(current_y),
    };
    let vmove = crate::hscroll::interact_vertical(c.ui, vscroll_id, vscroll_rect, prev_range);
    let mut area = egui::ScrollArea::vertical()
        .id_salt(scroll_salt)
        .auto_shrink([false, false])
        .stick_to_bottom(!setup)
        .max_width(content_frame.width())
        .max_height(content_frame.height())
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
    if let Some(mv) = vmove {
        let line = LINE * 2.;
        let page = (content_frame.height() - line).max(line);
        let max_y = prev_range.max() as f32;
        let next_y = match mv {
            crate::hscroll::Move::To(pos) => (pos as f32).clamp(0., max_y),
            crate::hscroll::Move::Line(dir) => (current_y + dir as f32 * line).clamp(0., max_y),
            crate::hscroll::Move::Page(dir) => (current_y + dir as f32 * page).clamp(0., max_y),
        };
        area = area.vertical_scroll_offset(next_y);
    }
    let scrolled = area.show(&mut child, |ui| {
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(7, 6))
            .show(ui, |ui| {
                let width = (content_frame.width() - 14.).max(80.);
                ui.set_width(width);
                ui.spacing_mut().item_spacing = Vec2::new(0., GAP);
                if conv.is_empty() || setup {
                    empty_state(ui, salt, width, &empty, &mut out);
                } else {
                    for (n, item) in conv.items.iter().enumerate() {
                        item_ui(ui, salt, n, item, width, view, &mut sel_ctx, &mut out);
                    }
                    if conv.running {
                        thinking_ui(ui, conv, view.elapsed_secs, width, &mut out);
                    } else if view.mode == Mode::Ask
                        && conv.suggests_edit_mode()
                        && conv.last_user_prompt().is_some()
                    {
                        let (clicked, actions) =
                            kit::flow_canvas(ui, salt, Vec2::new(width, kit::BUTTON.y), |c| {
                                kit::button(
                                    c,
                                    "chat_switch_edit_retry",
                                    0.,
                                    0.,
                                    156.,
                                    kit::BUTTON.y,
                                    "Switch to Edit && Retry",
                                    true,
                                )
                            });
                        out.actions.extend(actions);
                        out.switch_edit_retry |= clicked;
                    }
                }
            });
    });
    let total_h = scrolled.content_size.y;
    c.ui.ctx().data_mut(|d| d.insert_temp(total_id, total_h));
    let painted_range = crate::hscroll::Range {
        total: f64::from(total_h),
        view: f64::from(content_frame.height()),
        pos: f64::from(scrolled.state.offset.y),
    };
    let vgeo = crate::hscroll::paint_vertical(c.ui, vscroll_id, vscroll_rect, painted_range);
    out.actions.push(("chat_vscroll".into(), vgeo.bar));
    if sel_ctx.primary_pressed
        && !sel_ctx.pressed_on_block
        && sel_ctx
            .pointer_pos
            .is_some_and(|p| content_frame.contains(p))
    {
        sel_ctx.sel.anchor = None;
        sel_ctx.sel.head = None;
        sel_ctx.sel.dragging = false;
        sel_ctx.collected.clear();
    }
    if sel_ctx.sel.dragging && sel_ctx.primary_down {
        if let Some(fh) = sel_ctx.frame_head {
            if sel_ctx.sel.head != Some(fh) {
                sel_ctx.sel.head = Some(fh);
                c.ui.ctx().request_repaint();
            }
        }
    }
    if sel_ctx.primary_released || !sel_ctx.primary_down {
        sel_ctx.sel.dragging = false;
        if sel_ctx.sel.anchor == sel_ctx.sel.head {
            sel_ctx.sel.anchor = None;
            sel_ctx.sel.head = None;
        }
    }
    sel_ctx.sel.text = sel_ctx.collected.join("\n");
    out.text_interacted = sel_ctx.pressed_on_block;
    if copy_requested && !sel_ctx.sel.text.is_empty() && out.copy_text.is_none() {
        out.copy_text = Some((sel_ctx.sel.text.clone(), "Copied selection."));
    }
    c.ui.ctx().data_mut(|d| d.insert_temp(sel_id, sel_ctx.sel));
    out
}

/// Wrapped text of `spans` at the cursor, `width` wide; supports drag-to-select
/// when `sel` is provided, clicked links go to `out`, and hovered Activity ID
/// links show their schedule summary tooltip.
#[allow(clippy::too_many_arguments)]
fn text_ui(
    ui: &mut egui::Ui,
    spans: &[Span],
    width: f32,
    indent: f32,
    color: Option<Color32>,
    tips: &HashMap<String, String>,
    sel: Option<&mut SelCtx>,
    out: &mut ListOut,
) {
    let mut style = style();
    if let Some(color) = color {
        style.ink = color;
    }
    let markdown::Laid { job, links } = markdown::layout(spans, &style, width - indent);
    let galley = ui.painter().layout_job(job);
    let size = Vec2::new(width, galley.size().y.max(LINE));
    let sense = if links.is_empty() {
        Sense::hover()
    } else {
        Sense::click()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let pos = rect.min + Vec2::new(indent, 0.);
    let mut dragged = false;
    if let Some(sel) = sel {
        sel.interact_galley(ui.painter(), &galley, rect, pos);
        dragged = sel.drag_moved();
        if response.hovered() && links.is_empty() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }
    }
    ui.painter().galley(pos, galley.clone(), INK);
    if links.is_empty() {
        return;
    }
    let at = |p: Pos2| markdown::link_at(&galley, pos, &links, p).map(str::to_owned);
    if let Some(target) = response.hover_pos().and_then(at) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        if let Some(code) = links::code_of(&target) {
            if let Some(tip) = tips.get(code) {
                response.clone().on_hover_text(tip);
            }
        }
    } else if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }
    if response.clicked() && !dragged {
        if let Some(target) = response.interact_pointer_pos().and_then(at) {
            out.link = links::code_of(&target).map(str::to_owned);
        }
    }
    for (target, r) in markdown::link_rects(&galley, pos, &links) {
        if let Some(code) = links::code_of(&target) {
            let name = format!("chat_link_{code}");
            if !out.actions.iter().any(|(n, _)| *n == name) {
                out.actions.push((name, r));
            }
        }
    }
}

fn plain(text: &str) -> Vec<Span> {
    vec![Span::Text(text.to_owned())]
}

fn empty_state(
    ui: &mut egui::Ui,
    salt: &'static str,
    width: f32,
    empty: &Empty,
    out: &mut ListOut,
) {
    let no_tips = HashMap::new();
    let line = |ui: &mut egui::Ui, text: &str, out: &mut ListOut| {
        text_ui(ui, &plain(text), width, 0., None, &no_tips, None, out);
    };
    let button = |ui: &mut egui::Ui, key: &str, caption: &str, out: &mut ListOut| -> bool {
        let (clicked, actions) = kit::flow_canvas(ui, salt, Vec2::new(width, kit::BUTTON.y), |c| {
            kit::button(c, key, 0., 0., kit::BUTTON.x, kit::BUTTON.y, caption, true)
        });
        out.actions.extend(actions);
        clicked
    };
    match empty {
        Empty::NoProject => line(
            ui,
            "No project is open. Open a project to ask about its schedule.",
            out,
        ),
        Empty::NoAgent => line(ui, "No AI agent is set up for Chat on this PC yet.", out),
        Empty::NotInstalled { name } => line(
            ui,
            &format!("{name} is not installed on this PC. Install it, then choose it here again."),
            out,
        ),
        Empty::SignIn { name, privacy } => {
            line(
                ui,
                &format!("Sign in to {name} to chat with it. {privacy}"),
                out,
            );
            out.sign_in = button(ui, "chat_sign_in", "Sign &In...", out);
        }
        Empty::SigningIn { name } => {
            line(
                ui,
                &format!("Finish signing in to {name} in your browser. This window waits for it."),
                out,
            );
            out.cancel_sign_in = button(ui, "chat_sign_in_cancel", "Cancel", out);
        }
        Empty::SetUp { name, privacy } => {
            line(
                ui,
                &format!("Set up {name} to chat with it. {privacy}"),
                out,
            );
            out.set_up = button(ui, "chat_set_up", "Set &Up...", out);
        }
        Empty::Checking { name } => line(ui, &format!("Looking for {name} on this PC..."), out),
        Empty::Setup { rows, note } => setup_ui(ui, salt, width, rows, *note, out),
        Empty::Ready {
            line: text,
            starters,
        } => {
            line(ui, text, out);
            ui.add_space(6. - GAP + 6.);
            text_ui(
                ui,
                &plain("Try one of these: (or type / for playbooks)"),
                width,
                0.,
                Some(secondary()),
                &no_tips,
                None,
                out,
            );
            // Every row fits: the kit's rows show no scroll bar.
            let h = STARTER_PITCH * starters.len() as f32;
            let (picked, actions) = kit::flow_canvas(ui, salt, Vec2::new(width, h), |c| {
                let body = c.r(0., 0., width, h);
                let hover = c.ui.ctx().pointer_hover_pos();
                kit::rows_in(
                    c,
                    "chat_starter",
                    body,
                    STARTER_PITCH,
                    starters.len(),
                    None,
                    &mut |c, r, row, _| {
                        let cell = Rect::from_min_size(row.min, Vec2::new(row.width(), STARTER_H));
                        let hot = hover.is_some_and(|p| cell.contains(p));
                        let p = c.ui.painter();
                        p.rect_filled(cell, 0., if hot { pal::HOT_FILL } else { pal::READONLY });
                        p.rect_stroke(
                            cell.shrink(0.5),
                            0.,
                            Stroke::new(
                                1_f32,
                                if hot {
                                    pal::HOT_BORDER
                                } else {
                                    pal::CAPTION_LINE
                                },
                            ),
                            egui::StrokeKind::Middle,
                        );
                        kit::cell_text(c, cell, &starters[r], false, 6.);
                    },
                )
                .clicked
            });
            out.actions.extend(actions);
            out.starter = picked;
        }
    }
}

/// Choose how to connect: a heading, the routes on the shared app rows of
/// the AI Agents dialogs, and Check Setup.
fn setup_ui(
    ui: &mut egui::Ui,
    salt: &'static str,
    width: f32,
    rows: &[SetupRow],
    note: Option<&str>,
    out: &mut ListOut,
) {
    use crate::ai_agents::row;
    let no_tips = HashMap::new();
    if let Some(note) = note {
        text_ui(ui, &plain(note), width, 0., None, &no_tips, None, out);
    }
    // The heading, with Check Setup at the right where it stays in view.
    let (check, actions) = kit::flow_canvas(ui, salt, Vec2::new(width, kit::BUTTON.y), |c| {
        c.bold(0., 5., 16., "Choose how to connect");
        kit::button(
            c,
            "chat_check_setup",
            width - kit::BUTTON.x,
            0.,
            kit::BUTTON.x,
            kit::BUTTON.y,
            "Check Set&up...",
            true,
        )
    });
    out.actions.extend(actions);
    out.check_setup = check;
    // Each row's detail wraps to the room beside its buttons.
    let (wrapped, _) = kit::flow_canvas(ui, salt, Vec2::new(width, 0.), |c| {
        rows.iter()
            .map(|r| crate::ai_agents::wrap(c, &r.detail, row::text_width(width)))
            .collect::<Vec<_>>()
    });
    let heights: Vec<f32> = wrapped.iter().map(|l| row::height(l.len())).collect();
    let total: f32 = heights.iter().sum::<f32>() + 2.;
    let (picked, actions) = kit::flow_canvas(ui, salt, Vec2::new(width, total), |c| {
        c.fill(0., 0., width, total, Color32::WHITE);
        c.lowered(0., 0., width, total);
        let mut y = 1.;
        let mut picked = None;
        for (n, r) in rows.iter().enumerate() {
            if n > 0 {
                c.hline(1., y, width - 2., kit::SHADOW);
            }
            let (k1, k2) = (setup_key(n, false), setup_key(n, true));
            let clicked = row::paint(
                c,
                0.,
                y,
                width,
                &row::Row {
                    name: &r.name,
                    mark: r.mark,
                    state: &r.state,
                    state_dim: r.dim,
                    detail: &wrapped[n],
                    primary: r.primary.as_ref().map(|(label, enabled)| row::Button {
                        key: &k1,
                        label,
                        enabled: *enabled,
                    }),
                    secondary: r.secondary.as_ref().map(|(label, enabled)| row::Button {
                        key: &k2,
                        label,
                        enabled: *enabled,
                    }),
                },
            );
            if let Some(k) = clicked {
                picked = Some((n, k == k2));
            }
            y += heights[n];
        }
        picked
    });
    out.actions.extend(actions);
    out.setup = picked;
}

/// Unique Activity IDs mentioned in `text`, in order of first appearance.
pub(crate) fn mentioned_ids(text: &str, codes: &HashSet<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = vec![];
    for (a, b) in links::find(text, codes) {
        let code = &text[a..b];
        if seen.insert(code.to_string()) {
            out.push(code.to_string());
        }
    }
    out
}

/// Flatten a Markdown table into tab-separated text ready to paste into Excel.
pub(crate) fn table_to_tsv(rows: &[Vec<Vec<Span>>]) -> String {
    rows.iter()
        .map(|row| {
            row.iter()
                .map(|cell| {
                    cell.iter()
                        .map(|s| match s {
                            Span::Text(t) | Span::Bold(t) | Span::Italic(t) | Span::Code(t) => {
                                t.as_str()
                            }
                            Span::Link { text, .. } => text.as_str(),
                        })
                        .collect::<String>()
                        .replace(['\t', '\r', '\n'], " ")
                        .trim()
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Strip light Markdown markers into clean plain text for pasting into emails.
pub(crate) fn reply_plain_text(md: &str) -> String {
    let blocks = markdown::blocks(
        md,
        markdown::Dialect {
            links: false,
            streaming: false,
        },
    );
    let flat = |spans: &[Span]| -> String {
        spans
            .iter()
            .map(|s| match s {
                Span::Text(t) | Span::Bold(t) | Span::Italic(t) | Span::Code(t) => t.as_str(),
                Span::Link { text, .. } => text.as_str(),
            })
            .collect()
    };
    let mut parts = vec![];
    for block in blocks {
        match block {
            Block::Heading(h) => parts.push(h),
            Block::Para(s) | Block::Note(s) => parts.push(flat(&s)),
            Block::Bullet(s) => parts.push(format!("- {}", flat(&s))),
            Block::Step(n, s) => parts.push(format!("{n}. {}", flat(&s))),
            Block::Table(rows) => parts.push(table_to_tsv(&rows)),
        }
    }
    if parts.is_empty() {
        md.to_string()
    } else {
        parts.join("\n\n")
    }
}

#[allow(clippy::too_many_arguments)]
fn item_ui(
    ui: &mut egui::Ui,
    salt: &'static str,
    n: usize,
    item: &Item,
    width: f32,
    view: &ListView,
    sel: &mut SelCtx,
    out: &mut ListOut,
) {
    match item {
        Item::User(text) => user_ui(ui, n, text, width, sel, out),
        Item::Reply { text, streaming } => {
            let read = markdown::blocks(
                text,
                markdown::Dialect {
                    links: false,
                    streaming: *streaming,
                },
            );
            if read.is_empty() {
                return;
            }
            let start = ui.cursor().min;
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.y = 4.;
                for (k, block) in read.into_iter().enumerate() {
                    block_ui(ui, salt, (n, k), block, width, view, sel, out);
                }
            });
            if !streaming {
                let ids = mentioned_ids(text, view.codes);
                let reply_rect =
                    Rect::from_min_max(start, Pos2::new(start.x + width, ui.min_rect().bottom()));
                out.actions.push((format!("chat_reply_{n}"), reply_rect));
                let resp = ui.interact(
                    reply_rect,
                    ui.id().with(("chat_reply_ctx", n)),
                    Sense::hover(),
                );
                let open_at = ui.ctx().input(|i| {
                    i.pointer
                        .secondary_clicked()
                        .then(|| i.pointer.interact_pos())
                        .flatten()
                        .filter(|pos| reply_rect.contains(*pos))
                });
                let selected_now = if !sel.collected.is_empty() {
                    sel.collected.join("\n")
                } else {
                    sel.sel.text.clone()
                };
                let mut ctx_act: Option<u8> = None;
                crate::context_menu::ContextMenu::new("chat_msg")
                    .open_at(open_at)
                    .show(&resp, &mut ctx_act, |act, menu| {
                        if !selected_now.is_empty() {
                            if menu.item(
                                "copy_sel",
                                crate::chrome::Item::new("Copy &Selection").key("Ctrl+C"),
                            ) {
                                *act = Some(4);
                            }
                            menu.separator();
                        }
                        if menu.item("copy", crate::chrome::Item::new("&Copy Reply")) {
                            *act = Some(1);
                        }
                        if menu.item(
                            "copy_md",
                            crate::chrome::Item::new("Copy Reply as &Markdown"),
                        ) {
                            *act = Some(2);
                        }
                        let sel_label = if ids.is_empty() {
                            "Select Mentioned Activities".to_string()
                        } else {
                            format!("Select Mentioned Activities ({})", ids.len())
                        };
                        if menu.item(
                            "select_ids",
                            crate::chrome::Item::new(&sel_label).enabled(!ids.is_empty()),
                        ) {
                            *act = Some(3);
                        }
                    });
                match ctx_act {
                    Some(4) if !selected_now.is_empty() => {
                        out.copy_text = Some((selected_now, "Copied selection."));
                    }
                    Some(1) => out.copy_text = Some((reply_plain_text(text), "Reply copied.")),
                    Some(2) => out.copy_text = Some((text.clone(), "Reply Markdown copied.")),
                    Some(3) if !ids.is_empty() => out.select_all = Some(ids.clone()),
                    _ => {}
                }

                // Compact action links beneath finished replies: Copy, and Select all (N) when 2+ IDs match.
                let ((copied, selected), actions) =
                    kit::flow_canvas(ui, salt, Vec2::new(width, LINE), |c| {
                        let copy_key = format!("chat_copy_{n}");
                        let copy_clicked = kit::link(c, &copy_key, 0., 0., "Copy");
                        let mut sel_clicked = false;
                        if ids.len() >= 2 {
                            let label = format!("Select all ({})", ids.len());
                            let x =
                                c.ui.painter()
                                    .layout_no_wrap("Copy".into(), kit::body_font(), INK)
                                    .size()
                                    .x
                                    .ceil()
                                    + 10.;
                            sel_clicked =
                                kit::link(c, &format!("chat_select_all_{n}"), x, 0., &label);
                        }
                        (copy_clicked, sel_clicked)
                    });
                out.actions.extend(actions);
                if copied {
                    out.copy_text = Some((reply_plain_text(text), "Reply copied."));
                }
                if selected {
                    out.select_all = Some(ids);
                }
            }
        }
        Item::Tools(rows) => tools_ui(ui, salt, n, rows, width, view, out),
        Item::Proposal {
            id,
            rows,
            impact,
            state,
        } => proposal_ui(
            ui,
            salt,
            n,
            *id,
            rows,
            impact.as_deref(),
            state,
            width,
            view,
            out,
        ),
        Item::Question {
            id,
            title,
            prompts,
            state,
        } => question_ui(ui, salt, *id, title, prompts, state, width, out),
        Item::Status { kind, text } => {
            status_ui(ui, *kind, text, width, sel, out);
            if *kind == StatusKind::Limit && view.plan {
                let (clicked, actions) =
                    kit::flow_canvas(ui, salt, Vec2::new(width, kit::BUTTON.y), |c| {
                        kit::default_button(
                            c,
                            "chat_manage_usage",
                            0.,
                            0.,
                            kit::BUTTON.x,
                            kit::BUTTON.y,
                            openxer_chat::ui::MANAGE_USAGE,
                            true,
                        )
                    });
                out.actions.extend(actions);
                out.manage_usage |= clicked;
            }
        }
    }
}

fn tools_summary_line(rows: &[ToolRow], expanded: bool) -> String {
    let marker = if expanded { "\u{25be}" } else { "\u{25b8}" };
    let total_ms: u64 = rows.iter().filter_map(|r| r.millis).sum();
    let mut names: Vec<&str> = vec![];
    for r in rows {
        if !names.contains(&r.tool.as_str()) {
            names.push(&r.tool);
        }
    }
    let list = if names.len() <= 3 {
        names.join(", ")
    } else {
        format!("{}, +{} more", names[..2].join(", "), names.len() - 2)
    };
    let time = if total_ms > 0 {
        format!(" \u{b7} {}", super::model::duration(total_ms))
    } else {
        String::new()
    };
    format!("{marker} {} tools ({list}){time}", rows.len())
}

fn tools_ui(
    ui: &mut egui::Ui,
    salt: &'static str,
    n: usize,
    rows: &[ToolRow],
    width: f32,
    view: &ListView,
    out: &mut ListOut,
) {
    let collapsible = rows.len() >= 2 && rows.iter().all(|r| r.status == ToolStatus::Done);
    let expanded = !collapsible || view.expanded_tools.contains(&n);
    if collapsible {
        let summary = tools_summary_line(rows, expanded);
        let key = format!("chat_tools_toggle_{n}");
        let (clicked, actions) = kit::flow_canvas(ui, salt, Vec2::new(width, kit::ROW), |c| {
            let rect = c.r(0., 0., width, kit::ROW);
            let resp = c.ui.interact(rect, c.id(&key), Sense::click());
            let fill = if resp.hovered() {
                pal::HOT_FILL
            } else {
                pal::READONLY
            };
            let border = if resp.hovered() {
                pal::HOT_BORDER
            } else {
                pal::CAPTION_LINE
            };
            let p = c.ui.painter();
            p.rect_filled(rect, 0., fill);
            p.rect_stroke(
                rect.shrink(0.5),
                0.,
                Stroke::new(1_f32, border),
                egui::StrokeKind::Middle,
            );
            if resp.hovered() {
                c.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            p.with_clip_rect(rect.shrink(2.)).text(
                Pos2::new(rect.left() + 5., rect.center().y),
                Align2::LEFT_CENTER,
                summary,
                kit::body_font(),
                secondary(),
            );
            c.act(&key, rect);
            resp.clicked()
        });
        out.actions.extend(actions);
        if clicked {
            out.toggle_tools = Some(n);
        }
        if !expanded {
            return;
        }
    }
    let cells: Vec<Vec<Cell>> = rows
        .iter()
        .map(|r| {
            let target_color = if view.codes.contains(&r.target) {
                crate::theme::color(crate::theme::DATA_DATE)
            } else {
                secondary()
            };
            vec![
                Cell::fixed(&r.tool, FIRST_COL, INK),
                Cell::flex(&r.target, target_color),
                Cell::fit(r.status.label(), status_color(r.status)),
                Cell::fit_owned(
                    r.millis.map(super::model::duration).unwrap_or_default(),
                    secondary(),
                ),
            ]
        })
        .collect();
    let (clicked_row, actions) = rows_block(
        ui,
        salt,
        &format!("chat_tools_{n}"),
        width,
        pal::READONLY,
        &cells,
    );
    out.actions.extend(actions);
    if let Some(r) = clicked_row {
        if let Some(row) = rows.get(r) {
            if view.codes.contains(&row.target) {
                out.link = Some(row.target.clone());
            }
        }
    }
}

fn status_color(s: ToolStatus) -> Color32 {
    use crate::theme::{color, AMBER_DARK, ERROR_TEXT, SECONDARY_TEXT, TEAL_DARK};
    match s {
        ToolStatus::Done => color(TEAL_DARK),
        ToolStatus::Waiting => color(AMBER_DARK),
        ToolStatus::Failed => color(ERROR_TEXT),
        ToolStatus::Running | ToolStatus::Stopped => color(SECONDARY_TEXT),
    }
}

/// A user message: right-aligned in the light blue fill with its border.
fn user_ui(
    ui: &mut egui::Ui,
    n: usize,
    text: &str,
    width: f32,
    sel: &mut SelCtx,
    out: &mut ListOut,
) {
    let max = USER_W.min(width) - 12.;
    let galley = ui
        .painter()
        .layout(text.to_owned(), kit::body_font(), INK, max);
    let size = galley.size() + Vec2::new(12., 6.);
    let (row, _) = ui.allocate_exact_size(Vec2::new(width, size.y.max(LINE + 6.)), Sense::hover());
    let rect = Rect::from_min_size(
        Pos2::new(row.right() - size.x, row.top()),
        Vec2::new(size.x, row.height()),
    );
    out.actions.push((format!("chat_user_{n}"), rect));
    let text_pos = rect.min + Vec2::new(6., 3.);
    let p = ui.painter();
    p.rect_filled(rect, 0., pal::FOCUS_INNER);
    p.rect_stroke(
        rect.shrink(0.5),
        0.,
        Stroke::new(1_f32, pal::HOT_BORDER),
        egui::StrokeKind::Middle,
    );
    sel.interact_galley(p, &galley, rect, text_pos);
    p.galley(text_pos, galley, INK);
    let resp = ui.interact(rect, ui.id().with(("chat_user_ctx", n)), Sense::hover());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }
    let open_at = ui.ctx().input(|i| {
        i.pointer
            .secondary_clicked()
            .then(|| i.pointer.interact_pos())
            .flatten()
            .filter(|pos| rect.contains(*pos))
    });
    let selected_now = if !sel.collected.is_empty() {
        sel.collected.join("\n")
    } else {
        sel.sel.text.clone()
    };
    let mut ctx_act: Option<u8> = None;
    crate::context_menu::ContextMenu::new("chat_user_msg")
        .open_at(open_at)
        .show(&resp, &mut ctx_act, |act, menu| {
            if !selected_now.is_empty() {
                if menu.item(
                    "copy_sel",
                    crate::chrome::Item::new("Copy &Selection").key("Ctrl+C"),
                ) {
                    *act = Some(1);
                }
                menu.separator();
            }
            if menu.item("copy_msg", crate::chrome::Item::new("&Copy Message")) {
                *act = Some(2);
            }
        });
    match ctx_act {
        Some(1) if !selected_now.is_empty() => {
            out.copy_text = Some((selected_now, "Copied selection."));
        }
        Some(2) => out.copy_text = Some((text.to_owned(), "Message copied.")),
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn block_ui(
    ui: &mut egui::Ui,
    salt: &'static str,
    (n, k): (usize, usize),
    block: Block,
    width: f32,
    view: &ListView,
    sel: &mut SelCtx,
    out: &mut ListOut,
) {
    let linked = |s: Vec<Span>| links::link_ids(s, view.codes);
    match block {
        Block::Para(s) | Block::Note(s) => {
            text_ui(ui, &linked(s), width, 0., None, view.tips, Some(sel), out)
        }
        Block::Heading(h) => text_ui(
            ui,
            &[Span::Bold(h)],
            width,
            0.,
            None,
            view.tips,
            Some(sel),
            out,
        ),
        Block::Bullet(s) => marked(ui, "\u{2022}", &linked(s), width, view.tips, sel, out),
        Block::Step(number, s) => marked(
            ui,
            &format!("{number}."),
            &linked(s),
            width,
            view.tips,
            sel,
            out,
        ),
        Block::Table(rows) => {
            let key = format!("chat_table_{n}_{k}");
            table_ui(ui, salt, &key, &rows, width, view, out)
        }
    }
}

/// A bullet or numbered line: the mark, then the text wrapped beside it.
fn marked(
    ui: &mut egui::Ui,
    mark: &str,
    spans: &[Span],
    width: f32,
    tips: &HashMap<String, String>,
    sel: &mut SelCtx,
    out: &mut ListOut,
) {
    let top = ui.cursor().min;
    text_ui(ui, spans, width, 16., None, tips, Some(sel), out);
    ui.painter().text(
        top + Vec2::new(2., 0.),
        Align2::LEFT_TOP,
        mark,
        kit::body_font(),
        INK,
    );
}

/// A table of a reply on the shared Markdown table (`markdown::table_ui`):
/// columns fitted to the message width, text wrapped inside its cell,
/// Activity IDs as links with hover tooltips, and a Copy table link for Excel.
fn table_ui(
    ui: &mut egui::Ui,
    salt: &'static str,
    key: &str,
    rows: &[Vec<Vec<Span>>],
    width: f32,
    view: &ListView,
    out: &mut ListOut,
) {
    let linked_rows: Vec<Vec<Vec<Span>>> = rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|c| links::link_ids(c.clone(), view.codes))
                .collect()
        })
        .collect();
    let id = egui::Id::new((salt, key));
    let table = markdown::table_ui(ui, id, &linked_rows, &style(), width);
    if let Some(target) = table.clicked {
        out.link = links::code_of(&target).map(str::to_owned);
    }
    let hover = ui.ctx().pointer_hover_pos();
    for (idx, (target, r)) in table.links.into_iter().enumerate() {
        if let Some(code) = links::code_of(&target) {
            if hover.is_some_and(|p| r.contains(p)) {
                if let Some(tip) = view.tips.get(code) {
                    let _ = ui
                        .interact(r, id.with(("tip", idx, code)), Sense::hover())
                        .on_hover_text(tip);
                }
            }
            let name = format!("chat_link_{code}");
            if !out.actions.iter().any(|(n, _)| *n == name) {
                out.actions.push((name, r));
            }
        }
    }
    let copy_key = format!("{key}_copy");
    let (copied, actions) = kit::flow_canvas(ui, salt, Vec2::new(width, LINE), |c| {
        kit::link(c, &copy_key, 0., 0., "Copy table for Excel")
    });
    out.actions.extend(actions);
    if copied {
        out.copy_text = Some((table_to_tsv(rows), "Table copied for Excel."));
    }
}

/// One cell of a [`rows_block`] row.
struct Cell<'a> {
    text: std::borrow::Cow<'a, str>,
    width: Width,
    color: Color32,
}

enum Width {
    Fixed(f32),
    /// As wide as the widest text of the column.
    Fit,
    /// What is left.
    Flex,
}

impl<'a> Cell<'a> {
    fn fixed(text: &'a str, width: f32, color: Color32) -> Self {
        Self {
            text: text.into(),
            width: Width::Fixed(width),
            color,
        }
    }
    fn flex(text: &'a str, color: Color32) -> Self {
        Self {
            text: text.into(),
            width: Width::Flex,
            color,
        }
    }
    fn fit(text: &'a str, color: Color32) -> Self {
        Self {
            text: text.into(),
            width: Width::Fit,
            color,
        }
    }
    fn fit_owned(text: String, color: Color32) -> Self {
        Self {
            text: text.into(),
            width: Width::Fit,
            color,
        }
    }
}

/// A bordered block of 18 px rows (tool activity, proposed changes) on the
/// kit's list rows. Returns the clicked row and the control rectangles.
fn rows_block(
    ui: &mut egui::Ui,
    salt: &'static str,
    key: &str,
    width: f32,
    fill: Color32,
    rows: &[Vec<Cell>],
) -> (Option<usize>, Vec<(String, Rect)>) {
    const PAD: f32 = 5.;
    const SPACE: f32 = 6.;
    let count = rows.len();
    let columns = rows.first().map_or(0, Vec::len);
    let measure = |s: &str| {
        ui.painter()
            .layout_no_wrap(s.to_owned(), kit::body_font(), INK)
            .size()
            .x
    };
    let mut widths: Vec<f32> = (0..columns)
        .map(|k| match rows[0][k].width {
            Width::Fixed(w) => w,
            Width::Fit => rows
                .iter()
                .map(|r| measure(&r[k].text).ceil())
                .fold(0_f32, f32::max),
            Width::Flex => 0.,
        })
        .collect();
    let fixed: f32 = widths.iter().sum::<f32>() + SPACE * columns.saturating_sub(1) as f32;
    let flex = (width - 2. - 2. * PAD - fixed).max(20.);
    for (k, w) in widths.iter_mut().enumerate() {
        if matches!(rows[0][k].width, Width::Flex) {
            *w = flex;
        }
    }
    let h = 2. + kit::ROW * count as f32;
    kit::flow_canvas(ui, salt, Vec2::new(width, h), |c| {
        c.fill(0., 0., width, h, fill);
        let frame = c.r(0., 0., width, h);
        c.ui.painter().rect_stroke(
            frame.shrink(0.5),
            0.,
            Stroke::new(1_f32, pal::CAPTION_LINE),
            egui::StrokeKind::Middle,
        );
        let body = frame.shrink(1.);
        kit::rows_in(c, key, body, kit::ROW, count, None, &mut |c, r, row, _| {
            if r + 1 < count {
                c.ui.painter().rect_filled(
                    Rect::from_min_size(
                        Pos2::new(row.left(), row.bottom() - 1.),
                        Vec2::new(row.width(), 1.),
                    ),
                    0.,
                    pal::LIGHT,
                );
            }
            let mut left = row.left() + PAD;
            for (k, cell) in rows[r].iter().enumerate() {
                let rect = Rect::from_min_size(
                    Pos2::new(left, row.top()),
                    Vec2::new(widths[k], kit::ROW - 1.),
                );
                c.ui.painter().with_clip_rect(rect.intersect(body)).text(
                    rect.left_center(),
                    Align2::LEFT_CENTER,
                    cell.text.as_ref(),
                    kit::body_font(),
                    cell.color,
                );
                left += widths[k] + SPACE;
            }
        })
        .clicked
    })
}

#[allow(clippy::too_many_arguments)]
fn proposal_ui(
    ui: &mut egui::Ui,
    salt: &'static str,
    n: usize,
    id: u64,
    rows: &[super::backend::ChangeRow],
    impact: Option<&str>,
    state: &ProposalState,
    width: f32,
    view: &ListView,
    out: &mut ListOut,
) {
    let pending = *state == ProposalState::Pending;
    let head = match state {
        ProposalState::Pending => super::model::pending_line(rows.len()),
        ProposalState::Applied => "Applied.".to_string(),
        ProposalState::Discarded => "Discarded. Nothing was written.".to_string(),
        ProposalState::Replaced => "Replaced by the agent's next proposal.".to_string(),
        ProposalState::Failed(e) => format!("Not applied: {e}"),
    };
    let inner = width - 14.;
    let p = ui.painter().clone();
    let start = ui.cursor().min;
    // The box is painted under its content once its height is known.
    let slot = p.add(egui::Shape::Noop);
    let no_tips = HashMap::new();
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(7, 6))
        .show(ui, |ui| {
            ui.set_width(inner);
            ui.spacing_mut().item_spacing.y = 5.;
            let color = matches!(state, ProposalState::Failed(_))
                .then(|| crate::theme::color(crate::theme::ERROR_TEXT));
            if *state == ProposalState::Applied && view.undoable_proposal == Some(id) {
                let (undone, actions) = kit::flow_canvas(ui, salt, Vec2::new(inner, LINE), |c| {
                    kit::text(c, 0., 0., &head, true);
                    let x =
                        c.ui.painter()
                            .layout_no_wrap(head.clone(), kit::body_font(), INK)
                            .size()
                            .x
                            .ceil()
                            + 8.;
                    kit::link(c, &format!("chat_proposal_undo_{id}"), x, 0., "Undo")
                });
                out.actions.extend(actions);
                if undone {
                    out.undo_proposal = Some(id);
                }
            } else {
                text_ui(ui, &plain(&head), inner, 0., color, &no_tips, None, out);
            }
            if let Some(summary) = impact {
                let accent = crate::theme::color(crate::theme::DATA_DATE);
                text_ui(
                    ui,
                    &[Span::Bold(summary.to_string())],
                    inner,
                    0.,
                    Some(accent),
                    &no_tips,
                    None,
                    out,
                );
            }
            let folded = rows.len() > PROPOSAL_FOLD_ROWS && !view.expanded_proposals.contains(&n);
            let shown_rows = if folded {
                &rows[..PROPOSAL_FOLD_ROWS]
            } else {
                rows
            };
            let arrow: Vec<String> = shown_rows
                .iter()
                .map(|r| format!("{} \u{2192} {}", r.old, r.new))
                .collect();
            let link_color = crate::theme::color(crate::theme::DATA_DATE);
            let cells: Vec<Vec<Cell>> = shown_rows
                .iter()
                .zip(&arrow)
                .map(|(r, a)| {
                    let act_color = if view.codes.contains(&r.activity) {
                        link_color
                    } else {
                        INK
                    };
                    vec![
                        Cell::fixed(&r.activity, FIRST_COL, act_color),
                        Cell::flex(&r.field, secondary()),
                        Cell::fit(a, INK),
                    ]
                })
                .collect();
            if !cells.is_empty() {
                let (clicked_row, actions) = rows_block(
                    ui,
                    salt,
                    &format!("chat_changes_{n}"),
                    inner,
                    Color32::WHITE,
                    &cells,
                );
                out.actions.extend(actions);
                if let Some(r) = clicked_row {
                    if let Some(row) = shown_rows.get(r) {
                        if view.codes.contains(&row.activity) {
                            out.link = Some(row.activity.clone());
                        }
                    }
                }
            }
            if rows.len() > PROPOSAL_FOLD_ROWS {
                let toggle_label = if folded {
                    format!("Show all {} changes", rows.len())
                } else {
                    "Show fewer changes".to_string()
                };
                let (toggled, actions) = kit::flow_canvas(ui, salt, Vec2::new(inner, LINE), |c| {
                    kit::link(c, &format!("chat_proposal_more_{n}"), 0., 0., &toggle_label)
                });
                out.actions.extend(actions);
                if toggled {
                    out.toggle_proposal = Some(n);
                }
            }
            if pending {
                ui.add_space(2.);
                let ((apply, discard), actions) =
                    kit::flow_canvas(ui, salt, Vec2::new(inner, kit::BUTTON.y), |c| {
                        let x = inner - 2. * kit::BUTTON.x - 6.;
                        let apply = kit::default_button(
                            c,
                            "chat_apply",
                            x,
                            0.,
                            kit::BUTTON.x,
                            kit::BUTTON.y,
                            "&Apply",
                            true,
                        );
                        let discard = kit::button(
                            c,
                            "chat_discard",
                            x + kit::BUTTON.x + 6.,
                            0.,
                            kit::BUTTON.x,
                            kit::BUTTON.y,
                            "&Discard",
                            true,
                        );
                        (apply, discard)
                    });
                out.actions.extend(actions);
                if apply {
                    out.apply = Some(id);
                }
                if discard {
                    out.discard = Some(id);
                }
            }
        });
    let rect = Rect::from_min_max(start, Pos2::new(start.x + width, ui.min_rect().bottom()));
    p.set(
        slot,
        egui::Shape::Vec(vec![
            egui::Shape::rect_filled(rect, 0., pal::FACE),
            egui::Shape::rect_stroke(
                rect.shrink(0.5),
                0.,
                Stroke::new(1_f32, pal::SHADOW),
                egui::StrokeKind::Middle,
            ),
        ]),
    );
}

#[derive(Clone, Default)]
struct QuestionDraft {
    selected: Vec<Vec<usize>>,
    notes: Vec<String>,
}

fn format_question_answer(prompts: &[AskPrompt], draft: &QuestionDraft) -> Option<String> {
    let mut parts = vec![];
    for (qi, p) in prompts.iter().enumerate() {
        let picked: Vec<&str> = draft
            .selected
            .get(qi)
            .into_iter()
            .flatten()
            .filter_map(|&oi| p.options.get(oi).map(|o| o.label.as_str()))
            .collect();
        let note = draft.notes.get(qi).map(|s| s.trim()).unwrap_or("");
        let answer = match (picked.is_empty(), note.is_empty()) {
            (true, true) => continue,
            (false, true) => picked.join(", "),
            (true, false) => note.to_string(),
            (false, false) => format!("{} ({note})", picked.join(", ")),
        };
        if prompts.len() == 1 {
            return Some(answer);
        }
        parts.push(format!("- {}: {answer}", p.question));
    }
    (!parts.is_empty()).then(|| parts.join("\n"))
}

#[allow(clippy::too_many_arguments)]
fn question_ui(
    ui: &mut egui::Ui,
    salt: &'static str,
    id: u64,
    title: &str,
    prompts: &[AskPrompt],
    state: &QuestionState,
    width: f32,
    out: &mut ListOut,
) {
    let inner = width - 14.;
    let p = ui.painter().clone();
    let start = ui.cursor().min;
    let slot = p.add(egui::Shape::Noop);
    let no_tips = HashMap::new();
    let draft_id = egui::Id::new((salt, "chat_q_draft", id));
    let mut draft = ui
        .ctx()
        .data(|d| d.get_temp::<QuestionDraft>(draft_id).unwrap_or_default());
    draft.selected.resize_with(prompts.len(), Vec::new);
    draft.notes.resize_with(prompts.len(), String::new);
    let mut submit_now = false;
    let mut skip_now = false;
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(7, 6))
        .show(ui, |ui| {
            ui.set_width(inner);
            ui.spacing_mut().item_spacing.y = 5.;
            if !title.is_empty() {
                text_ui(
                    ui,
                    &[Span::Bold(title.to_string())],
                    inner,
                    0.,
                    Some(crate::theme::color(crate::theme::DATA_DATE)),
                    &no_tips,
                    None,
                    out,
                );
            }
            match state {
                QuestionState::Answered(ans) => {
                    if let Some(first) = prompts.first() {
                        text_ui(
                            ui,
                            &[Span::Bold(first.question.clone())],
                            inner,
                            0.,
                            None,
                            &no_tips,
                            None,
                            out,
                        );
                    }
                    text_ui(
                        ui,
                        &plain(&format!("Answered: {ans}")),
                        inner,
                        0.,
                        Some(secondary()),
                        &no_tips,
                        None,
                        out,
                    );
                }
                QuestionState::Dismissed => {
                    text_ui(
                        ui,
                        &plain("Question skipped."),
                        inner,
                        0.,
                        Some(secondary()),
                        &no_tips,
                        None,
                        out,
                    );
                }
                QuestionState::Pending => {
                    for (qi, prompt) in prompts.iter().enumerate() {
                        let q_text = if prompt.multi_select {
                            format!("{} (choose one or more)", prompt.question)
                        } else {
                            prompt.question.clone()
                        };
                        text_ui(
                            ui,
                            &[Span::Bold(q_text)],
                            inner,
                            0.,
                            None,
                            &no_tips,
                            None,
                            out,
                        );
                        for (oi, opt) in prompt.options.iter().enumerate() {
                            let chosen = draft.selected[qi].contains(&oi);
                            let mark = match (prompt.multi_select, chosen) {
                                (false, true) => "\u{25cf}",
                                (false, false) => "\u{25cb}",
                                (true, true) => "\u{2611}",
                                (true, false) => "\u{2610}",
                            };
                            let label_galley = ui.painter().layout(
                                format!("{mark}  {}", opt.label),
                                kit::body_font(),
                                INK,
                                (inner - 14.).max(40.),
                            );
                            let desc_galley = (!opt.description.is_empty()).then(|| {
                                ui.painter().layout(
                                    opt.description.clone(),
                                    kit::body_font(),
                                    secondary(),
                                    (inner - 26.).max(40.),
                                )
                            });
                            let row_h = label_galley.size().y
                                + desc_galley.as_ref().map_or(0., |g| g.size().y + 2.)
                                + 8.;
                            let (rect, resp) =
                                ui.allocate_exact_size(Vec2::new(inner, row_h), Sense::click());
                            let key = format!("chat_q_{id}_{qi}_opt_{oi}");
                            out.actions.push((key, rect));
                            let fill = if chosen {
                                pal::FOCUS_INNER
                            } else if resp.hovered() {
                                pal::HOT_FILL
                            } else {
                                Color32::WHITE
                            };
                            let border = if chosen || resp.hovered() {
                                pal::HOT_BORDER
                            } else {
                                pal::CAPTION_LINE
                            };
                            let rp = ui.painter();
                            rp.rect_filled(rect, 0., fill);
                            rp.rect_stroke(
                                rect.shrink(0.5),
                                0.,
                                Stroke::new(1_f32, border),
                                egui::StrokeKind::Middle,
                            );
                            if resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            let top = rect.min + Vec2::new(6., 4.);
                            let lh = label_galley.size().y;
                            rp.galley(top, label_galley, INK);
                            if let Some(dg) = desc_galley {
                                rp.galley(top + Vec2::new(14., lh + 2.), dg, secondary());
                            }
                            if resp.clicked() {
                                let picks = &mut draft.selected[qi];
                                if prompt.multi_select {
                                    if let Some(pos) = picks.iter().position(|&x| x == oi) {
                                        picks.remove(pos);
                                    } else {
                                        picks.push(oi);
                                        picks.sort_unstable();
                                    }
                                } else if picks.as_slice() == [oi] {
                                    picks.clear();
                                } else {
                                    *picks = vec![oi];
                                }
                            }
                        }
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut draft.notes[qi])
                                .id_salt((salt, "chat_q_other", id, qi))
                                .desired_width(inner)
                                .font(kit::body_font())
                                .hint_text("Add your own context or custom answer..."),
                        );
                        out.actions
                            .push((format!("chat_q_{id}_{qi}_other"), resp.rect));
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            submit_now = true;
                        }
                    }
                    let can_send = format_question_answer(prompts, &draft).is_some();
                    ui.add_space(2.);
                    let ((send, skip), actions) =
                        kit::flow_canvas(ui, salt, Vec2::new(inner, kit::BUTTON.y), |c| {
                            let send_w = 96.;
                            let skip_w = 56.;
                            let x = (inner - send_w - skip_w - 6.).max(0.);
                            let send = kit::default_button(
                                c,
                                &format!("chat_q_send_{id}"),
                                x,
                                0.,
                                send_w,
                                kit::BUTTON.y,
                                "&Send Answer",
                                can_send,
                            );
                            let skip = kit::button(
                                c,
                                &format!("chat_q_skip_{id}"),
                                x + send_w + 6.,
                                0.,
                                skip_w,
                                kit::BUTTON.y,
                                "Skip",
                                true,
                            );
                            (send, skip)
                        });
                    out.actions.extend(actions);
                    submit_now |= send;
                    skip_now |= skip;
                }
            }
        });
    if submit_now {
        if let Some(answer) = format_question_answer(prompts, &draft) {
            out.answer_question = Some((id, answer));
            ui.ctx().data_mut(|d| d.remove::<QuestionDraft>(draft_id));
        } else {
            ui.ctx().data_mut(|d| d.insert_temp(draft_id, draft));
        }
    } else if skip_now {
        out.dismiss_question = Some(id);
        ui.ctx().data_mut(|d| d.remove::<QuestionDraft>(draft_id));
    } else {
        ui.ctx().data_mut(|d| d.insert_temp(draft_id, draft));
    }
    let rect = Rect::from_min_max(start, Pos2::new(start.x + width, ui.min_rect().bottom()));
    p.set(
        slot,
        egui::Shape::Vec(vec![
            egui::Shape::rect_filled(rect, 0., pal::FACE),
            egui::Shape::rect_stroke(
                rect.shrink(0.5),
                0.,
                Stroke::new(1_f32, pal::SHADOW),
                egui::StrokeKind::Middle,
            ),
        ]),
    );
}

/// A status line: "Stopped" as a short divider, errors and the usage limit
/// as plain lines in their colors.
fn status_ui(
    ui: &mut egui::Ui,
    kind: StatusKind,
    text: &str,
    width: f32,
    sel: &mut SelCtx,
    out: &mut ListOut,
) {
    use crate::theme::{color, ERROR_TEXT, WARNING_TEXT};
    let no_tips = HashMap::new();
    match kind {
        StatusKind::Note => {
            let galley =
                ui.painter()
                    .layout_no_wrap(text.to_owned(), kit::body_font(), secondary());
            if galley.size().x > width - 40. {
                // A longer note wraps as plain grey text.
                text_ui(
                    ui,
                    &plain(text),
                    width,
                    0.,
                    Some(secondary()),
                    &no_tips,
                    Some(sel),
                    out,
                );
                return;
            }
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, LINE), Sense::hover());
            let p = ui.painter();
            let tw = galley.size().x;
            let mid = rect.center();
            let y = mid.y.round() + 0.5;
            for (a, b) in [
                (rect.left(), mid.x - tw / 2. - 8.),
                (mid.x + tw / 2. + 8., rect.right()),
            ] {
                p.line_segment(
                    [Pos2::new(a, y), Pos2::new(b, y)],
                    Stroke::new(1_f32, pal::CAPTION_LINE),
                );
            }
            p.galley(
                Pos2::new(mid.x - tw / 2., mid.y - galley.size().y / 2.),
                galley,
                secondary(),
            );
        }
        StatusKind::Error => text_ui(
            ui,
            &plain(text),
            width,
            0.,
            Some(color(ERROR_TEXT)),
            &no_tips,
            Some(sel),
            out,
        ),
        StatusKind::Limit => text_ui(
            ui,
            &plain(text),
            width,
            0.,
            Some(color(WARNING_TEXT)),
            &no_tips,
            Some(sel),
            out,
        ),
    }
}
