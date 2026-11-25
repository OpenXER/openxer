//! Tab groups of the main window's view area. Every open view is a tab in
//! exactly one group; the focused group's active tab is the current view.
//! View > Tab Groups splits the area: New Horizontal Tab Group stacks a new
//! group under the current one, New Vertical Tab Group puts it to the right,
//! each taking the current tab with it. All groups share one direction, so
//! the other split command is off while more than one group is open. Merge
//! All Tab Groups puts every tab back into one group.
use super::MainView;
use eframe::egui::{Pos2, Rect};

/// Width (or height) of the splitter between two groups.
pub const SPLITTER: f32 = 6.;
/// Smallest group size along the split direction.
pub const MIN_GROUP: f32 = 180.;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Split {
    /// Groups stacked top to bottom.
    Horizontal,
    /// Groups side by side.
    Vertical,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TabGroup {
    pub tabs: Vec<MainView>,
    pub active: usize,
}

impl TabGroup {
    pub fn view(&self) -> MainView {
        self.tabs[self.active.min(self.tabs.len() - 1)]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TabGroups {
    /// Never empty, and no group is empty.
    pub groups: Vec<TabGroup>,
    pub focused: usize,
    pub split: Split,
    /// Each group's share of the area along the split direction (sums to 1).
    pub shares: Vec<f32>,
}

impl TabGroups {
    pub fn new(tabs: Vec<MainView>, active: MainView) -> Self {
        let active = tabs.iter().position(|v| *v == active).unwrap_or(0);
        Self {
            groups: vec![TabGroup { tabs, active }],
            focused: 0,
            split: Split::Vertical,
            shares: vec![1.],
        }
    }

    /// The focused group's active view.
    pub fn current(&self) -> MainView {
        self.groups[self.focused].view()
    }

    /// Every tab, group by group.
    pub fn tabs(&self) -> Vec<MainView> {
        self.groups.iter().flat_map(|g| g.tabs.clone()).collect()
    }

    /// Group and tab position of `view`.
    pub fn find(&self, view: MainView) -> Option<(usize, usize)> {
        self.groups.iter().enumerate().find_map(|(g, group)| {
            group
                .tabs
                .iter()
                .position(|v| *v == view)
                .map(|tab| (g, tab))
        })
    }

    /// Show `view`: activate its tab and focus its group, or add a tab for
    /// it at the end of the focused group.
    pub fn open(&mut self, view: MainView) {
        match self.find(view) {
            Some((g, tab)) => {
                self.focused = g;
                self.groups[g].active = tab;
            }
            None => {
                let group = &mut self.groups[self.focused];
                group.tabs.push(view);
                group.active = group.tabs.len() - 1;
            }
        }
    }

    /// Can the current tab move into a new group split this way?
    pub fn can_split(&self, split: Split) -> bool {
        self.groups[self.focused].tabs.len() > 1 && (self.groups.len() == 1 || self.split == split)
    }

    /// New Horizontal / Vertical Tab Group: the current tab moves into a new
    /// group after the focused one, which takes half of its space.
    pub fn split_current(&mut self, split: Split) -> bool {
        if !self.can_split(split) {
            return false;
        }
        let g = self.focused;
        let group = &mut self.groups[g];
        let view = group.tabs.remove(group.active.min(group.tabs.len() - 1));
        group.active = group.active.saturating_sub(1).min(group.tabs.len() - 1);
        self.groups.insert(
            g + 1,
            TabGroup {
                tabs: vec![view],
                active: 0,
            },
        );
        let half = self.shares[g] / 2.;
        self.shares[g] = half;
        self.shares.insert(g + 1, half);
        self.split = split;
        self.focused = g + 1;
        true
    }

    pub fn can_merge(&self) -> bool {
        self.groups.len() > 1
    }

    /// Merge All Tab Groups: every tab in one group, group order kept, the
    /// current view still active.
    pub fn merge_all(&mut self) -> bool {
        if !self.can_merge() {
            return false;
        }
        let current = self.current();
        let tabs = self.tabs();
        let active = tabs.iter().position(|v| *v == current).unwrap_or(0);
        self.groups = vec![TabGroup { tabs, active }];
        self.shares = vec![1.];
        self.focused = 0;
        true
    }

    /// Close group `g`'s active tab; the tab to its left becomes active (the
    /// new first tab when it was first). An emptied group is removed and its
    /// space goes to its neighbour. The Activities view never closes.
    #[cfg(test)]
    pub fn close_active(&mut self, g: usize) -> Option<MainView> {
        let view = self.groups.get(g)?.view();
        self.remove_view(view).then_some(view)
    }

    /// Close the tab of `view`, wherever it is. In its group, the tab to its
    /// left becomes active when it was the active one (the new first tab
    /// when it was first). An emptied group is removed and its space goes to
    /// its neighbour. The Activities view never closes.
    pub fn remove_view(&mut self, view: MainView) -> bool {
        if view == MainView::Activities {
            return false;
        }
        let Some((g, index)) = self.find(view) else {
            return false;
        };
        let group = &mut self.groups[g];
        group.tabs.remove(index);
        if index <= group.active {
            group.active = group.active.saturating_sub(1);
        }
        if group.tabs.is_empty() {
            self.groups.remove(g);
            let share = self.shares.remove(g);
            let neighbour = g.saturating_sub(1).min(self.groups.len() - 1);
            self.shares[neighbour] += share;
            if self.focused >= g && self.focused > 0 {
                self.focused -= 1;
            }
            self.focused = self.focused.min(self.groups.len() - 1);
        } else {
            group.active = group.active.min(group.tabs.len() - 1);
            self.focused = g;
        }
        true
    }

    /// Ctrl+Tab and Ctrl+Shift+Tab: the next or previous tab of the focused
    /// group, wrapping around.
    pub fn cycle(&mut self, forward: bool) -> MainView {
        let group = &mut self.groups[self.focused];
        let n = group.tabs.len();
        group.active = if forward {
            (group.active + 1) % n
        } else {
            (group.active + n - 1) % n
        };
        group.view()
    }

    /// Group rectangles and the splitters between them inside `area`.
    pub fn layout(&self, area: Rect) -> (Vec<Rect>, Vec<Rect>) {
        let n = self.groups.len();
        let vertical = self.split == Split::Vertical;
        let total = if vertical {
            area.width()
        } else {
            area.height()
        };
        let room = (total - SPLITTER * (n - 1) as f32).max(0.);
        let mut groups = Vec::with_capacity(n);
        let mut splitters = Vec::with_capacity(n.saturating_sub(1));
        let mut at = if vertical { area.left() } else { area.top() };
        for (i, share) in self.shares.iter().enumerate() {
            let end = if i + 1 == n {
                if vertical {
                    area.right()
                } else {
                    area.bottom()
                }
            } else {
                (at + room * share).round()
            };
            let rect = if vertical {
                Rect::from_min_max(Pos2::new(at, area.top()), Pos2::new(end, area.bottom()))
            } else {
                Rect::from_min_max(Pos2::new(area.left(), at), Pos2::new(area.right(), end))
            };
            groups.push(rect);
            if i + 1 < n {
                splitters.push(if vertical {
                    Rect::from_min_max(
                        Pos2::new(end, area.top()),
                        Pos2::new(end + SPLITTER, area.bottom()),
                    )
                } else {
                    Rect::from_min_max(
                        Pos2::new(area.left(), end),
                        Pos2::new(area.right(), end + SPLITTER),
                    )
                });
            }
            at = end + SPLITTER;
        }
        (groups, splitters)
    }

    /// Drag splitter `i` (between groups `i` and `i + 1`) by `delta` pixels
    /// in an area `total` pixels long; both groups keep `MIN_GROUP`.
    pub fn drag_splitter(&mut self, i: usize, delta: f32, total: f32) {
        let n = self.groups.len();
        if i + 1 >= n {
            return;
        }
        let room = (total - SPLITTER * (n - 1) as f32).max(1.);
        let pair = self.shares[i] + self.shares[i + 1];
        let min = (MIN_GROUP / room).min(pair / 2.);
        let first = (self.shares[i] + delta / room).clamp(min, pair - min);
        self.shares[i] = first;
        self.shares[i + 1] = pair - first;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Vec2;
    use MainView::*;

    fn two() -> TabGroups {
        TabGroups::new(vec![Activities, Projects], Activities)
    }

    #[test]
    fn open_activates_or_adds_a_tab_in_the_focused_group() {
        let mut t = two();
        t.open(Wbs);
        assert_eq!(t.tabs(), vec![Activities, Projects, Wbs]);
        assert_eq!(t.current(), Wbs);
        t.open(Activities);
        assert_eq!(t.current(), Activities);
        assert_eq!(t.groups[0].active, 0);
    }

    #[test]
    fn split_moves_the_current_tab_into_a_new_group() {
        let mut t = two();
        t.open(Projects);
        assert!(t.can_split(Split::Horizontal) && t.can_split(Split::Vertical));
        assert!(t.split_current(Split::Horizontal));
        assert_eq!(t.groups.len(), 2);
        assert_eq!(t.groups[0].tabs, vec![Activities]);
        assert_eq!(t.groups[1].tabs, vec![Projects]);
        assert_eq!((t.focused, t.current()), (1, Projects));
        assert_eq!(t.shares, vec![0.5, 0.5]);
        // One tab left in the focused group: nothing to split off.
        assert!(!t.can_split(Split::Horizontal));
        t.open(Wbs);
        assert_eq!(t.groups[1].tabs, vec![Projects, Wbs]);
        // All groups share one direction.
        assert!(!t.can_split(Split::Vertical));
        assert!(t.split_current(Split::Horizontal));
        assert_eq!(t.groups.len(), 3);
        assert_eq!(t.shares, vec![0.5, 0.25, 0.25]);
        // Opening a view in another group focuses that group.
        t.open(Activities);
        assert_eq!((t.focused, t.current()), (0, Activities));
    }

    #[test]
    fn merge_keeps_tab_order_and_the_current_view() {
        let mut t = two();
        t.open(Wbs);
        t.split_current(Split::Vertical);
        t.open(Resources);
        t.open(Activities);
        assert!(t.can_merge());
        assert!(t.merge_all());
        assert_eq!(t.groups.len(), 1);
        assert_eq!(t.tabs(), vec![Activities, Projects, Wbs, Resources]);
        assert_eq!(t.current(), Activities);
        assert_eq!(t.shares, vec![1.]);
        assert!(!t.can_merge() && !t.merge_all());
    }

    #[test]
    fn close_activates_the_tab_to_the_left_and_drops_empty_groups() {
        let mut t = TabGroups::new(vec![Activities, Projects, Wbs, Resources], Wbs);
        assert_eq!(t.close_active(0), Some(Wbs));
        assert_eq!(t.current(), Projects);
        t.groups[0].active = 0;
        assert_eq!(t.close_active(0), None, "Activities stays open");
        t.open(Resources);
        t.split_current(Split::Vertical);
        assert_eq!(t.groups.len(), 2);
        assert_eq!(t.close_active(1), Some(Resources));
        assert_eq!(t.groups.len(), 1);
        assert_eq!(t.shares, vec![1.]);
        assert_eq!(t.focused, 0);
        assert_eq!(t.tabs(), vec![Activities, Projects]);
        // Closing the first tab activates the new first tab.
        let mut t = TabGroups::new(vec![Projects, Wbs], Projects);
        t.close_active(0);
        assert_eq!(t.current(), Wbs);
    }

    #[test]
    fn cycle_wraps_inside_the_focused_group() {
        let mut t = TabGroups::new(vec![Activities, Projects, Wbs], Activities);
        assert_eq!(t.cycle(true), Projects);
        assert_eq!(t.cycle(true), Wbs);
        assert_eq!(t.cycle(true), Activities);
        assert_eq!(t.cycle(false), Wbs);
    }

    #[test]
    fn layout_and_splitter_drag_keep_both_groups_usable() {
        let mut t = two();
        let area = Rect::from_min_size(Pos2::new(36., 81.), Vec2::new(1000., 700.));
        let (groups, splitters) = t.layout(area);
        assert_eq!(groups, vec![area]);
        assert!(splitters.is_empty());
        t.split_current(Split::Vertical);
        let (groups, splitters) = t.layout(area);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].left(), 36.);
        assert_eq!(groups[0].right(), splitters[0].left());
        assert_eq!(splitters[0].width(), SPLITTER);
        assert_eq!(groups[1].left(), splitters[0].right());
        assert_eq!(groups[1].right(), area.right());
        assert_eq!(groups[0].width(), 497.);
        t.drag_splitter(0, 100., 1000.);
        let (groups, _) = t.layout(area);
        assert_eq!(groups[0].width(), 597.);
        t.drag_splitter(0, 5000., 1000.);
        let (groups, _) = t.layout(area);
        assert_eq!(groups[1].width(), MIN_GROUP);
        assert!((t.shares.iter().sum::<f32>() - 1.).abs() < 1e-5);
        t.merge_all();
        t.split_current(Split::Horizontal);
        let (groups, splitters) = t.layout(area);
        assert_eq!(groups[0].top(), 81.);
        assert_eq!(splitters[0].height(), SPLITTER);
        assert_eq!(groups[1].bottom(), area.bottom());
    }
}
