//! Admin Categories: vertical tabs for the category lists an XER file carries
//! (baseline types, expense categories, document categories and status, risk
//! categories, notebook topics, units of measure), each a list with Add,
//! Delete and Shift buttons. WBS Categories are not in XER files and are
//! shown disabled.
use super::*;
use ops::Category;

pub struct AdminState {
    pub tab: Category,
    pub selected: Option<i64>,
    pub edit: Option<CellEdit>,
    pub edit_next: bool,
    /// Select mode: the picker that opened the dialog and the resource it
    /// sets. Select puts the chosen row there and closes the dialog.
    pub select: Option<(crate::views::resources::Pick, i64)>,
}

impl Default for AdminState {
    fn default() -> Self {
        Self {
            tab: Category::Expense,
            selected: None,
            edit: None,
            edit_next: false,
            select: None,
        }
    }
}

/// Client size, and the page beside the tabs (x, y, w, h).
const SIZE: Vec2 = Vec2::new(572., 374.);
const PAGE: (f32, f32, f32, f32) = (144., 5., 311., 364.);

fn tab_icon(c: Category) -> Icon {
    match c {
        Category::Baseline => Icon::Baselines,
        Category::Expense => Icon::Expenses,
        Category::Documents | Category::DocStatus => Icon::WorkProducts,
        Category::Risk => Icon::Risks,
        Category::Notebook => Icon::Details,
        Category::Units => Icon::Resources,
    }
}

impl App {
    pub(super) fn admin_dialog(
        &mut self,
        ctx: &egui::Context,
        st: &mut DictionaryState,
        cmds: &mut Vec<Cmd>,
        escape: bool,
    ) {
        let Some(mut a) = st.admin.take() else {
            return;
        };
        let busy = st.busy();
        let category = a.tab;
        let spec = category.spec();
        let rows = ops::categories(&self.project.document, category);
        if !rows.iter().any(|r| Some(r.id) == a.selected) {
            a.selected = rows.first().map(|r| r.id);
        }
        if std::mem::take(&mut a.edit_next) {
            if let Some(r) = rows.iter().find(|r| Some(r.id) == a.selected) {
                a.edit = Some(CellEdit {
                    id: r.id,
                    column: 0,
                    text: r.cells[0].clone(),
                    focus: true,
                });
            }
        }
        let selected = rows.iter().position(|r| Some(r.id) == a.selected);
        let sel = selected.map(|i| &rows[i]);
        let sibling = |forward: bool| {
            sel.is_some_and(|x| {
                let i = selected.unwrap();
                if forward {
                    rows[i + 1..]
                        .iter()
                        .take_while(|w| w.depth >= x.depth)
                        .any(|w| w.depth == x.depth)
                } else {
                    rows[..i]
                        .iter()
                        .rev()
                        .take_while(|w| w.depth >= x.depth)
                        .any(|w| w.depth == x.depth)
                }
            })
        };
        let tree = spec.tree.parent.is_some();
        // The standard column: Close (Enter), Select in select mode, Add,
        // Delete, the move pad (Shift Left and Right in a tree) and Help.
        let mut entries = vec![Entry::close("admin_close")];
        if a.select.is_some() {
            entries.push(Entry::Button(
                "admin_select",
                "Select",
                a.selected.is_some(),
                false,
            ));
        }
        entries.extend([
            Entry::Button("admin_add", "Add", true, true),
            Entry::Button("admin_delete", "Delete", sel.is_some(), false),
            Entry::Pad(
                "admin",
                [
                    tree && sel.is_some_and(|x| x.depth > 0),
                    sibling(false),
                    sibling(true),
                    tree && sibling(false),
                ],
            ),
            Entry::help("admin_help"),
        ]);
        let columns: Vec<(&str, &str, f32)> = spec.columns.to_vec();
        let mut open = !(escape && a.edit.is_none());
        let mut edit = a.edit.take();
        let mut cells: Vec<(usize, usize, Rect)> = vec![];
        let mut commits: Vec<(i64, usize, String)> = vec![];
        let mut flags: Vec<(i64, usize, bool)> = vec![];
        let mut out = crate::details::GridOut::default();
        let mut tab_click = None;
        let mut column = kit::Column::default();
        let mut rects = vec![];
        let (heading, caption) = category.heading();
        let tabs: Vec<kit::VTab> = Category::ALL
            .iter()
            .map(|c| kit::VTab {
                label: c.label(),
                key: c.key(),
                unavailable: None,
            })
            .collect();
        let icons: Vec<Option<Icon>> = Category::ALL.iter().map(|c| Some(tab_icon(*c))).collect();
        let active = Category::ALL
            .iter()
            .position(|c| *c == category)
            .unwrap_or(0);
        chrome::dialog_window("Categories and Lists")
            .id(egui::Id::new("admin_categories_dialog"))
            .default_pos([360., 130.])
            .show(ctx, |ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    let (_, r) = kit::flow_canvas(ui, "admin", SIZE, |c| {
                        let (px, py, pw, ph) = PAGE;
                        // The page first: the active tab merges with it.
                        kit::page(c, px, py, pw, ph);
                        if let Some(k) = kit::vertical_tabs_with_icons(
                            c,
                            "admin_tab_",
                            5.,
                            py,
                            px - 4.,
                            &tabs,
                            &icons,
                            active,
                        ) {
                            tab_click = Some(Category::ALL[k]);
                        }
                        let (x, y) = (px + 7., py + 7.);
                        kit::heading(c, x, y, pw - 14., heading);
                        // The list: the details grid under its Display bar.
                        let area = c.r(x, y + 25., pw - 14., ph - 39.);
                        let mut list = c.ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(area)
                                .layout(egui::Layout::top_down(egui::Align::Min)),
                        );
                        list.spacing_mut().item_spacing = Vec2::new(4., 4.);
                        widgets::display_bar(&mut list, pw - 14., caption);
                        let cols: Vec<(&str, f32)> =
                            columns.iter().map(|(c, _, w)| (*c, *w)).collect();
                        let id = grid_id(&list, &format!("admin_{}", category.key()));
                        out = grid(
                            &mut list,
                            &id,
                            &cols,
                            rows.len(),
                            selected,
                            Vec2::new(pw - 14., ph - 61.),
                            &mut |ui, ev, _| {
                                let Ev::Cell(i, c) = ev else {
                                    return;
                                };
                                cells.push((i, c, ui.max_rect()));
                                let r = &rows[i];
                                let column = columns[c].1;
                                if column.ends_with("_flag") {
                                    let mut on = r.cells[c] == "Y";
                                    ui.add_space(12.);
                                    if ui.checkbox(&mut on, "").clicked() {
                                        flags.push((r.id, c, on));
                                    }
                                    return;
                                }
                                if c == 0 {
                                    ui.add_space(r.depth as f32 * 14.);
                                    let (rect, _) =
                                        ui.allocate_exact_size(Vec2::new(18., 16.), Sense::hover());
                                    chrome::paint_icon_at(
                                        ui.painter(),
                                        tab_icon(category),
                                        rect.min,
                                        true,
                                    );
                                }
                                if let Some(t) = editable_cell(ui, &mut edit, r.id, c, &r.cells[c])
                                {
                                    commits.push((r.id, c, t));
                                }
                            },
                        );
                        if let Some(i) = out.clicked {
                            if selected == Some(i) && edit.is_none() {
                                if let Some(c) = clicked_column(&list, &cells, i)
                                    .filter(|c| !columns[*c].1.ends_with("_flag"))
                                {
                                    edit = Some(CellEdit {
                                        id: rows[i].id,
                                        column: c,
                                        text: rows[i].cells[c].clone(),
                                        focus: true,
                                    });
                                }
                            }
                        }
                        kit::strip(c, SIZE.x, SIZE.y);
                        column = kit::button_column(c, SIZE.x, &entries);
                    });
                    rects = r;
                });
            });
        for (name, rect) in rects {
            st.rect(name, rect);
        }
        for (r, c, rect) in &cells {
            st.rect(format!("admin_cell_{r}_{c}"), *rect);
        }
        // The pad's arrows are the Shift commands.
        let clicked = column.clicked.or(column.moved.map(|m| {
            match m {
                Move::Up => "admin_up",
                Move::Down => "admin_down",
                Move::Left => "admin_left",
                Move::Right => "admin_right",
            }
            .to_string()
        }));
        if let Some(i) = out.clicked {
            a.selected = Some(rows[i].id);
        }
        a.edit = edit;
        for (id, c, text) in commits {
            cmds.push(Cmd::SetCategory(category, id, columns[c].1, text));
        }
        for (id, c, on) in flags {
            let flag = if on { "Y" } else { "N" };
            cmds.push(Cmd::SetCategory(category, id, columns[c].1, flag.into()));
            a.selected = Some(id);
        }
        if let Some(t) = tab_click {
            if t != a.tab {
                a.tab = t;
                a.selected = None;
                a.edit = None;
            }
        }
        let tree_spec = ops::category_tree(category);
        match clicked.as_deref() {
            Some("admin_close") => open = false,
            Some("admin_select") => {
                if let (Some((pick, rsrc)), Some(id)) = (a.select, a.selected) {
                    st.chosen = Some((pick, rsrc, id.to_string()));
                    open = false;
                }
            }
            Some("admin_help") => self.open_help_for("Categories and Lists"),
            Some("admin_add") => {
                a.edit = None;
                a.edit_next = true;
                cmds.push(Cmd::AddCategory(category, a.selected));
            }
            Some("admin_delete") => {
                if let Some(x) = sel {
                    let mut used = ops::category_usage(&self.project.document, category, x.id);
                    if category == Category::Baseline {
                        used += self.baselines_with_type(x.id).len();
                    }
                    let name = &x.cells[0];
                    if used == 0 {
                        cmds.push(Cmd::DeleteCategory(category, x.id));
                    } else {
                        let text = if category == Category::Notebook {
                            format!(
                                "{name} holds {used} notebooks. Deleting the topic deletes those notebooks. Continue?"
                            )
                        } else if category == Category::Baseline {
                            format!(
                                "{name} is the type of {used} baselines. Deleting it leaves them without a type. Continue?"
                            )
                        } else {
                            format!(
                                "{name} is used by {used} rows. Deleting it leaves them without a value. Continue?"
                            )
                        };
                        st.confirm = Some(Confirm {
                            title: "Categories and Lists".into(),
                            text,
                            buttons: &["Yes", "No"],
                            then: vec![Some(Cmd::DeleteCategory(category, x.id)), None],
                            pick_replacement: None,
                        });
                    }
                }
            }
            Some(dir @ ("admin_up" | "admin_down" | "admin_left" | "admin_right")) => {
                if let Some(id) = a.selected {
                    let m = match dir {
                        "admin_up" => Move::Up,
                        "admin_down" => Move::Down,
                        "admin_left" => Move::Left,
                        _ => Move::Right,
                    };
                    cmds.push(Cmd::Move(tree_spec, None, id, m));
                }
            }
            _ => {}
        }
        if open {
            st.admin = Some(a);
        }
    }
}
