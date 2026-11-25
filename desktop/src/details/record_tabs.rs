//! Tabs backed by XER tables outside the scheduling model: Steps, Notebook, Codes,
//! Expenses, Documents, Notes to Resources, Risks and User Defined Fields.
use super::assign_kit::{AssignButton, AssignWindow};
use super::*;
use openxer_xer::edit::notebooks;
use openxer_xer::Document;

fn table_value(doc: &Document, table: &str, index: usize, column: &str) -> String {
    records::get(doc, table, index, column)
}
use openxer_xer::edit::steps::{self as shared_steps, renumber_steps, sorted_steps, StepEdit};
/// Rows of a dictionary table that belong to this project (or every row when not project scoped).
fn project_rows(doc: &Document, table: &str, pid: i64) -> Vec<usize> {
    let Some(t) = doc.table(table) else {
        return vec![];
    };
    let scoped = t.field("proj_id").is_some();
    (0..t.rows.len())
        .filter(|&r| {
            let p = t.value(&t.rows[r], "proj_id");
            !scoped || p.is_empty() || num(p) == pid
        })
        .collect()
}
use super::grid_columns::{self as gc, GridColumns};
use crate::chrome::Item;
use crate::context_menu::Menu;
pub use openxer_xer::edit::project_records::ACCRUAL;
/// Hint over Assign when every work product and document is assigned.
const NO_DOCUMENTS: &str = "No unassigned work products or documents in this file.";
/// Hint over Add from Template when no template is defined.
pub(crate) const NO_TEMPLATES: &str =
    "Define step templates in Enterprise > Activity Step Templates first.";
/// The grid's Customize ... Columns menu item; sets `open` when clicked.
fn grid_menu_customize(
    ui: &mut egui::Ui,
    grid: &'static GridColumns,
    open: &mut Option<&'static GridColumns>,
) {
    if gc::menu_item(ui, grid) {
        *open = Some(grid);
    }
}
/// Text cell editor for the selected row; returns the cleaned text when edited.
fn text_cell(ui: &mut egui::Ui, value: &str) -> Option<String> {
    editor_cell(ui);
    let mut text = value.to_string();
    let response = ui.add(
        egui::TextEdit::singleline(&mut text)
            .frame(false)
            .desired_width(f32::INFINITY),
    );
    (response.changed()).then(|| clean(&text))
}
fn number_cell(
    ui: &mut egui::Ui,
    value: f64,
    suffix: &str,
    range: std::ops::RangeInclusive<f64>,
) -> Option<f64> {
    editor_cell(ui);
    let mut v = value;
    let response = ui.add(
        egui::DragValue::new(&mut v)
            .range(range)
            .suffix(suffix)
            .speed(1.),
    );
    (response.changed() && v != value).then_some(v)
}

type PickerRows = Vec<(i64, Vec<String>)>;

/// A step's weight as a percent of all the activity's step weights, with
/// one decimal (`75.0`); `0.0` when no step carries weight.
pub(crate) fn weight_percent(weight: f64, total: f64) -> String {
    if total <= 0. {
        return "0.0".into();
    }
    format!("{:.1}", weight / total * 100.)
}

impl App {
    pub(super) fn steps_tab(&mut self, ui: &mut egui::Ui, i: usize) {
        let (task_id, pid) = self.ids(i);
        let doc = &self.project.document;
        let order = sorted_steps(doc, task_id, pid);
        struct Step {
            index: usize,
            id: i64,
            name: String,
            complete: bool,
            percent: f64,
            weight: f64,
            description: String,
        }
        let steps: Vec<Step> = order
            .iter()
            .map(|&r| {
                let v = |k| table_value(doc, "TASKPROC", r, k);
                Step {
                    index: r,
                    id: num(&v("proc_id")),
                    name: v("proc_name"),
                    complete: v("complete_flag") == "Y",
                    percent: float(&v("complete_pct")),
                    weight: float(&v("proc_wt")),
                    description: v("proc_descr"),
                }
            })
            .collect();
        let total_weight: f64 = steps.iter().map(|s| s.weight).sum();
        let selected = self.detail.rows[0].filter(|&r| r < steps.len());
        let content = ui.max_rect();
        let at = self.detail_split(ui, "steps", content, 426., [240., 260.]);
        let (left, right) = split(ui, content, at);
        #[derive(Clone)]
        enum Edit {
            Name(usize, String),
            Complete(usize, bool),
            Percent(usize, f64),
            Weight(usize, f64),
            Description(usize, String),
        }
        let mut edits = vec![];
        let mut customize: Option<&'static GridColumns> = None;
        let mut action: Option<&str> = None;
        let has_clip = self.detail.step_clipboard.is_some();
        {
            let ui = &mut area(ui, left);
            let height = ui.available_height() - form::BUTTON_BAR;
            // The reference's default columns; Completed and Step Weight
            // follow when all columns are on.
            let cols = self.detail_columns(&gc::STEPS, self.detail.all_columns.contains("steps"));
            let mut all_columns = self.detail.all_columns.contains("steps");
            let mut cell_rects: Vec<(String, [f32; 4])> = vec![];
            let out = grid_columns(
                ui,
                "steps_grid",
                &cols,
                steps.len(),
                selected,
                Vec2::new(left.width(), height),
                &mut |ui, ev, sel| match ev {
                    Ev::Cell(r, c) => {
                        let s = &steps[r];
                        // Every cell by row and data column, for the scenarios.
                        let m = ui.max_rect();
                        let key = format!("step_cell_{r}_{c}");
                        cell_rects.push((key, [m.left(), m.top(), m.right(), m.bottom()]));
                        match c {
                            0 if sel => {
                                if let Some(text) = text_cell(ui, &s.name) {
                                    edits.push(Edit::Name(s.index, text));
                                }
                            }
                            0 => cell(ui, &s.name),
                            1 => {
                                let mut done = s.complete;
                                if ui.checkbox(&mut done, "").changed() {
                                    edits.push(Edit::Complete(s.index, done));
                                }
                            }
                            2 if sel => {
                                if let Some(v) = number_cell(ui, s.percent, "%", 0.0..=100.) {
                                    edits.push(Edit::Percent(s.index, v));
                                }
                            }
                            2 => cell(ui, format!("{}%", format_number(s.percent))),
                            3 if sel => {
                                if let Some(v) = number_cell(ui, s.weight, "", 0.0..=1.0e9) {
                                    edits.push(Edit::Weight(s.index, v));
                                }
                            }
                            // Step Weight Percent: the step's share of the
                            // activity's step weights, read only.
                            4 => cell(ui, weight_percent(s.weight, total_weight)),
                            _ => cell(ui, format_number(s.weight)),
                        }
                    }
                    Ev::Menu(row) => {
                        grid_menu_customize(ui, &gc::STEPS, &mut customize);
                        let mut menu = Menu::on(ui, "steps_grid_menu");
                        if menu.check("show_all", "Show All Step Columns", all_columns) {
                            all_columns = !all_columns;
                        }
                        menu.separator();
                        if menu.item_if("copy", Item::new("Copy"), row.is_some()) {
                            action = Some("copy");
                        }
                        if menu.item_if("paste", Item::new("Paste"), has_clip) {
                            action = Some("paste");
                        }
                        let template = Item::new("Create Template...");
                        if menu.item_if("template", template, !steps.is_empty()) {
                            action = Some("template");
                        }
                        if menu.item("from_template", Item::new("Add from Template")) {
                            action = Some("from_template");
                        }
                    }
                },
            );
            if let Some(r) = out.clicked {
                self.detail.rows[0] = Some(r);
            }
            self.action_rects.extend(cell_rects);
            if let Some(g) = customize {
                self.open_column_customize(g);
            }
            if all_columns {
                self.detail.all_columns.insert("steps".into());
            } else {
                self.detail.all_columns.remove("steps");
            }
            form::button_row(ui, |ui| {
                let add = form::add_button(ui, "Add", true);
                self.action("detail_steps_add", &add);
                if add.clicked() {
                    action = Some("add");
                }
                // With no step templates the template list opens empty.
                let from = form::add_button(ui, "Add from Template", true);
                self.action("detail_steps_from_template", &from);
                if from.clicked() {
                    action = Some("from_template");
                }
                let delete = form::command_button(ui, "Delete", None, selected.is_some());
                self.action("detail_steps_delete", &delete);
                if delete.clicked() {
                    action = Some("delete");
                }
                if form::command_button(ui, "^", None, selected.is_some_and(|r| r > 0)).clicked() {
                    action = Some("up");
                }
                if form::command_button(
                    ui,
                    "V",
                    None,
                    selected.is_some_and(|r| r + 1 < steps.len()),
                )
                .clicked()
                {
                    action = Some("down");
                }
            });
        }
        match selected.map(|r| &steps[r]) {
            Some(s) => {
                if let Some(html) = self.memo_section(
                    ui,
                    right,
                    &s.name,
                    8.,
                    format!("step-{}", s.id),
                    &s.description,
                    true,
                    "detail_step_memo",
                ) {
                    edits.push(Edit::Description(s.index, html));
                }
            }
            None => {
                let _ = self.memo_section(
                    ui,
                    right,
                    &form::none_selected("Step Name"),
                    8.,
                    "step-none".into(),
                    "",
                    false,
                    "detail_step_memo",
                );
            }
        }
        if action == Some("from_template") {
            let first = crate::dictionaries::steps::templates(&self.project.document)
                .first()
                .map(|t| t.id);
            // The template's steps go in after the selected step.
            self.dictionaries.template_pick = Some(crate::dictionaries::steps::TemplatePick {
                task_id,
                selected: first,
                targets: vec![],
                after: selected.map(|r| steps[r].id),
            });
            action = None;
        }
        let changed = !edits.is_empty() || action.is_some_and(|a| a != "copy");
        let code = self.project.model.activities[i].task_code.clone();
        let doc = &mut self.project.document;
        for edit in edits {
            let (r, e) = match edit {
                Edit::Name(r, v) => (r, StepEdit::Name(v)),
                Edit::Complete(r, done) => (r, StepEdit::Complete(done)),
                Edit::Percent(r, v) => (r, StepEdit::Percent(v)),
                Edit::Weight(r, v) => (r, StepEdit::Weight(v)),
                Edit::Description(r, html) => (r, StepEdit::Description(html)),
            };
            shared_steps::edit_step(doc, r, e);
        }
        let new_step = |doc: &mut Document, copy: Option<&StepCopy>| {
            let fallback = StepCopy::default();
            shared_steps::new_step_row(doc, task_id, pid, copy.unwrap_or(&fallback))
        };
        let mut order = order;
        match action {
            Some("add") | Some("paste") => {
                let clip = if action == Some("paste") {
                    self.detail.step_clipboard.clone()
                } else {
                    None
                };
                let row = new_step(doc, clip.as_ref());
                let at = selected.map_or(order.len(), |s| s + 1);
                order.insert(at, row);
                renumber_steps(doc, &order);
                self.detail.rows[0] = Some(at);
            }
            Some("delete") => {
                if let Some(s) = selected {
                    let row = order.remove(s);
                    records::remove_row(doc, "TASKPROC", row);
                    let order = sorted_steps(doc, task_id, pid);
                    renumber_steps(doc, &order);
                    self.detail.rows[0] = None;
                }
            }
            Some(dir @ ("up" | "down")) => {
                if let Some(s) = selected {
                    let other = if dir == "up" { s - 1 } else { s + 1 };
                    order.swap(s, other);
                    renumber_steps(doc, &order);
                    self.detail.rows[0] = Some(other);
                }
            }
            Some("template") => {
                match crate::dictionaries::steps::template_from_activity(doc, &code, task_id, pid) {
                    Ok(_) => self.status = format!("Step template created from {code}"),
                    Err(e) => self.status = e,
                }
            }
            Some("copy") => {
                if let Some(s) = selected.map(|r| &steps[r]) {
                    self.detail.step_clipboard = Some(StepCopy {
                        name: s.name.clone(),
                        description: s.description.clone(),
                        weight: format_number(s.weight),
                        percent: format_number(s.percent),
                        complete: if s.complete { "Y" } else { "N" }.into(),
                    });
                    self.status = "Step copied.".into();
                }
            }
            _ => {}
        }
        if changed {
            shared_steps::after_steps_change(&mut self.project, task_id);
            self.touch_records();
        }
    }

    fn notebook_topics(&self) -> Vec<(i64, String)> {
        let doc = &self.project.document;
        let Some(t) = doc.table("MEMOTYPE") else {
            return vec![];
        };
        let mut topics: Vec<(i64, i64, String)> = t
            .rows
            .iter()
            .filter(|r| t.value(r, "task_flag") != "N")
            .map(|r| {
                (
                    num(t.value(r, "seq_num")),
                    num(t.value(r, "memo_type_id")),
                    t.value(r, "memo_type").to_string(),
                )
            })
            .collect();
        topics.sort();
        topics.into_iter().map(|(_, id, name)| (id, name)).collect()
    }

    pub(super) fn notebook_tab(&mut self, ui: &mut egui::Ui, i: usize) {
        let (task_id, pid) = self.ids(i);
        let owner = notebooks::Owner::Activity(task_id);
        let content = ui.max_rect();
        let selected = self.detail.rows[0];
        let out = self.notebook_pane(
            ui,
            content,
            "detail_notebook",
            "notebook",
            Some(owner),
            selected,
            true,
        );
        if let Some(r) = out.select {
            self.detail.rows[0] = Some(r);
        }
        if out.cleared {
            self.detail.rows[0] = None;
        }
        if out.add {
            if notebooks::unused_topics(&self.project.document, pid, owner).is_empty() {
                self.status = if self.notebook_topics().is_empty() {
                    "No activity notebook topics are defined in this file.".into()
                } else {
                    "Every activity notebook topic is already assigned.".into()
                };
            } else {
                self.detail.picker = Some(Picker::Topic { selected: None });
            }
        }
    }

    /// Activity code types visible to this project: global, EPS and this project's own.
    pub(crate) fn code_types(&self, pid: i64) -> Vec<(i64, String)> {
        let doc = &self.project.document;
        let Some(t) = doc.table("ACTVTYPE") else {
            return vec![];
        };
        let mut types: Vec<(i64, i64, String)> = t
            .rows
            .iter()
            .filter(|r| {
                let scope = t.value(r, "actv_code_type_scope");
                let p = t.value(r, "proj_id");
                match scope {
                    "AS_Global" | "AS_EPS" => true,
                    "AS_Project" => num(p) == pid,
                    _ => p.is_empty() || num(p) == pid,
                }
            })
            .map(|r| {
                (
                    num(t.value(r, "seq_num")),
                    num(t.value(r, "actv_code_type_id")),
                    t.value(r, "actv_code_type").to_string(),
                )
            })
            .collect();
        types.sort();
        types.into_iter().map(|(_, id, name)| (id, name)).collect()
    }

    pub(super) fn codes_tab(&mut self, ui: &mut egui::Ui, i: usize) {
        let (task_id, pid) = self.ids(i);
        let doc = &self.project.document;
        let codes: Vec<(usize, [String; 3])> = records::rows_for(doc, "TASKACTV", task_id, pid)
            .into_iter()
            .map(|r| {
                let kind = num(&table_value(doc, "TASKACTV", r, "actv_code_type_id"));
                let code = num(&table_value(doc, "TASKACTV", r, "actv_code_id"));
                (
                    r,
                    [
                        records::lookup(
                            doc,
                            "ACTVTYPE",
                            "actv_code_type_id",
                            kind,
                            "actv_code_type",
                        ),
                        records::lookup(doc, "ACTVCODE", "actv_code_id", code, "short_name"),
                        records::lookup(doc, "ACTVCODE", "actv_code_id", code, "actv_code_name"),
                    ],
                )
            })
            .collect();
        let selected = self.detail.rows[0].filter(|&r| r < codes.len());
        let size = Vec2::new(
            ui.available_width(),
            ui.available_height() - form::BUTTON_BAR,
        );
        let mut customize = None;
        let out = grid_columns(
            ui,
            "codes_grid",
            &self.detail_columns(&gc::CODES, false),
            codes.len(),
            selected,
            size,
            &mut |ui, ev, _| match ev {
                Ev::Cell(r, c) => cell(ui, &codes[r].1[c]),
                Ev::Menu(_) => grid_menu_customize(ui, &gc::CODES, &mut customize),
            },
        );
        if let Some(r) = out.clicked {
            self.detail.rows[0] = Some(r);
        }
        if let Some(g) = customize {
            self.open_column_customize(g);
        }
        let mut remove = false;
        form::button_row(ui, |ui| {
            let assign = form::add_button(ui, "Assign", true);
            self.action("detail_codes_assign", &assign);
            if assign.clicked() {
                self.assign_activity_codes();
            }
            let button = form::command_button(ui, "Remove", None, selected.is_some());
            self.action("detail_codes_remove", &button);
            remove = button.clicked();
        });
        // Remove asks first (`details::removals`).
        if remove {
            if let Some(r) = selected {
                let row = codes[r].0;
                let doc = &self.project.document;
                let code_id = num(&table_value(doc, "TASKACTV", row, "actv_code_id"));
                self.detail.removal = Some(super::removals::Removal::Code {
                    row,
                    task_id,
                    code_id,
                });
            }
        }
    }

    /// True when some activity code type of project `pid` has a value.
    pub(crate) fn has_code_values(&self, pid: i64) -> bool {
        let types = self.code_types(pid);
        self.project.document.table("ACTVCODE").is_some_and(|t| {
            t.rows.iter().any(|r| {
                let kind = num(t.value(r, "actv_code_type_id"));
                types.iter().any(|(id, _)| *id == kind)
            })
        })
    }

    /// Assign > Activity Codes: the code picker, or, while the project has
    /// no code values to pick, the Activity Codes dictionary to add some.
    /// Closing it with values to assign opens the picker.
    pub(crate) fn assign_activity_codes(&mut self) {
        let pid = self.project.model.project.proj_id;
        if self.has_code_values(pid) {
            self.detail.picker = Some(Picker::Code {
                type_id: None,
                selected: None,
            });
        } else {
            self.open_codes_for_assign(None);
        }
    }

    /// The Activity Codes dictionary (on `type_id` when given), returning to
    /// the code picker when it closes.
    pub(crate) fn open_codes_for_assign(&mut self, type_id: Option<i64>) {
        use crate::dictionaries::{ops, Dictionary};
        let kind = ops::CodeKind::Activity;
        self.detail.picker = None;
        self.open_dictionary(Dictionary::Codes(kind));
        let pid = self.project.model.project.proj_id;
        let doc = &self.project.document;
        if let Some(cs) = self.dictionaries.codes.as_mut() {
            let found = type_id.and_then(|id| {
                [ops::Scope::Project, ops::Scope::Global, ops::Scope::Eps]
                    .into_iter()
                    .find(|s| {
                        ops::code_types(doc, kind, *s, pid)
                            .iter()
                            .any(|t| t.id == id)
                    })
                    .map(|s| (s, id))
            });
            if let Some((scope, id)) = found {
                cs.scope = scope;
                cs.type_id = Some(id);
            } else {
                // Codes made from an activity start in its project; the
                // scope switch still offers Global and EPS.
                cs.scope = ops::Scope::Project;
                cs.type_id = ops::code_types(doc, kind, ops::Scope::Project, pid)
                    .first()
                    .map(|t| t.id);
            }
            cs.hint =
                Some("Add a code type (Modify...) and its values, then Close to assign.".into());
        }
        // The hint stands in for the empty-list notice.
        self.dictionaries.notice = None;
        self.detail.assign_codes_after = true;
    }

    /// Run after the dictionary dialogs: once the Activity Codes dictionary
    /// opened from Assign has closed, the picker opens if there is a value.
    pub(crate) fn after_codes_dictionary(&mut self) {
        if !self.detail.assign_codes_after || self.dictionary_open() {
            return;
        }
        self.detail.assign_codes_after = false;
        let pid = self.project.model.project.proj_id;
        let selected = self
            .selected
            .is_some_and(|i| i < self.project.model.activities.len());
        if selected && self.has_code_values(pid) {
            self.detail.picker = Some(Picker::Code {
                type_id: None,
                selected: None,
            });
        }
    }

    /// Assign a code value, replacing any value this activity has for the same code type.
    pub(crate) fn assign_code(&mut self, i: usize, type_id: i64, code_id: i64) {
        let task_id = self.project.model.activities[i].task_id;
        openxer_xer::edit::activity::assign_code(&mut self.project, task_id, type_id, code_id);
        self.touch_records();
    }

    pub(super) fn expenses_tab(&mut self, ui: &mut egui::Ui, i: usize) {
        let (task_id, pid) = self.ids(i);
        let doc = &self.project.document;
        let rows = records::rows_for(doc, "PROJCOST", task_id, pid);
        let categories: Vec<(i64, String)> = doc
            .table("COSTTYPE")
            .map(|t| {
                t.rows
                    .iter()
                    .map(|r| {
                        (
                            num(t.value(r, "cost_type_id")),
                            t.value(r, "cost_type").to_string(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        struct Expense {
            index: usize,
            text: [String; 5],
            nums: [f64; 5],
            category: i64,
            accrual: String,
            account: i64,
            auto_compute: bool,
        }
        let accounts: Vec<(i64, String)> = doc
            .table("ACCOUNT")
            .map(|t| {
                t.rows
                    .iter()
                    .map(|r| {
                        let short = t.value(r, "acct_short_name");
                        let name = if short.is_empty() {
                            t.value(r, "acct_name")
                        } else {
                            short
                        };
                        (num(t.value(r, "acct_id")), name.to_string())
                    })
                    .collect()
            })
            .unwrap_or_default();
        let expenses: Vec<Expense> = rows
            .iter()
            .map(|&r| {
                let v = |k| table_value(doc, "PROJCOST", r, k);
                Expense {
                    index: r,
                    text: [
                        v("cost_name"),
                        String::new(),
                        v("qty_name"),
                        v("vendor_name"),
                        String::new(),
                    ],
                    nums: [
                        float(&v("target_cost")),
                        float(&v("act_cost")),
                        float(&v("remain_cost")),
                        float(&v("target_qty")),
                        float(&v("cost_per_qty")),
                    ],
                    category: num(&v("cost_type_id")),
                    accrual: v("cost_load_type"),
                    account: num(&v("acct_id")),
                    auto_compute: v("auto_compute_act_flag") == "Y",
                }
            })
            .collect();
        let selected = self.detail.rows[0].filter(|&r| r < expenses.len());
        let cols = [
            ("Expense Item", 150.),
            ("Expense Category", 105.),
            ("Budgeted Cost", 85.),
            ("Actual Cost", 80.),
            ("Remaining Cost", 85.),
            ("At Completion Cost", 100.),
            ("Budgeted Units", 80.),
            ("Price / Unit", 75.),
            ("Unit of Measure", 85.),
            ("Vendor", 100.),
            ("Accrual Type", 100.),
        ];
        let mut sets: Vec<(usize, &str, String)> = vec![];
        let size = Vec2::new(
            ui.available_width(),
            ui.available_height() - form::BUTTON_BAR,
        );
        // The default column set, as the reference shows it.
        let shown = self.detail_columns(&gc::EXPENSES, false);
        let _ = cols;
        let mut customize = None;
        let out = grid_columns(
            ui,
            "expenses_grid",
            &shown,
            expenses.len(),
            selected,
            size,
            &mut |ui, ev, sel| {
                let Ev::Cell(r, c) = ev else {
                    if let Ev::Menu(_) = ev {
                        grid_menu_customize(ui, &gc::EXPENSES, &mut customize);
                    }
                    return;
                };
                let e = &expenses[r];
                let category = categories
                    .iter()
                    .find(|(id, _)| *id == e.category)
                    .map_or(String::new(), |(_, n)| n.clone());
                let accrual = ACCRUAL
                    .iter()
                    .find(|(code, _)| *code == e.accrual)
                    .map_or("", |(_, l)| l);
                match (c, sel) {
                    (0, true) => {
                        if let Some(t) = text_cell(ui, &e.text[0]) {
                            sets.push((e.index, "cost_name", t));
                        }
                    }
                    (0, false) => cell(ui, &e.text[0]),
                    (1, true) => {
                        editor_cell(ui);
                        egui::ComboBox::from_id_salt(("expense_category", e.index))
                            .selected_text(category)
                            .width(95.)
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(e.category == 0, "").clicked() {
                                    sets.push((e.index, "cost_type_id", String::new()));
                                }
                                for (id, name) in &categories {
                                    if ui.selectable_label(e.category == *id, name).clicked() {
                                        sets.push((e.index, "cost_type_id", id.to_string()));
                                    }
                                }
                            });
                    }
                    (1, false) => cell(ui, category),
                    (2..=4, true) => {
                        let k = c - 2;
                        if let Some(v) = number_cell(ui, e.nums[k], "", -1.0e12..=1.0e12) {
                            let column = ["target_cost", "act_cost", "remain_cost"][k];
                            sets.push((e.index, column, format_number(v)));
                            if k == 0 {
                                if e.nums[3] > 0. {
                                    sets.push((
                                        e.index,
                                        "cost_per_qty",
                                        format_number(v / e.nums[3]),
                                    ));
                                }
                                if e.nums[1] == 0. {
                                    sets.push((e.index, "remain_cost", format_number(v)));
                                }
                            }
                        }
                    }
                    (2..=4, false) => cell(ui, money(e.nums[c - 2])),
                    (5, _) => cell(ui, money(e.nums[1] + e.nums[2])),
                    (6 | 7, true) => {
                        let k = c - 3;
                        if let Some(v) = number_cell(ui, e.nums[k], "", 0.0..=1.0e12) {
                            let (qty, price) = if k == 3 {
                                (v, e.nums[4])
                            } else {
                                (e.nums[3], v)
                            };
                            sets.push((
                                e.index,
                                if k == 3 { "target_qty" } else { "cost_per_qty" },
                                format_number(v),
                            ));
                            let cost = qty * price;
                            sets.push((e.index, "target_cost", format_number(cost)));
                            if e.nums[1] == 0. {
                                sets.push((e.index, "remain_cost", format_number(cost)));
                            }
                        }
                    }
                    (6, false) => cell(ui, format_number(e.nums[3])),
                    (7, false) => cell(ui, money(e.nums[4])),
                    (8 | 9, true) => {
                        if let Some(t) = text_cell(ui, &e.text[c - 6]) {
                            sets.push((
                                e.index,
                                if c == 8 { "qty_name" } else { "vendor_name" },
                                t,
                            ));
                        }
                    }
                    (8 | 9, false) => cell(ui, &e.text[c - 6]),
                    (11, true) => {
                        editor_cell(ui);
                        let current = accounts
                            .iter()
                            .find(|(id, _)| *id == e.account)
                            .map_or(String::new(), |(_, n)| n.clone());
                        egui::ComboBox::from_id_salt(("expense_account", e.index))
                            .selected_text(current)
                            .width(95.)
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(e.account == 0, "").clicked() {
                                    sets.push((e.index, "acct_id", String::new()));
                                }
                                for (id, name) in &accounts {
                                    if ui.selectable_label(e.account == *id, name).clicked() {
                                        sets.push((e.index, "acct_id", id.to_string()));
                                    }
                                }
                            });
                    }
                    (11, false) => cell(
                        ui,
                        accounts
                            .iter()
                            .find(|(id, _)| *id == e.account)
                            .map_or("", |(_, n)| n.as_str()),
                    ),
                    (12, _) => {
                        let mut on = e.auto_compute;
                        if ui.add(egui::Checkbox::without_text(&mut on)).changed() {
                            sets.push((
                                e.index,
                                "auto_compute_act_flag",
                                if on { "Y" } else { "N" }.into(),
                            ));
                        }
                    }
                    (_, true) => {
                        editor_cell(ui);
                        egui::ComboBox::from_id_salt(("expense_accrual", e.index))
                            .selected_text(accrual)
                            .width(95.)
                            .show_ui(ui, |ui| {
                                for (code, label) in ACCRUAL {
                                    if ui.selectable_label(e.accrual == code, label).clicked() {
                                        sets.push((e.index, "cost_load_type", code.into()));
                                    }
                                }
                            });
                    }
                    _ => cell(ui, accrual),
                }
            },
        );
        if let Some(r) = out.clicked {
            self.detail.rows[0] = Some(r);
        }
        if let Some(g) = customize {
            self.open_column_customize(g);
        }
        let mut action = None;
        form::button_row(ui, |ui| {
            let add = form::add_button(ui, "Add", true);
            self.action("detail_expenses_add", &add);
            if add.clicked() {
                action = Some("add");
            }
            if form::command_button(ui, "Delete", None, selected.is_some()).clicked() {
                action = Some("delete");
            }
        });
        let doc = &mut self.project.document;
        let changed = !sets.is_empty() || action.is_some();
        for (r, column, value) in sets {
            records::set(doc, "PROJCOST", r, column, value);
        }
        match action {
            Some("add") => {
                let id = records::next_id(doc, "PROJCOST", "cost_item_id");
                records::add_row(
                    doc,
                    "PROJCOST",
                    &[
                        ("cost_item_id", id.to_string()),
                        ("proj_id", pid.to_string()),
                        ("task_id", task_id.to_string()),
                        ("cost_name", "(New Expense Item)".into()),
                        ("act_cost", "0".into()),
                        ("cost_per_qty", "0".into()),
                        ("remain_cost", "0".into()),
                        ("target_cost", "0".into()),
                        ("cost_load_type", "CL_Uniform".into()),
                        ("auto_compute_act_flag", "N".into()),
                        ("target_qty", "0".into()),
                    ],
                );
                self.detail.rows[0] = Some(expenses.len());
            }
            Some("delete") => {
                if let Some(s) = selected {
                    records::remove_row(doc, "PROJCOST", expenses[s].index);
                    self.detail.rows[0] = None;
                }
            }
            _ => {}
        }
        if changed {
            self.touch_records();
        }
    }

    pub(super) fn documents_tab(&mut self, ui: &mut egui::Ui, i: usize) {
        let (task_id, pid) = self.ids(i);
        let doc = &self.project.document;
        let rows: Vec<(usize, i64, bool, [String; 4])> =
            records::rows_for(doc, "TASKDOC", task_id, pid)
                .into_iter()
                .map(|r| {
                    let id = num(&table_value(doc, "TASKDOC", r, "doc_id"));
                    let status = num(&records::lookup(
                        doc,
                        "DOCUMENT",
                        "doc_id",
                        id,
                        "doc_status_id",
                    ));
                    (
                        r,
                        id,
                        table_value(doc, "TASKDOC", r, "wp_flag") == "Y",
                        [
                            records::lookup(doc, "DOCUMENT", "doc_id", id, "doc_name"),
                            records::lookup(doc, "DOCUMENT", "doc_id", id, "doc_short_name"),
                            records::lookup(
                                doc,
                                "DOCSTAT",
                                "doc_status_id",
                                status,
                                "doc_status_code",
                            ),
                            if records::lookup(doc, "DOCUMENT", "doc_id", id, "deliv_flag") == "Y" {
                                "Yes".into()
                            } else {
                                "No".into()
                            },
                        ],
                    )
                })
                .collect();
        let selected = self.detail.rows[0].filter(|&r| r < rows.len());
        let mut flag = None;
        let size = Vec2::new(
            ui.available_width(),
            ui.available_height() - form::BUTTON_BAR,
        );
        let mut customize = None;
        // Assign, Remove and Details... from the menu: what the buttons
        // under the grid do, on the row that was right-clicked.
        let mut menu_action: Option<(u8, Option<usize>)> = None;
        let out = grid_columns(
            ui,
            "documents_grid",
            &self.detail_columns(&gc::DOCUMENTS, false),
            rows.len(),
            selected,
            size,
            &mut |ui, ev, _| match ev {
                Ev::Cell(r, 3) => {
                    let mut wp = rows[r].2;
                    if ui.checkbox(&mut wp, "").changed() {
                        flag = Some((rows[r].0, wp));
                    }
                }
                Ev::Cell(r, c) => cell(ui, &rows[r].3[if c > 3 { 3 } else { c }]),
                Ev::Menu(row) => {
                    grid_menu_customize(ui, &gc::DOCUMENTS, &mut customize);
                    let mut menu = Menu::on(ui, "documents_grid_menu");
                    menu.separator();
                    if menu.item("assign", Item::new("Assign")) {
                        menu_action = Some((0, row));
                    }
                    if menu.item_if("remove", Item::new("Remove"), row.is_some()) {
                        menu_action = Some((1, row));
                    }
                    if menu.item_if("details", Item::new("Details..."), row.is_some()) {
                        menu_action = Some((2, row));
                    }
                }
            },
        );
        if let Some(r) = out.clicked {
            self.detail.rows[0] = Some(r);
        }
        if let Some(g) = customize {
            self.open_column_customize(g);
        }
        if let Some(r) = out.double {
            self.detail.document_details = Some(rows[r].1);
        }
        let mut remove_at = None;
        form::button_row(ui, |ui| {
            ui.add_space(0.);
            // With no unassigned documents the picker opens empty.
            let assign = form::add_button(ui, "Assign", true);
            self.action("detail_documents_assign", &assign);
            if assign.clicked() {
                menu_action = Some((0, None));
            }
            if form::command_button(ui, "Remove", None, selected.is_some()).clicked() {
                menu_action = Some((1, selected));
            }
            if form::command_button(ui, "Details...", None, selected.is_some()).clicked() {
                menu_action = Some((2, selected));
            }
        });
        match menu_action {
            Some((0, _)) => self.detail.picker = Some(Picker::Document { selected: None }),
            Some((1, Some(r))) => remove_at = Some(r),
            Some((2, Some(r))) => self.detail.document_details = Some(rows[r].1),
            _ => {}
        }
        if let Some((r, wp)) = flag {
            openxer_xer::edit::documents::set_work_product(&mut self.project, r, wp);
            self.touch_records();
        }
        if let Some(r) = remove_at {
            openxer_xer::edit::documents::remove_link(&mut self.project, rows[r].0);
            self.detail.rows[0] = None;
            self.touch_records();
        }
    }
    /// Work Products... from a context menu: the assign picker for the
    /// selected activity, or a note when every document is assigned.
    pub(crate) fn document_picker(&mut self) -> Option<Picker> {
        let i = self
            .selected
            .filter(|&i| i < self.project.model.activities.len())?;
        let (task_id, pid) = self.ids(i);
        if self.unassigned_documents(task_id, pid).is_empty() {
            self.status = NO_DOCUMENTS.into();
            return None;
        }
        Some(Picker::Document { selected: None })
    }

    pub(crate) fn unassigned_documents(&self, task_id: i64, pid: i64) -> Vec<(i64, [String; 2])> {
        let doc = &self.project.document;
        let used: Vec<i64> = records::rows_for(doc, "TASKDOC", task_id, pid)
            .into_iter()
            .map(|r| num(&table_value(doc, "TASKDOC", r, "doc_id")))
            .collect();
        project_rows(doc, "DOCUMENT", pid)
            .into_iter()
            .map(|r| {
                (
                    num(&table_value(doc, "DOCUMENT", r, "doc_id")),
                    [
                        table_value(doc, "DOCUMENT", r, "doc_name"),
                        table_value(doc, "DOCUMENT", r, "doc_short_name"),
                    ],
                )
            })
            .filter(|(id, _)| !used.contains(id))
            .collect()
    }

    pub(super) fn document_details_dialog(&mut self, ctx: &egui::Context) {
        let Some(id) = self.detail.document_details else {
            return;
        };
        let doc = &self.project.document;
        let value = |k: &str| records::lookup(doc, "DOCUMENT", "doc_id", id, k);
        let status = records::lookup(
            doc,
            "DOCSTAT",
            "doc_status_id",
            num(&value("doc_status_id")),
            "doc_status_code",
        );
        let fields = [
            ("Title", value("doc_name")),
            ("Reference No.", value("doc_short_name")),
            ("Version", value("version_name")),
            ("Author", value("author_name")),
            ("Status", status),
            (
                "Deliverable",
                if value("deliv_flag") == "Y" {
                    "Yes".into()
                } else {
                    "No".into()
                },
            ),
            ("Revision Date", value("doc_date")),
            ("Private Location", value("private_loc")),
            ("Public Location", value("public_loc")),
            ("Description", records::memo_to_text(&value("doc_content"))),
        ];
        let mut open = true;
        let mut close = false;
        crate::chrome::plain_window("Work Products and Documents")
            .id(egui::Id::new("document_details"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("document_details_grid")
                    .num_columns(2)
                    .spacing([10., 4.])
                    .show(ui, |ui| {
                        for (label, value) in fields {
                            ui.label(label);
                            schedule_tabs::readonly(ui, &value, 260.);
                            ui.end_row();
                        }
                    });
                ui.add_space(6.);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let button = crate::chrome::dialog_button(
                        ui,
                        &crate::chrome::DialogButton::new("Close", None),
                    );
                    self.action("document_details_close", &button);
                    close = button.clicked();
                });
            });
        if !open || close || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.detail.document_details = None;
        }
    }

    pub(super) fn feedback_tab(&mut self, ui: &mut egui::Ui, i: usize) {
        let (task_id, pid) = self.ids(i);
        let doc = &self.project.document;
        let note_row = records::rows_for(doc, "TASKNOTE", task_id, pid)
            .first()
            .copied();
        let note = note_row
            .map(|r| table_value(doc, "TASKNOTE", r, "task_notes"))
            .unwrap_or_default();
        let feedback: String = records::rows_for(doc, "TASKFDBK", task_id, pid)
            .into_iter()
            .map(|r| {
                let rsrc = num(&table_value(doc, "TASKFDBK", r, "rsrc_id"));
                let name = self
                    .project
                    .resource_names
                    .get(&rsrc)
                    .cloned()
                    .unwrap_or_default();
                let text = records::memo_to_text(&table_value(doc, "TASKFDBK", r, "task_fdbk"));
                if name.is_empty() {
                    text
                } else {
                    format!("{name}: {text}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let mut new_flag = records::task_value(doc, task_id, pid, "rev_fdbk_flag") == "Y";
        let has_feedback = !feedback.trim().is_empty() || new_flag;
        let before = new_flag;
        // Two captioned panels split 763 : 1061 in the reference: each has
        // its memo, a small entry box and an Add button under it.
        let content = ui.max_rect();
        let default = (content.width() * 763. / 1824.).round();
        let at = self.detail_split(ui, "feedback", content, default, [240., 240.]);
        let (left, right) = split(ui, content, at);
        let p = ui.painter().clone();
        form::section(&p, left, "Notes to Resources", 21., 19.);
        let memo = Rect::from_min_max(
            Pos2::new(left.left() + 1., left.top() + 24.),
            Pos2::new(left.right() - 1., left.bottom() - 105.),
        );
        form::fill(&p, memo, Color32::WHITE);
        let mut new_note = {
            let ui = &mut area(ui, memo);
            self.memo_editor(ui, format!("note-{task_id}"), &note, memo.size(), true)
        };
        let entry = Rect::from_min_max(
            Pos2::new(left.left() + 6., left.bottom() - 98.),
            Pos2::new(left.right() - 6., left.bottom() - 38.),
        );
        form::lowered(&p, entry);
        let key = format!("note-draft-{task_id}");
        let mut draft = self
            .detail
            .memo
            .get(&key)
            .map(|d| d.1.clone())
            .unwrap_or_default();
        {
            let inner = entry.shrink(1.);
            form::fill(&p, inner, Color32::WHITE);
            let ui = &mut area(ui, inner);
            let response = ui.add_sized(
                inner.size(),
                egui::TextEdit::multiline(&mut draft)
                    .id(egui::Id::new(("note_draft", task_id)))
                    .frame(false)
                    .font(form::font()),
            );
            crate::dialog_keys::note_keeps_enter(&response);
        }
        let add = Rect::from_min_size(
            Pos2::new(left.left() + 44., left.bottom() - 31.),
            Vec2::new(313., 25.),
        );
        // Add stays enabled as every add command does (DESIGN.md); with
        // nothing typed it puts the cursor in the empty entry box.
        let add_note = {
            let ui = &mut area(ui, add);
            form::text_button(ui, "Add", 313., true)
        };
        self.action("detail_feedback_add", &add_note);
        if add_note.clicked() && draft.trim().is_empty() {
            let entry = egui::Id::new(("note_draft", task_id));
            ui.ctx().memory_mut(|m| m.request_focus(entry));
        } else if add_note.clicked() {
            let text = records::memo_to_text(&note);
            let joined = if text.trim().is_empty() {
                draft.trim().to_string()
            } else {
                format!("{text}\n{}", draft.trim())
            };
            new_note = Some(records::text_to_memo(&joined));
            draft.clear();
        }
        self.detail.memo.insert(key, (String::new(), draft));
        // Feedback from resources: read only.
        form::section(&p, right, "Feedback from Resources", 21., 19.);
        let flag_x = right.right() - 222.;
        let response = form::checkbox(
            ui,
            ui.id().with("feedback_new"),
            flag_x,
            right.top() + 1.,
            "New",
            &mut new_flag,
            has_feedback,
        );
        self.action("detail_feedback_new", &response);
        let memo = Rect::from_min_max(
            Pos2::new(right.left() + 1., right.top() + 24.),
            Pos2::new(right.right() - 1., right.bottom() - 103.),
        );
        form::fill(&p, memo, form::READONLY);
        {
            let ui = &mut area(ui, memo.shrink2(Vec2::new(3., 2.)));
            let mut text = feedback;
            ui.add(
                egui::TextEdit::multiline(&mut text)
                    .frame(false)
                    .interactive(false)
                    .font(form::font())
                    .text_color(INK)
                    .desired_width(memo.width() - 8.),
            );
        }
        let entry = Rect::from_min_max(
            Pos2::new(right.left() + 6., right.bottom() - 92.),
            Pos2::new(right.right() - 6., right.bottom() - 32.),
        );
        form::lowered(&p, entry);
        form::fill(&p, entry.shrink(1.), Color32::WHITE);
        {
            let ui = &mut area(
                ui,
                Rect::from_min_size(
                    Pos2::new(right.left() + 38., right.bottom() - 25.),
                    Vec2::new(307., 25.),
                ),
            );
            form::text_button(ui, "Add", 307., false)
                .on_disabled_hover_text("Feedback comes from the resources' timesheets.");
        }
        let doc = &mut self.project.document;
        if let Some(html) = new_note {
            openxer_xer::edit::project_records::set_notes_to_resources(doc, task_id, pid, html);
            self.touch_records();
        }
        if new_flag != before {
            openxer_xer::edit::project_records::set_review_required(
                &mut self.project.document,
                task_id,
                pid,
                new_flag,
            );
            self.touch_records();
        }
    }

    pub(super) fn risks_tab(&mut self, ui: &mut egui::Ui, i: usize) {
        let (task_id, pid) = self.ids(i);
        let doc = &self.project.document;
        let table = records::risk_table(doc);
        let categories =
            crate::views::record_view::dictionary(doc, "RISKTYPE", "risk_type_id", "risk_type");
        let mut risks: Vec<(usize, [String; 6])> = records::rows_for(doc, "TASKRISK", task_id, pid)
            .into_iter()
            .map(|r| {
                let id = num(&table_value(doc, "TASKRISK", r, "risk_id"));
                let v = |k: &str| records::lookup(doc, table, "risk_id", id, k);
                let owner = num(&v("rsrc_id"));
                (
                    r,
                    [
                        v("risk_code"),
                        v("risk_name"),
                        crate::views::record_view::name_of(&categories, &v("risk_type_id")),
                        crate::views::record_view::label_of(
                            &crate::views::risks::KIND,
                            &v("risk_to_type"),
                        ),
                        crate::views::record_view::label_of(
                            &crate::views::risks::STATUS,
                            &v("status_code"),
                        ),
                        self.project
                            .resource_names
                            .get(&owner)
                            .cloned()
                            .unwrap_or_default(),
                    ],
                )
            })
            .collect();
        // Listed by Risk Name, as the reference sorts the tab.
        risks.sort_by_key(|(_, cells)| cells[1].to_lowercase());
        let selected = self.detail.rows[0].filter(|&r| r < risks.len());
        let size = Vec2::new(
            ui.available_width(),
            ui.available_height() - form::BUTTON_BAR,
        );
        let mut customize = None;
        let out = grid_columns(
            ui,
            "risks_grid",
            &self.detail_columns(&gc::RISKS, false),
            risks.len(),
            selected,
            size,
            &mut |ui, ev, _| match ev {
                Ev::Cell(r, c) => cell(ui, &risks[r].1[c]),
                Ev::Menu(_) => grid_menu_customize(ui, &gc::RISKS, &mut customize),
            },
        );
        if let Some(r) = out.clicked {
            self.detail.rows[0] = Some(r);
        }
        if let Some(g) = customize {
            self.open_column_customize(g);
        }
        let mut remove = false;
        form::button_row(ui, |ui| {
            ui.add_space(0.);
            // With no unassigned risks the picker opens empty.
            let assign = form::add_button(ui, "Assign", true);
            self.action("detail_risks_assign", &assign);
            if assign.clicked() {
                self.detail.picker = Some(Picker::Risk { selected: None });
            }
            remove = form::command_button(ui, "Remove", None, selected.is_some()).clicked();
        });
        if remove {
            if let Some(r) = selected {
                records::remove_row(&mut self.project.document, "TASKRISK", risks[r].0);
                self.detail.rows[0] = None;
                self.touch_records();
            }
        }
    }
    fn unassigned_risks(&self, task_id: i64, pid: i64) -> Vec<(i64, [String; 2])> {
        let doc = &self.project.document;
        let used: Vec<i64> = records::rows_for(doc, "TASKRISK", task_id, pid)
            .into_iter()
            .map(|r| num(&table_value(doc, "TASKRISK", r, "risk_id")))
            .collect();
        let risks = records::risk_table(doc);
        project_rows(doc, risks, pid)
            .into_iter()
            .map(|r| {
                (
                    num(&table_value(doc, risks, r, "risk_id")),
                    [
                        table_value(doc, risks, r, "risk_code"),
                        table_value(doc, risks, r, "risk_name"),
                    ],
                )
            })
            .filter(|(id, _)| !used.contains(id))
            .collect()
    }

    pub(super) fn udf_tab(&mut self, ui: &mut egui::Ui, i: usize) {
        let (task_id, _) = self.ids(i);
        let origin = self.project.model.origin_serial;
        let fields = udf_fields(&self.project.document, task_id, origin);
        let selected = self.detail.rows[0].filter(|&r| r < fields.len());
        let size = Vec2::new(ui.available_width().min(560.), ui.available_height() - 4.);
        let mut writes: Vec<(i64, setters::UdfValue)> = vec![];
        let mut error = None;
        let shown: Vec<String> = fields.iter().map(|f| f.display(origin)).collect();
        let out = grid(
            ui,
            "udf_grid",
            &[("Title", 200.), ("Value", 250.)],
            fields.len(),
            selected,
            size,
            &mut |ui, ev, sel| {
                let Ev::Cell(r, c) = ev else {
                    return;
                };
                let f = &fields[r];
                if c == 0 {
                    cell(ui, &f.label);
                    return;
                }
                if !sel || f.kind == UdfKind::ReadOnly {
                    cell(ui, &shown[r]);
                    return;
                }
                match f.kind {
                    UdfKind::Text => {
                        if let Some(t) = text_cell(ui, &f.text) {
                            writes.push((f.type_id, setters::UdfValue::Text(t)));
                        }
                    }
                    UdfKind::Number => {
                        editor_cell(ui);
                        let key = format!("udf-{}-{task_id}", f.type_id);
                        let current = shown[r].clone();
                        let entry = self
                            .detail
                            .memo
                            .entry(key.clone())
                            .or_insert_with(|| (current.clone(), current.clone()));
                        let id = ui.id().with(&key);
                        if entry.0 != current && !ui.memory(|m| m.has_focus(id)) {
                            *entry = (current.clone(), current.clone());
                        }
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut entry.1)
                                .id(id)
                                .frame(false)
                                .desired_width(f32::INFINITY),
                        );
                        if response.lost_focus() && entry.1 != current {
                            let t = entry.1.trim().to_string();
                            if t.is_empty() {
                                writes.push((f.type_id, setters::UdfValue::Number(None)));
                            } else if let Ok(v) = t.replace(',', "").parse::<f64>() {
                                writes.push((f.type_id, setters::UdfValue::Number(Some(v))));
                            } else {
                                error = Some(format!("{} must be a number.", f.label));
                                entry.1 = current;
                            }
                        }
                    }
                    UdfKind::Date => {
                        editor_cell(ui);
                        let mut value = f.date;
                        match self.date_edit(
                            ui,
                            &format!("udf-{}-{task_id}", f.type_id),
                            &mut value,
                            true,
                        ) {
                            Ok(true) => writes.push((f.type_id, setters::UdfValue::Date(value))),
                            Ok(false) => {}
                            Err(e) => error = Some(e),
                        }
                    }
                    UdfKind::Indicator => {
                        let mut picked = None;
                        egui::ComboBox::from_id_salt(("udf_indicator", f.type_id, task_id))
                            .selected_text(f.text.as_str())
                            .width(ui.available_width() - 8.)
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(f.text.is_empty(), " ").clicked() {
                                    picked = Some(String::new());
                                }
                                for (name, color) in INDICATORS {
                                    ui.horizontal(|ui| {
                                        let (rect, _) = ui
                                            .allocate_exact_size(Vec2::splat(12.), Sense::hover());
                                        paint_indicator(ui.painter(), rect.center(), color);
                                        if ui
                                            .selectable_label(
                                                f.text.eq_ignore_ascii_case(name),
                                                name,
                                            )
                                            .clicked()
                                        {
                                            picked = Some(name.to_string());
                                        }
                                    });
                                }
                            });
                        if let Some(t) = picked.filter(|t| *t != f.text) {
                            writes.push((f.type_id, setters::UdfValue::Text(t)));
                        }
                    }
                    UdfKind::ReadOnly => {}
                }
            },
        );
        if let Some(r) = out.clicked {
            self.detail.rows[0] = Some(r);
        }
        if fields.is_empty() {
            ui.label("No activity user defined fields are defined in this file.");
        }
        for (type_id, value) in writes {
            if let Err(e) = self.edit_activity(i, setters::Edit::Udf { type_id, value }) {
                error = Some(e);
            }
        }
        if let Some(e) = error {
            self.error(e);
        }
    }

    /// Assign dialogs for notebook topics, codes, documents and risks.
    pub(super) fn record_picker(&mut self, ctx: &egui::Context, picker: &mut Picker) -> bool {
        // The New Activity wizard picks codes and documents for an activity
        // it has not made yet: the pick goes to the wizard.
        let wizard = self.wizard_picks(crate::activity_wizard::Pick::Record);
        let selected_activity = if wizard {
            Some(usize::MAX)
        } else {
            self.selected
                .filter(|&i| i < self.project.model.activities.len())
        };
        let Some(i) = selected_activity else {
            return false;
        };
        let (task_id, pid) = if wizard {
            (0, self.project.model.project.proj_id)
        } else {
            self.ids(i)
        };
        let doc = &self.project.document;
        let (title, cols, options): (&str, Vec<(&str, f32)>, PickerRows) = match picker {
            Picker::Topic { .. } => {
                let used: Vec<i64> = records::rows_for(doc, "TASKMEMO", task_id, pid)
                    .into_iter()
                    .map(|r| num(&table_value(doc, "TASKMEMO", r, "memo_type_id")))
                    .collect();
                (
                    "Assign Notebook Topic",
                    vec![("Notebook Topic", 300.)],
                    self.notebook_topics()
                        .into_iter()
                        .filter(|(id, _)| !used.contains(id))
                        .map(|(id, name)| (id, vec![name]))
                        .collect(),
                )
            }
            Picker::Code { type_id, .. } => {
                let types = self.code_types(pid);
                if type_id.is_none_or(|t| !types.iter().any(|(id, _)| *id == t)) {
                    *type_id = types.first().map(|(id, _)| *id);
                }
                let values = doc
                    .table("ACTVCODE")
                    .map(|t| {
                        let mut v: Vec<(i64, i64, Vec<String>)> = t
                            .rows
                            .iter()
                            .filter(|r| Some(num(t.value(r, "actv_code_type_id"))) == *type_id)
                            .map(|r| {
                                (
                                    num(t.value(r, "seq_num")),
                                    num(t.value(r, "actv_code_id")),
                                    vec![
                                        t.value(r, "short_name").to_string(),
                                        t.value(r, "actv_code_name").to_string(),
                                    ],
                                )
                            })
                            .collect();
                        v.sort_by(|a, b| (a.0, &a.2).cmp(&(b.0, &b.2)));
                        v.into_iter().map(|(_, id, cells)| (id, cells)).collect()
                    })
                    .unwrap_or_default();
                (
                    "Assign Activity Codes",
                    vec![("Code Value", 110.), ("Description", 220.)],
                    values,
                )
            }
            Picker::Document { .. } => (
                "Assign Work Products and Documents",
                vec![("Work Product and Document", 220.), ("Reference No.", 110.)],
                self.unassigned_documents(task_id, pid)
                    .into_iter()
                    .map(|(id, c)| (id, c.to_vec()))
                    .collect(),
            ),
            Picker::Risk { .. } => (
                "Assign Risks",
                vec![("Risk ID", 100.), ("Risk Name", 230.)],
                self.unassigned_risks(task_id, pid)
                    .into_iter()
                    .map(|(id, c)| (id, vec![c[0].clone(), c[1].clone()]))
                    .collect(),
            ),
            Picker::Relationship { .. } => return false,
        };
        let code_types = if matches!(picker, Picker::Code { .. }) {
            self.code_types(pid)
        } else {
            vec![]
        };
        let code_targets = if wizard {
            0
        } else {
            self.assign_targets().len()
        };
        let mut dictionary = None;
        let mut double = None;
        const DIALOG: &str = "assign_record";
        let is_code = matches!(picker, Picker::Code { .. });
        let current = picker_choice(picker);
        let mut buttons = vec![AssignButton {
            key: "picker_assign",
            caption: "Assign",
            enabled: current.is_some(),
        }];
        // Assign Activity Codes, as observed: Remove takes the selected
        // value off the selected activities that have it; New is on once
        // a value is selected (or the code type has none yet).
        let code_targets_ids: Vec<i64> = if is_code && !wizard {
            let targets = self.assign_targets();
            let targets = if targets.is_empty() { vec![i] } else { targets };
            targets
                .iter()
                .map(|&t| self.project.model.activities[t].task_id)
                .collect()
        } else {
            vec![]
        };
        let remove_code = match self.assign_picked(DIALOG, current).as_slice() {
            [one] if is_code && !wizard => {
                let doc = &self.project.document;
                let has = |task: i64| {
                    doc.table("TASKACTV").is_some_and(|t| {
                        t.rows.iter().any(|r| {
                            num(t.value(r, "task_id")) == task
                                && num(t.value(r, "actv_code_id")) == *one
                        })
                    })
                };
                code_targets_ids.iter().any(|&t| has(t)).then_some(*one)
            }
            _ => None,
        };
        if is_code && !wizard {
            buttons.push(AssignButton {
                key: "picker_code_remove",
                caption: "Remove",
                enabled: remove_code.is_some(),
            });
            // A new value of the code type shown, typed in Add Code Value.
            buttons.push(AssignButton {
                key: "picker_code_new",
                caption: "New...",
                enabled: code_type_of(picker).is_some()
                    && (current.is_some() || options.is_empty()),
            });
        }
        let window = AssignWindow {
            id: DIALOG,
            title,
            buttons,
            close: "picker_close",
            help: "picker_help",
        };
        let keys: Vec<Option<i64>> = options.iter().map(|(id, _)| Some(*id)).collect();
        let (open, clicked) = self.assign_window(ctx, &window, |app, ui| {
            if let Picker::Code { type_id, selected } = picker {
                ui.horizontal(|ui| {
                    ui.label("Activity Code");
                    if code_targets > 1 {
                        ui.label(format!(
                            "Assigns to the {code_targets} selected activities."
                        ));
                    }
                    egui::ComboBox::from_id_salt("picker_code_type")
                        .selected_text(
                            code_types
                                .iter()
                                .find(|(id, _)| Some(*id) == *type_id)
                                .map_or("", |(_, n)| n.as_str()),
                        )
                        .width(220.)
                        .show_ui(ui, |ui| {
                            for (id, name) in &code_types {
                                if ui.selectable_label(Some(*id) == *type_id, name).clicked() {
                                    *type_id = Some(*id);
                                    *selected = None;
                                }
                            }
                        });
                    // The dictionary reopens this picker for the
                    // selected activity, so the wizard goes without it.
                    if !wizard {
                        let link = ui.link("Activity Codes...");
                        app.action("picker_code_dictionary", &link);
                        if link.clicked() {
                            dictionary = Some(*type_id);
                        }
                    }
                });
            }
            let selected = match picker {
                Picker::Code { selected, .. }
                | Picker::Topic { selected }
                | Picker::Document { selected }
                | Picker::Risk { selected } => selected,
                Picker::Relationship { .. } => unreachable!(),
            };
            let out = app.assign_list(
                ui,
                DIALOG,
                "record_picker_grid",
                &cols,
                &keys,
                selected,
                Vec2::new(360., 200.),
                &mut |ui, r, c, _| cell(ui, &options[r].1[c]),
            );
            double = out.double;
        });
        let mut assign: Vec<i64> = match clicked {
            Some("picker_assign") => self.assign_picked(DIALOG, picker_choice(picker)),
            _ => vec![],
        };
        if let Some(id) = double {
            assign = vec![id];
        }
        if let (Some("picker_code_remove"), Some(code_id)) = (clicked, remove_code) {
            self.detail.removal = Some(super::removals::Removal::CodeFrom {
                code_id,
                task_ids: code_targets_ids,
            });
        }
        let new_value = code_type_of(picker).filter(|_| clicked == Some("picker_code_new"));
        if let Some(t) = new_value {
            self.open_new_code(t);
        }
        if wizard {
            for &id in &assign {
                match picker {
                    Picker::Code {
                        type_id: Some(t), ..
                    } => {
                        // One value per code type: the first chosen.
                        self.wizard_take_code(*t, id);
                        break;
                    }
                    Picker::Document { selected } => {
                        self.wizard_take_document(id);
                        *selected = None;
                    }
                    _ => {}
                }
            }
        } else if !assign.is_empty() {
            let mut close = false;
            self.record_assign(ctx, i, task_id, pid, picker, &assign, &mut close);
            if close {
                self.detail.assign_rows.remove(DIALOG);
                return false;
            }
        }
        if let Some(type_id) = dictionary {
            self.detail.assign_rows.remove(DIALOG);
            self.open_codes_for_assign(type_id);
            return false;
        }
        open
    }

    /// Assign the choices `ids` of record picker `picker` to activity `i`
    /// (every selected activity for codes). Sets `close` when the picker
    /// closes after it (a notebook topic).
    #[allow(clippy::too_many_arguments)]
    fn record_assign(
        &mut self,
        ctx: &egui::Context,
        i: usize,
        task_id: i64,
        pid: i64,
        picker: &mut Picker,
        ids: &[i64],
        close: &mut bool,
    ) {
        match picker {
            Picker::Topic { selected } => {
                for &id in ids {
                    let doc = &mut self.project.document;
                    let memo_id = records::next_id(doc, "TASKMEMO", "memo_id");
                    records::add_row(
                        doc,
                        "TASKMEMO",
                        &[
                            ("memo_id", memo_id.to_string()),
                            ("task_id", task_id.to_string()),
                            ("memo_type_id", id.to_string()),
                            ("proj_id", pid.to_string()),
                            ("task_memo", records::text_to_memo("")),
                        ],
                    );
                }
                *selected = None;
                *close = true;
                self.touch_records();
                self.detail.rows[0] = Some(
                    records::rows_for(&self.project.document, "TASKMEMO", task_id, pid).len() - 1,
                );
            }
            Picker::Code { type_id, .. } => {
                // One value per code type and activity: the first chosen
                // (several selected values of one type assign only it).
                if let (Some(t), Some(&id)) = (*type_id, ids.first()) {
                    let targets = self.assign_targets();
                    if targets.len() > 1 {
                        let time = ctx.input(|i| i.time);
                        self.assign_code_bulk(&targets, t, id, time);
                    } else {
                        self.assign_code(i, t, id);
                    }
                }
            }
            Picker::Document { selected } => {
                for &id in ids {
                    openxer_xer::edit::documents::link(&mut self.project, id, Some(task_id), None);
                }
                *selected = None;
                self.touch_records();
            }
            Picker::Risk { selected } => {
                for &id in ids {
                    records::add_row(
                        &mut self.project.document,
                        "TASKRISK",
                        &[
                            ("task_id", task_id.to_string()),
                            ("risk_id", id.to_string()),
                            ("proj_id", pid.to_string()),
                        ],
                    );
                }
                *selected = None;
                self.touch_records();
            }
            Picker::Relationship { .. } => {}
        }
    }

    pub(super) fn picker_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut picker) = self.detail.picker.take() else {
            return;
        };
        let keep = if matches!(picker, Picker::Relationship { .. }) {
            self.relationship_picker(ctx, &mut picker)
        } else {
            self.record_picker(ctx, &mut picker)
        };
        if keep && self.detail.picker.is_none() {
            self.detail.picker = Some(picker);
        }
        if self.detail.picker.is_some() {
            self.new_code_dialog(ctx);
        } else {
            self.detail.new_code = None;
        }
    }
}

/// The code type Assign Activity Codes shows.
fn code_type_of(picker: &Picker) -> Option<i64> {
    match picker {
        Picker::Code { type_id, .. } => *type_id,
        _ => None,
    }
}

/// The current choice of a record picker.
fn picker_choice(picker: &Picker) -> Option<i64> {
    match picker {
        Picker::Code { selected, .. }
        | Picker::Topic { selected }
        | Picker::Document { selected }
        | Picker::Risk { selected } => *selected,
        Picker::Relationship { .. } => None,
    }
}

#[cfg(test)]
pub use openxer_xer::edit::udf::set_udf;
pub use openxer_xer::edit::udf::{udf_fields, udf_kind, UdfField, UdfKind};

/// The indicator choices with the color each is drawn in.
pub const INDICATORS: [(&str, egui::Color32); 4] = [
    ("Red", egui::Color32::from_rgb(214, 40, 40)),
    ("Yellow", egui::Color32::from_rgb(240, 192, 0)),
    ("Green", egui::Color32::from_rgb(46, 160, 67)),
    ("Blue", egui::Color32::from_rgb(40, 100, 214)),
];

/// The indicator named by `text`, ignoring case.
pub fn indicator_named(text: &str) -> Option<(&'static str, egui::Color32)> {
    INDICATORS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(text.trim()))
        .copied()
}

/// A small filled disc with a thin outline, our own drawing of an indicator.
pub fn paint_indicator(painter: &egui::Painter, center: Pos2, color: egui::Color32) {
    painter.circle_filled(center, 4.5, color);
    painter.circle_stroke(
        center,
        4.5,
        Stroke::new(1_f32, egui::Color32::from_black_alpha(110)),
    );
}
/// How the desktop shows a field value.
pub trait UdfDisplay {
    fn display(&self, origin: i64) -> String;
}
impl UdfDisplay for UdfField {
    fn display(&self, origin: i64) -> String {
        match self.kind {
            UdfKind::Number => self.number.map(format_number).unwrap_or_default(),
            UdfKind::Date => format::date(origin, self.date),
            _ => self.text.clone(),
        }
    }
}
