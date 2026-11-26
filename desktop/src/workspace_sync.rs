//! Live sync of the workspace dictionary (workspace schema 3).
//!
//! Global calendars, resources, roles, global and EPS codes, UDFs and the
//! other workspace dictionaries are kept once for the whole workspace
//! (`openxer_automation::workspace_db`). Each open project holds a copy of
//! them in its document so the edit layer and the engine see one whole
//! file. After every edit step that changes those rows the active project's
//! copy is merged into the dictionary at once, the other open projects take
//! the new rows, and the change becomes part of the step's undo: undoing the
//! step puts back the rows it changed (and only those). Deleting a row that
//! another project uses is refused and the step is taken back.
use super::*;
use crate::store::Sync;
use openxer_xer::workspace_scope::delta::{fingerprint, Delta};
use std::collections::HashMap;

/// Ids of dictionary steps, apart from the baseline and EPS step ids.
const OP_BASE: u64 = 1 << 41;

/// One dictionary change, undone and redone with the project step it was
/// part of.
struct DictOp {
    delta: Delta,
    /// The workspace id of the project the change was made in.
    project: Option<i64>,
    /// Another workspace step (EPS, baselines, a new project) of the same
    /// project step.
    also: Option<u64>,
}

/// The active project's workspace id, history changes, records revision
/// and source name and project key.
type Checked = (Option<i64>, u64, u64, (String, i64));

#[derive(Default)]
pub struct DictSync {
    /// Whether the workspace keeps a shared dictionary; read once.
    shared: Option<bool>,
    /// The fingerprint of the dictionary rows each open project held when
    /// they last matched the dictionary, by source name and project key.
    synced: HashMap<(String, i64), u64>,
    /// The state of the active project last checked.
    checked: Option<Checked>,
    /// The dictionary revision the open projects hold.
    pub revision: i64,
    ops: HashMap<u64, DictOp>,
    next_op: u64,
}

fn key_of(p: &ProjectData) -> (String, i64) {
    (p.source_name.clone(), p.model.project.proj_id)
}

impl App {
    /// Whether the workspace keeps one shared dictionary (schema 3).
    pub(crate) fn dictionary_shared(&mut self) -> bool {
        if self.dict_sync.shared.is_none() {
            self.dict_sync.shared = Some(self.store.shared() == Ok(true));
            if let Ok(revision) = self.store.dictionary_revision() {
                self.dict_sync.revision = revision;
            }
        }
        self.dict_sync.shared == Some(true)
    }

    /// A project was just opened from the workspace: its rows are the
    /// dictionary's.
    pub(crate) fn forget_synced(&mut self, p: &ProjectData) {
        self.dict_sync.synced.remove(&key_of(p));
        self.dict_sync.checked = None;
    }

    /// Forget what is known about the dictionary (after a conversion).
    pub(crate) fn reset_dictionary_sync(&mut self) {
        self.dict_sync = DictSync::default();
    }

    /// Run after every frame's undo step is recorded: merge the active
    /// project's dictionary rows into the workspace when an edit changed
    /// them, and bring the other open projects up to the dictionary when it
    /// changed.
    pub(crate) fn sync_dictionary(&mut self, time: f64) {
        if self.open_set.empty || !self.dictionary_shared() {
            return;
        }
        let key = key_of(&self.project);
        let state = (
            self.open_set.active,
            self.history.changes(),
            self.records_revision,
            key.clone(),
        );
        if self.dict_sync.checked.as_ref() != Some(&state) {
            self.dict_sync.checked = Some(state);
            let fp = fingerprint(&self.project.document);
            match self.dict_sync.synced.get(&key) {
                // A project the workspace has not stored yet (a file just
                // opened): its rows join the dictionary as it is, outside
                // any undo step.
                None if !self.store.has_mark(&key) => {
                    self.push_initial();
                }
                None => {
                    self.dict_sync.synced.insert(key.clone(), fp);
                }
                Some(&old) if old == fp => {}
                Some(_) => self.push_dictionary(time),
            }
        }
        // Another writer (a save, the command line) changed the dictionary.
        let revision = self
            .store
            .dictionary_revision()
            .unwrap_or(self.dict_sync.revision);
        if revision != self.dict_sync.revision {
            self.dict_sync.revision = revision;
            let fp = fingerprint(&self.project.document);
            if self.dict_sync.synced.get(&key) == Some(&fp) {
                self.rehydrate_active();
            }
            self.rehydrate_parked();
        }
    }

    /// Merge the active project's changed dictionary rows into the workspace.
    fn push_dictionary(&mut self, time: f64) {
        let key = key_of(&self.project);
        let others: Vec<(Option<i64>, Document)> = self
            .open_set
            .parked
            .iter()
            .map(|p| (Some(p.id), p.project.document.clone()))
            .collect();
        let refs: Vec<(Option<i64>, &Document)> = others.iter().map(|(i, d)| (*i, d)).collect();
        let result = self
            .store
            .sync_dictionary(&self.project, self.open_set.active, &refs);
        match result {
            Err(e) => self.error(e),
            Ok(Sync::Refused(message)) => {
                // Take the step back; the rows stay.
                if self.history.undo(&mut self.project) {
                    self.history.drop_redo();
                    self.records_revision = self.history.revision();
                    self.open_set.steps_seen = self.history.steps();
                    self.dirty = true;
                }
                if fingerprint(&self.project.document) != self.dict_sync.synced[&key] {
                    self.rehydrate_active();
                }
                self.dict_sync.checked = None;
                self.error(message);
            }
            Ok(Sync::Done {
                delta,
                document,
                revision,
            }) => {
                if let Some(doc) = document {
                    self.take_document(doc);
                }
                if !delta.is_empty() {
                    self.record_dictionary_op(delta, time);
                }
                let fp = fingerprint(&self.project.document);
                self.dict_sync.synced.insert(key, fp);
                if revision != self.dict_sync.revision {
                    self.dict_sync.revision = revision;
                    self.rehydrate_parked();
                }
            }
        }
    }

    /// Merge a project the workspace has not stored yet into the
    /// dictionary, as saving it would, without an undo step.
    fn push_initial(&mut self) {
        let key = key_of(&self.project);
        match self
            .store
            .sync_dictionary(&self.project, self.open_set.active, &[])
        {
            Ok(Sync::Done {
                document, revision, ..
            }) => {
                if let Some(doc) = document {
                    self.take_document(doc);
                }
                if revision != self.dict_sync.revision {
                    self.dict_sync.revision = revision;
                    self.rehydrate_parked();
                }
            }
            Ok(Sync::Refused(_)) => {}
            Err(e) => self.error(e),
        }
        let fp = fingerprint(&self.project.document);
        self.dict_sync.synced.insert(key, fp);
    }

    /// The active project's document as the dictionary stored it (renamed
    /// copies, rows matched to stored ones).
    pub(crate) fn take_document(&mut self, doc: Document) {
        match crate::store::adopt_document(&mut self.project, doc) {
            Ok(()) => self.rebase_active(),
            Err(e) => self.error(e),
        }
    }

    /// Parked project `k` takes its document as the workspace stored it.
    pub(crate) fn take_parked_document(&mut self, k: usize, doc: Document) {
        self.records_serial += 1;
        let serial = self.records_serial;
        let p = &mut self.open_set.parked[k];
        match crate::store::adopt_document(&mut p.project, doc) {
            Ok(()) => {
                p.records_revision = serial;
                p.history.rebase(&p.project, serial);
                self.open_set.version += 1;
            }
            Err(e) => self.error(e),
        }
    }

    /// Attach a dictionary change to the undo step just recorded; changes
    /// made while typing into one field join one step.
    fn record_dictionary_op(&mut self, delta: Delta, _time: f64) {
        let active = self.open_set.active;
        let last = self.history.undo_external();
        if let Some(op) = last.and_then(|id| self.dict_sync.ops.get_mut(&id)) {
            if op.project == active {
                op.delta.then(delta);
                return;
            }
        }
        self.dict_sync.next_op += 1;
        let id = OP_BASE + self.dict_sync.next_op;
        let also = last.filter(|id| !self.dict_sync.ops.contains_key(id));
        self.dict_sync.ops.insert(
            id,
            DictOp {
                delta,
                project: active,
                also,
            },
        );
        self.history.tag_last(id);
    }

    /// Undo (`redo` false) or redo dictionary step `id`. `None` when the id
    /// is not a dictionary step; otherwise the other workspace step of the
    /// same project step, if any, for the caller to run.
    pub(crate) fn dictionary_history(&mut self, id: u64, redo: bool) -> Option<Option<u64>> {
        let op = self.dict_sync.ops.get(&id)?;
        let also = op.also;
        let delta = op.delta.clone();
        let project = op.project.or(self.open_set.active);
        // Every open project's uses count, saved or not.
        let mut open: Vec<(Option<i64>, &Document)> =
            vec![(self.open_set.active, &self.project.document)];
        open.extend(
            self.open_set
                .parked
                .iter()
                .map(|p| (Some(p.id), &p.project.document)),
        );
        if let Err(e) = self
            .store
            .apply_dictionary_delta(&delta, redo, project, &open)
        {
            self.error(e);
        }
        if let Ok(revision) = self.store.dictionary_revision() {
            self.dict_sync.revision = revision;
        }
        self.rehydrate_active();
        self.rehydrate_parked();
        Some(also)
    }

    /// After undo or redo in the active project: a step that did not change
    /// the dictionary may still bring back the rows the project held then;
    /// the project takes the dictionary's rows again.
    pub(crate) fn settle_dictionary(&mut self) {
        if !self.dictionary_shared() {
            return;
        }
        let key = key_of(&self.project);
        let fp = fingerprint(&self.project.document);
        if self.dict_sync.synced.get(&key).is_some_and(|&s| s != fp) {
            self.rehydrate_active();
        }
        self.dict_sync.checked = None;
    }

    /// The active project takes the dictionary's current rows.
    fn rehydrate_active(&mut self) {
        let key = key_of(&self.project);
        match self
            .store
            .rehydrate(&mut self.project, self.open_set.active)
        {
            Ok(()) => self.rebase_active(),
            Err(e) => self.error(e),
        }
        let fp = fingerprint(&self.project.document);
        self.dict_sync.synced.insert(key, fp);
    }

    /// A new records revision for the active project's caches, taken as its
    /// current state without an undo step of its own.
    fn rebase_active(&mut self) {
        self.records_serial += 1;
        self.records_revision = self.records_serial;
        self.history.rebase(&self.project, self.records_revision);
        self.open_set.version += 1;
    }

    /// The other open projects take the dictionary's current rows.
    pub(crate) fn rehydrate_parked(&mut self) {
        let mut errors = vec![];
        for k in 0..self.open_set.parked.len() {
            let id = self.open_set.parked[k].id;
            self.records_serial += 1;
            let serial = self.records_serial;
            let p = &mut self.open_set.parked[k];
            match self.store.rehydrate(&mut p.project, Some(id)) {
                Ok(()) => {
                    p.records_revision = serial;
                    p.history.rebase(&p.project, serial);
                }
                Err(e) => errors.push(format!("{}: {e}", p.project.model.project.name)),
            }
            let fp = fingerprint(&p.project.document);
            self.dict_sync.synced.insert(key_of(&p.project), fp);
        }
        self.open_set.version += 1;
        if !errors.is_empty() {
            self.error(errors.join("\n"));
        }
    }
}
