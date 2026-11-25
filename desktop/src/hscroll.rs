//! Horizontal scroll bars under the activity table and under the Gantt
//! Chart.
//!
//! The look follows the overlay style: a 16 px bar on the face color. While
//! the pointer is away it shows a 4 px track with a 4 px thumb of at least
//! 32 px and faint arrows at each end, so it still reads as a scroll bar; a
//! full thumb with arrow buttons shows while the pointer is over the bar or
//! the thumb is being dragged. The bar moves nothing itself: `show`
//! reports what the user asked for (`Move`) and the caller applies it to the
//! table's pixel offset or the chart's first visible minute.
use super::*;
use crate::chrome::pal;
use crate::timescale;

/// Height of a bar and of the strip that holds the bars of a pane.
pub(crate) const BAR_HEIGHT: f32 = 16.;
/// The thumb track starts this far in from each end of the bar (an arrow
/// button and a pixel).
const TRACK_INSET: f32 = 17.;
const THUMB_MIN: f32 = 32.;
/// The idle bar's track and arrows.
const TRACK_IDLE: Color32 = Color32::from_gray(222);
const ARROW_IDLE: Color32 = Color32::from_gray(165);
const THUMB: Color32 = Color32::from_gray(133);
const THUMB_HOT: Color32 = Color32::from_gray(112);
const THUMB_DOWN: Color32 = Color32::from_gray(96);
const BUTTON_HOT: Color32 = Color32::from_gray(218);
const BUTTON_DOWN: Color32 = Color32::from_gray(166);
/// Held arrow buttons and track pieces repeat after this delay, then every
/// `REPEAT` seconds.
const REPEAT_DELAY: f64 = 0.4;
const REPEAT: f64 = 0.06;

/// What a bar stands for, in the caller's units (pixels for the table,
/// minutes for the chart): the whole scrollable extent, the part in view
/// and where the view starts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Range {
    pub total: f64,
    pub view: f64,
    pub pos: f64,
}

impl Range {
    /// The largest start of the view.
    pub fn max(&self) -> f64 {
        (self.total - self.view).max(0.)
    }

    pub fn scrollable(&self) -> bool {
        self.total > self.view + 0.5
    }
}

/// A request from the bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Move {
    /// Put the view's start here (a thumb drag).
    To(f64),
    /// One line (column or timescale unit) toward the end (+1) or the
    /// start (-1): an arrow button.
    Line(i32),
    /// One view toward the end or the start: a click on the track.
    Page(i32),
}

/// Where the parts of a bar are.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Geometry {
    pub bar: Rect,
    pub left_button: Rect,
    pub right_button: Rect,
    /// The track the thumb runs in.
    pub track: Rect,
    /// The thumb at the full height of the bar (what a pointer hits).
    pub thumb: Rect,
}

/// The parts of `bar` for `range`.
pub(crate) fn geometry(bar: Rect, range: &Range) -> Geometry {
    let left_button = Rect::from_min_size(bar.min, Vec2::new(BAR_HEIGHT, bar.height()));
    let right_button = Rect::from_min_max(Pos2::new(bar.right() - BAR_HEIGHT, bar.top()), bar.max);
    let track = Rect::from_min_max(
        Pos2::new(bar.left() + TRACK_INSET, bar.top()),
        Pos2::new(
            (bar.right() - TRACK_INSET).max(bar.left() + TRACK_INSET),
            bar.bottom(),
        ),
    );
    let length = if range.total > 0. {
        ((track.width() as f64 * range.view / range.total) as f32)
            .max(THUMB_MIN)
            .min(track.width())
    } else {
        track.width()
    };
    let travel = (track.width() - length).max(0.);
    let at = if range.max() > 0. {
        (range.pos / range.max()).clamp(0., 1.) as f32
    } else {
        0.
    };
    let left = (track.left() + travel * at).round();
    Geometry {
        bar,
        left_button,
        right_button,
        track,
        thumb: Rect::from_min_size(
            Pos2::new(left, bar.top()),
            Vec2::new(length.round(), bar.height()),
        ),
    }
}

/// The start of the view that puts the thumb's left edge at `left`.
pub(crate) fn pos_for_thumb(geo: &Geometry, range: &Range, left: f32) -> f64 {
    let travel = (geo.track.width() - geo.thumb.width()).max(0.);
    if travel <= 0. {
        return 0.;
    }
    let at = ((left - geo.track.left()) / travel).clamp(0., 1.) as f64;
    at * range.max()
}

/// A press held on a button or the track: true at the press and then again
/// at each repeat.
fn held(ui: &egui::Ui, response: &egui::Response) -> bool {
    let key = response.id.with("repeat");
    if !response.is_pointer_button_down_on() {
        ui.data_mut(|d| d.remove_temp::<f64>(key));
        return false;
    }
    let now = ui.input(|i| i.time);
    match ui.data(|d| d.get_temp::<f64>(key)) {
        None => {
            ui.data_mut(|d| d.insert_temp(key, now + REPEAT_DELAY));
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(REPEAT_DELAY));
            true
        }
        Some(due) if now >= due => {
            ui.data_mut(|d| d.insert_temp(key, now + REPEAT));
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(REPEAT));
            true
        }
        Some(due) => {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(due - now));
            false
        }
    }
}

/// How a bar looks this frame, kept between its handling and its painting.
#[derive(Clone, Copy, Default)]
struct Look {
    /// The pointer is over the bar or the thumb is being dragged.
    expanded: bool,
    thumb: Press,
    left: Press,
    right: Press,
}

#[derive(Clone, Copy, Default, PartialEq)]
enum Press {
    #[default]
    Idle,
    Hot,
    Down,
}

impl Press {
    fn of(response: &egui::Response) -> Self {
        if response.is_pointer_button_down_on() || response.dragged() {
            Self::Down
        } else if response.hovered() {
            Self::Hot
        } else {
            Self::Idle
        }
    }
}

/// Handles one bar: what the user asked for this frame, if anything. The
/// bar is drawn later by `paint`, after whatever is drawn over its strip.
pub(crate) fn interact(ui: &mut egui::Ui, id: egui::Id, bar: Rect, range: Range) -> Option<Move> {
    let look = id.with("look");
    if !range.scrollable() || bar.width() < 2. * TRACK_INSET + THUMB_MIN {
        ui.data_mut(|d| d.insert_temp(look, Look::default()));
        return None;
    }
    let geo = geometry(bar, &range);
    let mut request = None;
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let thumb = ui.interact(geo.thumb, id.with("thumb"), Sense::drag());
    let before = Rect::from_min_max(
        Pos2::new(geo.left_button.right(), bar.top()),
        Pos2::new(geo.thumb.left(), bar.bottom()),
    );
    let after = Rect::from_min_max(
        Pos2::new(geo.thumb.right(), bar.top()),
        Pos2::new(geo.right_button.left(), bar.bottom()),
    );
    let left = ui.interact(geo.left_button, id.with("left"), Sense::click());
    let right = ui.interact(geo.right_button, id.with("right"), Sense::click());
    let page_before = ui.interact(before, id.with("before"), Sense::click());
    let page_after = ui.interact(after, id.with("after"), Sense::click());
    // The thumb follows the pointer from where it was grabbed.
    let grab = id.with("grab");
    if thumb.drag_started() {
        let origin = ui.input(|i| i.pointer.press_origin());
        if let Some(origin) = origin {
            ui.data_mut(|d| d.insert_temp(grab, origin.x - geo.thumb.left()));
        }
    }
    if thumb.dragged() {
        if let (Some(pointer), Some(offset)) = (pointer, ui.data(|d| d.get_temp::<f32>(grab))) {
            request = Some(Move::To(pos_for_thumb(&geo, &range, pointer.x - offset)));
        }
    }
    if request.is_none() {
        if held(ui, &left) {
            request = Some(Move::Line(-1));
        } else if held(ui, &right) {
            request = Some(Move::Line(1));
        } else if held(ui, &page_before) && pointer.is_some_and(|p| before.contains(p)) {
            request = Some(Move::Page(-1));
        } else if held(ui, &page_after) && pointer.is_some_and(|p| after.contains(p)) {
            request = Some(Move::Page(1));
        }
    }
    let over = ui
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|p| bar.contains(p));
    ui.data_mut(|d| {
        d.insert_temp(
            look,
            Look {
                expanded: over || thumb.dragged(),
                thumb: Press::of(&thumb),
                left: Press::of(&left),
                right: Press::of(&right),
            },
        )
    });
    request
}

/// Draws one bar for `range` as `interact` found it. Returns where its parts
/// are.
pub(crate) fn paint(ui: &egui::Ui, id: egui::Id, bar: Rect, range: Range) -> Geometry {
    let geo = geometry(bar, &range);
    let painter = ui.painter().with_clip_rect(bar);
    chrome::fill(&painter, bar, pal::FACE);
    if !range.scrollable() || bar.width() < 2. * TRACK_INSET + THUMB_MIN {
        return geo;
    }
    let look = ui
        .data(|d| d.get_temp::<Look>(id.with("look")))
        .unwrap_or_default();
    let mid = bar.center().y.floor();
    let t = geo.thumb;
    if look.expanded {
        let color = match look.thumb {
            Press::Down => THUMB_DOWN,
            Press::Hot => THUMB_HOT,
            Press::Idle => THUMB,
        };
        // A 6 px pill: the first and last rows are a pixel shorter.
        chrome::fill(
            &painter,
            Rect::from_min_max(
                Pos2::new(t.left(), mid - 2.),
                Pos2::new(t.right(), mid + 2.),
            ),
            color,
        );
        for y in [mid - 3., mid + 2.] {
            chrome::fill(
                &painter,
                Rect::from_min_max(
                    Pos2::new(t.left() + 1., y),
                    Pos2::new(t.right() - 1., y + 1.),
                ),
                color,
            );
        }
        for (button, press, dir) in [
            (geo.left_button, look.left, -1.),
            (geo.right_button, look.right, 1.),
        ] {
            let ink = match press {
                Press::Down => {
                    chrome::fill(&painter, button, BUTTON_DOWN);
                    Color32::BLACK
                }
                Press::Hot => {
                    chrome::fill(&painter, button, BUTTON_HOT);
                    THUMB_DOWN
                }
                Press::Idle => THUMB,
            };
            arrow(&painter, button, dir, ink);
        }
    } else {
        let track = geo.track;
        chrome::fill(
            &painter,
            Rect::from_min_max(
                Pos2::new(track.left(), mid - 2.),
                Pos2::new(track.right(), mid + 2.),
            ),
            TRACK_IDLE,
        );
        chrome::fill(
            &painter,
            Rect::from_min_max(
                Pos2::new(t.left(), mid - 2.),
                Pos2::new(t.right(), mid + 2.),
            ),
            THUMB,
        );
        arrow(&painter, geo.left_button, -1., ARROW_IDLE);
        arrow(&painter, geo.right_button, 1., ARROW_IDLE);
    }
    geo
}

/// Where the bars of one pane are.
pub(crate) struct Bars {
    pub table: Rect,
    /// The Gantt's bar, when the chart is shown.
    pub gantt: Option<Rect>,
    /// Width the table's columns are shown in.
    pub view: f32,
    /// Width of the timescale in pixels.
    pub chart: f32,
}

impl Bars {
    /// The bars in the strip along the bottom of `pane`: the table's under
    /// the table (`table_w` wide) and, with the chart shown, the Gantt's
    /// under the chart (`chart` wide), the Gantt's short of the vertical
    /// scroll bar at the right.
    pub fn new(pane: Rect, table_w: f32, view: f32, chart: f32, chart_on: bool) -> Self {
        let top = (pane.bottom() - BAR_HEIGHT).round();
        let strip = |left: f32, right: f32| {
            Rect::from_min_max(
                Pos2::new(left, top),
                Pos2::new(right.max(left), top + BAR_HEIGHT),
            )
        };
        let table_right = if chart_on {
            pane.left() + table_w
        } else {
            pane.right() - BAR_HEIGHT
        };
        Self {
            table: strip(pane.left(), table_right),
            gantt: chart_on.then(|| {
                strip(
                    pane.left() + table_w + crate::activities::SPLITTER,
                    pane.right() - BAR_HEIGHT,
                )
            }),
            view,
            chart,
        }
    }
}

/// A solid triangle 6 px wide and 8 px tall pointing left (`dir` -1) or
/// right, centered in `button`.
fn arrow(painter: &egui::Painter, button: Rect, dir: f32, ink: Color32) {
    let c = Pos2::new(button.center().x.round(), button.center().y.round());
    let (tip, base) = (c.x + 3. * dir, c.x - 3. * dir);
    painter.add(egui::Shape::convex_polygon(
        vec![
            Pos2::new(tip, c.y),
            Pos2::new(base, c.y - 4.),
            Pos2::new(base, c.y + 4.),
        ],
        ink,
        Stroke::NONE,
    ));
}

/// A solid triangle 8 px wide and 6 px tall pointing up (`dir` -1) or
/// down, centered in `button`.
fn varrow(painter: &egui::Painter, button: Rect, dir: f32, ink: Color32) {
    let c = Pos2::new(button.center().x.round(), button.center().y.round());
    let (tip, base) = (c.y + 3. * dir, c.y - 3. * dir);
    painter.add(egui::Shape::convex_polygon(
        vec![
            Pos2::new(c.x, tip),
            Pos2::new(c.x - 4., base),
            Pos2::new(c.x + 4., base),
        ],
        ink,
        Stroke::NONE,
    ));
}

/// Where the parts of a vertical bar are.
#[derive(Clone, Copy, Debug)]
pub(crate) struct VGeometry {
    pub bar: Rect,
    pub up_button: Rect,
    pub down_button: Rect,
    pub track: Rect,
    pub thumb: Rect,
}

/// The parts of a vertical `bar` for `range`.
pub(crate) fn vgeometry(bar: Rect, range: &Range) -> VGeometry {
    let up_button = Rect::from_min_size(bar.min, Vec2::new(bar.width(), BAR_HEIGHT));
    let down_button = Rect::from_min_max(Pos2::new(bar.left(), bar.bottom() - BAR_HEIGHT), bar.max);
    let track = Rect::from_min_max(
        Pos2::new(bar.left(), bar.top() + TRACK_INSET),
        Pos2::new(
            bar.right(),
            (bar.bottom() - TRACK_INSET).max(bar.top() + TRACK_INSET),
        ),
    );
    let length = if range.total > 0. {
        ((track.height() as f64 * range.view / range.total) as f32)
            .max(THUMB_MIN)
            .min(track.height())
    } else {
        track.height()
    };
    let travel = (track.height() - length).max(0.);
    let at = if range.max() > 0. {
        (range.pos / range.max()).clamp(0., 1.) as f32
    } else {
        0.
    };
    let top = (track.top() + travel * at).round();
    VGeometry {
        bar,
        up_button,
        down_button,
        track,
        thumb: Rect::from_min_size(
            Pos2::new(bar.left(), top),
            Vec2::new(bar.width(), length.round()),
        ),
    }
}

/// The vertical start of the view that puts the thumb's top edge at `top`.
pub(crate) fn pos_for_vthumb(geo: &VGeometry, range: &Range, top: f32) -> f64 {
    let travel = (geo.track.height() - geo.thumb.height()).max(0.);
    if travel <= 0. {
        return 0.;
    }
    let at = ((top - geo.track.top()) / travel).clamp(0., 1.) as f64;
    at * range.max()
}

/// Handles one vertical bar: what the user asked for this frame, if anything.
/// Call before the scrolled content so drags take effect on the same frame,
/// then call [`paint_vertical`] after the content so nothing paints over it.
pub(crate) fn interact_vertical(
    ui: &mut egui::Ui,
    id: egui::Id,
    bar: Rect,
    range: Range,
) -> Option<Move> {
    let look = id.with("vlook");
    if !range.scrollable() || bar.height() < 2. * TRACK_INSET + THUMB_MIN {
        ui.data_mut(|d| d.insert_temp(look, Look::default()));
        return None;
    }
    let geo = vgeometry(bar, &range);
    let mut request = None;
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let thumb = ui.interact(geo.thumb, id.with("vthumb"), Sense::drag());
    let before = Rect::from_min_max(
        Pos2::new(bar.left(), geo.up_button.bottom()),
        Pos2::new(bar.right(), geo.thumb.top()),
    );
    let after = Rect::from_min_max(
        Pos2::new(bar.left(), geo.thumb.bottom()),
        Pos2::new(bar.right(), geo.down_button.top()),
    );
    let up = ui.interact(geo.up_button, id.with("vup"), Sense::click());
    let down = ui.interact(geo.down_button, id.with("vdown"), Sense::click());
    let page_before = ui.interact(before, id.with("vbefore"), Sense::click());
    let page_after = ui.interact(after, id.with("vafter"), Sense::click());
    let grab = id.with("vgrab");
    if thumb.drag_started() {
        if let Some(origin) = ui.input(|i| i.pointer.press_origin()) {
            ui.data_mut(|d| d.insert_temp(grab, origin.y - geo.thumb.top()));
        }
    }
    if thumb.dragged() {
        if let (Some(pointer), Some(offset)) = (pointer, ui.data(|d| d.get_temp::<f32>(grab))) {
            request = Some(Move::To(pos_for_vthumb(&geo, &range, pointer.y - offset)));
        }
    }
    if request.is_none() {
        if held(ui, &up) {
            request = Some(Move::Line(-1));
        } else if held(ui, &down) {
            request = Some(Move::Line(1));
        } else if held(ui, &page_before) && pointer.is_some_and(|p| before.contains(p)) {
            request = Some(Move::Page(-1));
        } else if held(ui, &page_after) && pointer.is_some_and(|p| after.contains(p)) {
            request = Some(Move::Page(1));
        }
    }
    let over = ui
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|p| bar.contains(p));
    ui.data_mut(|d| {
        d.insert_temp(
            look,
            Look {
                expanded: over || thumb.dragged(),
                thumb: Press::of(&thumb),
                left: Press::of(&up),
                right: Press::of(&down),
            },
        )
    });
    request
}

/// Draws one persistent vertical scrollbar for `range`, matching OpenXER's
/// shared scrollbar palette and geometry so the track, thumb and arrow buttons
/// remain clearly visible even when the pointer is away.
pub(crate) fn paint_vertical(ui: &egui::Ui, id: egui::Id, bar: Rect, range: Range) -> VGeometry {
    let geo = vgeometry(bar, &range);
    let painter = ui.painter().with_clip_rect(bar);
    chrome::fill(&painter, bar, pal::FACE);
    // Crisp 1 px left divider separates scrolled content from the bar track.
    chrome::fill(
        &painter,
        Rect::from_min_max(bar.min, Pos2::new(bar.left() + 1., bar.bottom())),
        Color32::from_gray(208),
    );
    let scrollable = range.scrollable() && bar.height() >= 2. * TRACK_INSET + THUMB_MIN;
    let look = if scrollable {
        ui.data(|d| d.get_temp::<Look>(id.with("vlook")))
            .unwrap_or_default()
    } else {
        Look::default()
    };
    let mid = bar.center().x.floor();
    // Persistent track lane so the scrollbar is always unmistakable.
    if geo.track.height() > 2. {
        chrome::fill(
            &painter,
            Rect::from_min_max(
                Pos2::new(mid - 2., geo.track.top()),
                Pos2::new(mid + 3., geo.track.bottom()),
            ),
            TRACK_IDLE,
        );
    }
    for (button, press, dir) in [
        (geo.up_button, look.left, -1.),
        (geo.down_button, look.right, 1.),
    ] {
        let ink = if !scrollable {
            ARROW_IDLE
        } else {
            match press {
                Press::Down => {
                    chrome::fill(&painter, button, BUTTON_DOWN);
                    Color32::BLACK
                }
                Press::Hot => {
                    chrome::fill(&painter, button, BUTTON_HOT);
                    THUMB_DOWN
                }
                Press::Idle => THUMB,
            }
        };
        varrow(&painter, button, dir, ink);
    }
    if scrollable {
        let color = match look.thumb {
            Press::Down => THUMB_DOWN,
            Press::Hot => THUMB_HOT,
            Press::Idle => THUMB,
        };
        let t = geo.thumb;
        // Persistent 8 px pill thumb (widens cleanly on hover/drag with higher contrast).
        let (half_l, half_r) = if look.expanded { (4., 4.) } else { (3., 4.) };
        chrome::fill(
            &painter,
            Rect::from_min_max(
                Pos2::new(mid - half_l, t.top() + 1.),
                Pos2::new(mid + half_r, t.bottom() - 1.),
            ),
            color,
        );
        for y in [t.top(), t.bottom() - 1.] {
            chrome::fill(
                &painter,
                Rect::from_min_max(
                    Pos2::new(mid - half_l + 1., y),
                    Pos2::new(mid + half_r - 1., y + 1.),
                ),
                color,
            );
        }
    }
    geo
}

impl App {
    /// The column that starts the next screenful of columns toward `dir`:
    /// the first column edge past the offset (+1) or the last one before it.
    fn column_step(edges: &[f32], pos: f64, dir: i32) -> f64 {
        let pos = pos as f32;
        let edge = if dir > 0 {
            edges.iter().copied().find(|&e| e > pos + 0.5)
        } else {
            edges.iter().rev().copied().find(|&e| e < pos - 0.5)
        };
        f64::from(edge.unwrap_or(if dir > 0 { f32::MAX } else { 0. }))
    }

    /// The range the table's bar stands for.
    fn table_range(&self, view: f32) -> Range {
        Range {
            total: f64::from(self.table_content_width()),
            view: f64::from(view),
            pos: f64::from(self.grid.scroll_x),
        }
    }

    /// Make the parts of a bar known to the headless drivers (only when it
    /// has something to scroll).
    fn register_bar(&mut self, name: &str, geo: &Geometry, range: &Range) {
        if !range.scrollable() {
            return;
        }
        for (suffix, rect) in [
            ("", geo.bar),
            ("_thumb", geo.thumb),
            ("_left", geo.left_button),
            ("_right", geo.right_button),
        ] {
            self.action_rects.insert(
                format!("{name}{suffix}"),
                [rect.left(), rect.top(), rect.right(), rect.bottom()],
            );
        }
    }

    /// Make the parts of a vertical bar known to headless drivers.
    pub(crate) fn register_vbar(&mut self, name: &str, geo: &VGeometry, range: &Range) {
        self.action_rects.insert(
            name.to_string(),
            [
                geo.bar.left(),
                geo.bar.top(),
                geo.bar.right(),
                geo.bar.bottom(),
            ],
        );
        if !range.scrollable() {
            return;
        }
        for (suffix, rect) in [
            ("_thumb", geo.thumb),
            ("_up", geo.up_button),
            ("_down", geo.down_button),
        ] {
            self.action_rects.insert(
                format!("{name}{suffix}"),
                [rect.left(), rect.top(), rect.right(), rect.bottom()],
            );
        }
    }

    /// Handle the pane's bars and apply what the user asked for. Done before
    /// the header and rows are drawn, so a drag shows in the same frame.
    pub(crate) fn hscroll_interact(&mut self, ui: &mut egui::Ui, bars: &Bars, bottom: bool) {
        let range = self.table_range(bars.view);
        let id = ui.id().with(("table_hscroll", bottom));
        if let Some(request) = interact(ui, id, bars.table, range) {
            let pos = match request {
                Move::To(pos) => pos,
                Move::Page(dir) => range.pos + f64::from(dir) * range.view,
                Move::Line(dir) => Self::column_step(&self.column_edges(), range.pos, dir),
            };
            self.grid.scroll_x = pos.clamp(0., range.max()) as f32;
        }
        let Some(gantt) = bars.gantt else {
            return;
        };
        let (low, range) = self.gantt_scroll_range(bars.chart);
        let id = ui.id().with(("gantt_hscroll", bottom));
        let Some(request) = interact(ui, id, gantt, range) else {
            return;
        };
        let origin = self.timeline_origin;
        let next = crate::timeline::moved(origin as f64, low, &range, request, |dir| {
            self.timescale_unit_step(dir) as f64
        });
        self.timeline_origin = next.round() as i64;
        self.panes.timeline_focus = if bottom {
            crate::timeline::Focus::BottomGantt
        } else {
            crate::timeline::Focus::Activities
        };
    }

    /// Draw the pane's bars, over everything else in their strip.
    pub(crate) fn hscroll_paint(&mut self, ui: &egui::Ui, bars: &Bars, bottom: bool) {
        let range = self.table_range(bars.view);
        let id = ui.id().with(("table_hscroll", bottom));
        let geo = paint(ui, id, bars.table, range);
        let name = if bottom {
            "bottom_hscroll_table"
        } else {
            "hscroll_table"
        };
        self.register_bar(name, &geo, &range);
        let Some(gantt) = bars.gantt else {
            return;
        };
        let (_, range) = self.gantt_scroll_range(bars.chart);
        let id = ui.id().with(("gantt_hscroll", bottom));
        let geo = paint(ui, id, gantt, range);
        let name = if bottom {
            "bottom_hscroll_gantt"
        } else {
            "hscroll_gantt"
        };
        self.register_bar(name, &geo, &range);
    }

    /// First and last displayed minute of the project's activities, kept
    /// while the project stays one undo snapshot.
    pub(crate) fn gantt_data_extent(&self) -> (i64, i64) {
        let leveled = self.shown().model.options.do_level;
        let key = (leveled, self.shown().model.project.proj_id);
        *self.memo(self.project_serial(), "gantt_extent", key, || {
            let m = &self.shown().model;
            let start = m
                .activities
                .iter()
                .filter_map(|a| displayed_start(a, leveled))
                .min();
            let finish = m
                .activities
                .iter()
                .filter_map(|a| displayed_finish(a, leveled))
                .max();
            let at = m.project.data_date.unwrap_or(0);
            let start = start.or(finish).unwrap_or(at);
            (start, finish.unwrap_or(start + 60 * DAY).max(start))
        })
    }

    /// What the chart's bar scrolls over, in minutes from the model origin:
    /// the project with half a view beyond each end, so there is always
    /// room to move the project across the chart.
    pub(crate) fn gantt_scroll_range(&self, chart_width: f32) -> (f64, Range) {
        let view =
            f64::from(chart_width.max(0.)) / f64::from(self.pixels_per_day.max(1e-3)) * DAY as f64;
        crate::timeline::scroll_range(self.timeline_origin as f64, view, self.gantt_data_extent())
    }

    /// The start of the timescale's lowest-band cell after the first visible
    /// minute (`dir` 1) or of the one before it (-1).
    fn timescale_unit_step(&self, dir: i32) -> i64 {
        let unit = self
            .timescale_bands()
            .last()
            .map_or(timescale::Unit::Day, |b| b.unit);
        crate::timeline::unit_step(
            self.timeline_origin as f64,
            dir,
            unit,
            self.shown().model.origin_serial,
            &self.label_options(),
        ) as i64
    }

    /// Mouse wheel over the timescale header or the bar strip: a sideways
    /// wheel, or the wheel with Shift (which egui reports as sideways), pans
    /// the chart. The chart body has its own handling.
    pub(crate) fn gantt_wheel_pan(&mut self, ctx: &egui::Context, over: &[Rect]) {
        if let Some(dx) = crate::timeline::wheel_pan(ctx, over) {
            self.timeline_origin -=
                (dx / self.pixels_per_day.max(1e-3) * DAY as f32).round() as i64;
            self.touch_gantt_timeline();
        }
    }
}
