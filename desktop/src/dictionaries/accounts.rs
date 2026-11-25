//! Cost Accounts: the account tree with ID and name, a detail panel with the
//! description, Add, Del. / Merge (with a replacement for accounts in use),
//! Cut, Copy, Paste and the move pad.
use super::*;
use ops::AccountField;

#[derive(Default)]
pub struct AccountsState {
    pub selected: Option<i64>,
    pub edit: Option<CellEdit>,
    pub edit_next: bool,
    /// Detail panel drafts for the selected account: id, ID, name, description.
    draft: Option<(i64, String, String, String)>,
    /// A delete waiting for its replacement account: (account, picked).
    pub replace: Option<(i64, Option<i64>)>,
}

impl App {
    pub(super) fn accounts_dialog(
        &mut self,
        ctx: &egui::Context,
        st: &mut DictionaryState,
        cmds: &mut Vec<Cmd>,
        escape: bool,
    ) {
        let Some(mut a) = st.accounts.take() else {
            return;
        };
        let replacing = a.replace.is_some();
        if replacing {
            self.replacement_picker(ctx, st, &mut a, cmds, escape);
        }
        let busy = st.busy() || a.replace.is_some();
        let doc = &self.project.document;
        let list = ops::accounts(doc);
        if !list.iter().any(|x| Some(x.id) == a.selected) {
            a.selected = list.first().map(|x| x.id);
        }
        if std::mem::take(&mut a.edit_next) {
            if let Some(x) = list.iter().find(|x| Some(x.id) == a.selected) {
                a.edit = Some(CellEdit {
                    id: x.id,
                    column: 0,
                    text: x.short.clone(),
                    focus: true,
                });
            }
        }
        let selected = list.iter().position(|x| Some(x.id) == a.selected);
        let sel = selected.map(|i| &list[i]);
        if a.draft.as_ref().map(|d| d.0) != a.selected {
            a.draft = sel.map(|x| {
                (
                    x.id,
                    x.short.clone(),
                    x.name.clone(),
                    ops::account_description(doc, x.id),
                )
            });
        }
        let sibling = |forward: bool| {
            sel.is_some_and(|x| {
                let i = selected.unwrap();
                if forward {
                    list[i + 1..]
                        .iter()
                        .take_while(|w| w.depth >= x.depth)
                        .any(|w| w.parent == x.parent)
                } else {
                    list[..i]
                        .iter()
                        .rev()
                        .take_while(|w| w.depth >= x.depth)
                        .any(|w| w.parent == x.parent)
                }
            })
        };
        let clip_ok = st.clip.as_ref().is_some_and(|c| c.spec.table == "ACCOUNT");
        let entries = [
            Entry::close("accounts_close"),
            Entry::Button("accounts_add", "Add", true, true),
            Entry::Button(
                "accounts_delete",
                "Delete or Merge...",
                sel.is_some(),
                false,
            ),
            Entry::Button("accounts_cut", "Cut", sel.is_some(), false),
            Entry::Button("accounts_copy", "Copy", sel.is_some(), false),
            Entry::Button("accounts_paste", "Paste", clip_ok, false),
            Entry::Pad(
                "accounts",
                [
                    sel.is_some_and(|x| x.parent.is_some()),
                    sibling(false),
                    sibling(true),
                    sibling(false),
                ],
            ),
            Entry::help("accounts_help"),
        ];
        let mut open = !(escape && a.edit.is_none() && !replacing);
        let mut edit = a.edit.take();
        let mut cells: Vec<(usize, usize, Rect)> = vec![];
        let mut commits: Vec<(i64, usize, String)> = vec![];
        let mut out = crate::details::GridOut::default();
        let mut column = kit::Column::default();
        let mut draft = a.draft.take();
        let mut panel_rects: Vec<(String, Rect)> = vec![];
        chrome::dialog_window("Cost Accounts")
            .id(egui::Id::new("cost_accounts_dialog"))
            .default_pos([360., 110.])
            .show(ctx, |ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    let (_, col, column_rects) = with_column(ui, "accounts", &entries, |ui| {
                        widgets::display_bar(ui, 413., "Current Project's Cost Account");
                        out = grid(
                            ui,
                            &grid_id(ui, "accounts_grid"),
                            &[("Cost Account ID", 150.), ("Cost Account Name", 260.)],
                            list.len(),
                            selected,
                            Vec2::new(413., 175.),
                            &mut |ui, ev, _| {
                                let Ev::Cell(i, c) = ev else {
                                    return;
                                };
                                cells.push((i, c, ui.max_rect()));
                                let x = &list[i];
                                if c == 0 {
                                    ui.add_space(x.depth as f32 * 14.);
                                    let (r, _) =
                                        ui.allocate_exact_size(Vec2::new(18., 16.), Sense::hover());
                                    chrome::paint_icon_at(
                                        ui.painter(),
                                        Icon::CostAccounts,
                                        r.min,
                                        true,
                                    );
                                }
                                let shown = if c == 0 { &x.short } else { &x.name };
                                if let Some(t) = editable_cell(ui, &mut edit, x.id, c, shown) {
                                    commits.push((x.id, c, t));
                                }
                            },
                        );
                        if let Some(i) = out.clicked {
                            if selected == Some(i) && edit.is_none() {
                                if let Some(c) = clicked_column(ui, &cells, i) {
                                    let x = &list[i];
                                    edit = Some(CellEdit {
                                        id: x.id,
                                        column: c,
                                        text: if c == 0 {
                                            x.short.clone()
                                        } else {
                                            x.name.clone()
                                        },
                                        focus: true,
                                    });
                                }
                            }
                        }
                        // Splitter, then the detail panel: ID and name, the
                        // description and its commands.
                        let on = draft.is_some();
                        let mut blank = (0, String::new(), String::new(), String::new());
                        let d = draft.as_mut().unwrap_or(&mut blank);
                        let (_, rects) =
                            kit::flow_canvas(ui, "accounts_panel", Vec2::new(413., 70.), |c| {
                                c.etched(0., 4., 413.);
                                kit::text(c, 10., 14., "Cost Account ID", on);
                                let r = c.edit(
                                    "accounts_id",
                                    10.,
                                    29.,
                                    119.,
                                    21.,
                                    &mut d.1,
                                    on,
                                    Align::Min,
                                );
                                if r.lost_focus() && d.0 != 0 {
                                    commits.push((d.0, 10, d.1.clone()));
                                }
                                kit::text(c, 135., 14., "Cost Account Name", on);
                                let r = c.edit(
                                    "accounts_name",
                                    135.,
                                    29.,
                                    268.,
                                    21.,
                                    &mut d.2,
                                    on,
                                    Align::Min,
                                );
                                if r.lost_focus() && d.0 != 0 {
                                    commits.push((d.0, 11, d.2.clone()));
                                }
                                kit::text(c, 10., 56., "Cost Account Description", on);
                            });
                        panel_rects.extend(rects);
                        ui.horizontal(|ui| {
                            ui.add_space(10.);
                            let r = ui
                                .add_enabled_ui(on, |ui| {
                                    ui.add_sized(
                                        [393., 124.],
                                        egui::TextEdit::multiline(&mut d.3)
                                            .id(egui::Id::new("accounts_description")),
                                    )
                                })
                                .inner;
                            crate::dialog_keys::note_keeps_enter(&r);
                            panel_rects.push(("accounts_description".into(), r.rect));
                            if r.lost_focus() && d.0 != 0 {
                                commits.push((d.0, 12, d.3.clone()));
                            }
                        });
                        let (_, rects) =
                            kit::flow_canvas(ui, "accounts_text", Vec2::new(413., 25.), |c| {
                                // The shared notes buttons (Modify, Print, Copy).
                                match crate::views::sheet::notes_buttons(
                                    c,
                                    "accounts_text",
                                    10.,
                                    2.,
                                    on,
                                    &d.3,
                                ) {
                                    Some(0) => c.ui.memory_mut(|m| {
                                        m.request_focus(egui::Id::new("accounts_description"))
                                    }),
                                    Some(2) => c.ui.ctx().copy_text(d.3.clone()),
                                    _ => {}
                                }
                            });
                        panel_rects.extend(rects);
                        ui.horizontal(|ui| {
                            ui.add_space(10.);
                            let r = ui
                                .add_enabled_ui(on, |ui| {
                                    ui.add_sized(
                                        [393., 124.],
                                        egui::TextEdit::multiline(&mut d.3)
                                            .id(egui::Id::new("accounts_description")),
                                    )
                                })
                                .inner;
                            crate::dialog_keys::note_keeps_enter(&r);
                            panel_rects.push(("accounts_description".into(), r.rect));
                            if r.lost_focus() && d.0 != 0 {
                                commits.push((d.0, 12, d.3.clone()));
                            }
                        });
                        let (_, rects) =
                            kit::flow_canvas(ui, "accounts_text", Vec2::new(413., 25.), |c| {
                                let w = kit::BUTTON.x;
                                let h = kit::BUTTON.y;
                                if kit::button(
                                    c,
                                    "accounts_text_modify",
                                    10.,
                                    0.,
                                    w,
                                    h,
                                    "Modify",
                                    on,
                                ) {
                                    c.ui.memory_mut(|m| {
                                        m.request_focus(egui::Id::new("accounts_description"))
                                    });
                                }
                                kit::button(
                                    c,
                                    "accounts_text_print",
                                    123.,
                                    0.,
                                    w,
                                    h,
                                    "Print",
                                    false,
                                );
                                if kit::button(c, "accounts_text_copy", 236., 0., w, h, "Copy", on)
                                {
                                    c.ui.ctx().copy_text(d.3.clone());
                                }
                            });
                        panel_rects.extend(rects);
                    });
                    column = col;
                    panel_rects.extend(column_rects);
                });
            });
        for (r, c, rect) in &cells {
            st.rect(format!("accounts_cell_{r}_{c}"), *rect);
        }
        for (name, rect) in panel_rects {
            st.rect(name, rect);
        }
        a.draft = draft;
        if let Some(i) = out.clicked {
            a.selected = Some(list[i].id);
        }
        a.edit = edit;
        for (id, c, text) in commits {
            let field = match c {
                0 | 10 => AccountField::Id,
                1 | 11 => AccountField::Name,
                _ => AccountField::Description,
            };
            cmds.push(Cmd::SetAccount(id, field, text));
            // Panel drafts reload from the document after the edit.
            a.draft = None;
        }
        if let Some(dir) = column.moved {
            if let Some(id) = a.selected {
                cmds.push(Cmd::Move(ops::ACCOUNT, None, id, dir));
            }
        }
        match column.clicked.as_deref() {
            Some("accounts_close") => open = false,
            Some("accounts_add") => {
                a.edit = None;
                a.edit_next = true;
                cmds.push(Cmd::AddAccount(a.selected));
            }
            Some("accounts_delete") => {
                if let Some(x) = sel {
                    let used = ops::account_usage(&self.project.document, x.id);
                    let children = if x.has_children {
                        " or its child accounts"
                    } else {
                        ""
                    };
                    if used > 0 {
                        st.confirm = Some(Confirm {
                            title: "Cost Accounts".into(),
                            text: format!(
                                "Cost account {}{children} is used by {used} expenses, resource assignments or project defaults. Select a replacement account for them? No deletes it and leaves them without a cost account.",
                                x.short
                            ),
                            buttons: &["Yes", "No", "Cancel"],
                            then: vec![None, Some(Cmd::DeleteAccount(x.id, None)), None],
                            pick_replacement: Some(x.id),
                        });
                    } else if x.has_children {
                        st.confirm = Some(Confirm {
                            title: "Cost Accounts".into(),
                            text: format!(
                                "Delete cost account {} and its child accounts?",
                                x.short
                            ),
                            buttons: &["Yes", "No"],
                            then: vec![Some(Cmd::DeleteAccount(x.id, None)), None],
                            pick_replacement: None,
                        });
                    } else {
                        cmds.push(Cmd::DeleteAccount(x.id, None));
                    }
                }
            }
            Some(k @ ("accounts_cut" | "accounts_copy")) => {
                if let Some(id) = a.selected {
                    st.clip = Some(ops::clip(
                        &self.project.document,
                        &ops::ACCOUNT,
                        None,
                        id,
                        k == "accounts_cut",
                    ));
                }
            }
            Some("accounts_paste") => cmds.push(Cmd::Paste(ops::ACCOUNT, None, a.selected)),
            Some("accounts_help") => self.open_help_for("Cost Accounts"),
            _ => {}
        }
        if open {
            st.accounts = Some(a);
        }
    }

    /// Pick the account that takes over the rows of one being deleted.
    fn replacement_picker(
        &mut self,
        ctx: &egui::Context,
        st: &mut DictionaryState,
        a: &mut AccountsState,
        cmds: &mut Vec<Cmd>,
        escape: bool,
    ) {
        let Some((deleting, picked)) = a.replace else {
            return;
        };
        let doc = &self.project.document;
        let gone = ops::subtree(doc, &ops::ACCOUNT, None, deleting);
        let list: Vec<ops::Account> = ops::accounts(doc)
            .into_iter()
            .filter(|x| !gone.contains(&x.id))
            .collect();
        let selected = list.iter().position(|x| Some(x.id) == picked);
        let mut out = crate::details::GridOut::default();
        let mut column = kit::Column::default();
        let mut rects = vec![];
        let entries = [
            Entry::Button(
                "accounts_replacement_select",
                "Select",
                selected.is_some(),
                false,
            ),
            Entry::cancel("accounts_replacement_cancel"),
        ];
        chrome::dialog_window("Select Replacement Cost Account")
            .id(egui::Id::new("accounts_replacement"))
            .default_pos([420., 200.])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let (_, col, r) = with_column(ui, "accounts_replacement", &entries, |ui| {
                    widgets::display_bar(ui, 330., "Cost Accounts");
                    out = grid(
                        ui,
                        &grid_id(ui, "accounts_replacement_grid"),
                        &[("Cost Account ID", 130.), ("Cost Account Name", 200.)],
                        list.len(),
                        selected,
                        Vec2::new(330., 220.),
                        &mut |ui, ev, _| {
                            if let Ev::Cell(i, c) = ev {
                                let x = &list[i];
                                if c == 0 {
                                    widgets::tree_cell(ui, x.depth, None, &x.short);
                                } else {
                                    cell(ui, &x.name);
                                }
                            }
                        },
                    );
                });
                column = col;
                rects = r;
            });
        if let Some(i) = out.clicked {
            a.replace = Some((deleting, Some(list[i].id)));
        }
        for (name, rect) in rects {
            st.rect(name, rect);
        }
        if let Some(i) = out.double {
            cmds.push(Cmd::DeleteAccount(deleting, Some(list[i].id)));
            a.replace = None;
            return;
        }
        match column.clicked.as_deref() {
            Some("accounts_replacement_select") => {
                if let Some(r) = a.replace.and_then(|r| r.1) {
                    cmds.push(Cmd::DeleteAccount(deleting, Some(r)));
                }
                a.replace = None;
            }
            Some("accounts_replacement_cancel") => a.replace = None,
            _ if escape => a.replace = None,
            _ => {}
        }
    }
}
