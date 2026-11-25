//! Activity, Project and Resource Codes: the code value tree of one code type
//! (Close, Add, Delete, Cut, Copy, Paste, the move pad) and the Modify dialog
//! that defines the code types (name, max length, order, scope).
use super::*;
use ops::{CodeKind, Scope, ValueField};

pub struct CodesState {
    pub kind: CodeKind,
    pub scope: Scope,
    pub type_id: Option<i64>,
    pub selected: Option<i64>,
    pub edit: Option<CellEdit>,
    /// Start editing the code value of the next selected row (after Add).
    pub edit_next: bool,
    /// The Modify dialog (code type definitions) is open.
    pub modify: Option<()>,
    pub modify_selected: Option<i64>,
    pub modify_edit: Option<CellEdit>,
    /// A line shown at the top of the dialog (opened from an Assign command).
    pub hint: Option<String>,
}

impl CodesState {
    pub fn open(p: &ProjectData, kind: CodeKind) -> Self {
        let pid = p.model.project.proj_id;
        let scope = if kind == CodeKind::Activity {
            [Scope::Project, Scope::Global, Scope::Eps]
                .into_iter()
                .find(|s| !ops::code_types(&p.document, kind, *s, pid).is_empty())
                .unwrap_or(Scope::Project)
        } else {
            Scope::Global
        };
        let type_id = ops::code_types(&p.document, kind, scope, pid)
            .first()
            .map(|t| t.id);
        Self {
            kind,
            scope,
            type_id,
            selected: None,
            edit: None,
            edit_next: false,
            modify: None,
            modify_selected: None,
            modify_edit: None,
            hint: None,
        }
    }

    /// The message shown when the chosen list has no code types.
    pub fn empty_notice(&self, p: &ProjectData) -> Option<String> {
        let pid = p.model.project.proj_id;
        if !ops::code_types(&p.document, self.kind, self.scope, pid).is_empty() {
            return None;
        }
        Some(match (self.kind, self.scope) {
            (CodeKind::Activity, Scope::Global) => "No global activity codes are defined. Click Modify to add one, or choose Project to see this project's activity codes.".into(),
            (CodeKind::Activity, Scope::Eps) => "No EPS activity codes are defined. Click Modify to add one, or choose Global to see the global activity codes.".into(),
            (CodeKind::Activity, Scope::Project) => "No project activity codes are defined. Click Modify to add one.".into(),
            (kind, _) => format!("No {} are defined. Click Modify to add one.", kind.title().to_lowercase()),
        })
    }
}

fn scope_caption(scope: Scope) -> &'static str {
    match scope {
        Scope::Global => "Global",
        Scope::Eps => "EPS",
        Scope::Project => "Project",
    }
}

impl App {
    pub(super) fn codes_dialog(
        &mut self,
        ctx: &egui::Context,
        st: &mut DictionaryState,
        cmds: &mut Vec<Cmd>,
        escape: bool,
    ) {
        let Some(mut cs) = st.codes.take() else {
            return;
        };
        let busy = st.busy();
        let modify_open = cs.modify.is_some();
        if modify_open {
            self.code_types_dialog(ctx, st, &mut cs, cmds, escape);
        }
        let pid = self.project.model.project.proj_id;
        let kind = cs.kind;
        let doc = &self.project.document;
        let types = ops::code_types(doc, kind, cs.scope, pid);
        if !types.iter().any(|t| Some(t.id) == cs.type_id) {
            cs.type_id = types.first().map(|t| t.id);
            cs.selected = None;
        }
        let values = cs
            .type_id
            .map(|t| ops::code_values(doc, kind, t))
            .unwrap_or_default();
        if !values.iter().any(|v| Some(v.id) == cs.selected) {
            cs.selected = values.first().map(|v| v.id);
        }
        if std::mem::take(&mut cs.edit_next) {
            if let Some(v) = values.iter().find(|v| Some(v.id) == cs.selected) {
                cs.edit = Some(CellEdit {
                    id: v.id,
                    column: 0,
                    text: v.short.clone(),
                    focus: true,
                });
            }
        }
        let selected = values.iter().position(|v| Some(v.id) == cs.selected);
        let sel = selected.map(|i| &values[i]);
        let spec = cs
            .type_id
            .map(|t| ops::value_spec(doc, kind, t))
            .unwrap_or(kind.tables().value);
        let prev_sibling = sel.and_then(|v| {
            values[..selected.unwrap()]
                .iter()
                .rev()
                .take_while(|w| w.depth >= v.depth)
                .any(|w| w.parent == v.parent)
                .then_some(())
        });
        let next_sibling = sel.and_then(|v| {
            values[selected.unwrap() + 1..]
                .iter()
                .take_while(|w| w.depth >= v.depth)
                .any(|w| w.parent == v.parent)
                .then_some(())
        });
        let clip_ok = st
            .clip
            .as_ref()
            .is_some_and(|c| c.spec.table == spec.table && (!c.cut || c.owner == cs.type_id));
        let has_type = cs.type_id.is_some();
        let entries = [
            Entry::close("codes_close"),
            Entry::Button("codes_add", "Add", has_type, true),
            Entry::Button("codes_delete", "Delete", sel.is_some(), false),
            Entry::Button("codes_cut", "Cut", sel.is_some(), false),
            Entry::Button("codes_copy", "Copy", sel.is_some(), false),
            Entry::Button("codes_paste", "Paste", clip_ok && has_type, false),
            Entry::Pad(
                "codes",
                [
                    sel.is_some_and(|v| v.parent.is_some()),
                    prev_sibling.is_some(),
                    next_sibling.is_some(),
                    prev_sibling.is_some(),
                ],
            ),
            Entry::help("codes_help"),
        ];
        let project_name = self.project.model.project.name.clone();
        let mut open = !(escape && cs.edit.is_none() && !modify_open);
        let mut column = kit::Column::default();
        let mut scope_click = None;
        let mut type_pick = None;
        let mut modify = false;
        let mut cells: Vec<(usize, usize, Rect)> = vec![];
        let mut commits: Vec<(i64, usize, String)> = vec![];
        let mut color_change: Option<(i64, String)> = None;
        let mut grid_out = crate::details::GridOut::default();
        let mut top_rects: Vec<(String, Rect)> = vec![];
        let mut edit = cs.edit.take();
        let width = 467.;
        let label = |t: &ops::CodeType| {
            if t.scope == Scope::Project && kind == CodeKind::Activity {
                format!("{project_name} - {}", t.name)
            } else {
                t.name.clone()
            }
        };
        let type_names: Vec<String> = types.iter().map(label).collect();
        chrome::dialog_window(kind.title())
            .id(egui::Id::new("codes_dialog"))
            .default_pos([360., 120.])
            .show(ctx, |ui| {
                ui.add_enabled_ui(!busy && cs.modify.is_none(), |ui| {
                    egui::Frame::NONE
                        .inner_margin(egui::Margin {
                            left: 5,
                            right: 5,
                            top: 5,
                            bottom: 0,
                        })
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(4., 4.);
                            let activity = kind == CodeKind::Activity;
                            let hint = cs.hint.is_some();
                            let height = 21.
                                + 31.
                                + if hint { 18. } else { 0. }
                                + if activity { 22. } else { 0. };
                            let (_, rects) =
                                kit::flow_canvas(ui, "codes_top", Vec2::new(width, height), |c| {
                                    let mut y = 0.;
                                    if let Some(hint) = &cs.hint {
                                        kit::text(c, 10., y + 2., hint, true);
                                        y += 18.;
                                    }
                                    if activity {
                                        for (k, s) in [Scope::Global, Scope::Eps, Scope::Project]
                                            .into_iter()
                                            .enumerate()
                                        {
                                            let caption = scope_caption(s);
                                            let key =
                                                format!("codes_scope_{}", caption.to_lowercase());
                                            if kit::radio(
                                                c,
                                                &key,
                                                10. + 90. * k as f32,
                                                y,
                                                80.,
                                                18.,
                                                caption,
                                                cs.scope == s,
                                                true,
                                            ) && cs.scope != s
                                            {
                                                scope_click = Some(s);
                                            }
                                        }
                                        y += 22.;
                                    }
                                    kit::heading(
                                        c,
                                        0.,
                                        y,
                                        width,
                                        &format!("Select {}", kind.singular()),
                                    );
                                    y += 25.;
                                    let items: Vec<(&str, bool)> = if type_names.is_empty() {
                                        vec![("", true)]
                                    } else {
                                        type_names.iter().map(|n| (n.as_str(), true)).collect()
                                    };
                                    let mut k = types
                                        .iter()
                                        .position(|t| Some(t.id) == cs.type_id)
                                        .unwrap_or(0);
                                    if c.combo(
                                        "codes_type",
                                        10.,
                                        y + 1.,
                                        275.,
                                        22.,
                                        &items,
                                        &mut k,
                                        !types.is_empty(),
                                    ) {
                                        type_pick = types.get(k).map(|t| t.id);
                                    }
                                    // Clear of the column's edge below, so
                                    // Enter keeps running the column's Close.
                                    modify = kit::button(
                                        c,
                                        "codes_modify",
                                        342.,
                                        y,
                                        kit::BUTTON.x,
                                        kit::BUTTON.y,
                                        "Modify...",
                                        true,
                                    );
                                });
                            top_rects = rects;
                        });
                    let (_, col, column_rects) = with_column(ui, "codes", &entries, |ui| {
                        let bar = types
                            .iter()
                            .find(|t| Some(t.id) == cs.type_id)
                            .map(|t| t.name.clone())
                            .unwrap_or_default();
                        widgets::display_bar(ui, 353., &bar);
                        let cols: Vec<(&str, f32)> = if kind == CodeKind::Activity {
                            vec![("Code Value", 110.), ("Description", 180.), ("Color", 60.)]
                        } else {
                            vec![("Code Value", 110.), ("Code Description", 240.)]
                        };
                        let icon = Dictionary::Codes(kind).icon();
                        grid_out = grid(
                            ui,
                            &grid_id(ui, "codes_values"),
                            &cols,
                            values.len(),
                            selected,
                            Vec2::new(353., 343.),
                            &mut |ui, ev, is_sel| {
                                let Ev::Cell(i, c) = ev else {
                                    return;
                                };
                                cells.push((i, c, ui.max_rect()));
                                let v = &values[i];
                                match c {
                                    0 => {
                                        ui.add_space(v.depth as f32 * 14.);
                                        if let Some(icon) = icon {
                                            let (r, _) = ui.allocate_exact_size(
                                                Vec2::new(18., 16.),
                                                Sense::hover(),
                                            );
                                            chrome::paint_icon_at(ui.painter(), icon, r.min, true);
                                        }
                                        if let Some(t) =
                                            editable_cell(ui, &mut edit, v.id, 0, &v.short)
                                        {
                                            commits.push((v.id, 0, t));
                                        }
                                    }
                                    1 => {
                                        if let Some(t) =
                                            editable_cell(ui, &mut edit, v.id, 1, &v.name)
                                        {
                                            commits.push((v.id, 1, t));
                                        }
                                    }
                                    _ => {
                                        let rgb = widgets::parse_rgb(&v.color);
                                        if is_sel {
                                            let mut c =
                                                rgb.map_or([0, 0, 255], |c| [c.r(), c.g(), c.b()]);
                                            let r = egui::color_picker::color_edit_button_srgb(
                                                ui, &mut c,
                                            );
                                            if r.changed() {
                                                color_change = Some((v.id, widgets::rgb_text(c)));
                                            }
                                        } else {
                                            widgets::swatch(ui, rgb);
                                        }
                                    }
                                }
                            },
                        );
                        if let Some(i) = grid_out.clicked {
                            if selected == Some(i) && edit.is_none() {
                                if let Some(c) = clicked_column(ui, &cells, i).filter(|c| *c < 2) {
                                    let v = &values[i];
                                    edit = Some(CellEdit {
                                        id: v.id,
                                        column: c,
                                        text: if c == 0 {
                                            v.short.clone()
                                        } else {
                                            v.name.clone()
                                        },
                                        focus: true,
                                    });
                                }
                            }
                        }
                        if edit.is_none() && ui.input(|i| i.key_pressed(Key::F2)) {
                            if let Some(v) = sel {
                                edit = Some(CellEdit {
                                    id: v.id,
                                    column: 0,
                                    text: v.short.clone(),
                                    focus: true,
                                });
                            }
                        }
                    });
                    column = col;
                    top_rects.extend(column_rects);
                });
            });
        for (name, rect) in top_rects {
            st.rect(name, rect);
        }
        for (r, c, rect) in &cells {
            st.rect(format!("codes_cell_{r}_{c}"), *rect);
        }
        if let Some(i) = grid_out.clicked {
            cs.selected = Some(values[i].id);
        }
        cs.edit = edit;
        let type_id = cs.type_id;
        for (id, c, text) in commits {
            if let Some(t) = type_id {
                let field = if c == 0 {
                    ValueField::Short
                } else {
                    ValueField::Name
                };
                cmds.push(Cmd::SetValue(kind, t, id, field, text));
            }
        }
        if let (Some((id, color)), Some(t)) = (color_change, type_id) {
            cmds.push(Cmd::SetValue(kind, t, id, ValueField::Color, color));
        }
        if let Some(s) = scope_click {
            cs.scope = s;
            cs.type_id = ops::code_types(&self.project.document, kind, s, pid)
                .first()
                .map(|t| t.id);
            cs.selected = None;
            cs.edit = None;
            if let Some(text) = cs.empty_notice(&self.project) {
                st.notice = Some(text);
            }
        }
        if let Some(t) = type_pick {
            cs.type_id = Some(t);
            cs.selected = None;
            cs.edit = None;
        }
        if modify {
            cs.modify = Some(());
            cs.modify_selected = cs.type_id;
        }
        if let Some(dir) = column.moved {
            if let (Some(id), Some(t)) = (cs.selected, type_id) {
                cmds.push(Cmd::Move(spec, Some(t), id, dir));
            }
        }
        match column.clicked.as_deref() {
            Some("codes_close") => open = false,
            Some("codes_add") => {
                if let Some(t) = type_id {
                    cs.edit = None;
                    cs.edit_next = true;
                    cmds.push(Cmd::AddValue(kind, t, cs.selected));
                }
            }
            Some("codes_delete") => {
                if let (Some(v), Some(t)) = (sel, type_id) {
                    let used = ops::code_value_usage(&self.project.document, kind, t, v.id);
                    let children = if v.has_children {
                        " and its child values"
                    } else {
                        ""
                    };
                    let text = if used > 0 {
                        format!(
                            "The code value {}{children} is assigned to {used} {}. Delete it and remove those assignments?",
                            v.short,
                            if used == 1 { kind.key() } else { kind.owners() }
                        )
                    } else {
                        format!("Delete the code value {}{children}?", v.short)
                    };
                    st.confirm = Some(Confirm {
                        title: kind.title().into(),
                        text,
                        buttons: &["Yes", "No"],
                        then: vec![Some(Cmd::DeleteValue(kind, t, v.id)), None],
                        pick_replacement: None,
                    });
                }
            }
            Some(k @ ("codes_cut" | "codes_copy")) => {
                if let (Some(id), Some(t)) = (cs.selected, type_id) {
                    st.clip = Some(ops::clip(
                        &self.project.document,
                        &spec,
                        Some(t),
                        id,
                        k == "codes_cut",
                    ));
                }
            }
            Some("codes_paste") => {
                if let Some(t) = type_id {
                    cmds.push(Cmd::Paste(spec, Some(t), cs.selected));
                }
            }
            Some("codes_help") => self.open_help_for(kind.title()),
            _ => {}
        }
        if open {
            st.codes = Some(cs);
        }
    }

    /// The Modify dialog: the code types of the list, with name and length.
    fn code_types_dialog(
        &mut self,
        ctx: &egui::Context,
        st: &mut DictionaryState,
        cs: &mut CodesState,
        cmds: &mut Vec<Cmd>,
        escape: bool,
    ) {
        let pid = self.project.model.project.proj_id;
        let kind = cs.kind;
        let busy = st.busy();
        let types = ops::code_types(&self.project.document, kind, cs.scope, pid);
        if !types.iter().any(|t| Some(t.id) == cs.modify_selected) {
            cs.modify_selected = types.first().map(|t| t.id);
        }
        let selected = types.iter().position(|t| Some(t.id) == cs.modify_selected);
        let sel = selected.map(|i| &types[i]);
        let activity = kind == CodeKind::Activity;
        let mut entries = vec![
            Entry::close("code_types_close"),
            Entry::Button("code_types_add", "Add", true, true),
            Entry::Button("code_types_delete", "Delete", sel.is_some(), false),
            Entry::Button(
                "code_types_up",
                "Shift up",
                selected.is_some_and(|i| i > 0),
                false,
            ),
            Entry::Button(
                "code_types_down",
                "Shift down",
                selected.is_some_and(|i| i + 1 < types.len()),
                false,
            ),
        ];
        if activity {
            entries.push(Entry::Button(
                "code_types_global",
                "Make Global",
                sel.is_some_and(|t| t.scope != Scope::Global),
                true,
            ));
            entries.push(Entry::Button(
                "code_types_eps",
                "Make EPS",
                sel.is_some_and(|t| t.scope == Scope::Project),
                false,
            ));
        }
        entries.push(Entry::help("code_types_help"));
        let mut edit = cs.modify_edit.take();
        let mut cells: Vec<(usize, usize, Rect)> = vec![];
        let mut commits: Vec<(i64, usize, String)> = vec![];
        let mut out = crate::details::GridOut::default();
        let mut column = kit::Column::default();
        let mut rects = vec![];
        let mut open = !(escape && edit.is_none());
        let title = format!("{} Name", kind.singular());
        chrome::dialog_window(kind.title())
            .id(egui::Id::new("code_types_dialog"))
            .default_pos([420., 170.])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    let (_, col, r) = with_column(ui, "code_types", &entries, |ui| {
                        widgets::display_bar(ui, 330., "Codes");
                        out = grid(
                            ui,
                            &grid_id(ui, "code_types_grid"),
                            &[(title.as_str(), 230.), ("Max Length", 90.)],
                            types.len(),
                            selected,
                            Vec2::new(330., 300.),
                            &mut |ui, ev, _| {
                                let Ev::Cell(i, c) = ev else {
                                    return;
                                };
                                cells.push((i, c, ui.max_rect()));
                                let t = &types[i];
                                let shown = if c == 0 {
                                    t.name.clone()
                                } else {
                                    t.max_len.to_string()
                                };
                                if let Some(text) = editable_cell(ui, &mut edit, t.id, c, &shown) {
                                    commits.push((t.id, c, text));
                                }
                            },
                        );
                        if let Some(i) = out.clicked {
                            if selected == Some(i) && edit.is_none() {
                                if let Some(c) = clicked_column(ui, &cells, i) {
                                    let t = &types[i];
                                    edit = Some(CellEdit {
                                        id: t.id,
                                        column: c,
                                        text: if c == 0 {
                                            t.name.clone()
                                        } else {
                                            t.max_len.to_string()
                                        },
                                        focus: true,
                                    });
                                }
                            }
                        }
                    });
                    column = col;
                    rects = r;
                });
            });
        for (name, rect) in rects {
            st.rect(name, rect);
        }
        for (r, c, rect) in &cells {
            st.rect(format!("code_types_cell_{r}_{c}"), *rect);
        }
        if let Some(i) = out.clicked {
            cs.modify_selected = Some(types[i].id);
        }
        cs.modify_edit = edit;
        for (id, c, text) in commits {
            cmds.push(if c == 0 {
                Cmd::RenameType(kind, id, text)
            } else {
                Cmd::TypeLength(kind, id, text)
            });
        }
        let clicked = column.clicked.as_deref();
        match clicked {
            Some("code_types_close") => open = false,
            Some("code_types_add") => {
                cs.modify_edit = None;
                cmds.push(Cmd::AddType(kind, cs.scope));
            }
            Some("code_types_delete") => {
                if let Some(t) = sel {
                    let (used, values) = ops::code_type_usage(&self.project.document, kind, t.id);
                    if used == 0 && values == 0 {
                        cmds.push(Cmd::DeleteType(kind, t.id));
                    } else {
                        st.confirm = Some(Confirm {
                            title: kind.title().into(),
                            text: format!(
                                "Deleting {} also deletes its {values} code values and {used} assignments to {}. Continue?",
                                t.name,
                                kind.owners()
                            ),
                            buttons: &["Yes", "No"],
                            then: vec![Some(Cmd::DeleteType(kind, t.id)), None],
                            pick_replacement: None,
                        });
                    }
                }
            }
            Some("code_types_up") | Some("code_types_down") => {
                if let Some(t) = sel {
                    let by = if clicked == Some("code_types_up") {
                        -1
                    } else {
                        1
                    };
                    cmds.push(Cmd::ShiftType(kind, cs.scope, t.id, by));
                }
            }
            Some("code_types_global") | Some("code_types_eps") => {
                if let Some(t) = sel {
                    let to = if clicked == Some("code_types_global") {
                        Scope::Global
                    } else {
                        Scope::Eps
                    };
                    cmds.push(Cmd::WidenType(kind, t.id, to));
                }
            }
            Some("code_types_help") => self.open_help_for(kind.title()),
            _ => {}
        }
        if !open {
            cs.modify = None;
            cs.modify_edit = None;
            if cs.type_id.is_none() || !types.iter().any(|t| Some(t.id) == cs.type_id) {
                cs.type_id = cs.modify_selected;
            }
        }
    }
}
