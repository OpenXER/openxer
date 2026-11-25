//! File > Open Project (workspace projects under their EPS row, several at
//! once) and Project > Set Default Project.
use super::*;
use crate::dialog_kit::{self, Col, Entry};
use crate::schedule_dialogs::Canvas;

/// One row of the Open Project grid: an EPS folder (`folder` set) or a
/// workspace project (`id` set), with its indent level.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenRow {
    pub id: Option<i64>,
    pub folder: Option<i64>,
    pub depth: usize,
    pub project_id: String,
    pub name: String,
    pub open: bool,
}

#[derive(Clone, Debug, Default)]
pub struct OpenDialog {
    /// Every row of the tree, folders before what they hold.
    pub rows: Vec<OpenRow>,
    /// Selected workspace ids.
    pub selected: BTreeSet<i64>,
    /// Collapsed folders.
    pub collapsed: BTreeSet<i64>,
    /// Focused row, as an index into [`OpenDialog::visible`].
    pub focus: Option<usize>,
    pub anchor: Option<usize>,
}

impl OpenDialog {
    /// Rows shown (indexes into `rows`): those not under a collapsed folder.
    pub fn visible(&self) -> Vec<usize> {
        let mut out = vec![];
        let mut hidden_below: Option<usize> = None;
        for (i, r) in self.rows.iter().enumerate() {
            if hidden_below.is_some_and(|d| r.depth > d) {
                continue;
            }
            hidden_below = None;
            out.push(i);
            if r.folder.is_some_and(|f| self.collapsed.contains(&f)) {
                hidden_below = Some(r.depth);
            }
        }
        out
    }

    /// The projects of the folder at `rows[i]` and every folder under it.
    pub fn under(&self, i: usize) -> Vec<i64> {
        let depth = self.rows[i].depth;
        self.rows[i + 1..]
            .iter()
            .take_while(|r| r.depth > depth)
            .filter_map(|r| r.id)
            .collect()
    }

    /// Show or hide what the folder at shown row `r` holds.
    pub fn toggle(&mut self, r: usize) {
        let Some(folder) = self.visible().get(r).and_then(|&i| self.rows[i].folder) else {
            return;
        };
        if !self.collapsed.remove(&folder) {
            self.collapsed.insert(folder);
        }
    }

    /// A click on shown row `r` with Ctrl (`toggle`) or Shift (`range`). A
    /// folder picks every project under it, child folders included.
    pub fn click(&mut self, r: usize, toggle: bool, range: bool) {
        let shown = self.visible();
        let Some(&i) = shown.get(r) else {
            return;
        };
        let ids: Vec<i64> = if self.rows[i].folder.is_some() {
            self.under(i)
        } else if range {
            let a = self.anchor.unwrap_or(r).min(shown.len() - 1);
            shown[a.min(r)..=a.max(r)]
                .iter()
                .flat_map(|&k| match self.rows[k].id {
                    Some(id) => vec![id],
                    None => self.under(k),
                })
                .collect()
        } else {
            self.rows[i].id.into_iter().collect()
        };
        if toggle {
            let all = !ids.is_empty() && ids.iter().all(|id| self.selected.contains(id));
            for id in ids {
                if all {
                    self.selected.remove(&id);
                } else {
                    self.selected.insert(id);
                }
            }
        } else {
            self.selected = ids.into_iter().collect();
        }
        if !range {
            self.anchor = Some(r);
        }
        self.focus = Some(r);
    }
    /// Selected ids in tree order.
    pub fn chosen(&self) -> Vec<i64> {
        let mut seen = BTreeSet::new();
        self.rows
            .iter()
            .filter_map(|r| r.id)
            .filter(|id| self.selected.contains(id) && seen.insert(*id))
            .collect()
    }
}
#[derive(Clone, Debug, Default)]
pub struct DefaultDialog {
    /// (workspace id, project ID, project name).
    pub rows: Vec<(i64, String, String)>,
    pub default: Option<i64>,
    pub focus: Option<usize>,
}

impl App {
    /// File > Open Project...: the workspace projects under their EPS
    /// folders.
    pub(crate) fn open_project_dialog(&mut self) {
        let open = self.open_ids();
        let projects: Vec<_> = self
            .workspace_projects()
            .into_iter()
            .filter(|s| s.store_id.is_some())
            .collect();
        let keys: Vec<i64> = projects.iter().filter_map(|s| s.store_id).collect();
        let ws = self.workspace().clone();
        let groups = ws.grouped(&keys);
        let mut rows = vec![];
        for line in crate::views::projects::eps_lines(&groups, &BTreeSet::new()) {
            match line {
                crate::views::projects::EpsLine::Eps(id, depth) => rows.push(OpenRow {
                    id: None,
                    folder: Some(id),
                    depth,
                    project_id: ws.eps_value(id, "eps_name"),
                    name: String::new(),
                    open: false,
                }),
                crate::views::projects::EpsLine::Project(id, depth) => {
                    let Some(s) = projects.iter().find(|s| s.store_id == Some(id)) else {
                        continue;
                    };
                    rows.push(OpenRow {
                        id: Some(id),
                        folder: None,
                        depth,
                        project_id: s.id.clone(),
                        name: s.name.clone(),
                        open: open.contains(&id),
                    });
                }
            }
        }
        let selected = open.iter().copied().collect();
        let mut d = OpenDialog {
            rows,
            selected,
            ..Default::default()
        };
        let focus = d
            .visible()
            .iter()
            .position(|&i| d.rows[i].id == self.open_set.active);
        d.focus = focus;
        d.anchor = focus;
        self.open_set.open_dialog = Some(d);
    }
    /// Project > Set Default Project...
    pub(crate) fn default_project_dialog(&mut self) {
        let rows = self
            .open_projects()
            .into_iter()
            .zip(self.open_ids())
            .map(|(p, id)| {
                (
                    id,
                    p.model.project.name.clone(),
                    crate::views::project_title(p),
                )
            })
            .collect::<Vec<_>>();
        let mut rows = rows;
        rows.sort_by(|a, b| a.1.cmp(&b.1));
        let default = self.open_set.default.or(self.open_set.active);
        let focus = rows.iter().position(|r| Some(r.0) == default);
        self.open_set.default_dialog = Some(DefaultDialog {
            rows,
            default,
            focus,
        });
    }

    pub(crate) fn open_set_dialogs(&mut self, ctx: &egui::Context) {
        self.open_project_window(ctx);
        self.default_project_window(ctx);
    }

    fn open_project_window(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.open_set.open_dialog.take() else {
            return;
        };
        let modifiers = ctx.input(|i| i.modifiers);
        let mut open = false;
        let mut cancel = false;
        let mut from_file = false;
        let mut help = false;
        let ((), closed, actions) = dialog_kit::window(
            ctx,
            "open_project",
            "Open Project",
            Vec2::new(575., 353.),
            |c: &mut Canvas| {
                c.fill(0., 0., 575., 353., chrome::pal::FACE);
                let cols = [
                    Col {
                        title: "Project ID",
                        width: 170.,
                    },
                    Col {
                        title: "Project Name",
                        width: 255.,
                    },
                ];
                let shown = d.visible();
                let rows: Vec<OpenRow> = shown.iter().map(|&i| d.rows[i].clone()).collect();
                let selected = d.selected.clone();
                let collapsed = d.collapsed.clone();
                let mut expand = None;
                let out = dialog_kit::list(
                    c,
                    "open_project_grid",
                    (10., 10., 445., 333.),
                    Some("Display: All Projects"),
                    &cols,
                    rows.len(),
                    d.focus,
                    None,
                    &mut |c, r, k, rect, focused| {
                        let row = &rows[r];
                        let folder = row.folder.is_some();
                        let picked = focused || row.id.is_some_and(|id| selected.contains(&id));
                        if picked && !focused {
                            c.ui.painter().rect_filled(
                                rect.shrink2(Vec2::new(0., 0.5)),
                                0.,
                                dialog_kit::SELECTED,
                            );
                        }
                        let indent = row.depth as f32 * 14.;
                        if k == 0 && folder {
                            // The folder's +/- box; it shows or hides what
                            // the folder holds without picking it.
                            let b = Rect::from_min_size(
                                Pos2::new(
                                    rect.left() + 3. + indent,
                                    (rect.center().y - 4.5).floor(),
                                ),
                                Vec2::splat(9.),
                            );
                            let open_now = !row.folder.is_some_and(|f| collapsed.contains(&f));
                            crate::views::grid::expander(c.ui.painter(), b, open_now);
                            let hit = c.ui.interact(
                                b.expand(2.),
                                c.id("open_project_expand").with(r),
                                egui::Sense::click(),
                            );
                            c.act(&format!("open_project_expand_{r}"), b);
                            if hit.clicked() {
                                expand = Some(r);
                            }
                        }
                        let text = match (k, folder) {
                            (0, true) => row.project_id.clone(),
                            (0, false) if row.open => format!("{}  (open)", row.project_id),
                            (0, false) => row.project_id.clone(),
                            (_, true) => String::new(),
                            _ => row.name.clone(),
                        };
                        let pad = if k == 0 { 18. + indent } else { 3. };
                        dialog_kit::cell_text(c, rect, &text, picked, pad);
                    },
                );
                if let Some(r) = expand {
                    d.toggle(r);
                } else if let Some(r) = out.clicked {
                    d.click(r, modifiers.command, modifiers.shift);
                }
                if let Some(r) = out.double.filter(|_| expand.is_none()) {
                    match rows.get(r) {
                        Some(OpenRow { id: Some(id), .. }) => {
                            d.selected = [*id].into();
                            open = true;
                        }
                        // A folder opens everything under it.
                        Some(_) => {
                            d.click(r, false, false);
                            open = !d.selected.is_empty();
                        }
                        None => {}
                    }
                }
                let any = !d.selected.is_empty();
                dialog_kit::strip(c, 575., 353.);
                // An XER file goes through the file picker as before.
                let column = dialog_kit::button_column(
                    c,
                    575.,
                    &[
                        Entry::Button("open_project_open", "&Open", any, false),
                        Entry::cancel("open_project_cancel"),
                        Entry::Button("open_project_file", "&File...", true, true),
                        Entry::help("open_project_help"),
                    ],
                );
                let clicked = |key: &str| column.clicked.as_deref() == Some(key);
                open |= clicked("open_project_open");
                cancel = clicked("open_project_cancel");
                help = clicked("open_project_help");
                from_file = clicked("open_project_file");
            },
        );
        for (name, rect) in actions {
            self.action_rects
                .insert(name, [rect.left(), rect.top(), rect.right(), rect.bottom()]);
        }
        if help {
            self.open_help_for("open_project");
        }
        if from_file {
            self.open();
        } else if open {
            let ids = d.chosen();
            self.open_project_set(&ids);
            self.show_view(crate::views::MainView::Activities);
        } else if !(cancel || closed || ctx.input(|i| i.key_pressed(Key::Escape))) {
            self.open_set.open_dialog = Some(d);
        }
    }

    fn default_project_window(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.open_set.default_dialog.take() else {
            return;
        };
        let mut ok = false;
        let mut cancel = false;
        let mut help = false;
        let ((), closed, actions) = dialog_kit::window(
            ctx,
            "default_project",
            "Set Default Project",
            Vec2::new(452., 236.),
            |c: &mut Canvas| {
                c.fill(0., 0., 452., 236., chrome::pal::FACE);
                let cols = [
                    Col {
                        title: "Project ID",
                        width: 110.,
                    },
                    Col {
                        title: "Project",
                        width: 160.,
                    },
                    Col {
                        title: "Default",
                        width: 56.,
                    },
                ];
                let rows = d.rows.clone();
                let default = d.default;
                let out = dialog_kit::list(
                    c,
                    "default_project_grid",
                    (8., 8., 330., 220.),
                    None,
                    &cols,
                    rows.len(),
                    d.focus,
                    None,
                    &mut |c, r, k, rect, focused| {
                        let row = &rows[r];
                        match k {
                            0 => dialog_kit::cell_text(c, rect, &row.1, focused, 3.),
                            1 => dialog_kit::cell_text(c, rect, &row.2, focused, 3.),
                            _ => {
                                let at = rect.center() - c.origin - Vec2::splat(6.5);
                                dialog_kit::check_glyph(
                                    c,
                                    at.x,
                                    at.y,
                                    default == Some(row.0),
                                    true,
                                );
                            }
                        }
                    },
                );
                if let Some(r) = out.clicked {
                    d.focus = Some(r);
                    d.default = d.rows.get(r).map(|x| x.0);
                }
                dialog_kit::strip(c, 452., 236.);
                let column = dialog_kit::button_column(
                    c,
                    452.,
                    &[
                        Entry::ok("default_project_ok"),
                        Entry::cancel("default_project_cancel"),
                        Entry::help("default_project_help"),
                    ],
                );
                let clicked = |key: &str| column.clicked.as_deref() == Some(key);
                ok = clicked("default_project_ok");
                cancel = clicked("default_project_cancel");
                help = clicked("default_project_help");
            },
        );
        for (name, rect) in actions {
            self.action_rects
                .insert(name, [rect.left(), rect.top(), rect.right(), rect.bottom()]);
        }
        if help {
            self.open_help_for("default_project");
        }
        if ok {
            if let Some(id) = d.default {
                self.set_default_project(id);
            }
        } else if !(cancel || closed || ctx.input(|i| i.key_pressed(Key::Escape))) {
            self.open_set.default_dialog = Some(d);
        }
    }
}
