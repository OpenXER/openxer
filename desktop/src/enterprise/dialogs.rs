//! The tree dialogs: EPS, OBS, Roles, Role Codes and Funding Sources. Each is
//! a grid of the tree with in-place editing of names and IDs, Close first in
//! the right-hand column, then Add, Delete, Cut, Copy, Paste, the move pad
//! and Help, and a detail area under the grid where the dictionary has more
//! fields.
use super::tree::{self, Clip, TreeSpec};
use super::ui::{self, RowView};
use super::*;
use crate::chrome::Icon;
use crate::context_menu::{ContextMenu, Entry as MenuEntry, Spec};
use crate::dialog_kit::{self as kit, Entry, Move};

/// How a grid column is edited.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ColKind {
    /// The tree's name or ID: checked by `tree::rename`.
    Rename,
    /// Free text up to this length.
    Text(usize),
    ReadOnly,
}

pub(super) struct TreeView<'a> {
    pub prefix: &'static str,
    pub spec: TreeSpec,
    pub owner: Option<i64>,
    pub workspace: bool,
    pub cols: Vec<(&'a str, f32, &'static str, ColKind)>,
    pub display: &'a str,
    pub icon: Option<Icon>,
    pub grid: (f32, f32, f32, f32),
    pub width: f32,
    /// Buttons between Paste's group and Help (Modify..., etc.).
    pub cut_paste: bool,
    /// Caption of a right-click item that shows the details tab under the
    /// grid, for the dialogs that have one.
    pub details: Option<&'static str>,
}

#[derive(Default)]
pub(super) struct TreeOut {
    pub close: bool,
    pub help: bool,
    pub delete: Option<i64>,
    /// A new clip from Cut or Copy, or `Some(None)` to clear it.
    pub clip: Option<Option<Clip>>,
    pub other: Option<String>,
    /// Select mode: Select was clicked on this row.
    pub select: Option<i64>,
}

/// The selected row, kept on an existing row (the first when it is gone).
pub(super) fn keep_selection(selected: &mut Option<i64>, ids: &[i64]) {
    if !selected.is_some_and(|s| ids.contains(&s)) {
        *selected = ids.first().copied();
    }
}

/// Grid, button column and the edits they ask for.
#[allow(clippy::too_many_arguments)]
pub(super) fn tree_body(
    c: &mut Canvas,
    st: &mut DialogState,
    view: &TreeView,
    doc: &Document,
    clip: Option<&Clip>,
    extra_buttons: &[(&'static str, &'static str, bool)],
    busy: bool,
    jobs: &mut Vec<Job>,
) -> TreeOut {
    let spec = view.spec;
    let owner = view.owner;
    let nodes = tree::nodes(doc, &spec, owner);
    let ids: Vec<i64> = nodes.iter().map(|n| n.id).collect();
    keep_selection(&mut st.selected, &ids);
    let selected = st.selected;
    let rows: Vec<RowView> = nodes
        .iter()
        .map(|n| RowView {
            id: n.id,
            depth: n.depth,
            icon: view.icon,
            cells: view
                .cols
                .iter()
                .map(|(_, _, column, _)| tree::value(doc, &spec, n.id, column))
                .collect(),
            editable: view
                .cols
                .iter()
                .map(|(_, _, _, kind)| *kind != ColKind::ReadOnly)
                .collect(),
            ..Default::default()
        })
        .collect();
    let row = selected.and_then(|s| nodes.iter().position(|n| n.id == s));
    let cols: Vec<(&str, f32)> = view.cols.iter().map(|(t, w, _, _)| (*t, *w)).collect();
    let out = ui::grid(
        c,
        &format!("{}_list", view.prefix),
        view.grid,
        Some(view.display),
        &cols,
        &rows,
        row,
        &mut st.edit,
        !busy,
    );
    let mut result = TreeOut::default();
    if let Some((r, _)) = out.secondary {
        // A right-click selects the row under the pointer first.
        if Some(rows[r].id) != selected {
            st.edit = None;
        }
        st.selected = Some(rows[r].id);
    }
    if let Some(r) = out.clicked {
        if Some(rows[r].id) != selected {
            st.edit = None;
        }
        st.selected = Some(rows[r].id);
    }
    if let Some((id, col, text)) = out.committed {
        let (_, _, column, kind) = view.cols[col];
        let ws = view.workspace;
        match kind {
            ColKind::Rename => jobs.push(tree_job(ws, move |doc| {
                Ok(if tree::rename(doc, &spec, owner, id, column, &text)? {
                    Done::Changed(None)
                } else {
                    Done::Nothing
                })
            })),
            ColKind::Text(max) => jobs.push(tree_job(ws, move |doc| {
                Ok(if ops::set_tree_text(doc, &spec, id, column, &text, max)? {
                    Done::Changed(None)
                } else {
                    Done::Nothing
                })
            })),
            ColKind::ReadOnly => {}
        }
    }
    // Button column.
    let node = row.map(|r| nodes[r].clone());
    let siblings: Vec<i64> = node.as_ref().map_or(vec![], |n| {
        nodes
            .iter()
            .filter(|m| m.parent == n.parent)
            .map(|m| m.id)
            .collect()
    });
    let at = node
        .as_ref()
        .and_then(|n| siblings.iter().position(|s| *s == n.id));
    let has = node.is_some() && !busy;
    let pad = [
        has && node.as_ref().is_some_and(|n| n.parent.is_some()),
        has && at.is_some_and(|k| k > 0),
        has && at.is_some_and(|k| k + 1 < siblings.len()),
        has && at.is_some_and(|k| k > 0),
    ];
    let can_paste = !busy && clip.is_some_and(|c| c.spec == spec && c.owner == owner);
    let p = view.prefix;
    let keys: Vec<String> = [
        "close", "add", "delete", "cut", "copy", "paste", "help", "move", "select",
    ]
    .iter()
    .map(|k| format!("{p}_{k}"))
    .collect();
    let mut entries = vec![Entry::Button(&keys[0], "Close", !busy, false)];
    if st.select.is_some() {
        entries.push(Entry::Button(&keys[8], "Select", has, false));
    }
    entries.extend([
        Entry::Button(&keys[1], "Add", !busy && owner_ok(&spec, owner), true),
        Entry::Button(&keys[2], "Delete", has, false),
    ]);
    if view.cut_paste {
        entries.push(Entry::Button(&keys[3], "Cut", has, true));
        entries.push(Entry::Button(&keys[4], "Copy", has, false));
        entries.push(Entry::Button(&keys[5], "Paste", can_paste, false));
    }
    for (key, label, enabled) in extra_buttons {
        entries.push(Entry::Button(key, label, *enabled && !busy, true));
    }
    entries.push(Entry::Pad(&keys[7], pad));
    entries.push(Entry::Button(&keys[6], "Help", !busy, true));
    let column = kit::button_column(c, view.width, &entries);
    let ws = view.workspace;
    // The list's right-click menu: the button column's commands.
    let chosen = {
        let item = |spec: Spec| MenuEntry::Item(spec);
        let mut menu = vec![
            item(
                Spec::new("add", "Add")
                    .icon(Icon::Add)
                    .enabled(!busy && owner_ok(&spec, owner)),
            ),
            item(
                Spec::new("delete", "Delete")
                    .icon(Icon::Delete)
                    .enabled(has),
            ),
        ];
        if view.cut_paste {
            menu.push(MenuEntry::Separator);
            menu.push(item(Spec::new("cut", "Cut").icon(Icon::Cut).enabled(has)));
            menu.push(item(
                Spec::new("copy", "Copy").icon(Icon::Copy).enabled(has),
            ));
            menu.push(item(
                Spec::new("paste", "Paste")
                    .icon(Icon::Paste)
                    .enabled(can_paste),
            ));
        }
        if let Some(label) = view.details {
            menu.push(MenuEntry::Separator);
            menu.push(item(
                Spec::new("details", label).icon(Icon::Details).enabled(has),
            ));
        }
        // The popup hangs on a zero-size anchor in this window.
        let at = out.secondary.map(|(_, pos)| pos);
        let anchor = c.ui.interact(
            Rect::from_min_size(at.unwrap_or(Pos2::ZERO), Vec2::ZERO),
            c.id(&format!("{p}_list_menu")),
            Sense::hover(),
        );
        let mut chosen = None;
        ContextMenu::new(&format!("{p}_menu"))
            .min_width(120.)
            .open_at(at)
            .show(&anchor, &mut chosen, |chosen, m| {
                if let Some(key) = m.entries(&menu) {
                    *chosen = Some(key);
                }
            });
        chosen
    };
    if chosen == Some("details") {
        st.tab = 0;
    }
    if let Some(dir) = column.moved {
        if let Some(id) = selected {
            jobs.push(tree_job(ws, move |doc| {
                let changed = match dir {
                    Move::Up => tree::shift(doc, &spec, owner, id, -1),
                    Move::Down => tree::shift(doc, &spec, owner, id, 1),
                    Move::Left => tree::outdent(doc, &spec, owner, id)?,
                    Move::Right => tree::indent(doc, &spec, owner, id)?,
                };
                Ok(if changed {
                    Done::Changed(Some(Sel::Main(id)))
                } else {
                    Done::Nothing
                })
            }));
        }
    }
    let menu_command = chosen
        .filter(|key| *key != "details")
        .map(|key| format!("{p}_{key}"));
    let Some(clicked) = column.clicked.or(menu_command) else {
        return result;
    };
    let name = clicked.trim_start_matches(p).trim_start_matches('_');
    match name {
        "close" => result.close = true,
        "select" => result.select = selected,
        "help" => result.help = true,
        "add" => {
            st.edit = None;
            jobs.push(tree_job(ws, move |doc| {
                let id = tree::add(doc, &spec, owner, selected, &[])?;
                Ok(Done::Changed(Some(Sel::Main(id))))
            }));
        }
        "delete" => result.delete = selected,
        "cut" | "copy" => {
            if let Some(id) = selected {
                result.clip = Some(tree::copy(doc, &spec, owner, id, name == "cut"));
            }
        }
        "paste" => {
            if let Some(clip) = clip.cloned() {
                if clip.cut {
                    result.clip = Some(None);
                }
                jobs.push(tree_job(ws, move |doc| {
                    let id = tree::paste(doc, &spec, owner, selected, &clip)?;
                    Ok(Done::Changed(Some(Sel::Main(id))))
                }));
            }
        }
        _ => result.other = Some(clicked),
    }
    result
}

/// A tree split by owner (code values) needs an owner to add to.
fn owner_ok(spec: &TreeSpec, owner: Option<i64>) -> bool {
    spec.owner.is_none() || owner.is_some()
}

impl App {
    fn apply_tree_out(&mut self, out: &TreeOut) {
        if out.help {
            self.request_context_help();
        }
        if let Some(clip) = &out.clip {
            self.enterprise.clip = clip.clone();
        }
    }

    // ------------------------------------------------------------------ EPS

    pub(super) fn eps_dialog(
        &mut self,
        ctx: &egui::Context,
        st: &mut DialogState,
        jobs: &mut Vec<Job>,
        busy: bool,
    ) -> bool {
        let size = Vec2::new(640., 440.);
        let ws_doc = self.workspace().doc.clone();
        let obs: Vec<String> = tree::nodes(&self.project.document, &ops::OBS, None)
            .iter()
            .map(|n| tree::value(&self.project.document, &ops::OBS, n.id, "obs_name"))
            .collect();
        let clip = self.enterprise.clip.clone();
        let view = TreeView {
            prefix: "eps",
            spec: workspace::EPS,
            owner: None,
            workspace: true,
            cols: vec![
                ("EPS ID", 140., "eps_short_name", ColKind::Rename),
                ("EPS Name", 220., "eps_name", ColKind::Rename),
                ("Responsible Manager", 149., "obs_name", ColKind::ReadOnly),
            ],
            display: "EPS",
            icon: Some(Icon::Eps),
            grid: (5., 5., size.x - 122., size.y - 60.),
            width: size.x,
            cut_paste: true,
            details: None,
        };
        let (out, closed, actions) = kit::window(ctx, "eps", Dialog::Eps.title(), size, |c| {
            kit::strip(c, size.x, size.y);
            let out = tree_body(c, st, &view, &ws_doc, clip.as_ref(), &[], busy, jobs);
            // Responsible Manager of the selected node, from the project's OBS.
            kit::text(c, 8., size.y - 47., "Responsible Manager", true);
            if let Some(id) = st.selected {
                let current = tree::value(&ws_doc, &workspace::EPS, id, "obs_name");
                let mut items: Vec<(&str, bool)> = vec![("", true)];
                items.extend(obs.iter().map(|n| (n.as_str(), true)));
                let mut k = obs.iter().position(|n| *n == current).map_or(0, |k| k + 1);
                if c.combo(
                    "eps_manager",
                    8.,
                    size.y - 31.,
                    300.,
                    22.,
                    &items,
                    &mut k,
                    !busy && !obs.is_empty(),
                ) {
                    let name = if k == 0 {
                        String::new()
                    } else {
                        obs[k - 1].clone()
                    };
                    jobs.push(Job::Ws(Box::new(move |ws| {
                        Ok(
                            if ops::set_tree_text(
                                &mut ws.doc,
                                &workspace::EPS,
                                id,
                                "obs_name",
                                &name,
                                100,
                            )? {
                                Done::Changed(None)
                            } else {
                                Done::Nothing
                            },
                        )
                    })));
                }
            }
            out
        });
        self.record_dialog_actions(actions);
        self.apply_tree_out(&out);
        // Delete takes the node with everything under it, after one
        // question that counts what goes (shared with the Projects view).
        if let Some(id) = out.delete {
            match self.folder_delete_question(id) {
                Ok(text) => jobs.push(Job::Ask(Confirm::yes_no(
                    "Delete EPS Node",
                    text,
                    Job::DeleteFolder(id),
                ))),
                Err(e) => st.notice = Some(e),
            }
        }
        out.close || closed
    }

    // ------------------------------------------------------------------ OBS

    pub(super) fn obs_dialog(
        &mut self,
        ctx: &egui::Context,
        st: &mut DialogState,
        jobs: &mut Vec<Job>,
        busy: bool,
    ) -> bool {
        let size = Vec2::new(560., 460.);
        let doc = &self.project.document;
        let clip = self.enterprise.clip.clone();
        let view = TreeView {
            prefix: "obs",
            spec: ops::OBS,
            owner: None,
            workspace: false,
            cols: vec![("OBS Name", size.x - 124., "obs_name", ColKind::Rename)],
            display: "OBS Chart",
            icon: Some(Icon::Obs),
            grid: (5., 5., size.x - 122., 280.),
            width: size.x,
            cut_paste: true,
            details: None,
        };
        let (out, closed, actions) = kit::window(ctx, "obs", Dialog::Obs.title(), size, |c| {
            kit::strip(c, size.x, size.y);
            let out = tree_body(c, st, &view, doc, clip.as_ref(), &[], busy, jobs);
            kit::section(c, 5., 291., size.x - 122., size.y - 296., "General");
            if let Some(id) = st.selected {
                let name = tree::value(doc, &ops::OBS, id, "obs_name");
                kit::text(c, 14., 321., "OBS Name", !busy);
                if let Some(text) =
                    kit::commit_edit(c, "obs_name", 14., 336., 300., &name, !busy, Align::Min)
                {
                    jobs.push(Job::Doc(Box::new(move |doc| {
                        Ok(
                            if tree::rename(doc, &ops::OBS, None, id, "obs_name", &text)? {
                                Done::Changed(None)
                            } else {
                                Done::Nothing
                            },
                        )
                    })));
                }
                let descr = records::memo_to_text(&tree::value(doc, &ops::OBS, id, "obs_descr"));
                kit::text(c, 14., 366., "Description", !busy);
                if let Some(text) = kit::commit_edit(
                    c,
                    "obs_descr",
                    14.,
                    381.,
                    size.x - 145.,
                    &descr,
                    !busy,
                    Align::Min,
                ) {
                    jobs.push(Job::Doc(Box::new(move |doc| {
                        let html = records::text_to_memo(&ops::clean_note(&text, 4000)?);
                        Ok(
                            if ops::set_tree_text(doc, &ops::OBS, id, "obs_descr", &html, 8000)? {
                                Done::Changed(None)
                            } else {
                                Done::Nothing
                            },
                        )
                    })));
                }
                let uses = ops::obs_uses(doc, id);
                kit::text(
                    c,
                    14.,
                    415.,
                    &format!("Responsible for {uses} WBS, issue and threshold record(s)"),
                    true,
                );
            }
            out
        });
        self.record_dialog_actions(actions);
        self.apply_tree_out(&out);
        if let Some(id) = out.delete {
            let doc = &self.project.document;
            if tree::nodes(doc, &ops::OBS, None).len()
                <= tree::subtree(doc, &ops::OBS, None, id).len()
            {
                st.notice = Some("The OBS needs at least one element.".into());
            } else {
                let uses = ops::obs_uses(doc, id);
                let heir = ops::obs_heir(doc, id)
                    .map(|h| tree::value(doc, &ops::OBS, h, "obs_name"))
                    .unwrap_or_default();
                let text = if uses > 0 {
                    format!("{uses} record(s) name this OBS element or one under it as responsible. Delete it and make \"{heir}\" responsible instead?")
                } else {
                    "Delete the selected OBS element and the elements under it?".into()
                };
                jobs.push(Job::Ask(Confirm::yes_no(
                    "Delete OBS Element",
                    text,
                    Job::Doc(Box::new(move |doc| {
                        ops::delete_obs(doc, id)?;
                        Ok(Done::Changed(None))
                    })),
                )));
            }
        }
        out.close || closed
    }

    // ---------------------------------------------------------------- roles

    pub(super) fn roles_dialog(
        &mut self,
        ctx: &egui::Context,
        st: &mut DialogState,
        jobs: &mut Vec<Job>,
        busy: bool,
    ) -> bool {
        let size = Vec2::new(620., 480.);
        let doc = &self.project.document;
        let clip = self.enterprise.clip.clone();
        let view = TreeView {
            prefix: "roles",
            spec: ops::ROLES,
            owner: None,
            workspace: false,
            cols: vec![
                ("Role ID", 140., "role_short_name", ColKind::Rename),
                ("Role Name", size.x - 264., "role_name", ColKind::Rename),
            ],
            display: "Roles",
            icon: Some(Icon::Roles),
            grid: (5., 5., size.x - 122., 250.),
            width: size.x,
            cut_paste: true,
            details: Some("Role Details"),
        };
        let code_types = ops::role_code_types(doc);
        let (out, closed, actions) = kit::window(ctx, "roles", Dialog::Roles.title(), size, |c| {
            kit::strip(c, size.x, size.y);
            let out = tree_body(c, st, &view, doc, clip.as_ref(), &[], busy, jobs);
            let top = 262.;
            let tabs = ["General", "Prices", "Codes"].map(|label| kit::VTab {
                label,
                key: label,
                unavailable: None,
            });
            if let Some(k) = kit::tabs(
                c,
                "roles_tab_",
                (5., top + 21., size.x - 122., size.y - top - 26.),
                &tabs,
                st.tab,
            ) {
                st.tab = k;
            }
            let Some(id) = st.selected else {
                return out;
            };
            let y = top + 50.;
            match st.tab {
                0 => {
                    for (k, (label, column)) in
                        [("Role ID", "role_short_name"), ("Role Name", "role_name")]
                            .into_iter()
                            .enumerate()
                    {
                        let current = tree::value(doc, &ops::ROLES, id, column);
                        let x = 14. + 200. * k as f32;
                        let key = format!("roles_field_{column}");
                        kit::text(c, x, y - 15., label, !busy);
                        if let Some(text) =
                            kit::commit_edit(c, &key, x, y, 180., &current, !busy, Align::Min)
                        {
                            jobs.push(Job::Doc(Box::new(move |doc| {
                                Ok(
                                    if tree::rename(doc, &ops::ROLES, None, id, column, &text)? {
                                        Done::Changed(None)
                                    } else {
                                        Done::Nothing
                                    },
                                )
                            })));
                        }
                    }
                    let descr =
                        records::memo_to_text(&tree::value(doc, &ops::ROLES, id, "role_descr"));
                    kit::text(c, 14., y + 31., "Responsibilities", !busy);
                    if let Some(text) = kit::commit_edit(
                        c,
                        "roles_field_role_descr",
                        14.,
                        y + 46.,
                        size.x - 145.,
                        &descr,
                        !busy,
                        Align::Min,
                    ) {
                        jobs.push(Job::Doc(Box::new(move |doc| {
                            let html = records::text_to_memo(&ops::clean_note(&text, 4000)?);
                            Ok(
                                if ops::set_tree_text(
                                    doc,
                                    &ops::ROLES,
                                    id,
                                    "role_descr",
                                    &html,
                                    8000,
                                )? {
                                    Done::Changed(None)
                                } else {
                                    Done::Nothing
                                },
                            )
                        })));
                    }
                }
                1 => {
                    let prices = ops::role_prices(doc, id);
                    let labels = [
                        "Price / Unit",
                        "Price / Unit2",
                        "Price / Unit3",
                        "Price / Unit4",
                        "Price / Unit5",
                    ];
                    for (k, label) in labels.iter().enumerate() {
                        let row_y = y - 18. + 27. * k as f32;
                        kit::text(c, 14., row_y + 4., label, true);
                        let shown = format::cost(prices[k]);
                        let key = format!("roles_price_{k}");
                        if let Some(text) =
                            kit::commit_edit(c, &key, 120., row_y, 140., &shown, !busy, Align::Max)
                        {
                            jobs.push(Job::Doc(Box::new(move |doc| {
                                Ok(if ops::set_role_price(doc, id, k, &text)? {
                                    Done::Changed(None)
                                } else {
                                    Done::Nothing
                                })
                            })));
                        }
                    }
                }
                _ => {
                    // One row per role code with the role's value.
                    let rows: Vec<RowView> = code_types
                        .iter()
                        .map(|t| {
                            let value = ops::role_code(doc, id, *t);
                            let spec = ops::role_value_spec(doc, *t);
                            RowView {
                                id: *t,
                                cells: vec![
                                    ops::list_value(
                                        doc,
                                        &ops::ROLE_CODE_TYPES,
                                        *t,
                                        "role_catg_type",
                                    ),
                                    value.map_or(String::new(), |v| {
                                        tree::value(doc, &spec, v, spec.name)
                                    }),
                                ],
                                ..Default::default()
                            }
                        })
                        .collect();
                    let mut none = None;
                    let grid_rect = (10., top + 30., size.x - 135., size.y - top - 40.);
                    let out = ui::grid(
                        c,
                        "roles_codes",
                        grid_rect,
                        None,
                        &[("Role Code", 200.), ("Code Value", size.x - 337.)],
                        &rows,
                        st.sub.and_then(|s| code_types.iter().position(|t| *t == s)),
                        &mut none,
                        !busy,
                    );
                    if let Some(r) = out.clicked {
                        st.sub = Some(code_types[r]);
                    }
                    if let Some(t) = st.sub.filter(|s| code_types.contains(s)) {
                        let r = code_types.iter().position(|x| *x == t).unwrap();
                        let spec = ops::role_value_spec(doc, t);
                        let values = tree::nodes(doc, &spec, Some(t));
                        let names: Vec<String> = values
                            .iter()
                            .map(|n| {
                                format!(
                                    "{}{}",
                                    "  ".repeat(n.depth),
                                    tree::value(doc, &spec, n.id, spec.name)
                                )
                            })
                            .collect();
                        let mut items: Vec<(&str, bool)> = vec![("", true)];
                        items.extend(names.iter().map(|n| (n.as_str(), true)));
                        let current = ops::role_code(doc, id, t);
                        let mut k = current
                            .and_then(|v| values.iter().position(|n| n.id == v))
                            .map_or(0, |k| k + 1);
                        // The combo sits in the Code Value cell of the row.
                        if c.combo(
                            "roles_code_value",
                            212.,
                            top + 50. + 18. * r as f32,
                            size.x - 339.,
                            16.,
                            &items,
                            &mut k,
                            true,
                        ) {
                            let value = (k > 0).then(|| values[k - 1].id);
                            jobs.push(Job::Doc(Box::new(move |doc| {
                                Ok(if ops::set_role_code(doc, id, t, value) {
                                    Done::Changed(None)
                                } else {
                                    Done::Nothing
                                })
                            })));
                        }
                    }
                }
            }
            out
        });
        self.record_dialog_actions(actions);
        self.apply_tree_out(&out);
        if let Some(id) = out.delete {
            let uses = ops::role_uses(&self.project.document, id);
            let text = if uses > 0 {
                format!("{uses} assignment(s) and resource(s) use this role or a role under it. Delete it and remove it from them?")
            } else {
                "Delete the selected role and the roles under it?".into()
            };
            jobs.push(Job::Ask(Confirm::yes_no(
                "Delete Role",
                text,
                Job::Doc(Box::new(move |doc| {
                    ops::delete_role(doc, id);
                    Ok(Done::Changed(None))
                })),
            )));
        }
        if out.select.is_some() {
            st.chosen = out.select;
        }
        out.close || closed
    }

    // ------------------------------------------------------ funding sources

    pub(super) fn funding_dialog(
        &mut self,
        ctx: &egui::Context,
        st: &mut DialogState,
        jobs: &mut Vec<Job>,
        busy: bool,
    ) -> bool {
        let size = Vec2::new(560., 440.);
        let doc = &self.project.document;
        let clip = self.enterprise.clip.clone();
        let view = TreeView {
            prefix: "funding",
            spec: ops::FUNDING,
            owner: None,
            workspace: false,
            cols: vec![(
                "Funding Source",
                size.x - 124.,
                "fund_name",
                ColKind::Rename,
            )],
            display: "Funding Sources",
            icon: Some(Icon::FundingSources),
            grid: (5., 5., size.x - 122., 300.),
            width: size.x,
            cut_paste: true,
            details: None,
        };
        let (out, closed, actions) =
            kit::window(ctx, "funding", Dialog::Funding.title(), size, |c| {
                kit::strip(c, size.x, size.y);
                let out = tree_body(c, st, &view, doc, clip.as_ref(), &[], busy, jobs);
                kit::section(c, 5., 311., size.x - 122., size.y - 316., "Notes");
                if let Some(id) = st.selected {
                    let descr =
                        records::memo_to_text(&tree::value(doc, &ops::FUNDING, id, "fund_descr"));
                    kit::text(c, 14., 341., "Description", !busy);
                    if let Some(text) = kit::commit_edit(
                        c,
                        "funding_descr",
                        14.,
                        356.,
                        size.x - 145.,
                        &descr,
                        !busy,
                        Align::Min,
                    ) {
                        jobs.push(Job::Doc(Box::new(move |doc| {
                            let html = records::text_to_memo(&ops::clean_note(&text, 4000)?);
                            Ok(
                                if ops::set_tree_text(
                                    doc,
                                    &ops::FUNDING,
                                    id,
                                    "fund_descr",
                                    &html,
                                    8000,
                                )? {
                                    Done::Changed(None)
                                } else {
                                    Done::Nothing
                                },
                            )
                        })));
                    }
                }
                out
            });
        self.record_dialog_actions(actions);
        self.apply_tree_out(&out);
        if let Some(id) = out.delete {
            let uses = ops::funding_uses(&self.project.document, id);
            let text = if uses > 0 {
                format!("{uses} project funding row(s) name this source or one under it. Delete it and those rows?")
            } else {
                "Delete the selected funding source and those under it?".into()
            };
            jobs.push(Job::Ask(Confirm::yes_no(
                "Delete Funding Source",
                text,
                Job::Doc(Box::new(move |doc| {
                    ops::delete_funding(doc, id);
                    Ok(Done::Changed(None))
                })),
            )));
        }
        out.close || closed
    }

    // ----------------------------------------------------------- role codes

    pub(super) fn role_codes_dialog(
        &mut self,
        ctx: &egui::Context,
        st: &mut DialogState,
        jobs: &mut Vec<Job>,
        busy: bool,
    ) -> bool {
        let size = Vec2::new(500., 480.);
        let doc = &self.project.document;
        let clip = self.enterprise.clip.clone();
        let types = ops::role_code_types(doc);
        if !st.owner.is_some_and(|o| types.contains(&o)) {
            st.owner = types.first().copied();
        }
        let owner = st.owner;
        let spec = owner.map_or(ops::ROLE_CODE_VALUES, |t| ops::role_value_spec(doc, t));
        let display = owner
            .map(|t| ops::list_value(doc, &ops::ROLE_CODE_TYPES, t, "role_catg_type"))
            .unwrap_or_default();
        let view = TreeView {
            prefix: "role_codes",
            spec,
            owner,
            workspace: false,
            cols: vec![
                ("Code Value", 130., "role_catg_short_name", ColKind::Rename),
                (
                    "Code Description",
                    size.x - 254.,
                    "role_catg_name",
                    ColKind::Text(100),
                ),
            ],
            display: &display,
            icon: Some(Icon::RoleCodes),
            grid: (5., 60., size.x - 122., size.y - 65.),
            width: size.x,
            cut_paste: true,
            details: None,
        };
        let child = st.types_open;
        let (out, closed, actions) =
            kit::window(ctx, "role_codes", Dialog::RoleCodes.title(), size, |c| {
                kit::strip(c, size.x, size.y);
                kit::heading(c, 5., 5., size.x - 122., "Select Role Code");
                let names: Vec<String> = types
                    .iter()
                    .map(|t| ops::list_value(doc, &ops::ROLE_CODE_TYPES, *t, "role_catg_type"))
                    .collect();
                let items: Vec<(&str, bool)> = names.iter().map(|n| (n.as_str(), true)).collect();
                let mut k = owner
                    .and_then(|o| types.iter().position(|t| *t == o))
                    .unwrap_or(0);
                if !items.is_empty()
                    && c.combo(
                        "role_codes_type",
                        5.,
                        31.,
                        250.,
                        22.,
                        &items,
                        &mut k,
                        !busy && !child,
                    )
                {
                    st.owner = Some(types[k]);
                    st.selected = None;
                    st.edit = None;
                }
                let modify = kit::button(
                    c,
                    "role_codes_modify",
                    262.,
                    30.,
                    kit::BUTTON.x,
                    kit::BUTTON.y,
                    "Modify...",
                    !busy && !child,
                );
                let out = tree_body(c, st, &view, doc, clip.as_ref(), &[], busy || child, jobs);
                (out, modify)
            });
        self.record_dialog_actions(actions);
        let (out, modify) = out;
        if modify {
            st.types_open = true;
            st.type_selected = st.owner;
        }
        self.apply_tree_out(&out);
        if let (Some(id), Some(t)) = (out.delete, owner) {
            let uses = tree::uses(
                &self.project.document,
                "ROLERCAT",
                "role_catg_id",
                &tree::subtree(&self.project.document, &spec, Some(t), id),
            );
            let text = if uses > 0 {
                format!("{uses} role(s) have this value or one under it. Delete it and remove it from them?")
            } else {
                "Delete the selected code value and the values under it?".into()
            };
            jobs.push(Job::Ask(Confirm::yes_no(
                "Delete Code Value",
                text,
                Job::Doc(Box::new(move |doc| {
                    ops::delete_role_code_value(doc, t, id);
                    Ok(Done::Changed(None))
                })),
            )));
        }
        if st.types_open {
            self.role_code_types_dialog(ctx, st, jobs, busy);
        }
        out.close || closed
    }

    /// Modify...: the role codes themselves, with their Max Length.
    fn role_code_types_dialog(
        &mut self,
        ctx: &egui::Context,
        st: &mut DialogState,
        jobs: &mut Vec<Job>,
        busy: bool,
    ) {
        let size = Vec2::new(440., 320.);
        let doc = &self.project.document;
        let types = ops::role_code_types(doc);
        keep_selection(&mut st.type_selected, &types);
        let rows: Vec<RowView> = types
            .iter()
            .map(|t| RowView {
                id: *t,
                cells: vec![
                    ops::list_value(doc, &ops::ROLE_CODE_TYPES, *t, "role_catg_type"),
                    ops::role_code_length(doc, *t).to_string(),
                ],
                editable: vec![true, true],
                ..Default::default()
            })
            .collect();
        let selected = st.type_selected;
        let ((close, add, delete), closed, actions) =
            kit::window(ctx, "role_code_types", "Role Code Definitions", size, |c| {
                kit::strip(c, size.x, size.y);
                let out = ui::grid(
                    c,
                    "role_code_types_list",
                    (5., 5., size.x - 122., size.y - 10.),
                    Some("Role Codes"),
                    &[("Role Code", 220.), ("Max Length", size.x - 344.)],
                    &rows,
                    selected.and_then(|s| types.iter().position(|t| *t == s)),
                    &mut st.type_edit,
                    !busy,
                );
                if let Some(r) = out.clicked {
                    st.type_selected = Some(types[r]);
                }
                if let Some((id, col, text)) = out.committed {
                    jobs.push(Job::Doc(Box::new(move |doc| {
                        let changed = if col == 0 {
                            ops::list_rename(doc, &ops::ROLE_CODE_TYPES, id, &text)?
                        } else {
                            ops::set_role_code_length(doc, id, &text)?
                        };
                        Ok(if changed {
                            Done::Changed(None)
                        } else {
                            Done::Nothing
                        })
                    })));
                }
                let column = kit::button_column(
                    c,
                    size.x,
                    &[
                        Entry::Button("role_code_types_close", "Close", !busy, false),
                        Entry::Button("role_code_types_add", "Add", !busy, true),
                        Entry::Button(
                            "role_code_types_delete",
                            "Delete",
                            !busy && selected.is_some(),
                            false,
                        ),
                    ],
                );
                let clicked = column.clicked.unwrap_or_default();
                (
                    clicked == "role_code_types_close",
                    clicked == "role_code_types_add",
                    clicked == "role_code_types_delete",
                )
            });
        self.record_dialog_actions(actions);
        if close || closed {
            st.types_open = false;
        }
        if add {
            jobs.push(Job::Doc(Box::new(|doc| {
                let id = ops::add_role_code_type(doc);
                Ok(Done::Changed(Some(Sel::Sub(id))))
            })));
        }
        if let (true, Some(id)) = (delete, selected) {
            let doc = &self.project.document;
            let values = tree::nodes(doc, &ops::role_value_spec(doc, id), Some(id)).len();
            jobs.push(Job::Ask(Confirm::yes_no(
                "Delete Role Code",
                format!("Delete this role code and its {values} value(s)?"),
                Job::Doc(Box::new(move |doc| {
                    ops::delete_role_code_type(doc, id);
                    Ok(Done::Changed(None))
                })),
            )));
        }
        // A new code is selected in the definitions list.
        if let Some(id) = st.sub.take() {
            st.type_selected = Some(id);
        }
    }
}
