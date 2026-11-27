//! Project > Resource Assignments: every assignment of the open project in one
//! grid, with a General form for the selected one (activity, resource, role,
//! primary resource, budgeted and remaining units and units/time, actual
//! units, original and remaining duration, price, rate type, cost account).
//! Edits use the same assignment rules as the Resources tab.
use super::assignments::{parse_units, parse_units_per_time};
use super::*;
use crate::chrome::DialogButton;
use openxer_xer::assignments::{AssignmentEdit, AssignmentView, RATE_TYPES};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AssignmentWindow {
    /// Selected TASKRSRC row.
    pub selected: Option<usize>,
    /// Frames shown; the grid waits for the window's sizing frame to pass so
    /// its remembered column widths come from the real width.
    pub frames: u32,
    /// Add in progress: the activity and resource chosen so far.
    pub adding: Option<(Option<i64>, Option<i64>)>,
    /// Delete (the button or the Delete key) on this TASKRSRC row: the
    /// question is up.
    pub confirm_delete: Option<usize>,
}

const COLUMNS: [(&str, f32); 11] = [
    ("Activity ID", 70.),
    ("Activity Name", 150.),
    ("Resource ID Name", 130.),
    ("Role", 80.),
    ("Budgeted Units", 85.),
    ("Remaining Units", 95.),
    ("Remaining Units / Time", 120.),
    ("Actual Start", 95.),
    ("Actual Finish", 95.),
    ("Remaining Start", 105.),
    ("Remaining Finish", 110.),
];

fn lookup_name(
    doc: &openxer_xer::Document,
    table: &str,
    key: &str,
    id: Option<i64>,
    columns: [&str; 2],
) -> String {
    let Some(id) = id else {
        return String::new();
    };
    let short = records::lookup(doc, table, key, id, columns[0]);
    let name = records::lookup(doc, table, key, id, columns[1]);
    match (short.is_empty(), name.is_empty()) {
        (true, true) => id.to_string(),
        (true, false) => name,
        (false, true) => short,
        (false, false) => format!("{short}  {name}"),
    }
}

/// A read-only value box of a fixed width (grid cells would squeeze a text edit).
fn field(ui: &mut egui::Ui, text: &str, width: f32) {
    let mut text = text.to_string();
    ui.add_enabled_ui(false, |ui| {
        ui.add_sized([width, 18.], egui::TextEdit::singleline(&mut text));
    });
}

impl App {
    pub(crate) fn open_assignment_window(&mut self) {
        self.detail.assignment_window = Some(AssignmentWindow::default());
    }

    pub(super) fn assignment_window(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.detail.assignment_window.take() else {
            return;
        };
        let origin = self.project.model.origin_serial;
        let index: BTreeMap<i64, usize> = self
            .project
            .model
            .activities
            .iter()
            .enumerate()
            .map(|(i, a)| (a.task_id, i))
            .collect();
        let mut views: Vec<(usize, AssignmentView)> = vec![];
        for (&task_id, &i) in &index {
            for row in self.project.assignment_rows(task_id) {
                if let Some(v) = self.project.assignment_view(row) {
                    views.push((i, v));
                }
            }
        }
        views.sort_by(|a, b| {
            let code = |i: usize| &self.project.model.activities[i].task_code;
            code(a.0).cmp(code(b.0)).then(a.1.row.cmp(&b.1.row))
        });
        let doc = &self.project.document;
        let cells: Vec<[String; 11]> = views
            .iter()
            .map(|(i, v)| {
                let a = &self.project.model.activities[*i];
                let d = v.dates;
                [
                    a.task_code.clone(),
                    self.project.name(*i).to_string(),
                    lookup_name(
                        doc,
                        "RSRC",
                        "rsrc_id",
                        v.rsrc_id,
                        ["rsrc_short_name", "rsrc_name"],
                    ),
                    lookup_name(
                        doc,
                        "ROLES",
                        "role_id",
                        v.role_id,
                        ["role_short_name", "role_name"],
                    ),
                    format::units(v.budgeted.units),
                    format::units(v.remaining.units),
                    format::units_per_time(v.remaining.per_hour),
                    format::date_actual(origin, d.act_start, true),
                    format::date_actual(origin, d.act_finish, true),
                    format::date(origin, d.rem_start),
                    format::date(origin, d.rem_finish),
                ]
            })
            .collect();
        let current = state
            .selected
            .and_then(|row| views.iter().position(|(_, v)| v.row == row));
        let mut open = true;
        let mut close = false;
        let mut remove = None;
        let mut add: Option<(i64, i64)> = None;
        let mut edits: Vec<(usize, AssignmentEdit)> = vec![];
        let mut primary = None;
        let mut date_error: Option<String> = None;
        crate::chrome::plain_window("Resource Assignments")
            .id(egui::Id::new("resource_assignments_window"))
            .open(&mut open)
            .collapsible(false)
            .default_size([1180., 520.])
            .min_width(900.)
            .show(ctx, |ui| {
                let size = Vec2::new(ui.available_width().max(880.), 220.);
                let out = if state.frames == 0 {
                    ui.allocate_space(size);
                    GridOut::default()
                } else {
                    grid(
                        ui,
                        "resource_assignments_grid",
                        &COLUMNS,
                        cells.len(),
                        current,
                        size,
                        &mut |ui, ev, _| {
                            if let Ev::Cell(r, c) = ev {
                                cell(ui, &cells[r][c]);
                            }
                        },
                    )
                };
                if let Some(r) = out.clicked {
                    state.selected = Some(views[r].1.row);
                }
                ui.add_space(6.);
                ui.label(RichText::new("General").font(chrome::bold_font()));
                if let Some(r) = current {
                    let (i, v) = (views[r].0, views[r].1.clone());
                    let c = &cells[r];
                    let periods = format::activity_periods(&self.project, i);
                    let completed = self.project.model.activities[i].is_completed();
                    let row = v.row;
                    egui::Grid::new("assignment_general")
                        .num_columns(4)
                        .spacing([10., 3.])
                        .show(ui, |ui| {
                            ui.label("Activity");
                            field(ui, &format!("{}  {}", c[0], c[1]), 260.);
                            ui.label("Resource");
                            field(ui, &c[2], 200.);
                            ui.end_row();
                            ui.label("Role");
                            field(ui, &c[3], 260.);
                            let mut on = v.primary;
                            if crate::dialog_kit::flow_checkbox(
                                ui,
                                &mut on,
                                "Primary Resource",
                                v.rsrc_id.is_some(),
                            )
                            .changed()
                            {
                                primary = Some((row, on));
                            }
                            ui.end_row();
                            let number = |ui: &mut egui::Ui,
                                          value: f64,
                                          _speed: f64,
                                          show: fn(f64) -> String,
                                          parse: fn(&str) -> Option<f64>,
                                          enabled: bool| {
                                crate::dialog_kit::flow_parsed_number(
                                    ui,
                                    ui.next_auto_id(),
                                    value,
                                    110.,
                                    show,
                                    move |t| parse(t).map(|v| v.clamp(0.0, 1.0e9)),
                                    enabled,
                                )
                            };
                            ui.label("Budgeted Units");
                            if let Some(x) =
                                number(ui, v.budgeted.units, 1., format::units, parse_units, true)
                            {
                                edits.push((row, AssignmentEdit::BudgetedUnits(x)));
                            }
                            ui.label("Budgeted Units / Time");
                            if let Some(x) = number(
                                ui,
                                v.budgeted.per_hour,
                                0.01,
                                format::units_per_time,
                                parse_units_per_time,
                                true,
                            ) {
                                edits.push((row, AssignmentEdit::BudgetedUnitsPerTime(x)));
                            }
                            ui.end_row();
                            ui.label("Remaining Units");
                            if let Some(x) = number(
                                ui,
                                v.remaining.units,
                                1.,
                                format::units,
                                parse_units,
                                !completed,
                            ) {
                                edits.push((row, AssignmentEdit::RemainingUnits(x)));
                            }
                            ui.label("Remaining Units / Time");
                            if let Some(x) = number(
                                ui,
                                v.remaining.per_hour,
                                0.01,
                                format::units_per_time,
                                parse_units_per_time,
                                !completed,
                            ) {
                                edits.push((row, AssignmentEdit::RemainingUnitsPerTime(x)));
                            }
                            ui.end_row();
                            ui.label("Actual Units");
                            let started =
                                self.project.model.activities[i].status != StatusType::NotStarted;
                            if let Some(x) =
                                number(ui, v.actual_units, 1., format::units, parse_units, started)
                            {
                                let regular = (x - v.actual_overtime_units).max(0.);
                                edits.push((row, AssignmentEdit::ActualRegularUnits(regular)));
                            }
                            ui.label("Price / Unit");
                            if let Some(x) = number(
                                ui,
                                v.price,
                                1.,
                                format::cost,
                                |t| {
                                    t.trim()
                                        .trim_start_matches('$')
                                        .replace(',', "")
                                        .parse()
                                        .ok()
                                },
                                true,
                            ) {
                                edits.push((row, AssignmentEdit::PricePerUnit(x)));
                            }
                            ui.end_row();
                            ui.label("Original Duration");
                            let mut m = v.original_duration;
                            if format::duration_editor(ui, true, &mut m, periods, 0..=100000 * 60)
                                .changed()
                                && m != v.original_duration
                            {
                                edits.push((row, AssignmentEdit::OriginalDuration(m)));
                            }
                            ui.label("Rate Type");
                            let label = RATE_TYPES
                                .iter()
                                .find(|(c, _, _)| *c == v.rate_type)
                                .map_or(v.rate_type.as_str(), |(_, l, _)| l);
                            crate::dialog_kit::flow_combo_with(
                                ui,
                                "assignment_window_rate_type",
                                120.,
                                label,
                                true,
                                |ui| {
                                    for (code, label, _) in RATE_TYPES {
                                        let on = v.rate_type == code;
                                        if crate::dialog_kit::flow_selectable(ui, on, label, 116.)
                                            .clicked()
                                        {
                                            if !on {
                                                edits.push((
                                                    row,
                                                    AssignmentEdit::RateType(code.into()),
                                                ));
                                            }
                                            ui.close();
                                        }
                                    }
                                },
                            );
                            ui.end_row();
                            ui.label("Remaining Duration");
                            let mut m = v.remaining_duration;
                            if format::duration_editor(
                                ui,
                                !completed,
                                &mut m,
                                periods,
                                0..=100000 * 60,
                            )
                            .changed()
                                && m != v.remaining_duration
                            {
                                edits.push((row, AssignmentEdit::RemainingDuration(m)));
                            }
                            ui.label("Cost Account");
                            field(
                                ui,
                                &lookup_name(
                                    &self.project.document,
                                    "ACCOUNT",
                                    "acct_id",
                                    v.acct_id,
                                    ["acct_short_name", "acct_name"],
                                ),
                                120.,
                            );
                            ui.end_row();
                            ui.label("Lag");
                            let mut m = v.lag;
                            let not_started =
                                self.project.model.activities[i].status == StatusType::NotStarted;
                            if format::duration_editor(
                                ui,
                                not_started,
                                &mut m,
                                periods,
                                0..=100000 * 60,
                            )
                            .changed()
                                && m != v.lag
                            {
                                edits.push((row, AssignmentEdit::Lag(m)));
                            }
                            ui.label("Overtime Factor");
                            if let Some(x) = number(
                                ui,
                                v.ot_factor,
                                0.1,
                                crate::records::format_number,
                                |t| t.trim().parse().ok(),
                                true,
                            ) {
                                edits.push((row, AssignmentEdit::OvertimeFactor(x)));
                            }
                            ui.end_row();
                            // Actual dates need a started activity; the remaining
                            // start is the lag of a not started activity's
                            // assignment; the remaining finish its duration.
                            let act = &self.project.model.activities[i];
                            let is_started = act.status != StatusType::NotStarted;
                            let is_not_started = !is_started && !completed;
                            let dates = [
                                ("Actual Start", v.dates.act_start, is_started),
                                ("Actual Finish", v.dates.act_finish, is_started),
                                ("Remaining Start", v.dates.rem_start, is_not_started),
                                ("Remaining Finish", v.dates.rem_finish, !completed),
                            ];
                            for (k, (caption, shown, enabled)) in dates.into_iter().enumerate() {
                                ui.label(caption);
                                let mut value = shown;
                                let key = format!("assignment_window-{row}-{k}");
                                let at = ui.cursor().min;
                                self.action_rects.insert(
                                    format!("assignment_window_date_{k}"),
                                    [at.x, at.y, at.x + 104., at.y + 20.],
                                );
                                match self.date_edit(ui, &key, &mut value, enabled) {
                                    Ok(true) => match value {
                                        Some(t) => edits.push((
                                            row,
                                            [
                                                AssignmentEdit::ActualStart(t),
                                                AssignmentEdit::ActualFinish(t),
                                                AssignmentEdit::RemainingStart(t),
                                                AssignmentEdit::RemainingFinish(t),
                                            ][k]
                                                .clone(),
                                        )),
                                        None => date_error = Some("Enter a date".into()),
                                    },
                                    Ok(false) => {}
                                    Err(e) => date_error = Some(e),
                                }
                                if k % 2 == 1 {
                                    ui.end_row();
                                }
                            }
                        });
                } else {
                    ui.label(RichText::new("Select an assignment.").color(Color32::GRAY));
                }
                ui.add_space(6.);
                if let Some((task, rsrc)) = state.adding.as_mut() {
                    ui.horizontal(|ui| {
                        ui.label("Activity");
                        let name = |t: Option<i64>| {
                            t.and_then(|t| index.get(&t)).map_or(String::new(), |&i| {
                                format!(
                                    "{}  {}",
                                    self.project.model.activities[i].task_code,
                                    self.project.name(i)
                                )
                            })
                        };
                        crate::dialog_kit::flow_combo_with(
                            ui,
                            "assignment_window_add_activity",
                            260.,
                            &name(*task),
                            true,
                            |ui| {
                                for a in self.project.model.activities.iter().filter(|a| {
                                    !matches!(a.task_type, TaskType::Mile | TaskType::FinMile)
                                }) {
                                    let text = name(Some(a.task_id));
                                    if crate::dialog_kit::flow_selectable(
                                        ui,
                                        *task == Some(a.task_id),
                                        &text,
                                        256.,
                                    )
                                    .clicked()
                                    {
                                        *task = Some(a.task_id);
                                        ui.close();
                                    }
                                }
                            },
                        );
                        ui.label("Resource");
                        let resources: Vec<(i64, String)> =
                            self.project.document.table("RSRC").map_or(vec![], |t| {
                                t.rows
                                    .iter()
                                    .map(|r| {
                                        (
                                            records::num(t.value(r, "rsrc_id")),
                                            format!(
                                                "{}  {}",
                                                t.value(r, "rsrc_short_name"),
                                                t.value(r, "rsrc_name")
                                            ),
                                        )
                                    })
                                    .collect()
                            });
                        let shown = rsrc
                            .and_then(|id| resources.iter().find(|(r, _)| *r == id))
                            .map_or(String::new(), |(_, n)| n.clone());
                        crate::dialog_kit::flow_combo_with(
                            ui,
                            "assignment_window_add_resource",
                            200.,
                            &shown,
                            true,
                            |ui| {
                                for (id, text) in &resources {
                                    if crate::dialog_kit::flow_selectable(
                                        ui,
                                        *rsrc == Some(*id),
                                        text,
                                        196.,
                                    )
                                    .clicked()
                                    {
                                        *rsrc = Some(*id);
                                        ui.close();
                                    }
                                }
                            },
                        );
                    });
                }
                ui.add_space(6.);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = crate::dialog_kit::ROW_GAP;
                    let button = chrome::dialog_button(ui, &DialogButton::new("Close", None));
                    self.action("assignment_window_close", &button);
                    if button.clicked() {
                        close = true;
                    }
                    let button = chrome::dialog_button(
                        ui,
                        &DialogButton::new("Delete", None).enabled(current.is_some()),
                    );
                    self.action("assignment_window_delete", &button);
                    if button.clicked() {
                        state.confirm_delete = state.selected;
                    }
                    match state.adding {
                        Some((task, rsrc)) => {
                            let button =
                                chrome::dialog_button(ui, &DialogButton::new("Cancel Add", None));
                            self.action("assignment_window_add_cancel", &button);
                            if button.clicked() {
                                state.adding = None;
                            }
                            let ready = task.is_some() && rsrc.is_some();
                            let button = chrome::dialog_button(
                                ui,
                                &DialogButton::new("Assign", None).enabled(ready),
                            );
                            self.action("assignment_window_assign", &button);
                            if button.clicked() {
                                add = task.zip(rsrc);
                            }
                        }
                        None => {
                            let button = chrome::dialog_button(ui, &DialogButton::new("Add", None));
                            self.action("assignment_window_add", &button);
                            if button.clicked() {
                                let task = current
                                    .map(|r| self.project.model.activities[views[r].0].task_id)
                                    .or_else(|| {
                                        self.selected
                                            .and_then(|i| self.project.model.activities.get(i))
                                            .map(|a| a.task_id)
                                    });
                                state.adding = Some((task, None));
                            }
                        }
                    }
                });
            });
        let mut changed = false;
        if let Some(e) = date_error {
            self.status = e;
        }
        for (row, edit) in edits {
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
        if let Some((task, rsrc)) = add {
            match self.project.add_assignment_with(
                task,
                Some(rsrc),
                None,
                self.detail.calc.recalculate,
            ) {
                Ok(row) => {
                    state.selected = Some(row);
                    state.adding = None;
                    changed = true;
                    self.status = "Resource assignment added".into();
                }
                Err(e) => self.status = e,
            }
        }
        // Delete asks first, as observed in the reference (No is the
        // default). The Delete key asks the same.
        let asking = state.confirm_delete.is_some();
        if !asking
            && current.is_some()
            && !ctx.wants_keyboard_input()
            && ctx.input(|i| i.key_pressed(Key::Delete))
        {
            state.confirm_delete = state.selected;
        }
        if state.confirm_delete.is_some() {
            let buttons = ["Yes", "No"];
            let mut rects = vec![];
            let answer = crate::kit_window::message_box(
                ctx,
                "delete_resource_assignment",
                "Confirmation",
                crate::kit_window::MessageIcon::Question,
                "Delete the selected resource assignments?",
                &buttons,
                crate::kit_window::Question::DeleteResourceAssignment.default_in(&buttons),
                &mut rects,
            );
            self.record_dialog_actions(rects);
            match answer {
                Some(0) => remove = state.confirm_delete.take(),
                Some(_) => state.confirm_delete = None,
                None if asking && ctx.input(|i| i.key_pressed(Key::Escape)) => {
                    state.confirm_delete = None;
                }
                None => {}
            }
        }
        if let Some(row) = remove {
            match self
                .project
                .remove_assignment_with(row, self.detail.calc.recalculate)
            {
                Ok(()) => {
                    state.selected = None;
                    changed = true;
                }
                Err(e) => self.status = e,
            }
        }
        if changed {
            self.touch_records();
        }
        state.frames += 1;
        let escape = !asking && ctx.input(|i| i.key_pressed(Key::Escape));
        if open && !close && !escape {
            self.detail.assignment_window = Some(state);
        }
    }
}
