//! The scrolling page viewer of Print Preview and the report preview: the
//! pages stacked top to bottom with a gap between them (side by side where a
//! printout runs several pages across), scrolled by the mouse wheel, a drag
//! on the page, the keyboard and a scroll bar. The viewer lays the pages out
//! and says which are in view; the owner builds and paints only those.
use crate::*;

/// Space around the stack of pages.
pub const MARGIN: f32 = 20.;
/// Space between pages: the margin, so a page that fills the view at Fit
/// leaves the next one just out of it.
pub const GAP: f32 = MARGIN;
pub const MIN_ZOOM: f32 = 0.1;
pub const MAX_ZOOM: f32 = 4.;
/// Width of a scroll bar.
pub const BAR: f32 = 17.;
/// Screen pixels one wheel notch scrolls, in units of what egui reports
/// (40 pixels a notch).
pub const WHEEL_GAIN: f32 = 2.5;
/// Screen pixels an arrow key scrolls.
pub const ARROW_STEP: f32 = 40.;
/// How far above a page the view stops when it is scrolled to the page.
const NEAR: f32 = 8.;
const COVER: &str = "page_viewer_cover";
const TAKEN: &str = "page_viewer_wheel";

/// Where the view is. The page and zoom the owner shows are fields of the
/// owner; this is the rest.
#[derive(Clone, Debug, Default)]
pub struct Nav {
    /// Screen pixels the stack of pages is scrolled by, at `zoom`.
    pub scroll: Vec2,
    /// The zoom `scroll` is in; 0 before the first frame.
    zoom: f32,
    /// Page the next frame scrolls to.
    goto: Option<usize>,
    /// A part of a page the next frame scrolls into the middle of the view.
    goto_rect: Option<(usize, Rect)>,
    /// Page a jump went to: it stays the current page until the view is moved
    /// by hand (the last pages cannot always reach the top of the view).
    hold: Option<usize>,
    /// Offset of the pointer in the thumb being dragged, per axis.
    grab: Option<(usize, f32)>,
    /// The pages in view and where they were painted, for tests.
    #[cfg(test)]
    pub shown: Vec<(usize, Pos2)>,
}

impl Nav {
    /// Scrolls to page `k` on the next frame.
    pub fn go_to(&mut self, k: usize) {
        self.goto = Some(k);
    }
    /// Scrolls the next frame to show `rect` (page pixels) of page `k` in
    /// the middle of the view.
    pub fn go_to_rect(&mut self, k: usize, rect: Rect) {
        self.goto_rect = Some((k, rect));
    }
}

/// What the viewer needs of its owner each frame.
pub struct Params<'a> {
    /// Size of a page at zoom 1.
    pub page_size: Vec2,
    pub count: usize,
    /// Pages in a row of the stack.
    pub across: usize,
    /// The current page; follows the page at the top of the view.
    pub page: &'a mut usize,
    /// Screen pixels per page pixel; `None` fits a page to the view.
    pub zoom: &'a mut Option<f32>,
    pub nav: &'a mut Nav,
    /// Is the viewer the one that takes keys and the wheel (no dialog over it)?
    pub active: bool,
    /// What `zoom: None` fits.
    pub fit: Fit,
    /// Does the wheel over the view scroll it (false where another part of
    /// the viewer, the page strip, has the pointer)?
    pub wheel: bool,
}

/// The pages in view and where to paint them.
pub struct Laid {
    /// The zoom in force.
    pub zoom: f32,
    /// Pages in view with the screen position of their top left.
    pub cells: Vec<(usize, Pos2)>,
    /// Paints inside the part of the view the pages use.
    pub painter: egui::Painter,
    /// Did the wheel, a drag, a key or a zoom move the view this frame?
    pub moved: bool,
}

/// What `zoom: None` fits to the view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fit {
    /// The whole page.
    #[default]
    Page,
    /// The page's width (the scroll bar's room included).
    Width,
}

/// The zoom that fits a page of `page` pixels to a view of `view`.
#[cfg(test)]
pub fn fit_zoom(view: Vec2, page: Vec2) -> f32 {
    fit_zoom_to(view, page, Fit::Page)
}

/// The zoom that fits a page to a view by `fit`.
pub fn fit_zoom_to(view: Vec2, page: Vec2, fit: Fit) -> f32 {
    if page.x <= 0. || page.y <= 0. {
        return 1.;
    }
    match fit {
        Fit::Page => ((view.x - 2. * MARGIN) / page.x)
            .min((view.y - 2. * MARGIN) / page.y)
            .max(0.05),
        Fit::Width => ((view.x - 2. * MARGIN - BAR) / page.x).clamp(0.05, MAX_ZOOM),
    }
}

/// One axis of the stack in content coordinates (the stack's own, before it
/// is scrolled or centered): margin, pages of `len * z` with `GAP` between.
#[derive(Clone, Copy)]
struct Axis {
    len: f32,
    z: f32,
}

impl Axis {
    /// The page-pixel position of content position `c`. A position in a gap
    /// is the start of the page after it.
    fn to_doc(self, c: f32) -> f32 {
        let t = c - MARGIN;
        if t < 0. {
            return t / self.z;
        }
        let stride = self.len * self.z + GAP;
        let k = (t / stride).floor();
        let within = (t - k * stride).min(self.len * self.z);
        k * self.len + within / self.z
    }
    /// Content position of page-pixel position `d`.
    fn to_content(self, d: f32) -> f32 {
        if d < 0. {
            return MARGIN + d * self.z;
        }
        let k = (d / self.len).floor();
        MARGIN + k * (self.len * self.z + GAP) + (d - k * self.len) * self.z
    }
}

/// The stack of pages at one zoom.
#[derive(Clone, Copy)]
struct Geo {
    page: Vec2,
    count: usize,
    across: usize,
    z: f32,
}

impl Geo {
    fn cols(&self) -> usize {
        self.across.clamp(1, self.count.max(1))
    }
    fn rows(&self) -> usize {
        self.count.max(1).div_ceil(self.cols())
    }
    fn size(&self) -> Vec2 {
        self.page * self.z
    }
    fn content(&self) -> Vec2 {
        let (c, r) = (self.cols() as f32, self.rows() as f32);
        let s = self.size();
        Vec2::new(
            2. * MARGIN + c * s.x + (c - 1.) * GAP,
            2. * MARGIN + r * s.y + (r - 1.) * GAP,
        )
    }
    /// Content position of the top left of page `k`.
    fn cell(&self, k: usize) -> Vec2 {
        let (r, c) = (k / self.cols(), k % self.cols());
        let s = self.size();
        Vec2::new(
            MARGIN + c as f32 * (s.x + GAP),
            MARGIN + r as f32 * (s.y + GAP),
        )
    }
    fn axis(&self, a: usize) -> Axis {
        Axis {
            len: self.page[a],
            z: self.z,
        }
    }
    /// Which scroll bars the stack needs in `view`: (vertical, horizontal).
    fn bars(&self, view: Vec2) -> (bool, bool) {
        let c = self.content();
        let v = c.y > view.y + 0.5;
        let h = c.x > view.x - if v { BAR } else { 0. } + 0.5;
        (v || (h && c.y > view.y - BAR + 0.5), h)
    }
    /// The part of `view` the pages use, without the scroll bars.
    fn inner(&self, view: Vec2) -> Vec2 {
        let (v, h) = self.bars(view);
        view - Vec2::new(if v { BAR } else { 0. }, if h { BAR } else { 0. })
    }
    /// Room a stack smaller than the view leaves on each side.
    fn pad(&self, inner: Vec2) -> Vec2 {
        ((inner - self.content()) / 2.).max(Vec2::ZERO)
    }
    fn max_scroll(&self, inner: Vec2) -> Vec2 {
        (self.content() - inner).max(Vec2::ZERO)
    }
    /// The page at the top left of a view scrolled by `scroll`; at the end
    /// of the stack the last row, which cannot reach the top of the view.
    fn top_page(&self, inner: Vec2, scroll: Vec2, max: Vec2) -> usize {
        let pad = self.pad(inner);
        let at = |a: usize, n: usize| {
            let d = self.axis(a).to_doc(scroll[a] - pad[a]);
            ((d / self.page[a] + 1e-3).floor().max(0.) as usize).min(n - 1)
        };
        let mut row = at(1, self.rows());
        if max.y > 0. && scroll.y >= max.y - 0.5 {
            row = self.rows() - 1;
        }
        let col = at(0, self.cols());
        (row * self.cols() + col).min(self.count.saturating_sub(1))
    }
}

/// Wheel and zoom the viewer took at the start of the frame.
#[derive(Clone, Copy, Debug, Default)]
struct Taken {
    scroll: Vec2,
    zoom: f32,
}

/// Marks `rect` as covered by a viewer that takes the wheel: the next frame
/// keeps the wheel from everything painted under it.
pub fn cover(ctx: &egui::Context, rect: Rect) {
    let r = [rect.left(), rect.top(), rect.right(), rect.bottom()];
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(COVER), r));
}

/// At the start of a frame: when the pointer is over the covered rectangle,
/// moves the wheel and the zoom from the frame's input to the viewer.
pub fn take_wheel(ctx: &egui::Context) {
    let cover = ctx.data_mut(|d| {
        d.remove_temp::<Taken>(egui::Id::new(TAKEN));
        d.remove_temp::<[f32; 4]>(egui::Id::new(COVER))
    });
    let Some([l, t, r, b]) = cover else {
        return;
    };
    let cover = Rect::from_min_max(Pos2::new(l, t), Pos2::new(r, b));
    let over = ctx
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|p| cover.contains(p));
    if !over {
        return;
    }
    let (scroll, zoom) = ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
    ctx.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(TAKEN), Taken { scroll, zoom }));
}

/// The wheel a viewer took this frame, in screen pixels.
pub fn taken_scroll(ctx: &egui::Context) -> Option<Vec2> {
    ctx.data(|d| d.get_temp::<Taken>(egui::Id::new(TAKEN)))
        .map(|t| t.scroll)
}

/// Did a viewer take this frame's wheel and zoom? The chart under a preview
/// leaves them alone then.
pub fn wheel_taken(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<Taken>(egui::Id::new(TAKEN)).is_some())
}

/// Lays the pages out in `view`, applies this frame's input to the scroll
/// and the zoom, paints the scroll bars and returns the pages in view.
pub fn lay_out(ui: &mut egui::Ui, view: Rect, id: egui::Id, p: Params) -> Laid {
    let Params {
        page_size,
        count,
        across,
        page,
        zoom,
        nav,
        active,
        fit: fit_mode,
        wheel,
    } = p;
    let ctx = ui.ctx().clone();
    let fit = fit_zoom_to(view.size(), page_size, fit_mode);
    let current = zoom.unwrap_or(fit);
    let empty = |zoom: f32| Laid {
        zoom,
        cells: vec![],
        painter: ui.painter_at(view),
        moved: false,
    };
    if count == 0 || page_size.x <= 0. || page_size.y <= 0. {
        nav.zoom = 0.;
        return empty(current);
    }
    *page = (*page).min(count - 1);
    let taken = if active && wheel {
        ctx.data(|d| d.get_temp::<Taken>(egui::Id::new(TAKEN)))
    } else {
        None
    };
    let pointer = ctx.input(|i| i.pointer.hover_pos());
    // Ctrl+wheel: zoom about the pointer.
    let mut anchor = view.size() / 2.;
    if let Some(t) = taken.filter(|t| (t.zoom - 1.).abs() > 1e-4) {
        *zoom = Some((current * t.zoom).clamp(MIN_ZOOM, MAX_ZOOM));
        if let Some(pos) = pointer {
            anchor = pos - view.min;
        }
    }
    let z = zoom.unwrap_or(fit);
    let geo_at = |z: f32| Geo {
        page: page_size,
        count,
        across,
        z,
    };
    let geo = geo_at(z);
    let mut moved = false;
    if nav.zoom <= 0. {
        nav.goto = Some(*page);
    } else if (z - nav.zoom).abs() > 1e-6 {
        if zoom.is_none() {
            // Fit to the view again: back to the page being looked at.
            nav.goto = Some(*page);
        } else {
            // Keep what is under the anchor where it is.
            let old = geo_at(nav.zoom);
            let (old_inner, inner) = (old.inner(view.size()), geo.inner(view.size()));
            let (old_pad, pad) = (old.pad(old_inner), geo.pad(inner));
            for a in 0..2 {
                let at = nav.scroll[a] - old_pad[a] + anchor[a];
                let doc = old.axis(a).to_doc(at);
                nav.scroll[a] = geo.axis(a).to_content(doc) + pad[a] - anchor[a];
            }
            moved = true;
        }
    }
    nav.zoom = z;
    let resp = ui.interact(view, id, egui::Sense::drag());
    if active {
        if let Some(t) = taken.filter(|t| (t.zoom - 1.).abs() <= 1e-4 && t.scroll != Vec2::ZERO) {
            nav.scroll -= t.scroll * WHEEL_GAIN;
            moved = true;
        }
        if resp.dragged_by(egui::PointerButton::Primary) {
            nav.scroll -= resp.drag_delta();
            moved = true;
        }
        if resp.dragged() {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if resp.contains_pointer() {
            ctx.set_cursor_icon(egui::CursorIcon::Grab);
        }
        if !ctx.wants_keyboard_input() {
            let key = |k: Key| ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, k));
            if key(Key::PageDown) {
                nav.goto = Some((*page + 1).min(count - 1));
            }
            if key(Key::PageUp) {
                nav.goto = Some(page.saturating_sub(1));
            }
            if key(Key::Home) {
                nav.goto = Some(0);
            }
            if key(Key::End) {
                nav.goto = Some(count - 1);
            }
            for (k, step) in [
                (Key::ArrowDown, Vec2::new(0., ARROW_STEP)),
                (Key::ArrowUp, Vec2::new(0., -ARROW_STEP)),
                (Key::ArrowRight, Vec2::new(ARROW_STEP, 0.)),
                (Key::ArrowLeft, Vec2::new(-ARROW_STEP, 0.)),
            ] {
                if key(k) {
                    nav.scroll += step;
                    moved = true;
                }
            }
        }
    }
    if moved {
        nav.hold = None;
    }
    let inner = geo.inner(view.size());
    let max = geo.max_scroll(inner);
    if let Some(k) = nav.goto.take() {
        let k = k.min(count - 1);
        let cell = geo.cell(k);
        let (row, col) = (k / geo.cols(), k % geo.cols());
        nav.scroll.y = if row == 0 { 0. } else { cell.y - NEAR };
        if geo.cols() > 1 {
            nav.scroll.x = if col == 0 { 0. } else { cell.x - NEAR };
        }
        nav.hold = Some(k);
        *page = k;
    }
    if let Some((k, r)) = nav.goto_rect.take() {
        let at = geo.cell(k.min(count - 1)) + r.center().to_vec2() * z;
        nav.scroll.y = at.y - inner.y / 2.;
        if max.x > 0. {
            nav.scroll.x = at.x - inner.x / 2.;
        }
        nav.hold = None;
    }
    nav.scroll = nav.scroll.max(Vec2::ZERO).min(max);
    // Scroll bars.
    let (vbar, hbar) = geo.bars(view.size());
    let content = geo.content();
    let mut dragged = false;
    for (axis, shown) in [(1, vbar), (0, hbar)] {
        if !shown {
            continue;
        }
        let track = if axis == 1 {
            Rect::from_min_size(
                Pos2::new(view.right() - BAR, view.top()),
                Vec2::new(BAR, inner.y),
            )
        } else {
            Rect::from_min_size(
                Pos2::new(view.left(), view.bottom() - BAR),
                Vec2::new(inner.x, BAR),
            )
        };
        if scroll_bar(
            ui,
            id.with(("bar", axis)),
            track,
            axis,
            &mut nav.scroll[axis],
            max[axis],
            (inner[axis], content[axis]),
            &mut nav.grab,
        ) {
            dragged = true;
        }
    }
    if hbar && vbar {
        let corner = Rect::from_min_size(view.max - Vec2::splat(BAR), Vec2::splat(BAR));
        ui.painter().rect_filled(corner, 0., chrome::pal::FACE);
    }
    if dragged {
        nav.hold = None;
    }
    nav.scroll = nav.scroll.max(Vec2::ZERO).min(max);
    if nav.hold.is_none() {
        *page = geo.top_page(inner, nav.scroll, max);
    }
    // The pages in view.
    let eff = Rect::from_min_size(view.min, inner);
    let pad = geo.pad(inner);
    let size = geo.size();
    let stride = size + Vec2::splat(GAP);
    let origin = eff.min + pad - nav.scroll;
    let span = |a: usize, n: usize| {
        let lo = ((nav.scroll[a] - pad[a] - MARGIN) / stride[a])
            .floor()
            .max(0.) as usize;
        let hi = ((nav.scroll[a] - pad[a] + inner[a] - MARGIN) / stride[a]).floor();
        (lo, (hi.max(0.) as usize).min(n - 1))
    };
    let (r0, r1) = span(1, geo.rows());
    let (c0, c1) = span(0, geo.cols());
    let mut cells = vec![];
    for r in r0..=r1 {
        for c in c0..=c1 {
            let k = r * geo.cols() + c;
            if k >= count {
                continue;
            }
            let at = (origin + geo.cell(k)).round();
            // A page that only touches the view's edge is not in view.
            if Rect::from_min_size(at, size).intersects(eff.shrink(1.)) {
                cells.push((k, at));
            }
        }
    }
    #[cfg(test)]
    {
        nav.shown = cells.clone();
    }
    Laid {
        zoom: z,
        cells,
        painter: ui.painter_at(eff),
        moved: moved || dragged,
    }
}

/// One scroll bar: a track with a thumb that is dragged, and a click on the
/// track scrolls a view length. True when the thumb moved the scroll.
#[allow(clippy::too_many_arguments)]
fn scroll_bar(
    ui: &mut egui::Ui,
    id: egui::Id,
    track: Rect,
    axis: usize,
    scroll: &mut f32,
    max: f32,
    (view_len, content_len): (f32, f32),
    grab: &mut Option<(usize, f32)>,
) -> bool {
    use chrome::pal;
    let track_len = track.size()[axis];
    let thumb_len = (track_len * view_len / content_len.max(1.))
        .clamp(20., track_len)
        .min(track_len);
    let travel = (track_len - thumb_len).max(0.);
    let thumb_at = if max > 0. { *scroll / max * travel } else { 0. };
    let thumb = |at: f32| {
        let (lo, hi) = (track.min[axis] + at, track.min[axis] + at + thumb_len);
        if axis == 1 {
            Rect::from_min_max(Pos2::new(track.left(), lo), Pos2::new(track.right(), hi))
        } else {
            Rect::from_min_max(Pos2::new(lo, track.top()), Pos2::new(hi, track.bottom()))
        }
    };
    let resp = ui.interact(track, id, egui::Sense::click_and_drag());
    let mut moved = false;
    if let Some(pos) = resp.interact_pointer_pos() {
        let along = pos[axis] - track.min[axis];
        if resp.drag_started() {
            let inside = along >= thumb_at && along <= thumb_at + thumb_len;
            *grab = Some((
                axis,
                if inside {
                    along - thumb_at
                } else {
                    thumb_len / 2.
                },
            ));
        }
        if resp.dragged() {
            if let Some((a, offset)) = *grab {
                if a == axis && travel > 0. {
                    *scroll = ((along - offset) / travel).clamp(0., 1.) * max;
                    moved = true;
                }
            }
        } else if resp.clicked() && (along < thumb_at || along > thumb_at + thumb_len) {
            let down = along > thumb_at + thumb_len;
            *scroll += if down { view_len } else { -view_len };
            moved = true;
        }
    }
    if !resp.dragged() && grab.is_some_and(|(a, _)| a == axis) {
        *grab = None;
    }
    let at = if moved && max > 0. {
        scroll.clamp(0., max) / max * travel
    } else {
        thumb_at
    };
    let p = ui.painter();
    p.rect_filled(track, 0., pal::LIGHT);
    let t = thumb(at).shrink(1.);
    let fill = if resp.dragged() { pal::GRIP } else { pal::FACE };
    p.rect_filled(t, 0., fill);
    p.rect_stroke(
        t,
        0.,
        Stroke::new(1_f32, pal::SHADOW),
        egui::StrokeKind::Inside,
    );
    moved
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geo(count: usize, across: usize, z: f32) -> Geo {
        Geo {
            page: Vec2::new(800., 1000.),
            count,
            across,
            z,
        }
    }

    #[test]
    fn positions_round_trip_through_the_page_pixels() {
        for z in [0.1, 0.5, 1., 3.] {
            let a = geo(5, 1, z).axis(1);
            for page in 0..5 {
                for off in [0., 1., 499., 999.] {
                    let d = page as f32 * 1000. + off;
                    let back = a.to_doc(a.to_content(d));
                    assert!((back - d).abs() < 0.05, "z {z}: {d} came back {back}");
                }
            }
            // A position in the gap is the start of the next page.
            assert_eq!(a.to_doc(MARGIN + 1000. * z + GAP / 2.).round(), 1000.);
            assert!((a.to_doc(MARGIN / 2.) - (-MARGIN / 2. / z)).abs() < 1e-3);
        }
    }

    #[test]
    fn the_top_page_is_the_one_at_the_top_of_the_view() {
        let g = geo(5, 1, 1.);
        let inner = Vec2::new(1000., 700.);
        let max = g.max_scroll(inner);
        assert_eq!(g.top_page(inner, Vec2::ZERO, max), 0);
        let stride = 1000. + GAP;
        // Page k's top is at MARGIN + k * stride; a view starting just above it
        // is on page k, and one starting a little into page k-1 is on k-1.
        for k in 1..5 {
            let top = MARGIN + k as f32 * stride;
            let at = |dy: f32| g.top_page(inner, Vec2::new(0., (top + dy).min(max.y - 1.)), max);
            if top + 10. < max.y - 1. {
                assert_eq!(at(-NEAR), k);
                assert_eq!(at(10.), k);
                assert_eq!(at(-GAP - 10.), k - 1);
            }
        }
        // The last pages never reach the top of the view; at the end the
        // last page is the current one.
        assert_eq!(g.top_page(inner, max, max), 4);
    }

    #[test]
    fn a_grid_of_pages_is_rows_of_the_pages_across() {
        let g = geo(7, 3, 0.5);
        assert_eq!((g.cols(), g.rows()), (3, 3));
        let (a, b, c) = (g.cell(0), g.cell(1), g.cell(3));
        assert_eq!(a.y, b.y);
        assert!(b.x > a.x && c.y > a.y && c.x == a.x);
        let content = g.content();
        assert!(content.x > 3. * 400. && content.y > 3. * 500.);
        let inner = Vec2::new(2000., 2000.);
        assert_eq!(g.max_scroll(inner), Vec2::ZERO);
        assert!(g.pad(inner).x > 0.);
        assert_eq!(g.bars(Vec2::new(300., 300.)), (true, true));
        assert_eq!(g.bars(Vec2::new(2000., 2000.)), (false, false));
    }

    #[test]
    fn fit_leaves_the_margin_and_never_needs_a_bar() {
        let view = Vec2::new(1500., 780.);
        let page = Vec2::new(1056., 816.);
        let z = fit_zoom(view, page);
        let g = Geo {
            page,
            count: 4,
            across: 1,
            z,
        };
        // One page fits; the stack of four does not, and bars show then.
        let one = Geo { count: 1, ..g };
        assert_eq!(one.bars(view), (false, false));
        assert!(g.bars(view).0);
    }
}
