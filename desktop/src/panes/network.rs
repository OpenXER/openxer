//! Activity Network: the top pane alternative to the table and Gantt.
//!
//! Layout rules:
//! - Rows: one band per WBS element, in the order the activity table shows
//!   them (the layout's grouping and sorting), titled with the WBS code and
//!   name. Activities hidden by the filter or a collapsed band are left out.
//! - Columns: one column per distinct early start day (the displayed start,
//!   leveled when the schedule was leveled), earliest at the left, across
//!   the whole diagram, so a column reads as one start date in every band.
//!   Activities without a start go to a last column.
//! - Inside a band, activities sharing a column stack downwards, ordered by
//!   remaining duration (Longest on Top or Shortest on Top), then by ID.
//!   A band is as tall as its tallest stack; with Organize centered shorter
//!   stacks are centered in the band, otherwise they start at its top.
//! - Pitch: the 160 x 78 box plus 80 px times the horizontal spacing factor
//!   across and 30 px times the vertical spacing factor down.
//! - The layout depends only on the schedule and the options, so it does not
//!   move when the selection, zoom or scroll position changes.
//! - A box dragged to a new place keeps it (by Activity ID, in the layout)
//!   until Reorganize returns every box to the computed layout.
//! - A band can be collapsed to its title strip (Expand All and Collapse All
//!   act on the bands while the network is on top). Collapsing hides boxes
//!   without moving the columns of the others.
use super::boxes::{self, Template, BOX_H, BOX_W};
use super::*;
use crate::context_menu::{Caps, ContextMenu, Menu};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub const MARGIN: f32 = 12.;
pub const BAND_TITLE: f32 = 20.;
pub const BAND_GAP: f32 = 10.;
pub const H_GAP: f32 = 80.;
pub const V_GAP: f32 = 30.;
/// Width of the WBS tree beside the canvas.
pub const TREE_WIDTH: f32 = 190.;
pub const MIN_ZOOM: f32 = 0.1;
pub const MAX_ZOOM: f32 = 2.;
/// Most saved box positions a layout may hold, and the canvas bound.
pub const MAX_POSITIONS: usize = 100_000;
pub const MAX_COORD: f32 = 1_000_000.;
/// Box text size range in Font & Colors.
pub const FONT_RANGE: std::ops::RangeInclusive<f32> = 6. ..=16.;

fn hex(c: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

fn parse_hex(v: &serde_json::Value) -> Option<Color32> {
    let text = v.as_str()?.strip_prefix('#')?;
    if text.len() != 6 || !text.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&text[i..i + 2], 16).ok();
    Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?))
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct NetworkOptions {
    pub template: Template,
    pub horizontal_spacing: f32,
    pub vertical_spacing: f32,
    pub centered: bool,
    pub longest_on_top: bool,
    pub show_progress: bool,
    pub zoom_to_fit: bool,
    /// Font & Colors: box text size and the normal and critical box fills.
    pub font_size: f32,
    pub normal: Color32,
    pub critical: Color32,
}

impl Default for NetworkOptions {
    fn default() -> Self {
        Self {
            template: Template::Standard,
            horizontal_spacing: 1.,
            vertical_spacing: 1.,
            centered: false,
            longest_on_top: true,
            show_progress: false,
            zoom_to_fit: false,
            font_size: boxes::FONT,
            normal: boxes::NORMAL.fill,
            critical: boxes::CRITICAL_BOX.fill,
        }
    }
}

impl NetworkOptions {
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "template": self.template.key(),
            "horizontal_spacing": self.horizontal_spacing,
            "vertical_spacing": self.vertical_spacing,
            "centered": self.centered,
            "longest_on_top": self.longest_on_top,
            "show_progress": self.show_progress,
            "zoom_to_fit": self.zoom_to_fit,
            "font_size": self.font_size,
            "normal_fill": hex(self.normal),
            "critical_fill": hex(self.critical),
        })
    }
    pub fn restore(&mut self, v: &serde_json::Value) {
        if let Some(t) = v["template"].as_str().and_then(Template::from_key) {
            self.template = t;
        }
        for (key, field) in [
            ("horizontal_spacing", &mut self.horizontal_spacing),
            ("vertical_spacing", &mut self.vertical_spacing),
        ] {
            if let Some(f) = v[key].as_f64().filter(|f| f.is_finite()) {
                *field = (f as f32).clamp(0.25, 5.);
            }
        }
        for (key, field) in [
            ("centered", &mut self.centered),
            ("longest_on_top", &mut self.longest_on_top),
            ("show_progress", &mut self.show_progress),
            ("zoom_to_fit", &mut self.zoom_to_fit),
        ] {
            if let Some(b) = v[key].as_bool() {
                *field = b;
            }
        }
        if let Some(f) = v["font_size"].as_f64().filter(|f| f.is_finite()) {
            self.font_size = (f as f32).clamp(*FONT_RANGE.start(), *FONT_RANGE.end());
        }
        if let Some(c) = parse_hex(&v["normal_fill"]) {
            self.normal = c;
        }
        if let Some(c) = parse_hex(&v["critical_fill"]) {
            self.critical = c;
        }
    }
    /// Box colors for a normal or critical activity.
    pub fn colors(&self, critical: bool) -> boxes::BoxColors {
        boxes::BoxColors::for_fill(if critical { self.critical } else { self.normal })
    }
    pub fn column_pitch(&self) -> f32 {
        BOX_W + H_GAP * self.horizontal_spacing
    }
    pub fn row_pitch(&self) -> f32 {
        BOX_H + V_GAP * self.vertical_spacing
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NetBox {
    pub activity: usize,
    pub column: usize,
    /// Unscaled canvas rectangle of the box outline.
    pub rect: Rect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NetBand {
    pub label: String,
    pub top: f32,
    pub bottom: f32,
    /// Only the title strip is shown; the band's boxes are left out.
    pub collapsed: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetworkLayout {
    pub boxes: Vec<NetBox>,
    pub bands: Vec<NetBand>,
    pub size: Vec2,
}

impl NetworkLayout {
    pub fn find(&self, activity: usize) -> Option<&NetBox> {
        self.boxes.iter().find(|b| b.activity == activity)
    }
}

/// Lay out `bands` (title and member activities in table order).
#[cfg(test)]
pub fn layout(
    model: &Model,
    bands: &[(String, Vec<usize>)],
    options: &NetworkOptions,
) -> NetworkLayout {
    layout_with(model, bands, options, &BTreeSet::new())
}

/// `layout` with the bands titled in `collapsed` reduced to their title
/// strip. The columns still count every activity, so the boxes of the open
/// bands stay where they were.
pub fn layout_with(
    model: &Model,
    bands: &[(String, Vec<usize>)],
    options: &NetworkOptions,
    collapsed: &BTreeSet<String>,
) -> NetworkLayout {
    let start_day = |i: usize| {
        displayed_start(&model.activities[i], model.options.do_level).map(|m| m.div_euclid(DAY))
    };
    let mut days: Vec<i64> = bands
        .iter()
        .flat_map(|(_, members)| members.iter().filter_map(|&i| start_day(i)))
        .collect();
    days.sort_unstable();
    days.dedup();
    let column = |i: usize| {
        start_day(i).map_or(days.len(), |d| days.binary_search(&d).unwrap_or(days.len()))
    };
    let (cp, rp) = (options.column_pitch(), options.row_pitch());
    let mut out = NetworkLayout::default();
    let mut y = MARGIN;
    let mut columns_used = 0;
    for (label, members) in bands {
        if members.is_empty() {
            continue;
        }
        if collapsed.contains(label) {
            out.bands.push(NetBand {
                label: label.clone(),
                top: y,
                bottom: y + BAND_TITLE + 2.,
                collapsed: true,
            });
            y += BAND_TITLE + 2. + BAND_GAP;
            continue;
        }
        let mut stacks: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for &i in members {
            stacks.entry(column(i)).or_default().push(i);
        }
        let tallest = stacks.values().map(Vec::len).max().unwrap_or(1);
        let top = y;
        let first = top + BAND_TITLE + 8.;
        for (&c, stack) in stacks.iter_mut() {
            stack.sort_by(|&a, &b| {
                let (ra, rb) = (model.activities[a].rem, model.activities[b].rem);
                let order = if options.longest_on_top {
                    rb.cmp(&ra)
                } else {
                    ra.cmp(&rb)
                };
                order.then_with(|| {
                    model.activities[a]
                        .task_code
                        .cmp(&model.activities[b].task_code)
                })
            });
            let shift = if options.centered {
                ((tallest - stack.len()) as f32 * rp / 2.).round()
            } else {
                0.
            };
            for (r, &i) in stack.iter().enumerate() {
                let min = Pos2::new(MARGIN + c as f32 * cp, first + shift + r as f32 * rp);
                out.boxes.push(NetBox {
                    activity: i,
                    column: c,
                    rect: Rect::from_min_size(min, Vec2::new(BOX_W, BOX_H)),
                });
            }
            columns_used = columns_used.max(c + 1);
        }
        let bottom = first + tallest as f32 * rp - (rp - BOX_H) + 12.;
        out.bands.push(NetBand {
            label: label.clone(),
            top,
            bottom,
            collapsed: false,
        });
        y = bottom + BAND_GAP;
    }
    out.size = Vec2::new(
        (MARGIN * 2. + columns_used as f32 * cp - (cp - BOX_W)).max(MARGIN * 2.),
        y + MARGIN - BAND_GAP,
    );
    out
}

/// Move the boxes listed in `positions` (Activity ID to the box's top-left
/// canvas corner) and grow the canvas to hold them.
pub fn apply_positions(net: &mut NetworkLayout, model: &Model, positions: &BTreeMap<String, Pos2>) {
    if positions.is_empty() {
        return;
    }
    for b in &mut net.boxes {
        if let Some(at) = model
            .activities
            .get(b.activity)
            .and_then(|a| positions.get(&a.task_code))
        {
            b.rect = Rect::from_min_size(*at, b.rect.size());
        }
    }
    for b in &net.boxes {
        net.size = net.size.max(b.rect.max.to_vec2() + Vec2::splat(MARGIN));
    }
}

/// Parse saved box positions; invalid entries are left out and the list is
/// bounded.
pub fn positions_from_json(v: &serde_json::Value) -> BTreeMap<String, Pos2> {
    let mut out = BTreeMap::new();
    let Some(map) = v.as_object() else {
        return out;
    };
    for (code, at) in map.iter().take(MAX_POSITIONS) {
        let xy = at.as_array().and_then(|a| {
            let x = a.first()?.as_f64()? as f32;
            let y = a.get(1)?.as_f64()? as f32;
            let ok = |c: f32| c.is_finite() && (0. ..=MAX_COORD).contains(&c);
            (a.len() == 2 && ok(x) && ok(y)).then_some(Pos2::new(x, y))
        });
        if let Some(p) = xy.filter(|_| !code.is_empty() && code.len() <= 100) {
            out.insert(code.clone(), p);
        }
    }
    out
}

/// A relationship between two boxes as a polyline of horizontal and
/// vertical runs, with the arrow direction at its end.
pub fn link_path(from: Rect, to: Rect, kind: LinkType, scale: f32) -> (Vec<Pos2>, Vec2) {
    let from_finish = matches!(kind, LinkType::FS | LinkType::FF);
    let to_start = matches!(kind, LinkType::FS | LinkType::SS);
    let a = Pos2::new(
        if from_finish {
            from.right()
        } else {
            from.left()
        },
        from.center().y,
    );
    let b = Pos2::new(if to_start { to.left() } else { to.right() }, to.center().y);
    let out = 10. * scale;
    if from_finish && to_start && b.x - a.x > 2. * out {
        let mid = (a.x + b.x) / 2.;
        let points = if (a.y - b.y).abs() < 0.5 {
            vec![a, b]
        } else {
            vec![a, Pos2::new(mid, a.y), Pos2::new(mid, b.y), b]
        };
        return (points, Vec2::X);
    }
    let ax = if from_finish { a.x + out } else { a.x - out };
    let bx = if to_start { b.x - out } else { b.x + out };
    let dir = if to_start { Vec2::X } else { -Vec2::X };
    // Run between the rows when the boxes overlap vertically.
    let mid_y = if (a.y - b.y).abs() < BOX_H * scale {
        from.bottom().max(to.bottom()) + 8. * scale
    } else {
        (a.y + b.y) / 2.
    };
    (
        vec![
            a,
            Pos2::new(ax, a.y),
            Pos2::new(ax, mid_y),
            Pos2::new(bx, mid_y),
            Pos2::new(bx, b.y),
            b,
        ],
        dir,
    )
}

#[derive(Clone, Debug)]
pub struct NetworkState {
    pub options: NetworkOptions,
    pub zoom: f32,
    pub fit_pending: bool,
    /// Selection last scrolled to.
    pub synced: Option<i64>,
    pub center_on: Option<i64>,
    /// Box rectangles (screen) of the last frame, for tests and renders.
    pub boxes: Vec<(i64, Rect)>,
    pub view_size: Vec2,
    pub content_size: Vec2,
    /// Boxes the user dragged: Activity ID to the top-left canvas corner.
    pub positions: BTreeMap<String, Pos2>,
    /// Titles of the bands shown as a title strip only.
    pub collapsed: BTreeSet<String>,
    /// The WBS tree beside the canvas.
    pub show_tree: bool,
    /// The band last picked in the tree, until the selection changes.
    pub tree_pick: Option<String>,
    /// Relationship lines (model link index and screen polyline) of the
    /// last frame, for the double-click hit test and for tests.
    pub links: Vec<(usize, Vec<Pos2>)>,
    /// Scroll offset of the last frame.
    pub offset: Vec2,
    /// Scroll the canvas to this offset next frame.
    scroll_to: Option<Vec2>,
    /// Alt+drag zoom rectangle in canvas coordinates: where the press began
    /// and where the pointer is now.
    pub zoom_drag: Option<(Pos2, Pos2)>,
    /// A relationship being dragged from a box edge.
    pub link_drag: Option<NetLinkDrag>,
    /// The computed layout (before dragged positions) and the project
    /// snapshot serial, rows, options and collapsed bands it was computed
    /// for.
    layout_cache: RefCell<Option<LayoutCache>>,
}

/// A relationship drag from the start or finish edge of a box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetLinkDrag {
    pub activity: usize,
    pub edge: Edge,
}

type LayoutCache = (
    u64,
    Vec<grid::Row>,
    NetworkOptions,
    BTreeSet<String>,
    Rc<NetworkLayout>,
);

/// Most collapsed band titles a saved layout may hold.
pub const MAX_COLLAPSED: usize = 10_000;

impl NetworkState {
    /// Forget the kept layout.
    #[cfg(test)]
    pub(crate) fn forget_layout(&self) {
        self.layout_cache.borrow_mut().take();
    }

    /// The stored form: the options and the dragged box positions.
    pub fn json(&self) -> serde_json::Value {
        let mut v = self.options.json();
        v["positions"] = serde_json::Value::Object(
            self.positions
                .iter()
                .map(|(code, p)| (code.clone(), serde_json::json!([p.x.round(), p.y.round()])))
                .collect(),
        );
        v["collapsed"] = self.collapsed.iter().cloned().collect();
        v["show_tree"] = self.show_tree.into();
        v
    }
    pub fn restore(&mut self, v: &serde_json::Value) {
        self.options.restore(v);
        if !v["positions"].is_null() {
            self.positions = positions_from_json(&v["positions"]);
        }
        if let Some(list) = v["collapsed"].as_array() {
            self.collapsed = list
                .iter()
                .filter_map(|t| t.as_str())
                .filter(|t| !t.is_empty() && t.len() <= 400)
                .take(MAX_COLLAPSED)
                .map(str::to_string)
                .collect();
        }
        if let Some(b) = v["show_tree"].as_bool() {
            self.show_tree = b;
        }
    }
}

impl Default for NetworkState {
    fn default() -> Self {
        Self {
            options: NetworkOptions::default(),
            zoom: 1.,
            fit_pending: false,
            synced: None,
            center_on: None,
            boxes: vec![],
            view_size: Vec2::new(800., 400.),
            content_size: Vec2::ZERO,
            positions: BTreeMap::new(),
            collapsed: BTreeSet::new(),
            show_tree: false,
            tree_pick: None,
            links: vec![],
            offset: Vec2::ZERO,
            scroll_to: None,
            zoom_drag: None,
            link_drag: None,
            layout_cache: RefCell::new(None),
        }
    }
}

/// Font & Colors' working copy: the options and the size text.
#[derive(Clone, Debug)]
pub struct FontDraft {
    pub options: NetworkOptions,
    pub size: String,
}

impl FontDraft {
    pub fn new(options: NetworkOptions) -> Self {
        Self {
            options,
            size: format!("{}", options.font_size),
        }
    }
}

/// The Activity Network Options dialog's working copy.
#[derive(Clone, Debug)]
pub struct NetworkDialog {
    pub options: NetworkOptions,
    pub tab: usize,
    /// Box positions taken by Copy From, set when the dialog is applied.
    pub positions: Option<BTreeMap<String, Pos2>>,
}

/// What Copy From takes from another saved layout: its box template and
/// the places its boxes were dragged to.
#[derive(Clone, Debug, PartialEq)]
pub struct Copied {
    pub template: Template,
    pub positions: BTreeMap<String, Pos2>,
}

/// One saved layout that holds Activity Network settings.
#[derive(Clone, Debug)]
pub struct CopySource {
    pub name: String,
    pub copied: Copied,
}

/// The Copy From dialog while open: the saved layouts to choose from.
#[derive(Clone, Debug)]
pub struct CopyDraft {
    pub sources: Vec<CopySource>,
    pub selected: Option<usize>,
}

impl App {
    /// WBS bands of the visible activities, in table order.
    #[cfg(test)]
    pub(crate) fn network_bands(&self) -> Vec<(String, Vec<usize>)> {
        self.network_bands_of(&self.rows())
    }

    /// `network_bands` of these table rows.
    fn network_bands_of(&self, rows: &[grid::Row]) -> Vec<(String, Vec<usize>)> {
        let mut bands: Vec<(i64, Vec<usize>)> = vec![];
        let mut band_of: HashMap<i64, usize> = HashMap::new();
        for row in rows {
            if let grid::Row::Activity(i) = *row {
                let wbs = self
                    .project
                    .details
                    .get(&self.project.model.activities[i].task_id)
                    .map_or(0, |d| d.wbs);
                match band_of.get(&wbs) {
                    Some(&k) => bands[k].1.push(i),
                    None => {
                        band_of.insert(wbs, bands.len());
                        bands.push((wbs, vec![i]));
                    }
                }
            }
        }
        bands
            .into_iter()
            .map(|(wbs, members)| (self.project.wbs_label(wbs), members))
            .collect()
    }

    /// The network as drawn: the computed layout (kept while the project,
    /// the rows and the options are unchanged) with dragged boxes moved.
    pub(crate) fn network_layout(&self) -> NetworkLayout {
        let rows = self.rows();
        let serial = self.project_serial();
        let options = self.panes.network.options;
        let collapsed = &self.panes.network.collapsed;
        let kept = self
            .panes
            .network
            .layout_cache
            .borrow()
            .as_ref()
            .filter(|(s, r, o, c, _)| {
                Some(*s) == serial && *r == rows && *o == options && c == collapsed
            })
            .map(|c| c.4.clone());
        let computed = kept.unwrap_or_else(|| {
            let net = Rc::new(layout_with(
                &self.project.model,
                &self.network_bands_of(&rows),
                &options,
                collapsed,
            ));
            *self.panes.network.layout_cache.borrow_mut() =
                serial.map(|s| (s, rows, options, collapsed.clone(), net.clone()));
            net
        });
        let mut net = (*computed).clone();
        apply_positions(&mut net, &self.project.model, &self.panes.network.positions);
        net
    }

    /// Reorganize All: every box back to the computed layout.
    pub(crate) fn network_reorganize(&mut self) {
        self.panes.network.positions.clear();
        if self.panes.network.options.zoom_to_fit {
            self.network_fit();
        }
    }

    /// Reorganize: the selected boxes and every box downstream of them (the
    /// branch they start) go back to the computed layout and the rest keep
    /// their places. With nothing selected it is Reorganize All.
    pub(crate) fn network_reorganize_branch(&mut self) {
        let selected = self.selected_in_order();
        if selected.is_empty() {
            self.network_reorganize();
            return;
        }
        let model = &self.project.model;
        let mut successors: Vec<Vec<usize>> = vec![vec![]; model.activities.len()];
        for link in &model.links {
            if let Some(list) = successors.get_mut(link.pred) {
                list.push(link.succ);
            }
        }
        let mut branch: BTreeSet<usize> = selected.iter().copied().collect();
        let mut stack = selected;
        while let Some(at) = stack.pop() {
            for &next in successors.get(at).map_or(&[][..], |v| v.as_slice()) {
                if branch.insert(next) {
                    stack.push(next);
                }
            }
        }
        let n = &mut self.panes.network;
        let mut moved = 0;
        for &i in &branch {
            if n.positions.remove(&model.activities[i].task_code).is_some() {
                moved += 1;
            }
        }
        self.status = match moved {
            0 => "The selected branch is already in the computed layout.".into(),
            1 => "1 box returned to the computed layout".into(),
            k => format!("{k} boxes returned to the computed layout"),
        };
    }

    pub(crate) fn network_zoom(&mut self, zoom_in: bool) {
        let n = &mut self.panes.network;
        n.zoom = (n.zoom * if zoom_in { 1.25 } else { 0.8 }).clamp(MIN_ZOOM, MAX_ZOOM);
    }

    pub(crate) fn network_fit(&mut self) {
        self.panes.network.fit_pending = true;
    }

    /// Zoom so that `area` (canvas coordinates) fills the view, and scroll
    /// to it. A rectangle smaller than a few pixels is a click and does
    /// nothing.
    pub(crate) fn network_zoom_to(&mut self, area: Rect) {
        let n = &mut self.panes.network;
        if area.width() * n.zoom < 8. || area.height() * n.zoom < 8. {
            return;
        }
        let view = n.view_size;
        let zoom = ((view.x - 20.) / area.width())
            .min((view.y - 20.) / area.height())
            .clamp(MIN_ZOOM, MAX_ZOOM);
        n.zoom = zoom;
        n.scroll_to = Some((area.center().to_vec2() * zoom - view / 2.).max(Vec2::ZERO));
    }

    /// Titles of the bands the network shows now, in order.
    pub(crate) fn network_band_labels(&self) -> Vec<String> {
        self.network_bands_of(&self.rows())
            .into_iter()
            .map(|(label, _)| label)
            .collect()
    }

    /// Expand All while the network is on top: every band shows its boxes.
    pub(crate) fn network_expand_all(&mut self) {
        self.panes.network.collapsed.clear();
    }

    /// Collapse All while the network is on top: every band shows only its
    /// title strip.
    pub(crate) fn network_collapse_all(&mut self) {
        self.panes.network.collapsed = self.network_band_labels().into_iter().collect();
    }

    /// Collapse or expand one band.
    pub(crate) fn network_toggle_band(&mut self, label: &str) {
        let collapsed = &mut self.panes.network.collapsed;
        if !collapsed.remove(label) {
            collapsed.insert(label.to_string());
        }
    }

    /// Scroll the network so that the band titled `label` is at the top.
    pub(crate) fn network_scroll_to_band(&mut self, label: &str) {
        let net = self.network_layout();
        let n = &mut self.panes.network;
        if let Some(band) = net.bands.iter().find(|b| b.label == label) {
            n.tree_pick = Some(label.to_string());
            n.scroll_to = Some(Vec2::new(n.offset.x, (band.top - 4.).max(0.) * n.zoom));
        }
    }

    /// Tie `pred` to `succ` with a relationship of `kind` (no lag), as one
    /// undo step. A tie that would close a loop or repeat one is refused
    /// with a status message.
    pub(crate) fn network_link_boxes(
        &mut self,
        pred: usize,
        succ: usize,
        kind: LinkType,
        time: f64,
    ) {
        let before = self.project.clone();
        let before_revision = self.records_revision;
        let names = (
            self.project.model.activities[pred].task_code.clone(),
            self.project.model.activities[succ].task_code.clone(),
        );
        match relationships::add(&mut self.project.model, pred, succ, kind, 0) {
            Ok(()) => {
                self.edit_step(before, before_revision, time);
                self.check_relationship_sequence(self.project.model.links.len() - 1, None);
                self.status = format!(
                    "Created {} to {} ({})",
                    names.0,
                    names.1,
                    &kind.as_str()[3..]
                );
            }
            Err(error) => {
                self.project = before;
                self.status = format!("Relationship not created: {error}");
            }
        }
    }

    /// What the network's menu can do right now: the selection is read on
    /// every open, so a right-click on the background (which clears it)
    /// shows the selection commands disabled.
    pub(crate) fn network_caps(&self) -> Caps {
        Caps {
            can_edit: true,
            can_cut: false,
            can_paste: false,
            selection_count: self.selected_in_order().len(),
            view_has_gantt: false,
            has_rows: !self.table_activities().is_empty(),
        }
    }

    /// Opens the network's context menu on a right-click of `response`, or
    /// at `at` when the keyboard asked for it.
    fn network_menu_popup(&mut self, response: &egui::Response, at: Option<Pos2>) {
        let caps = self.network_caps();
        ContextMenu::new("net")
            .caps(caps)
            .min_width(199.)
            .open_at(at)
            .show(response, self, |app, menu| app.network_context_menu(menu));
    }

    /// The 13 items of the Activity Network menu. Rectangles are
    /// `net_<key>`.
    fn network_context_menu(&mut self, m: &mut Menu) {
        use chrome::{Icon, Item};
        let caps = m.caps();
        let any = caps.edit_selection_enabled();
        let many = caps.can_edit && caps.several();
        let mut run = None;
        let details = Item::new("Acti&vity Details")
            .icon(Icon::Details)
            .enabled(caps.has_selection());
        if m.item("activity_details", details) {
            run = Some(editing::Command::ActivityDetails);
        }
        m.separator();
        if m.item("options", Item::new("Activity Network Options...")) {
            self.open_network_options();
        }
        if m.item("trace_options", Item::new("Trace Logic Options...")) {
            self.open_trace_options();
        }
        m.separator();
        if m.item(
            "reorganize",
            Item::new("Reor&ganize")
                .icon(Icon::Reorganize)
                .enabled(caps.has_rows),
        ) {
            self.network_reorganize_branch();
        }
        if m.item(
            "reorganize_all",
            Item::new("Reorgani&ze All").enabled(caps.has_rows),
        ) {
            self.network_reorganize();
        }
        if m.item("fit", Item::new("Zoom to &Best Fit").icon(Icon::Fit)) {
            self.network_fit();
        }
        m.separator();
        let dissolve = any && self.can_dissolve_selection();
        if m.item("dissolve", Item::new("Diss&olve").enabled(dissolve)) {
            run = Some(editing::Command::Dissolve);
        }
        let link = Item::new("Lin&k Activities").icon(Icon::Link).enabled(many);
        if m.item("link", link) {
            run = Some(editing::Command::Link);
        }
        let unlink = Item::new("Unlink Activities")
            .icon(Icon::Unlink)
            .enabled(many);
        if m.item("unlink", unlink) {
            run = Some(editing::Command::Unlink);
        }
        m.separator();
        for (key, label, icon, command) in [
            (
                "assign_codes",
                "Activity &Codes...",
                Some(Icon::AssignCode),
                editing::Command::Assign(DetailTab::Codes),
            ),
            (
                "assign_roles",
                "R&oles...",
                Some(Icon::AssignRole),
                editing::Command::AssignResource(details::assignments::ResourceAssign::Roles),
            ),
            (
                "assign_by_role",
                "Resources by Ro&le...",
                Some(Icon::AssignByRole),
                editing::Command::AssignResource(details::assignments::ResourceAssign::ByRole),
            ),
            (
                "work_products",
                "Work Products...",
                None,
                editing::Command::Assign(DetailTab::Documents),
            ),
        ] {
            let mut item = Item::new(label).enabled(any);
            item.icon = icon;
            if m.item(key, item) {
                run = Some(command);
            }
        }
        if let Some(command) = run {
            self.editing.pending = Some(command);
        }
    }

    /// The WBS tree beside the canvas: one row per band, a box to collapse
    /// or expand it, and a click scrolls the network to the band.
    fn network_tree(&mut self, ui: &mut egui::Ui) {
        let bands = self.network_bands_of(&self.rows());
        let mut shown_for_selection = None;
        if let Some(i) = self.selected {
            shown_for_selection = bands
                .iter()
                .find(|(_, members)| members.contains(&i))
                .map(|(label, _)| label.clone());
        }
        let picked = self.panes.network.tree_pick.clone().or(shown_for_selection);
        let mut toggle = None;
        let mut scroll = None;
        egui::ScrollArea::vertical()
            .id_salt("network_wbs_tree")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                for (k, (label, members)) in bands.iter().enumerate() {
                    let collapsed = self.panes.network.collapsed.contains(label);
                    let (row, response) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 20.), Sense::click());
                    let on = picked.as_deref() == Some(label.as_str());
                    if on {
                        ui.painter().rect_filled(row, 0., SELECT);
                    }
                    let box_rect = Rect::from_center_size(
                        row.left_center() + Vec2::new(11., 0.),
                        Vec2::splat(11.),
                    );
                    let stroke = Stroke::new(1_f32, Color32::from_gray(0x60));
                    ui.painter().rect_filled(box_rect, 0., Color32::WHITE);
                    ui.painter()
                        .rect_stroke(box_rect, 0., stroke, egui::StrokeKind::Inside);
                    ui.painter().hline(
                        box_rect.shrink(3.).x_range(),
                        box_rect.center().y,
                        Stroke::new(1_f32, Color32::BLACK),
                    );
                    if collapsed {
                        ui.painter().vline(
                            box_rect.center().x,
                            box_rect.shrink(3.).y_range(),
                            Stroke::new(1_f32, Color32::BLACK),
                        );
                    }
                    ui.painter().with_clip_rect(row).text(
                        row.left_center() + Vec2::new(24., 0.),
                        Align2::LEFT_CENTER,
                        format!("{label} ({})", members.len()),
                        chrome::table_font(),
                        INK,
                    );
                    self.action(&format!("network_tree_{k}"), &response);
                    let toggle_hit = ui.interact(
                        box_rect.expand(2.),
                        ui.id().with(("network_tree_toggle", k)),
                        Sense::click(),
                    );
                    self.action(&format!("network_tree_toggle_{k}"), &toggle_hit);
                    if toggle_hit.clicked() {
                        toggle = Some(label.clone());
                    } else if response.clicked() {
                        scroll = Some(label.clone());
                    }
                }
            });
        if let Some(label) = toggle {
            self.network_toggle_band(&label);
        }
        if let Some(label) = scroll {
            self.network_scroll_to_band(&label);
        }
    }

    /// The Activity Network in the top pane.
    pub(crate) fn network_view(&mut self, host: &mut egui::Ui) {
        if self.panes.network.show_tree {
            egui::SidePanel::left("network_wbs_tree_panel")
                .exact_width(TREE_WIDTH)
                .resizable(false)
                .show_separator_line(true)
                .frame(egui::Frame::NONE.fill(Color32::WHITE))
                .show_inside(host, |ui| self.network_tree(ui));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::WHITE))
            .show_inside(host, |ui| {
                let rect = ui.max_rect();
                self.network_canvas(ui, rect);
            });
    }

    fn network_canvas(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let net = self.network_layout();
        let n = &mut self.panes.network;
        n.view_size = rect.size();
        n.content_size = net.size;
        if n.fit_pending {
            n.fit_pending = false;
            let fit = ((rect.width() - 20.) / net.size.x.max(1.))
                .min((rect.height() - 20.) / net.size.y.max(1.));
            n.zoom = fit.clamp(MIN_ZOOM, MAX_ZOOM);
            n.scroll_to = Some(Vec2::ZERO);
        }
        // Ctrl+wheel (and a pinch) arrive as a zoom factor, not a scroll.
        if ui.rect_contains_pointer(rect) {
            let factor = ui.input(|i| i.zoom_delta());
            if factor != 1. {
                n.zoom = (n.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
            }
        }
        let zoom = n.zoom;
        let selected_id = self
            .selected
            .and_then(|i| self.project.model.activities.get(i))
            .map(|a| a.task_id);
        if selected_id != self.panes.network.synced {
            self.panes.network.synced = selected_id;
            self.panes.network.center_on = selected_id;
        }
        let mut scroll = egui::ScrollArea::both()
            .id_salt("activity_network")
            .auto_shrink([false, false])
            // Dragging moves boxes, so it does not scroll.
            .scroll_source(egui::scroll_area::ScrollSource {
                mouse_wheel: !ui.input(|i| i.modifiers.ctrl),
                drag: false,
                ..Default::default()
            });
        let center = self.panes.network.center_on.take();
        if let Some(offset) = self.panes.network.scroll_to.take() {
            scroll = scroll.scroll_offset(offset);
        } else if let Some(center) = center {
            if let Some(b) = self
                .project
                .model
                .activities
                .iter()
                .position(|a| a.task_id == center)
                .and_then(|i| net.find(i))
            {
                let at = b.rect.center().to_vec2() * zoom;
                let view = rect.size();
                let offset = (at - view / 2.).max(Vec2::ZERO);
                scroll = scroll.scroll_offset(offset);
            }
        }
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::top_down(Align::LEFT)),
        );
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        let modifiers = ui.input(|i| i.modifiers);
        let time = ui.input(|i| i.time);
        // Shift+F10 and the Menu key reach the network first.
        let key_menu = self.ctx_menu.take();
        let mut clicked: Option<(usize, egui::Modifiers)> = None;
        let mut details = None;
        let mut toggled: Option<String> = None;
        let mut linked: Option<(usize, usize, LinkType)> = None;
        let mut zoomed: Option<Rect> = None;
        let mut double_at: Option<Pos2> = None;
        let mut moved: Option<(usize, Vec2)> = None;
        let mut lines: Vec<(usize, Vec<Pos2>)> = vec![];
        let mut out = vec![];
        let output = scroll.show_viewport(&mut child, |ui, viewport| {
            let size = (net.size * zoom).max(rect.size() - Vec2::splat(16.));
            let (canvas, background) = ui.allocate_exact_size(size, Sense::click());
            if background.secondary_clicked() {
                // Nothing is under the pointer: nothing stays selected, so
                // the selection commands show disabled.
                self.selected = None;
                self.editing.selection.clear();
                self.date_edits.clear();
                self.panes.network.synced = None;
                self.panes.network.tree_pick = None;
            }
            if background.double_clicked() {
                double_at = background.interact_pointer_pos();
            }
            let origin = canvas.min;
            let to_screen =
                |r: Rect| Rect::from_min_size(origin + r.min.to_vec2() * zoom, r.size() * zoom);
            let visible = Rect::from_min_size(origin + viewport.min.to_vec2(), viewport.size());
            let p = ui.painter().clone();
            let zooming = modifiers.alt || self.panes.network.zoom_drag.is_some();
            // WBS bands: a title strip in the layout's first band colors.
            let style = self.grid.layout.group_sort.band_style(0);
            let band_fill = Color32::from_rgb(
                style.background[0],
                style.background[1],
                style.background[2],
            );
            let band_text = Color32::from_rgb(style.text[0], style.text[1], style.text[2]);
            for (k, band) in net.bands.iter().enumerate() {
                let r = Rect::from_min_max(
                    Pos2::new(origin.x, origin.y + band.top * zoom),
                    Pos2::new(canvas.right(), origin.y + band.bottom * zoom),
                );
                if !r.intersects(visible) {
                    continue;
                }
                let title = Rect::from_min_max(
                    r.min,
                    Pos2::new(r.right(), r.top() + (BAND_TITLE * zoom).max(4.)),
                );
                p.rect_filled(title, 0., band_fill);
                p.hline(
                    r.x_range(),
                    r.bottom(),
                    Stroke::new(1_f32, Color32::from_gray(0xC0)),
                );
                if zoom >= 0.4 {
                    let left = visible.left().max(title.left());
                    // The collapse box, then the title.
                    let glyph = Rect::from_center_size(
                        Pos2::new(left + 12., title.center().y),
                        Vec2::splat(11.),
                    );
                    p.rect_filled(glyph, 0., Color32::WHITE);
                    p.rect_stroke(
                        glyph,
                        0.,
                        Stroke::new(1_f32, band_text),
                        egui::StrokeKind::Inside,
                    );
                    p.hline(
                        glyph.shrink(3.).x_range(),
                        glyph.center().y,
                        Stroke::new(1_f32, Color32::BLACK),
                    );
                    if band.collapsed {
                        p.vline(
                            glyph.center().x,
                            glyph.shrink(3.).y_range(),
                            Stroke::new(1_f32, Color32::BLACK),
                        );
                    }
                    let hit = ui.interact(
                        glyph.expand(2.),
                        ui.id().with(("network_band", k)),
                        Sense::click(),
                    );
                    self.action(&format!("network_band_toggle_{k}"), &hit);
                    if hit.clicked() && !zooming {
                        toggled = Some(band.label.clone());
                    }
                    let font = FontId::new(
                        (FONT_SIZE * zoom.max(0.75)).min(14.),
                        chrome::bold_font().family,
                    );
                    p.with_clip_rect(title.intersect(visible)).text(
                        Pos2::new(left + 24., title.center().y),
                        Align2::LEFT_CENTER,
                        &band.label,
                        font,
                        band_text,
                    );
                }
            }
            let index: BTreeMap<usize, Rect> = net
                .boxes
                .iter()
                .map(|b| (b.activity, to_screen(b.rect)))
                .collect();
            for (at, link) in self.project.model.links.iter().enumerate() {
                let (Some(&a), Some(&b)) = (index.get(&link.pred), index.get(&link.succ)) else {
                    continue;
                };
                if !a.union(b).expand(20. * zoom).intersects(visible) {
                    continue;
                }
                let (points, dir) = link_path(a, b, link.link_type, zoom);
                let critical = self.box_critical(link.pred) && self.box_critical(link.succ);
                let color = if critical { CRITICAL } else { Color32::BLACK };
                boxes::relationship(&p, &points, dir, color, zoom);
                lines.push((at, points));
            }
            let options = self.panes.network.options;
            let mut hits: Vec<(usize, Rect)> = vec![];
            for b in &net.boxes {
                let r = index[&b.activity];
                if !r.expand(4.).intersects(visible) {
                    continue;
                }
                let i = b.activity;
                let colors = options.colors(self.box_critical(i));
                let progress = if options.show_progress {
                    self.box_progress(i)
                } else {
                    boxes::Progress::None
                };
                let text = self.box_text(i);
                boxes::paint_box_font(
                    &p,
                    r.min,
                    zoom,
                    &text,
                    colors,
                    options.template,
                    self.is_selected(i),
                    progress,
                    options.font_size,
                );
                let task_id = self.project.model.activities[i].task_id;
                let response = ui.interact(
                    r,
                    ui.id().with(("network_box", task_id)),
                    Sense::click_and_drag(),
                );
                self.action(&format!("network_box_{}", text.id), &response);
                out.push((task_id, r));
                hits.push((i, r));
                if zooming {
                    continue;
                }
                if response.hovered() {
                    let edge = ui
                        .input(|x| x.pointer.hover_pos())
                        .and_then(|pos| boxes::edge_at(r, pos, zoom));
                    if edge.is_some() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                    }
                }
                if response.double_clicked() {
                    details = Some(i);
                } else if response.clicked() {
                    clicked = Some((i, modifiers));
                }
                if response.drag_started() {
                    let edge = ui
                        .input(|x| x.pointer.press_origin())
                        .and_then(|pos| boxes::edge_at(r, pos, zoom));
                    match edge {
                        Some(edge) => {
                            self.panes.network.link_drag = Some(NetLinkDrag { activity: i, edge });
                        }
                        None if !self.is_selected(i) => clicked = Some((i, Default::default())),
                        None => {}
                    }
                }
                if response.dragged() && self.panes.network.link_drag.is_none() {
                    let delta = drag_motion(&response, ui.ctx());
                    if delta != Vec2::ZERO {
                        moved = Some((i, delta / zoom));
                    }
                }
                if response.secondary_clicked() && !self.is_selected(i) {
                    self.click_row(i, Default::default());
                    self.scroll_to_selected = true;
                    self.date_edits.clear();
                    self.panes.network.synced = Some(task_id);
                    self.panes.network.tree_pick = None;
                }
                if response.secondary_clicked() || response.context_menu_opened() {
                    self.network_menu_popup(&response, None);
                }
                if !response.dragged() && self.panes.network.link_drag.is_none() {
                    response.clone().on_hover_ui(|ui| {
                        ui.label(RichText::new(format!("{}  {}", text.id, text.name)).strong());
                        ui.label(format!("Start: {}", text.start));
                        ui.label(format!("Finish: {}", text.finish));
                    });
                }
            }
            // Drag from a box edge to another box: the relationship so far.
            if let Some(drag) = self.panes.network.link_drag {
                let pointer = ui.input(|i| i.pointer.interact_pos());
                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    self.panes.network.link_drag = None;
                } else if let (Some(from), Some(pos)) = (index.get(&drag.activity), pointer) {
                    let target = hits
                        .iter()
                        .find(|(i, r)| *i != drag.activity && r.contains(pos))
                        .copied();
                    let begin = if drag.edge == Edge::Start {
                        from.left_center()
                    } else {
                        from.right_center()
                    };
                    let kind = target.map(|(_, r)| {
                        // Dropped on the left two thirds of a box: its
                        // start; on the right third: its finish.
                        let to = if pos.x < r.left() + r.width() * 2. / 3. {
                            Edge::Start
                        } else {
                            Edge::Finish
                        };
                        relationships::kind(drag.edge, to)
                    });
                    let valid = target.zip(kind).is_some_and(|((i, _), kind)| {
                        relationships::can_connect(&self.project.model, drag.activity, i).is_ok()
                            && !self.project.model.links.iter().any(|l| {
                                l.pred == drag.activity && l.succ == i && l.link_type == kind
                            })
                    });
                    p.line_segment(
                        [begin, pos],
                        Stroke::new(1.5_f32, if valid { BLUE } else { CRITICAL }),
                    );
                    let hint = match (target, kind) {
                        (Some((i, _)), Some(kind)) => format!(
                            "{} to {}   {}",
                            self.project.model.activities[drag.activity].task_code,
                            self.project.model.activities[i].task_code,
                            &kind.as_str()[3..]
                        ),
                        _ => "Drag to another activity - Esc to cancel".into(),
                    };
                    egui::Area::new(egui::Id::new("network_relationship_hint"))
                        .order(egui::Order::Tooltip)
                        .fixed_pos(pos + Vec2::new(12., 12.))
                        .interactable(false)
                        .show(ui.ctx(), |ui| {
                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                ui.label(hint);
                            });
                        });
                    if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
                        self.panes.network.link_drag = None;
                        match (target, kind) {
                            (Some((i, _)), Some(kind)) => linked = Some((drag.activity, i, kind)),
                            _ => {
                                self.status =
                                    "Relationship cancelled: release on another activity.".into()
                            }
                        }
                    }
                } else if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
                    self.panes.network.link_drag = None;
                }
            }
            // Alt+drag: a rectangle to zoom to.
            let pointer = ui.input(|i| i.pointer.interact_pos());
            let to_canvas = |pos: Pos2| ((pos - origin) / zoom).to_pos2();
            if let Some(pos) = pointer {
                if modifiers.alt
                    && rect.contains(pos)
                    && ui.input(|i| i.pointer.primary_pressed())
                    && self.panes.network.link_drag.is_none()
                {
                    self.panes.network.zoom_drag = Some((to_canvas(pos), to_canvas(pos)));
                }
            }
            if let Some((start, _)) = self.panes.network.zoom_drag {
                let end = pointer.map_or(start, to_canvas);
                self.panes.network.zoom_drag = Some((start, end));
                let area = Rect::from_two_pos(start, end);
                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    self.panes.network.zoom_drag = None;
                } else if ui.input(|i| i.pointer.primary_down()) {
                    let screen = to_screen(area);
                    p.rect_filled(screen, 0., Color32::from_rgba_unmultiplied(0, 0, 255, 24));
                    p.rect_stroke(
                        screen,
                        0.,
                        Stroke::new(1_f32, BLUE),
                        egui::StrokeKind::Inside,
                    );
                } else {
                    self.panes.network.zoom_drag = None;
                    zoomed = Some(area);
                }
            }
            // The menu, also from the keyboard: just below the selected box,
            // else near the top of the view.
            let at = key_menu.then(|| {
                self.selected
                    .and_then(|s| index.get(&s))
                    .map_or(visible.min + Vec2::new(40., 24.), |r| {
                        Pos2::new(r.left() + 20., r.bottom())
                    })
            });
            if at.is_some() || background.secondary_clicked() || background.context_menu_opened() {
                self.network_menu_popup(&background, at);
            }
        });
        self.panes.network.offset = output.state.offset;
        self.panes.network.boxes = out;
        self.panes.network.links = lines;
        if let Some((i, delta)) = moved {
            if let Some(b) = net.find(i) {
                let code = self.project.model.activities[i].task_code.clone();
                let n = &mut self.panes.network;
                if n.positions.len() < MAX_POSITIONS || n.positions.contains_key(&code) {
                    let at =
                        (b.rect.min + delta).clamp(Pos2::ZERO, Pos2::new(MAX_COORD, MAX_COORD));
                    n.positions.insert(code, at);
                }
            }
        }
        if let Some(label) = toggled {
            self.network_toggle_band(&label);
        }
        if let Some(area) = zoomed {
            self.network_zoom_to(area);
        }
        if let Some((pred, succ, kind)) = linked {
            self.network_link_boxes(pred, succ, kind, time);
        }
        if let Some(pos) = double_at {
            if let Some(index) = crate::gantt::pick_link(pos, &self.panes.network.links) {
                if index < self.project.model.links.len() {
                    self.edit_link(index);
                }
            }
        }
        if let Some((i, modifiers)) = clicked {
            self.click_row(i, modifiers);
            self.scroll_to_selected = true;
            self.date_edits.clear();
            // Selecting here does not scroll this pane.
            self.panes.network.synced = self
                .selected
                .and_then(|s| self.project.model.activities.get(s))
                .map(|a| a.task_id);
            self.panes.network.tree_pick = None;
        }
        if let Some(i) = details {
            self.selected = Some(i);
            self.panes.network.synced = Some(self.project.model.activities[i].task_id);
            self.show_detail_tab(DetailTab::General);
        }
    }

    pub(crate) fn open_network_options(&mut self) {
        self.panes.network_dialog = Some(NetworkDialog {
            options: self.panes.network.options,
            tab: 1,
            positions: None,
        });
    }

    /// The saved layouts with Activity Network settings, to copy from.
    pub(crate) fn network_copy_sources(&self) -> Vec<CopySource> {
        self.grid
            .library
            .iter()
            .filter_map(|l| {
                let v = &l.panes.as_ref()?["network"];
                if !v.is_object() {
                    return None;
                }
                Some(CopySource {
                    name: l.name.clone(),
                    copied: Copied {
                        template: v["template"]
                            .as_str()
                            .and_then(Template::from_key)
                            .unwrap_or(Template::Standard),
                        positions: positions_from_json(&v["positions"]),
                    },
                })
            })
            .collect()
    }

    /// Activity Network Options: Activity Box Template and Activity Network
    /// Layout tabs with OK, Cancel, Apply and Help.
    pub(crate) fn network_options_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.panes.network_dialog.clone() else {
            return;
        };
        let mut close = false;
        let mut apply = false;
        let mut font = false;
        let mut copy = false;
        let mut help = false;
        use crate::dialog_kit::{self as kit, Entry};
        const W: f32 = 500.;
        const H: f32 = 330.;
        let (clicked, closed, actions) = kit::window(
            ctx,
            "Activity Network Options",
            "Activity Network Options",
            Vec2::new(W, H),
            |c| {
                let tabs = [
                    ("Activity Box Template", "0"),
                    ("Activity Network Layout", "1"),
                ]
                .map(|(label, key)| kit::VTab {
                    label,
                    key,
                    unavailable: None,
                });
                let page = (8., 28., W - 128., H - 36.);
                if let Some(k) = kit::tabs(c, "network_tab_", page, &tabs, draft.tab) {
                    draft.tab = k;
                }
                let o = &mut draft.options;
                if draft.tab == 0 {
                    kit::text(c, 20., 42., "Box template", true);
                    for (n, t) in Template::ALL.into_iter().enumerate() {
                        let key = format!("network_template_{}", t.key());
                        let y = 62. + 24. * n as f32;
                        if c.radio(&key, 20., y, 300., 18., t.caption(), o.template == t, true) {
                            o.template = t;
                        }
                    }
                } else {
                    kit::section(c, 16., 38., W - 144., 76., "Organization Options");
                    for (key, caption, centered, y) in [
                        ("network_along_top", "Organize along the top", false, 66.),
                        ("network_centered", "Organize centered", true, 88.),
                    ] {
                        if c.radio(
                            key,
                            26.,
                            y,
                            300.,
                            18.,
                            caption,
                            o.centered == centered,
                            true,
                        ) {
                            o.centered = centered;
                        }
                    }
                    kit::section(c, 16., 120., W - 144., 76., "Network Order Options");
                    for (key, caption, longest, y) in [
                        ("network_longest", "Longest on Top", true, 148.),
                        ("network_shortest", "Shortest on Top", false, 170.),
                    ] {
                        if c.radio(
                            key,
                            26.,
                            y,
                            300.,
                            18.,
                            caption,
                            o.longest_on_top == longest,
                            true,
                        ) {
                            o.longest_on_top = longest;
                        }
                    }
                    for (key, caption, value, y) in [
                        (
                            "network_hspacing",
                            "Horizontal spacing factor",
                            &mut o.horizontal_spacing,
                            208.,
                        ),
                        (
                            "network_vspacing",
                            "Vertical spacing factor",
                            &mut o.vertical_spacing,
                            236.,
                        ),
                    ] {
                        kit::text(c, 26., y + 4., caption, true);
                        let mut number = f64::from(*value);
                        kit::number_spin(
                            c,
                            key,
                            (200., y, 80., 21.),
                            &mut number,
                            0.05,
                            0.25..=5.,
                            true,
                        );
                        *value = number as f32;
                    }
                    c.checkbox(
                        "network_show_progress",
                        26.,
                        266.,
                        330.,
                        18.,
                        "Show progress",
                        &mut o.show_progress,
                        true,
                    );
                    c.checkbox(
                        "network_zoom_fit",
                        26.,
                        290.,
                        330.,
                        18.,
                        "Always zoom to best fit after reorganize",
                        &mut o.zoom_to_fit,
                        true,
                    );
                }
                kit::strip(c, W, H);
                kit::button_column(
                    c,
                    W,
                    &[
                        Entry::ok("network_ok"),
                        Entry::cancel("network_cancel"),
                        Entry::apply("network_apply", true),
                        Entry::Button("network_font_colors", "&Font && Colors...", true, true),
                        Entry::Button("network_copy_from", "Copy From...", true, false),
                        Entry::help("network_help"),
                    ],
                )
                .clicked
            },
        );
        self.record_dialog_actions(actions);
        match clicked.as_deref() {
            Some("network_ok") => {
                apply = true;
                close = true;
            }
            Some("network_cancel") => close = true,
            Some("network_apply") => apply = true,
            Some("network_font_colors") => font = true,
            Some("network_copy_from") => copy = true,
            Some("network_help") => help = true,
            _ => close |= closed,
        }
        if help {
            self.open_help_for("Activity Network Options");
        }
        if font {
            self.panes.network_font = Some(FontDraft::new(draft.options));
        }
        // Font & Colors edits the draft's fonts and colors; its OK hands
        // them back here.
        if let Some(back) = self.panes.network_font_done.take() {
            draft.options.font_size = back.font_size;
            draft.options.normal = back.normal;
            draft.options.critical = back.critical;
        }
        if copy {
            let sources = self.network_copy_sources();
            self.panes.network_copy = Some(CopyDraft {
                selected: (!sources.is_empty()).then_some(0),
                sources,
            });
        }
        // Copy From hands back the box template and positions of a saved
        // layout; the positions are applied with the options.
        if let Some(copied) = self.panes.network_copy_done.take() {
            draft.options.template = copied.template;
            draft.positions = Some(copied.positions);
        }
        if apply {
            let reorganized = draft.options != self.panes.network.options;
            self.panes.network.options = draft.options;
            if let Some(positions) = draft.positions.take() {
                self.panes.network.positions = positions;
            }
            if reorganized && draft.options.zoom_to_fit {
                self.network_fit();
            }
        }
        self.panes.network_dialog = if close { None } else { Some(draft) };
    }

    /// Font & Colors of the Activity Network: the box text size and the
    /// normal and critical box fills. OK hands them to an open Activity
    /// Network Options dialog, or applies them; Cancel, the close button and
    /// Escape drop them.
    pub(crate) fn network_font_dialog(&mut self, ctx: &egui::Context) {
        use crate::dialog_kit as kit;
        use crate::gantt::settings::Rgb;
        let Some(mut d) = self.panes.network_font.take() else {
            return;
        };
        let mut normal = Rgb::from_color(d.options.normal);
        let mut critical = Rgb::from_color(d.options.critical);
        let (clicked, closed, actions) = kit::window(
            ctx,
            "network_font",
            "Activity Network Font and Colors",
            Vec2::new(332., 150.),
            |c| {
                c.fill(0., 0., 332., 150., kit::STRIP);
                c.fill(5., 5., 210., 140., kit::FACE);
                c.lowered(5., 5., 210., 140.);
                kit::section(c, 11., 11., 198., 128., "Activity Boxes");
                kit::text(c, 21., 44., "Font size", true);
                c.spin(
                    "network_font_size",
                    120.,
                    40.,
                    70.,
                    21.,
                    &mut d.size,
                    0.5,
                    (*FONT_RANGE.start() as f64)..=(*FONT_RANGE.end() as f64),
                    true,
                );
                for (n, (key, caption, color)) in [
                    ("network_font_normal", "Normal", &mut normal),
                    ("network_font_critical", "Critical", &mut critical),
                ]
                .into_iter()
                .enumerate()
                {
                    let y = 74. + 30. * n as f32;
                    kit::text(c, 21., y + 4., caption, true);
                    kit::swatch(c, key, (120., y, 70., 22.), color, true);
                }
                kit::button_column(
                    c,
                    332.,
                    &[
                        kit::Entry::ok("network_font_ok"),
                        kit::Entry::cancel("network_font_cancel"),
                        kit::Entry::Button("network_font_default", "Default", true, true),
                    ],
                )
                .clicked
            },
        );
        self.record_dialog_actions(actions);
        let clicked = clicked.as_deref();
        d.options.normal = normal.color();
        d.options.critical = critical.color();
        if let Ok(size) = d.size.trim().parse::<f32>() {
            if size.is_finite() {
                d.options.font_size = size.clamp(*FONT_RANGE.start(), *FONT_RANGE.end());
            }
        }
        match clicked {
            Some("network_font_ok") => {
                if self.panes.network_dialog.is_some() {
                    self.panes.network_font_done = Some(d.options);
                } else {
                    let o = &mut self.panes.network.options;
                    o.font_size = d.options.font_size;
                    o.normal = d.options.normal;
                    o.critical = d.options.critical;
                }
            }
            Some("network_font_cancel") => {}
            Some("network_font_default") => {
                let defaults = NetworkOptions::default();
                d.options.font_size = defaults.font_size;
                d.options.normal = defaults.normal;
                d.options.critical = defaults.critical;
                d.size = format!("{}", defaults.font_size);
                self.panes.network_font = Some(d);
            }
            _ if closed => {}
            _ => self.panes.network_font = Some(d),
        }
    }

    /// Copy From: pick a saved layout; OK hands its box template and box
    /// positions to the open Activity Network Options dialog. Cancel, the
    /// close button and Escape change nothing.
    pub(crate) fn network_copy_dialog(&mut self, ctx: &egui::Context) {
        use crate::dialog_kit::{self as kit, Col};
        let Some(mut d) = self.panes.network_copy.take() else {
            return;
        };
        let mut chosen = None;
        let (clicked, closed, actions) = kit::window(
            ctx,
            "network_copy_from",
            "Copy From",
            Vec2::new(424., 232.),
            |c| {
                c.fill(0., 0., 424., 232., kit::STRIP);
                c.fill(5., 5., 302., 222., kit::FACE);
                c.lowered(5., 5., 302., 222.);
                kit::heading(c, 6., 6., 300., "Select the layout to copy from");
                let sources = &d.sources;
                let out = kit::list(
                    c,
                    "network_copy_from",
                    (10., 33., 292., 188.),
                    None,
                    &[
                        Col {
                            title: "Layout",
                            width: 130.,
                        },
                        Col {
                            title: "Box Template",
                            width: 112.,
                        },
                        Col {
                            title: "Moved",
                            width: 46.,
                        },
                    ],
                    sources.len(),
                    d.selected,
                    Some(0),
                    &mut |c, i, col, rect, sel| {
                        let s = &sources[i];
                        let text = match col {
                            0 => s.name.clone(),
                            1 => s.copied.template.caption().to_string(),
                            _ => s.copied.positions.len().to_string(),
                        };
                        kit::cell_text(c, rect, &text, sel, 4.);
                        if col == 0 {
                            c.act(&format!("network_copy_row_{i}"), rect);
                        }
                    },
                );
                if let Some(i) = out.clicked {
                    d.selected = Some(i);
                }
                if let Some(i) = out.double {
                    chosen = Some(i);
                }
                kit::button_column(
                    c,
                    424.,
                    &[
                        kit::Entry::Button("network_copy_ok", "OK", d.selected.is_some(), false),
                        kit::Entry::cancel("network_copy_cancel"),
                    ],
                )
                .clicked
            },
        );
        self.record_dialog_actions(actions);
        let clicked = clicked.as_deref();
        let pick = match clicked {
            Some("network_copy_ok") => d.selected,
            _ => chosen,
        };
        if let Some(source) = pick.and_then(|i| d.sources.get(i)) {
            self.panes.network_copy_done = Some(source.copied.clone());
            return;
        }
        if clicked == Some("network_copy_cancel") || closed {
            return;
        }
        self.panes.network_copy = Some(d);
    }
}
