//! The chrome of the shared document viewer (Print Preview and the report
//! preview): the toolbar with its zoom box, page box and navigation, the
//! Find bar, the page strip, and the remembered zoom and strip settings.
//! The pages themselves are laid out and scrolled by `viewer`; the owner
//! builds and paints the ones in view and the strip's thumbnails.
use super::ops::{Op, Page};
use super::viewer::{self, Fit, Nav};
use crate::*;

pub const TOOLBAR: f32 = 30.;
const FIND_BAR: f32 = 28.;
/// Width of the page strip.
pub const STRIP: f32 = 132.;
const THUMB_W: f32 = 96.;
const STRIP_BG: Color32 = Color32::from_gray(92);
const MATCH: Color32 = Color32::from_rgba_premultiplied(120, 110, 0, 120);
const CURRENT: Color32 = Color32::from_rgba_premultiplied(140, 70, 0, 150);
/// Zoom box entries after Fit Width and Fit Page.
const STEPS: [(&str, f32); 5] = [
    ("50%", 0.5),
    ("75%", 0.75),
    ("100%", 1.),
    ("150%", 1.5),
    ("200%", 2.),
];

const PREF_ZOOM: &str = "viewer_zoom_v1";
const PREF_STRIP: &str = "viewer_strip_v1";

/// The action names of one viewer: every control is `<prefix>_<what>`, but
/// the page-by-page arrows and the close button keep their own names.
#[derive(Clone, Copy)]
pub struct Names {
    pub prefix: &'static str,
    pub prev: &'static str,
    pub next: &'static str,
    pub close: &'static str,
    /// Tells apart viewers of one kind that are on screen together (report
    /// tabs): part of every widget id.
    pub salt: u32,
}

pub const PRINT: Names = Names {
    prefix: "preview",
    prev: "preview_left",
    next: "preview_right",
    close: "preview_close",
    salt: 0,
};

pub const REPORT: Names = Names {
    prefix: "report_preview",
    prev: "report_preview_prev",
    next: "report_preview_next",
    close: "report_preview_close",
    salt: 0,
};

/// The viewer of a report tab (its own names, so a report tab and a
/// modal preview can stand side by side).
pub const TAB: Names = Names {
    prefix: "report_tab",
    prev: "report_tab_prev",
    next: "report_tab_next",
    close: "report_tab_close",
    salt: 0,
};

/// A found piece of text: the page and its box in page pixels.
#[derive(Clone, Copy, Debug)]
pub struct Match {
    pub page: usize,
    pub rect: Rect,
}

/// Find in the document. The owner feeds it the pages one at a time
/// (`next_scan`, `scan`) so a long document is searched over several frames.
#[derive(Clone, Debug, Default)]
pub struct Find {
    pub open: bool,
    pub query: String,
    pub matches: Vec<Match>,
    pub current: Option<usize>,
    /// Pages searched for `done_for`.
    scanned: usize,
    done_for: String,
    /// Put the keyboard focus in the text box on the next frame.
    focus: bool,
    /// Scroll to the current match on the next frame.
    jump: bool,
}

impl Find {
    /// The document changed under the search: search it again.
    pub fn invalidate(&mut self) {
        self.done_for = String::from("\u{0}");
    }
    fn reset_if_changed(&mut self) {
        if self.query != self.done_for {
            self.done_for = self.query.clone();
            self.matches.clear();
            self.current = None;
            self.scanned = 0;
        }
    }
    /// The next page to search, or `None` when the search is done (or has
    /// nothing to look for).
    pub fn next_scan(&mut self, count: usize) -> Option<usize> {
        self.reset_if_changed();
        if self.query.is_empty() || !self.open {
            return None;
        }
        (self.scanned < count).then_some(self.scanned)
    }
    /// Adds the matches of page `k`, the one `next_scan` named.
    pub fn scan(&mut self, k: usize, page: &Page) {
        if k != self.scanned {
            return;
        }
        self.matches.extend(find_in_page(k, page, &self.query));
        self.scanned = k + 1;
        if self.current.is_none() && !self.matches.is_empty() {
            self.current = Some(0);
            self.jump = true;
        }
    }
    /// Is some of the document still to be searched?
    pub fn searching(&self, count: usize) -> bool {
        !self.query.is_empty() && self.scanned < count
    }
    /// Moves to the next or previous match, wrapping.
    pub fn step(&mut self, forward: bool) {
        let n = self.matches.len();
        if n == 0 {
            return;
        }
        let at = self.current.unwrap_or(if forward { n - 1 } else { 0 });
        self.current = Some(if forward {
            (at + 1) % n
        } else {
            (at + n - 1) % n
        });
        self.jump = true;
    }
    pub fn current_match(&self) -> Option<Match> {
        self.current.and_then(|i| self.matches.get(i).copied())
    }
    /// "3 of 12", "No matches", or how far the search has got.
    pub fn status(&self, count: usize) -> String {
        if self.query.is_empty() {
            return String::new();
        }
        let searching = if self.searching(count) {
            format!(" (searching, {} of {count} pages)", self.scanned)
        } else {
            String::new()
        };
        match self.current {
            Some(i) => format!("{} of {}{searching}", i + 1, self.matches.len()),
            None if self.searching(count) => {
                format!("Searching, {} of {count} pages", self.scanned)
            }
            None => "No matches".into(),
        }
    }
}

/// The boxes of `query` in the text runs of `page`, in page pixels, by
/// reading order of the runs. Case does not matter.
pub fn find_in_page(k: usize, page: &Page, query: &str) -> Vec<Match> {
    let needle: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    let mut out = vec![];
    if needle.is_empty() {
        return out;
    }
    let mut runs: Vec<(f32, f32, Rect)> = vec![];
    for c in &page.ops {
        let Op::Text {
            pos,
            text,
            size,
            width,
            ..
        } = &c.op
        else {
            continue;
        };
        let hay: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
        let n = text.chars().count().max(1) as f32;
        if hay.len() < needle.len() {
            continue;
        }
        let mut at = 0;
        while at + needle.len() <= hay.len() {
            if hay[at..at + needle.len()] == needle[..] {
                let x0 = pos.x + width * at as f32 / n;
                let x1 = pos.x + width * (at + needle.len()) as f32 / n;
                let rect = Rect::from_min_max(
                    Pos2::new(x0, pos.y - size * 0.85),
                    Pos2::new(x1.max(x0 + 1.), pos.y + size * 0.25),
                );
                runs.push((rect.top(), rect.left(), rect));
                at += needle.len();
            } else {
                at += 1;
            }
        }
    }
    runs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    out.extend(runs.into_iter().map(|(_, _, rect)| Match { page: k, rect }));
    out
}

/// What the viewer keeps between frames besides the page, the zoom and the
/// scroll.
#[derive(Clone, Debug, Default)]
pub struct ViewState {
    /// What `zoom: None` fits.
    pub fit: Fit,
    /// Is the page strip showing?
    pub thumbs: bool,
    pub find: Find,
    page_text: String,
    pub thumb_scroll: f32,
    last_page: Option<usize>,
    /// Zoom or strip changed and is not saved yet.
    dirty: bool,
}

impl ViewState {
    /// The page box's text, for tests.
    #[cfg(test)]
    pub fn page_text(&self) -> &str {
        &self.page_text
    }
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}

fn encode_zoom(zoom: Option<f32>, fit: Fit) -> String {
    match (zoom, fit) {
        (Some(z), _) => format!("z:{z:.4}"),
        (None, Fit::Width) => "width".into(),
        (None, Fit::Page) => "page".into(),
    }
}

fn decode_zoom(s: &str) -> (Option<f32>, Fit) {
    match s {
        "width" => (None, Fit::Width),
        "page" => (None, Fit::Page),
        _ => s
            .strip_prefix("z:")
            .and_then(|z| z.parse::<f32>().ok())
            .filter(|z| (viewer::MIN_ZOOM..=viewer::MAX_ZOOM).contains(z))
            .map_or((None, Fit::Page), |z| (Some(z), Fit::Page)),
    }
}

impl App {
    /// The zoom and the strip a viewer opens with: what the last one left.
    pub(crate) fn viewer_start(&self) -> (Option<f32>, ViewState) {
        let saved = |key: &str| self.store.preference(key).ok().flatten();
        let (zoom, fit) = saved(PREF_ZOOM).map_or((None, Fit::Page), |s| decode_zoom(&s));
        let state = ViewState {
            fit,
            thumbs: saved(PREF_STRIP).is_some_and(|s| s == "1"),
            ..ViewState::default()
        };
        (zoom, state)
    }
    /// Remembers the zoom and the strip for the next viewer.
    pub(crate) fn save_viewer_prefs(&self, zoom: Option<f32>, view: &ViewState) {
        let _ = self
            .store
            .set_preference(PREF_ZOOM, &encode_zoom(zoom, view.fit));
        let _ = self
            .store
            .set_preference(PREF_STRIP, if view.thumbs { "1" } else { "0" });
    }
}

/// A toolbar button with a text label; `selected` draws it pressed in.
pub fn text_button(
    ui: &mut egui::Ui,
    actions: &mut Vec<(String, Rect)>,
    name: &str,
    label: &str,
    enabled: bool,
    selected: bool,
) -> bool {
    let r = crate::dialog_kit::toolbar_button(ui, label, enabled, selected);
    actions.push((name.into(), r.rect));
    r.clicked()
}

/// A toolbar button with one of our icons.
fn tool(
    ui: &mut egui::Ui,
    actions: &mut Vec<(String, Rect)>,
    name: &str,
    icon: chrome::Icon,
    tip: &str,
    enabled: bool,
) -> bool {
    let r = chrome::tool_button(ui, icon, tip, enabled, false, false);
    actions.push((name.into(), r.rect));
    enabled && r.clicked()
}

/// What the owner gives the viewer each frame.
pub struct Chrome<'a> {
    pub names: Names,
    /// The viewer's whole area: toolbar, pages and strip; the owner's
    /// status line is below it.
    pub area: Rect,
    pub page_size: Vec2,
    pub count: usize,
    pub across: usize,
    pub page: &'a mut usize,
    pub zoom: &'a mut Option<f32>,
    pub nav: &'a mut Nav,
    pub view: &'a mut ViewState,
    /// Does the viewer take the keys and the wheel (no dialog over it)?
    pub active: bool,
}

/// What the owner paints and does after `begin`.
pub struct Frame {
    pub zoom: f32,
    /// Pages in view with the screen position of their top left.
    pub cells: Vec<(usize, Pos2)>,
    pub painter: egui::Painter,
    /// Thumbnails showing in the page strip with their rectangles.
    pub thumbs: Vec<(usize, Rect)>,
    pub thumb_painter: egui::Painter,
    /// Did the wheel, a drag, a key or a zoom move the view this frame?
    pub moved: bool,
    /// Rectangles of the controls, by action name.
    pub actions: Vec<(String, Rect)>,
    pub close: bool,
    /// The zoom or the strip changed and has settled: save them.
    pub save_prefs: bool,
}

/// Does the viewer take this frame's wheel at the pointer, apart from the
/// page view? (The strip scrolls by it.)
fn strip_wheel(ctx: &egui::Context) -> Vec2 {
    viewer::taken_scroll(ctx).unwrap_or(Vec2::ZERO)
}

/// The owner's toolbar buttons: draws them and records their rectangles.
pub type HostButtons<'a> = dyn FnMut(&mut egui::Ui, &mut Vec<(String, Rect)>) + 'a;

/// Lays out and draws the toolbar, the Find bar and the strip, and applies
/// the frame's input. `host` draws the owner's buttons at the left of the
/// toolbar.
pub fn begin(ui: &mut egui::Ui, c: Chrome, host: &mut HostButtons) -> Frame {
    let Chrome {
        names,
        area,
        page_size,
        count,
        across,
        page,
        zoom,
        nav,
        view,
        active,
    } = c;
    let ctx = ui.ctx().clone();
    let p = names.prefix;
    let salt = names.salt;
    let before = (
        zoom.map(|z| (z * 1000.).round() as i64),
        view.fit,
        view.thumbs,
    );
    if active && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::F)) {
        view.find.open = true;
        view.find.focus = true;
    }
    view.find.reset_if_changed();
    if view.find.jump {
        view.find.jump = false;
        if let Some(m) = view.find.current_match() {
            nav.go_to_rect(m.page, m.rect);
        }
    }
    let bar = Rect::from_min_size(area.min, Vec2::new(area.width(), TOOLBAR));
    let find_rect = view
        .find
        .open
        .then(|| Rect::from_min_size(bar.left_bottom(), Vec2::new(area.width(), FIND_BAR)));
    let top = find_rect.map_or(bar.bottom(), |r| r.bottom());
    let body = Rect::from_min_max(Pos2::new(area.left(), top), area.max);
    let strip = view.thumbs.then(|| {
        Rect::from_min_size(
            body.min,
            Vec2::new(STRIP.min(body.width() / 2.), body.height()),
        )
    });
    let pages = Rect::from_min_max(
        Pos2::new(body.left() + strip.map_or(0., |r| r.width()), body.top()),
        body.max,
    );
    let pointer = ctx.input(|i| i.pointer.hover_pos());
    let over_strip = strip.zip(pointer).is_some_and(|(r, pos)| r.contains(pos));
    let laid = viewer::lay_out(
        ui,
        pages,
        egui::Id::new((p, salt, "pages")),
        viewer::Params {
            page_size,
            count,
            across,
            page: &mut *page,
            zoom: &mut *zoom,
            nav: &mut *nav,
            active,
            fit: view.fit,
            wheel: !over_strip,
        },
    );
    let zoom_now = laid.zoom;
    let mut actions: Vec<(String, Rect)> = vec![];
    let mut close = false;
    // The toolbar.
    let painter = ui.painter().clone();
    painter.rect_filled(bar, 0., chrome::pal::FACE);
    chrome::hline(
        &painter,
        bar.left(),
        bar.right(),
        bar.bottom() - 1.,
        chrome::pal::SHADOW,
    );
    let mut bar_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(bar.shrink2(Vec2::new(6., 3.)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    bar_ui.spacing_mut().item_spacing = Vec2::new(2., 0.);
    host(&mut bar_ui, &mut actions);
    bar_ui.add_space(8.);
    if text_button(
        &mut bar_ui,
        &mut actions,
        &format!("{p}_thumbs"),
        "Pages",
        count > 0,
        view.thumbs,
    ) {
        view.thumbs = !view.thumbs;
    }
    if text_button(
        &mut bar_ui,
        &mut actions,
        &format!("{p}_find"),
        "Find",
        count > 0,
        view.find.open,
    ) {
        view.find.open = !view.find.open;
        view.find.focus = view.find.open;
    }
    bar_ui.add_space(8.);
    use chrome::Icon;
    if tool(
        &mut bar_ui,
        &mut actions,
        &format!("{p}_zoom_out"),
        Icon::ZoomOut,
        "Zoom Out",
        zoom_now > viewer::MIN_ZOOM,
    ) {
        *zoom = Some((zoom_now / 1.25).max(viewer::MIN_ZOOM));
    }
    let label = match (*zoom, view.fit) {
        (None, Fit::Width) => "Fit Width".to_string(),
        (None, Fit::Page) => "Fit Page".to_string(),
        (Some(_), _) => format!("{:.0}%", zoom_now * 100.),
    };
    let mut pick: Option<(Option<f32>, Fit)> = None;
    let combo_resp = crate::dialog_kit::flow_combo_with(
        &mut bar_ui,
        (p, salt, "zoom_box"),
        84.,
        &label,
        true,
        |ui| {
            let mut item = |ui: &mut egui::Ui, name: &str, label: &str, on: bool| {
                let r = crate::dialog_kit::flow_selectable(ui, on, label, 80.);
                actions.push((format!("{p}_zoom_{name}"), r.rect));
                if r.clicked() {
                    ui.close();
                    true
                } else {
                    false
                }
            };
            if item(
                ui,
                "fit_width",
                "Fit Width",
                zoom.is_none() && view.fit == Fit::Width,
            ) {
                pick = Some((None, Fit::Width));
            }
            if item(
                ui,
                "fit_page",
                "Fit Page",
                zoom.is_none() && view.fit == Fit::Page,
            ) {
                pick = Some((None, Fit::Page));
            }
            for (label, z) in STEPS {
                let on = zoom.is_some_and(|c| (c - z).abs() < 1e-3);
                if item(ui, label.trim_end_matches('%'), label, on) {
                    pick = Some((Some(z), view.fit));
                }
            }
        },
    );
    actions.push((format!("{p}_zoom_box"), combo_resp.rect));
    if let Some((z, f)) = pick {
        *zoom = z;
        view.fit = f;
    }
    if tool(
        &mut bar_ui,
        &mut actions,
        &format!("{p}_zoom_in"),
        Icon::ZoomIn,
        "Zoom In",
        zoom_now < viewer::MAX_ZOOM,
    ) {
        *zoom = Some((zoom_now * 1.25).min(viewer::MAX_ZOOM));
    }
    if tool(
        &mut bar_ui,
        &mut actions,
        &format!("{p}_fit"),
        Icon::Fit,
        "Zoom to Fit",
        zoom.is_some() || view.fit != Fit::Page,
    ) {
        *zoom = None;
        view.fit = Fit::Page;
    }
    bar_ui.add_space(8.);
    // Pages.
    let at = *page;
    let (first, last) = (at > 0, at + 1 < count);
    let go = |k: usize, page: &mut usize, nav: &mut Nav| {
        *page = k;
        nav.go_to(k);
    };
    let wide = across > 1;
    let (prev_tip, next_tip) = if wide {
        ("Left One Page", "Right One Page")
    } else {
        ("Previous Page", "Next Page")
    };
    if text_button(
        &mut bar_ui,
        &mut actions,
        &format!("{p}_first"),
        "First Page",
        first,
        false,
    ) {
        go(0, page, nav);
    }
    if tool(
        &mut bar_ui,
        &mut actions,
        names.prev,
        Icon::ShiftLeft,
        prev_tip,
        first,
    ) {
        go(at - 1, page, nav);
    }
    if tool(
        &mut bar_ui,
        &mut actions,
        names.next,
        Icon::ShiftRight,
        next_tip,
        last,
    ) {
        go(at + 1, page, nav);
    }
    if wide {
        if tool(
            &mut bar_ui,
            &mut actions,
            &format!("{p}_up"),
            Icon::ShiftUp,
            "Up One Page",
            at >= across,
        ) {
            go(at - across, page, nav);
        }
        if tool(
            &mut bar_ui,
            &mut actions,
            &format!("{p}_down"),
            Icon::ShiftDown,
            "Down One Page",
            at + across < count,
        ) {
            go(at + across, page, nav);
        }
    }
    if text_button(
        &mut bar_ui,
        &mut actions,
        &format!("{p}_last"),
        "Last Page",
        last,
        false,
    ) {
        go(count - 1, page, nav);
    }
    bar_ui.add_space(8.);
    bar_ui.label(RichText::new("Page").font(chrome::ui_font()));
    let box_id = bar_ui.make_persistent_id((p, salt, "page_box"));
    if !ctx.memory(|m| m.has_focus(box_id)) {
        view.page_text = (at + 1).min(count).to_string();
    }
    let edit = crate::dialog_kit::flow_edit(
        &mut bar_ui,
        (p, salt, "page_box"),
        &mut view.page_text,
        38.,
        true,
    );
    actions.push((format!("{p}_page_box"), edit.rect));
    if edit.lost_focus() && ctx.input(|i| i.key_pressed(Key::Enter)) {
        if let Ok(n) = view.page_text.trim().parse::<usize>() {
            go(n.clamp(1, count.max(1)) - 1, page, nav);
        }
    }
    bar_ui.label(RichText::new(format!("of {count}")).font(chrome::ui_font()));
    bar_ui.add_space(8.);
    close |= text_button(&mut bar_ui, &mut actions, names.close, "Close", true, false);
    // The Find bar.
    if let Some(fr) = find_rect {
        painter.rect_filled(fr, 0., chrome::pal::FACE);
        chrome::hline(
            &painter,
            fr.left(),
            fr.right(),
            fr.bottom() - 1.,
            chrome::pal::SHADOW,
        );
        let mut f_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(fr.shrink2(Vec2::new(6., 3.)))
                .layout(Layout::left_to_right(Align::Center)),
        );
        f_ui.spacing_mut().item_spacing = Vec2::new(4., 0.);
        f_ui.label(RichText::new("Find").font(chrome::ui_font()));
        let edit = crate::dialog_kit::flow_edit(
            &mut f_ui,
            (p, salt, "find_text"),
            &mut view.find.query,
            220.,
            true,
        );
        actions.push((format!("{p}_find_text"), edit.rect));
        if std::mem::take(&mut view.find.focus) {
            edit.request_focus();
        }
        view.find.reset_if_changed();
        let entered = edit.lost_focus() && ctx.input(|i| i.key_pressed(Key::Enter));
        let some = !view.find.matches.is_empty();
        let back = ctx.input(|i| i.modifiers.shift);
        let prev = text_button(
            &mut f_ui,
            &mut actions,
            &format!("{p}_find_prev"),
            "Previous",
            some,
            false,
        );
        let next = text_button(
            &mut f_ui,
            &mut actions,
            &format!("{p}_find_next"),
            "Next",
            some,
            false,
        );
        if prev || (entered && back) {
            view.find.step(false);
        } else if next || entered {
            view.find.step(true);
        }
        if entered {
            edit.request_focus();
        }
        if text_button(
            &mut f_ui,
            &mut actions,
            &format!("{p}_find_close"),
            "Close",
            true,
            false,
        ) {
            view.find.open = false;
        }
        f_ui.label(RichText::new(view.find.status(count)).font(chrome::ui_font()));
    }
    // The page strip.
    let mut thumbs = vec![];
    let strip_painter = ui.painter_at(strip.unwrap_or(Rect::NOTHING));
    if let Some(sr) = strip {
        strip_painter.rect_filled(sr, 0., STRIP_BG);
        chrome::vline(
            &strip_painter,
            sr.right() - 1.,
            sr.top(),
            sr.bottom(),
            chrome::pal::SHADOW,
        );
        let th = if page_size.x > 0. {
            THUMB_W * page_size.y / page_size.x
        } else {
            THUMB_W
        };
        let cell = th + 28.;
        let total = 10. + count as f32 * cell;
        let max = (total - sr.height()).max(0.);
        let mut by_hand = false;
        if active && over_strip {
            let wheel = strip_wheel(&ctx);
            if wheel.y != 0. {
                view.thumb_scroll -= wheel.y * 2.5;
                by_hand = true;
            }
        }
        if view.last_page != Some(*page) && !by_hand {
            let (lo, hi) = (
                10. + *page as f32 * cell,
                10. + *page as f32 * cell + th + 20.,
            );
            if lo < view.thumb_scroll {
                view.thumb_scroll = lo - 10.;
            } else if hi > view.thumb_scroll + sr.height() {
                view.thumb_scroll = hi - sr.height() + 10.;
            }
        }
        view.last_page = Some(*page);
        view.thumb_scroll = view.thumb_scroll.clamp(0., max);
        if count > 0 {
            let lo = (((view.thumb_scroll - 10.) / cell).floor().max(0.)) as usize;
            let hi = ((((view.thumb_scroll + sr.height()) - 10.) / cell)
                .floor()
                .max(0.) as usize)
                .min(count - 1);
            for k in lo..=hi {
                let top = sr.top() + 10. + k as f32 * cell - view.thumb_scroll;
                let rect = Rect::from_min_size(
                    Pos2::new(sr.left() + (STRIP - 1. - THUMB_W) / 2., top),
                    Vec2::new(THUMB_W, th),
                );
                let hit = Rect::from_min_size(rect.min, Vec2::new(THUMB_W, th + 20.)).intersect(sr);
                let resp = ui.interact(hit, egui::Id::new((p, salt, "thumb", k)), Sense::click());
                actions.push((format!("{p}_thumb_{k}"), hit));
                if resp.clicked() {
                    go(k, page, nav);
                }
                let current = k == *page;
                if current {
                    strip_painter.rect_stroke(
                        rect.expand(3.),
                        0.,
                        Stroke::new(2_f32, chrome::pal::PRESSED_BORDER),
                        egui::StrokeKind::Outside,
                    );
                }
                strip_painter.text(
                    Pos2::new(rect.center().x, rect.bottom() + 10.),
                    Align2::CENTER_CENTER,
                    (k + 1).to_string(),
                    if current {
                        chrome::bold_font()
                    } else {
                        chrome::ui_font()
                    },
                    Color32::WHITE,
                );
                thumbs.push((k, rect));
            }
        }
    }
    let after = (
        zoom.map(|z| (z * 1000.).round() as i64),
        view.fit,
        view.thumbs,
    );
    if after != before {
        view.dirty = true;
    }
    let save_prefs = view.dirty && !laid.moved;
    Frame {
        zoom: zoom_now,
        cells: laid.cells,
        painter: laid.painter,
        thumbs,
        thumb_painter: strip_painter,
        moved: laid.moved,
        actions,
        close,
        save_prefs,
    }
}

/// Paints the highlight of the Find matches on page `k`, whose top left is
/// at `origin` on screen.
pub fn paint_matches(painter: &egui::Painter, find: &Find, k: usize, origin: Pos2, zoom: f32) {
    if !find.open || find.matches.is_empty() {
        return;
    }
    let start = find.matches.partition_point(|m| m.page < k);
    for (i, m) in find.matches.iter().enumerate().skip(start) {
        if m.page != k {
            break;
        }
        let r = Rect::from_min_max(
            origin + m.rect.min.to_vec2() * zoom,
            origin + m.rect.max.to_vec2() * zoom,
        );
        let now = find.current == Some(i);
        painter.rect_filled(r, 0., if now { CURRENT } else { MATCH });
        if now {
            painter.rect_stroke(
                r,
                0.,
                Stroke::new(1_f32, Color32::from_rgb(0xC0, 0x58, 0x00)),
                egui::StrokeKind::Outside,
            );
        }
    }
}

/// Paints a thumbnail: blank paper, with the page's marks when it is built.
pub fn paint_thumb(painter: &egui::Painter, rect: Rect, page: Option<&Page>) {
    painter.rect_filled(rect.translate(Vec2::splat(2.)), 0., Color32::from_gray(48));
    painter.rect_filled(rect, 0., Color32::WHITE);
    if let Some(page) = page {
        if page.size.x > 0. {
            super::preview::paint_page_marks(painter, page, rect.min, rect.width() / page.size.x);
        }
    }
}
