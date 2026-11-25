//! Edit > Assign for several selected activities: Assign Resources, Roles,
//! Activity Codes, Predecessors and Successors work on every selected
//! activity at once and record one undo step.
//!
//! The pickers are the ones Activity Details opens. With one activity
//! selected they keep their single-activity paths; with several selected the
//! chosen value goes to each of them here, in table order. An activity the
//! change cannot apply to (a milestone for a resource, a tie that already
//! exists, a loop) is skipped and counted in the message, and the rest still
//! take it.
use super::*;
use crate::relationships;
use openxer_engine::model::LinkType;

impl App {
    /// The activities an Assign command works on: every selected activity of
    /// the open project in table order, else the current one.
    pub(crate) fn assign_targets(&self) -> Vec<usize> {
        let n = self.project.model.activities.len();
        self.selected_in_order()
            .into_iter()
            .filter(|&i| i < n)
            .collect()
    }

    /// Run `each` on every target as one undo step. `each` returns whether
    /// it changed the activity or why it could not. Returns the status text.
    pub(super) fn bulk_assign<F>(
        &mut self,
        targets: &[usize],
        time: f64,
        mut each: F,
    ) -> (usize, Vec<String>)
    where
        F: FnMut(&mut App, usize) -> Result<bool, String>,
    {
        let before = self.project.clone();
        let before_revision = self.records_revision;
        let mut done = 0;
        let mut skipped = vec![];
        for &i in targets {
            let code = self.project.model.activities[i].task_code.clone();
            match each(self, i) {
                Ok(true) => done += 1,
                Ok(false) => skipped.push(format!("{code}: nothing to change")),
                Err(e) => skipped.push(format!("{code}: {e}")),
            }
        }
        if done > 0 {
            self.edit_step(before, before_revision, time);
        } else {
            self.project = before;
            self.records_revision = before_revision;
        }
        (done, skipped)
    }

    fn bulk_status(&mut self, what: &str, done: usize, skipped: &[String]) {
        let skipped_note = match skipped.len() {
            0 => String::new(),
            n => format!(", {n} skipped ({})", skipped[0]),
        };
        self.status = match (done, skipped.len()) {
            (0, _) => format!("{what}: nothing was assigned{skipped_note}"),
            (1, _) => format!("{what}: 1 activity{skipped_note}"),
            (n, _) => format!("{what}: {n} activities{skipped_note}"),
        };
    }

    /// Assign each of `picks` (resource, role, or a resource in a role;
    /// several chosen in the Assign dialog) to every target as one undo
    /// step. A resource already assigned in the same role is skipped.
    pub(crate) fn assign_resources_bulk(
        &mut self,
        targets: &[usize],
        picks: &[(Option<i64>, Option<i64>)],
        time: f64,
    ) {
        let recalculate = self.detail.calc.recalculate;
        let (done, skipped) = self.bulk_assign(targets, time, |app, i| {
            let task_id = app.project.model.activities[i].task_id;
            let mut added = false;
            let mut error = None;
            for &(rsrc, role) in picks {
                if rsrc.is_some()
                    && app.project.assignment_rows(task_id).into_iter().any(|row| {
                        app.project
                            .assignment_view(row)
                            .is_some_and(|v| v.rsrc_id == rsrc && v.role_id == role)
                    })
                {
                    error = Some("already assigned".to_string());
                    continue;
                }
                match app
                    .project
                    .add_assignment_with(task_id, rsrc, role, recalculate)
                {
                    Ok(_) => {
                        added = true;
                        app.touch_records();
                    }
                    Err(e) => error = Some(e),
                }
            }
            match (added, error) {
                (true, _) => Ok(true),
                (false, Some(e)) => Err(e),
                (false, None) => Ok(false),
            }
        });
        let rsrc = picks.first().and_then(|p| p.0);
        self.bulk_status(
            if rsrc.is_some() {
                "Resource assignment added"
            } else {
                "Role assignment added"
            },
            done,
            &skipped,
        );
    }

    /// Give every target the code value `code` of code type `type_id`.
    pub(crate) fn assign_code_bulk(
        &mut self,
        targets: &[usize],
        type_id: i64,
        code: i64,
        time: f64,
    ) {
        let (done, skipped) = self.bulk_assign(targets, time, |app, i| {
            app.edit_activity(
                i,
                setters::Edit::Code {
                    type_id,
                    value: Some(code),
                },
            )
        });
        self.bulk_status("Activity code assigned", done, &skipped);
    }

    /// Tie every target to `other`: `other` is each target's predecessor
    /// (`predecessor`) or successor.
    #[cfg(test)]
    pub(crate) fn link_bulk(
        &mut self,
        targets: &[usize],
        other: usize,
        predecessor: bool,
        kind: LinkType,
        lag: i64,
        time: f64,
    ) {
        self.link_bulk_many(targets, &[other], predecessor, kind, lag, time);
    }

    /// Tie every target to each of `others` (several chosen in Assign
    /// Predecessors or Successors) as one undo step. A target is skipped
    /// only when none of them could be tied to it.
    pub(crate) fn link_bulk_many(
        &mut self,
        targets: &[usize],
        others: &[usize],
        predecessor: bool,
        kind: LinkType,
        lag: i64,
        time: f64,
    ) {
        let first_new = self.project.model.links.len();
        let (done, skipped) = self.bulk_assign(targets, time, |app, i| {
            let mut added = false;
            let mut error = None;
            for &other in others {
                if i == other {
                    error = Some("an activity cannot be tied to itself".to_string());
                    continue;
                }
                let (pred, succ) = if predecessor { (other, i) } else { (i, other) };
                match relationships::add(&mut app.project.model, pred, succ, kind, lag) {
                    Ok(()) => added = true,
                    Err(e) => error = Some(e),
                }
            }
            match (added, error) {
                (true, _) => Ok(true),
                (false, Some(e)) => Err(e),
                (false, None) => Ok(false),
            }
        });
        self.bulk_status(
            if predecessor {
                "Predecessor assigned"
            } else {
                "Successor assigned"
            },
            done,
            &skipped,
        );
        if done > 0 {
            // Every new relationship, not just the last one.
            let links = first_new..self.project.model.links.len();
            self.check_relationships_sequence(links);
        }
    }

    /// Add template `group`'s steps to every activity `pick` names: after
    /// the step selected on the Steps tab of the activity Activity Details
    /// shows, after the last step on the others.
    pub(crate) fn assign_template_bulk(
        &mut self,
        pick: &crate::dictionaries::steps::TemplatePick,
        group: i64,
        time: f64,
    ) {
        let activities = &self.project.model.activities;
        let targets: Vec<usize> = pick
            .targets
            .iter()
            .filter_map(|t| activities.iter().position(|a| a.task_id == *t))
            .collect();
        let (done, skipped) = self.bulk_assign(&targets, time, |app, i| {
            let task_id = app.project.model.activities[i].task_id;
            let after = pick.after.filter(|_| task_id == pick.task_id);
            let added = app.add_template_steps(group, task_id, after);
            app.touch_records();
            Ok(added > 0)
        });
        self.bulk_status("Steps added from the template", done, &skipped);
    }
}
