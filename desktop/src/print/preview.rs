//! Print Preview: the printed pages over a dark backdrop, with a toolbar for
//! Page Setup, Print, Publish to PDF, zoom and page navigation. Pages are
//! painted from the same ops the PDF writer receives, and scroll as one
//! stack (`viewer`); only the pages in view are built.
use super::compose::{scale_line, PrintJob, READABLE_SCALE, TINY_SCALE};
use super::ops::{Clipped, Op, Page};
use super::setup::PageSetup;
use super::viewer;
use super::viewer_ui::{self, ViewState};
use crate::*;
use std::collections::HashMap;
use std::rc::Rc;

/// Dashes whose period is under this many screen pixels are painted as one
/// faint line.
const FINE_DASH: f32 = 4.;

/// Text for a printout that is small or runs to pages side by side.
const WARNING: Color32 = Color32::from_rgb(0xC0, 0x58, 0x00);
pub(crate) const BACKDROP: Color32 = Color32::from_rgb(128, 128, 128);
const STATUS: f32 = 20.;

/// Most built pages a preview keeps.
const KEPT_PAGES: usize = 64;

/// Pages built in one frame. The pages scrolled into view past this show as
/// blank paper and are built on the frames after, so a fast scroll or a
/// zoom out never makes one long frame.
const BUILDS_PER_FRAME: usize = 1;

/// A setup waiting for its canvas to be painted.
struct Waiting {
    setup: PageSetup,
    /// Has a frame shown "Updating" yet? The painting happens on the next.
    announced: bool,
}

pub struct Preview {
    /// The page grid and canvas; `None` until the first one is painted.
    job: Option<PrintJob>,
    /// Pages of `job` built so far (a page is built when first shown).
    pages: HashMap<usize, Rc<Page>>,
    /// Page shown, from 0.
    pub page: usize,
    /// Screen pixels per page pixel; `None` fits the page to the window.
    pub zoom: Option<f32>,
    /// Where the pages are scrolled to.
    pub nav: viewer::Nav,
    /// Fit mode, page strip and Find.
    pub view: ViewState,
    /// First and last page in view, kept when pages are dropped.
    visible: (usize, usize),
    waiting: Option<Waiting>,
}

impl Preview {
    pub fn empty() -> Self {
        Self {
            job: None,
            pages: HashMap::new(),
            page: 0,
            zoom: None,
            nav: viewer::Nav::default(),
            view: ViewState::default(),
            visible: (0, 0),
            waiting: None,
        }
    }
    /// Shows `job`, keeping the page number as far as the new count allows.
    pub fn set_job(&mut self, job: PrintJob) {
        self.page = self.page.min(job.count().saturating_sub(1));
        self.job = Some(job);
        self.pages.clear();
        self.waiting = None;
        self.nav.go_to(self.page);
    }
    /// A setup whose canvas has to be painted before it can be shown.
    pub fn wait_for(&mut self, setup: PageSetup) {
        self.waiting = Some(Waiting {
            setup,
            announced: false,
        });
    }
    /// Laid out and nothing waiting to be painted.
    #[cfg(test)]
    pub fn ready(&self) -> bool {
        self.job.is_some() && self.waiting.is_none()
    }
    pub fn count(&self) -> usize {
        self.job.as_ref().map_or(0, |j| j.count())
    }
    pub(super) fn across(&self) -> usize {
        self.job.as_ref().map_or(1, |j| j.across()).max(1)
    }
    /// Printed scale, 1 before the first job.
    fn scale(&self) -> f32 {
        self.job.as_ref().map_or(1., |j| j.scale())
    }
    /// Moves by (`dx` pages across, `dy` rows of pages down).
    #[cfg(test)]
    pub fn step(&mut self, dx: isize, dy: isize) {
        let across = self.across() as isize;
        let target = self.page as isize + dx + dy * across;
        if target >= 0 && (target as usize) < self.count() {
            self.go(target as usize);
        }
    }
    /// Scrolls to page `k`.
    #[cfg(test)]
    pub fn go(&mut self, k: usize) {
        self.page = k;
        self.nav.go_to(k);
    }
    /// Pages built and kept so far.
    #[cfg(test)]
    pub fn built(&self) -> usize {
        self.pages.len()
    }
    /// Is page `k` built?
    pub fn has_page(&self, k: usize) -> bool {
        self.pages.contains_key(&k)
    }
    /// Size of a page at zoom 1.
    pub(super) fn page_size(&self) -> Vec2 {
        self.job.as_ref().map_or(Vec2::ZERO, |j| j.page_size())
    }
    /// Page `k` if it is built.
    fn cached(&self, k: usize) -> Option<Rc<Page>> {
        self.pages.get(&k).cloned()
    }
    /// Page `k` for a search: the built one, or one built just for the
    /// search and not kept.
    fn page_for_search(&self, app: &App, ctx: &egui::Context, k: usize) -> Option<Rc<Page>> {
        let job = self.job.as_ref()?;
        if k >= job.count() {
            return None;
        }
        Some(
            self.cached(k)
                .unwrap_or_else(|| Rc::new(app.job_page(ctx, job, k))),
        )
    }
    /// Page `k`, built on first use.
    fn page_at(&mut self, app: &App, ctx: &egui::Context, k: usize) -> Option<Rc<Page>> {
        let job = self.job.as_ref()?;
        if k >= job.count() {
            return None;
        }
        if let Some(page) = self.pages.get(&k) {
            return Some(page.clone());
        }
        if self.pages.len() >= KEPT_PAGES {
            // Keep the pages in view and a few around them.
            let (first, last) = self.visible;
            let (lo, hi) = (first.saturating_sub(8), last + 8);
            self.pages.retain(|i, _| (lo..=hi).contains(i));
            if self.pages.len() >= KEPT_PAGES {
                self.pages.clear();
            }
        }
        let page = Rc::new(app.job_page(ctx, job, k));
        self.pages.insert(k, page.clone());
        Some(page)
    }
    /// Every page, for tests.
    #[cfg(test)]
    pub fn all_pages(&mut self, app: &App, ctx: &egui::Context) -> Vec<Rc<Page>> {
        (0..self.count())
            .filter_map(|k| self.page_at(app, ctx, k))
            .collect()
    }
}

fn paint_paper(painter: &egui::Painter, paper: Rect) {
    painter.rect_filled(paper.translate(Vec2::splat(3.)), 0., Color32::from_gray(64));
    painter.rect_filled(paper, 0., Color32::WHITE);
}

/// Blank paper where a page of `size` will be painted once it is built.
pub fn paint_blank(painter: &egui::Painter, size: Vec2, origin: Pos2, zoom: f32) {
    paint_paper(painter, Rect::from_min_size(origin, size * zoom));
}

/// Paints one page's ops scaled by `zoom` with the page's top left at `origin`.
pub fn paint_page(painter: &egui::Painter, page: &Page, origin: Pos2, zoom: f32) {
    let paper = Rect::from_min_size(origin, page.size * zoom);
    paint_paper(painter, paper);
    paint_ops(painter, page, origin, zoom, false);
}

/// The marks of a page at thumbnail size, on paper already painted: no
/// text, and nothing under a pixel.
pub fn paint_page_marks(painter: &egui::Painter, page: &Page, origin: Pos2, zoom: f32) {
    paint_ops(painter, page, origin, zoom, true);
}

fn paint_ops(painter: &egui::Painter, page: &Page, origin: Pos2, zoom: f32, small: bool) {
    let offset = origin.to_vec2();
    let paper = Rect::from_min_size(origin, page.size * zoom);
    let visible = painter.clip_rect();
    for Clipped { clip, op } in &page.ops {
        let clip = Rect::from_min_max(
            Pos2::new(clip.min.x * zoom, clip.min.y * zoom) + offset,
            Pos2::new(clip.max.x * zoom, clip.max.y * zoom) + offset,
        )
        .intersect(visible)
        .intersect(paper);
        if !clip.is_positive() {
            continue;
        }
        let op = op.transformed(zoom, offset);
        let bounds = op.bounds();
        if !clip.intersects(bounds) {
            continue;
        }
        if small
            && (matches!(op, Op::Text { .. }) || (bounds.width() < 0.6 && bounds.height() < 0.6))
        {
            continue;
        }
        let p = painter.with_clip_rect(clip);
        match op {
            Op::Fill { rect, color } => {
                p.rect_filled(rect, 0., color);
            }
            Op::Dash {
                from,
                to,
                width,
                on,
                off,
                color,
            } => {
                // Back to the dashes the screen paints.
                let vertical = from.x == to.x;
                let (start, end) = if vertical {
                    (from.y, to.y)
                } else {
                    (from.x, to.x)
                };
                let step = (on + off).max(0.5);
                let half = width / 2.;
                // A dashed line can run the whole canvas; only the dashes
                // inside the clip are walked.
                let (lo, hi) = if vertical {
                    (clip.top(), clip.bottom())
                } else {
                    (clip.left(), clip.right())
                };
                if step < FINE_DASH {
                    // Dashes too close to tell apart at this zoom: one line
                    // as faint as the dashes' share of it.
                    let (a, b) = (start.max(lo), end.min(hi));
                    if b > a {
                        let r = if vertical {
                            Rect::from_min_max(
                                Pos2::new(from.x - half, a),
                                Pos2::new(from.x + half, b),
                            )
                        } else {
                            Rect::from_min_max(
                                Pos2::new(a, from.y - half),
                                Pos2::new(b, from.y + half),
                            )
                        };
                        p.rect_filled(r, 0., color.gamma_multiply(on / (on + off).max(1e-3)));
                    }
                    continue;
                }
                let skipped = ((lo - start - on) / step).floor().max(0.);
                let mut at = start + skipped * step;
                let end = end.min(hi + step);
                // One mesh for all the dashes, not a shape for each.
                let mut dashes = egui::Mesh::default();
                while at < end {
                    let b = (at + on).min(end);
                    let r = if vertical {
                        Rect::from_min_max(
                            Pos2::new(from.x - half, at),
                            Pos2::new(from.x + half, b),
                        )
                    } else {
                        Rect::from_min_max(
                            Pos2::new(at, from.y - half),
                            Pos2::new(b, from.y + half),
                        )
                    };
                    if r.intersects(clip) {
                        dashes.add_colored_rect(r, color);
                    }
                    at += step;
                }
                if !dashes.is_empty() {
                    p.add(egui::Shape::mesh(dashes));
                }
            }
            Op::Polygon { points, color } => {
                p.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
            }
            Op::Stroke {
                points,
                closed,
                width,
                color,
            } => {
                let stroke = Stroke::new(width.max(0.5), color);
                if closed {
                    p.add(egui::Shape::closed_line(points, stroke));
                } else {
                    p.add(egui::Shape::line(points, stroke));
                }
            }
            Op::Text {
                pos,
                text,
                size,
                bold,
                color,
                ..
            } => {
                if size < 2.5 {
                    continue;
                }
                let family = if bold { "Arial Bold" } else { "Arial" };
                let font = FontId::new(size, egui::FontFamily::Name(family.into()));
                let galley = p.layout_no_wrap(text, font, color);
                let ascent = galley
                    .rows
                    .first()
                    .and_then(|row| row.glyphs.first())
                    .map_or(galley.size().y * 0.8, |glyph| glyph.pos.y);
                p.galley(Pos2::new(pos.x, pos.y - ascent), galley, color);
            }
        }
    }
}

impl App {
    /// Paints the canvas of a setup that was waiting: the frame that first
    /// shows the wait only announces it, the next one does the work.
    fn paint_waiting_preview(&mut self, ctx: &egui::Context, preview: &mut Preview) {
        let Some(waiting) = preview.waiting.as_mut() else {
            return;
        };
        if !waiting.announced {
            waiting.announced = true;
            ctx.request_repaint();
            return;
        }
        let setup = preview.waiting.take().map(|w| w.setup).unwrap_or_default();
        let print_ctx = self.print_context();
        match self.compose_job(&print_ctx, &setup, true) {
            Ok(Some(job)) => preview.set_job(job),
            Ok(None) => {}
            Err(e) => self.error(e),
        }
    }

    pub(crate) fn print_preview_window(&mut self, ctx: &egui::Context) {
        let Some(mut preview) = self.print.preview.take() else {
            return;
        };
        self.paint_waiting_preview(ctx, &mut preview);
        let print_ctx = self.print_context();
        // The viewer keeps the keys and the wheel unless Page Setup is up.
        let active = self.print.dialog.is_none();
        let screen = ctx.content_rect();
        let mut close = false;
        let mut setup = false;
        let mut print = false;
        let mut publish = false;
        let mut save_prefs = false;
        let mut actions: Vec<(String, Rect)> = vec![];
        egui::Area::new(egui::Id::new("print_preview"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (all, _) = ui.allocate_exact_size(screen.size(), Sense::click());
                // The preview fills the screen: a view with its own toolbar, not a
                // dialog. Noted for F1 help and the sweep (which checks its
                // text and controls but no dialog frame).
                crate::dialog_kit::note_client(ui.ctx(), ui.layer_id(), "print_preview", all);
                if active {
                    viewer::cover(ui.ctx(), all);
                }
                let p = ui.painter().clone();
                p.rect_filled(all, 0., BACKDROP);
                let area =
                    Rect::from_min_max(all.min, Pos2::new(all.right(), all.bottom() - STATUS));
                let count = preview.count();
                let across = preview.across();
                let page_size = preview.page_size();
                let Preview {
                    page,
                    zoom,
                    nav,
                    view,
                    ..
                } = &mut preview;
                let mut host = |ui: &mut egui::Ui, acts: &mut Vec<(String, Rect)>| {
                    use chrome::Icon;
                    let mut tool = |name: &str, icon, tip: &str, enabled: bool| {
                        let r = chrome::tool_button(ui, icon, tip, enabled, false, false);
                        acts.push((name.into(), r.rect));
                        enabled && r.clicked()
                    };
                    setup |= tool("preview_page_setup", Icon::PageSetup, "Page Setup", true);
                    print |= tool("preview_print", Icon::Print, "Print", count > 0);
                    publish |= tool("preview_publish", Icon::Export, "Publish to PDF", count > 0);
                };
                let frame = viewer_ui::begin(
                    ui,
                    viewer_ui::Chrome {
                        names: viewer_ui::PRINT,
                        area,
                        page_size,
                        count,
                        across,
                        page,
                        zoom,
                        nav,
                        view,
                        active,
                    },
                    &mut host,
                );
                close |= frame.close;
                save_prefs = frame.save_prefs;
                actions = frame.actions;
                let zoom = frame.zoom;
                // The pages in view; the ones not built yet are built a few
                // a frame, and blank paper stands in for the rest.
                if let (Some(&(first, _)), Some(&(last, _))) =
                    (frame.cells.first(), frame.cells.last())
                {
                    preview.visible = (first, last);
                }
                let mut builds = BUILDS_PER_FRAME;
                let mut pending = false;
                let size = preview.page_size();
                for &(k, origin) in &frame.cells {
                    let page = if preview.has_page(k) || builds > 0 {
                        if !preview.has_page(k) {
                            builds -= 1;
                        }
                        preview.page_at(self, &print_ctx, k)
                    } else {
                        None
                    };
                    match page {
                        Some(page) => paint_page(&frame.painter, &page, origin, zoom),
                        None => {
                            pending = true;
                            paint_blank(&frame.painter, size, origin, zoom);
                        }
                    }
                    viewer_ui::paint_matches(&frame.painter, &preview.view.find, k, origin, zoom);
                }
                // Thumbnails show the pages built so far; when the view is
                // still, one more is built each frame.
                let mut thumb_build = !pending && !frame.moved;
                for &(k, rect) in &frame.thumbs {
                    if thumb_build && !preview.has_page(k) {
                        thumb_build = false;
                        preview.page_at(self, &print_ctx, k);
                    }
                    let page = preview.cached(k);
                    viewer_ui::paint_thumb(&frame.thumb_painter, rect, page.as_deref());
                }
                // Find searches the pages a few a frame, without keeping them.
                let mut scans = 1;
                while scans > 0 && !frame.moved {
                    let Some(k) = preview.view.find.next_scan(count) else {
                        break;
                    };
                    scans -= 1;
                    match preview.page_for_search(self, &print_ctx, k) {
                        Some(page) => preview.view.find.scan(k, &page),
                        None => break,
                    }
                }
                if pending || preview.view.find.searching(count) {
                    ui.ctx().request_repaint();
                }
                let status = Rect::from_min_max(Pos2::new(all.left(), area.bottom()), all.max);
                p.rect_filled(status, 0., chrome::pal::FACE);
                let scale = preview.scale();
                let info = if preview.job.is_none() {
                    "Computing the pages...".to_string()
                } else if preview.waiting.is_some() {
                    format!("{count} page(s); updating the preview...")
                } else if across > 1 {
                    format!(
                        "{}. {across} pages across: tape or place side by side",
                        scale_line(count, across, scale)
                    )
                } else {
                    scale_line(count, across, scale)
                };
                let (font, color) = if preview.job.is_none() || scale >= READABLE_SCALE - 1e-4 {
                    (chrome::ui_font(), INK)
                } else if scale >= TINY_SCALE - 1e-4 {
                    (chrome::ui_font(), WARNING)
                } else {
                    (chrome::bold_font(), WARNING)
                };
                let color = if across > 1 { WARNING } else { color };
                p.text(
                    status.left_center() + Vec2::new(8., 0.),
                    Align2::LEFT_CENTER,
                    info,
                    font,
                    color,
                );
            });
        for (name, r) in actions {
            self.action_rects
                .insert(name, [r.left(), r.top(), r.right(), r.bottom()]);
        }
        if close || save_prefs {
            self.save_viewer_prefs(preview.zoom, &preview.view);
            preview.view.take_dirty();
        }
        if close {
            return;
        }
        self.print.preview = Some(preview);
        if setup {
            self.open_page_setup();
        }
        if print {
            self.print_command();
        }
        if publish {
            self.publish_pdf();
        }
    }
}
