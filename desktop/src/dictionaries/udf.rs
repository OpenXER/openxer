//! User Defined Fields: the fields of one subject area with Title and Data
//! Type, plus Add and Delete. Activity fields feed the Activity Details User
//! Defined Fields tab and the grid's User Defined Fields columns.
use super::*;

#[derive(Default)]
pub struct UdfState {
    /// Index into `ops::SUBJECT_AREAS`.
    pub area: usize,
    pub selected: Option<i64>,
    pub edit: Option<CellEdit>,
    pub edit_next: bool,
}

impl App {
    pub(super) fn udf_dialog(
        &mut self,
        ctx: &egui::Context,
        st: &mut DictionaryState,
        cmds: &mut Vec<Cmd>,
        escape: bool,
    ) {
        let Some(mut us) = st.udf.take() else {
            return;
        };
        let busy = st.busy();
        let (_, table) = ops::SUBJECT_AREAS[us.area.min(ops::SUBJECT_AREAS.len() - 1)];
        let fields = ops::udfs(&self.project.document, table);
        if !fields.iter().any(|f| Some(f.id) == us.selected) {
            us.selected = fields.first().map(|f| f.id);
        }
        if std::mem::take(&mut us.edit_next) {
            if let Some(f) = fields.iter().find(|f| Some(f.id) == us.selected) {
                us.edit = Some(CellEdit {
                    id: f.id,
                    column: 0,
                    text: f.title.clone(),
                    focus: true,
                });
            }
        }
        let selected = fields.iter().position(|f| Some(f.id) == us.selected);
        let entries = [
            Entry::close("udf_close"),
            Entry::Button("udf_add", "Add", true, true),
            Entry::Button("udf_delete", "Delete", selected.is_some(), false),
            Entry::help("udf_help"),
        ];
        let mut open = !(escape && us.edit.is_none());
        let mut edit = us.edit.take();
        let mut cells: Vec<(usize, usize, Rect)> = vec![];
        let mut commits: Vec<(i64, String)> = vec![];
        let mut type_change: Option<(i64, &'static str)> = None;
        let mut area_pick = None;
        let mut out = crate::details::GridOut::default();
        let mut column = kit::Column::default();
        let mut rects = vec![];
        chrome::dialog_window("User Defined Fields")
            .id(egui::Id::new("udf_dialog"))
            .default_pos([380., 140.])
            .show(ctx, |ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    let (_, col, column_rects) = with_column(ui, "udf", &entries, |ui| {
                        heading(ui, "udf_heading", 335., "User Defined Fields");
                        ui.add_space(5.);
                        let (_, combo) =
                            kit::flow_canvas(ui, "udf_area", Vec2::new(335., 22.), |c| {
                                let items: Vec<(&str, bool)> = ops::SUBJECT_AREAS
                                    .iter()
                                    .map(|(label, _)| (*label, true))
                                    .collect();
                                let mut k = us.area;
                                if c.combo(
                                    "udf_subject_area",
                                    4.,
                                    0.,
                                    243.,
                                    22.,
                                    &items,
                                    &mut k,
                                    true,
                                ) {
                                    area_pick = Some(k);
                                }
                            });
                        rects.extend(combo);
                        ui.add_space(5.);
                        widgets::display_bar(ui, 335., "User Defined Fields");
                        out = grid(
                            ui,
                            &grid_id(ui, "udf_grid"),
                            &[("Title", 248.), ("Data Type", 85.)],
                            fields.len(),
                            selected,
                            Vec2::new(335., 250.),
                            &mut |ui, ev, is_sel| {
                                let Ev::Cell(i, c) = ev else {
                                    return;
                                };
                                cells.push((i, c, ui.max_rect()));
                                let f = &fields[i];
                                if c == 0 {
                                    let (r, _) =
                                        ui.allocate_exact_size(Vec2::new(18., 16.), Sense::hover());
                                    chrome::paint_icon_at(ui.painter(), Icon::Udf, r.min, true);
                                    if let Some(t) = editable_cell(ui, &mut edit, f.id, 0, &f.title)
                                    {
                                        commits.push((f.id, t));
                                    }
                                    return;
                                }
                                let label = ops::data_type_label(&f.logical);
                                // The type is chosen while the field holds no values.
                                if is_sel && f.values == 0 {
                                    let w = ui.available_width() - 2.;
                                    let names: Vec<&str> =
                                        ops::DATA_TYPES.iter().map(|(n, _)| *n).collect();
                                    let cur =
                                        ops::DATA_TYPES.iter().position(|(_, l)| *l == f.logical);
                                    if let (_, Some(idx)) = crate::dialog_kit::flow_combo(
                                        ui,
                                        ("udf_type", f.id),
                                        w,
                                        &names,
                                        cur,
                                        true,
                                    ) {
                                        let (_, logical) = ops::DATA_TYPES[idx];
                                        if logical != f.logical {
                                            type_change = Some((f.id, logical));
                                        }
                                    }
                                } else {
                                    cell(ui, label);
                                }
                            },
                        );
                        if let Some(i) = out.clicked {
                            if selected == Some(i)
                                && edit.is_none()
                                && clicked_column(ui, &cells, i) == Some(0)
                            {
                                edit = Some(CellEdit {
                                    id: fields[i].id,
                                    column: 0,
                                    text: fields[i].title.clone(),
                                    focus: true,
                                });
                            }
                        }
                    });
                    column = col;
                    rects.extend(column_rects);
                });
            });
        for (name, rect) in rects {
            st.rect(name, rect);
        }
        for (r, c, rect) in &cells {
            st.rect(format!("udf_cell_{r}_{c}"), *rect);
        }
        if let Some(i) = out.clicked {
            us.selected = Some(fields[i].id);
        }
        us.edit = edit;
        for (id, text) in commits {
            cmds.push(Cmd::UdfTitle(id, text));
        }
        if let Some((id, logical)) = type_change {
            cmds.push(Cmd::UdfType(id, logical));
        }
        if let Some(k) = area_pick {
            us.area = k;
            us.selected = None;
            us.edit = None;
        }
        match column.clicked.as_deref() {
            Some("udf_close") => open = false,
            Some("udf_add") => {
                us.edit = None;
                us.edit_next = true;
                cmds.push(Cmd::AddUdf(table));
            }
            Some("udf_delete") => {
                if let Some(f) = selected.map(|i| &fields[i]) {
                    if f.values == 0 {
                        cmds.push(Cmd::DeleteUdf(f.id));
                    } else {
                        st.confirm = Some(Confirm {
                            title: "User Defined Fields".into(),
                            text: format!(
                                "{} holds {} values. Deleting the field deletes its values. Continue?",
                                f.title, f.values
                            ),
                            buttons: &["Yes", "No"],
                            then: vec![Some(Cmd::DeleteUdf(f.id)), None],
                            pick_replacement: None,
                        });
                    }
                }
            }
            Some("udf_help") => self.open_help_for("User Defined Fields"),
            _ => {}
        }
        if open {
            st.udf = Some(us);
        }
    }
}
