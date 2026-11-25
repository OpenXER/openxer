//! Activity Details Resources tab: the assignment grid with in-place editing,
//! Add Resource, Add Role, Assign by Role and Remove, and their pickers.
//! Edits go through `openxer_xer::assignments`, which applies the duration
//! type rules and rebuilds the scheduling inputs; each edit is one undo step.
use super::assign_kit::{AssignButton, AssignWindow};
use super::*;
use crate::format::Periods;
use openxer_xer::assignments::{AssignmentEdit, AssignmentView, RATE_SOURCES, RATE_TYPES};

/// The picker window opened by the Resources tab buttons.
#[derive(Clone, Debug, PartialEq)]
pub enum AssignPicker {
    Resource {
        query: String,
        selected: Option<i64>,
    },
    Role {
        selected: Option<i64>,
    },
    /// Staff the role assignment in TASKRSRC row `row` with a resource that has the role.
    ByRole {
        row: usize,
        role: i64,
        selected: Option<i64>,
    },
    /// Edit > Assign > Resources by Role, first page: choose the role.
    PickRole {
        selected: Option<i64>,
    },
    /// Edit > Assign > Resources by Role, second page: choose a resource that
    /// has the role; it is assigned in that role to every selected activity.
    StaffRole {
        role: i64,
        selected: Option<i64>,
    },
}

/// Which of the Edit > Assign commands that work on resources was chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceAssign {
    Resources,
    ByRole,
    Roles,
}

/// Preference key of [`AssignmentCalc`].
pub const CALC_KEY: &str = "assignment_calculations_v1";

/// User Preferences, Calculations: what happens to an assignment when a
/// resource is assigned to it (Assignment Staffing).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Staffing {
    /// Use the new resource's units per time and overtime factor.
    New,
    /// Keep the assignment's own units per time and overtime factor.
    Current,
    #[default]
    Ask,
}

/// User Preferences, Calculations: where the price comes from when a
/// resource and a role share an assignment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PriceSource {
    Resource,
    Role,
    #[default]
    Ask,
}

/// User Preferences, Calculations, assignment options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AssignmentCalc {
    /// Recalculate units, duration and units/time of existing assignments by
    /// the duration type when assignments are added or removed (otherwise
    /// they are preserved).
    pub recalculate: bool,
    pub staffing: Staffing,
    pub price: PriceSource,
}

/// A resource replacement that waits for the user's choices.
#[derive(Clone, Debug, PartialEq)]
pub struct Question {
    pub row: usize,
    pub rsrc: i64,
    /// Ask for units/time (`Some(choice)`) or the price source.
    pub staffing: Option<bool>,
    pub role_price: Option<bool>,
}

/// Dictionary rows offered by the pickers and combo boxes: (id, cells).
pub(crate) type Rows = Vec<(i64, Vec<String>)>;
pub(crate) fn dictionary(
    doc: &openxer_xer::Document,
    table: &str,
    key: &str,
    columns: &[&str],
) -> Rows {
    doc.table(table)
        .map(|t| {
            t.rows
                .iter()
                .map(|r| {
                    (
                        num(t.value(r, key)),
                        columns.iter().map(|c| t.value(r, c).to_string()).collect(),
                    )
                })
                .filter(|(id, _)| *id != 0)
                .collect()
        })
        .unwrap_or_default()
}
pub(crate) fn resource_type_label(code: &str) -> &'static str {
    match code {
        "RT_Equip" => "Nonlabor",
        "RT_Mat" => "Material",
        _ => "Labor",
    }
}
/// `id  name` for a dictionary row, or the bare ID when it is not in the file.
pub(crate) fn id_name(rows: &[(i64, Vec<String>)], id: Option<i64>) -> String {
    let Some(id) = id else {
        return String::new();
    };
    rows.iter()
        .find(|(i, _)| *i == id)
        .map_or(id.to_string(), |(_, c)| {
            if c[0].is_empty() {
                c[1].clone()
            } else {
                format!("{}  {}", c[0], c[1])
            }
        })
}

/// Minutes in one of the duration unit named by `suffix` (the user's duration
/// unit when blank), at the default period lengths.
fn unit_minutes(suffix: &str) -> Option<i64> {
    let f = format::current();
    f.durations
        .parse(&format!("1{}", suffix.trim()), Periods::DEFAULT)
        .ok()
        .filter(|m| *m > 0)
}
/// Units typed in the user's Units format (`8`, `8h`, `1d`), as hours.
pub fn parse_units(text: &str) -> Option<f64> {
    let minutes = format::current().units.parse(text, Periods::DEFAULT).ok()?;
    (minutes >= 0).then_some(minutes as f64 / 60.)
}
/// Units/time typed as `100%`, `8h/d`, `8/d` or `8` (per the duration unit), as units per hour.
pub fn parse_units_per_time(text: &str) -> Option<f64> {
    let text = text.trim();
    if let Some(p) = text.strip_suffix('%') {
        return p
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| *v >= 0.)
            .map(|v| v / 100.);
    }
    let (units, per) = text.split_once('/').unwrap_or((text, ""));
    let units = parse_units(units)?;
    Some(units / (unit_minutes(per)? as f64 / 60.))
}

/// A money amount typed with or without the currency symbol and separators,
/// in the user's currency format. Not negative.
pub fn parse_money(text: &str) -> Option<f64> {
    format::parse_cost(text).filter(|v| v.is_finite() && *v >= 0.)
}
pub(crate) fn number_text(v: f64) -> String {
    records::format_number(v)
}
pub(crate) fn parse_number(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

/// An in-place number editor shown and typed in a display format.
fn value_editor(
    ui: &mut egui::Ui,
    value: f64,
    speed: f64,
    show: impl Fn(f64) -> String + 'static,
    parse: impl Fn(&str) -> Option<f64> + 'static,
) -> Option<f64> {
    editor_cell(ui);
    let mut v = value;
    let response = ui.add(
        egui::DragValue::new(&mut v)
            .range(0.0..=1.0e9)
            .speed(speed)
            .custom_formatter(move |v, _| show(v))
            .custom_parser(parse),
    );
    (response.changed() && (v - value).abs() > 1e-9).then_some(v)
}

/// The Curve picker's note on an activity whose duration type ignores
/// curves.
pub const CURVE_IGNORED: &str = "This activity's duration type ignores resource curves: its units spread evenly. Curves shape the spread only on Fixed Duration and Units/Time and Fixed Duration and Units activities.";

/// A curve's shape for the Curve picker: 21 bars in a small box.
fn curve_preview(ui: &mut egui::Ui, points: &[f64; 21]) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(126., 48.), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0., ui.visuals().extreme_bg_color);
    let top = points.iter().copied().fold(1e-9, f64::max) as f32;
    let pitch = (rect.width() - 4.) / 21.;
    for (k, v) in points.iter().enumerate() {
        let h = (*v as f32 / top) * (rect.height() - 6.);
        let x = rect.left() + 2. + pitch * k as f32;
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(x + 0.5, rect.bottom() - 3. - h),
                egui::pos2(x + pitch - 0.5, rect.bottom() - 3.),
            ),
            0.,
            ui.visuals().selection.bg_fill,
        );
    }
}

impl App {
    pub(super) fn resources_tab(&mut self, ui: &mut egui::Ui, i: usize) {
        let task_id = self.project.model.activities[i].task_id;
        let periods = format::activity_periods(&self.project, i);
        let rows: Vec<AssignmentView> = self
            .project
            .assignment_rows(task_id)
            .into_iter()
            .filter_map(|r| self.project.assignment_view(r))
            .collect();
        let doc = &self.project.document;
        let resources = dictionary(doc, "RSRC", "rsrc_id", &["rsrc_short_name", "rsrc_name"]);
        let roles = dictionary(doc, "ROLES", "role_id", &["role_short_name", "role_name"]);
        let accounts = dictionary(doc, "ACCOUNT", "acct_id", &["acct_short_name", "acct_name"]);
        // Resource curves (Enterprise > Resource Curves): they shape the
        // usage spreads, not the schedule. The list holds the file's curves
        // and the presets; a preset is copied into the file when it is
        // picked. A curve can be picked on any activity, but it shapes the
        // spread only on the Fixed Duration types; the picker says so.
        let curves = crate::enterprise::ops::curve_choices(doc);
        let curve_applies =
            openxer_xer::spread::curve_applies(&openxer_xer::edit::records::task_value(
                doc,
                task_id,
                self.project.model.project.proj_id,
                "duration_type",
            ));
        // "Manual" once period values were typed into a usage spreadsheet.
        let curve_names: Vec<String> = rows
            .iter()
            .map(|v| openxer_xer::edit::curves::curve_label(doc, v.row).unwrap_or_default())
            .collect();
        let mut curve_edit: Option<(usize, Option<String>)> = None;
        let completed = self.project.model.activities[i].is_completed();
        let started = self.project.model.activities[i].status != StatusType::NotStarted;
        let milestone = matches!(
            self.project.model.activities[i].task_type,
            TaskType::Mile | TaskType::FinMile
        );
        let selected = self.detail.rows[0].filter(|&r| r < rows.len());
        let origin = self.project.model.origin_serial;
        // Actual Start and Finish, Start and Finish open a date editor
        // whatever the status, as observed in the reference (the edit
        // layer refuses what the status does not allow). Remaining start:
        // not started (it is the assignment's lag). Remaining finish: not
        // completed.
        let date_editable = move |c: usize| match c {
            23 | 24 | 32 | 33 => true,
            25 => !started && !completed,
            _ => !completed,
        };
        let date_of = |v: &AssignmentView, c: usize| match c {
            23 => v.dates.act_start,
            24 => v.dates.act_finish,
            25 => v.dates.rem_start,
            32 => v.dates.act_start.or(v.dates.rem_start),
            33 => v.dates.act_finish.or(v.dates.rem_finish),
            _ => v.dates.rem_finish,
        };
        // Start is the actual start once the activity has started, Finish
        // the actual finish once it is completed; otherwise the remaining
        // dates.
        let date_edit_for = |c: usize, t: i64| match c {
            23 => AssignmentEdit::ActualStart(t),
            24 => AssignmentEdit::ActualFinish(t),
            32 if started => AssignmentEdit::ActualStart(t),
            33 if completed => AssignmentEdit::ActualFinish(t),
            25 | 32 => AssignmentEdit::RemainingStart(t),
            _ => AssignmentEdit::RemainingFinish(t),
        };
        let mut edits: Vec<(usize, AssignmentEdit)> = vec![];
        let mut replace: Option<(usize, i64)> = None;
        let mut primary: Option<(usize, bool)> = None;
        // Cost Units Linked ticked again: asked first.
        let mut relink: Option<usize> = None;
        let size = Vec2::new(
            ui.available_width(),
            ui.available_height() - form::BUTTON_BAR,
        );
        // The reference's default column set and widths; Show All turns on
        // every column and Customize Resource Columns picks and orders them.
        let columns = &super::grid_columns::RESOURCES;
        let shown = self.detail_columns(columns, self.detail.all_resource_columns);
        let mut all_columns = self.detail.all_resource_columns;
        let mut customize = false;
        let mut date_edits: Vec<(usize, AssignmentEdit)> = vec![];
        let mut date_error: Option<String> = None;
        let mut date_cells: Vec<(String, [f32; 4])> = vec![];
        let mut cell_rects: Vec<(String, [f32; 4])> = vec![];
        let out = grid_columns(
            ui,
            "resources_grid",
            &shown,
            rows.len(),
            selected,
            size,
            &mut |ui, ev, sel| {
                let Ev::Cell(r, c) = ev else {
                    customize |= super::grid_columns::menu_item(ui, columns);
                    let mut menu = crate::context_menu::Menu::on(ui, "resources_grid_menu");
                    if menu.check("show_all", "Show All Resource Columns", all_columns) {
                        all_columns = !all_columns;
                    }
                    return;
                };
                let v = &rows[r];
                let row = v.row;
                // Every cell by row and data column, for the scenarios.
                let m = ui.max_rect();
                let key = format!("assignment_cell_{r}_{c}");
                cell_rects.push((key, [m.left(), m.top(), m.right(), m.bottom()]));
                let rate_label = RATE_TYPES
                    .iter()
                    .find(|(code, _, _)| *code == v.rate_type)
                    .map_or(v.rate_type.as_str(), |(_, l, _)| l);
                match (c, sel) {
                    (0, true) if !resources.is_empty() => {
                        editor_cell(ui);
                        egui::ComboBox::from_id_salt(("assignment_resource", row))
                            .selected_text(id_name(&resources, v.rsrc_id))
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                for (id, cells) in &resources {
                                    let text = format!("{}  {}", cells[0], cells[1]);
                                    if ui.selectable_label(v.rsrc_id == Some(*id), text).clicked()
                                        && v.rsrc_id != Some(*id)
                                    {
                                        replace = Some((row, *id));
                                    }
                                }
                            });
                    }
                    (0, _) => cell(ui, id_name(&resources, v.rsrc_id)),
                    // A price that comes from the resource opens no editor,
                    // as observed; choose Override as the Rate Source to
                    // type one.
                    (1, true) if v.rate_source != "ST_Rsrc" => {
                        let show = |v: f64| format::cost(v);
                        if let Some(p) = value_editor(ui, v.price, 1., show, parse_money) {
                            edits.push((row, AssignmentEdit::PricePerUnit(p)));
                        }
                    }
                    (1, _) => cell(ui, format::cost(v.price)),
                    (2, true) => {
                        editor_cell(ui);
                        egui::ComboBox::from_id_salt(("assignment_rate_type", row))
                            .selected_text(rate_label)
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                for (code, label, _) in RATE_TYPES {
                                    if ui.selectable_label(v.rate_type == code, label).clicked()
                                        && v.rate_type != code
                                    {
                                        edits.push((row, AssignmentEdit::RateType(code.into())));
                                    }
                                }
                            });
                    }
                    (2, false) => cell(ui, rate_label),
                    (3, true) => {
                        editor_cell(ui);
                        let label = RATE_SOURCES
                            .iter()
                            .find(|(code, _)| *code == v.rate_source)
                            .map_or("", |(_, l)| l);
                        egui::ComboBox::from_id_salt(("assignment_rate_source", row))
                            .selected_text(label)
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                for (code, label) in RATE_SOURCES {
                                    if ui.selectable_label(v.rate_source == code, label).clicked()
                                        && v.rate_source != code
                                    {
                                        edits.push((row, AssignmentEdit::RateSource(code.into())));
                                    }
                                }
                            });
                    }
                    (3, _) => cell(
                        ui,
                        RATE_SOURCES
                            .iter()
                            .find(|(code, _)| *code == v.rate_source)
                            .map_or("", |(_, l)| l),
                    ),
                    (4, _) => {
                        let mut on = v.primary;
                        if ui
                            .add_enabled(v.rsrc_id.is_some(), egui::Checkbox::without_text(&mut on))
                            .changed()
                        {
                            primary = Some((row, on));
                        }
                    }
                    (5, true) => {
                        if let Some(u) =
                            value_editor(ui, v.budgeted.units, 1., format::units, parse_units)
                        {
                            edits.push((row, AssignmentEdit::BudgetedUnits(u)));
                        }
                    }
                    (5, false) => cell(ui, format::units(v.budgeted.units)),
                    (6, true) => {
                        if let Some(u) =
                            value_editor(ui, v.actual_units, 1., format::units, parse_units)
                        {
                            let regular = (u - v.actual_overtime_units).max(0.);
                            edits.push((row, AssignmentEdit::ActualRegularUnits(regular)));
                        }
                    }
                    (6, _) => cell(ui, format::units(v.actual_units)),
                    (7, true) => {
                        if let Some(u) =
                            value_editor(ui, v.remaining.units, 1., format::units, parse_units)
                        {
                            edits.push((row, AssignmentEdit::RemainingUnits(u)));
                        }
                    }
                    (7, _) => cell(ui, format::units(v.remaining.units)),
                    (8, true) => {
                        if let Some(u) = value_editor(
                            ui,
                            v.remaining.per_hour,
                            0.01,
                            format::units_per_time,
                            parse_units_per_time,
                        ) {
                            edits.push((row, AssignmentEdit::RemainingUnitsPerTime(u)));
                        }
                    }
                    (8, _) => cell(ui, format::units_per_time(v.remaining.per_hour)),
                    (9, true) if !roles.is_empty() => {
                        editor_cell(ui);
                        egui::ComboBox::from_id_salt(("assignment_role", row))
                            .selected_text(id_name(&roles, v.role_id))
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                if v.rsrc_id.is_some()
                                    && ui.selectable_label(v.role_id.is_none(), "").clicked()
                                    && v.role_id.is_some()
                                {
                                    edits.push((row, AssignmentEdit::Role(None)));
                                }
                                for (id, cells) in &roles {
                                    let text = format!("{}  {}", cells[0], cells[1]);
                                    if ui.selectable_label(v.role_id == Some(*id), text).clicked()
                                        && v.role_id != Some(*id)
                                    {
                                        edits.push((row, AssignmentEdit::Role(Some(*id))));
                                    }
                                }
                            });
                    }
                    (9, _) => cell(ui, id_name(&roles, v.role_id)),
                    (10, true) if !accounts.is_empty() => {
                        editor_cell(ui);
                        egui::ComboBox::from_id_salt(("assignment_account", row))
                            .selected_text(id_name(&accounts, v.acct_id))
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(v.acct_id.is_none(), "").clicked()
                                    && v.acct_id.is_some()
                                {
                                    edits.push((row, AssignmentEdit::CostAccount(None)));
                                }
                                for (id, cells) in &accounts {
                                    let text = format!("{}  {}", cells[0], cells[1]);
                                    if ui.selectable_label(v.acct_id == Some(*id), text).clicked()
                                        && v.acct_id != Some(*id)
                                    {
                                        edits.push((row, AssignmentEdit::CostAccount(Some(*id))));
                                    }
                                }
                            });
                    }
                    (10, _) => cell(ui, id_name(&accounts, v.acct_id)),
                    (11, true) => {
                        if let Some(u) = value_editor(
                            ui,
                            v.budgeted.per_hour,
                            0.01,
                            format::units_per_time,
                            parse_units_per_time,
                        ) {
                            edits.push((row, AssignmentEdit::BudgetedUnitsPerTime(u)));
                        }
                    }
                    (11, false) => cell(ui, format::units_per_time(v.budgeted.per_hour)),
                    (12, true) => {
                        editor_cell(ui);
                        let mut m = v.original_duration;
                        if format::duration_editor(ui, true, &mut m, periods, 0..=100000 * 60)
                            .changed()
                            && m != v.original_duration
                        {
                            edits.push((row, AssignmentEdit::OriginalDuration(m)));
                        }
                    }
                    (12, false) => cell(ui, format::duration(v.original_duration, periods)),
                    (13, true) if !completed => {
                        editor_cell(ui);
                        let mut m = v.remaining_duration;
                        if format::duration_editor(ui, true, &mut m, periods, 0..=100000 * 60)
                            .changed()
                            && m != v.remaining_duration
                        {
                            edits.push((row, AssignmentEdit::RemainingDuration(m)));
                        }
                    }
                    (13, _) => cell(ui, format::duration(v.remaining_duration, periods)),
                    (14, _) => {
                        let mut on = v.drives_dates;
                        if ui.add(egui::Checkbox::without_text(&mut on)).changed() {
                            edits.push((row, AssignmentEdit::DriveActivityDates(on)));
                        }
                    }
                    (16, true) if !started => {
                        editor_cell(ui);
                        let mut m = v.lag;
                        if format::duration_editor(ui, true, &mut m, periods, 0..=100000 * 60)
                            .changed()
                            && m != v.lag
                        {
                            edits.push((row, AssignmentEdit::Lag(m)));
                        }
                    }
                    (16, _) => cell(ui, format::duration(v.lag, periods)),
                    (17, true) => {
                        editor_cell(ui);
                        let mut m = v.planned_lag;
                        if format::duration_editor(ui, true, &mut m, periods, 0..=100000 * 60)
                            .changed()
                            && m != v.planned_lag
                        {
                            edits.push((row, AssignmentEdit::PlannedLag(m)));
                        }
                    }
                    (17, _) => cell(ui, format::duration(v.planned_lag, periods)),
                    (18, true) => {
                        if let Some(f) =
                            value_editor(ui, v.ot_factor, 0.1, number_text, parse_number)
                        {
                            edits.push((row, AssignmentEdit::OvertimeFactor(f)));
                        }
                    }
                    (18, _) => cell(ui, number_text(v.ot_factor)),
                    (19 | 20, true) if started => {
                        let value = if c == 19 {
                            v.actual_regular_units
                        } else {
                            v.actual_overtime_units
                        };
                        if let Some(u) = value_editor(ui, value, 1., format::units, parse_units) {
                            edits.push((
                                row,
                                if c == 19 {
                                    AssignmentEdit::ActualRegularUnits(u)
                                } else {
                                    AssignmentEdit::ActualOvertimeUnits(u)
                                },
                            ));
                        }
                    }
                    (19, _) => cell(ui, format::units(v.actual_regular_units)),
                    (20, _) => cell(ui, format::units(v.actual_overtime_units)),
                    (21 | 22, true) if started => {
                        let value = if c == 21 {
                            v.actual_regular_cost
                        } else {
                            v.actual_overtime_cost
                        };
                        if let Some(x) = value_editor(ui, value, 1., format::cost, parse_money) {
                            edits.push((
                                row,
                                if c == 21 {
                                    AssignmentEdit::ActualRegularCost(x)
                                } else {
                                    AssignmentEdit::ActualOvertimeCost(x)
                                },
                            ));
                        }
                    }
                    (21, _) => cell(ui, format::cost(v.actual_regular_cost)),
                    (22, _) => cell(ui, format::cost(v.actual_overtime_cost)),
                    (23..=26 | 32 | 33, true) if date_editable(c) => {
                        let mut value = date_of(v, c);
                        let key = format!("assignment-{row}-{c}");
                        let cell = ui.max_rect();
                        date_cells.push((
                            format!("assignment_date_{r}_{c}"),
                            [cell.left(), cell.top(), cell.right(), cell.bottom()],
                        ));
                        match self.date_edit(ui, &key, &mut value, true) {
                            Ok(true) => match value {
                                Some(t) => date_edits.push((row, date_edit_for(c, t))),
                                None => date_error = Some("Enter a date".into()),
                            },
                            Ok(false) => {}
                            Err(e) => date_error = Some(e),
                        }
                    }
                    (23..=26, _) => cell(ui, format::date_actual(origin, date_of(v, c), c < 25)),
                    // Costs: budgeted and remaining set the units at the
                    // price while cost and units are linked (the edit layer
                    // refuses a remaining cost on a completed activity).
                    (27, true) => {
                        let budget = v.budgeted_cost;
                        if let Some(x) = value_editor(ui, budget, 1., format::cost, parse_money) {
                            edits.push((row, AssignmentEdit::BudgetedCost(x)));
                        }
                    }
                    (27, _) => cell(ui, format::cost(v.budgeted_cost)),
                    (28, true) => {
                        let actual = v.actual_cost;
                        if let Some(x) = value_editor(ui, actual, 1., format::cost, parse_money) {
                            edits.push((row, AssignmentEdit::ActualCost(x)));
                        }
                    }
                    (28, _) => cell(ui, format::cost(v.actual_cost)),
                    (29, true) => {
                        let left = v.remaining_cost;
                        if let Some(x) = value_editor(ui, left, 1., format::cost, parse_money) {
                            edits.push((row, AssignmentEdit::RemainingCost(x)));
                        }
                    }
                    (29, _) => cell(ui, format::cost(v.remaining_cost)),
                    // At completion: typed, the remaining takes the value
                    // less the actual (refused once completed).
                    (30, true) => {
                        let total = v.actual_cost + v.remaining_cost;
                        if let Some(x) = value_editor(ui, total, 1., format::cost, parse_money) {
                            edits.push((row, AssignmentEdit::AtCompletionCost(x)));
                        }
                    }
                    (30, _) => cell(ui, format::cost(v.actual_cost + v.remaining_cost)),
                    (31, true) => {
                        let total = v.actual_units + v.remaining.units;
                        if let Some(u) = value_editor(ui, total, 1., format::units, parse_units) {
                            edits.push((row, AssignmentEdit::AtCompletionUnits(u)));
                        }
                    }
                    (31, _) => cell(ui, format::units(v.actual_units + v.remaining.units)),
                    // Start and Finish: the actual date once there is one,
                    // else the remaining date.
                    (32, _) => {
                        let start = v.dates.act_start.or(v.dates.rem_start);
                        let actual = v.dates.act_start.is_some();
                        cell(ui, format::date_actual(origin, start, actual));
                    }
                    (33, _) => {
                        let finish = v.dates.act_finish.or(v.dates.rem_finish);
                        let actual = v.dates.act_finish.is_some();
                        cell(ui, format::date_actual(origin, finish, actual));
                    }
                    (34 | 35, _) => {
                        let planned = if c == 34 {
                            v.dates.target_start
                        } else {
                            v.dates.target_finish
                        };
                        cell(ui, format::date_actual(origin, planned, false));
                    }
                    (36, _) => {
                        let mut on = v.cost_linked;
                        if ui.add(egui::Checkbox::without_text(&mut on)).changed() {
                            // Unticking needs no answer; ticking it again
                            // recalculates the costs, so it asks first.
                            if on {
                                relink = Some(row);
                            } else {
                                edits.push((row, AssignmentEdit::CostUnitsLinked(on)));
                            }
                        }
                    }
                    // Actual this period: no editor before the start, as
                    // observed.
                    (37, true) if started => {
                        let value = v.this_period_units;
                        if let Some(u) = value_editor(ui, value, 1., format::units, parse_units) {
                            edits.push((row, AssignmentEdit::ThisPeriodUnits(u)));
                        }
                    }
                    (37, _) => cell(ui, format::units(v.this_period_units)),
                    (38, true) if started => {
                        let value = v.this_period_cost;
                        if let Some(x) = value_editor(ui, value, 1., format::cost, parse_money) {
                            edits.push((row, AssignmentEdit::ThisPeriodCost(x)));
                        }
                    }
                    (38, _) => cell(ui, format::cost(v.this_period_cost)),
                    (15, sel) => {
                        let current = &curve_names[r];
                        if sel && !curves.is_empty() {
                            editor_cell(ui);
                            let picker = egui::ComboBox::from_id_salt(("assignment_curve", row))
                                .selected_text(current.as_str())
                                .width(ui.available_width())
                                .show_ui(ui, |ui| {
                                    if ui.selectable_label(current.is_empty(), "").clicked()
                                        && !current.is_empty()
                                    {
                                        curve_edit = Some((row, None));
                                    }
                                    for (n, points, _) in &curves {
                                        let on = current == n;
                                        let item = ui
                                            .selectable_label(on, n)
                                            .on_hover_ui(|ui| curve_preview(ui, points));
                                        if item.clicked() && !on {
                                            curve_edit = Some((row, Some(n.clone())));
                                        }
                                    }
                                });
                            if !curve_applies {
                                picker.response.on_hover_text(CURVE_IGNORED);
                            }
                        } else {
                            cell(ui, current.clone());
                        }
                    }
                    _ => {}
                }
            },
        );
        if let Some(r) = out.clicked {
            self.detail.rows[0] = Some(r);
        }
        self.action_rects.extend(cell_rects);
        self.action_rects.extend(date_cells);
        let mut changed = false;
        if customize {
            self.open_column_customize(columns);
        }
        if let Some(e) = date_error {
            self.status = e;
        }
        for (row, edit) in edits.into_iter().chain(date_edits) {
            match self.project.edit_assignment(row, edit) {
                Ok(()) => changed = true,
                Err(e) => self.status = e,
            }
        }
        if let Some((row, on)) = primary {
            match self.project.set_primary_resource(row, on) {
                Ok(()) => changed = true,
                Err(e) => self.status = e,
            }
        }
        // Typed period values go only after a question (`removals`).
        let manual = curve_edit.as_ref().is_some_and(|(row, _)| {
            openxer_xer::manual_units::is_manual(&self.project.document, *row)
        });
        if let Some((row, curve)) = curve_edit.clone().filter(|_| manual) {
            self.detail.removal = Some(super::removals::Removal::ManualCurve {
                row,
                task_id,
                curve,
            });
        } else if let Some((row, curve)) = curve_edit {
            match openxer_xer::edit::curves::set_assignment_curve(
                &mut self.project.document,
                row,
                curve.as_deref(),
            ) {
                Ok(c) => changed |= c,
                Err(e) => self.status = e,
            }
        }
        if changed {
            self.touch_records();
        }
        if let Some((row, rsrc)) = replace {
            self.replace_assignment_resource(row, rsrc);
        }
        if let Some(row) = relink {
            self.detail.removal = Some(super::removals::Removal::RelinkCosts { row });
        }
        self.detail.all_resource_columns = all_columns;

        let selected_view = selected.map(|r| rows[r].clone());
        form::button_row(ui, |ui| {
            // Always offered on a task: a project with no resources yet
            // gets its first one from New in the Assign Resources window.
            let add = form::add_button(ui, "Add Resource", !milestone);
            self.action("detail_rsrc_add", &add);
            let add = add.on_disabled_hover_text("Milestones have no resource assignments.");
            if add.clicked() {
                self.detail.new_resource = None;
                self.detail.replace_from = None;
                // The selected assignment's resource opens highlighted, as
                // observed in the reference.
                self.detail.assign = Some(AssignPicker::Resource {
                    query: String::new(),
                    selected: selected_view.as_ref().and_then(|v| v.rsrc_id),
                });
            }
            // With no roles in the file the Assign Roles picker opens empty.
            let role = form::add_button(ui, "Add Role", !milestone);
            self.action("detail_rsrc_add_role", &role);
            let role = role.on_disabled_hover_text("Milestones have no resource assignments.");
            if role.clicked() {
                self.detail.assign = Some(AssignPicker::Role { selected: None });
            }
            let staff = selected_view
                .as_ref()
                .and_then(|v| v.role_id.map(|r| (v.row, r)));
            let by_role = form::command_button(ui, "Assign by Role", None, staff.is_some());
            self.action("detail_rsrc_by_role", &by_role);
            if by_role
                .on_disabled_hover_text("Select an assignment that has a role.")
                .clicked()
            {
                if let Some((row, role)) = staff {
                    self.detail.assign = Some(AssignPicker::ByRole {
                        row,
                        role,
                        selected: None,
                    });
                }
            }
            let remove = form::command_button(ui, "Remove", Some(101.), selected_view.is_some());
            self.action("detail_rsrc_remove", &remove);
            // Remove asks first (`details::removals`).
            if remove.clicked() {
                if let Some(v) = &selected_view {
                    self.detail.removal = Some(super::removals::Removal::Assignment {
                        row: v.row,
                        task_id,
                        rsrc_id: v.rsrc_id,
                        primary: v.primary,
                    });
                }
            }
        });
    }

    /// Assign `rsrc` to an existing assignment (Resource column, Assign by
    /// Role) with the User Preferences staffing and price choices; "Ask"
    /// opens the question window first.
    pub(crate) fn replace_assignment_resource(&mut self, row: usize, rsrc: i64) {
        let Some(view) = self.project.assignment_view(row) else {
            return;
        };
        let calc = self.detail.calc;
        let staffing = match calc.staffing {
            Staffing::New => Some(true),
            Staffing::Current => Some(false),
            Staffing::Ask => None,
        };
        let role_price = match (view.role_id, calc.price) {
            (None, _) | (_, PriceSource::Resource) => Some(false),
            (_, PriceSource::Role) => Some(true),
            (_, PriceSource::Ask) => None,
        };
        match (staffing, role_price) {
            (Some(new_rate), Some(role)) => self.apply_replacement(row, rsrc, new_rate, role),
            // Only the choices left to "Ask" are asked.
            (staffing, role_price) => {
                self.detail.question = Some(Question {
                    row,
                    rsrc,
                    staffing: staffing.is_none().then_some(true),
                    role_price: role_price.is_none().then_some(false),
                });
            }
        }
    }

    fn apply_replacement(&mut self, row: usize, rsrc: i64, new_rate: bool, role_price: bool) {
        match self
            .project
            .replace_resource(row, rsrc, new_rate, role_price)
        {
            Ok(()) => {
                self.touch_records();
                self.status = "Resource assigned".into();
            }
            Err(e) => self.status = e,
        }
    }

    /// The question asked before a resource replaces another on an
    /// assignment, when User Preferences say "Ask me".
    pub(super) fn assignment_question(&mut self, ctx: &egui::Context) {
        let Some(mut q) = self.detail.question.take() else {
            return;
        };
        let calc = self.detail.calc;
        let mut answer: Option<bool> = None;
        // 372 px wide: one group of two choices per question asked, OK and
        // Cancel in the button column.
        const W: f32 = 372.;
        let groups = usize::from(q.staffing.is_some()) + usize::from(q.role_price.is_some());
        let h = (8. + 66. * groups as f32).max(72.);
        let ((), closed, actions) = crate::dialog_kit::window(
            ctx,
            "assignment_question",
            "Assign Resource",
            Vec2::new(W, h),
            |c| {
                use crate::dialog_kit as kit;
                kit::strip(c, W, h);
                let mut y = 8.;
                for (value, caption, choices) in [
                    (
                        q.staffing.as_mut(),
                        "Units per Time and Overtime factor:",
                        [
                            ("assignment_question_new", "Use the new resource's", true),
                            (
                                "assignment_question_current",
                                "Keep the current assignment's",
                                false,
                            ),
                        ],
                    ),
                    (
                        q.role_price.as_mut(),
                        "Price per Unit:",
                        [
                            (
                                "assignment_question_resource_price",
                                "Use the resource's",
                                false,
                            ),
                            ("assignment_question_role_price", "Use the role's", true),
                        ],
                    ),
                ] {
                    let Some(value) = value else {
                        continue;
                    };
                    kit::text(c, 8., y, caption, true);
                    for (k, (key, label, choice)) in choices.into_iter().enumerate() {
                        let top = y + 18. + 20. * k as f32;
                        if c.radio(key, 16., top, 230., 17., label, *value == choice, true) {
                            *value = choice;
                        }
                    }
                    y += 66.;
                }
                let out = kit::button_column(
                    c,
                    W,
                    &[
                        kit::Entry::ok("assignment_question_ok"),
                        kit::Entry::cancel("assignment_question_cancel"),
                    ],
                );
                answer = out.clicked.map(|key| key == "assignment_question_ok");
            },
        );
        self.record_dialog_actions(actions);
        let open = !closed;
        match answer {
            Some(true) => {
                let new_rate = q.staffing.unwrap_or(calc.staffing == Staffing::New);
                let role = q.role_price.unwrap_or(calc.price == PriceSource::Role);
                self.apply_replacement(q.row, q.rsrc, new_rate, role);
            }
            Some(false) => {}
            None if open && !ctx.input(|i| i.key_pressed(Key::Escape)) => {
                self.detail.question = Some(q);
            }
            None => {}
        }
    }

    /// The Assign Resources / Assign Roles / Assign by Role window.
    pub(super) fn assignment_picker(&mut self, ctx: &egui::Context) {
        let Some(mut picker) = self.detail.assign.take() else {
            self.detail.assign_for = None;
            return;
        };
        // The New Activity wizard picks resources for an activity it has not
        // made yet: the pick goes to the wizard.
        let wizard = self.wizard_picks(crate::activity_wizard::Pick::Resource);
        // Add in the Resource Assignments window names its activity.
        let window = self.detail.assign_for.filter(|_| !wizard);
        let selected_activity = if wizard {
            Some(usize::MAX)
        } else {
            window
                .or(self.selected)
                .filter(|&i| i < self.project.model.activities.len())
        };
        let Some(i) = selected_activity else {
            return;
        };
        let task_id = self
            .project
            .model
            .activities
            .get(i)
            .map_or(0, |a| a.task_id);
        let targets = if wizard || window.is_some() {
            vec![]
        } else {
            self.assign_targets()
        };
        let time = ctx.input(|i| i.time);
        // New (a resource typed in this window) belongs to Assign Resources only.
        let mut new_resource = self
            .detail
            .new_resource
            .take()
            .filter(|_| matches!(picker, AssignPicker::Resource { .. }) && !wizard);
        let mut create = None;
        let doc = &self.project.document;
        let no_resources = doc.table("RSRC").is_none_or(|t| t.rows.is_empty());
        let (title, cols, options): (&str, Vec<(&str, f32)>, Rows) = match &picker {
            AssignPicker::Resource { query, .. } => {
                let q = query.to_lowercase();
                let rows = doc.table("RSRC").map_or(vec![], |t| {
                    t.rows
                        .iter()
                        .map(|r| {
                            (
                                num(t.value(r, "rsrc_id")),
                                vec![
                                    t.value(r, "rsrc_short_name").to_string(),
                                    t.value(r, "rsrc_name").to_string(),
                                    resource_type_label(t.value(r, "rsrc_type")).into(),
                                ],
                            )
                        })
                        .filter(|(id, c)| {
                            *id != 0
                                && (q.is_empty()
                                    || c[0].to_lowercase().contains(&q)
                                    || c[1].to_lowercase().contains(&q))
                        })
                        .collect()
                });
                (
                    "Assign Resources",
                    vec![
                        ("Resource ID", 90.),
                        ("Resource Name", 180.),
                        ("Resource Type", 80.),
                    ],
                    rows,
                )
            }
            AssignPicker::Role { .. } => (
                "Assign Roles",
                vec![("Role ID", 90.), ("Role Name", 260.)],
                dictionary(doc, "ROLES", "role_id", &["role_short_name", "role_name"]),
            ),
            AssignPicker::PickRole { .. } => (
                "Assign Resources by Role",
                vec![("Role ID", 90.), ("Role Name", 260.)],
                dictionary(doc, "ROLES", "role_id", &["role_short_name", "role_name"]),
            ),
            AssignPicker::ByRole { role, .. } | AssignPicker::StaffRole { role, .. } => {
                let names = dictionary(doc, "RSRC", "rsrc_id", &["rsrc_short_name", "rsrc_name"]);
                let rows = doc.table("RSRCROLE").map_or(vec![], |t| {
                    t.rows
                        .iter()
                        .filter(|r| num(t.value(r, "role_id")) == *role)
                        .filter_map(|r| {
                            let id = num(t.value(r, "rsrc_id"));
                            let (_, c) = names.iter().find(|(n, _)| *n == id)?;
                            Some((
                                id,
                                vec![c[0].clone(), c[1].clone(), t.value(r, "skill_level").into()],
                            ))
                        })
                        .collect()
                });
                (
                    "Assign Resources by Role",
                    vec![
                        ("Resource ID", 90.),
                        ("Resource Name", 180.),
                        ("Proficiency", 80.),
                    ],
                    rows,
                )
            }
        };
        const DIALOG: &str = "assign_resources";
        let next = matches!(picker, AssignPicker::PickRole { .. });
        // What New starts the Resource Name from: the search text.
        let typed = match &picker {
            AssignPicker::Resource { query, .. } if !wizard => Some(query.trim().to_string()),
            _ => None,
        };
        let current = assign_choice(&picker);
        // Remove and Replace (Assign Resources): one resource selected that
        // the activity (Remove: any selected activity) already has; Assign
        // is then off for the one activity, as observed in the reference.
        let resources_mode = matches!(picker, AssignPicker::Resource { .. }) && !wizard;
        let picked = self.assign_picked(DIALOG, current);
        let single = match picked.as_slice() {
            [one] if resources_mode => Some(*one),
            _ => None,
        };
        let has = |app: &App, task: i64, rsrc: i64| {
            let mut found = false;
            for r in app.project.assignment_rows(task) {
                let view = app.project.assignment_view(r);
                found |= view.is_some_and(|v| v.rsrc_id == Some(rsrc));
            }
            found
        };
        let target_tasks: Vec<i64> = if targets.is_empty() {
            vec![task_id]
        } else {
            targets
                .iter()
                .map(|&t| self.project.model.activities[t].task_id)
                .collect()
        };
        let one_activity = target_tasks.len() == 1;
        let on_activity = single.is_some_and(|r| one_activity && has(self, task_id, r));
        let on_any = single.is_some_and(|r| target_tasks.iter().any(|&t| has(self, t, r)));
        let replacing = self.detail.replace_from.filter(|_| resources_mode);
        let mut buttons = vec![AssignButton {
            key: "rsrc_picker_assign",
            caption: if next { "Next" } else { "Assign" },
            enabled: current.is_some() && (replacing.is_some() || !on_activity),
        }];
        if resources_mode {
            buttons.push(AssignButton {
                key: "rsrc_picker_remove",
                caption: "Remove",
                enabled: on_any,
            });
            buttons.push(AssignButton {
                key: "rsrc_picker_replace",
                caption: "Replace",
                enabled: on_activity,
            });
        }
        if typed.is_some() {
            buttons.push(AssignButton {
                key: "rsrc_picker_new",
                caption: "New",
                enabled: true,
            });
        }
        let window = AssignWindow {
            id: DIALOG,
            title,
            buttons,
            close: "rsrc_picker_close",
            help: "rsrc_picker_help",
        };
        let keys: Vec<Option<i64>> = options.iter().map(|(id, _)| Some(*id)).collect();
        let mut double = None;
        let names = dictionary(doc, "RSRC", "rsrc_id", &["rsrc_short_name", "rsrc_name"]);
        let replacing_name = replacing.map(|row| {
            let id = self.project.assignment_view(row).and_then(|v| v.rsrc_id);
            id_name(&names, id)
        });
        let (open, clicked) = self.assign_window(ctx, &window, |app, ui| {
            if let AssignPicker::Resource { query, .. } = &mut picker {
                ui.horizontal(|ui| {
                    ui.label("Search");
                    ui.add(egui::TextEdit::singleline(query).desired_width(220.));
                });
            }
            if targets.len() > 1 {
                ui.label(format!(
                    "Assigns to the {} selected activities.",
                    targets.len()
                ));
            }
            if let Some(name) = &replacing_name {
                ui.label(format!(
                    "Select the resource to put in place of {name}, then click Assign."
                ));
            }
            if options.is_empty() {
                ui.label(match picker {
                    AssignPicker::ByRole { .. } | AssignPicker::StaffRole { .. } => {
                        "No resource in this file has the role."
                    }
                    AssignPicker::Resource { .. } if no_resources => {
                        "This project has no resources yet. Click New to add one."
                    }
                    AssignPicker::Resource { .. } => "No resource matches. Click New to add one.",
                    _ => "Nothing to assign.",
                });
            }
            let selected = match &mut picker {
                AssignPicker::Resource { selected, .. }
                | AssignPicker::Role { selected }
                | AssignPicker::PickRole { selected }
                | AssignPicker::StaffRole { selected, .. }
                | AssignPicker::ByRole { selected, .. } => selected,
            };
            let out = app.assign_list(
                ui,
                DIALOG,
                "assign_resources_grid",
                &cols,
                &keys,
                selected,
                Vec2::new(360., 200.),
                &mut |ui, r, c, _| cell(ui, &options[r].1[c]),
            );
            double = out.double;
            ui.add_space(4.);
            if let Some((code, name)) = new_resource.as_mut() {
                ui.horizontal(|ui| {
                    ui.label("Resource ID");
                    let response = ui.add(egui::TextEdit::singleline(code).desired_width(64.));
                    app.action("rsrc_picker_new_id", &response);
                    ui.label("Name");
                    let response = ui.add(egui::TextEdit::singleline(name).desired_width(118.));
                    app.action("rsrc_picker_new_name", &response);
                    let button = crate::chrome::dialog_button(
                        ui,
                        &crate::chrome::DialogButton::new("Add", None)
                            .enabled(!code.trim().is_empty()),
                    );
                    app.action("rsrc_picker_new_add", &button);
                    if button.clicked() {
                        create = Some((code.clone(), name.clone()));
                    }
                });
            }
        });
        if clicked == Some("rsrc_picker_new") {
            if let Some(typed) = &typed {
                // New again puts the typing away.
                new_resource = match new_resource.take() {
                    Some(_) => None,
                    None => Some((String::new(), typed.clone())),
                };
            }
        }
        if let Some((code, name)) = create {
            match self.add_named_resource(&code, &name, time) {
                Ok(id) => {
                    // Show the new resource selected, ready for Assign.
                    if let AssignPicker::Resource { query, selected } = &mut picker {
                        query.clear();
                        *selected = Some(id);
                    }
                    self.detail.assign_rows.remove(DIALOG);
                    new_resource = None;
                    self.status = format!(
                        "Resource {} added. Set its price and limit in the Resources window.",
                        code.trim()
                    );
                }
                Err(e) => self.status = e,
            }
        }
        // Remove: the selected resource from every selected activity that
        // has it, after the question Remove on the Resources tab asks.
        if let (Some("rsrc_picker_remove"), Some(rsrc_id)) = (clicked, single) {
            self.detail.removal = Some(super::removals::Removal::ResourceFrom {
                rsrc_id,
                task_ids: target_tasks.clone(),
            });
        }
        // Replace: the next Assign puts the picked resource in place of the
        // selected one on this activity.
        if let (Some("rsrc_picker_replace"), Some(rsrc_id)) = (clicked, single) {
            let mut row = None;
            for r in self.project.assignment_rows(task_id) {
                let view = self.project.assignment_view(r);
                if view.is_some_and(|v| v.rsrc_id == Some(rsrc_id)) {
                    row = Some(r);
                }
            }
            self.detail.replace_from = row;
            self.status = "Select the resource to put in its place, then click Assign.".into();
        }
        // Assign takes every selected choice; a double click the one.
        let mut picks: Vec<i64> = match clicked {
            Some("rsrc_picker_assign") => self.assign_picked(DIALOG, assign_choice(&picker)),
            _ => vec![],
        };
        if let Some(id) = double {
            picks = vec![id];
        }
        let one_pick = match picks.as_slice() {
            [id] => Some(*id),
            _ => None,
        };
        if let (Some(row), Some(id)) = (replacing, one_pick) {
            self.detail.replace_from = None;
            if self.project.assignment_view(row).and_then(|v| v.rsrc_id) != Some(id) {
                self.replace_assignment_resource(row, id);
            }
            picks.clear();
        }
        if let (Some(&id), AssignPicker::PickRole { .. }) = (picks.first(), &picker) {
            picker = AssignPicker::StaffRole {
                role: id,
                selected: None,
            };
            self.detail.assign_rows.remove(DIALOG);
            picks.clear();
        }
        if wizard {
            for id in picks.drain(..) {
                self.wizard_take_resource(id);
            }
        }
        let pairs: Vec<(Option<i64>, Option<i64>)> = picks
            .iter()
            .filter_map(|&id| match &picker {
                AssignPicker::Resource { .. } => Some((Some(id), None)),
                AssignPicker::Role { .. } => Some((None, Some(id))),
                AssignPicker::StaffRole { role, .. } => Some((Some(id), Some(*role))),
                _ => None,
            })
            .collect();
        if !pairs.is_empty() && (targets.len() > 1 || pairs.len() > 1) {
            let targets = if targets.is_empty() { vec![i] } else { targets };
            self.assign_resources_bulk(&targets, &pairs, time);
            picks.clear();
        }
        if let Some(&id) = picks.first() {
            let result = match &picker {
                AssignPicker::Resource { .. } => self.project.add_assignment_with(
                    task_id,
                    Some(id),
                    None,
                    self.detail.calc.recalculate,
                ),
                AssignPicker::Role { .. } => self.project.add_assignment_with(
                    task_id,
                    None,
                    Some(id),
                    self.detail.calc.recalculate,
                ),
                AssignPicker::StaffRole { role, .. } => self.project.add_assignment_with(
                    task_id,
                    Some(id),
                    Some(*role),
                    self.detail.calc.recalculate,
                ),
                AssignPicker::PickRole { .. } => return,
                AssignPicker::ByRole { row, .. } => {
                    let row = *row;
                    self.replace_assignment_resource(row, id);
                    self.detail.assign = None;
                    self.detail.assign_rows.remove(DIALOG);
                    return;
                }
            };
            match result {
                Ok(row) => {
                    let rows = self.project.assignment_rows(task_id);
                    self.detail.rows[0] = rows.iter().position(|&r| r == row);
                    self.touch_records();
                    self.status = "Resource assignment added".into();
                }
                Err(e) => self.status = e,
            }
        }
        if open {
            self.detail.assign = Some(picker);
            self.detail.new_resource = new_resource;
        }
    }
}

/// The current choice of an Assign Resources, Roles or by Role window.
fn assign_choice(picker: &AssignPicker) -> Option<i64> {
    match picker {
        AssignPicker::Resource { selected, .. }
        | AssignPicker::Role { selected }
        | AssignPicker::PickRole { selected }
        | AssignPicker::StaffRole { selected, .. }
        | AssignPicker::ByRole { selected, .. } => *selected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn units_and_units_per_time_parse_in_the_user_format() {
        format::install(&format::Formats::default());
        assert_eq!(parse_units("8"), Some(8.));
        assert_eq!(parse_units("1d"), Some(8.));
        assert_eq!(parse_units_per_time("8h/d"), Some(1.));
        assert_eq!(parse_units_per_time("4"), Some(0.5));
        assert_eq!(parse_units_per_time("150%"), Some(1.5));
        assert_eq!(parse_units_per_time("x"), None);
    }
}
