//! Spreadsheet Fields and Profile Options: the fields a usage spreadsheet
//! lists at the left of its time-phased values, and what the usage profiles
//! draw besides their bars. Both are edited in dialogs that change nothing
//! until OK.
use super::time_fields;
use super::usage::SheetSort;
use super::*;
use crate::dialog_kit::{self as kit, Col};
use crate::grid::columns::{self as cols};
use crate::views::resources::Resource;

/// Which list of fields a field set belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    /// The activity columns of the Activity Usage Spreadsheet.
    Activity,
    /// The activity columns of the assignment list of the Resource Usage
    /// Spreadsheet.
    Assignment,
    /// The resource list of the Resource Usage Spreadsheet.
    Resource,
    /// The resource list of the Resource Usage Profile.
    ProfileResource,
    /// The columns of the Resource Assignments table.
    Assignments,
}

/// Field ids and titles of a view table's columns, in column order.
fn table_fields(which: Which) -> Option<(&'static [&'static str], &'static [&'static str])> {
    match which {
        Which::Assignments => Some((
            &crate::views::assignment_columns::IDS,
            &crate::views::assignment_columns::TITLES,
        )),
        _ => None,
    }
}

/// Resource list fields: id, title, width in pixels. Resource codes
/// (`rcode_<type id>`) and resource user fields (`rudf_<field id>`) follow
/// them in the dialog.
pub const RESOURCE_FIELDS: [(&str, &str, f32); 21] = [
    ("resource_id", "Resource ID", 120.),
    ("resource_name", "Resource Name", 150.),
    ("resource_type", "Resource Type", 100.),
    ("primary_role", "Primary Role", 110.),
    ("calendar", "Calendar", 110.),
    ("max_units", "Max Units/Time", 95.),
    ("default_units", "Default Units/Time", 110.),
    ("price", "Price/Unit", 80.),
    ("unit", "Unit of Measure", 100.),
    ("currency", "Currency", 90.),
    ("overtime_allowed", "Overtime Allowed", 100.),
    ("overtime_factor", "Overtime Factor", 95.),
    ("auto_compute_actuals", "Auto Compute Actuals", 125.),
    ("cost_from_units", "Calculate costs from units", 150.),
    ("employee_id", "Employee ID", 90.),
    ("title", "Title", 110.),
    ("email", "E-Mail Address", 150.),
    ("office_phone", "Office Phone", 100.),
    ("location", "Location", 100.),
    ("active", "Active", 50.),
    ("parent_resource", "Parent Resource", 110.),
];

/// Resource fields holding numbers: right aligned, sorted as numbers.
pub const NUMBER_FIELDS: [&str; 4] = ["max_units", "default_units", "price", "overtime_factor"];

pub fn resource_field(id: &str) -> Option<(&'static str, &'static str, f32)> {
    RESOURCE_FIELDS.iter().copied().find(|f| f.0 == id)
}

/// A resource code or resource user field column: its prefix and id.
pub fn dynamic_field(id: &str) -> Option<(&'static str, i64)> {
    ["rcode_", "rudf_"].into_iter().find_map(|prefix| {
        id.strip_prefix(prefix)
            .and_then(|n| n.parse::<i64>().ok())
            .map(|n| (prefix, n))
    })
}

/// A resource list field id that can be saved: a fixed field, a resource
/// code or a resource user field.
pub fn known_resource_field(id: &str) -> bool {
    resource_field(id).is_some() || dynamic_field(id).is_some()
}

/// The resource code and user field columns this project offers.
fn dynamic_resource_fields(p: &ProjectData) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = super::usage_list::code_types(p)
        .into_iter()
        .map(|(id, name)| (format!("rcode_{id}"), name))
        .collect();
    out.extend(
        crate::dictionaries::ops::udfs(&p.document, "RSRC")
            .into_iter()
            .map(|u| (format!("rudf_{}", u.id), u.title)),
    );
    out
}

pub fn default_ids(which: Which) -> Vec<String> {
    let ids: &[&str] = match which {
        Which::Activity => &["activity_id", "activity_name"],
        Which::Assignment => &["activity_id", "start", "finish"],
        Which::Resource => &["resource_id", "resource_name"],
        Which::ProfileResource => &["resource_id", "resource_name", "resource_type"],
        Which::Assignments => crate::views::assignment_columns::default_ids(),
    };
    ids.iter().map(|s| s.to_string()).collect()
}

/// Saved field ids that still make sense: no duplicates, at least one.
/// Activity columns depend on the project (codes, user fields), so those
/// are checked when used.
pub fn restore_ids(v: &serde_json::Value, which: Which) -> Option<Vec<String>> {
    let list = v.as_array()?;
    let mut out: Vec<String> = vec![];
    for id in list.iter().filter_map(|x| x.as_str()) {
        let known = match which {
            Which::Activity | Which::Assignment => id.len() < 100,
            _ => known_resource_field(id),
        };
        if known && !out.iter().any(|o| o == id) && out.len() < 40 {
            out.push(id.to_string());
        }
    }
    (!out.is_empty()).then_some(out)
}

/// A field as a spreadsheet lists it.
#[derive(Clone, Debug)]
pub struct Field {
    pub id: String,
    pub title: String,
    pub width: f32,
    /// The activity column behind it, for the activity-based lists.
    pub column: Option<cols::Column>,
}

/// The fields of `ids` that exist in this project, in order; the defaults
/// when none does.
pub fn fields_of(p: &ProjectData, which: Which, ids: &[String]) -> Vec<Field> {
    let pick = |ids: &[String]| -> Vec<Field> {
        match which {
            Which::Activity | Which::Assignment => {
                let catalog = cols::catalog(p);
                ids.iter()
                    .filter_map(|id| cols::find(&catalog, id))
                    .map(|c| Field {
                        id: c.id.clone(),
                        title: c.title.clone(),
                        width: if c.id == "activity_id" { 90. } else { c.width },
                        column: Some(c.clone()),
                    })
                    .collect()
            }
            Which::Assignments => vec![],
            _ => {
                let extra = dynamic_resource_fields(p);
                ids.iter()
                    .filter_map(|id| match resource_field(id) {
                        Some((id, title, width)) => Some(Field {
                            id: id.into(),
                            title: title.into(),
                            width,
                            column: None,
                        }),
                        None => extra
                            .iter()
                            .find(|(e, _)| e == id)
                            .map(|(id, title)| Field {
                                id: id.clone(),
                                title: title.clone(),
                                width: 100.,
                                column: None,
                            }),
                    })
                    .collect()
            }
        }
    };
    let found = pick(ids);
    if found.is_empty() {
        pick(&default_ids(which))
    } else {
        found
    }
}

/// Every field a list can show, in the order the dialog offers them.
pub fn available(p: &ProjectData, which: Which) -> Vec<(String, String)> {
    match which {
        Which::Activity | Which::Assignment => cols::catalog(p)
            .into_iter()
            .map(|c| (c.id, c.title))
            .collect(),
        Which::Assignments => {
            let (ids, titles) = table_fields(which).unwrap_or_default();
            // The assignment user defined fields follow the table's own.
            let udfs = crate::views::udf_columns::choices(&p.document, "assignments");
            ids.iter()
                .zip(titles)
                .map(|(id, title)| (id.to_string(), title.to_string()))
                .chain(udfs)
                .collect()
        }
        _ => RESOURCE_FIELDS
            .iter()
            .map(|(id, title, _)| (id.to_string(), title.to_string()))
            .chain(dynamic_resource_fields(p))
            .collect(),
    }
}

/// The name of the role with id `role`.
fn role_name(p: &ProjectData, role: &str) -> String {
    let role = role.trim();
    if role.is_empty() {
        return String::new();
    }
    p.document
        .table("ROLES")
        .and_then(|t| {
            t.rows
                .iter()
                .find(|r| t.value(r, "role_id").trim() == role)
                .map(|r| t.value(r, "role_short_name").to_string())
        })
        .unwrap_or_default()
}

/// The text of resource field `id` for `r`.
pub fn resource_text(p: &ProjectData, r: &Resource, id: &str) -> String {
    match id {
        "resource_id" => r.code.clone(),
        "resource_name" => r.name.clone(),
        "resource_type" => r.kind.label().to_string(),
        "primary_role" => role_name(p, &r.role),
        "unit" => r.unit.clone(),
        "calendar" => r.calendar.clone(),
        "max_units" => r
            .max_units
            .map_or(String::new(), crate::views::resources::table_units_text),
        "default_units" => crate::views::resources::table_units_text(r.default_units),
        "price" => r.price.map_or(String::new(), format::cost),
        "currency" => r.info.currency.clone(),
        "overtime_allowed" => yes_no(r.info.overtime),
        "overtime_factor" => r.info.overtime_factor.clone(),
        "auto_compute_actuals" => yes_no(r.info.auto_compute),
        "cost_from_units" => yes_no(r.info.cost_from_units),
        "employee_id" => r.info.employee_id.clone(),
        "title" => r.info.title.clone(),
        "email" => r.info.email.clone(),
        "office_phone" => r.info.office_phone.clone(),
        "location" => r.info.location.clone(),
        "active" => yes_no(r.active),
        "parent_resource" => r.info.parent_code.clone(),
        _ => match dynamic_field(id) {
            Some(("rcode_", kind)) => resource_code(p, r.id, kind),
            Some((_, udf)) => resource_udf(p, r.id, udf),
            None => String::new(),
        },
    }
}

fn yes_no(flag: bool) -> String {
    if flag { "Yes" } else { "No" }.to_string()
}

/// The value of resource code type `kind` on resource `rsrc`.
fn resource_code(p: &ProjectData, rsrc: i64, kind: i64) -> String {
    let doc = &p.document;
    let (Some(t), Some(values)) = (doc.table("RSRCRCAT"), doc.table("RCATVAL")) else {
        return String::new();
    };
    let num = crate::records::num;
    t.rows
        .iter()
        .filter(|r| num(t.value(r, "rsrc_id")) == rsrc)
        .find_map(|r| {
            let value = t.value(r, "rsrc_catg_id");
            values
                .rows
                .iter()
                .find(|x| values.value(x, "rsrc_catg_id") == value)
                .filter(|x| num(values.value(x, "rsrc_catg_type_id")) == kind)
                .map(|x| values.value(x, "rsrc_catg_short_name").to_string())
        })
        .unwrap_or_default()
}

/// The value of resource user field `udf` on resource `rsrc`, as a User
/// Defined cell shows it (`views::udf_columns`).
fn resource_udf(p: &ProjectData, rsrc: i64, udf: i64) -> String {
    crate::views::udf_columns::value_text(&p.document, p.model.origin_serial, udf, rsrc)
}

/// The value of field `id` for sorting: numbers for the unit and price
/// fields, lower-case text for the rest.
pub fn resource_sort_value(
    p: &ProjectData,
    r: &Resource,
    id: &str,
) -> super::usage_list::SortValue {
    use super::usage_list::SortValue;
    let number = match id {
        "max_units" => Some(r.max_units),
        "default_units" => Some(Some(r.default_units)),
        "price" => Some(r.price),
        "overtime_factor" => Some(r.info.overtime_factor.trim().parse::<f64>().ok()),
        _ => None,
    };
    if let Some(v) = number {
        return v.map_or(SortValue::Empty, SortValue::Number);
    }
    if let Some(("rudf_", udf)) = dynamic_field(id) {
        // Numbers and dates by value, the rest by text.
        use crate::views::udf_columns::{value_sort, SortKey};
        return match value_sort(&p.document, p.model.origin_serial, udf, r.id) {
            Some(SortKey::Number(n)) => SortValue::Number(n),
            Some(SortKey::Text(t)) => SortValue::Text(t),
            None => SortValue::Empty,
        };
    }
    let text = resource_text(p, r, id);
    if text.is_empty() {
        SortValue::Empty
    } else {
        SortValue::Text(text.to_lowercase())
    }
}

/// Column widths for `fields` across `total` pixels. The default activity
/// pair keeps its measured 90 px ID; the last field takes what is left, and
/// fields that do not fit are cut off at the edge.
pub fn widths_in(fields: &[Field], total: f32) -> Vec<f32> {
    let mut widths: Vec<f32> = fields.iter().map(|f| f.width).collect();
    let sum: f32 = widths.iter().sum();
    if let Some(last) = widths.last_mut() {
        if sum < total {
            *last += total - sum;
        }
    }
    widths
}

/// Widths split by weights across `total`: the assignment list keeps its
/// 4:3:3 split of Activity ID, Start and Finish.
pub fn weighted(fields: &[Field], total: f32) -> Vec<f32> {
    let weight = |f: &Field| if f.id == "activity_id" { 4. } else { 3. };
    let sum: f32 = fields.iter().map(weight).sum();
    fields.iter().map(|f| total * weight(f) / sum).collect()
}

/// Which spreadsheet or profile the Spreadsheet Fields dialog edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldsView {
    ActivitySheet,
    ResourceSheet,
    ResourceProfile,
    Assignments,
}

impl FieldsView {
    /// The view edits the shown columns of a table.
    fn is_table(self) -> bool {
        matches!(self, FieldsView::Assignments)
    }
    fn lists(self) -> &'static [Which] {
        match self {
            FieldsView::ActivitySheet => &[Which::Activity],
            FieldsView::ResourceSheet => &[Which::Resource, Which::Assignment],
            FieldsView::ResourceProfile => &[Which::ProfileResource],
            FieldsView::Assignments => &[Which::Assignments],
        }
    }
}

/// The open Spreadsheet Fields dialog: working copies of the lists.
#[derive(Clone, Debug)]
pub struct FieldsDraft {
    pub view: FieldsView,
    pub tab: usize,
    pub lists: Vec<Vec<String>>,
    pub sorts: Vec<SheetSort>,
    pub line_numbers: bool,
    pub available: Option<usize>,
    pub selected: Option<usize>,
    pub error: Option<String>,
}

impl FieldsDraft {
    pub fn new(
        view: FieldsView,
        u: &usage::UsageState,
        table: &crate::views::grid::Columns,
    ) -> Self {
        let lists = view
            .lists()
            .iter()
            .map(|w| match w {
                Which::Assignments => {
                    let ids = table_fields(*w).map(|f| f.0).unwrap_or_default();
                    let id_of = |c: usize| ids.get(c).copied().or_else(|| table.extra_of(c));
                    table
                        .order
                        .iter()
                        .filter_map(|c| id_of(*c).map(String::from))
                        .collect()
                }
                Which::Activity => u.activity_fields.clone(),
                Which::Assignment => u.assignment_fields.clone(),
                Which::Resource => u.resource_fields.clone(),
                Which::ProfileResource => u.profile_fields.clone(),
            })
            .collect();
        let sorts = view
            .lists()
            .iter()
            .map(|w| match w {
                Which::Activity | Which::Assignments => SheetSort::default(),
                Which::Assignment => u.assignment_sort.clone(),
                Which::Resource => u.sheet_list.sort.clone(),
                Which::ProfileResource => u.profile_list.sort.clone(),
            })
            .collect();
        Self {
            view,
            tab: 0,
            lists,
            sorts,
            line_numbers: u.line_numbers,
            available: None,
            selected: None,
            error: None,
        }
    }

    pub fn which(&self) -> Which {
        self.view.lists()[self.tab.min(self.view.lists().len() - 1)]
    }

    /// Copies the lists back into the view settings.
    pub fn apply(&self, u: &mut usage::UsageState) {
        for (k, w) in self.view.lists().iter().enumerate() {
            match w {
                Which::Activity => u.activity_fields = self.lists[k].clone(),
                Which::Assignment => {
                    u.assignment_fields = self.lists[k].clone();
                    u.assignment_sort = self.sorts[k].clone();
                }
                Which::Resource => {
                    u.resource_fields = self.lists[k].clone();
                    u.sheet_list.sort = self.sorts[k].clone();
                }
                Which::ProfileResource => {
                    u.profile_fields = self.lists[k].clone();
                    u.profile_list.sort = self.sorts[k].clone();
                }
                Which::Assignments => {}
            }
        }
        if !self.view.is_table() {
            u.line_numbers = self.line_numbers;
        }
    }

    /// Copies a table view's list back into its shown columns.
    pub fn apply_table(&self, columns: &mut crate::views::grid::Columns) {
        let Some((ids, _)) = table_fields(self.which()).filter(|_| self.view.is_table()) else {
            return;
        };
        // The table's own fields by place, its User Defined ones by id.
        let shown: Vec<Result<usize, String>> = self.lists[0]
            .iter()
            .filter_map(|id| match ids.iter().position(|f| f == id) {
                Some(i) => Some(Ok(i)),
                None => crate::views::udf_columns::type_of(id).map(|_| Err(id.clone())),
            })
            .collect();
        columns.show_fields(&shown, crate::views::udf_columns::WIDTH);
    }

    /// The first column of a table carries its tree: it stays.
    fn required(&self, id: &str) -> bool {
        self.view.is_table() && table_fields(self.which()).is_some_and(|f| f.0[0] == id)
    }

    /// Adds the picked available field at the end of the selected list.
    pub fn add(&mut self, available: &[(String, String)]) {
        let Some((id, _)) = self.available.and_then(|i| available.get(i)) else {
            return;
        };
        let list = &mut self.lists[self.tab];
        if !list.contains(id) && list.len() < 40 {
            list.push(id.clone());
            self.selected = Some(list.len() - 1);
        }
        self.available = None;
        self.error = None;
    }

    /// Takes the picked selected field out. The last one stays.
    pub fn remove(&mut self) {
        let len = self.lists[self.tab].len();
        let Some(i) = self.selected.filter(|i| *i < len) else {
            return;
        };
        let id = self.lists[self.tab][i].clone();
        if len == 1 || self.required(&id) {
            self.error = Some(if len == 1 {
                "A spreadsheet keeps at least one field.".into()
            } else {
                "The first column stays.".into()
            });
            return;
        }
        let list = &mut self.lists[self.tab];
        list.remove(i);
        self.selected = Some(i.min(list.len() - 1));
        self.error = None;
    }

    /// Moves the picked selected field one place up or down.
    pub fn shift(&mut self, up: bool) {
        let list = &mut self.lists[self.tab];
        let Some(i) = self.selected.filter(|i| *i < list.len()) else {
            return;
        };
        let to = if up { i.checked_sub(1) } else { Some(i + 1) };
        if let Some(to) = to.filter(|t| *t < list.len()) {
            list.swap(i, to);
            self.selected = Some(to);
        }
    }
}

/// The open Spreadsheet Fields > Customize... dialog: the working copy of a
/// spreadsheet's time-phased fields.
#[derive(Clone, Debug)]
pub struct TimeFieldsDraft {
    pub sheet: time_fields::Sheet,
    /// Saved ids, in order.
    pub selected: Vec<String>,
    /// The picked Selected Options row and Available Options field.
    pub pick_selected: Option<usize>,
    pub pick_available: Option<String>,
    pub open_groups: BTreeSet<String>,
    /// The Resource Assignments window's spreadsheet's fields, not the
    /// Resource Usage Spreadsheet's.
    pub window: bool,
}

impl TimeFieldsDraft {
    pub fn new(sheet: time_fields::Sheet, u: &usage::UsageState) -> Self {
        let ids = match sheet {
            time_fields::Sheet::Activity => &u.activity_time_fields,
            time_fields::Sheet::Resource => &u.resource_time_fields,
        };
        Self {
            sheet,
            selected: time_fields::fields_of(sheet, ids)
                .iter()
                .map(|f| f.id())
                .collect(),
            pick_selected: Some(0),
            pick_available: None,
            // The groups start closed, as the Columns dialog's do.
            open_groups: BTreeSet::new(),
            window: false,
        }
    }

    /// Adds field `id` after the picked selected row (at the end when none
    /// is picked), unless it is there already or the list is full.
    pub fn add(&mut self, id: &str) {
        let full = self.selected.len() >= time_fields::MAX_FIELDS;
        if full || self.selected.iter().any(|s| s == id) {
            return;
        }
        let at = self
            .pick_selected
            .map_or(self.selected.len(), |i| i + 1)
            .min(self.selected.len());
        self.selected.insert(at, id.to_string());
        self.pick_selected = Some(at);
    }

    /// Copies the list into the view settings.
    pub fn apply(&self, u: &mut usage::UsageState) {
        let list = match self.sheet {
            time_fields::Sheet::Activity => &mut u.activity_time_fields,
            time_fields::Sheet::Resource => &mut u.resource_time_fields,
        };
        *list = self.selected.clone();
    }
}

/// Fields a sort can use: the selected list's own.
fn sort_choice(sort: &SheetSort, id: &str) -> usize {
    match &sort.field {
        Some(f) if f == id => {
            if sort.descending {
                2
            } else {
                1
            }
        }
        _ => 0,
    }
}

impl App {
    /// Opens the Spreadsheet Fields dialog for `view`.
    pub(crate) fn open_spreadsheet_fields(&mut self, view: FieldsView) {
        self.panes.usage_fields = Some(FieldsDraft::new(
            view,
            &self.panes.usage,
            &self.views.assignments.columns,
        ));
    }

    /// Escape closes the usage dialogs without applying them.
    pub(crate) fn usage_dialogs_escape(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            // An open Available Options menu closes first.
            if self.panes.time_fields.is_some() && crate::available_options::menu_open(ctx) {
                return;
            }
            // The topmost one only: the list dialogs open over the others.
            let _ = self.panes.usage_list_filter.take().is_some()
                || self.panes.usage_list_group.take().is_some()
                || self.panes.time_fields.take().is_some()
                || self.panes.usage_fields.take().is_some()
                || self.panes.profile_dialog.take().is_some();
        }
    }

    /// Opens Spreadsheet Fields > Customize... for `sheet`'s time-phased
    /// fields.
    pub(crate) fn open_time_fields(&mut self, sheet: time_fields::Sheet) {
        let mut d = TimeFieldsDraft::new(sheet, &self.panes.usage);
        // From the Resource Assignments window's spreadsheet: its own list.
        let host = super::usage_cells::SheetHost::Assignments;
        if sheet == time_fields::Sheet::Resource && self.panes.sheet_host == host {
            d.window = true;
            d.selected = self
                .views
                .assignments
                .time_fields()
                .iter()
                .map(|f| f.id())
                .collect();
        }
        self.panes.time_fields = Some(d);
    }

    /// OK or Apply in Spreadsheet Fields > Customize...: the list goes to
    /// its spreadsheet.
    fn apply_time_fields(&mut self, d: &TimeFieldsDraft) {
        if d.window {
            self.views.assignments.time_field_ids = d.selected.clone();
        } else {
            d.apply(&mut self.panes.usage);
        }
    }

    /// Spreadsheet Fields > Customize...: the Columns dialog's chooser over
    /// the time-phased fields, grouped Time Interval and Cumulative. OK and
    /// Apply set the spreadsheet's fields, Default goes back to the one
    /// field a spreadsheet starts with.
    fn time_fields_dialog(&mut self, ctx: &egui::Context) {
        use crate::grid::dialogs::{chooser_panel, ChoiceRow, Chooser, ChooserOut};
        let Some(mut d) = self.panes.time_fields.take() else {
            return;
        };
        let view = self.available_view;
        let list = view.is_list();
        let catalog = time_fields::catalog(d.sheet);
        let costs = self.panes.usage.costs;
        let items: Vec<(String, String, &'static str)> = catalog
            .iter()
            .filter(|f| !d.selected.contains(&f.id()))
            .map(|f| (f.id(), f.name(d.sheet, costs), f.group()))
            .collect();
        let mut tree: Vec<ChoiceRow> = vec![];
        if list {
            let mut flat: Vec<&(String, String, &str)> = items.iter().collect();
            crate::available_options::alphabetical(&mut flat, |x| &x.1);
            tree.extend(flat.into_iter().map(|x| ChoiceRow::Item(&x.0, &x.1)));
        } else {
            for g in time_fields::GROUPS {
                let open = d.open_groups.contains(g);
                tree.push(ChoiceRow::Group(g, open));
                if open {
                    tree.extend(
                        items
                            .iter()
                            .filter(|x| x.2 == g)
                            .map(|x| ChoiceRow::Item(&x.0, &x.1)),
                    );
                }
            }
        }
        let titles: Vec<String> = d
            .selected
            .iter()
            .filter_map(|id| time_fields::TimeField::parse(d.sheet, id))
            .map(|f| f.selected_name(d.sheet, costs))
            .collect();
        let mut panel = ChooserOut::default();
        let mut clicked = None;
        let ((), closed, actions) = kit::window(
            ctx,
            "usage_time_fields",
            "Fields",
            Vec2::new(554., 295.),
            |c| {
                c.fill(0., 0., 554., 295., kit::STRIP);
                panel = chooser_panel(
                    c,
                    "time_fields",
                    &tree,
                    view,
                    d.pick_available.as_deref(),
                    &titles,
                    d.pick_selected,
                );
                let entries = [
                    kit::Entry::ok("time_fields_ok"),
                    kit::Entry::cancel("time_fields_cancel"),
                    kit::Entry::apply("time_fields_apply", true),
                    kit::Entry::Button("time_fields_default", "Default", true, true),
                    kit::Entry::help("time_fields_help"),
                ];
                clicked = kit::button_column(c, 554., &entries).clicked;
            },
        );
        self.record_dialog_actions(actions);
        if let Some(view) = panel.view {
            self.set_available_view(view);
            d.pick_available = None;
        }
        if let Some(r) = panel.tree_click {
            match &tree[r] {
                ChoiceRow::Group(g, _) => {
                    let g = g.to_string();
                    if !d.open_groups.insert(g.clone()) {
                        d.open_groups.remove(&g);
                    }
                }
                ChoiceRow::Item(id, _) => d.pick_available = Some(id.to_string()),
            }
        }
        if let Some(r) = panel.tree_double {
            if let ChoiceRow::Item(id, _) = &tree[r] {
                let id = id.to_string();
                d.add(&id);
                d.pick_available = None;
            }
        }
        if let Some(r) = panel.picked {
            d.pick_selected = Some(r);
        }
        let shown: Vec<String> = tree
            .iter()
            .filter_map(|r| match r {
                ChoiceRow::Item(id, _) => Some(id.to_string()),
                ChoiceRow::Group(..) => None,
            })
            .collect();
        for key in panel.clicks {
            let len = d.selected.len();
            let picked = d.pick_selected.filter(|i| *i < len);
            match key {
                Chooser::ADD => {
                    if let Some(id) = d.pick_available.take() {
                        d.add(&id);
                    }
                }
                Chooser::ADD_ALL => {
                    for id in &shown {
                        d.add(id);
                    }
                }
                // A spreadsheet keeps at least one field.
                Chooser::REMOVE => {
                    if let Some(i) = picked.filter(|_| len > 1) {
                        d.selected.remove(i);
                        d.pick_selected = Some(i.min(len - 2));
                    }
                }
                Chooser::REMOVE_ALL => {
                    d.selected.truncate(1);
                    d.pick_selected = Some(0);
                }
                Chooser::UP | Chooser::DOWN => {
                    let to = picked.and_then(|i| {
                        if key == Chooser::UP {
                            i.checked_sub(1)
                        } else {
                            Some(i + 1).filter(|t| *t < len)
                        }
                    });
                    if let (Some(i), Some(to)) = (picked, to) {
                        d.selected.swap(i, to);
                        d.pick_selected = Some(to);
                    }
                }
                _ => {}
            }
        }
        match clicked.as_deref() {
            Some("time_fields_ok") => {
                self.apply_time_fields(&d);
                crate::available_options::close_menu(ctx);
                return;
            }
            Some("time_fields_cancel") => {
                crate::available_options::close_menu(ctx);
                return;
            }
            Some("time_fields_apply") => self.apply_time_fields(&d),
            Some("time_fields_default") => {
                d.selected = time_fields::default_ids(d.sheet);
                d.pick_selected = Some(0);
            }
            Some("time_fields_help") => self.open_help_for("usage_time_fields"),
            _ => {}
        }
        if closed {
            crate::available_options::close_menu(ctx);
            return;
        }
        self.panes.time_fields = Some(d);
    }

    pub(crate) fn usage_dialogs(&mut self, ctx: &egui::Context) {
        self.time_fields_dialog(ctx);
        self.spreadsheet_fields_dialog(ctx);
        self.profile_options_dialog(ctx);
        self.usage_list_dialogs(ctx);
    }

    fn spreadsheet_fields_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.panes.usage_fields.take() else {
            return;
        };
        let which = d.which();
        let available: Vec<(String, String)> = available(&self.project, which)
            .into_iter()
            .filter(|(id, _)| !d.lists[d.tab].contains(id))
            .collect();
        let titles: BTreeMap<String, String> =
            self::available(&self.project, which).into_iter().collect();
        let tabs = d.view.lists().len();
        let mut group_sort = false;
        let (clicked, closed, actions) = kit::window(
            ctx,
            "usage_fields",
            "Spreadsheet Fields",
            Vec2::new(456., 322.),
            |c| {
                if tabs > 1 {
                    let lists =
                        [("Resource list", "0"), ("Assignment list", "1")].map(|(label, key)| {
                            kit::VTab {
                                label,
                                key,
                                unavailable: None,
                            }
                        });
                    let page = (5., 23., 446., 262.);
                    if let Some(k) = kit::tabs(c, "usage_fields_tab_", page, &lists, d.tab) {
                        d.tab = k;
                        d.available = None;
                        d.selected = None;
                    }
                } else {
                    c.label(10., 4., 20., "Choose the fields to list, in order.", true);
                }
                c.label(10., 28., 16., "Available Options", true);
                c.label(260., 28., 16., "Selected Options", true);
                let out = kit::list(
                    c,
                    "usage_fields_available",
                    (10., 46., 186., 190.),
                    None,
                    &[Col {
                        title: "Field",
                        width: 168.,
                    }],
                    available.len(),
                    d.available,
                    Some(0),
                    &mut |c, i, _, rect, sel| {
                        kit::cell_text(c, rect, &available[i].1, sel, 4.);
                    },
                );
                if let Some(i) = out.clicked {
                    d.available = Some(i);
                    d.selected = None;
                }
                let current = d.lists[d.tab].clone();
                let out = kit::list(
                    c,
                    "usage_fields_selected",
                    (260., 46., 186., 190.),
                    None,
                    &[Col {
                        title: "Field",
                        width: 168.,
                    }],
                    current.len(),
                    d.selected,
                    Some(0),
                    &mut |c, i, _, rect, sel| {
                        let title = titles.get(&current[i]).cloned().unwrap_or_default();
                        kit::cell_text(c, rect, &title, sel, 4.);
                    },
                );
                if let Some(i) = out.clicked {
                    d.selected = Some(i);
                    d.available = None;
                }
                let mut clicked = None;
                for (key, y, caption, enabled) in [
                    ("usage_fields_add", 64., "Add >", d.available.is_some()),
                    ("usage_fields_remove", 94., "< Remove", d.selected.is_some()),
                    ("usage_fields_up", 134., "Up", d.selected.is_some()),
                    ("usage_fields_down", 164., "Down", d.selected.is_some()),
                ] {
                    if kit::button(c, key, 200., y, 56., 25., caption, enabled) {
                        clicked = Some(key.to_string());
                    }
                }
                c.checkbox(
                    "usage_fields_line_numbers",
                    10.,
                    244.,
                    186.,
                    18.,
                    "Line Numbers",
                    &mut d.line_numbers,
                    !d.view.is_table(),
                );
                let picked = d.selected.and_then(|i| current.get(i)).cloned();
                // The activity spreadsheet follows the table's own Group and
                // Sort; the resource lists sort on one of their fields.
                let sortable = matches!(
                    d.view,
                    FieldsView::ResourceSheet | FieldsView::ResourceProfile
                );
                c.label(
                    260.,
                    242.,
                    16.,
                    "Sort by the selected field",
                    sortable && picked.is_some(),
                );
                let tab = d.tab;
                let mut choice = picked
                    .as_deref()
                    .map_or(0, |id| sort_choice(&d.sorts[tab], id));
                let before = choice;
                for (k, (key, caption, x, w)) in [
                    ("usage_fields_sort_none", "None", 250., 48.),
                    ("usage_fields_sort_ascending", "Ascending", 298., 76.),
                    ("usage_fields_sort_descending", "Descending", 374., 82.),
                ]
                .into_iter()
                .enumerate()
                {
                    let on = sortable && picked.is_some();
                    if kit::radio(c, key, x, 258., w, 18., caption, choice == k, on) {
                        choice = k;
                    }
                }
                if choice != before {
                    if let Some(id) = picked {
                        d.sorts[tab] = match choice {
                            0 => SheetSort::default(),
                            n => SheetSort {
                                field: Some(id),
                                descending: n == 2,
                            },
                        };
                    }
                }
                if let Some(e) = &d.error {
                    kit::error_text(c, 10., 296., 210., e);
                }
                if d.view == FieldsView::ActivitySheet
                    && kit::button(
                        c,
                        "usage_fields_group_sort",
                        10.,
                        288.,
                        120.,
                        25.,
                        "Group and Sort...",
                        true,
                    )
                {
                    group_sort = true;
                }
                let row = kit::button_row(
                    c,
                    446.,
                    288.,
                    &[
                        kit::Entry::ok("usage_fields_ok"),
                        kit::Entry::cancel("usage_fields_cancel"),
                    ],
                );
                row.clicked.or(clicked)
            },
        );
        self.record_dialog_actions(actions);
        let clicked = clicked.as_deref();
        match clicked {
            Some("usage_fields_add") => d.add(&available),
            Some("usage_fields_remove") => d.remove(),
            Some("usage_fields_up") => d.shift(true),
            Some("usage_fields_down") => d.shift(false),
            Some("usage_fields_ok") => {
                d.apply(&mut self.panes.usage);
                if d.view == FieldsView::Assignments {
                    d.apply_table(&mut self.views.assignments.columns);
                }
                return;
            }
            Some("usage_fields_cancel") => return,
            _ => {}
        }
        if closed {
            return;
        }
        if group_sort {
            self.editing.pending = Some(editing::Command::GroupSort);
            return;
        }
        self.panes.usage_fields = Some(d);
    }
}
