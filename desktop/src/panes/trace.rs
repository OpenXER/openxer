//! Trace Logic: the selected activity's predecessors and successors as a
//! box diagram in the bottom pane.
//!
//! Rules:
//! - The trace holds the focus activity, its predecessors up to the
//!   configured number of predecessor levels (one level per relationship
//!   step) and its successors up to the successor levels.
//! - Columns are the longest-path rank inside the traced set, so a chain
//!   reads left to right one box per column even when a relationship skips
//!   a level; activities of the same rank stack in rows.
//! - Boxes are 160 x 78 on a 240 px column pitch, the first 4 px right and
//!   8 px below the pane's top-left corner.
//! - A finish-to-start relationship between neighbouring columns on one row
//!   is a straight line between the box sides at mid height. Every other
//!   relationship leaves the predecessor's bottom corner (start side left,
//!   finish side right) on a 2:1 diagonal down to a route 20 px below the
//!   boxes, runs along it and rises into the successor's bottom corner with
//!   an upward arrowhead. Arrowheads are 10 px long and 11 px across.
use super::boxes::{self, Template, BOX_H, BOX_W};
use super::network::{MAX_ZOOM, MIN_ZOOM};
use super::*;
use crate::context_menu::{ContextMenu, Menu};
use std::collections::{BTreeMap, VecDeque};

pub const COLUMN_PITCH: f32 = 240.;
/// Rows of one rank: the box, its route and a gap.
pub const ROW_PITCH: f32 = 120.;
pub const ORIGIN: Vec2 = Vec2::new(4., 8.);
pub const ROUTE_DROP: f32 = 20.;
pub const DIAGONAL: f32 = 40.;
pub const MAX_LEVELS: u32 = 2000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TraceOptions {
    pub predecessor_levels: u32,
    pub successor_levels: u32,
}

impl Default for TraceOptions {
    /// Three levels of predecessors and successors.
    fn default() -> Self {
        Self {
            predecessor_levels: 3,
            successor_levels: 3,
        }
    }
}

impl TraceOptions {
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "predecessor_levels": self.predecessor_levels,
            "successor_levels": self.successor_levels,
        })
    }
    pub fn restore(&mut self, v: &serde_json::Value) {
        for (key, field) in [
            ("predecessor_levels", &mut self.predecessor_levels),
            ("successor_levels", &mut self.successor_levels),
        ] {
            if let Some(n) = v[key].as_u64() {
                *field = (n as u32).min(MAX_LEVELS);
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Predecessor,
    Focus,
    Successor,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TraceNode {
    pub activity: usize,
    pub side: Side,
    /// Relationship steps from the focus.
    pub level: u32,
    pub column: usize,
    pub row: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TraceGraph {
    pub nodes: Vec<TraceNode>,
    /// Indexes into the model's links joining two traced activities.
    pub links: Vec<usize>,
}

impl TraceGraph {
    pub fn node(&self, activity: usize) -> Option<&TraceNode> {
        self.nodes.iter().find(|n| n.activity == activity)
    }
    pub fn columns(&self) -> usize {
        self.nodes.iter().map(|n| n.column + 1).max().unwrap_or(0)
    }
    pub fn rows(&self) -> usize {
        self.nodes.iter().map(|n| n.row + 1).max().unwrap_or(0)
    }
    /// Activities at `level` steps on `side`.
    #[cfg(test)]
    pub fn level(&self, side: Side, level: u32) -> Vec<usize> {
        let mut out: Vec<usize> = self
            .nodes
            .iter()
            .filter(|n| n.side == side && n.level == level)
            .map(|n| n.activity)
            .collect();
        out.sort_unstable();
        out
    }
}

/// Build the trace around `focus`.
pub fn trace(model: &Model, focus: usize, options: TraceOptions) -> TraceGraph {
    if focus >= model.activities.len() {
        return TraceGraph::default();
    }
    let mut into: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut out_of: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (index, link) in model.links.iter().enumerate() {
        into.entry(link.succ).or_default().push(index);
        out_of.entry(link.pred).or_default().push(index);
    }
    // Discovery order: focus, then predecessors level by level, then
    // successors level by level.
    let mut found: Vec<(usize, Side, u32)> = vec![(focus, Side::Focus, 0)];
    let mut seen: BTreeMap<usize, usize> = BTreeMap::from([(focus, 0)]);
    for (side, limit, edges, forward) in [
        (Side::Predecessor, options.predecessor_levels, &into, false),
        (Side::Successor, options.successor_levels, &out_of, true),
    ] {
        let mut queue = VecDeque::from([(focus, 0u32)]);
        while let Some((at, level)) = queue.pop_front() {
            if level >= limit {
                continue;
            }
            for &l in edges.get(&at).map_or(&[][..], |v| v.as_slice()) {
                let link = &model.links[l];
                let next = if forward { link.succ } else { link.pred };
                if let std::collections::btree_map::Entry::Vacant(e) = seen.entry(next) {
                    e.insert(found.len());
                    found.push((next, side, level + 1));
                    queue.push_back((next, level + 1));
                }
            }
        }
    }
    let links: Vec<usize> = model
        .links
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            seen.contains_key(&l.pred) && seen.contains_key(&l.succ) && l.pred != l.succ
        })
        .map(|(i, _)| i)
        .collect();
    // Longest-path rank inside the traced set (Kahn order; nodes caught in
    // a loop follow the rest in discovery order).
    let n = found.len();
    let slot = |activity: usize| seen[&activity];
    let mut indegree = vec![0usize; n];
    let mut next: Vec<Vec<usize>> = vec![vec![]; n];
    let mut prev: Vec<Vec<usize>> = vec![vec![]; n];
    for &l in &links {
        let (a, b) = (slot(model.links[l].pred), slot(model.links[l].succ));
        indegree[b] += 1;
        next[a].push(b);
        prev[b].push(a);
    }
    let mut rank = vec![0usize; n];
    let mut done = vec![false; n];
    let mut ready: VecDeque<usize> = (0..n).filter(|&k| indegree[k] == 0).collect();
    while let Some(k) = ready.pop_front() {
        done[k] = true;
        for &s in &next[k] {
            rank[s] = rank[s].max(rank[k] + 1);
            indegree[s] -= 1;
            if indegree[s] == 0 {
                ready.push_back(s);
            }
        }
    }
    let mut last = rank.iter().copied().max().unwrap_or(0);
    for k in 0..n {
        if !done[k] {
            last += 1;
            rank[k] = last;
        }
    }
    // Rows: the first column in discovery order, later columns by the mean
    // row of their predecessors in earlier columns.
    let columns = rank.iter().copied().max().map_or(0, |r| r + 1);
    let mut row = vec![0usize; n];
    for c in 0..columns {
        let mut members: Vec<(f32, usize)> = (0..n)
            .filter(|&k| rank[k] == c)
            .map(|k| {
                let before: Vec<f32> = prev[k]
                    .iter()
                    .filter(|&&p| rank[p] < c)
                    .map(|&p| row[p] as f32)
                    .collect();
                let key = if before.is_empty() {
                    f32::MAX
                } else {
                    before.iter().sum::<f32>() / before.len() as f32
                };
                (key, k)
            })
            .collect();
        members.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for (r, (_, k)) in members.into_iter().enumerate() {
            row[k] = r;
        }
    }
    TraceGraph {
        nodes: found
            .iter()
            .enumerate()
            .map(|(k, &(activity, side, level))| TraceNode {
                activity,
                side,
                level,
                column: rank[k],
                row: row[k],
            })
            .collect(),
        links,
    }
}

/// Top-left outline pixel of a node's box, relative to the canvas origin.
pub fn box_origin(node: &TraceNode) -> Vec2 {
    ORIGIN
        + Vec2::new(
            node.column as f32 * COLUMN_PITCH,
            node.row as f32 * ROW_PITCH,
        )
}

/// The polyline of a relationship from `from` to `to` (box rectangles),
/// with the arrow direction at its last point.
pub fn link_path(from: Rect, to: Rect, kind: LinkType, neighbours: bool) -> (Vec<Pos2>, Vec2) {
    let from_finish = matches!(kind, LinkType::FS | LinkType::FF);
    let to_start = matches!(kind, LinkType::FS | LinkType::SS);
    if from_finish && to_start && neighbours && to.left() > from.right() {
        let a = Pos2::new(from.right(), from.center().y.floor());
        let b = Pos2::new(to.left(), to.center().y.floor());
        if a.y == b.y {
            return (vec![a, b], Vec2::X);
        }
        let mid = ((a.x + b.x) / 2.).round();
        return (
            vec![a, Pos2::new(mid, a.y), Pos2::new(mid, b.y), b],
            Vec2::X,
        );
    }
    let corner_x = if from_finish {
        from.right()
    } else {
        from.left()
    };
    let start = Pos2::new(corner_x, from.bottom() - 1.);
    let route = from.bottom().max(to.bottom()) - 1. + ROUTE_DROP;
    let bend = Pos2::new(corner_x + DIAGONAL, route);
    let target_x = if to_start { to.left() } else { to.right() };
    // The arrow starts under the outline; the box's shadow covers its tip.
    let tip = Pos2::new(target_x, to.bottom());
    (vec![start, bend, Pos2::new(target_x, route), tip], -Vec2::Y)
}

#[derive(Clone, Debug)]
pub struct TraceState {
    pub options: TraceOptions,
    /// Activity (task id) the trace is built around.
    pub focus: Option<i64>,
    /// The top pane's selection last seen, so a new selection there moves
    /// the focus while a click on a box here does not.
    pub synced: Option<i64>,
    /// Scroll the canvas to center this activity's box next frame.
    pub center_on: Option<i64>,
    /// Box rectangles of the last frame, for tests and renders.
    pub boxes: Vec<(i64, Rect)>,
    /// The pane's own scale, from `MIN_ZOOM` to `MAX_ZOOM` (1 is the
    /// measured size). It is not kept with the layout.
    pub zoom: f32,
    /// Fit the whole trace to the pane next frame.
    pub fit_pending: bool,
    /// Scroll the canvas to this offset next frame.
    scroll_to: Option<Vec2>,
}

impl Default for TraceState {
    fn default() -> Self {
        Self {
            options: TraceOptions::default(),
            focus: None,
            synced: None,
            center_on: None,
            boxes: vec![],
            zoom: 1.,
            fit_pending: false,
            scroll_to: None,
        }
    }
}

impl App {
    fn task_index(&self, task_id: i64) -> Option<usize> {
        self.project
            .model
            .activities
            .iter()
            .position(|a| a.task_id == task_id)
    }

    /// Follow the top pane: a new selection there becomes the focus.
    pub(crate) fn sync_trace_focus(&mut self) {
        let selected = self
            .selected
            .and_then(|i| self.project.model.activities.get(i))
            .map(|a| a.task_id);
        let t = &mut self.panes.trace;
        if selected != t.synced {
            t.synced = selected;
            if selected.is_some() {
                t.focus = selected;
            }
        }
    }

    /// The trace of the current focus.
    pub(crate) fn current_trace(&mut self) -> Option<(usize, TraceGraph)> {
        self.sync_trace_focus();
        let focus = self.panes.trace.focus.and_then(|t| self.task_index(t))?;
        Some((
            focus,
            trace(&self.project.model, focus, self.panes.trace.options),
        ))
    }

    /// Zoom In and Zoom Out of the Trace Logic pane.
    pub(crate) fn trace_zoom(&mut self, zoom_in: bool) {
        let t = &mut self.panes.trace;
        t.zoom = (t.zoom * if zoom_in { 1.25 } else { 0.8 }).clamp(MIN_ZOOM, MAX_ZOOM);
    }

    /// Zoom to Best Fit of the Trace Logic pane.
    pub(crate) fn trace_fit(&mut self) {
        self.panes.trace.fit_pending = true;
    }

    /// The Trace Logic menu, on the background and on the boxes. Rectangles
    /// are `trace_menu_<key>`.
    fn trace_context_menu(&mut self, m: &mut Menu) {
        use chrome::{Icon, Item};
        if m.item(
            "options",
            Item::new("Trace Logic Options...").icon(Icon::Trace),
        ) {
            self.open_trace_options();
        }
        m.separator();
        if m.item("zoom_in", Item::new("Zoom In").icon(Icon::ZoomIn)) {
            self.trace_zoom(true);
        }
        if m.item("zoom_out", Item::new("Zoom Out").icon(Icon::ZoomOut)) {
            self.trace_zoom(false);
        }
        if m.item("fit", Item::new("Zoom to Best Fit").icon(Icon::Fit)) {
            self.trace_fit();
        }
    }

    fn trace_menu_popup(&mut self, response: &egui::Response, at: Option<Pos2>) {
        ContextMenu::new("trace_menu")
            .min_width(160.)
            .open_at(at)
            .show(response, self, |app, menu| app.trace_context_menu(menu));
    }

    /// Select a box's activity in the top pane (the focus stays unless
    /// `make_focus`) and, with `center`, scroll its box to the middle.
    pub(crate) fn trace_select(&mut self, activity: usize, make_focus: bool, center: bool) {
        let task_id = self.project.model.activities[activity].task_id;
        self.jump_to(activity);
        let t = &mut self.panes.trace;
        t.synced = Some(task_id);
        if make_focus {
            t.focus = Some(task_id);
        }
        if center {
            t.center_on = Some(task_id);
        }
    }

    /// The Trace Logic pane filling `rect`.
    pub(crate) fn trace_logic_pane(&mut self, ui: &mut egui::Ui, rect: Rect) {
        ui.painter().rect_filled(rect, 0., Color32::WHITE);
        let Some((_, graph)) = self.current_trace() else {
            self.panes.trace.boxes.clear();
            ui.painter().text(
                rect.left_top() + Vec2::new(8., 8.),
                Align2::LEFT_TOP,
                "Select an activity to trace its logic.",
                chrome::table_font(),
                Color32::GRAY,
            );
            return;
        };
        let natural = Vec2::new(
            ORIGIN.x + graph.columns() as f32 * COLUMN_PITCH,
            ORIGIN.y + graph.rows().max(1) as f32 * ROW_PITCH,
        );
        let t = &mut self.panes.trace;
        if t.fit_pending {
            t.fit_pending = false;
            t.zoom = ((rect.width() - 20.) / natural.x.max(1.))
                .min((rect.height() - 20.) / natural.y.max(1.))
                .clamp(MIN_ZOOM, MAX_ZOOM);
            t.scroll_to = Some(Vec2::ZERO);
        }
        // Ctrl+wheel (and a pinch) arrive as a zoom factor, not a scroll.
        if ui.rect_contains_pointer(rect) {
            let factor = ui.input(|i| i.zoom_delta());
            if factor != 1. {
                t.zoom = (t.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
            }
        }
        let zoom = t.zoom;
        let size = natural * zoom;
        let mut scroll = egui::ScrollArea::both()
            .id_salt("trace_logic_canvas")
            .auto_shrink([false, false])
            .scroll_source(egui::scroll_area::ScrollSource {
                mouse_wheel: !ui.input(|i| i.modifiers.ctrl),
                ..Default::default()
            });
        let center = self.panes.trace.center_on.take();
        if let Some(offset) = self.panes.trace.scroll_to.take() {
            scroll = scroll.scroll_offset(offset);
        } else if let Some(center) = center {
            if let Some(node) = self.task_index(center).and_then(|i| graph.node(i).copied()) {
                let at = (box_origin(&node) + Vec2::new(BOX_W, BOX_H) / 2.) * zoom;
                let offset = (at - rect.size() / 2.).max(Vec2::ZERO);
                scroll = scroll.scroll_offset(offset);
            }
        }
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::top_down(Align::LEFT)),
        );
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        let mut clicked = None;
        let mut boxes_out = vec![];
        let key_menu = self.ctx_menu.take();
        scroll.show(&mut child, |ui| {
            let (canvas, background) =
                ui.allocate_exact_size(size.max(rect.size() - Vec2::splat(16.)), Sense::click());
            let p = ui.painter().clone();
            let view = ui.clip_rect();
            let selected = self.selected.filter(|_| !self.panes.printing);
            let base = canvas.min;
            // Boxes in the measured size at the canvas corner, and on the
            // screen at the pane's scale.
            let mut rects: BTreeMap<usize, Rect> = BTreeMap::new();
            let mut natural_rects: BTreeMap<usize, Rect> = BTreeMap::new();
            for node in &graph.nodes {
                let o = box_origin(node);
                natural_rects.insert(
                    node.activity,
                    Rect::from_min_size(base + o, Vec2::new(BOX_W, BOX_H)),
                );
                rects.insert(
                    node.activity,
                    Rect::from_min_size(base + o * zoom, Vec2::new(BOX_W, BOX_H) * zoom),
                );
            }
            for &l in &graph.links {
                let link = &self.project.model.links[l];
                let (Some(a), Some(b)) = (graph.node(link.pred), graph.node(link.succ)) else {
                    continue;
                };
                let neighbours = b.column == a.column + 1;
                let (points, dir) = link_path(
                    natural_rects[&link.pred],
                    natural_rects[&link.succ],
                    link.link_type,
                    neighbours,
                );
                let points: Vec<Pos2> = points
                    .into_iter()
                    .map(|q| base + (q - base) * zoom)
                    .collect();
                let critical = self.box_critical(link.pred) && self.box_critical(link.succ);
                let color = if critical { CRITICAL } else { Color32::BLACK };
                boxes::relationship(&p, &points, dir, color, zoom);
            }
            for node in &graph.nodes {
                let i = node.activity;
                let r = rects[&i];
                let colors = if self.box_critical(i) {
                    boxes::CRITICAL_BOX
                } else {
                    boxes::NORMAL
                };
                let text = self.box_text(i);
                boxes::paint_box(
                    &p,
                    r.min,
                    zoom,
                    &text,
                    colors,
                    Template::Standard,
                    selected == Some(i),
                    boxes::Progress::None,
                );
                let task_id = self.project.model.activities[i].task_id;
                let response = ui
                    .interact(r, ui.id().with(("trace_box", task_id)), Sense::click())
                    .on_hover_text(format!("{}  {}", text.id, text.name));
                self.action(&format!("trace_box_{}", text.id), &response);
                boxes_out.push((task_id, r));
                // A box partly out of view is scrolled to the middle; a box
                // in full view stays under the pointer for a double-click.
                let center = !view.contains_rect(r);
                if response.double_clicked() {
                    clicked = Some((i, true, center));
                } else if response.clicked() && clicked.is_none() {
                    clicked = Some((i, false, center));
                } else if response.secondary_clicked() && clicked.is_none() {
                    // Select first; the focus stays.
                    clicked = Some((i, false, false));
                }
                if response.secondary_clicked() || response.context_menu_opened() {
                    self.trace_menu_popup(&response, None);
                }
            }
            let at = key_menu.then(|| view.min + Vec2::new(40., 24.));
            if at.is_some() || background.secondary_clicked() || background.context_menu_opened() {
                self.trace_menu_popup(&background, at);
            }
        });
        self.panes.trace.boxes = boxes_out;
        if let Some((i, focus, center)) = clicked {
            self.trace_select(i, focus, center);
        }
    }

    pub(crate) fn open_trace_options(&mut self) {
        self.panes.trace_dialog = Some(self.panes.trace.options);
    }

    /// Trace Logic Options: the two level counts with OK, Cancel, Apply
    /// and Help.
    pub(crate) fn trace_options_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.panes.trace_dialog else {
            return;
        };
        let mut close = false;
        let mut apply = false;
        use crate::dialog_kit::{self as kit, Entry};
        const W: f32 = 380.;
        const H: f32 = 160.;
        let (clicked, closed, actions) = kit::window(
            ctx,
            "Trace Logic Options",
            "Trace Logic Options",
            Vec2::new(W, H),
            |c| {
                for (n, (label, value, key)) in [
                    (
                        "Number of predecessor levels",
                        &mut draft.predecessor_levels,
                        "trace_predecessor_levels",
                    ),
                    (
                        "Number of successor levels",
                        &mut draft.successor_levels,
                        "trace_successor_levels",
                    ),
                ]
                .into_iter()
                .enumerate()
                {
                    let y = 12. + 56. * n as f32;
                    kit::text(c, 12., y, label, true);
                    let mut number = f64::from(*value);
                    let range = 0.0..=f64::from(MAX_LEVELS);
                    kit::number_spin(
                        c,
                        key,
                        (12., y + 18., 90., 21.),
                        &mut number,
                        1.,
                        range,
                        true,
                    );
                    *value = number.round() as u32;
                }
                kit::strip(c, W, H);
                kit::button_column(
                    c,
                    W,
                    &[
                        Entry::ok("trace_ok"),
                        Entry::cancel("trace_cancel"),
                        Entry::apply("trace_apply", true),
                        Entry::help("trace_help"),
                    ],
                )
                .clicked
            },
        );
        self.record_dialog_actions(actions);
        match clicked.as_deref() {
            Some("trace_ok") => {
                apply = true;
                close = true;
            }
            Some("trace_cancel") => close = true,
            Some("trace_apply") => apply = true,
            Some("trace_help") => self.open_help_for("Trace Logic Options"),
            _ => close |= closed,
        }
        if apply {
            self.panes.trace.options = draft;
        }
        self.panes.trace_dialog = if close { None } else { Some(draft) };
    }
}
