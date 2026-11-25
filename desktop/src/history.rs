use crate::records::RecordSnapshot;
use openxer_engine::model::Model;
use openxer_xer::{ActivityDetails, Document, ProjectData, Table, Wbs};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[cfg(test)]
thread_local! {
    /// Whole-project comparisons made by `state_serial` on this thread, so a
    /// test can check how many a frame costs.
    pub(crate) static STATE_COMPARES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Every snapshot gets its own serial, so a cache can tell which project
/// state it was built from.
static SERIAL: AtomicU64 = AtomicU64::new(1);

/// Detail-table edits bump `revision`; comparing the counter avoids diffing tables every frame.
#[derive(Clone)]
struct Snapshot {
    model: Model,
    details: BTreeMap<i64, ActivityDetails>,
    wbs: Vec<Wbs>,
    revision: u64,
    records: RecordSnapshot,
    calendar_names: Vec<String>,
    /// Whole-project copy for bulk changes (a spreadsheet import) that also touch
    /// tables outside the detail records, such as deleted activity rows.
    full: Option<Arc<FullProject>>,
    /// A workspace change (baselines) made in the step from this snapshot to
    /// the next one; the caller undoes and redoes it by this id.
    external: Option<u64>,
    serial: u64,
}
impl Snapshot {
    fn from(project: &ProjectData, revision: u64, previous: Option<&Snapshot>) -> Self {
        // Detail tables only change through edits that bump the revision.
        let records = match previous {
            Some(p) if p.revision == revision => p.records.clone(),
            _ => RecordSnapshot::capture_shared(&project.document, previous.map(|p| &p.records)),
        };
        Self {
            model: project.model.clone(),
            details: project.details.clone(),
            wbs: project.wbs.clone(),
            revision,
            records,
            calendar_names: project.calendar_names.clone(),
            full: None,
            external: None,
            serial: SERIAL.fetch_add(1, Ordering::Relaxed),
        }
    }
    fn matches(&self, project: &ProjectData, revision: u64) -> bool {
        self.revision == revision && self.model == project.model && self.details == project.details
    }
    fn apply(&self, project: &mut ProjectData) {
        if let Some(full) = &self.full {
            *project = full.restore();
            return;
        }
        project.model = self.model.clone();
        project.details = self.details.clone();
        project.wbs = self.wbs.clone();
        self.records.restore(&mut project.document);
        project.calendar_names = self.calendar_names.clone();
    }
}

/// A whole project kept for a bulk undo step. Its document tables are shared
/// with the previous whole-project snapshot wherever they are unchanged: a
/// leveling run on a 3,000-activity project rewrites TASK and TASKRSRC but
/// leaves the other tables alone, and a full copy of every table per step
/// cost about 45 MB on corpus 369.
struct FullProject {
    /// The project with an empty table list.
    shell: ProjectData,
    tables: Vec<Arc<Table>>,
}

fn same_table(a: &Table, b: &Table) -> bool {
    a.name == b.name && a.fields == b.fields && a.rows == b.rows
}

impl FullProject {
    /// `shared` are tables already kept by other snapshots.
    fn new(p: &ProjectData, shared: &[Arc<Table>]) -> Self {
        let tables = p
            .document
            .tables
            .iter()
            .map(|t| {
                shared
                    .iter()
                    .find(|old| same_table(old, t))
                    .cloned()
                    .unwrap_or_else(|| Arc::new(t.clone()))
            })
            .collect();
        let shell = ProjectData {
            model: p.model.clone(),
            details: p.details.clone(),
            wbs: p.wbs.clone(),
            resource_names: p.resource_names.clone(),
            calendar_names: p.calendar_names.clone(),
            document: Document {
                header: p.document.header.clone(),
                tables: vec![],
                windows_encoding: p.document.windows_encoding,
                warnings: p.document.warnings.clone(),
                lone_quotes: p.document.lone_quotes.clone(),
                id_floor: p.document.id_floor.clone(),
            },
            source_name: p.source_name.clone(),
            warnings: p.warnings.clone(),
            resources: p.resources.clone(),
        };
        Self { shell, tables }
    }
    fn restore(&self) -> ProjectData {
        let mut p = self.shell.clone();
        p.document.tables = self.tables.iter().map(|t| Table::clone(t)).collect();
        p
    }
}

pub struct History {
    current: Snapshot,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    merge: Option<u64>,
    last_change: f64,
    /// Undo steps started so far (a typing merge adds none).
    steps: u64,
    /// Changes of the current state (new or merged steps, undo, redo).
    changes: u64,
}
impl History {
    pub fn new(project: &ProjectData) -> Self {
        Self {
            current: Snapshot::from(project, 0, None),
            undo: vec![],
            redo: vec![],
            merge: None,
            last_change: 0.,
            steps: 0,
            changes: 0,
        }
    }
    /// The next change starts its own undo step, even while the same field
    /// keeps the keyboard (a formatting command in a notes pane).
    pub fn seal(&mut self) {
        self.merge = None;
    }
    /// Undo steps started since this history was created.
    pub fn steps(&self) -> u64 {
        self.steps
    }
    /// Changes of the recorded state since this history was created.
    pub fn changes(&self) -> u64 {
        self.changes
    }
    /// The serial of the current snapshot when the project is exactly that
    /// snapshot (model, details, detail tables, WBS and calendar names), so
    /// anything derived from the project alone can be reused while the
    /// serial stays the same.
    pub fn state_serial(&self, project: &ProjectData, revision: u64) -> Option<u64> {
        #[cfg(test)]
        STATE_COMPARES.with(|n| n.set(n.get() + 1));
        let c = &self.current;
        (c.matches(project, revision)
            && c.wbs == project.wbs
            && c.calendar_names == project.calendar_names)
            .then_some(c.serial)
    }
    pub fn counts(&self) -> (usize, usize) {
        (self.undo.len(), self.redo.len())
    }
    /// Revision of the detail tables in the current snapshot (restored by undo/redo).
    pub fn revision(&self) -> u64 {
        self.current.revision
    }
    /// Whether `project` is the state last recorded (nothing changed since).
    pub fn is_current(&self, project: &ProjectData, revision: u64) -> bool {
        self.current.matches(project, revision)
    }
    /// The scheduling model as last recorded.
    pub fn latest_model(&self) -> &Model {
        &self.current.model
    }
    pub fn record(&mut self, project: &ProjectData, revision: u64, merge: Option<u64>, time: f64) {
        if self.current.matches(project, revision) {
            return;
        }
        if !(merge.is_some() && merge == self.merge && time - self.last_change < 0.8) {
            self.undo.push(self.step_start());
            if self.undo.len() > 64 {
                self.undo.remove(0);
            }
            self.steps += 1;
        }
        self.changes += 1;
        self.current = Snapshot::from(project, revision, Some(&self.current));
        self.redo.clear();
        self.merge = merge;
        self.last_change = time;
    }
    /// Record a bulk change as one undo step. `before` is the project as it was
    /// just before the change; both states are kept whole so undo and redo restore
    /// every table, not only the scheduling model and detail records.
    pub fn record_bulk(
        &mut self,
        before: &ProjectData,
        before_revision: u64,
        after: &ProjectData,
        revision: u64,
        time: f64,
    ) {
        self.record(before, before_revision, None, time);
        let previous = self.latest_full();
        let shared = previous
            .iter()
            .flat_map(|f| f.tables.iter())
            .chain(self.current.records.tables())
            .cloned()
            .collect::<Vec<_>>();
        let first = Arc::new(FullProject::new(before, &shared));
        self.current.full = Some(first.clone());
        let start = self.step_start();
        self.undo.push(start);
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
        self.steps += 1;
        self.changes += 1;
        let mut next = Snapshot::from(after, revision, Some(&self.current));
        let shared: Vec<_> = first
            .tables
            .iter()
            .chain(next.records.tables())
            .cloned()
            .collect();
        next.full = Some(Arc::new(FullProject::new(after, &shared)));
        self.current = next;
        self.redo.clear();
        self.merge = None;
        self.last_change = time;
    }
    /// The newest whole-project snapshot, to share unchanged tables with.
    fn latest_full(&self) -> Option<Arc<FullProject>> {
        std::iter::once(&self.current)
            .chain(self.undo.iter().rev())
            .chain(self.redo.iter())
            .find_map(|s| s.full.clone())
    }
    /// The current snapshot as the start of a new step; a workspace change
    /// marked on it belonged to an undone step and no longer applies.
    fn step_start(&self) -> Snapshot {
        let mut s = self.current.clone();
        s.external = None;
        s
    }
    /// Record a workspace change (with any project change it made) as one
    /// undo step. Call `record` before making the change, so the current
    /// snapshot is the state before it.
    pub fn record_external(&mut self, project: &ProjectData, revision: u64, id: u64, time: f64) {
        let mut start = self.step_start();
        start.external = Some(id);
        self.undo.push(start);
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
        self.steps += 1;
        self.changes += 1;
        self.current = Snapshot::from(project, revision, Some(&self.current));
        self.redo.clear();
        self.merge = None;
        self.last_change = time;
    }
    /// Attach a workspace change to the step just recorded, so one undo
    /// reverts both.
    pub fn tag_last(&mut self, id: u64) {
        if let Some(s) = self.undo.last_mut() {
            s.external = Some(id);
        }
    }
    /// Take `project` as the current state without a new undo step: the
    /// workspace dictionary rows it holds were replaced by another project's
    /// change, which that project's history undoes.
    pub fn rebase(&mut self, project: &ProjectData, revision: u64) {
        let records =
            RecordSnapshot::capture_shared(&project.document, Some(&self.current.records));
        let mut next = Snapshot::from(project, revision, Some(&self.current));
        next.records = records;
        next.external = self.current.external;
        // A bulk step's whole-project copy stays; undo and redo put the
        // dictionary rows back from the workspace afterwards.
        next.full = self.current.full.clone();
        self.current = next;
    }
    /// Forget the step just undone (an edit that was refused), so Redo
    /// cannot bring it back.
    pub fn drop_redo(&mut self) {
        self.redo.pop();
    }
    /// The workspace change the next undo reverts, if any.
    pub fn undo_external(&self) -> Option<u64> {
        self.undo.last().and_then(|s| s.external)
    }
    /// The workspace change the next redo repeats, if any.
    pub fn redo_external(&self) -> Option<u64> {
        if self.redo.is_empty() {
            None
        } else {
            self.current.external
        }
    }
    pub fn undo(&mut self, project: &mut ProjectData) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.current.clone());
        previous.apply(project);
        self.current = previous;
        self.merge = None;
        self.changes += 1;
        true
    }
    pub fn redo(&mut self, project: &mut ProjectData) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.current.clone());
        next.apply(project);
        self.current = next;
        self.merge = None;
        self.changes += 1;
        true
    }
}
impl History {
    /// Relationships made since the oldest undo step still kept, each with
    /// the step that first had it (larger is later).
    pub fn link_additions(
        &self,
    ) -> std::collections::HashMap<crate::loops::analysis::LinkKey, u32> {
        use crate::loops::analysis::link_key;
        let mut out = std::collections::HashMap::new();
        let snaps: Vec<&Snapshot> = self
            .undo
            .iter()
            .chain(std::iter::once(&self.current))
            .collect();
        let base: std::collections::HashSet<_> = snaps[0]
            .model
            .links
            .iter()
            .map(|l| link_key(&snaps[0].model, l))
            .collect();
        for (step, s) in snaps.iter().enumerate().skip(1) {
            for l in &s.model.links {
                let key = link_key(&s.model, l);
                if !base.contains(&key) {
                    out.entry(key).or_insert(step as u32);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn relationship_delete_and_activity_edit_can_be_undone_and_redone() {
        let mut p = openxer_xer::starter();
        let original = p.model.clone();
        let name = p.details[&1010].name.clone();
        let mut h = History::new(&p);
        crate::relationships::remove(&mut p.model, 0).unwrap();
        h.record(&p, 0, None, 1.);
        p.details.get_mut(&1010).unwrap().name = "Changed".into();
        h.record(&p, 0, Some(1), 2.);
        p.details.get_mut(&1010).unwrap().name = "Changed name".into();
        h.record(&p, 0, Some(1), 2.2);
        assert_eq!(h.counts(), (2, 0));
        assert!(h.undo(&mut p));
        assert_eq!(p.details[&1010].name, name);
        assert!(h.undo(&mut p));
        assert_eq!(p.model, original);
        assert!(h.redo(&mut p));
        assert_eq!(p.model.links.len(), original.links.len() - 1);
        assert!(h.redo(&mut p));
        assert_eq!(p.details[&1010].name, "Changed name");
    }
    #[test]
    fn bulk_change_undoes_and_redoes_whole_project() {
        let mut p = openxer_xer::starter();
        let mut h = History::new(&p);
        let before = p.clone();
        let removed = p.model.activities.len() - 1;
        let id = p.model.activities[removed].task_id;
        p.model.activities.remove(removed);
        p.model
            .links
            .retain(|l| l.pred != removed && l.succ != removed);
        p.model.rebuild_links().unwrap();
        let t = p.document.table_mut("TASK");
        let col = t.field("task_id").unwrap();
        t.rows.retain(|r| r[col] != id.to_string());
        h.record_bulk(&before, 0, &p, 1, 1.);
        assert_eq!(h.counts(), (1, 0));
        assert!(h.undo(&mut p));
        assert_eq!(p.model, before.model);
        assert_eq!(p.document.table("TASK").unwrap().rows.len(), 14);
        assert!(h.redo(&mut p));
        assert_eq!(p.document.table("TASK").unwrap().rows.len(), 13);
        // A later ordinary edit undoes on top of the bulk state.
        p.details.get_mut(&1000).unwrap().name = "After".into();
        h.record(&p, 1, None, 3.);
        assert!(h.undo(&mut p));
        assert_eq!(p.document.table("TASK").unwrap().rows.len(), 13);
        assert_ne!(p.details[&1000].name, "After");
    }
    #[test]
    fn detail_table_edits_undo_by_revision() {
        let mut p = openxer_xer::starter();
        let mut h = History::new(&p);
        crate::records::add_row(&mut p.document, "TASKNOTE", &[("task_id", "1000".into())]);
        h.record(&p, 1, None, 1.);
        assert_eq!(h.counts(), (1, 0));
        assert!(h.undo(&mut p));
        assert_eq!(h.revision(), 0);
        assert!(p.document.table("TASKNOTE").is_none());
        assert!(h.redo(&mut p));
        assert_eq!(h.revision(), 1);
        assert_eq!(p.document.table("TASKNOTE").unwrap().rows.len(), 1);
    }
    #[test]
    fn workspace_steps_carry_their_id_through_undo_and_redo() {
        let mut p = openxer_xer::starter();
        let mut h = History::new(&p);
        p.details.get_mut(&1000).unwrap().name = "Edited".into();
        h.record(&p, 0, None, 1.);
        h.record_external(&p, 0, 7, 2.);
        p.details.get_mut(&1000).unwrap().name = "Edited again".into();
        h.record(&p, 0, None, 3.);
        h.tag_last(8);
        assert_eq!(h.counts(), (3, 0));
        assert_eq!(h.undo_external(), Some(8));
        assert!(h.undo(&mut p));
        assert_eq!(p.details[&1000].name, "Edited");
        assert_eq!(h.redo_external(), Some(8));
        assert_eq!(h.undo_external(), Some(7));
        assert!(h.undo(&mut p));
        assert_eq!(
            p.details[&1000].name, "Edited",
            "a workspace step leaves the project"
        );
        assert_eq!(h.undo_external(), None);
        assert_eq!(h.redo_external(), Some(7));
        assert!(h.redo(&mut p));
        assert_eq!(h.redo_external(), Some(8));
        // A new edit after an undo starts a plain step.
        assert!(h.undo(&mut p));
        p.details.get_mut(&1010).unwrap().name = "New".into();
        h.record(&p, 0, None, 4.);
        assert_eq!(h.undo_external(), None);
        assert_eq!(h.redo_external(), None);
    }
    #[test]
    fn wbs_structure_edits_undo_and_redo() {
        let mut p = openxer_xer::starter();
        let mut h = History::new(&p);
        let before = p.wbs.len();
        let rows = |p: &ProjectData| p.document.table("PROJWBS").map_or(0, |t| t.rows.len());
        let rows_before = rows(&p);
        let last = p.wbs.last().map(|w| w.id);
        let id = crate::views::wbs::add(&mut p, last).unwrap();
        h.record(&p, 1, None, 1.);
        assert_eq!(p.wbs.len(), before + 1);
        assert!(h.undo(&mut p));
        assert_eq!(p.wbs.len(), before);
        assert!(p.wbs.iter().all(|w| w.id != id));
        assert_eq!(rows(&p), rows_before, "PROJWBS rows restored with the list");
        assert!(h.redo(&mut p));
        assert!(p.wbs.iter().any(|w| w.id == id));
    }
}
