//! Undo across opening and closing projects in one session.
//!
//! Each project has its own undo history. Opening another project closes the
//! open one; its history is kept for the rest of the session and comes back
//! when that project is opened again, as long as its workspace copy has not
//! changed since. Workspace steps (adding, pasting or deleting a project)
//! are steps of the project that was open when they were made, so they stay
//! undoable there.
//!
//! File > New is the one workspace step that also opens a project: its step
//! heads the new project's history as well. Undoing it there deletes the new
//! project and returns to the project that was open before, with that
//! project's history, where Redo adds the new project again and opens it
//! with the steps undone in it, ready to redo.
use super::*;
use std::hash::{Hash, Hasher};

/// Histories of projects closed this session, newest last.
const KEPT: usize = 8;

/// The undo history of a project closed this session.
pub struct Closed {
    /// Workspace row id.
    id: i64,
    /// Fingerprint of the workspace copy when it closed.
    text: u64,
    history: history::History,
    records_revision: u64,
}

fn fingerprint(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

impl App {
    fn stored_fingerprint(&self, id: i64) -> Option<u64> {
        self.store.source(id).ok().map(|s| fingerprint(&s.2))
    }

    /// Keep the history of the open project before another one opens (a
    /// single open project only, saved first by the caller).
    pub(crate) fn keep_closing_history(&mut self) {
        if self.open_set.multi() {
            return;
        }
        let Some(id) = self.open_set.active else {
            return;
        };
        self.history
            .record(&self.project, self.records_revision, None, 0.);
        if self.history.counts() == (0, 0) {
            return;
        }
        let Some(text) = self.stored_fingerprint(id) else {
            return;
        };
        let history = std::mem::replace(&mut self.history, history::History::new(&self.project));
        let kept = &mut self.open_set.closed;
        kept.retain(|c| c.id != id);
        kept.push(Closed {
            id,
            text,
            history,
            records_revision: self.records_revision,
        });
        if kept.len() > KEPT {
            kept.remove(0);
        }
    }

    /// Give the project just opened the history it had when it closed,
    /// when its workspace copy is still the one it closed with.
    pub(crate) fn restore_closed_history(&mut self) {
        let Some(id) = self.open_set.active else {
            return;
        };
        let Some(k) = self.open_set.closed.iter().position(|c| c.id == id) else {
            return;
        };
        let c = self.open_set.closed.remove(k);
        if self.stored_fingerprint(id) != Some(c.text)
            || !c.history.is_current(&self.project, c.records_revision)
        {
            return;
        }
        self.history = c.history;
        self.records_revision = c.records_revision;
        self.records_serial = self.records_serial.max(c.records_revision);
        self.open_set.steps_seen = self.history.steps();
    }

    /// File > New just opened the project it added in step `op`, made in
    /// project `from`: the step also heads the new project's history.
    pub(crate) fn adopt_create_step(&mut self, op: u64, from: i64, time: f64) {
        self.history
            .record_external(&self.project, self.records_revision, op, time);
        self.open_set.steps_seen = self.history.steps();
        self.open_set.created_from.insert(op, from);
    }

    /// After undoing workspace step `op`: when it removed the open project
    /// (File > New undone in the new project), open the project it was
    /// made in again, with its history, past that same step.
    pub(crate) fn leave_removed_project(&mut self, op: u64) {
        let Some(&from) = self.open_set.created_from.get(&op) else {
            return;
        };
        let Some(id) = self.open_set.active else {
            return;
        };
        if self.workspace_ids().contains(&id) || self.open_set.multi() {
            return;
        }
        let name = self.project.model.project.name.clone();
        self.open_set.closed.retain(|c| c.id != id);
        let undone = std::mem::replace(&mut self.history, history::History::new(&self.project));
        self.open_set.created_redo.insert(op, (id, undone));
        self.dirty = false;
        self.open_set.active = None;
        let back = match self.store.load(from) {
            Ok(p) => p,
            Err(e) => {
                self.error(e);
                return;
            }
        };
        self.load(back);
        // The step is undone already; step back past it without running it again.
        if self.history.undo_external() == Some(op) && self.history.undo(&mut self.project) {
            self.records_revision = self.history.revision();
            self.open_set.steps_seen = self.history.steps();
        }
        self.status = format!("Undo: project {name} removed");
    }

    /// After redoing workspace step `op` (File > New undone in the project
    /// it was made in): open the project it adds again, with the history it
    /// had when the step was undone, past that same step.
    pub(crate) fn enter_recreated_project(&mut self, op: u64) {
        let Some((id, undone)) = self.open_set.created_redo.remove(&op) else {
            return;
        };
        if self.open_set.multi() || !self.workspace_ids().contains(&id) {
            return;
        }
        let project = match self.store.load(id) {
            Ok(p) => p,
            Err(e) => {
                self.error(e);
                return;
            }
        };
        self.load(project);
        if self.open_set.active != Some(id)
            || !undone.is_current(&self.project, undone.revision())
            || undone.redo_external() != Some(op)
        {
            return;
        }
        self.history = undone;
        self.records_revision = self.history.revision();
        self.records_serial = self.records_serial.max(self.records_revision);
        // The step is redone already; step past it without running it again.
        if self.history.redo(&mut self.project) {
            self.records_revision = self.history.revision();
        }
        self.open_set.steps_seen = self.history.steps();
        self.status = format!("Redo: project {} added", self.project.model.project.name);
    }
}
