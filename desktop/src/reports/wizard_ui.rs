//! Tools > Report Wizard and Tools > Reports > Reports.
//! The wizard pages follow the reference order: subject area, columns with
//! filter and group and sort, title, then run and save.
use super::doc::{self, Output, Report};
use super::wizard::{self, Context, Definition, Subject};
use crate::dialog_kit as kit;
use crate::grid::layout::SortKey;
use crate::schedule_dialogs::modal;
use crate::App;
use eframe::egui::{self, Align, Key, Vec2};
use std::path::PathBuf;

pub const PAGES: [&str; 4] = [
    "Subject Area",
    "Columns, Filter, Group and Sort",
    "Report Title",
    "Run and Save",
];

pub struct WizardState {
    pub page: usize,
    /// 0 New Report, 1 Use Current Screen, 2 Modify Wizard Report.
    pub start: usize,
    pub subject: Subject,
    /// The saved report to modify (index into the saved list).
    pub modify: usize,
    pub def: Definition,
    /// The saved report this wizard edits, if any.
    pub editing: Option<usize>,
    /// The report group a new report is saved in (the group selected in the
    /// Reports view when Add was pressed).
    pub group: Option<u32>,
    pub available_row: Option<usize>,
    pub selected_row: Option<usize>,
    pub save_name: String,
    pub note: Option<String>,
    pub error: Option<String>,
}

impl App {
    pub(crate) fn report_definitions(&self) -> Vec<Definition> {
        self.store
            .preference(wizard::DEFINITIONS_KEY)
            .ok()
            .flatten()
            .map(|t| wizard::definitions_from_json(&t))
            .unwrap_or_default()
    }

    pub(crate) fn save_report_definitions(&mut self, list: &[Definition]) {
        if let Err(e) = self
            .store
            .set_preference(wizard::DEFINITIONS_KEY, &wizard::definitions_to_json(list))
        {
            self.error(e);
        }
    }

    /// Tools menu: the Reports submenu and Report Wizard.
    pub(crate) fn reports_menu_items(&mut self, ui: &mut egui::Ui) {
        crate::chrome::submenu(
            ui,
            crate::chrome::Item::new("Re&ports").icon(crate::chrome::Icon::Reports),
            |ui| {
                if self.item(ui, "menu_reports", crate::chrome::Item::new("&Reports")) {
                    self.show_view(crate::views::MainView::Reports);
                }
                if self.item(
                    ui,
                    "menu_report_groups",
                    crate::chrome::Item::new("Report &Groups..."),
                ) {
                    self.open_report_groups();
                }
                if self.item(
                    ui,
                    "menu_batch_reports",
                    crate::chrome::Item::new("&Batch Reports..."),
                ) {
                    self.open_batch_reports();
                }
                if self.item(ui, "menu_scurve", crate::chrome::Item::new("S-&Curve...")) {
                    self.open_scurve();
                }
                if self.item(
                    ui,
                    "menu_earned_hours",
                    crate::chrome::Item::new("&Earned Hours..."),
                ) {
                    self.open_earned_hours();
                }
            },
        );
        if self.item(
            ui,
            "menu_report_wizard",
            crate::chrome::Item::new("Report &Wizard...").icon(crate::chrome::Icon::ReportWizard),
        ) {
            self.open_report_wizard();
        }
    }

    pub(crate) fn open_report_wizard(&mut self) {
        self.open_report_wizard_in(None);
    }

    /// Starts the Report Wizard; a new report goes in `group`.
    pub(crate) fn open_report_wizard_in(&mut self, group: Option<u32>) {
        self.reports.wizard = Some(WizardState {
            group,
            page: 0,
            start: 0,
            subject: Subject::Activities,
            modify: 0,
            def: Definition::new(Subject::Activities),
            editing: None,
            available_row: None,
            selected_row: None,
            save_name: String::new(),
            note: None,
            error: None,
        });
    }

    /// Open the wizard on a saved report.
    pub(crate) fn modify_report(&mut self, index: usize) {
        let saved = self.report_definitions();
        let Some(def) = saved.get(index).cloned() else {
            return;
        };
        self.open_report_wizard();
        if let Some(w) = self.reports.wizard.as_mut() {
            w.start = 2;
            w.modify = index;
            w.subject = def.subject;
            w.save_name = def.name.clone();
            w.def = def;
            w.editing = Some(index);
            w.page = 1;
        }
    }

    pub(crate) fn build_report(&self, def: &Definition) -> Report {
        let ix = self.grid_index();
        wizard::build(
            def,
            &self.project,
            &Context {
                ix: &ix,
                users: &self.grid.user_filters,
            },
        )
    }

    /// Run a definition: a report tab, or the file it names.
    pub(crate) fn run_report(&mut self, def: &Definition) -> Result<(), String> {
        match def.output {
            None => {
                self.run_definition_in_tab(def);
                Ok(())
            }
            Some(format) => {
                let report = self.build_report(def);
                let path = PathBuf::from(def.path.trim());
                if def.path.trim().is_empty() {
                    return Err("Choose a file for the report.".into());
                }
                doc::write(&report, format, &path)?;
                self.status = format!("Report saved to {}", path.display());
                Ok(())
            }
        }
    }

    /// Save the wizard's definition under its name (replacing a saved
    /// report of the same name).
    pub(crate) fn save_wizard_report(&mut self) -> Result<(), String> {
        let w = self
            .reports
            .wizard
            .as_ref()
            .ok_or("The wizard is not open.")?;
        let name = w.save_name.trim().to_string();
        if name.is_empty() {
            return Err("Enter a name for the report.".into());
        }
        let mut def = w.def.clone();
        def.name = name.clone();
        let editing = w.editing;
        let w_group = w.group;
        let mut list = self.report_definitions();
        let index = match editing.filter(|&i| i < list.len()) {
            Some(i) => {
                // Editing keeps what the list holds now: its group, scope and
                // last run may have changed since the wizard opened.
                def.group = list[i].group;
                def.scope = list[i].scope.clone();
                def.last_run = list[i].last_run.clone();
                list[i] = def;
                i
            }
            None => match list.iter().position(|d| d.name == name) {
                Some(i) => {
                    // Saving over a report of the same name keeps its place.
                    def.group = list[i].group;
                    def.scope = list[i].scope.clone();
                    def.last_run = list[i].last_run.clone();
                    list[i] = def;
                    i
                }
                None => {
                    def.group = def.group.or(w_group);
                    list.push(def);
                    list.len() - 1
                }
            },
        };
        self.save_report_definitions(&list);
        if let Some(w) = self.reports.wizard.as_mut() {
            w.editing = Some(index);
            w.note = Some(format!("Saved as \"{name}\"."));
        }
        Ok(())
    }

    /// Move the wizard to `page`, settling the definition when leaving the
    /// first page.
    pub(crate) fn wizard_go(&mut self, page: usize) {
        let saved = self.report_definitions();
        let layout = self.grid.layout.clone();
        let Some(w) = self.reports.wizard.as_mut() else {
            return;
        };
        if w.page == 0 && page > 0 {
            match w.start {
                1 => {
                    w.def = Definition::from_layout(&layout);
                    w.editing = None;
                }
                2 => match saved.get(w.modify) {
                    Some(d) => {
                        w.def = d.clone();
                        w.save_name = d.name.clone();
                        w.editing = Some(w.modify);
                    }
                    None => {
                        w.error = Some("There is no saved report to modify.".into());
                        return;
                    }
                },
                _ => {
                    if w.def.subject != w.subject || w.editing.is_some() {
                        w.def = Definition::new(w.subject);
                        w.editing = None;
                    }
                }
            }
            w.available_row = None;
            w.selected_row = None;
        }
        w.error = None;
        w.page = page.min(PAGES.len() - 1);
    }

    pub(crate) fn report_dialogs(&mut self, ctx: &egui::Context) {
        self.reports.escape = ctx.input(|i| i.key_pressed(Key::Escape));
        // The topmost dialog first: it takes Escape.
        if self.reports.wizard.is_some() {
            if self.reports.take_escape() {
                self.reports.wizard = None;
            } else {
                self.wizard_dialog(ctx);
            }
        }
        self.scurve_dialog(ctx);
        self.earned_hours_dialog(ctx);
        self.report_page_preview(ctx);
        self.run_report_dialog(ctx);
        self.report_text_export_dialog(ctx);
        self.report_import_window(ctx);
        self.report_export_window(ctx);
        self.report_editor_window(ctx);
        self.report_group_pick_dialog(ctx);
        self.run_batch_dialog(ctx);
        self.report_groups_window(ctx);
        self.batch_reports_window(ctx);
    }

    fn wizard_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.reports.wizard.take() else {
            return;
        };
        const W: f32 = 640.;
        const H: f32 = 440.;
        #[derive(Default)]
        struct Clicks {
            cancel: bool,
            prev: bool,
            next: bool,
            finish: bool,
            run: bool,
            save: bool,
            browse: bool,
        }
        let saved = self.report_definitions();
        let available = wizard::available(w.def.subject, &self.project);
        let filters = wizard::filter_choices(&self.grid.user_filters);
        let (k, closed, actions) = modal(
            ctx,
            "report_wizard",
            "Report Wizard",
            Vec2::new(W, H),
            |c| {
                let mut k = Clicks::default();
                c.fill(0., 0., W, H, kit::FACE);
                kit::wizard_title(c, 16., 12., PAGES[w.page]);
                c.lowered(5., 48., W - 10., H - 92.);
                match w.page {
                    0 => {
                        kit::text(c, 20., 62., "You can create a new report, or a report based on the current screen (the activity layout).", true);
                        let starts = ["New Report", "Use Current Screen", "Modify Wizard Report"];
                        for (n, label) in starts.iter().enumerate() {
                            let enabled = n != 2 || !saved.is_empty();
                            if c.radio(
                                &format!("wizard_start_{n}"),
                                26.,
                                90. + 24. * n as f32,
                                200.,
                                17.,
                                label,
                                w.start == n,
                                enabled,
                            ) {
                                w.start = n;
                            }
                        }
                        if !saved.is_empty() {
                            let names: Vec<String> = saved.iter().map(|d| d.name.clone()).collect();
                            let items: Vec<(&str, bool)> =
                                names.iter().map(|n| (n.as_str(), true)).collect();
                            c.combo(
                                "wizard_modify",
                                236.,
                                136.,
                                260.,
                                22.,
                                &items,
                                &mut w.modify,
                                w.start == 2,
                            );
                        }
                        kit::section(
                            c,
                            16.,
                            180.,
                            340.,
                            140.,
                            "Select the subject area for the report.",
                        );
                        for (n, s) in Subject::ALL.iter().enumerate() {
                            if c.radio(
                                &format!("wizard_subject_{}", s.key()),
                                30.,
                                210. + 24. * n as f32,
                                280.,
                                17.,
                                s.caption(),
                                w.subject == *s,
                                w.start == 0,
                            ) {
                                w.subject = *s;
                            }
                        }
                    }
                    1 => {
                        let def = &mut w.def;
                        kit::text(
                            c,
                            16.,
                            56.,
                            "Configure columns, group and sort, and filter options.",
                            true,
                        );
                        let unused: Vec<&(String, String)> = available
                            .iter()
                            .filter(|(id, _)| !def.columns.contains(id))
                            .collect();
                        let title = |id: &str| {
                            available
                                .iter()
                                .find(|(i, _)| i == id)
                                .map_or(id.to_string(), |(_, t)| t.clone())
                        };
                        let out = kit::list(
                            c,
                            "wizard_available",
                            (16., 76., 230., 200.),
                            None,
                            &[kit::Col {
                                title: "Available Options",
                                width: 210.,
                            }],
                            unused.len(),
                            w.available_row,
                            None,
                            &mut |c, i, _, rect, s| kit::cell_text(c, rect, &unused[i].1, s, 3.),
                        );
                        if let Some(i) = out.clicked {
                            w.available_row = Some(i);
                        }
                        let add = kit::button(
                            c,
                            "wizard_add",
                            256.,
                            96.,
                            108.,
                            25.,
                            "Add >",
                            w.available_row.is_some_and(|r| r < unused.len()),
                        );
                        let remove = kit::button(
                            c,
                            "wizard_remove",
                            256.,
                            126.,
                            108.,
                            25.,
                            "< Remove",
                            w.selected_row.is_some() && def.columns.len() > 1,
                        );
                        let up = kit::button(
                            c,
                            "wizard_up",
                            256.,
                            166.,
                            108.,
                            25.,
                            "Move Up",
                            w.selected_row.is_some_and(|r| r > 0),
                        );
                        let down = kit::button(
                            c,
                            "wizard_down",
                            256.,
                            196.,
                            108.,
                            25.,
                            "Move Down",
                            w.selected_row.is_some_and(|r| r + 1 < def.columns.len()),
                        );
                        let chosen: Vec<String> = def.columns.iter().map(|id| title(id)).collect();
                        let out = kit::list(
                            c,
                            "wizard_selected",
                            (374., 76., 250., 200.),
                            None,
                            &[kit::Col {
                                title: "Selected Options",
                                width: 230.,
                            }],
                            chosen.len(),
                            w.selected_row,
                            None,
                            &mut |c, i, _, rect, s| kit::cell_text(c, rect, &chosen[i], s, 3.),
                        );
                        if let Some(i) = out.clicked {
                            w.selected_row = Some(i);
                        }
                        if add {
                            if let Some(id) = w
                                .available_row
                                .and_then(|r| unused.get(r))
                                .map(|(id, _)| id.clone())
                            {
                                def.columns.push(id);
                                w.available_row = None;
                            }
                        }
                        if remove {
                            if let Some(r) =
                                w.selected_row.take().filter(|r| *r < def.columns.len())
                            {
                                def.columns.remove(r);
                            }
                        }
                        if let Some(r) = w.selected_row {
                            if up && r > 0 {
                                def.columns.swap(r, r - 1);
                                w.selected_row = Some(r - 1);
                            } else if down && r + 1 < def.columns.len() {
                                def.columns.swap(r, r + 1);
                                w.selected_row = Some(r + 1);
                            }
                        }
                        // Filter.
                        let activities = def.subject == Subject::Activities;
                        c.label(16., 292., 17., "Filter", true);
                        let mut names: Vec<String> = vec!["All Activities".into()];
                        names.extend(filters.iter().map(|(_, n)| n.clone()));
                        let items: Vec<(&str, bool)> =
                            names.iter().map(|n| (n.as_str(), true)).collect();
                        let mut pick = def
                            .filters
                            .first()
                            .and_then(|k| filters.iter().position(|(key, _)| key == k))
                            .map_or(0, |p| p + 1);
                        if c.combo(
                            "wizard_filter",
                            104.,
                            290.,
                            200.,
                            22.,
                            &items,
                            &mut pick,
                            activities,
                        ) {
                            def.filters = if pick == 0 {
                                vec![]
                            } else {
                                vec![filters[pick - 1].0.clone()]
                            };
                        }
                        c.label(320., 292., 17., "Containing", true);
                        c.edit(
                            "wizard_contains",
                            394.,
                            290.,
                            230.,
                            22.,
                            &mut def.contains,
                            true,
                            Align::Min,
                        );
                        // Group and sort.
                        c.label(16., 322., 17., "Group By", true);
                        let mut groups: Vec<(Option<String>, String)> =
                            vec![(None, "(None)".into())];
                        if activities {
                            groups.push((Some("wbs".into()), "WBS".into()));
                            for (id, t) in &available {
                                if id.starts_with("code:") {
                                    groups.push((Some(id.clone()), t.clone()));
                                }
                            }
                        }
                        for (id, t) in def.columns.iter().zip(&chosen) {
                            if !groups.iter().any(|g| g.0.as_deref() == Some(id.as_str())) {
                                groups.push((Some(id.clone()), t.clone()));
                            }
                        }
                        let items: Vec<(&str, bool)> =
                            groups.iter().map(|g| (g.1.as_str(), true)).collect();
                        let mut pick = groups.iter().position(|g| g.0 == def.group_by).unwrap_or(0);
                        if c.combo(
                            "wizard_group",
                            104.,
                            320.,
                            200.,
                            22.,
                            &items,
                            &mut pick,
                            true,
                        ) {
                            def.group_by = groups[pick].0.clone();
                        }
                        c.label(320., 322., 17., "Sort By", true);
                        let items: Vec<(&str, bool)> =
                            chosen.iter().map(|t| (t.as_str(), true)).collect();
                        let mut pick = def
                            .sort
                            .first()
                            .and_then(|s| def.columns.iter().position(|c| *c == s.column))
                            .unwrap_or(0);
                        let mut descending = def.sort.first().is_some_and(|s| s.descending);
                        let changed = c.combo(
                            "wizard_sort",
                            394.,
                            320.,
                            140.,
                            22.,
                            &items,
                            &mut pick,
                            true,
                        );
                        let flipped = c
                            .checkbox(
                                "wizard_descending",
                                542.,
                                322.,
                                90.,
                                17.,
                                "Descending",
                                &mut descending,
                                true,
                            )
                            .clicked();
                        if changed || flipped || def.sort.is_empty() {
                            if let Some(col) = def.columns.get(pick) {
                                def.sort = vec![SortKey {
                                    column: col.clone(),
                                    descending,
                                }];
                            }
                        }
                        if !activities {
                            kit::text(c, 16., 352., "Filters apply to activities; use Containing to narrow this subject area.", true);
                        }
                    }
                    2 => {
                        let def = &mut w.def;
                        kit::text(c, 16., 60., "Add a title for your report.", true);
                        for (n, (label, text)) in [
                            ("Report Title", &mut def.title),
                            ("Header", &mut def.header),
                            ("Footer", &mut def.footer),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            let y = 92. + 34. * n as f32;
                            c.label(20., y + 2., 17., label, true);
                            c.edit(
                                &format!("wizard_text_{n}"),
                                120.,
                                y,
                                480.,
                                22.,
                                text,
                                true,
                                Align::Min,
                            );
                        }
                        c.checkbox(
                            "wizard_totals",
                            120.,
                            200.,
                            200.,
                            17.,
                            "Show Grand Totals",
                            &mut def.show_totals,
                            true,
                        );
                        kit::text(
                            c,
                            120.,
                            222.,
                            "Grand totals add up units and costs and show the date range.",
                            true,
                        );
                    }
                    _ => {
                        let def = &mut w.def;
                        kit::text(c, 16., 60., "Your report has been created. Click Run Report to preview or write it.", true);
                        kit::section(c, 16., 84., 608., 112., "Send Report To");
                        let mut outputs: Vec<(Option<Output>, &str)> = vec![(None, "Screen")];
                        outputs.extend(Output::ALL.iter().map(|o| (Some(*o), o.caption())));
                        for (n, (o, label)) in outputs.iter().enumerate() {
                            if c.radio(
                                &format!("wizard_output_{n}"),
                                28. + 116. * n as f32,
                                112.,
                                110.,
                                17.,
                                label,
                                def.output == *o,
                                true,
                            ) && def.output != *o
                            {
                                def.output = *o;
                                if let Some(o) = o {
                                    let base = if def.path.trim().is_empty() {
                                        doc::documents().join("Report").display().to_string()
                                    } else {
                                        def.path.clone()
                                    };
                                    def.path = PathBuf::from(base)
                                        .with_extension(o.extension())
                                        .display()
                                        .to_string();
                                }
                            }
                        }
                        c.label(28., 144., 17., "Output File", true);
                        c.edit(
                            "wizard_path",
                            116.,
                            142.,
                            470.,
                            22.,
                            &mut def.path,
                            def.output.is_some(),
                            Align::Min,
                        );
                        let frame = c.r(116., 142., 491., 22.);
                        if c.ellipsis("wizard_path_browse", frame, def.output.is_some())
                            .clicked()
                        {
                            k.browse = true;
                        }
                        k.run = kit::button(
                            c,
                            "wizard_run",
                            28.,
                            210.,
                            kit::BUTTON.x,
                            kit::BUTTON.y,
                            "Run Report...",
                            true,
                        );
                        kit::section(c, 16., 250., 608., 96., "Save Report");
                        kit::text(
                            c,
                            28.,
                            278.,
                            "If you save your report, you can rerun it later from Tools, Reports.",
                            true,
                        );
                        c.label(28., 304., 17., "Report Name", true);
                        c.edit(
                            "wizard_name",
                            116.,
                            302.,
                            360.,
                            22.,
                            &mut w.save_name,
                            true,
                            Align::Min,
                        );
                        k.save = kit::button(
                            c,
                            "wizard_save",
                            486.,
                            300.,
                            kit::BUTTON.x,
                            kit::BUTTON.y,
                            "Save Report",
                            true,
                        );
                    }
                }
                if let Some(e) = &w.error {
                    kit::error_text(c, 16., H - 65., W - 32., e);
                } else if let Some(note) = &w.note {
                    kit::text(c, 16., H - 65., note, true);
                }
                c.fill(0., H - 38., W, 38., kit::STRIP);
                // Cancel alone at the left, the page buttons at the right.
                let left = kit::button_row(
                    c,
                    8. + kit::BUTTON.x,
                    H - 32.,
                    &[kit::Entry::cancel("wizard_cancel")],
                );
                let right = kit::button_row(
                    c,
                    W - 8.,
                    H - 32.,
                    &[
                        kit::Entry::Button("wizard_prev", "Prev", w.page > 0, false),
                        kit::Entry::Button("wizard_next", "Next", w.page + 1 < PAGES.len(), false),
                        kit::Entry::Button("wizard_finish", "Finish", w.page > 0, false),
                    ],
                );
                match left.clicked.or(right.clicked).as_deref() {
                    Some("wizard_cancel") => k.cancel = true,
                    Some("wizard_prev") => k.prev = true,
                    Some("wizard_next") => k.next = true,
                    Some("wizard_finish") => k.finish = true,
                    _ => {}
                }
                k
            },
        );
        self.record_dialog_actions(actions);
        if k.browse {
            let ext = w.def.output.map_or("html", |o| o.extension());
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Report", &[ext])
                .set_file_name(format!("Report.{ext}"))
                .save_file()
            {
                w.def.path = path.display().to_string();
            }
        }
        if k.cancel || closed {
            return;
        }
        let page = w.page;
        self.reports.wizard = Some(w);
        if k.next {
            self.wizard_go(page + 1);
        } else if k.prev {
            self.wizard_go(page.saturating_sub(1));
        }
        if k.save {
            if let Err(e) = self.save_wizard_report() {
                if let Some(w) = self.reports.wizard.as_mut() {
                    w.error = Some(e);
                }
            }
        }
        if k.run || k.finish {
            let def = self.reports.wizard.as_ref().map(|w| w.def.clone());
            if let Some(mut def) = def {
                let saved_name = self
                    .reports
                    .wizard
                    .as_ref()
                    .map(|w| w.save_name.trim().to_string())
                    .unwrap_or_default();
                let named = !saved_name.is_empty();
                if named {
                    // The report tab carries the name the report is saved under.
                    def.name = saved_name;
                }
                let result = if k.finish && named {
                    self.save_wizard_report()
                        .and_then(|_| self.run_report(&def))
                } else {
                    self.run_report(&def)
                };
                match result {
                    Ok(()) if k.finish => self.reports.wizard = None,
                    Ok(()) => {
                        if let Some(w) = self.reports.wizard.as_mut() {
                            w.error = None;
                            w.note = Some(match def.output {
                                None => "Report opened in a tab.".into(),
                                Some(_) => doc::saved_note(std::path::Path::new(def.path.trim())),
                            });
                        }
                    }
                    Err(e) => {
                        if let Some(w) = self.reports.wizard.as_mut() {
                            w.error = Some(e);
                        }
                    }
                }
            }
        }
    }
}
