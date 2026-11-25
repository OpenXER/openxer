//! Paste of activities cut in another open project: a move. The activities
//! leave their project and arrive in the paste target's WBS with their
//! Activity IDs, their internal keys where the target does not use them,
//! and every detail row, with no dialogs. Relationships with activities that
//! stay behind become relationships between the two projects (rows owned by
//! the successor's project, as the file format keeps them); relationships
//! with activities of the target become ordinary ones there; relationships
//! with a third open project are pointed at the moved activities. A
//! relationship that cannot be kept (the two projects share a project ID,
//! so a row between them could not say which is which) is dropped and the
//! status line says how many. An Activity ID the target already uses
//! refuses the paste and keeps the cut. The move is one undo step across
//! every project it changed.
use super::*;
use openxer_engine::model::Link;
use std::collections::{HashMap, HashSet};

/// A relationship touching a moved activity, by `task_id` and project ID.
struct Tie {
    pred: (i64, i64),
    succ: (i64, i64),
    link_type: LinkType,
    lag: i64,
}

impl App {
    /// Move the cut activities from the open project they were cut in into
    /// this one. Returns false when that project is not open (the caller
    /// then pastes copies through the usual dialogs).
    pub(super) fn move_cut_across(&mut self, time: f64) -> bool {
        let Some(clip) = self.editing.clipboard.clone() else {
            return false;
        };
        let Some(k) = self
            .open_set
            .parked
            .iter()
            .position(|p| ops::project_key(&p.project) == clip.source)
        else {
            return false;
        };
        let target_name = self.project.model.project.name.clone();
        let source = &self.open_set.parked[k].project;
        let rows: Vec<usize> = clip
            .activities
            .iter()
            .filter_map(|c| {
                source
                    .model
                    .activities
                    .iter()
                    .position(|a| a.task_id == c.activity.task_id)
            })
            .collect();
        if rows.is_empty() {
            self.error("The cut activities are no longer in their project.".to_string());
            return true;
        }
        // An Activity ID already used here refuses the paste; the cut stays.
        let taken: Vec<String> = rows
            .iter()
            .map(|&i| source.model.activities[i].task_code.clone())
            .filter(|code| {
                self.project
                    .model
                    .activities
                    .iter()
                    .any(|a| &a.task_code == code)
            })
            .collect();
        if !taken.is_empty() {
            self.error(format!(
                "{} already has Activity ID {}. Nothing was moved; the cut is kept, so you can \
                 renumber and paste again.",
                target_name,
                taken.join(", ")
            ));
            return true;
        }
        // The activities as they are now, with every relationship that
        // touches them in any open project.
        let fresh = ops::copy(source, &rows);
        let moved: HashSet<i64> = fresh
            .activities
            .iter()
            .map(|c| c.activity.task_id)
            .collect();
        let source_pid = source.model.project.proj_id;
        let target_pid = self.project.model.project.proj_id;
        let mut ties = vec![];
        for l in &source.model.links {
            let (p, s) = (
                source.model.activities[l.pred].task_id,
                source.model.activities[l.succ].task_id,
            );
            // Links among the moved activities travel with the paste.
            if moved.contains(&p) != moved.contains(&s) {
                ties.push(Tie {
                    pred: (source_pid, p),
                    succ: (source_pid, s),
                    link_type: l.link_type,
                    lag: l.lag,
                });
            }
        }
        let mut owners: Vec<(Option<usize>, i64)> = vec![];
        for (slot, q) in std::iter::once(None)
            .chain((0..self.open_set.parked.len()).map(Some))
            .map(|slot| {
                let q = match slot {
                    None => &self.project,
                    Some(j) => &self.open_set.parked[j].project,
                };
                (slot, q)
            })
        {
            for x in q.external_links() {
                let pred_moved = x.pred_project == source_pid && moved.contains(&x.pred_task);
                let succ_moved = slot == Some(k) && moved.contains(&x.succ_task);
                if pred_moved || succ_moved {
                    owners.push((slot, x.row_id));
                    ties.push(Tie {
                        pred: (x.pred_project, x.pred_task),
                        succ: (x.succ_project, x.succ_task),
                        link_type: x.link_type,
                        lag: x.lag,
                    });
                }
            }
        }
        // Apply to copies; nothing changes unless all of it works.
        let wbs = self.target_wbs();
        let before_target = self.project.clone();
        let before_revision = self.records_revision;
        let mut target = self.project.clone();
        let mut others: Vec<ProjectData> = self
            .open_set
            .parked
            .iter()
            .map(|p| p.project.clone())
            .collect();
        let mut changed: HashSet<usize> = HashSet::from([k]);
        let result = (|| -> Result<(Vec<usize>, usize), String> {
            for (slot, row) in &owners {
                match slot {
                    None => target.remove_external_link(*row)?,
                    Some(j) => {
                        others[*j].remove_external_link(*row)?;
                        changed.insert(*j);
                    }
                }
            }
            let pasted = ops::paste_moved(&mut target, &fresh, wbs)?;
            // Old key to new key (the same unless the target used it).
            let keys: HashMap<i64, i64> = fresh
                .activities
                .iter()
                .zip(&pasted.rows)
                .map(|(c, &i)| (c.activity.task_id, target.model.activities[i].task_id))
                .collect();
            let old: Vec<i64> = keys.keys().copied().collect();
            others[k].delete_activities(&old)?;
            let mut dropped = 0;
            for t in ties {
                let end = |(pid, task): (i64, i64)| match keys.get(&task) {
                    Some(&new) if pid == source_pid => (target_pid, new, true),
                    _ => (pid, task, false),
                };
                let (pred, succ) = (end(t.pred), end(t.succ));
                if pred.0 == succ.0 && pred.2 != succ.2 && source_pid == target_pid {
                    // Two projects with one project ID: a row between them
                    // could not tell them apart.
                    dropped += 1;
                    continue;
                }
                if pred.0 == succ.0 && pred.2 != succ.2 && pred.0 == target_pid {
                    // Both ends now in the target: an ordinary relationship.
                    let at = |task: i64| {
                        target
                            .model
                            .activities
                            .iter()
                            .position(|a| a.task_id == task)
                    };
                    match (at(pred.1), at(succ.1)) {
                        (Some(p), Some(s))
                            if crate::relationships::can_connect(&target.model, p, s).is_ok() =>
                        {
                            target.model.links.push(Link {
                                pred: p,
                                succ: s,
                                link_type: t.link_type,
                                lag: t.lag,
                                driving_path: false,
                            });
                            target.model.rebuild_links()?;
                        }
                        _ => dropped += 1,
                    }
                    continue;
                }
                if pred.0 == succ.0 {
                    dropped += 1;
                    continue;
                }
                // The row belongs to the successor's project.
                let added = if succ.0 == target_pid {
                    target.add_external_link(pred.0, pred.1, succ.1, t.link_type, t.lag)
                } else {
                    match others
                        .iter()
                        .position(|q| q.model.project.proj_id == succ.0)
                    {
                        Some(j) => {
                            changed.insert(j);
                            others[j].add_external_link(pred.0, pred.1, succ.1, t.link_type, t.lag)
                        }
                        None => Err(String::new()),
                    }
                };
                if added.is_err() {
                    dropped += 1;
                }
            }
            Ok((pasted.rows, dropped))
        })();
        let (rows, dropped) = match result {
            Ok(done) => done,
            Err(e) => {
                self.error(e);
                return true;
            }
        };
        // Commit: one undo step across every project that changed.
        let mut touched = vec![];
        for (j, next) in others.into_iter().enumerate() {
            if !changed.contains(&j) {
                continue;
            }
            let parked = &mut self.open_set.parked[j];
            let previous = std::mem::replace(&mut parked.project, next);
            let previous_revision = parked.records_revision;
            self.records_serial += 1;
            parked.records_revision = self.records_serial;
            parked.history.record_bulk(
                &previous,
                previous_revision,
                &parked.project,
                parked.records_revision,
                time,
            );
            parked.dirty = true;
            touched.push(parked.id);
        }
        self.project = target;
        self.project.rebuild_assignments();
        self.edit_step(before_target, before_revision, time);
        if let Some(id) = self.open_set.active {
            touched.insert(0, id);
        }
        self.open_set.steps_seen = self.history.steps();
        self.open_set.undo.push(touched);
        self.open_set.redo.clear();
        self.open_set.version += 1;
        self.refresh_composite();
        // The cut now names the activities where they are: a later Paste
        // moves them on within this project.
        let mut again = ops::copy(&self.project, &rows);
        again.text = clip.text;
        again.cut = true;
        self.editing.clipboard = Some(again);
        self.select_activities(&rows);
        self.reveal_activity(rows[0]);
        self.scroll_to_selected = true;
        let count = match rows.len() {
            1 => format!("1 activity moved to {target_name}"),
            n => format!("{n} activities moved to {target_name}"),
        };
        self.status = match dropped {
            0 => count,
            1 => format!("{count}. 1 relationship could not be kept and was removed."),
            n => format!("{count}. {n} relationships could not be kept and were removed."),
        };
        true
    }
}
