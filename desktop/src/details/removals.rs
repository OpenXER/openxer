//! The questions Activity Details asks before a removal: a relationship
//! (Predecessors, Successors and Relationships tabs), a resource assignment
//! (Resources tab, and Remove in Assign Resources; the activity's primary
//! resource gets a second question) and an activity code assignment (Codes
//! tab, and Remove in Assign Activity Codes); and in Project Details a
//! project code assignment (Codes tab and Assign Project Codes). Yes
//! removes it as one undo step; No and Escape leave everything as it was.
//! Also the question before Cost Units Linked is ticked again. The defaults
//! come from `kit_window::Question`.
use super::*;
use crate::kit_window::{self as kit, Question as Q};

/// A removal waiting for its answer. Each names what it removes by ids, so
/// the answer acts on the same record even if rows moved meanwhile.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Removal {
    /// A relationship of this project: predecessor and successor task ids
    /// and its type; `slot` is the details grid whose selection it clears.
    Relationship {
        pred: i64,
        succ: i64,
        kind: LinkType,
        slot: usize,
    },
    /// A relationship with an activity of another project: (owner project,
    /// row id), as `change_cross_link` takes them.
    CrossRelationship {
        owner: i64,
        row_id: i64,
        slot: usize,
    },
    /// A TASKRSRC row of `task_id`. `primary` asks the second question.
    Assignment {
        row: usize,
        task_id: i64,
        rsrc_id: Option<i64>,
        primary: bool,
    },
    /// The second question, for the activity's primary resource.
    PrimaryResource {
        row: usize,
        task_id: i64,
        rsrc_id: Option<i64>,
    },
    /// A TASKACTV row: activity `task_id`, code value `code_id`.
    Code {
        row: usize,
        task_id: i64,
        code_id: i64,
    },
    /// The Curve cell of a Manual assignment (TASKRSRC row of `task_id`)
    /// was cleared or given `curve`: its typed period values go.
    ManualCurve {
        row: usize,
        task_id: i64,
        curve: Option<String>,
    },
    /// Remove in Assign Resources: resource `rsrc_id` from each of the
    /// activities `task_ids` that has it.
    ResourceFrom { rsrc_id: i64, task_ids: Vec<i64> },
    /// Remove in Assign Activity Codes: value `code_id` from each of the
    /// activities `task_ids` that has it.
    CodeFrom { code_id: i64, task_ids: Vec<i64> },
    /// Cost Units Linked ticked again on TASKRSRC row `row`: recalculate
    /// its costs from its units?
    RelinkCosts { row: usize },
    /// A PROJPCAT row of the open project: project code value `value_id`.
    ProjectCode { value_id: i64 },
}

/// Message box ids; the buttons' action keys are `<id>_yes` and `<id>_no`.
pub(crate) const RELATIONSHIP_ID: &str = "remove_relationship";
pub(crate) const ASSIGNMENT_ID: &str = "remove_assignment";
pub(crate) const PRIMARY_ID: &str = "remove_primary";
pub(crate) const CODE_ID: &str = "remove_code";
pub(crate) const MANUAL_CURVE_ID: &str = "remove_manual_curve";
pub(crate) const RELINK_ID: &str = "relink_costs";
pub(crate) const PROJECT_CODE_ID: &str = "remove_project_code";

impl App {
    /// Ask before removing the relationship at model index `index`.
    pub(super) fn ask_remove_relationship(&mut self, index: usize, slot: usize) {
        let model = &self.project.model;
        let Some(l) = model.links.get(index) else {
            return;
        };
        self.detail.removal = Some(Removal::Relationship {
            pred: model.activities[l.pred].task_id,
            succ: model.activities[l.succ].task_id,
            kind: l.link_type,
            slot,
        });
    }

    /// The question on top of the details pane, when one is waiting.
    pub(super) fn removal_dialog(&mut self, ctx: &egui::Context) {
        let Some(removal) = self.detail.removal.clone() else {
            return;
        };
        let (id, text, question) = match &removal {
            Removal::Relationship { .. } | Removal::CrossRelationship { .. } => (
                RELATIONSHIP_ID,
                "Remove the selected relationship?".to_string(),
                Q::RemoveRelationship,
            ),
            Removal::Assignment { .. } | Removal::ResourceFrom { .. } => (
                ASSIGNMENT_ID,
                "Remove the selected resource assignment from this activity?".to_string(),
                Q::RemoveAssignment,
            ),
            Removal::RelinkCosts { .. } => (
                RELINK_ID,
                "The costs of this assignment will be worked out again from its units \
                 and price. Calculate its costs from its units?"
                    .to_string(),
                Q::RelinkCosts,
            ),
            Removal::PrimaryResource {
                task_id, rsrc_id, ..
            } => {
                let resource = match rsrc_id.and_then(|r| self.project.resource_names.get(&r)) {
                    Some(name) => name.clone(),
                    None => "This resource".to_string(),
                };
                let activities = &self.project.model.activities;
                let activity = match activities.iter().position(|a| a.task_id == *task_id) {
                    Some(i) => format!("{} - {}", activities[i].task_code, self.project.name(i)),
                    None => String::new(),
                };
                let text = [
                    resource.as_str(),
                    " is the primary resource of ",
                    activity.as_str(),
                    ". Remove it anyway?",
                ];
                (PRIMARY_ID, text.concat(), Q::RemovePrimaryResource)
            }
            Removal::Code { .. } | Removal::CodeFrom { .. } => (
                CODE_ID,
                "Remove the selected activity code from this activity?".to_string(),
                Q::RemoveCode,
            ),
            Removal::ManualCurve { .. } => (
                MANUAL_CURVE_ID,
                "This assignment's curve is Manual. Taking it off deletes the units typed into \
                 its periods, and its units spread evenly again. Go ahead?"
                    .to_string(),
                Q::RemoveManualCurve,
            ),
            Removal::ProjectCode { .. } => (
                PROJECT_CODE_ID,
                "Remove the selected project code from this project?".to_string(),
                Q::RemoveProjectCode,
            ),
        };
        let buttons = ["Yes", "No"];
        let mut rects = vec![];
        let clicked = kit::message_box(
            ctx,
            id,
            "Confirmation",
            kit::MessageIcon::Question,
            &text,
            &buttons,
            question.default_in(&buttons),
            &mut rects,
        );
        self.record_dialog_actions(rects);
        let answer = match clicked {
            Some(k) => Some(k == 0),
            None if ctx.input(|i| i.key_pressed(Key::Escape)) => Some(false),
            None => None,
        };
        match answer {
            Some(true) => {
                self.detail.removal = None;
                self.remove_confirmed(removal, ctx.input(|i| i.time));
            }
            Some(false) => self.detail.removal = None,
            None => {}
        }
    }

    /// Yes: the removal itself, as before the question existed.
    fn remove_confirmed(&mut self, removal: Removal, time: f64) {
        match removal {
            Removal::Relationship {
                pred,
                succ,
                kind,
                slot,
            } => {
                let model = &self.project.model;
                let found = model.links.iter().position(|l| {
                    model.activities[l.pred].task_id == pred
                        && model.activities[l.succ].task_id == succ
                        && l.link_type == kind
                });
                let Some(index) = found else {
                    return;
                };
                match relationships::remove(&mut self.project.model, index) {
                    Ok(()) => {
                        self.dirty = true;
                        self.detail.rows[slot] = None;
                    }
                    Err(error) => self.status = error,
                }
            }
            Removal::CrossRelationship {
                owner,
                row_id,
                slot,
            } => match self.change_cross_link(owner, row_id, None) {
                Ok(()) => self.detail.rows[slot] = None,
                Err(error) => self.status = error,
            },
            Removal::Assignment {
                row,
                task_id,
                rsrc_id,
                primary: true,
            } => {
                self.detail.removal = Some(Removal::PrimaryResource {
                    row,
                    task_id,
                    rsrc_id,
                });
            }
            Removal::Assignment {
                row,
                task_id,
                rsrc_id,
                primary: false,
            }
            | Removal::PrimaryResource {
                row,
                task_id,
                rsrc_id,
            } => {
                let view = self.project.assignment_view(row);
                let mine = self.project.assignment_rows(task_id).contains(&row);
                if !mine || view.is_none_or(|v| v.rsrc_id != rsrc_id) {
                    return;
                }
                let recalculate = self.detail.calc.recalculate;
                match self.project.remove_assignment_with(row, recalculate) {
                    Ok(()) => {
                        self.detail.rows[0] = None;
                        self.touch_records();
                    }
                    Err(e) => self.status = e,
                }
            }
            Removal::Code {
                row,
                task_id,
                code_id,
            } => {
                let doc = &self.project.document;
                let same = num(&records::get(doc, "TASKACTV", row, "task_id")) == task_id
                    && num(&records::get(doc, "TASKACTV", row, "actv_code_id")) == code_id;
                if !same {
                    return;
                }
                records::remove_row(&mut self.project.document, "TASKACTV", row);
                self.detail.rows[0] = None;
                self.touch_records();
            }
            Removal::ManualCurve {
                row,
                task_id,
                curve,
            } => {
                if !self.project.assignment_rows(task_id).contains(&row) {
                    return;
                }
                let doc = &mut self.project.document;
                match openxer_xer::edit::curves::set_assignment_curve(doc, row, curve.as_deref()) {
                    Ok(true) => self.touch_records(),
                    Ok(false) => {}
                    Err(e) => self.status = e,
                }
            }
            Removal::ResourceFrom { rsrc_id, task_ids } => {
                let recalculate = self.detail.calc.recalculate;
                let targets = self.targets_of(&task_ids);
                self.remove_from_each(&targets, "Resource removed", time, |app, task_id| {
                    let row = app.project.assignment_rows(task_id).into_iter().find(|&r| {
                        app.project
                            .assignment_view(r)
                            .is_some_and(|v| v.rsrc_id == Some(rsrc_id))
                    });
                    match row {
                        Some(row) => app
                            .project
                            .remove_assignment_with(row, recalculate)
                            .map(|()| true),
                        None => Ok(false),
                    }
                });
                self.detail.rows[0] = None;
            }
            Removal::CodeFrom { code_id, task_ids } => {
                let targets = self.targets_of(&task_ids);
                self.remove_from_each(&targets, "Code removed", time, |app, task_id| {
                    let doc = &app.project.document;
                    let rows = doc.table("TASKACTV").map_or(0, |t| t.rows.len());
                    let found = (0..rows).find(|&r| {
                        num(&records::get(doc, "TASKACTV", r, "task_id")) == task_id
                            && num(&records::get(doc, "TASKACTV", r, "actv_code_id")) == code_id
                    });
                    match found {
                        Some(r) => {
                            records::remove_row(&mut app.project.document, "TASKACTV", r);
                            Ok(true)
                        }
                        None => Ok(false),
                    }
                });
                self.detail.rows[0] = None;
            }
            Removal::RelinkCosts { row } => {
                use openxer_xer::assignments::AssignmentEdit;
                match self
                    .project
                    .edit_assignment(row, AssignmentEdit::CostUnitsLinked(true))
                {
                    Ok(()) => self.touch_records(),
                    Err(e) => self.status = e,
                }
            }
            Removal::ProjectCode { value_id } => {
                let pid = self.project.model.project.proj_id;
                let doc = &mut self.project.document;
                if openxer_xer::edit::project_codes::remove(doc, pid, value_id) {
                    self.views.projects.code_row = None;
                    self.notes_step();
                }
            }
        }
    }

    /// The activity indexes of `task_ids` in the open project.
    fn targets_of(&self, task_ids: &[i64]) -> Vec<usize> {
        let activities = &self.project.model.activities;
        task_ids
            .iter()
            .filter_map(|t| activities.iter().position(|a| a.task_id == *t))
            .collect()
    }

    /// Run `each` on every activity of `targets` (by task id) as one undo
    /// step; `each` says whether it removed something.
    fn remove_from_each(
        &mut self,
        targets: &[usize],
        what: &str,
        time: f64,
        mut each: impl FnMut(&mut App, i64) -> Result<bool, String>,
    ) {
        let (done, skipped) = self.bulk_assign(targets, time, |app, i| {
            let task_id = app.project.model.activities[i].task_id;
            let removed = each(app, task_id)?;
            if removed {
                app.touch_records();
            }
            Ok(removed)
        });
        self.status = match (done, skipped.first()) {
            (0, Some(why)) => format!("{what}: nothing removed ({why})"),
            (0, None) => format!("{what}: nothing removed"),
            (1, _) => format!("{what} from 1 activity"),
            (n, _) => format!("{what} from {n} activities"),
        };
    }
}
