//! Several open projects. The active project stays in
//! `App.project`, so every command keeps addressing "the current project";
//! the other open projects are parked here with their own undo history. The
//! activity table and the Gantt read `App::shown()`, a composite of all open
//! projects whose first activities are the active project's.
use super::*;
pub mod compose;
mod dialogs;
mod numbers;
pub mod recovery;
pub mod schedule;
#[cfg(test)]
mod tests;

pub use dialogs::{DefaultDialog, OpenDialog};
use openxer_engine::options::OutOfSequence;

/// Workspace preference holding the open set.
pub const OPEN_SET_KEY: &str = "open_set_v1";

/// An activity across open projects: (workspace project id, task id).
pub type ActivityKey = (i64, i64);

/// An open project that is not the active one.
pub struct Parked {
    /// Workspace row id.
    pub id: i64,
    pub project: ProjectData,
    pub history: history::History,
    pub records_revision: u64,
    pub dirty: bool,
}

/// A relationship between an activity of the active project and one of
/// another project, as the Predecessors and Successors grids list it.
pub struct CrossLine {
    /// Workspace id of the project that owns the row (the successor's).
    pub owner: i64,
    /// The other activity when its project is open: (workspace id, slot, index).
    pub other: Option<(i64, usize, usize)>,
    pub other_project: String,
    pub other_task: i64,
    pub link: openxer_xer::ExternalLink,
}

/// The display composite of all open projects.
pub struct Composite {
    pub project: ProjectData,
    /// Per composite activity: (slot, activity index); slot 0 is the active
    /// project, slot k is `parked[k - 1]`.
    pub origin: Vec<(usize, usize)>,
    /// Activities of the active project when the composite was built.
    pub active_len: usize,
    stamp: (u64, u64, u64, usize),
}

#[derive(Default)]
pub struct OpenSet {
    /// No project is currently open (the user closed or deleted the last open
    /// project, or restarted an emptied workspace). `App.project` holds an
    /// unsaved empty placeholder until a project is opened, created or
    /// imported.
    pub empty: bool,
    /// Workspace id of the active project (`App.project`).
    pub active: Option<i64>,
    /// Workspace id of the Default Project.
    pub default: Option<i64>,
    pub parked: Vec<Parked>,
    /// Bumped by every change to the set or to a parked project.
    pub version: u64,
    pub composite: Option<Composite>,
    /// Undo journal: the projects each undo step touched, oldest first.
    pub undo: Vec<Vec<i64>>,
    pub redo: Vec<Vec<i64>>,
    /// The active history's step count last entered in the journal.
    pub steps_seen: u64,
    /// Undo histories of projects closed this session (`session_history`).
    pub closed: Vec<crate::session_history::Closed>,
    /// File > New steps that opened the project they added: step id to the
    /// workspace id of the project it was made in.
    pub created_from: std::collections::HashMap<u64, i64>,
    /// File > New steps undone: step id to the removed project's workspace
    /// id and its history then, for Redo to open it with.
    pub created_redo: std::collections::HashMap<u64, (i64, history::History)>,
    /// "Apply selected data date to all open projects" for the next F9
    /// (absolute minutes from serial day 0).
    #[cfg(test)]
    pub apply_data_date: Option<i64>,
    /// The next F9 schedules every open project with the Default Project's
    /// scheduling options (the answer Yes to `conflicting_options`).
    pub default_options_for_all: bool,
    pub open_dialog: Option<OpenDialog>,
    pub default_dialog: Option<DefaultDialog>,
    /// Timed recovery copies of unsaved changes (`recovery`).
    pub recovery: recovery::Recovery,
}

impl OpenSet {
    /// More than one project is open.
    pub fn multi(&self) -> bool {
        !self.empty && !self.parked.is_empty()
    }
}

/// An unsaved empty `ProjectData` kept in `App.project` while no project is
/// open (`OpenSet::empty`). It has no activities and is never written to the
/// workspace database.
pub(crate) fn empty_placeholder() -> ProjectData {
    let mut cal = openxer_xer::calendar_data::CalendarData::default();
    for day in 1..=5 {
        cal.week[day] = vec![(8 * 60, 12 * 60), (13 * 60, 17 * 60)];
    }
    let cal_text = cal.serialize();
    let text = format!(
        "ERMHDR\t25.12\t2026-01-01\tProject\tOpenXER\tOpenXER\tdbxDatabaseNoName\tProject Management\tUSD\n\
%T\tCALENDAR\n\
%F\tclndr_id\tdefault_flag\tclndr_name\tclndr_type\tday_hr_cnt\tweek_hr_cnt\tmonth_hr_cnt\tyear_hr_cnt\trsrc_private\tclndr_data\n\
%R\t1\tY\tStandard\tCA_Base\t8\t40\t172\t2000\tN\t{cal_text}\n\
%T\tPROJECT\n\
%F\tproj_id\tproj_short_name\tclndr_id\tplan_start_date\tlast_recalc_date\n\
%R\t1\t\t1\t2026-01-01 08:00\t2026-01-01 08:00\n\
%E\n"
    );
    let doc = openxer_xer::Document::parse(&text).expect("empty placeholder XER");
    ProjectData::from_document(doc, 1, String::new()).expect("empty placeholder loads")
}

impl App {
    /// True when at least one project is open.
    pub(crate) fn has_open_project(&self) -> bool {
        !self.open_set.empty
    }

    /// Leave the active project open state and enter an empty workspace
    /// (Projects view, no project open). The caller saves or discards the
    /// closing project first.
    pub(crate) fn enter_empty_workspace(&mut self) {
        self.project = empty_placeholder();
        self.open_set.empty = true;
        self.open_set.active = None;
        self.open_set.default = None;
        self.open_set.parked.clear();
        self.open_set.composite = None;
        self.open_set.undo.clear();
        self.open_set.redo.clear();
        self.open_set.steps_seen = 0;
        self.open_set.version += 1;
        self.history = history::History::new(&self.project);
        self.records_revision = 0;
        self.dirty = false;
        self.reset_project_ui();
        self.selected = None;
        self.query.clear();
        self.wbs_filter = None;
        self.collapsed.clear();
        self.date_edits.clear();
        self.data_date.clear();
        self.must_finish.clear();
        self.saved_projects = self.store.list().unwrap_or_default();
        self.views.projects.invalidate();
        self.show_view(views::MainView::Projects);
        self.store.clear_last();
        self.save_open_set();
    }
    /// What the activity table and the Gantt show: every open project.
    pub(crate) fn shown(&self) -> &ProjectData {
        self.open_set
            .composite
            .as_ref()
            .map_or(&self.project, |c| &c.project)
    }

    /// With several projects open: the count of composite indices that are
    /// the active project's own indices.
    pub(crate) fn active_rows(&self) -> Option<usize> {
        self.open_set
            .composite
            .as_ref()
            .map(|c| c.active_len.min(self.project.model.activities.len()))
    }

    /// Workspace ids of the open projects: the active one first.
    pub(crate) fn open_ids(&self) -> Vec<i64> {
        self.open_set
            .active
            .into_iter()
            .chain(self.open_set.parked.iter().map(|p| p.id))
            .collect()
    }

    pub(crate) fn is_open(&self, id: i64) -> bool {
        self.open_ids().contains(&id)
    }

    /// The open projects, the active one first (slot order).
    pub(crate) fn open_projects(&self) -> Vec<&ProjectData> {
        if self.open_set.empty {
            return vec![];
        }
        std::iter::once(&self.project)
            .chain(self.open_set.parked.iter().map(|p| &p.project))
            .collect()
    }

    /// Rebuild the composite when the active project, a parked project or
    /// the set changed.
    pub(crate) fn refresh_composite(&mut self) {
        if self.open_set.parked.is_empty() {
            self.open_set.composite = None;
            return;
        }
        let stamp = (
            self.history.changes(),
            self.records_revision,
            self.open_set.version,
            self.project.model.activities.len(),
        );
        if self
            .open_set
            .composite
            .as_ref()
            .is_some_and(|c| c.stamp == stamp)
        {
            return;
        }
        let (project, origin) = compose::compose(&self.open_projects());
        self.open_set.composite = Some(Composite {
            project,
            origin,
            active_len: self.project.model.activities.len(),
            stamp,
        });
    }

    /// The active project's index of composite row `i`, activating the
    /// project it belongs to first.
    pub(crate) fn local_row(&mut self, i: usize) -> Option<usize> {
        let Some(c) = &self.open_set.composite else {
            return (i < self.project.model.activities.len()).then_some(i);
        };
        let &(slot, local) = c.origin.get(i)?;
        if slot > 0 {
            self.activate_parked(slot - 1);
        }
        (local < self.project.model.activities.len()).then_some(local)
    }

    /// The workspace id of the active project, saving it first if needed.
    fn ensure_active_id(&mut self) -> Option<i64> {
        if self.open_set.empty {
            return None;
        }
        if self.open_set.active.is_none() {
            self.open_set.active = self.store.id_of(
                &self.project.source_name,
                self.project.model.project.proj_id,
            );
        }
        if self.open_set.active.is_none() {
            if let Ok(id) = self.store.save(&self.project) {
                self.open_set.active = Some(id);
                self.drop_stale_recovery_copies();
            }
        }
        self.open_set.active
    }

    /// Make `parked[k]` the active project; the active one is parked in its place.
    pub(crate) fn activate_parked(&mut self, k: usize) {
        if k >= self.open_set.parked.len() {
            return;
        }
        let Some(current) = self.ensure_active_id() else {
            return;
        };
        self.history
            .record(&self.project, self.records_revision, None, 0.);
        // Its dictionary changes reach the workspace before it is parked.
        self.sync_dictionary(0.);
        self.sync_journal();
        let other = &mut self.open_set.parked[k];
        std::mem::swap(&mut self.project, &mut other.project);
        std::mem::swap(&mut self.history, &mut other.history);
        std::mem::swap(&mut self.records_revision, &mut other.records_revision);
        std::mem::swap(&mut self.dirty, &mut other.dirty);
        let id = std::mem::replace(&mut other.id, current);
        self.open_set.active = Some(id);
        self.open_set.steps_seen = self.history.steps();
        self.open_set.version += 1;
        self.reset_project_ui();
        self.selected = None;
        self.refresh_composite();
        self.status = format!("{} is the current project", self.project.model.project.name);
        self.repair_add_sample_wbs();
    }

    /// Per-project editing state that does not carry over to another project.
    fn reset_project_ui(&mut self) {
        self.detail.picker = None;
        self.link_drag = None;
        self.link_editor = None;
        self.edit_cell = None;
        self.grid.pending_edit = None;
        self.date_edits.clear();
        self.editing.selection.clear();
        self.wbs_filter = None;
        self.data_date = date_string(
            self.project.model.origin_serial,
            self.project.model.project.data_date,
        );
        self.must_finish = date_string(
            self.project.model.origin_serial,
            self.project.model.project.plan_end,
        );
    }

    /// Activate the open project with workspace id `id`.
    pub(crate) fn activate_id(&mut self, id: i64) {
        if let Some(k) = self.open_set.parked.iter().position(|p| p.id == id) {
            self.activate_parked(k);
        }
    }

    /// Enter new undo steps of the active project in the journal.
    pub(crate) fn sync_journal(&mut self) {
        let steps = self.history.steps();
        if steps == self.open_set.steps_seen {
            return;
        }
        if let (Some(id), true) = (self.open_set.active, steps > self.open_set.steps_seen) {
            for _ in 0..(steps - self.open_set.steps_seen).min(64) {
                self.open_set.undo.push(vec![id]);
            }
            self.open_set.redo.clear();
        }
        self.open_set.steps_seen = steps;
    }

    /// Undo or redo the last journal entry across open projects. Returns
    /// false when the journal has nothing, so the caller undoes the active
    /// project alone.
    pub(crate) fn journal_move(&mut self, ctx: &egui::Context, redo: bool) -> bool {
        let entry = if redo {
            self.open_set.redo.pop()
        } else {
            self.open_set.undo.pop()
        };
        let Some(entry) = entry else {
            return false;
        };
        let active = self.open_set.active;
        let mut changed = false;
        let mut parked_steps = vec![];
        for id in &entry {
            if Some(*id) == active {
                changed |= self.active_history_step(ctx, redo);
            } else if let Some(p) = self.open_set.parked.iter_mut().find(|p| p.id == *id) {
                let external = if redo {
                    p.history.redo_external()
                } else {
                    p.history.undo_external()
                };
                let done = if redo {
                    p.history.redo(&mut p.project)
                } else {
                    p.history.undo(&mut p.project)
                };
                if done {
                    p.records_revision = p.history.revision();
                    p.dirty = true;
                    self.dirty = true;
                    changed = true;
                    parked_steps.extend(external);
                }
            }
        }
        // Dictionary changes of the parked projects' steps; the parked
        // projects then hold the dictionary's rows again.
        if changed && self.dictionary_shared() {
            for id in parked_steps {
                self.dictionary_history(id, redo);
            }
            self.rehydrate_parked();
        }
        if changed {
            self.open_set.version += 1;
            self.status = if redo { "Redo" } else { "Undo" }.into();
        }
        // Show the project the step changed.
        if !active.is_some_and(|a| entry.contains(&a)) {
            if let Some(&id) = entry.first() {
                self.activate_id(id);
            }
        }
        if redo {
            self.open_set.undo.push(entry);
        } else {
            self.open_set.redo.push(entry);
        }
        true
    }

    /// Forget the other open projects (after saving them), leaving the active one.
    pub(crate) fn close_parked(&mut self) {
        let mut parked = std::mem::take(&mut self.open_set.parked);
        for p in &mut parked {
            if p.dirty {
                if let Err(e) = self.store.save(&p.project) {
                    let e = self.save_failure(&p.project.model.project.name, &e);
                    self.error(e);
                }
            }
        }
        self.drop_stale_recovery_copies();
        self.open_set.composite = None;
        self.open_set.undo.clear();
        self.open_set.redo.clear();
        self.open_set.version += 1;
        self.open_set.default = self.open_set.active;
    }

    /// Save every changed parked project (the active one is saved by `save`).
    pub(crate) fn save_parked(&mut self) -> Result<(), String> {
        for k in 0..self.open_set.parked.len() {
            if self.open_set.parked[k].dirty {
                let saved = self.store.save_stored(&self.open_set.parked[k].project);
                let (id, stored) = match saved {
                    Ok(s) => s,
                    Err(e) => {
                        let e = self
                            .save_failure(&self.open_set.parked[k].project.model.project.name, &e);
                        // The projects saved before this one lose their copies.
                        self.drop_stale_recovery_copies();
                        return Err(e);
                    }
                };
                let p = &mut self.open_set.parked[k];
                p.id = id;
                p.dirty = false;
                if let Some(doc) = stored {
                    self.take_parked_document(k, doc);
                }
            }
        }
        // The active project's save follows in `try_save`, which drops the
        // copies of every project it saved.
        Ok(())
    }

    /// Workspace project `id` as the user sees it: the open copy with its
    /// unsaved changes when it is open, else the copy the workspace stores.
    pub(crate) fn open_or_stored(&self, id: i64) -> Result<ProjectData, String> {
        if Some(id) == self.open_set.active {
            return Ok(self.project.clone());
        }
        if let Some(p) = self.open_set.parked.iter().find(|p| p.id == id) {
            return Ok(p.project.clone());
        }
        self.store.load(id)
    }

    /// Workspace projects `ids` were rewritten in the workspace outside the
    /// edit layer (an import, Undo Last Import), after every open project
    /// was saved. Their open copies take the stored projects, with a new
    /// undo history (the change is undone with Undo Last Import, not Ctrl+Z),
    /// so a later save can never write the older copy back over it. An open
    /// project whose row is gone is closed. Returns false when that is the
    /// active project: the caller opens another one.
    pub(crate) fn reload_open_copies(&mut self, ids: &[i64]) -> bool {
        let mut active_kept = true;
        let mut changed = false;
        let active = self.open_set.active.or_else(|| {
            self.store.id_of(
                &self.project.source_name,
                self.project.model.project.proj_id,
            )
        });
        if let Some(id) = active.filter(|a| ids.contains(a)) {
            match self.store.load(id) {
                Ok(p) => {
                    self.forget_synced(&p);
                    self.project = p;
                    self.history = history::History::new(&self.project);
                    self.open_set.steps_seen = self.history.steps();
                    self.touch_records();
                    self.dirty = false;
                    self.open_set.active = Some(id);
                    self.reset_project_ui();
                    self.selected = (!self.project.model.activities.is_empty()).then_some(0);
                    self.drop_journal_of(Some(id));
                    self.drop_frame_cache();
                }
                Err(_) => active_kept = false,
            }
            changed = true;
        }
        let mut k = 0;
        while k < self.open_set.parked.len() {
            let id = self.open_set.parked[k].id;
            if !ids.contains(&id) {
                k += 1;
                continue;
            }
            changed = true;
            self.drop_journal_of(Some(id));
            match self.store.load(id) {
                Ok(p) => {
                    self.forget_synced(&p);
                    self.records_serial += 1;
                    let serial = self.records_serial;
                    let parked = &mut self.open_set.parked[k];
                    parked.history = history::History::new(&p);
                    parked.project = p;
                    parked.records_revision = serial;
                    parked.dirty = false;
                    k += 1;
                }
                Err(_) => {
                    self.open_set.parked.remove(k);
                    if self.open_set.default == Some(id) {
                        self.open_set.default = self.open_set.active;
                    }
                }
            }
        }
        if changed {
            self.open_set.version += 1;
            self.refresh_composite();
            if active_kept {
                self.save_open_set();
            }
        }
        // The import (or its undo) wrote these projects: copies of them
        // taken before are older than what the workspace now holds.
        self.drop_stale_recovery_copies();
        active_kept
    }

    /// Any open project has unsaved changes.
    pub(crate) fn any_dirty(&self) -> bool {
        self.dirty || self.open_set.parked.iter().any(|p| p.dirty)
    }

    /// Remember the open set in the workspace.
    pub(crate) fn save_open_set(&self) {
        let value = serde_json::json!({
            "open": self.open_ids(),
            "default": self.open_set.default,
            "active": self.open_set.active,
        });
        let _ = self.store.set_preference(OPEN_SET_KEY, &value.to_string());
    }

    /// Reopen the projects that were open with the active one last time.
    pub(crate) fn restore_open_set(&mut self) {
        if self.open_set.empty {
            return;
        }
        let Some(active) = self.ensure_active_id() else {
            return;
        };
        let Some(value) = self
            .store
            .preference(OPEN_SET_KEY)
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        else {
            self.open_set.default = Some(active);
            self.repair_add_sample_wbs();
            return;
        };
        let ids: Vec<i64> = value["open"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_i64()).collect())
            .unwrap_or_default();
        if ids.is_empty() && value["active"].is_null() {
            self.enter_empty_workspace();
            self.status = "No project open. Open a project from the Projects view.".into();
            return;
        }
        let mut notes = vec![];
        if ids.contains(&active) {
            notes = self.clear_number_clashes(&ids, &[active]);
            for id in ids.into_iter().filter(|&id| id != active) {
                if let Ok(p) = self.store.load(id) {
                    self.park(id, p);
                }
            }
        }
        self.open_set.default = value["default"]
            .as_i64()
            .filter(|&d| self.is_open(d))
            .or(Some(active));
        if let Some(a) = value["active"].as_i64() {
            self.activate_id(a);
        }
        self.refresh_composite();
        self.repair_add_sample_wbs();
        if !notes.is_empty() {
            self.status = notes.join(". ");
        }
    }

    fn park(&mut self, id: i64, project: ProjectData) {
        self.forget_synced(&project);
        let history = history::History::new(&project);
        self.open_set.parked.push(Parked {
            id,
            project,
            history,
            records_revision: 0,
            dirty: false,
        });
        self.open_set.version += 1;
    }

    /// Open exactly these workspace projects (File > Open Project). The
    /// active project stays active when it is among them.
    pub(crate) fn open_project_set(&mut self, ids: &[i64]) {
        if ids.is_empty() {
            return;
        }
        if self.any_dirty() {
            self.save();
            if self.any_dirty() {
                return;
            }
        }
        let active = self.ensure_active_id();
        // A project opened beside one with the same internal number gets a
        // number of its own first (`numbers`). Loading another active
        // project closes every open one.
        let staying: Vec<i64> = match active {
            Some(a) if ids.contains(&a) => self
                .open_ids()
                .into_iter()
                .filter(|id| ids.contains(id))
                .collect(),
            _ => vec![],
        };
        let notes = self.clear_number_clashes(ids, &staying);
        if !active.is_some_and(|a| ids.contains(&a)) {
            match self.store.load(ids[0]) {
                Ok(p) => self.load(p),
                Err(e) => {
                    self.error(e);
                    return;
                }
            }
        }
        let active = self.ensure_active_id();
        self.open_set.parked.retain(|p| ids.contains(&p.id));
        for &id in ids {
            if Some(id) == active || self.open_set.parked.iter().any(|p| p.id == id) {
                continue;
            }
            match self.store.load(id) {
                Ok(p) => self.park(id, p),
                Err(e) => self.error(e),
            }
        }
        if !self.open_set.default.is_some_and(|d| self.is_open(d)) {
            self.open_set.default = active;
        }
        self.open_set.undo.clear();
        self.open_set.redo.clear();
        self.open_set.version += 1;
        self.refresh_composite();
        self.save_open_set();
        let n = self.open_ids().len();
        let repaired = self
            .status
            .starts_with(crate::views::project_ops::SAMPLE_WBS_REPAIRED);
        if !repaired {
            self.status = if n > 1 {
                format!("{n} projects open")
            } else {
                format!("{} opened", self.project.model.project.name)
            };
        }
        for note in notes {
            self.status.push_str(&format!(". {note}"));
        }
    }

    /// Close the active project: another open project (the default one first)
    /// becomes active, or the workspace enters the empty (no open project)
    /// state on the Projects view.
    pub(crate) fn close_active_project(&mut self) {
        if self.open_set.empty {
            return;
        }
        if self.dirty {
            self.save();
            if self.dirty {
                return;
            }
        }
        let name = self.project.model.project.name.clone();
        if self.open_set.parked.is_empty() {
            self.keep_closing_history();
            self.enter_empty_workspace();
            self.status = format!("{name} closed");
            return;
        }
        let closing = self.open_set.active;
        let next = self
            .open_set
            .parked
            .iter()
            .position(|p| Some(p.id) == self.open_set.default)
            .unwrap_or(0);
        self.activate_parked(next);
        self.open_set.parked.retain(|p| Some(p.id) != closing);
        if self.open_set.default == closing || self.open_set.default.is_none() {
            self.open_set.default = self.open_set.active;
        }
        self.drop_journal_of(closing);
        self.open_set.version += 1;
        self.refresh_composite();
        self.save_open_set();
        self.status = format!("{name} closed");
    }

    /// Close open project `id` (Projects view, Close Project).
    pub(crate) fn close_project_id(&mut self, id: i64) {
        if Some(id) == self.open_set.active {
            self.close_active_project();
            return;
        }
        let Some(k) = self.open_set.parked.iter().position(|p| p.id == id) else {
            return;
        };
        if self.open_set.parked[k].dirty {
            if let Err(e) = self.store.save(&self.open_set.parked[k].project) {
                let e = self.save_failure(&self.open_set.parked[k].project.model.project.name, &e);
                self.error(e);
                return;
            }
            // Its older copy goes with the save, not at the next timed copy.
            self.drop_stale_recovery_copies();
        }
        let p = self.open_set.parked.remove(k);
        if self.open_set.default == Some(id) {
            self.open_set.default = self.open_set.active;
        }
        self.drop_journal_of(Some(id));
        self.open_set.version += 1;
        self.refresh_composite();
        self.save_open_set();
        self.status = format!("{} closed", p.project.model.project.name);
    }

    /// File > Close All: every project but the default one closes (OpenXER
    /// always shows one project).
    pub(crate) fn close_all_projects(&mut self) {
        if self.open_set.parked.is_empty() {
            self.status = "Only the default project is open".into();
            return;
        }
        if self.any_dirty() {
            self.save();
            if self.any_dirty() {
                return;
            }
        }
        if let Some(d) = self.open_set.default {
            self.activate_id(d);
        }
        let n = self.open_set.parked.len();
        self.close_parked();
        self.refresh_composite();
        self.save_open_set();
        self.status = format!("{n} projects closed; the default project stays open");
    }

    fn drop_journal_of(&mut self, id: Option<i64>) {
        for list in [&mut self.open_set.undo, &mut self.open_set.redo] {
            for entry in list.iter_mut() {
                entry.retain(|e| Some(*e) != id);
            }
            list.retain(|e| !e.is_empty());
        }
    }

    /// Project > Set Default Project.
    pub(crate) fn set_default_project(&mut self, id: i64) {
        if self.is_open(id) {
            self.open_set.default = Some(id);
            self.save_open_set();
            let name = self
                .open_projects()
                .into_iter()
                .zip(self.open_ids())
                .find(|(_, k)| *k == id)
                .map(|(p, _)| p.model.project.name.clone())
                .unwrap_or_default();
            self.status = format!("Default project: {name}");
        }
    }

    /// The Default Project's ID (short name), when several projects are open.
    pub(crate) fn default_project_name(&self) -> Option<String> {
        if !self.open_set.multi() {
            return None;
        }
        let default = self.open_set.default?;
        self.open_projects()
            .into_iter()
            .zip(self.open_ids())
            .find(|(_, id)| *id == default)
            .map(|(p, _)| p.model.project.name.clone())
    }

    /// Before adding an activity: make the Default Project active.
    pub(crate) fn activate_default(&mut self) {
        if let Some(d) = self.open_set.default {
            if self.open_set.active != Some(d) {
                self.activate_id(d);
            }
        }
    }

    /// Give activity `i` of the active project an id no other open project of
    /// the same file uses (each project keeps its own copy of the file).
    pub(crate) fn unique_new_activity(&mut self, i: usize) {
        if self.open_set.parked.is_empty() {
            return;
        }
        let source = self.project.source_name.clone();
        let mut used: std::collections::HashSet<i64> = std::collections::HashSet::new();
        for p in &self.open_set.parked {
            if p.project.source_name != source {
                continue;
            }
            used.extend(p.project.model.activities.iter().map(|a| a.task_id));
            if let Some(t) = p.project.document.table("TASK") {
                used.extend(
                    t.rows
                        .iter()
                        .filter_map(|r| t.value(r, "task_id").parse::<i64>().ok()),
                );
            }
        }
        let old = self.project.model.activities[i].task_id;
        if !used.contains(&old) {
            return;
        }
        let max = used
            .iter()
            .chain(self.project.model.activities.iter().map(|a| &a.task_id))
            .copied()
            .max()
            .unwrap_or(old);
        let id = max + 10;
        self.project.model.activities[i].task_id = id;
        if let Some(d) = self.project.details.remove(&old) {
            self.project.details.insert(id, d);
        }
        // The new activity's TASK row (its default types) follows.
        let pid = self.project.model.project.proj_id.to_string();
        let t = self.project.document.table_mut("TASK");
        for r in 0..t.rows.len() {
            let row = &t.rows[r];
            if t.value(row, "task_id") == old.to_string() && t.value(row, "proj_id") == pid {
                t.set(r, "task_id", id.to_string());
            }
        }
    }

    /// Slot of the Default Project among `open_ids` (0 when none is set).
    fn default_slot(&self, ids: &[i64]) -> usize {
        self.open_set
            .default
            .and_then(|d| ids.iter().position(|&id| id == d))
            .unwrap_or(0)
    }

    /// With several projects open whose scheduling options differ: the
    /// Default Project as "ID-name", for the question F9 asks first.
    /// The float path end activity names an activity of its own project,
    /// so it is not compared.
    pub(crate) fn conflicting_options(&self) -> Option<String> {
        if !self.open_set.multi() {
            return None;
        }
        let projects = self.open_projects();
        let default = &projects[self.default_slot(&self.open_ids())];
        let comparable = |p: &ProjectData| {
            let mut o = p.model.options.clone();
            o.float_paths_end_activity = None;
            o
        };
        let base = comparable(default);
        if projects.iter().all(|p| comparable(p) == base) {
            return None;
        }
        Some(format!(
            "{}-{}",
            default.model.project.name,
            crate::views::project_title(default)
        ))
    }
}

pub(crate) struct MultiScheduleJob {
    pub projects: Vec<ProjectData>,
    pub ids: Vec<i64>,
    pub default: usize,
    pub all_default: bool,
    pub op: &'static str,
}

pub(crate) struct MultiScheduleOut {
    pub projects: Vec<ProjectData>,
    pub ids: Vec<i64>,
    pub total_projects: usize,
    pub linked: usize,
    pub elapsed: f64,
}

pub(crate) fn log_start_multi(
    diag: &crate::diag_log::DiagLog,
    op: &str,
    projects: &[ProjectData],
    default: usize,
) {
    let opts = &projects[default].model.options;
    let acts: usize = projects.iter().map(|p| p.model.activities.len()).sum();
    let links: usize = projects.iter().map(|p| p.model.links.len()).sum();
    let rsrcs: usize = projects
        .iter()
        .map(|p| p.document.table("RSRC").map_or(0, |t| t.rows.len()))
        .sum();
    let has_plan_end = projects.iter().any(|p| p.model.project.plan_end.is_some());
    diag.line(&format!(
        "{op} state=start open_projects={} multi=true do_level={} multiple_float_paths={} float_paths_use_total={} max_float_paths={} open_end_uses_early={} has_plan_end={} activities={} links={} resources={}",
        projects.len(),
        opts.do_level,
        opts.multiple_float_paths,
        opts.float_paths_use_total,
        opts.max_float_paths,
        opts.open_end_uses_early,
        has_plan_end,
        acts,
        links,
        rsrcs,
    ));
}

pub(crate) fn run_multi_schedule_job(
    mut job: MultiScheduleJob,
    progress: Option<&std::sync::Mutex<crate::busy::Progress>>,
    cancel: &std::sync::atomic::AtomicBool,
    diag: &crate::diag_log::DiagLog,
) -> Option<Result<MultiScheduleOut, String>> {
    use std::sync::atomic::Ordering;
    let op = job.op;
    let now = Instant::now();
    let cancelled = || cancel.load(Ordering::Relaxed);
    let report = |phase: &str, fraction: f32| {
        if let Some(p) = progress {
            if let Ok(mut g) = p.lock() {
                g.phase = phase.to_string();
                g.fraction = Some(fraction);
            }
        }
    };

    if cancelled() {
        return None;
    }
    report("Refreshing derived durations...", 0.10);
    diag.phase_start(op, "refresh_derived_durations");
    let t = Instant::now();
    for p in &mut job.projects {
        p.refresh_derived_durations();
        if cancelled() {
            return None;
        }
    }
    diag.phase_done(op, "refresh_derived_durations", t.elapsed().as_millis());

    let options = job.projects[job.default].model.options.clone();
    // Yes to the conflicting options question: this run uses the Default
    // Project's options for every project (each keeps its own float path
    // end activity); the options each project stores do not change.
    let own_options: Vec<_> = job
        .projects
        .iter()
        .map(|p| p.model.options.clone())
        .collect();
    if job.all_default {
        for p in &mut job.projects {
            let end = p.model.options.float_paths_end_activity;
            p.model.options = options.clone();
            p.model.options.float_paths_end_activity = end;
        }
    }
    let groups = if options.out_of_sequence == OutOfSequence::None {
        (0..job.projects.len()).map(|k| vec![k]).collect()
    } else {
        compose::link_groups(&job.projects.iter().collect::<Vec<_>>())
    };
    let group_count = groups.len().max(1);
    for (gi, group) in groups.iter().enumerate() {
        if cancelled() {
            return None;
        }
        let frac = 0.22 + 0.56 * (gi as f32 / group_count as f32);
        if group_count > 1 {
            report(
                &format!("Scheduling group {} of {}...", gi + 1, group_count),
                frac,
            );
        } else {
            report("Scheduling activities...", frac);
        }
        let result = if let [k] = group[..] {
            // The Default Project's choice about other projects holds for
            // every project of the run; each keeps its other options.
            let p = &mut job.projects[k];
            let own = p.model.options.out_of_sequence;
            if options.out_of_sequence == OutOfSequence::None {
                p.model.options.out_of_sequence = OutOfSequence::None;
            }
            diag.phase_start(op, "validate");
            let tv = Instant::now();
            let valid = p.model.validate();
            diag.phase_done(op, "validate", tv.elapsed().as_millis());
            let result = match valid {
                Ok(()) => {
                    diag.phase_start(op, "schedule_model_report");
                    let ts = Instant::now();
                    let r = scheduler::schedule_with_resources_report(&mut p.model, &p.resources)
                        .map(|_| ());
                    diag.phase_done(op, "schedule_model_report", ts.elapsed().as_millis());
                    r
                }
                Err(e) => Err(e),
            };
            p.model.options.out_of_sequence = own;
            result
        } else {
            let run = if group.contains(&job.default) {
                options.clone()
            } else {
                job.projects[group[0]].model.options.clone()
            };
            let mut members: Vec<ProjectData> =
                group.iter().map(|&k| job.projects[k].clone()).collect();
            diag.phase_start(op, "schedule_model_report");
            let ts = Instant::now();
            let r = schedule::schedule_group(&mut members, &run).map(|_| {
                for (&k, p) in group.iter().zip(members) {
                    job.projects[k] = p;
                }
            });
            diag.phase_done(op, "schedule_model_report", ts.elapsed().as_millis());
            r
        };
        if let Err(e) = result {
            diag.error(op, &e);
            let names: Vec<String> = group
                .iter()
                .map(|&k| job.projects[k].model.project.name.clone())
                .collect();
            return Some(Err(format!("{}: {e}", names.join(", "))));
        }
    }
    if cancelled() {
        return None;
    }
    report("Updating assignment dates...", 0.84);
    diag.phase_start(op, "write_assignment_schedule");
    let t = Instant::now();
    let stamp = chrono::Local::now()
        .naive_local()
        .signed_duration_since(
            chrono::NaiveDate::from_ymd_opt(1899, 12, 30)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
        )
        .num_minutes();
    for (p, own) in job.projects.iter_mut().zip(own_options) {
        if job.all_default {
            p.model.options = own;
        }
        p.write_assignment_schedule();
        p.model.project.last_schedule = Some(stamp - p.model.origin_serial * DAY);
        if cancelled() {
            return None;
        }
    }
    diag.phase_done(op, "write_assignment_schedule", t.elapsed().as_millis());
    report("Finishing...", 0.96);
    let total_projects = groups.iter().map(Vec::len).sum::<usize>();
    let linked = groups.iter().filter(|g| g.len() > 1).count();
    Some(Ok(MultiScheduleOut {
        projects: job.projects,
        ids: job.ids,
        total_projects,
        linked,
        elapsed: now.elapsed().as_secs_f64(),
    }))
}

impl App {
    pub(crate) fn prepare_multi_schedule_job(
        &mut self,
        date: i64,
        finish: Option<i64>,
        apply: Option<i64>,
        all_default: bool,
        op: &'static str,
    ) -> MultiScheduleJob {
        let mut projects: Vec<ProjectData> = self.open_projects().into_iter().cloned().collect();
        if self.sched.progress.manual_level {
            projects[0].model.options.do_level = false;
        }
        projects[0].model.project.data_date = Some(date);
        projects[0].model.project.plan_end = finish;
        if let Some(absolute) = apply {
            for p in projects.iter_mut().skip(1) {
                p.model.project.data_date = Some(absolute - p.model.origin_serial * DAY);
            }
        }
        let ids = self.open_ids();
        let default = self.default_slot(&ids);
        MultiScheduleJob {
            projects,
            ids,
            default,
            all_default,
            op,
        }
    }

    pub(crate) fn commit_multi_schedule_output(
        &mut self,
        out: MultiScheduleOut,
        op: &str,
        close_schedule_dialog: bool,
        write_log_file: bool,
    ) {
        let diag = self.diag_log();
        let mut projects = out.projects.into_iter();
        self.project = projects.next().expect("the active project");
        self.sched.progress.manual_level = false;
        crate::scheduling::note_scheduled(&self.project.model);
        for (parked, p) in self.open_set.parked.iter_mut().zip(projects) {
            parked.project = p;
            crate::scheduling::note_scheduled(&parked.project.model);
            self.records_serial += 1;
            parked.records_revision = self.records_serial;
            parked
                .history
                .record(&parked.project, parked.records_revision, None, 0.);
            parked.dirty = true;
        }
        self.touch_records();
        self.history
            .record(&self.project, self.records_revision, None, 0.);
        self.open_set.steps_seen = self.history.steps();
        self.open_set.undo.push(out.ids);
        self.open_set.redo.clear();
        self.open_set.version += 1;
        let critical: usize = self
            .open_projects()
            .iter()
            .map(|p| crate::schedule_dialogs::critical_count(p))
            .sum();
        diag.phase_start(op, "schedule_log");
        let t = Instant::now();
        self.log = self.schedule_log(out.elapsed);
        diag.phase_done(op, "schedule_log", t.elapsed().as_millis());
        self.status = format!(
            "Schedule complete · {} projects{} · {critical} critical activities",
            out.total_projects,
            if out.linked > 0 {
                format!(", {} linked group(s) scheduled together", out.linked)
            } else {
                String::new()
            }
        );
        if close_schedule_dialog {
            self.sched.schedule_open = false;
        }
        self.show_schedule = false;
        self.draft_options = None;
        self.schedule_runs += 1;
        self.dirty = true;
        self.date_edits.clear();
        self.data_date = date_string(
            self.project.model.origin_serial,
            self.project.model.project.data_date,
        );
        if write_log_file && self.sched.log_to_file {
            if let Err(error) = crate::schedule_dialogs::write_log(
                Path::new(&self.sched.log_path),
                &self.log,
                self.sched.log_html,
                self.sched.log_append,
            ) {
                self.status = format!("Fail to create log file: {error}  Log disabled.");
            }
        }
        diag.phase_start(op, "save");
        let t = Instant::now();
        self.save();
        diag.phase_done(op, "save", t.elapsed().as_millis());
        self.refresh_composite();
        self.fit();
        diag.line(&format!(
            "{op} state=done elapsed_ms={} critical={critical}",
            (out.elapsed * 1000.0).round() as u128
        ));
    }

    /// F9 with several projects open: every open project is scheduled, the
    /// linked ones together as one network. `date` and
    /// `finish` are the active project's data date and must-finish date.
    #[cfg(test)]
    pub(crate) fn schedule_open_projects(&mut self, date: i64, finish: Option<i64>) {
        let apply = self.open_set.apply_data_date.take();
        let all_default = std::mem::take(&mut self.open_set.default_options_for_all);
        let job = self.prepare_multi_schedule_job(date, finish, apply, all_default, "schedule");
        let diag = self.diag_log();
        log_start_multi(&diag, "schedule", &job.projects, job.default);
        let cancel = std::sync::atomic::AtomicBool::new(false);
        match run_multi_schedule_job(job, None, &cancel, &diag) {
            Some(Ok(out)) => self.commit_multi_schedule_output(out, "schedule", false, false),
            Some(Err(e)) => self.error(e),
            None => {}
        }
    }
}

impl App {
    /// Relationships between activity `activity` of the active project and
    /// activities of other projects: its predecessors (`predecessor`) or its
    /// successors. Relationships to closed projects are listed too.
    pub(crate) fn cross_lines(&self, activity: usize, predecessor: bool) -> Vec<CrossLine> {
        let Some(task) = self
            .project
            .model
            .activities
            .get(activity)
            .map(|a| a.task_id)
        else {
            return vec![];
        };
        let projects = self.open_projects();
        let ids = self.open_ids();
        let find = |slot: usize, task: i64| {
            projects[slot]
                .model
                .activities
                .iter()
                .position(|a| a.task_id == task)
        };
        let mut out = vec![];
        if predecessor {
            for x in self.project.external_links() {
                if x.succ_task != task {
                    continue;
                }
                let other = compose::resolve(&projects, 0, x.pred_project)
                    .and_then(|slot| Some((slot, find(slot, x.pred_task)?)));
                out.push(CrossLine {
                    owner: ids.first().copied().unwrap_or(-1),
                    other: other.map(|(slot, i)| (ids[slot], slot, i)),
                    other_project: other.map_or_else(
                        || format!("{} (not open)", x.pred_project),
                        |(slot, _)| projects[slot].model.project.name.clone(),
                    ),
                    other_task: x.pred_task,
                    link: x,
                });
            }
        } else {
            for (slot, q) in projects.iter().enumerate().skip(1) {
                for x in q.external_links() {
                    if x.pred_task != task
                        || compose::resolve(&projects, slot, x.pred_project) != Some(0)
                    {
                        continue;
                    }
                    out.push(CrossLine {
                        owner: ids[slot],
                        other: find(slot, x.succ_task).map(|i| (ids[slot], slot, i)),
                        other_project: q.model.project.name.clone(),
                        other_task: x.succ_task,
                        link: x,
                    });
                }
            }
        }
        out
    }

    /// Change or (with `kind` = `None`) remove relationship row `row_id` of
    /// open project `owner` (the successor's project).
    pub(crate) fn change_cross_link(
        &mut self,
        owner: i64,
        row_id: i64,
        change: Option<(LinkType, i64)>,
    ) -> Result<(), String> {
        let apply = |p: &mut ProjectData| match change {
            Some((kind, lag)) => p.set_external_link(row_id, kind, lag),
            None => p.remove_external_link(row_id),
        };
        if Some(owner) == self.open_set.active {
            apply(&mut self.project)?;
            self.touch_records();
            return Ok(());
        }
        self.records_serial += 1;
        let serial = self.records_serial;
        let p = self
            .open_set
            .parked
            .iter_mut()
            .find(|p| p.id == owner)
            .ok_or("That project is not open")?;
        apply(&mut p.project)?;
        p.records_revision = serial;
        p.history.record(&p.project, serial, None, 0.);
        p.dirty = true;
        self.dirty = true;
        self.open_set.undo.push(vec![owner]);
        self.open_set.redo.clear();
        self.open_set.version += 1;
        Ok(())
    }

    /// Select activity `index` of open project `id`, activating it.
    pub(crate) fn goto_open_activity(&mut self, id: i64, index: usize) {
        if Some(id) != self.open_set.active {
            self.activate_id(id);
        }
        if index < self.project.model.activities.len() {
            self.jump_to(index);
        }
    }

    /// The (workspace id, task id) of composite activity `i`.
    pub(crate) fn activity_key(&self, i: usize) -> Option<ActivityKey> {
        let c = self.open_set.composite.as_ref()?;
        let &(slot, local) = c.origin.get(i)?;
        let ids = self.open_ids();
        let project = self.open_projects().get(slot).copied()?;
        Some((
            *ids.get(slot)?,
            project.model.activities.get(local)?.task_id,
        ))
    }

    /// A relationship drawn in the Gantt between composite activities: when
    /// either end is in another project, add it there and return the result;
    /// `None` when both are in the active project.
    pub(crate) fn cross_project_drop(
        &mut self,
        pred: usize,
        succ: usize,
        kind: LinkType,
    ) -> Option<Result<(), String>> {
        let n = self.active_rows()?;
        if pred < n && succ < n {
            return None;
        }
        let (Some(p), Some(s)) = (self.activity_key(pred), self.activity_key(succ)) else {
            return Some(Err("Activity not found".into()));
        };
        Some(self.add_cross_link(p, s, kind, 0))
    }

    /// Add a relationship from `pred` to `succ` (both `(workspace id, task
    /// id)`) between two open projects. It belongs to the successor's project.
    pub(crate) fn add_cross_link(
        &mut self,
        pred: ActivityKey,
        succ: ActivityKey,
        kind: LinkType,
        lag: i64,
    ) -> Result<(), String> {
        if pred.0 == succ.0 {
            return Err("Both activities are in the same project".into());
        }
        let pred_project = self
            .open_projects()
            .into_iter()
            .zip(self.open_ids())
            .find(|(_, id)| *id == pred.0)
            .map(|(p, _)| p.model.project.proj_id)
            .ok_or("The predecessor's project is not open")?;
        if Some(succ.0) == self.open_set.active {
            self.project
                .add_external_link(pred_project, pred.1, succ.1, kind, lag)?;
            self.touch_records();
        } else {
            self.records_serial += 1;
            let serial = self.records_serial;
            let p = self
                .open_set
                .parked
                .iter_mut()
                .find(|p| p.id == succ.0)
                .ok_or("The successor's project is not open")?;
            p.project
                .add_external_link(pred_project, pred.1, succ.1, kind, lag)?;
            p.records_revision = serial;
            p.history.record(&p.project, serial, None, 0.);
            p.dirty = true;
            self.open_set.undo.push(vec![succ.0]);
            self.open_set.redo.clear();
            self.open_set.version += 1;
        }
        Ok(())
    }
}
