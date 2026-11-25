//! Movable command bars: the model behind the menu bar, the toolbars and the
//! left and right directory bars. Every bar is either docked (on the top,
//! bottom, left or right edge of the main window, in a row, in an order) or
//! floating (a small tool window at a position with a width). This file holds
//! the layout, its saved form, the geometry the dock areas report each frame
//! and the rules that turn a pointer position into a drop place. The drawing
//! and the mouse handling are in `dockbars_ui.rs`.
use super::*;

/// The menu bar's name in the layout (it is not a toolbar: it cannot be
/// switched off).
pub const MENU_BAR: &str = "Menu Bar";

/// Most rows (columns on a side) one edge keeps.
pub const MAX_ROWS: usize = 8;
/// Most bars one row keeps.
const MAX_PER_ROW: usize = 32;
/// Narrowest content a floating bar can be dragged to.
pub const MIN_FLOAT_WIDTH: f32 = 36.;
/// Widest content a floating bar keeps from a saved file.
const MAX_FLOAT_WIDTH: f32 = 1800.;
/// Saved floating positions beyond this are ignored.
const POSITION_LIMIT: f32 = 10_000.;
/// A pointer this near the line between two rows asks for a new row.
const NEW_ROW_SLOP: f32 = 6.;
/// How far outside a dock area a pointer still counts as over it.
pub const DOCK_SNAP: f32 = 8.;
/// Thickness of the strip at the window edge that takes a bar when its dock
/// has no visible rows.
const EMPTY_STRIP: f32 = 12.;

/// A window edge a bar docks on.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

impl Side {
    pub const ALL: [Side; 4] = [Side::Top, Side::Bottom, Side::Left, Side::Right];

    pub fn index(self) -> usize {
        match self {
            Side::Top => 0,
            Side::Bottom => 1,
            Side::Left => 2,
            Side::Right => 3,
        }
    }

    /// Left and right docks stack their buttons downwards.
    pub fn vertical(self) -> bool {
        matches!(self, Side::Left | Side::Right)
    }

    pub fn key(self) -> &'static str {
        match self {
            Side::Top => "top",
            Side::Bottom => "bottom",
            Side::Left => "left",
            Side::Right => "right",
        }
    }

    pub fn from_key(key: &str) -> Option<Side> {
        Side::ALL.into_iter().find(|side| side.key() == key)
    }
}

/// A place in a dock: the edge, the row (column on a side) and the position
/// in that row. Rows count from the top or the left.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Slot {
    pub side: Side,
    pub row: usize,
    pub index: usize,
}

/// Where a dragged bar lands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Drop {
    /// Into an existing row at a position.
    Row(Slot),
    /// Into a new row inserted before row `row` (the end when `row` is the
    /// row count).
    NewRow { side: Side, row: usize },
}

/// A floating bar: where its window is, how wide its button area is, and the
/// dock place it returns to.
#[derive(Clone, Debug, PartialEq)]
pub struct Floater {
    pub pos: Pos2,
    pub width: f32,
    pub home: Slot,
    /// The bar had a row of its own at home: it returns to a new row.
    pub alone: bool,
}

/// Which bars sit where.
#[derive(Clone, Debug, PartialEq)]
pub struct BarLayout {
    rows: [Vec<Vec<String>>; 4],
    pub floating: BTreeMap<String, Floater>,
}

impl Default for BarLayout {
    fn default() -> Self {
        let row = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        Self {
            rows: [
                vec![
                    row(&[MENU_BAR]),
                    row(&menu::TOP_ROW_BARS),
                    row(&menu::SECOND_ROW_BARS),
                ],
                vec![],
                vec![row(&["Standard", "Enterprise", "Project"])],
                vec![row(&["Edit", "Assign", "Move"])],
            ],
            floating: BTreeMap::new(),
        }
    }
}

/// Is `name` a bar this build knows?
pub fn is_bar(name: &str) -> bool {
    name == MENU_BAR || menu::BUILT_TOOLBARS.contains(&name)
}

impl BarLayout {
    pub fn rows(&self, side: Side) -> &Vec<Vec<String>> {
        &self.rows[side.index()]
    }

    fn rows_mut(&mut self, side: Side) -> &mut Vec<Vec<String>> {
        &mut self.rows[side.index()]
    }

    /// Where `name` is docked.
    pub fn slot_of(&self, name: &str) -> Option<Slot> {
        for side in Side::ALL {
            for (row, bars) in self.rows(side).iter().enumerate() {
                if let Some(index) = bars.iter().position(|bar| bar == name) {
                    return Some(Slot { side, row, index });
                }
            }
        }
        None
    }

    #[cfg(test)]
    pub fn is_floating(&self, name: &str) -> bool {
        self.floating.contains_key(name)
    }

    /// Where `name` sits in the default layout.
    fn default_slot(name: &str) -> Slot {
        Self::default().slot_of(name).unwrap_or(Slot {
            side: Side::Top,
            row: 0,
            index: 0,
        })
    }

    /// Every bar name once, in dock order, then the floating ones.
    #[cfg(test)]
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = Side::ALL
            .iter()
            .flat_map(|side| self.rows(*side).iter().flatten().cloned())
            .collect();
        names.extend(self.floating.keys().cloned());
        names
    }

    /// Takes `name` out of its row; the row stays, perhaps empty, until
    /// `normalize` so row numbers do not move meanwhile.
    fn take(&mut self, name: &str) -> Option<Slot> {
        let slot = self.slot_of(name)?;
        self.rows_mut(slot.side)[slot.row].remove(slot.index);
        Some(slot)
    }

    /// Drops empty rows.
    fn normalize(&mut self) {
        for side in Side::ALL {
            self.rows_mut(side).retain(|row| !row.is_empty());
        }
    }

    /// Docks `name` at `drop`. `drop` is in the numbers of the layout as it
    /// was before the bar left its old place.
    pub fn apply(&mut self, name: &str, drop: Drop) {
        if !is_bar(name) {
            return;
        }
        let old = self.take(name);
        self.floating.remove(name);
        match drop {
            Drop::Row(slot) => {
                let mut index = slot.index;
                if let Some(old) = old {
                    if old.side == slot.side && old.row == slot.row && index > old.index {
                        index -= 1;
                    }
                }
                let rows = self.rows_mut(slot.side);
                if slot.row >= rows.len() {
                    rows.push(vec![name.to_string()]);
                } else {
                    let row = &mut rows[slot.row];
                    row.insert(index.min(row.len()), name.to_string());
                }
            }
            Drop::NewRow { side, row } => {
                let rows = self.rows_mut(side);
                if rows.len() >= MAX_ROWS && !rows.iter().any(|r| r.is_empty()) {
                    // No room for another row: join the last one.
                    if let Some(last) = rows.last_mut() {
                        last.push(name.to_string());
                    }
                } else {
                    rows.insert(row.min(rows.len()), vec![name.to_string()]);
                }
            }
        }
        self.normalize();
    }

    /// Floats `name` with its window's top-left corner at `pos`; a bar that
    /// already floats just moves. `width` is the button area width for a bar
    /// that was docked.
    pub fn float(&mut self, name: &str, pos: Pos2, width: f32) {
        if !is_bar(name) {
            return;
        }
        if let Some(floater) = self.floating.get_mut(name) {
            floater.pos = pos;
            return;
        }
        let (home, alone) = match self.slot_of(name) {
            Some(slot) => {
                let alone = self.rows(slot.side)[slot.row].len() == 1;
                (slot, alone)
            }
            None => (Self::default_slot(name), false),
        };
        self.take(name);
        self.normalize();
        self.floating.insert(
            name.to_string(),
            Floater {
                pos,
                width: width.clamp(MIN_FLOAT_WIDTH, MAX_FLOAT_WIDTH),
                home,
                alone,
            },
        );
    }

    /// Returns a floating bar to the place it was docked at last.
    pub fn redock(&mut self, name: &str) {
        let Some(floater) = self.floating.remove(name) else {
            return;
        };
        let home = floater.home;
        let drop = if floater.alone || home.row >= self.rows(home.side).len() {
            Drop::NewRow {
                side: home.side,
                row: home.row,
            }
        } else {
            Drop::Row(home)
        };
        self.apply(name, drop);
    }

    // ------------------------------------------------------------ saved form

    pub fn to_json(&self) -> serde_json::Value {
        let mut value = serde_json::Map::new();
        for side in Side::ALL {
            value.insert(side.key().into(), serde_json::json!(self.rows(side)));
        }
        let floating: serde_json::Map<String, serde_json::Value> = self
            .floating
            .iter()
            .map(|(name, f)| {
                (
                    name.clone(),
                    serde_json::json!({
                        "x": f.pos.x,
                        "y": f.pos.y,
                        "w": f.width,
                        "edge": f.home.side.key(),
                        "row": f.home.row,
                        "index": f.home.index,
                        "alone": f.alone,
                    }),
                )
            })
            .collect();
        value.insert("floating".into(), floating.into());
        value.into()
    }

    /// The layout a saved value describes. Anything that does not parse is
    /// ignored: unknown or repeated bar names, rows beyond the limits,
    /// positions that are not numbers. Bars the value does not mention come
    /// back at their default places.
    pub fn from_json(value: &serde_json::Value) -> Self {
        let Some(object) = value.as_object() else {
            return Self::default();
        };
        let mut layout = Self {
            rows: [vec![], vec![], vec![], vec![]],
            floating: BTreeMap::new(),
        };
        let mut seen = BTreeSet::new();
        for side in Side::ALL {
            let Some(rows) = object.get(side.key()).and_then(|v| v.as_array()) else {
                continue;
            };
            for row in rows.iter().take(MAX_ROWS) {
                let names: Vec<String> = row
                    .as_array()
                    .map(|bars| {
                        bars.iter()
                            .take(MAX_PER_ROW)
                            .filter_map(|bar| bar.as_str())
                            .filter(|bar| is_bar(bar))
                            .filter(|bar| seen.insert(bar.to_string()))
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default();
                if !names.is_empty() {
                    layout.rows_mut(side).push(names);
                }
            }
        }
        if let Some(floating) = object.get("floating").and_then(|v| v.as_object()) {
            for (name, f) in floating.iter().take(MAX_PER_ROW) {
                if !is_bar(name) || seen.contains(name) {
                    continue;
                }
                let number = |key: &str| {
                    f.get(key)
                        .and_then(|v| v.as_f64())
                        .filter(|v| v.is_finite())
                        .map(|v| v as f32)
                };
                let (Some(x), Some(y)) = (number("x"), number("y")) else {
                    continue;
                };
                if x.abs() > POSITION_LIMIT || y.abs() > POSITION_LIMIT {
                    continue;
                }
                let width = number("w")
                    .unwrap_or(MAX_FLOAT_WIDTH)
                    .clamp(MIN_FLOAT_WIDTH, MAX_FLOAT_WIDTH);
                let count = |key: &str| {
                    f.get(key)
                        .and_then(|v| v.as_u64())
                        .map_or(0, |v| v.min(MAX_PER_ROW as u64) as usize)
                };
                let side = f
                    .get("edge")
                    .and_then(|v| v.as_str())
                    .and_then(Side::from_key)
                    .unwrap_or_else(|| Self::default_slot(name).side);
                seen.insert(name.clone());
                layout.floating.insert(
                    name.clone(),
                    Floater {
                        pos: Pos2::new(x, y),
                        width,
                        home: Slot {
                            side,
                            row: count("row"),
                            index: count("index"),
                        },
                        alone: f.get("alone").and_then(|v| v.as_bool()).unwrap_or(false),
                    },
                );
            }
        }
        // Bars the saved value does not place return to their defaults.
        let defaults = Self::default();
        for side in Side::ALL {
            for (row, bars) in defaults.rows(side).iter().enumerate() {
                for name in bars {
                    if seen.insert(name.clone()) {
                        let rows = layout.rows_mut(side);
                        if row < rows.len() {
                            rows[row].push(name.clone());
                        } else if rows.len() < MAX_ROWS {
                            rows.push(vec![name.clone()]);
                        } else if let Some(last) = rows.last_mut() {
                            last.push(name.clone());
                        }
                    }
                }
            }
        }
        layout
    }
}

// ---------------------------------------------------------------- geometry

/// One bar as drawn in a dock this frame.
#[derive(Clone, Debug)]
pub struct BarGeo {
    pub name: String,
    pub index: usize,
    pub rect: Rect,
}

/// One drawn row (column on a side) of a dock.
#[derive(Clone, Debug)]
pub struct RowGeo {
    pub row: usize,
    pub rect: Rect,
    pub bars: Vec<BarGeo>,
}

/// What the docks and floating windows drew this frame.
#[derive(Clone, Debug)]
pub struct Geo {
    /// The main window below nothing but the status bar: where docks live.
    pub client: Rect,
    pub dock: [Option<Rect>; 4],
    pub rows: [Vec<RowGeo>; 4],
    pub floats: BTreeMap<String, Rect>,
}

impl Default for Geo {
    fn default() -> Self {
        Self::new(Rect::from_min_size(Pos2::ZERO, Vec2::ZERO))
    }
}

impl Geo {
    pub fn new(client: Rect) -> Self {
        Self {
            client,
            dock: Default::default(),
            rows: Default::default(),
            floats: BTreeMap::new(),
        }
    }

    pub fn rows(&self, side: Side) -> &Vec<RowGeo> {
        &self.rows[side.index()]
    }

    /// The pointer area that counts as over `side`'s dock.
    fn zone(&self, side: Side) -> Rect {
        if let Some(dock) = self.dock[side.index()] {
            return match side {
                Side::Top => dock.with_max_y(dock.bottom() + DOCK_SNAP),
                Side::Bottom => dock.with_min_y(dock.top() - DOCK_SNAP),
                Side::Left => dock.with_max_x(dock.right() + DOCK_SNAP),
                Side::Right => dock.with_min_x(dock.left() - DOCK_SNAP),
            };
        }
        // No visible bars: a strip along the window edge, between the
        // docks that run the full width.
        let client = self.client;
        let top = self.dock[Side::Top.index()].map_or(client.top(), |r| r.bottom());
        let bottom = self.dock[Side::Bottom.index()].map_or(client.bottom(), |r| r.top());
        match side {
            Side::Top => Rect::from_min_max(
                client.min,
                Pos2::new(client.right(), client.top() + EMPTY_STRIP),
            ),
            Side::Bottom => Rect::from_min_max(
                Pos2::new(client.left(), client.bottom() - EMPTY_STRIP),
                client.max,
            ),
            Side::Left => Rect::from_min_max(
                Pos2::new(client.left(), top),
                Pos2::new(client.left() + EMPTY_STRIP, bottom),
            ),
            Side::Right => Rect::from_min_max(
                Pos2::new(client.right() - EMPTY_STRIP, top),
                Pos2::new(client.right(), bottom),
            ),
        }
    }

    /// Where a bar dragged to `p` would dock, or none when `p` is away from
    /// every dock area. `dragged` is the bar being moved, which does not
    /// count as a neighbor.
    pub fn drop_at(&self, layout: &BarLayout, dragged: &str, p: Pos2) -> Option<Drop> {
        Side::ALL
            .into_iter()
            .find_map(|side| self.drop_on(layout, dragged, side, p))
    }

    fn drop_on(&self, layout: &BarLayout, dragged: &str, side: Side, p: Pos2) -> Option<Drop> {
        if !self.zone(side).contains(p) {
            return None;
        }
        let rows = self.rows(side);
        if rows.is_empty() {
            return Some(Drop::NewRow {
                side,
                row: layout.rows(side).len(),
            });
        }
        let vertical = side.vertical();
        let (cross, main) = if vertical { (p.x, p.y) } else { (p.y, p.x) };
        for row in rows {
            let (lo, hi) = if vertical {
                (row.rect.left(), row.rect.right())
            } else {
                (row.rect.top(), row.rect.bottom())
            };
            if cross < lo + NEW_ROW_SLOP {
                return Some(Drop::NewRow { side, row: row.row });
            }
            if cross < hi - NEW_ROW_SLOP {
                let others = row.bars.iter().filter(|bar| bar.name != dragged);
                let mut index = None;
                let mut last = None;
                for bar in others {
                    let center = if vertical {
                        bar.rect.center().y
                    } else {
                        bar.rect.center().x
                    };
                    if main < center {
                        index = Some(bar.index);
                        break;
                    }
                    last = Some(bar.index + 1);
                }
                let index = index.or(last).unwrap_or(0);
                return Some(Drop::Row(Slot {
                    side,
                    row: row.row,
                    index,
                }));
            }
        }
        Some(Drop::NewRow {
            side,
            row: layout.rows(side).len(),
        })
    }

    /// The gray outline shown for a drop: where the bar, `size` big once
    /// docked there, will sit.
    pub fn outline(&self, drop: Drop, dragged: &str, size: Vec2) -> Rect {
        match drop {
            Drop::Row(slot) => {
                let Some(row) = self.rows(slot.side).iter().find(|r| r.row == slot.row) else {
                    return Rect::from_min_size(self.client.min, size);
                };
                let vertical = slot.side.vertical();
                let others: Vec<&BarGeo> =
                    row.bars.iter().filter(|bar| bar.name != dragged).collect();
                let along = match others.iter().find(|bar| bar.index >= slot.index) {
                    Some(bar) if vertical => bar.rect.top(),
                    Some(bar) => bar.rect.left(),
                    None => match others.last() {
                        Some(bar) if vertical => bar.rect.bottom(),
                        Some(bar) => bar.rect.right(),
                        None if vertical => row.rect.top(),
                        None => row.rect.left(),
                    },
                };
                if vertical {
                    Rect::from_min_size(
                        Pos2::new(row.rect.left(), along),
                        Vec2::new(row.rect.width(), size.y),
                    )
                } else {
                    Rect::from_min_size(
                        Pos2::new(along, row.rect.top()),
                        Vec2::new(size.x, row.rect.height()),
                    )
                }
            }
            Drop::NewRow { side, row } => {
                let vertical = side.vertical();
                let rows = self.rows(side);
                let zone = self.zone(side);
                // The line the new row opens at.
                let line = match rows.iter().find(|r| r.row >= row) {
                    Some(r) if vertical => r.rect.left(),
                    Some(r) => r.rect.top(),
                    None => match (rows.last(), self.dock[side.index()]) {
                        (Some(r), _) if vertical => r.rect.right(),
                        (Some(r), _) => r.rect.bottom(),
                        (None, _) if vertical => zone.left(),
                        (None, _) => zone.top(),
                    },
                };
                let line = match side {
                    Side::Bottom if rows.is_empty() => zone.bottom() - size.y,
                    Side::Right if rows.is_empty() => zone.right() - size.x,
                    _ => line,
                };
                if vertical {
                    let top = self.dock[side.index()].map_or(zone.top(), |d| d.top());
                    let x = (line - size.x / 2.).max(self.client.left());
                    Rect::from_min_size(Pos2::new(x, top), size)
                } else {
                    let y = (line - size.y / 2.).max(self.client.top());
                    Rect::from_min_size(Pos2::new(self.client.left(), y), size)
                }
            }
        }
    }
}

/// A bar being dragged.
#[derive(Clone, Debug)]
pub struct Drag {
    pub name: String,
    /// The drag started on a floating window's title.
    pub from_float: bool,
    /// The pointer relative to the bar (docked) or window (floating) corner.
    pub grab: Vec2,
    /// Where the bar would land now.
    pub target: Option<Target>,
    /// The gray outline shown now.
    pub outline: Option<Rect>,
}

/// A drag's landing place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Dock(Drop),
    /// Float with the window's top-left corner here.
    Float(Pos2),
}

/// Bar state that is not saved: the geometry and the drag in progress.
#[derive(Default)]
pub struct Runtime {
    pub geo: Geo,
    pub drag: Option<Drag>,
    /// The width each toolbar button last drew with, by action.
    pub button_width: BTreeMap<String, f32>,
    /// The widest button each bar drew when stacked, by bar.
    pub thickness: BTreeMap<String, f32>,
    /// Buttons of the bars that have fixed button lists (the directory and
    /// edit bars), by bar, for this frame.
    pub specs: BTreeMap<&'static str, Vec<crate::views::DockButton>>,
    /// The action a directory or edit bar button asked for this frame.
    pub clicked: Option<&'static str>,
    /// The bar whose grip or title is under the pointer.
    pub hot: Option<String>,
    /// The icon each toolbar button drew, by action, for the test that
    /// checks `toolbars::button_icon`.
    #[cfg(test)]
    pub drawn_icons: BTreeMap<String, crate::chrome::Icon>,
}

/// The bars and everything about where they are.
#[derive(Default)]
pub struct Bars {
    pub layout: BarLayout,
    pub rt: Runtime,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(layout: &BarLayout, side: Side) -> Vec<Vec<String>> {
        layout.rows(side).clone()
    }

    fn every_bar_once(layout: &BarLayout) {
        let mut names = layout.names();
        names.sort();
        let mut expect: Vec<String> = menu::BUILT_TOOLBARS.iter().map(|s| s.to_string()).collect();
        expect.push(MENU_BAR.into());
        expect.sort();
        assert_eq!(names, expect, "each bar is somewhere exactly once");
    }

    #[test]
    fn the_default_layout_places_every_bar_once() {
        let layout = BarLayout::default();
        every_bar_once(&layout);
        assert_eq!(layout.rows(Side::Top).len(), 3);
        assert_eq!(layout.rows(Side::Top)[0], vec![MENU_BAR]);
        assert_eq!(layout.rows(Side::Left)[0][0], "Standard");
        assert_eq!(layout.rows(Side::Right)[0][2], "Move");
        assert!(layout.rows(Side::Bottom).is_empty());
    }

    #[test]
    fn moving_within_a_row_reorders() {
        let mut layout = BarLayout::default();
        // "Print" before "Tools" in the first toolbar row: pre-removal index
        // of "Tools" is 4, so insert at 4 puts Print right before Tools.
        layout.apply(
            "Print",
            Drop::Row(Slot {
                side: Side::Top,
                row: 1,
                index: 4,
            }),
        );
        let row = &names(&layout, Side::Top)[1];
        assert_eq!(
            row[..5],
            ["Top Layout", "Bottom Layout", "Layout", "Print", "Tools"]
        );
        every_bar_once(&layout);
    }

    #[test]
    fn a_bar_can_move_to_another_row_or_a_new_row() {
        let mut layout = BarLayout::default();
        layout.apply(
            "Display",
            Drop::Row(Slot {
                side: Side::Top,
                row: 2,
                index: 0,
            }),
        );
        assert_eq!(layout.rows(Side::Top)[2][0], "Display");
        assert!(!layout.rows(Side::Top)[1].contains(&"Display".to_string()));
        layout.apply(
            "Print",
            Drop::NewRow {
                side: Side::Top,
                row: 1,
            },
        );
        assert_eq!(layout.rows(Side::Top).len(), 4);
        assert_eq!(layout.rows(Side::Top)[1], vec!["Print"]);
        every_bar_once(&layout);
        // A bar alone in its row that opens a row next to itself stays put.
        let before = layout.clone();
        layout.apply(
            "Print",
            Drop::NewRow {
                side: Side::Top,
                row: 2,
            },
        );
        assert_eq!(layout, before);
        // Emptied rows go away.
        let mut layout = BarLayout::default();
        layout.apply(
            MENU_BAR,
            Drop::Row(Slot {
                side: Side::Top,
                row: 1,
                index: 0,
            }),
        );
        assert_eq!(layout.rows(Side::Top).len(), 2);
        assert_eq!(layout.rows(Side::Top)[0][0], MENU_BAR);
    }

    #[test]
    fn floating_remembers_home_and_redocks_there() {
        let mut layout = BarLayout::default();
        layout.float("Layout", Pos2::new(200., 300.), 100.);
        assert!(layout.is_floating("Layout"));
        assert_eq!(layout.slot_of("Layout"), None);
        assert_eq!(
            layout.floating["Layout"].home,
            Slot {
                side: Side::Top,
                row: 1,
                index: 3
            }
        );
        every_bar_once(&layout);
        layout.redock("Layout");
        assert_eq!(layout, BarLayout::default());
        // A bar alone in its row returns to a row of its own.
        layout.float(MENU_BAR, Pos2::new(10., 10.), 337.);
        assert_eq!(layout.rows(Side::Top).len(), 2);
        layout.redock(MENU_BAR);
        assert_eq!(layout, BarLayout::default());
        // Its home row has gone: it still comes back to the edge.
        layout.float("Standard", Pos2::ZERO, 80.);
        layout.float("Enterprise", Pos2::ZERO, 80.);
        layout.float("Project", Pos2::ZERO, 80.);
        assert!(layout.rows(Side::Left).is_empty());
        layout.redock("Project");
        assert_eq!(layout.rows(Side::Left), &vec![vec!["Project".to_string()]]);
    }

    #[test]
    fn saved_form_round_trips_and_rejects_garbage() {
        let mut layout = BarLayout::default();
        layout.apply(
            "Move",
            Drop::NewRow {
                side: Side::Bottom,
                row: 0,
            },
        );
        layout.float("Find", Pos2::new(120.5, 80.), 90.);
        let json = layout.to_json();
        assert_eq!(BarLayout::from_json(&json), layout);
        for garbage in [
            serde_json::json!(null),
            serde_json::json!("x"),
            serde_json::json!(7),
            serde_json::json!([1, 2]),
            serde_json::json!({"top": "no", "left": [[1, null, {}]], "floating": [1]}),
            serde_json::json!({"top": [["Nope", "Nope2"]], "floating": {"Nope": {"x": 1, "y": 2}}}),
        ] {
            let parsed = BarLayout::from_json(&garbage);
            every_bar_once(&parsed);
        }
        assert_eq!(
            BarLayout::from_json(&serde_json::json!(null)),
            BarLayout::default()
        );
        // Repeats keep the first place; unknown names are dropped; bad or
        // far-off floating positions are ignored; sizes are clamped.
        let parsed = BarLayout::from_json(&serde_json::json!({
            "top": [["Print", "Print", "Bogus"]],
            "right": [["Print"]],
            "floating": {
                "Find": {"x": 5, "y": 9, "w": 99999},
                "Edit": {"x": 1e30, "y": 2},
                "Assign": {"x": "a", "y": 2},
                "Move": {"x": 3, "y": 4, "w": -5, "edge": "nowhere", "row": 9999}
            }
        }));
        every_bar_once(&parsed);
        assert_eq!(parsed.rows(Side::Top)[0][0], "Print");
        assert_eq!(parsed.names().iter().filter(|n| *n == "Print").count(), 1);
        assert_eq!(parsed.floating["Find"].width, MAX_FLOAT_WIDTH);
        assert!(!parsed.is_floating("Edit") && !parsed.is_floating("Assign"));
        assert_eq!(parsed.floating["Move"].width, MIN_FLOAT_WIDTH);
        assert!(parsed.floating["Move"].home.row <= MAX_PER_ROW);
        // Rows past the limit are cut.
        let many: Vec<_> = menu::BUILT_TOOLBARS
            .iter()
            .map(|name| serde_json::json!([name]))
            .collect();
        let parsed = BarLayout::from_json(&serde_json::json!({ "top": many }));
        every_bar_once(&parsed);
        assert!(parsed.rows(Side::Top).len() <= MAX_ROWS);
    }

    fn geo() -> Geo {
        // A 1000 x 600 window: top dock of two 28 px rows, left dock of one
        // 28 px column, nothing on the right or bottom.
        let mut geo = Geo::new(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 582.)));
        let bar = |name: &str, index: usize, x: f32, y: f32, w: f32| BarGeo {
            name: name.into(),
            index,
            rect: Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, 28.)),
        };
        geo.dock[0] = Some(Rect::from_min_max(Pos2::ZERO, Pos2::new(1000., 56.)));
        geo.rows[0] = vec![
            RowGeo {
                row: 0,
                rect: Rect::from_min_max(Pos2::ZERO, Pos2::new(1000., 28.)),
                bars: vec![bar("A", 0, 0., 0., 100.), bar("B", 1, 100., 0., 100.)],
            },
            RowGeo {
                row: 1,
                rect: Rect::from_min_max(Pos2::new(0., 28.), Pos2::new(1000., 56.)),
                bars: vec![bar("C", 0, 0., 28., 100.)],
            },
        ];
        geo.dock[2] = Some(Rect::from_min_max(Pos2::new(0., 56.), Pos2::new(28., 582.)));
        geo.rows[2] = vec![RowGeo {
            row: 0,
            rect: Rect::from_min_max(Pos2::new(0., 56.), Pos2::new(28., 582.)),
            bars: vec![],
        }];
        geo
    }

    #[test]
    fn a_pointer_picks_a_row_a_place_or_a_new_row() {
        let geo = geo();
        let layout = BarLayout::default();
        let at = |x: f32, y: f32| geo.drop_at(&layout, "Z", Pos2::new(x, y));
        // Inside row 0, left of A's middle, between A and B, right of B.
        let row0 = |index| {
            Some(Drop::Row(Slot {
                side: Side::Top,
                row: 0,
                index,
            }))
        };
        assert_eq!(at(20., 14.), row0(0));
        assert_eq!(at(120., 14.), row0(1));
        assert_eq!(at(600., 14.), row0(2));
        // Near the line between rows 0 and 1: a new row.
        assert_eq!(
            at(300., 27.),
            Some(Drop::NewRow {
                side: Side::Top,
                row: 1
            })
        );
        // Just under the last row: a new row at the end.
        assert_eq!(
            at(300., 60.),
            Some(Drop::NewRow {
                side: Side::Top,
                row: layout.rows(Side::Top).len()
            })
        );
        // The dragged bar is not its own neighbor.
        assert_eq!(
            geo.drop_at(&layout, "A", Pos2::new(120., 14.)),
            row0(1),
            "B is the only other bar; 120 is before its middle: index 1"
        );
        // Far from every dock: floats.
        assert_eq!(at(600., 300.), None);
        // An empty right dock takes a bar at the window edge.
        assert_eq!(
            at(995., 300.),
            Some(Drop::NewRow {
                side: Side::Right,
                row: 1
            })
        );
        // The outline sits in the row at the place.
        let outline = geo.outline(row0(1).unwrap(), "Z", Vec2::new(80., 28.));
        assert_eq!(outline.min, Pos2::new(100., 0.));
        assert_eq!(outline.size(), Vec2::new(80., 28.));
    }
}
