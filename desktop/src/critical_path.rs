//! View > Activity Critical Path and its toolbar: the most critical paths
//! into and out of one activity, shown as a filter on the activity table and
//! Gantt, optionally grouped into one band per path.
//!
//! Backward paths are the multiple float paths the scheduler computes with
//! the activity as the ending activity (so path 1 is what the Float Path
//! column shows as 1 for that ending activity); forward paths follow the most
//! critical successor relationships out of it (`openxer_engine::features::
//! activity_paths`). Show More / Fewer Paths change how many of each are
//! shown, Select Paths to Show hides single paths, and Enable Group and Sort
//! bands the table by path in path order. Nothing here changes the project
//! or the saved layout: turning the paths off shows the layout as it was.
//!
//! What happens past the menu and toolbar follows the documented feature
//! and is not verified against other software.
use super::*;
use crate::dialog_kit::{self as kit, Col};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// The most paths Show More Paths offers.
pub const MAX_PATHS: usize = 50;

/// Which paths through the activity are shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Paths that lead into the activity.
    Backward,
    /// Paths that lead out of it.
    Forward,
    /// Both.
    Both,
}

/// The paths as the table shows them.
#[derive(Debug)]
pub struct PathView {
    /// Changes whenever the shown paths do, so cached row builds notice.
    pub serial: u64,
    /// Activity index (in `App::shown()`) to (shown path slot, order on it).
    pub rows: HashMap<usize, (usize, usize)>,
    /// Band caption per shown path slot.
    pub labels: Vec<String>,
    /// Enable Group and Sort: one band per path.
    pub group: bool,
}

/// One computed path.
#[derive(Clone, Debug, PartialEq)]
pub struct PathInfo {
    pub label: String,
    /// Activity indexes in path order.
    pub members: Vec<usize>,
}

/// What is shown: the activity the paths run through and the direction.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub task_id: i64,
    pub direction: Direction,
    /// Paths turned off in Select Paths to Show (indexes into the computed list).
    pub hidden: BTreeSet<usize>,
}

/// The Select Paths to Show dialog while open: one check per path.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectPaths {
    pub paths: Vec<(String, usize)>,
    pub checked: Vec<bool>,
    pub selected: Option<usize>,
}

type Cached = (CacheKey, Rc<Vec<PathInfo>>, Rc<PathView>);
type CacheKey = (
    i64,
    Direction,
    usize,
    BTreeSet<usize>,
    bool,
    u64,
    u64,
    usize,
);

pub struct CriticalPathState {
    /// Paths of each direction to show (the number in the menu).
    pub count: usize,
    /// Enable Group and Sort.
    pub group: bool,
    pub shown: Option<Shown>,
    pub select: Option<SelectPaths>,
    cache: RefCell<Option<Cached>>,
    serial: Cell<u64>,
}

impl Default for CriticalPathState {
    fn default() -> Self {
        Self {
            count: 1,
            group: false,
            shown: None,
            select: None,
            cache: RefCell::new(None),
            serial: Cell::new(0),
        }
    }
}

/// The paths through activity `seed` of `model`, labelled, backward ones first.
pub fn compute(
    model: &Model,
    seed: usize,
    direction: Direction,
    count: usize,
) -> Result<Vec<PathInfo>, String> {
    let mut out = vec![];
    let mut add = |forward: bool, name: &str| -> Result<(), String> {
        let paths = openxer_engine::features::activity_paths(model, seed, forward, count)?;
        out.extend(paths.into_iter().enumerate().map(|(n, members)| PathInfo {
            label: format!("{name} Path {}", n + 1),
            members,
        }));
        Ok(())
    };
    if direction != Direction::Forward {
        add(false, "Backward")?;
    }
    if direction != Direction::Backward {
        add(true, "Forward")?;
    }
    Ok(out)
}

/// The table rows of the paths not hidden; an activity on more than one
/// path stays on the first.
pub fn view(paths: &[PathInfo], hidden: &BTreeSet<usize>, group: bool, serial: u64) -> PathView {
    let mut rows = HashMap::new();
    let mut labels = vec![];
    for (n, path) in paths.iter().enumerate() {
        if hidden.contains(&n) {
            continue;
        }
        let slot = labels.len();
        labels.push(path.label.clone());
        for (order, &i) in path.members.iter().enumerate() {
            rows.entry(i).or_insert((slot, order));
        }
    }
    PathView {
        serial,
        rows,
        labels,
        group,
    }
}

impl App {
    fn cp_key(&self) -> Option<CacheKey> {
        let st = &self.critical_path;
        let shown = st.shown.as_ref()?;
        Some((
            shown.task_id,
            shown.direction,
            st.count,
            shown.hidden.clone(),
            st.group,
            self.history.changes(),
            self.records_revision,
            self.shown().model.activities.len(),
        ))
    }

    /// The computed paths and the view of them, rebuilt when the schedule,
    /// the activity or the choices change.
    fn cp_paths(&self) -> Option<(Rc<Vec<PathInfo>>, Rc<PathView>)> {
        let key = self.cp_key()?;
        let st = &self.critical_path;
        if let Some((k, paths, view)) = st.cache.borrow().as_ref() {
            if *k == key {
                return Some((paths.clone(), view.clone()));
            }
        }
        let shown = st.shown.as_ref()?;
        let model = &self.shown().model;
        let seed = model
            .activities
            .iter()
            .position(|a| a.task_id == shown.task_id)?;
        let paths = Rc::new(compute(model, seed, shown.direction, st.count).ok()?);
        st.serial.set(st.serial.get() + 1);
        let view = Rc::new(view(&paths, &shown.hidden, st.group, st.serial.get()));
        *st.cache.borrow_mut() = Some((key, paths.clone(), view.clone()));
        Some((paths, view))
    }

    /// The paths the table shows now, if any (for `grid_index`).
    pub(crate) fn critical_path_view(&self) -> Option<Rc<PathView>> {
        self.cp_paths().map(|(_, v)| v)
    }

    /// The computed paths through the activity, in menu order.
    pub(crate) fn critical_paths(&self) -> Vec<PathInfo> {
        self.cp_paths()
            .map(|(p, _)| p.as_ref().clone())
            .unwrap_or_default()
    }

    /// The name of the path filter in force, for the layout bar, while
    /// critical paths show.
    pub(crate) fn critical_path_filter_name(&self) -> Option<String> {
        let shown = self.critical_path.shown.as_ref()?;
        Some(
            match shown.direction {
                Direction::Backward => "Backward Activity Critical Paths",
                Direction::Forward => "Forward Activity Critical Paths",
                Direction::Both => "Activity Critical Paths",
            }
            .into(),
        )
    }

    fn cp_direction(&self) -> Option<Direction> {
        self.critical_path.shown.as_ref().map(|s| s.direction)
    }

    /// The activity the paths would run through: the selected one.
    fn cp_seed(&self) -> Option<(i64, usize)> {
        let model = &self.shown().model;
        let i = self.selected.filter(|&i| i < model.activities.len())?;
        Some((model.activities[i].task_id, i))
    }

    /// Show Backward / Forward / Activity Critical Paths: show the paths of
    /// `direction` through the selected activity; the same command again
    /// turns them off.
    pub(crate) fn toggle_critical_paths(&mut self, direction: Direction) {
        if self.cp_direction() == Some(direction) {
            self.hide_critical_paths();
            return;
        }
        let seed = match &self.critical_path.shown {
            Some(s) => self
                .shown()
                .model
                .activities
                .iter()
                .position(|a| a.task_id == s.task_id)
                .map(|i| (s.task_id, i)),
            None => self.cp_seed(),
        };
        let Some((task_id, i)) = seed else {
            self.status = "Select an activity to show its critical paths.".into();
            return;
        };
        if let Err(e) = compute(&self.shown().model, i, direction, 1) {
            self.status = e;
            return;
        }
        self.critical_path.shown = Some(Shown {
            task_id,
            direction,
            hidden: BTreeSet::new(),
        });
        self.drop_frame_cache();
        let count = self.critical_paths().len();
        self.status = format!(
            "{count} activity critical path(s) through {}",
            self.shown().model.activities[i].task_code
        );
    }

    /// Turn the paths off; the table shows the layout as before.
    pub(crate) fn hide_critical_paths(&mut self) {
        self.critical_path.shown = None;
        self.critical_path.select = None;
        *self.critical_path.cache.borrow_mut() = None;
        self.drop_frame_cache();
        self.status = "Activity critical paths hidden".into();
    }

    /// Show More Paths (`more`) or Show Fewer Paths.
    pub(crate) fn change_path_count(&mut self, more: bool) {
        let st = &mut self.critical_path;
        st.count = if more {
            (st.count + 1).min(MAX_PATHS)
        } else {
            st.count.saturating_sub(1).max(1)
        };
        if let Some(s) = st.shown.as_mut() {
            s.hidden.retain(|_| false);
        }
        self.drop_frame_cache();
        self.status = format!("Activity critical paths: {}", self.critical_path.count);
    }

    /// Enable Group and Sort.
    pub(crate) fn toggle_path_grouping(&mut self) {
        self.critical_path.group = !self.critical_path.group;
        self.drop_frame_cache();
    }

    /// Select Paths to Show.
    pub(crate) fn open_select_paths(&mut self) {
        let Some(shown) = &self.critical_path.shown else {
            return;
        };
        let hidden = shown.hidden.clone();
        let paths: Vec<(String, usize)> = self
            .critical_paths()
            .into_iter()
            .map(|p| (p.label, p.members.len()))
            .collect();
        self.critical_path.select = Some(SelectPaths {
            checked: (0..paths.len()).map(|n| !hidden.contains(&n)).collect(),
            paths,
            selected: None,
        });
    }

    /// The View > Activity Critical Path submenu.
    pub(crate) fn critical_path_items(&mut self, ui: &mut egui::Ui) {
        use chrome::{Icon, Item};
        let activities = self.views.main.is_activities();
        let shown = self.critical_path.shown.is_some();
        let can_show = activities && (shown || self.cp_seed().is_some());
        let count = self.critical_path.count;
        let more = Item::new("Show More Paths")
            .icon(Icon::AcpMore)
            .enabled(activities && count < MAX_PATHS);
        if self.item(ui, "menu_acp_more", more) {
            self.change_path_count(true);
        }
        let fewer = Item::new("Show Fewer Paths")
            .icon(Icon::AcpFewer)
            .enabled(activities && count > 1);
        if self.item(ui, "menu_acp_fewer", fewer) {
            self.change_path_count(false);
        }
        let select = Item::new("Select Paths to Show")
            .icon(Icon::AcpSelect)
            .enabled(activities && shown);
        if self.item(ui, "menu_acp_select", select) {
            self.open_select_paths();
        }
        let group = Item::new("Enable Group and Sort")
            .icon(Icon::AcpGroup)
            .enabled(activities)
            .checked(self.critical_path.group);
        if self.item(ui, "menu_acp_group", group) {
            self.toggle_path_grouping();
        }
        let current = self.cp_direction();
        for (action, label, icon, direction) in [
            (
                "menu_acp_backward",
                "Show Backward Activity Critical Paths",
                Icon::AcpBack,
                Direction::Backward,
            ),
            (
                "menu_acp_forward",
                "Show Forward Activity Critical Paths",
                Icon::AcpForward,
                Direction::Forward,
            ),
            (
                "menu_acp_show",
                "Show Activity Critical Paths",
                Icon::AcpShow,
                Direction::Both,
            ),
        ] {
            let item = Item::new(label)
                .icon(icon)
                .enabled(can_show)
                .checked(current == Some(direction));
            if self.item(ui, action, item) {
                self.toggle_critical_paths(direction);
            }
        }
    }

    /// The Activity Critical Path toolbar (off until View > Toolbars turns it on).
    pub(crate) fn critical_path_bar(&mut self, ui: &mut egui::Ui) {
        use chrome::Icon;
        let activities = self.views.main.is_activities();
        let shown = self.critical_path.shown.is_some();
        let can_show = activities && (shown || self.cp_seed().is_some());
        let count = self.critical_path.count;
        let current = self.cp_direction();
        if self.tool(
            ui,
            "tool_acp_more",
            Icon::AcpMore,
            "Show More Paths",
            activities && count < MAX_PATHS,
        ) {
            self.change_path_count(true);
        }
        if self.tool(
            ui,
            "tool_acp_fewer",
            Icon::AcpFewer,
            "Show Fewer Paths",
            activities && count > 1,
        ) {
            self.change_path_count(false);
        }
        if self.tool(
            ui,
            "tool_acp_select",
            Icon::AcpSelect,
            "Select Paths to Show",
            activities && shown,
        ) {
            self.open_select_paths();
        }
        if self.tool_ex(
            ui,
            "tool_acp_group",
            Icon::AcpGroup,
            "Enable Group and Sort",
            activities,
            self.critical_path.group,
            false,
        ) {
            self.toggle_path_grouping();
        }
        for (action, tip, icon, direction) in [
            (
                "tool_acp_backward",
                "Show Backward Activity Critical Paths",
                Icon::AcpBack,
                Direction::Backward,
            ),
            (
                "tool_acp_forward",
                "Show Forward Activity Critical Paths",
                Icon::AcpForward,
                Direction::Forward,
            ),
            (
                "tool_acp_show",
                "Show Activity Critical Paths",
                Icon::AcpShow,
                Direction::Both,
            ),
        ] {
            if self.tool_ex(
                ui,
                action,
                icon,
                tip,
                can_show,
                current == Some(direction),
                false,
            ) {
                self.toggle_critical_paths(direction);
            }
        }
    }

    /// Select Paths to Show: a check per path; OK applies, Cancel and
    /// Escape leave the shown paths alone.
    pub(crate) fn select_paths_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.critical_path.select.take() else {
            return;
        };
        let (w, h) = (424., 262.);
        let (clicked, closed, actions) = kit::window(
            ctx,
            "acp_select",
            "Select Paths to Show",
            Vec2::new(w, h),
            |c| {
                c.fill(0., 0., w, h, kit::STRIP);
                c.fill(5., 5., 302., h - 10., kit::FACE);
                c.lowered(5., 5., 302., h - 10.);
                kit::heading(c, 6., 6., 300., "Paths to show");
                let rows = d.paths.len();
                let mut toggled = None;
                let out = kit::list(
                    c,
                    "acp_select",
                    (10., 33., 292., h - 48.),
                    None,
                    &[
                        Col {
                            title: "Show",
                            width: 44.,
                        },
                        Col {
                            title: "Path",
                            width: 150.,
                        },
                        Col {
                            title: "Activities",
                            width: 78.,
                        },
                    ],
                    rows,
                    d.selected,
                    None,
                    &mut |c, i, col, rect, sel| match col {
                        0 => {
                            let at = rect.center() - c.origin.to_vec2() - Vec2::splat(6.5);
                            kit::check_glyph(c, at.x.round(), at.y.round(), d.checked[i], true);
                            if c.hit(&format!("acp_select_check_{i}"), rect, true)
                                .clicked()
                            {
                                toggled = Some(i);
                            }
                        }
                        1 => kit::cell_text(c, rect, &d.paths[i].0, sel, 4.),
                        _ => kit::cell_text(c, rect, &d.paths[i].1.to_string(), sel, 4.),
                    },
                );
                if let Some(i) = toggled {
                    d.checked[i] = !d.checked[i];
                }
                if let Some(i) = out.clicked {
                    d.selected = Some(i);
                }
                kit::button_column(
                    c,
                    w,
                    &[
                        kit::Entry::ok("acp_select_ok"),
                        kit::Entry::cancel("acp_select_cancel"),
                        kit::Entry::Button("acp_select_all", "Select All", true, true),
                    ],
                )
                .clicked
            },
        );
        self.record_dialog_actions(actions);
        match clicked.as_deref() {
            Some("acp_select_ok") => {
                if let Some(s) = self.critical_path.shown.as_mut() {
                    s.hidden = d
                        .checked
                        .iter()
                        .enumerate()
                        .filter(|(_, on)| !**on)
                        .map(|(n, _)| n)
                        .collect();
                }
                self.drop_frame_cache();
                return;
            }
            Some("acp_select_cancel") => return,
            Some("acp_select_all") => d.checked.iter_mut().for_each(|c| *c = true),
            _ => {}
        }
        if !closed {
            self.critical_path.select = Some(d);
        }
    }
}

#[cfg(test)]
mod tests;
