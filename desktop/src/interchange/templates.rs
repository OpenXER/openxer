//! Saved spreadsheet export templates in the Export wizard: choosing one on
//! the options page, and the editor window for a template's subject area,
//! columns, filter and sort. The templates live in the workspace preferences
//! (`openxer_interchange::export_template`), so `openxer-cli export-xlsx
//! --template` reads the same ones.
use super::*;
use openxer_interchange::export_template::{
    self as et, Condition, ExportTemplate, Op, SortKey, DATE_FORMATS, PREFERENCE_KEY,
};

/// The editor window while it is open.
pub(crate) struct TemplateEditor {
    /// The saved template being edited; `None` for a new one.
    pub index: Option<usize>,
    pub draft: ExportTemplate,
    /// Every column this project offers: (column, title).
    pub catalog: Vec<(Column, String)>,
    /// Row picked in the Available list, and in the Selected list.
    pub pick_available: Option<usize>,
    pub pick_chosen: Option<usize>,
    pub error: Option<String>,
}

impl App {
    /// The saved templates and the entries that were left out of the list.
    pub(crate) fn saved_export_templates(&self) -> (Vec<ExportTemplate>, Vec<String>) {
        let text = self
            .store
            .preference(PREFERENCE_KEY)
            .ok()
            .flatten()
            .unwrap_or_default();
        et::parse_list(&text)
    }

    fn store_export_templates(&mut self, templates: &[ExportTemplate]) {
        if let Err(e) = self
            .store
            .set_preference(PREFERENCE_KEY, &et::list_json(templates))
        {
            self.error(format!("The export templates were not saved: {e}"));
        }
    }

    /// New... (`index` `None`) or Edit... on the options page.
    pub(crate) fn open_template_editor(&mut self, index: Option<usize>) {
        let lookup = Lookup::new(&self.project);
        let catalog: Vec<(Column, String)> = lookup
            .catalog()
            .into_iter()
            .map(|c| (c, c.title(&lookup)))
            .collect();
        let Some(w) = self.interchange.export.as_mut() else {
            return;
        };
        let draft = match index.and_then(|i| w.templates.get(i)) {
            Some(t) => t.clone(),
            None => {
                let first: Vec<Column> = [
                    Column::ActivityId,
                    Column::ActivityName,
                    Column::RemainingDuration,
                    Column::Start,
                    Column::Finish,
                ]
                .into_iter()
                .filter(|c| catalog.iter().any(|(k, _)| k == c))
                .collect();
                ExportTemplate::new("", &first)
            }
        };
        w.editor = Some(TemplateEditor {
            index,
            draft,
            catalog,
            pick_available: None,
            pick_chosen: None,
            error: None,
        });
    }

    /// Delete the template chosen on the options page.
    pub(crate) fn delete_chosen_template(&mut self) {
        let Some(w) = self.interchange.export.as_mut() else {
            return;
        };
        let Some(k) = w.template.take() else {
            return;
        };
        if k < w.templates.len() {
            w.templates.remove(k);
        }
        let list = w.templates.clone();
        self.store_export_templates(&list);
    }

    /// Save the editor's draft (Save): checked, then added or replaced by name.
    fn save_template_draft(&mut self) {
        let Some(w) = self.interchange.export.as_mut() else {
            return;
        };
        let Some(editor) = w.editor.as_mut() else {
            return;
        };
        let mut draft = editor.draft.clone();
        draft.name = draft.name.trim().to_string();
        if let Err(e) = draft.validate() {
            editor.error = Some(e);
            return;
        }
        let taken = w.templates.iter().enumerate().any(|(k, t)| {
            Some(k) != editor.index && t.name.trim().eq_ignore_ascii_case(&draft.name)
        });
        if taken {
            editor.error = Some(format!("A template named {:?} already exists.", draft.name));
            return;
        }
        let at = match editor.index.filter(|&k| k < w.templates.len()) {
            Some(k) => {
                w.templates[k] = draft;
                k
            }
            None => {
                w.templates.push(draft);
                w.templates.len() - 1
            }
        };
        w.template = Some(at);
        w.editor = None;
        let list = w.templates.clone();
        self.store_export_templates(&list);
    }

    /// The editor window; Save keeps the template, Cancel and Escape drop it.
    pub(crate) fn template_editor_dialog(&mut self, ctx: &egui::Context) {
        let Some(editor) = self
            .interchange
            .export
            .as_mut()
            .and_then(|w| w.editor.as_mut())
        else {
            return;
        };
        let id = egui::Id::new("export_template_editor");
        let mut rects: Vec<(String, Rect)> = vec![];
        let (mut save, mut cancel) = (false, false);
        let mut open = true;
        crate::chrome::plain_window("Export Template")
            .id(id)
            .open(&mut open)
            .order(egui::Order::Foreground)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_width(640.);
                template_form(ui, editor, &mut rects);
                if let Some(e) = &editor.error {
                    ui.add_space(4.);
                    ui.add(egui::Label::new(RichText::new(e).color(CRITICAL)).wrap());
                }
                ui.add_space(6.);
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = crate::dialog_kit::ROW_GAP;
                        let b = crate::chrome::dialog_button(
                            ui,
                            &crate::chrome::DialogButton::cancel(),
                        );
                        rects.push(("template_cancel".into(), b.rect));
                        cancel |= b.clicked();
                        let b = crate::chrome::dialog_button(
                            ui,
                            &crate::chrome::DialogButton::new("Save", None),
                        );
                        rects.push(("template_save".into(), b.rect));
                        save |= b.clicked();
                    });
                });
            });
        self.record_dialog_actions(rects);
        let escape = crate::kit_window::escape_goes_to(ctx, id, egui::Order::Foreground);
        if save {
            self.save_template_draft();
        } else if cancel || !open || escape {
            if let Some(w) = self.interchange.export.as_mut() {
                w.editor = None;
            }
        }
    }
}

fn combo_column(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash,
    catalog: &[(Column, String)],
    key: &mut String,
    width: f32,
) {
    let shown = Column::from_key(key)
        .and_then(|c| catalog.iter().find(|(k, _)| *k == c))
        .map_or("(choose a column)", |(_, t)| t.as_str());
    crate::dialog_kit::flow_combo_with(ui, salt, width, shown, true, |ui| {
        for (column, title) in catalog {
            let k = column.key();
            if crate::dialog_kit::flow_selectable(ui, *key == k, title, width - 4.).clicked() {
                *key = k;
                ui.close();
            }
        }
    });
}

fn template_form(ui: &mut egui::Ui, e: &mut TemplateEditor, rects: &mut Vec<(String, Rect)>) {
    ui.horizontal(|ui| {
        ui.label("Name");
        let r = crate::dialog_kit::flow_edit(ui, "template_name", &mut e.draft.name, 240., true);
        rects.push(("template_name".into(), r.rect));
        ui.add_space(12.);
        ui.label("Subject area");
        if let (_, Some(_)) = crate::dialog_kit::flow_combo(
            ui,
            "template_subject",
            140.,
            &[et::SUBJECT_ACTIVITIES],
            Some(0),
            true,
        ) {
            e.draft.subject = et::SUBJECT_ACTIVITIES.to_string();
        }
    });
    ui.add_space(4.);
    ui.label(RichText::new("Columns").strong());
    let chosen: Vec<Column> = e
        .draft
        .columns
        .iter()
        .filter_map(|k| Column::from_key(k))
        .collect();
    let available: Vec<usize> = (0..e.catalog.len())
        .filter(|&i| !chosen.contains(&e.catalog[i].0))
        .collect();
    let title_of = |c: &Column, catalog: &[(Column, String)]| {
        catalog
            .iter()
            .find(|(k, _)| k == c)
            .map_or_else(|| c.key(), |(_, t)| t.clone())
    };
    let (mut add, mut remove, mut up, mut down) = (false, false, false, false);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.label("Available");
            egui::ScrollArea::vertical()
                .id_salt("template_available")
                .max_height(150.)
                .min_scrolled_height(150.)
                .show(ui, |ui| {
                    ui.set_width(220.);
                    for (n, &i) in available.iter().enumerate() {
                        let on = e.pick_available == Some(n);
                        let r = crate::dialog_kit::flow_selectable(ui, on, &e.catalog[i].1, 210.);
                        if r.clicked() {
                            e.pick_available = Some(n);
                        }
                        if r.double_clicked() {
                            add = true;
                        }
                    }
                });
        });
        ui.vertical(|ui| {
            ui.add_space(24.);
            for (label, key, flag) in [
                ("Add >", "template_add", &mut add),
                ("< Remove", "template_remove", &mut remove),
                ("Move Up", "template_up", &mut up),
                ("Move Down", "template_down", &mut down),
            ] {
                let b = crate::chrome::command_button(ui, label, crate::dialog_kit::BUTTON, true);
                rects.push((key.into(), b.rect));
                *flag |= b.clicked();
            }
        });
        ui.vertical(|ui| {
            ui.label("Selected");
            egui::ScrollArea::vertical()
                .id_salt("template_chosen")
                .max_height(150.)
                .min_scrolled_height(150.)
                .show(ui, |ui| {
                    ui.set_width(220.);
                    for (n, c) in chosen.iter().enumerate() {
                        let on = e.pick_chosen == Some(n);
                        if crate::dialog_kit::flow_selectable(
                            ui,
                            on,
                            &title_of(c, &e.catalog),
                            210.,
                        )
                        .clicked()
                        {
                            e.pick_chosen = Some(n);
                        }
                    }
                });
        });
    });
    if add {
        if let Some(&i) = e.pick_available.and_then(|n| available.get(n)) {
            e.draft.columns.push(e.catalog[i].0.key());
            if e.pick_available.is_some_and(|n| n >= available.len() - 1) {
                e.pick_available = None;
            }
        }
    }
    if remove {
        if let Some(n) = e.pick_chosen.filter(|&n| n < e.draft.columns.len()) {
            e.draft.columns.remove(n);
            e.pick_chosen = None;
        }
    }
    if let Some(n) = e.pick_chosen.filter(|&n| n < e.draft.columns.len()) {
        if up && n > 0 {
            e.draft.columns.swap(n, n - 1);
            e.pick_chosen = Some(n - 1);
        } else if down && n + 1 < e.draft.columns.len() {
            e.draft.columns.swap(n, n + 1);
            e.pick_chosen = Some(n + 1);
        }
    }
    ui.add_space(4.);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Filter").strong());
        crate::dialog_kit::flow_radio(ui, &mut e.draft.match_all, true, "Match all", true);
        crate::dialog_kit::flow_radio(ui, &mut e.draft.match_all, false, "Match any", true);
        let b = crate::chrome::command_button(ui, "Add filter line", Vec2::new(0., 21.), true);
        rects.push(("template_add_filter".into(), b.rect));
        if b.clicked() && e.draft.filter.len() < 20 {
            e.draft.filter.push(Condition {
                column: e.catalog.first().map(|(c, _)| c.key()).unwrap_or_default(),
                op: Op::Is,
                value: String::new(),
            });
        }
    });
    let op_labels: Vec<&str> = Op::ALL.iter().map(|op| op.label()).collect();
    let mut drop_filter = None;
    for (n, c) in e.draft.filter.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            combo_column(
                ui,
                ("template_filter_column", n),
                &e.catalog,
                &mut c.column,
                170.,
            );
            let sel = Op::ALL.iter().position(|&op| op == c.op);
            if let (_, Some(idx)) = crate::dialog_kit::flow_combo(
                ui,
                ("template_filter_op", n),
                170.,
                &op_labels,
                sel,
                true,
            ) {
                c.op = Op::ALL[idx];
            }
            let takes = !matches!(c.op, Op::IsEmpty | Op::IsNotEmpty);
            crate::dialog_kit::flow_edit(ui, ("template_filter_val", n), &mut c.value, 150., takes);
            if crate::chrome::command_button(ui, "Remove", Vec2::new(0., 21.), true).clicked() {
                drop_filter = Some(n);
            }
        });
    }
    if let Some(n) = drop_filter {
        e.draft.filter.remove(n);
    }
    ui.add_space(4.);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Sort").strong());
        let b = crate::chrome::command_button(ui, "Add sort key", Vec2::new(0., 21.), true);
        rects.push(("template_add_sort".into(), b.rect));
        if b.clicked() && e.draft.sort.len() < 10 {
            e.draft.sort.push(SortKey {
                column: e.draft.columns.first().cloned().unwrap_or_default(),
                descending: false,
            });
        }
    });
    let mut drop_sort = None;
    for (n, s) in e.draft.sort.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            combo_column(
                ui,
                ("template_sort_column", n),
                &e.catalog,
                &mut s.column,
                170.,
            );
            let sel = Some(usize::from(s.descending));
            if let (_, Some(idx)) = crate::dialog_kit::flow_combo(
                ui,
                ("template_sort_order", n),
                110.,
                &["Ascending", "Descending"],
                sel,
                true,
            ) {
                s.descending = idx == 1;
            }
            if crate::chrome::command_button(ui, "Remove", Vec2::new(0., 21.), true).clicked() {
                drop_sort = Some(n);
            }
        });
    }
    if let Some(n) = drop_sort {
        e.draft.sort.remove(n);
    }
    ui.add_space(4.);
    ui.horizontal(|ui| {
        ui.label("Durations, float and lag in");
        crate::dialog_kit::flow_radio(ui, &mut e.draft.unit, DurationUnit::Days, "Days", true);
        crate::dialog_kit::flow_radio(ui, &mut e.draft.unit, DurationUnit::Hours, "Hours", true);
        ui.add_space(8.);
        ui.label("Date format");
        let sel = DATE_FORMATS.iter().position(|&f| f == e.draft.date_format);
        if let (_, Some(idx)) = crate::dialog_kit::flow_combo(
            ui,
            "template_date_format",
            140.,
            &DATE_FORMATS,
            sel,
            true,
        ) {
            e.draft.date_format = DATE_FORMATS[idx].to_string();
        }
    });
    crate::dialog_kit::flow_checkbox(
        ui,
        &mut e.draft.group_by_wbs,
        "Group by WBS (bold band rows with outline levels)",
        true,
    );
}
