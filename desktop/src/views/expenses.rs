//! Expenses view (Project > Expenses): every expense item of the open
//! project (PROJCOST) with its activity, category, costs, accrual and
//! vendor, grouped by WBS or activity. Add, Delete and the details tabs edit
//! the same rows as the Activity Details Expenses tab, so undo and export
//! treat them alike.
use super::grid::Row;
use super::record_view::{self, ListSpec, ListState, PickTarget, RowKey};
use super::sheet;
use super::*;
use crate::details::record_tabs::ACCRUAL;
use crate::records::{self, float, num};
use crate::schedule_dialogs::Canvas;
pub use openxer_xer::edit::project_records::{
    add_expense, delete_expense, move_expense, set_expense,
};

pub const TITLES: [&str; 12] = [
    "Activity ID",
    "Activity Name",
    "Expense Item",
    "Expense Category",
    "Budgeted Cost",
    "Actual Cost",
    "Remaining Cost",
    "At Completion Cost",
    "Accrual Type",
    "Vendor",
    "WBS",
    "Cost Account",
];
const WIDTHS: [f32; 12] = [
    100., 180., 160., 110., 90., 90., 90., 100., 100., 120., 110., 130.,
];
const RIGHT: [usize; 4] = [4, 5, 6, 7];
pub const TABS: [&str; 5] = [
    "General",
    "Activity",
    "Costs",
    "Description",
    "User Defined Fields",
];

/// How the table groups expense items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Wbs,
    Activity,
    None,
}

pub struct ExpensesState {
    pub list: ListState,
    pub group: Group,
}

impl Default for ExpensesState {
    fn default() -> Self {
        Self {
            list: ListState::new("expenses", &TITLES, &WIDTHS),
            group: if crate::defaults::expenses_grouped() {
                Group::Wbs
            } else {
                Group::None
            },
        }
    }
}

impl ExpensesState {
    pub fn json(&self) -> serde_json::Value {
        let mut v = self.list.json();
        v["group"] = match self.group {
            Group::Wbs => "wbs",
            Group::Activity => "activity",
            Group::None => "none",
        }
        .into();
        v
    }
    pub fn restore(&mut self, value: &serde_json::Value) {
        self.list.restore(value, TABS.len());
        self.group = match value["group"].as_str() {
            Some("activity") => Group::Activity,
            Some("none") => Group::None,
            Some("wbs") => Group::Wbs,
            _ => self.group,
        };
    }
}

/// One expense item as the view lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct Expense {
    pub id: i64,
    pub task_id: i64,
    pub activity_id: String,
    pub activity_name: String,
    pub wbs: i64,
    pub name: String,
    pub category: String,
    pub budgeted: f64,
    pub actual: f64,
    pub remaining: f64,
    pub accrual: String,
    pub vendor: String,
    /// Cost Account name.
    pub account: String,
}

impl Expense {
    pub fn at_completion(&self) -> f64 {
        self.actual + self.remaining
    }
}

/// The open project's expense items, by Activity ID then item name.
pub fn expenses(p: &ProjectData) -> Vec<Expense> {
    let doc = &p.document;
    let pid = p.model.project.proj_id;
    let categories = record_view::dictionary(doc, "COSTTYPE", "cost_type_id", "cost_type");
    let accounts = record_view::dictionary(doc, "ACCOUNT", "acct_id", "acct_name");
    // Activity IDs indexed once, not a scan of every activity per row.
    let codes: std::collections::HashMap<i64, &str> = p
        .model
        .activities
        .iter()
        .rev()
        .map(|a| (a.task_id, a.task_code.as_str()))
        .collect();
    let mut list: Vec<Expense> = record_view::project_rows(doc, "PROJCOST", pid)
        .into_iter()
        .map(|r| {
            let v = |k: &str| records::get(doc, "PROJCOST", r, k);
            let task_id = num(&v("task_id"));
            let (activity_id, activity_name) =
                codes
                    .get(&task_id)
                    .map_or((String::new(), String::new()), |code| {
                        (
                            code.to_string(),
                            p.details
                                .get(&task_id)
                                .map_or(String::new(), |d| d.name.clone()),
                        )
                    });
            Expense {
                id: num(&v("cost_item_id")),
                task_id,
                activity_id,
                activity_name,
                wbs: p.details.get(&task_id).map_or(0, |d| d.wbs),
                name: v("cost_name"),
                category: record_view::name_of(&categories, &v("cost_type_id")),
                budgeted: float(&v("target_cost")),
                actual: float(&v("act_cost")),
                remaining: float(&v("remain_cost")),
                accrual: record_view::label_of(&ACCRUAL, &v("cost_load_type")),
                vendor: v("vendor_name"),
                account: record_view::name_of(&accounts, &v("acct_id")),
            }
        })
        .collect();
    list.sort_by(|a, b| {
        a.activity_id
            .cmp(&b.activity_id)
            .then(a.name.cmp(&b.name))
            .then(a.id.cmp(&b.id))
    });
    list
}

impl App {
    pub(super) fn expenses_view(&mut self, host: &mut egui::Ui) {
        // Kept while the project is unchanged, not rebuilt every frame.
        let serial = self.project_serial();
        let list = self.memo(serial, "expenses", (), || expenses(&self.project));
        let state = &self.views.records.expenses;
        let group = state.group;
        let collapsed = state.list.collapsed.clone();
        let mut rows = vec![];
        let mut keys = vec![];
        let record = |e: &Expense, depth: usize| Row {
            depth,
            icon: Some(chrome::Icon::Expenses),
            cells: vec![
                e.activity_id.clone(),
                e.activity_name.clone(),
                e.name.clone(),
                e.category.clone(),
                format::cost(e.budgeted),
                format::cost(e.actual),
                format::cost(e.remaining),
                format::cost(e.at_completion()),
                e.accrual.clone(),
                e.vendor.clone(),
                record_view::wbs_path(&self.project, e.wbs),
                e.account.clone(),
            ],
            strips: depth,
            ..Default::default()
        };
        if group == Group::None {
            for e in list.iter() {
                keys.push(RowKey::Record(e.id));
                rows.push(record(e, 0));
            }
        } else {
            // Bands in WBS outline order (or by Activity ID), each with its items.
            let mut bands: Vec<(i64, String)> = vec![];
            for e in list.iter() {
                let band = if group == Group::Wbs {
                    e.wbs
                } else {
                    e.task_id
                };
                if !bands.iter().any(|(b, _)| *b == band) {
                    let caption = if group == Group::Wbs {
                        let name = self
                            .project
                            .wbs
                            .iter()
                            .find(|w| w.id == band)
                            .map_or(String::new(), |w| w.name.clone());
                        let code = record_view::wbs_path(&self.project, band);
                        if code.is_empty() {
                            project_title(&self.project)
                        } else {
                            format!("{code}  {name}")
                        }
                    } else {
                        format!("{}  {}", e.activity_id, e.activity_name)
                    };
                    bands.push((band, caption));
                }
            }
            if group == Group::Wbs {
                let order: Vec<i64> = record_view::wbs_choices(&self.project)
                    .into_iter()
                    .map(|(id, _, _)| id)
                    .collect();
                bands.sort_by_key(|(id, _)| order.iter().position(|o| o == id));
            }
            for (band, caption) in bands {
                let open = !collapsed.contains(&band);
                keys.push(RowKey::Band(band));
                rows.push(Row {
                    expander: Some(open),
                    cells: vec![caption],
                    band: true,
                    span: true,
                    ..Default::default()
                });
                if !open {
                    continue;
                }
                for e in list.iter().filter(|e| {
                    band == if group == Group::Wbs {
                        e.wbs
                    } else {
                        e.task_id
                    }
                }) {
                    keys.push(RowKey::Record(e.id));
                    rows.push(record(e, 1));
                }
            }
        }
        record_view::keep_selection(&mut self.views.records.expenses.list, &keys);
        let selected = self.views.records.expenses.list.selected;
        let spec = ListSpec {
            id: "expenses",
            display: "All Expenses".into(),
            titles: &TITLES,
            right: &RIGHT,
            tabs: &TABS,
            commands: vec![
                (
                    "Add",
                    chrome::Icon::Add,
                    !self.project.model.activities.is_empty(),
                ),
                ("Delete", chrome::Icon::Delete, selected.is_some()),
            ],
            empty: "This project has no expense items. Add one to an activity with the Add button.",
            details: "Expense Details",
            customize: "Expense",
            extras: vec![],
        };
        let chosen = selected.and_then(|id| list.iter().find(|e| e.id == id).cloned());
        let command = self.record_list_view(
            host,
            &spec,
            |v| &mut v.records.expenses.list,
            rows,
            keys,
            Box::new(move |app, c, size, tab| app.expense_details(c, size, tab, chosen.as_ref())),
            &mut |app, ui| {
                let g = &mut app.views.records.expenses.group;
                let before = *g;
                ui.radio_value(g, Group::Wbs, "Group by WBS");
                ui.radio_value(g, Group::Activity, "Group by Activity");
                ui.radio_value(g, Group::None, "No Grouping");
                if *g != before {
                    app.views.records.expenses.list.collapsed.clear();
                }
            },
        );
        match command {
            Some("Add") => self.expense_command_add(),
            Some("Delete") => self.expense_command_delete(),
            _ => {}
        }
    }

    /// Add: a new item on the selected item's activity, else on the
    /// activity selected in the Activities view.
    pub(super) fn expense_command_add(&mut self) {
        let list = expenses(&self.project);
        let selected = self.views.records.expenses.list.selected;
        let task_id = selected
            .and_then(|id| list.iter().find(|e| e.id == id))
            .map(|e| e.task_id)
            .or_else(|| {
                self.selected
                    .and_then(|i| self.project.model.activities.get(i))
                    .map(|a| a.task_id)
            })
            .or_else(|| self.project.model.activities.first().map(|a| a.task_id));
        let Some(task_id) = task_id else {
            self.status = "Add an activity before adding expenses.".into();
            return;
        };
        let id = add_expense(&mut self.project, task_id);
        self.views.records.expenses.list.selected = Some(id);
        self.views.records.expenses.list.collapsed.clear();
        self.touch_records();
        self.status = "Expense item added".into();
    }

    pub(super) fn expense_command_delete(&mut self) {
        self.record_delete(MainView::Expenses, false);
    }

    fn set_expense_field(&mut self, id: i64, column: &str, text: String) {
        if set_expense(&mut self.project, id, column, text) {
            self.touch_records();
        }
    }

    fn expense_details(&mut self, c: &mut Canvas, size: Vec2, tab: usize, e: Option<&Expense>) {
        let (w, h) = (size.x, size.y);
        let Some(e) = e else {
            c.label(12., 12., 13., "Select an expense item.", true);
            return;
        };
        let id = e.id;
        let doc = &self.project.document;
        let v = |k: &str| record_view::value(doc, "PROJCOST", "cost_item_id", id, k);
        let half = ((w - 40.) / 2.).clamp(160., 420.);
        let x2 = 20. + half + 20.;
        match TABS[tab.min(TABS.len() - 1)] {
            "General" => {
                let name = v("cost_name");
                let category = v("cost_type_id");
                let accrual = v("cost_load_type");
                let vendor = v("vendor_name");
                let po = v("po_number");
                let unit = v("qty_name");
                let account = record_view::name_of(
                    &record_view::dictionary(doc, "ACCOUNT", "acct_id", "acct_name"),
                    &v("acct_id"),
                );
                let mut auto = v("auto_compute_act_flag") == "Y";
                let categories =
                    record_view::dictionary(doc, "COSTTYPE", "cost_type_id", "cost_type");
                if let Some(t) = sheet::text_field(
                    c,
                    "expense_name",
                    20.,
                    25.,
                    w - 40.,
                    "Expense Item",
                    &name,
                    true,
                ) {
                    self.set_expense_field(id, "cost_name", records::clean(&t));
                }
                if let Some(code) = record_view::dictionary_field(
                    c,
                    "expense_category",
                    20.,
                    70.,
                    half,
                    "Expense Category",
                    &categories,
                    &category,
                    true,
                ) {
                    self.set_expense_field(id, "cost_type_id", code);
                }
                sheet::above(c, x2, 70., "Cost Account", true);
                sheet::value_box(c, "expense_account", x2, 70., half, &account, false);
                if let Some(code) = record_view::combo_field(
                    c,
                    "expense_accrual",
                    20.,
                    115.,
                    half,
                    "Accrual Type",
                    &ACCRUAL,
                    &accrual,
                    true,
                ) {
                    self.set_expense_field(id, "cost_load_type", code);
                }
                if let Some(t) = sheet::text_field(
                    c,
                    "expense_unit",
                    x2,
                    115.,
                    half,
                    "Unit of Measure",
                    &unit,
                    true,
                ) {
                    self.set_expense_field(id, "qty_name", records::clean(&t));
                }
                if let Some(t) = sheet::text_field(
                    c,
                    "expense_vendor",
                    20.,
                    160.,
                    half,
                    "Vendor",
                    &vendor,
                    true,
                ) {
                    self.set_expense_field(id, "vendor_name", records::clean(&t));
                }
                if let Some(t) = sheet::text_field(
                    c,
                    "expense_po",
                    x2,
                    160.,
                    half,
                    "Document Number",
                    &po,
                    true,
                ) {
                    self.set_expense_field(id, "po_number", records::clean(&t));
                }
                if c.checkbox(
                    "expense_auto",
                    20.,
                    195.,
                    200.,
                    21.,
                    "Auto Compute Actuals",
                    &mut auto,
                    true,
                )
                .clicked()
                {
                    self.set_expense_field(
                        id,
                        "auto_compute_act_flag",
                        if auto { "Y" } else { "N" }.into(),
                    );
                }
            }
            "Activity" => {
                let (task_id, origin) = (e.task_id, self.project.model.origin_serial);
                let activity = self
                    .project
                    .model
                    .activities
                    .iter()
                    .find(|a| a.task_id == task_id);
                let wbs = record_view::wbs_path(&self.project, e.wbs);
                let start = activity.and_then(|a| a.act_start.or(a.early_start));
                let finish = activity.and_then(|a| a.act_finish.or(a.early_finish));
                let status = activity.map_or(String::new(), |a| {
                    openxer_xer::status_label(a.status).to_string()
                });
                sheet::above(c, 20., 25., "Activity", true);
                sheet::value_box(
                    c,
                    "expense_activity",
                    20.,
                    25.,
                    w - 140.,
                    &format!("{}  {}", e.activity_id, e.activity_name),
                    false,
                );
                if c.button(
                    "expense_activity_pick",
                    w - 110.,
                    25.,
                    90.,
                    21.,
                    "Activity...",
                    false,
                    true,
                ) {
                    self.open_picker(PickTarget::ExpenseActivity(id));
                }
                sheet::above(c, 20., 70., "WBS", true);
                sheet::value_box(c, "expense_wbs", 20., 70., half, &wbs, false);
                sheet::above(c, x2, 70., "Activity Status", true);
                sheet::value_box(c, "expense_status", x2, 70., half, &status, false);
                sheet::above(c, 20., 115., "Activity Start", true);
                sheet::value_box(
                    c,
                    "expense_start",
                    20.,
                    115.,
                    half,
                    &format::date(origin, start),
                    false,
                );
                sheet::above(c, x2, 115., "Activity Finish", true);
                sheet::value_box(
                    c,
                    "expense_finish",
                    x2,
                    115.,
                    half,
                    &format::date(origin, finish),
                    false,
                );
            }
            "Costs" => {
                let third = ((w - 60.) / 3.).clamp(120., 260.);
                let col = |k: usize| 20. + k as f32 * (third + 20.);
                let fields = [
                    (0, 25., "target_qty", "Budgeted Units"),
                    (1, 25., "cost_per_qty", "Price / Unit"),
                    (2, 25., "target_cost", "Budgeted Cost"),
                    (0, 70., "act_cost", "Actual Cost"),
                    (1, 70., "remain_cost", "Remaining Cost"),
                ]
                .map(|(k, row, column, caption)| (k, row, column, caption, v(column)));
                for (k, row, column, caption, stored) in fields {
                    if let Some(t) = record_view::number_text(
                        c,
                        &format!("expense_{column}"),
                        col(k),
                        row,
                        third,
                        caption,
                        &stored,
                        false,
                        true,
                    ) {
                        self.set_expense_field(id, column, t);
                    }
                }
                sheet::above(c, col(2), 70., "At Completion Cost", true);
                sheet::value_box(
                    c,
                    "expense_at_completion",
                    col(2),
                    70.,
                    third,
                    &format::cost(e.at_completion()),
                    true,
                );
                let pct = if e.at_completion() > 0. {
                    e.actual / e.at_completion() * 100.
                } else {
                    0.
                };
                sheet::above(c, col(0), 115., "Expense % Complete", true);
                sheet::value_box(
                    c,
                    "expense_pct",
                    col(0),
                    115.,
                    third,
                    &format!("{pct:.0}%"),
                    true,
                );
            }
            "Description" => {
                let stored = v("cost_descr");
                if let Some(t) = record_view::memo(
                    c,
                    "expense_description",
                    6.,
                    6.,
                    w - 12.,
                    h - 12.,
                    &stored,
                    true,
                ) {
                    self.set_expense_field(id, "cost_descr", t);
                }
            }
            _ => {
                let rows = record_view::udf_rows(doc, "PROJCOST", id);
                record_view::udf_tab(c, "expense_udf", w, h, &rows);
            }
        }
    }
}
