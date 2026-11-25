//! Activity Step Templates: named lists of steps (`PROCGROUP` with its
//! `PROCITEM` rows, kept in the project document so they travel with the
//! XER file). The dialog adds, renames and deletes templates and edits each
//! one's steps; the Steps tab of Activity Details adds a template's steps to
//! an activity (Add from Template) and makes a template from an activity's
//! steps (Create Template...). Each change is one undo step.
use super::*;
use crate::records;

pub use openxer_xer::edit::steps::{
    add_template, add_template_step, apply_template_after, delete_template, delete_template_step,
    rename_template, set_template_step, template_from_activity, template_steps, templates,
    StepField,
};

// The tests and the record warm-up still append a template the old way.
#[cfg(test)]
pub use openxer_xer::edit::steps::apply_template;

/// The Activity Step Templates dialog while open.
#[derive(Default)]
pub struct StepsState {
    pub selected: Option<i64>,
    pub step: Option<i64>,
    pub edit: Option<CellEdit>,
    pub step_edit: Option<CellEdit>,
}

/// Steps tab > Add from Template, or Assign > Steps, while open: the chosen
/// template.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TemplatePick {
    /// The activity shown in Activity Details.
    pub task_id: i64,
    pub selected: Option<i64>,
    /// Every activity that takes the steps (Assign > Steps with several
    /// selected); empty means `task_id` alone.
    pub targets: Vec<i64>,
    /// The step of `task_id` selected on the Steps tab (`proc_id`): the
    /// template's steps go in after it there, after the last step elsewhere.
    pub after: Option<i64>,
}

impl App {
    pub(super) fn steps_dialog(
        &mut self,
        ctx: &egui::Context,
        st: &mut DictionaryState,
        cmds: &mut Vec<Cmd>,
        escape: bool,
    ) {
        let Some(mut ss) = st.steps.take() else {
            return;
        };
        let busy = st.busy();
        let doc = &self.project.document;
        let list = templates(doc);
        if !list.iter().any(|t| Some(t.id) == ss.selected) {
            ss.selected = list.first().map(|t| t.id);
        }
        let steps = ss.selected.map_or(vec![], |id| template_steps(doc, id));
        if !steps.iter().any(|s| Some(s.id) == ss.step) {
            ss.step = None;
        }
        let selected = list.iter().position(|t| Some(t.id) == ss.selected);
        let step_row = steps.iter().position(|s| Some(s.id) == ss.step);
        let entries = [
            Entry::close("step_templates_close"),
            Entry::Button("step_templates_add", "Add", true, true),
            Entry::Button("step_templates_delete", "Delete", selected.is_some(), false),
            Entry::Button(
                "step_templates_add_step",
                "Add Step",
                selected.is_some(),
                true,
            ),
            Entry::Button(
                "step_templates_delete_step",
                "Delete Step",
                step_row.is_some(),
                false,
            ),
            Entry::help("step_templates_help"),
        ];
        let editing = ss.edit.is_some() || ss.step_edit.is_some();
        let mut open = !escape || editing;
        let mut edit = ss.edit.take();
        let mut step_edit = ss.step_edit.take();
        let mut renames: Vec<(i64, String)> = vec![];
        let mut step_sets: Vec<(i64, StepField, String)> = vec![];
        let mut cells: Vec<(usize, usize, Rect)> = vec![];
        let mut step_cells: Vec<(usize, usize, Rect)> = vec![];
        let mut out = crate::details::GridOut::default();
        let mut step_out = crate::details::GridOut::default();
        let mut column = kit::Column::default();
        let mut rects = vec![];
        chrome::dialog_window("Activity Step Templates")
            .id(egui::Id::new("step_templates_dialog"))
            .default_pos([340., 120.])
            .show(ctx, |ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    let (_, col, r) = with_column(ui, "step_templates", &entries, |ui| {
                        heading(
                            ui,
                            "step_templates_heading",
                            420.,
                            "Activity Step Templates",
                        );
                        ui.add_space(6.);
                        widgets::display_bar(ui, 420., "Templates");
                        out = grid(
                            ui,
                            &grid_id(ui, "step_templates_grid"),
                            &[("Step Template Name", 340.), ("Steps", 78.)],
                            list.len(),
                            selected,
                            Vec2::new(420., 150.),
                            &mut |ui, ev, _| {
                                let Ev::Cell(i, c) = ev else {
                                    return;
                                };
                                cells.push((i, c, ui.max_rect()));
                                let t = &list[i];
                                if c == 0 {
                                    if let Some(text) =
                                        editable_cell(ui, &mut edit, t.id, 0, &t.name)
                                    {
                                        renames.push((t.id, text));
                                    }
                                } else {
                                    cell(ui, t.steps.to_string());
                                }
                            },
                        );
                        if let Some(i) = out.clicked {
                            if selected == Some(i)
                                && edit.is_none()
                                && clicked_column(ui, &cells, i) == Some(0)
                            {
                                edit = Some(CellEdit {
                                    id: list[i].id,
                                    column: 0,
                                    text: list[i].name.clone(),
                                    focus: true,
                                });
                            }
                        }
                        ui.add_space(6.);
                        widgets::display_bar(ui, 420., "Steps");
                        step_out = grid(
                            ui,
                            &grid_id(ui, "step_template_steps_grid"),
                            &[("Step Name", 340.), ("Step Weight", 78.)],
                            steps.len(),
                            step_row,
                            Vec2::new(420., 150.),
                            &mut |ui, ev, _| {
                                let Ev::Cell(i, c) = ev else {
                                    return;
                                };
                                step_cells.push((i, c, ui.max_rect()));
                                let s = &steps[i];
                                let shown = if c == 0 {
                                    s.name.clone()
                                } else {
                                    records::format_number(s.weight)
                                };
                                if let Some(text) =
                                    editable_cell(ui, &mut step_edit, s.id, c, &shown)
                                {
                                    let field = if c == 0 {
                                        StepField::Name
                                    } else {
                                        StepField::Weight
                                    };
                                    step_sets.push((s.id, field, text));
                                }
                            },
                        );
                        if let Some(i) = step_out.clicked {
                            if step_row == Some(i) && step_edit.is_none() {
                                if let Some(c) = clicked_column(ui, &step_cells, i) {
                                    let s = &steps[i];
                                    step_edit = Some(CellEdit {
                                        id: s.id,
                                        column: c,
                                        text: if c == 0 {
                                            s.name.clone()
                                        } else {
                                            records::format_number(s.weight)
                                        },
                                        focus: true,
                                    });
                                }
                            }
                        }
                    });
                    column = col;
                    rects = r;
                });
            });
        for (name, rect) in rects {
            st.rect(name, rect);
        }
        for (r, c, rect) in &cells {
            st.rect(format!("step_templates_cell_{r}_{c}"), *rect);
        }
        for (r, c, rect) in &step_cells {
            st.rect(format!("step_template_steps_cell_{r}_{c}"), *rect);
        }
        if let Some(i) = out.clicked {
            if ss.selected != Some(list[i].id) {
                ss.step = None;
            }
            ss.selected = Some(list[i].id);
        }
        if let Some(i) = step_out.clicked {
            ss.step = Some(steps[i].id);
        }
        ss.edit = edit;
        ss.step_edit = step_edit;
        for (id, text) in renames {
            cmds.push(Cmd::RenameTemplate(id, text));
        }
        for (id, field, text) in step_sets {
            cmds.push(Cmd::SetTemplateStep(id, field, text));
        }
        match column.clicked.as_deref() {
            Some("step_templates_close") => open = false,
            Some("step_templates_add") => cmds.push(Cmd::AddTemplate),
            Some("step_templates_delete") => {
                if let Some(t) = selected.map(|i| &list[i]) {
                    st.confirm = Some(Confirm {
                        title: "Activity Step Templates".into(),
                        text: format!(
                            "Delete the template {} and its {} steps? Activities keep the steps already added from it.",
                            t.name, t.steps
                        ),
                        buttons: &["Yes", "No"],
                        then: vec![Some(Cmd::DeleteTemplate(t.id)), None],
                        pick_replacement: None,
                    });
                }
            }
            Some("step_templates_add_step") => {
                if let Some(id) = ss.selected {
                    cmds.push(Cmd::AddTemplateStep(id));
                }
            }
            Some("step_templates_delete_step") => {
                if let Some(id) = ss.step {
                    cmds.push(Cmd::DeleteTemplateStep(id));
                    ss.step = None;
                }
            }
            Some("step_templates_help") => self.open_help_for("Activity Step Templates"),
            _ => {}
        }
        if open {
            st.steps = Some(ss);
        }
    }

    /// Assign > Steps: the Steps tab and the template picker for every
    /// selected activity. The steps go in after the step selected on the
    /// Steps tab of the activity shown, after the last step elsewhere.
    pub(crate) fn open_assign_steps(&mut self) {
        let on_steps = self.detail.tab == crate::details::DetailTab::Steps;
        let step_row = self.detail.rows[0].filter(|_| on_steps);
        self.show_detail_tab(crate::details::DetailTab::Steps);
        let targets = self.assign_targets();
        let Some(&first) = targets.first() else {
            self.status = "Select an activity first.".into();
            return;
        };
        let Some(template) = templates(&self.project.document).first().map(|t| t.id) else {
            self.status = crate::details::record_tabs::NO_TEMPLATES.into();
            return;
        };
        let activities = &self.project.model.activities;
        let shown = self
            .selected
            .filter(|&i| i < activities.len())
            .unwrap_or(first);
        let task_id = activities[shown].task_id;
        let doc = &self.project.document;
        let pid = self.project.model.project.proj_id;
        let order = openxer_xer::edit::steps::sorted_steps(doc, task_id, pid);
        let after = step_row
            .and_then(|r| order.get(r))
            .map(|&row| openxer_xer::edit::steps::step_id(doc, row));
        self.dictionaries.template_pick = Some(TemplatePick {
            task_id,
            selected: Some(template),
            targets: targets.iter().map(|&i| activities[i].task_id).collect(),
            after,
        });
    }

    /// Add template `group`'s steps to activity `task_id` after its step
    /// `after` (after the last step when `None`); returns how many.
    pub(crate) fn add_template_steps(
        &mut self,
        group: i64,
        task_id: i64,
        after: Option<i64>,
    ) -> usize {
        let pid = self.project.model.project.proj_id;
        let doc = &mut self.project.document;
        let n = apply_template_after(doc, group, task_id, pid, after);
        openxer_xer::edit::steps::after_steps_change(&mut self.project, task_id);
        n
    }

    /// Steps tab > Add from Template and Assign > Steps: pick a template; Add
    /// puts its steps on the activity (after the selected step) or, with
    /// several activities selected, on each of them as one undo step.
    pub(crate) fn template_pick_window(&mut self, ctx: &egui::Context) {
        let Some(mut pick) = self.dictionaries.template_pick.take() else {
            return;
        };
        let list = templates(&self.project.document);
        let size = Vec2::new(440., 230.);
        let selected = list.iter().position(|t| Some(t.id) == pick.selected);
        let many = pick.targets.len() > 1;
        let prompt = if many {
            format!(
                "Select a step template for the {} selected activities:",
                pick.targets.len()
            )
        } else {
            "Select a step template:".to_string()
        };
        let ((add, close), closed, actions) =
            kit::window(ctx, "template_pick", "Add from Template", size, |c| {
                kit::strip(c, size.x, size.y);
                kit::text(c, 5., 5., &prompt, true);
                let out = kit::list(
                    c,
                    "template_pick",
                    (5., 22., size.x - 122., size.y - 27.),
                    None,
                    &[
                        kit::Col {
                            title: "Step Template",
                            width: 220.,
                        },
                        kit::Col {
                            title: "Steps",
                            width: 79.,
                        },
                    ],
                    list.len(),
                    selected,
                    None,
                    &mut |c, r, col, cell, sel| {
                        let t = &list[r];
                        let text = if col == 0 {
                            t.name.clone()
                        } else {
                            t.steps.to_string()
                        };
                        kit::cell_text(c, cell, &text, sel, 3.);
                    },
                );
                if let Some(r) = out.clicked.or(out.double) {
                    pick.selected = Some(list[r].id);
                }
                let column = kit::button_column(
                    c,
                    size.x,
                    &[
                        Entry::Button("template_pick_add", "Add", pick.selected.is_some(), false),
                        Entry::cancel("template_pick_cancel"),
                    ],
                );
                let clicked = column.clicked.unwrap_or_default();
                (
                    out.double.is_some() || clicked == "template_pick_add",
                    clicked == "template_pick_cancel",
                )
            });
        self.record_dialog_actions(actions);
        let close = close || closed;
        if add {
            if let Some(group) = pick.selected {
                if many {
                    let time = ctx.input(|i| i.time);
                    self.assign_template_bulk(&pick, group, time);
                } else {
                    let n = self.add_template_steps(group, pick.task_id, pick.after);
                    self.touch_records();
                    self.status = format!("{n} steps added from the template");
                }
            }
            return;
        }
        if close {
            return;
        }
        self.dictionaries.template_pick = Some(pick);
    }
}

#[cfg(test)]
mod tests;
