//! The Codes tab of the Resource Assignments view: the assignment code
//! values of the selected assignment (`ASGNMNTACAT` rows), Assign through a
//! picker of the Assignment Codes dictionary and Remove. Each change is one
//! undo step and travels with the project's XER file.
use super::assignments::Assignment;
use super::sheet;
use super::*;
use crate::dictionaries::{ops, CodeKind};
use crate::records::{self, num};
use crate::schedule_dialogs::Canvas;

const TABLE: &str = "ASGNMNTACAT";

/// The Assign picker while open: the assignment, the chosen code type and value.
#[derive(Clone, Debug, PartialEq)]
pub struct AssignPick {
    pub assignment: i64,
    pub type_id: Option<i64>,
    pub value: Option<i64>,
}

/// One assigned code: (row in `ASGNMNTACAT`, code type id, [code, value, description]).
pub fn assigned(p: &ProjectData, assignment: i64) -> Vec<(usize, i64, [String; 3])> {
    let doc = &p.document;
    let Some(t) = doc.table(TABLE) else {
        return vec![];
    };
    let kind = CodeKind::Assignment.tables();
    t.rows
        .iter()
        .enumerate()
        .filter(|(_, r)| num(t.value(r, "taskrsrc_id")) == assignment)
        .map(|(i, r)| {
            let type_id = num(t.value(r, kind.type_id));
            let value = num(t.value(r, kind.assign_value));
            let name = ops::code_type(doc, CodeKind::Assignment, type_id)
                .map(|c| c.name)
                .unwrap_or_default();
            let v = ops::code_values(doc, CodeKind::Assignment, type_id)
                .into_iter()
                .find(|v| v.id == value);
            (
                i,
                type_id,
                [
                    name,
                    v.as_ref().map(|v| v.short.clone()).unwrap_or_default(),
                    v.map(|v| v.name).unwrap_or_default(),
                ],
            )
        })
        .collect()
}

/// Give an assignment a code value, replacing its value of the same code.
pub fn assign(p: &mut ProjectData, assignment: i64, type_id: i64, value: i64) {
    let pid = p.model.project.proj_id;
    let doc = &mut p.document;
    let same: Vec<usize> = doc.table(TABLE).map_or(vec![], |t| {
        t.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                num(t.value(r, "taskrsrc_id")) == assignment
                    && num(t.value(r, "asgnmnt_catg_type_id")) == type_id
            })
            .map(|(i, _)| i)
            .collect()
    });
    for i in same.into_iter().rev() {
        records::remove_row(doc, TABLE, i);
    }
    records::add_row(
        doc,
        TABLE,
        &[
            ("taskrsrc_id", assignment.to_string()),
            ("asgnmnt_catg_type_id", type_id.to_string()),
            ("asgnmnt_catg_id", value.to_string()),
            ("proj_id", pid.to_string()),
        ],
    );
}

impl App {
    /// The Codes tab body.
    pub(super) fn assignment_codes_tab(
        &mut self,
        c: &mut Canvas,
        w: f32,
        h: f32,
        a: Option<&Assignment>,
    ) {
        let cols = [
            ("Assignment Code", 120., false),
            ("Code Value", 100., false),
            ("Code Description", 200., false),
        ];
        let list = a.map_or(vec![], |a| assigned(&self.project, a.id));
        let rows: Vec<Vec<String>> = list.iter().map(|(_, _, cells)| cells.to_vec()).collect();
        let selected = self
            .dictionaries
            .assignment_code_row
            .filter(|&r| r < rows.len());
        if let Some(r) = sheet::table(
            c,
            "assignment_codes",
            6.,
            6.,
            w - 12.,
            h - 38.,
            &cols,
            &rows,
            selected,
        ) {
            self.dictionaries.assignment_code_row = Some(r);
        }
        let has_types = !ops::code_types(
            &self.project.document,
            CodeKind::Assignment,
            ops::Scope::Global,
            0,
        )
        .is_empty();
        match sheet::buttons(
            c,
            "assignment_codes_buttons",
            5.,
            h - 26.,
            &[
                ("Assign", a.is_some() && has_types),
                ("Remove", a.is_some() && selected.is_some()),
            ],
        ) {
            Some(0) => {
                if let Some(a) = a {
                    self.dictionaries.assignment_pick = Some(AssignPick {
                        assignment: a.id,
                        type_id: None,
                        value: None,
                    });
                }
            }
            Some(1) => {
                if let Some(&(row, _, _)) = selected.and_then(|r| list.get(r)) {
                    records::remove_row(&mut self.project.document, TABLE, row);
                    self.dictionaries.assignment_code_row = None;
                    self.touch_records();
                }
            }
            _ => {}
        }
        if !has_types {
            let r = c.r(5., h - 26., 180., 21.);
            sheet::not_in_xer(
                c,
                "assignment_codes_tip",
                r,
                "Define assignment codes in Enterprise > Assignment Codes first.",
            );
        }
        let ctx = c.ui.ctx().clone();
        self.assignment_code_pick_window(&ctx);
    }

    /// Assign Assignment Codes: a code, then one of its values.
    fn assignment_code_pick_window(&mut self, ctx: &egui::Context) {
        let Some(mut pick) = self.dictionaries.assignment_pick.take() else {
            return;
        };
        let doc = &self.project.document;
        let types = ops::code_types(doc, CodeKind::Assignment, ops::Scope::Global, 0);
        if pick
            .type_id
            .is_none_or(|id| !types.iter().any(|t| t.id == id))
        {
            pick.type_id = types.first().map(|t| t.id);
            pick.value = None;
        }
        let values = pick
            .type_id
            .map_or(vec![], |id| ops::code_values(doc, CodeKind::Assignment, id));
        let mut assign_now = false;
        let mut close = false;
        crate::chrome::plain_window("Assign Assignment Codes")
            .id(egui::Id::new("assign_assignment_codes"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                let current = types
                    .iter()
                    .find(|t| Some(t.id) == pick.type_id)
                    .map_or(String::new(), |t| t.name.clone());
                crate::dialog_kit::flow_combo_with(
                    ui,
                    "assignment_code_type",
                    220.,
                    &current,
                    true,
                    |ui| {
                        for t in &types {
                            if crate::dialog_kit::flow_selectable(
                                ui,
                                pick.type_id == Some(t.id),
                                &t.name,
                                216.,
                            )
                            .clicked()
                            {
                                pick.type_id = Some(t.id);
                                pick.value = None;
                                ui.close();
                            }
                        }
                    },
                );
                egui::ScrollArea::vertical()
                    .max_height(180.)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (n, v) in values.iter().enumerate() {
                            let text = format!("{}{}  {}", "  ".repeat(v.depth), v.short, v.name);
                            let r = crate::dialog_kit::flow_selectable(
                                ui,
                                pick.value == Some(v.id),
                                &text,
                                240.,
                            );
                            self.action(&format!("assignment_pick_value_{n}"), &r);
                            if r.clicked() {
                                pick.value = Some(v.id);
                            }
                            if r.double_clicked() {
                                pick.value = Some(v.id);
                                assign_now = true;
                            }
                        }
                    });
                ui.add_space(6.);
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.x = crate::dialog_kit::ROW_GAP;
                    let r = crate::chrome::dialog_button(
                        ui,
                        &crate::chrome::DialogButton::new("Close", None),
                    );
                    self.action("assignment_pick_close", &r);
                    close = r.clicked();
                    let r = crate::chrome::dialog_button(
                        ui,
                        &crate::chrome::DialogButton::new("Assign", None)
                            .enabled(pick.value.is_some()),
                    );
                    self.action("assignment_pick_assign", &r);
                    assign_now |= r.clicked();
                });
            });
        if assign_now {
            if let (Some(type_id), Some(value)) = (pick.type_id, pick.value) {
                assign(&mut self.project, pick.assignment, type_id, value);
                self.touch_records();
                self.status = "Assignment code assigned".into();
            }
        }
        if close || ctx.input(|i| i.key_pressed(Key::Escape)) {
            return;
        }
        self.dictionaries.assignment_pick = Some(pick);
    }
}

#[cfg(test)]
mod tests;
