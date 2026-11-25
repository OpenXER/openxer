//! Baselines: frozen copies of a project kept in the workspace beside it.
//!
//! Project > Maintain Baselines adds (a copy of the open project, or another
//! workspace project converted), copies, deletes, updates and restores them
//! (Restore detaches a baseline into a separate project) and edits their
//! ID, name and type; Project > Assign Baselines picks the project baseline
//! and the Primary, Secondary and Tertiary user baselines; Project > Revert
//! Project to Baseline gives the open project a baseline's data. The rules
//! are `openxer_automation::baseline_ops`, shared with the CLI and MCP. The
//! activity table's BL columns, the Gantt baseline bars, the status bar,
//! Check Schedule and the progress line read the assigned baselines through
//! `Compare`.
//!
//! Every change is one undo step: the workspace rows before and after are
//! kept in an `Op`, and the project history carries the op's id.
use super::*;
use openxer_automation::baseline_ops as ops;
use openxer_interchange::baseline as xb;
use openxer_interchange::baseline_update as xu;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

mod columns;
pub mod data;
pub(crate) mod dialogs;
pub mod store;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_m4;
pub(crate) mod update_dialog;

pub use columns::{column_value, columns};
pub use data::{BlField, Compare, Slot};
use store::{BaselineRow, Choice, ProjectRow, UserBaselines};

/// One undoable change to the workspace's baselines: the shared change
/// (each row as it was and as it is after; `None` means absent).
#[derive(Clone, Debug, Default)]
pub struct Op {
    change: ops::Change,
    /// Deleting the first-run sample: the dictionary change that takes out
    /// the rows only it used, and its sample record, put back on undo.
    sample: Option<(
        openxer_xer::workspace_scope::delta::Delta,
        openxer_automation::workspace_db::SampleRecord,
    )>,
    /// Removed projects other than `change.parent`: their Primary,
    /// Secondary and Tertiary baseline choices, put back on undo (the rows
    /// cascade with the project, so redo has nothing to write).
    users: Vec<(i64, UserBaselines)>,
}

impl From<ops::Change> for Op {
    fn from(change: ops::Change) -> Self {
        Op {
            change,
            sample: None,
            users: vec![],
        }
    }
}

type CompareKey = (Option<i64>, u64, String, i64);
/// (source name, project id, revision) of the cached workspace row lookup.
type ParentKey = (String, i64, u64);

#[derive(Default)]
pub struct BaselineState {
    pub(crate) maintain: Option<dialogs::Maintain>,
    pub(crate) assign: Option<dialogs::Assign>,
    pub(crate) add: Option<dialogs::AddChoice>,
    pub(crate) select: Option<dialogs::SelectProject>,
    pub(crate) update: Option<update_dialog::UpdateDialog>,
    /// Update Baseline > Preview.
    pub(crate) preview: Option<update_dialog::PreviewView>,
    /// The filter picker of Update Baseline's Activities page.
    pub(crate) filter_pick: Option<update_dialog::FilterPick>,
    /// The message after an update: the log written, if any.
    pub(crate) update_done: Option<Option<std::path::PathBuf>>,
    /// The update log shown by View Log: (path, text).
    pub(crate) log_view: Option<(std::path::PathBuf, String)>,
    pub(crate) confirm: Option<dialogs::Confirm>,
    /// Assign Baselines' lookup of one slot.
    pub(crate) lookup: Option<dialogs::Lookup>,
    /// Project > Revert Project to Baseline.
    pub(crate) revert: Option<dialogs::Revert>,
    /// A message answered with OK only (a refusal, or Delete's done note).
    pub(crate) notice: Option<String>,
    /// Bumped by every baseline change; cached comparisons are rebuilt.
    revision: u64,
    /// The earned value date choice: which dates of a baseline the BL date
    /// columns and earned value read. Kept in the workspace.
    basis: data::DateBasis,
    ops: BTreeMap<u64, Op>,
    next_op: u64,
    parent: RefCell<Option<(ParentKey, Option<i64>)>>,
    /// Baselines read from the workspace by (row id, revision, origin): any
    /// baseline change (an update, a rename, a renumber, undo) bumps the
    /// revision, so a row is read again after it.
    loaded: RefCell<HashMap<(i64, u64, i64), Arc<data::Loaded>>>,
    compare: RefCell<Option<(CompareKey, Arc<Compare>)>>,
}

impl BaselineState {
    /// Any baseline dialog is open.
    pub(crate) fn any_open(&self) -> bool {
        self.maintain.is_some()
            || self.assign.is_some()
            || self.add.is_some()
            || self.select.is_some()
            || self.update.is_some()
            || self.preview.is_some()
            || self.filter_pick.is_some()
            || self.update_done.is_some()
            || self.log_view.is_some()
            || self.confirm.is_some()
            || self.lookup.is_some()
            || self.revert.is_some()
            || self.notice.is_some()
    }
}

pub use xu::UpdateOptions;

/// Workspace preference holding the Update Baseline options.
const UPDATE_OPTIONS_KEY: &str = "update_baseline_options";

/// The update log's default place: the user's Documents folder (a test
/// build uses the temporary folder, so no test writes to Documents).
#[cfg(test)]
fn default_update_log() -> String {
    std::env::temp_dir()
        .join("openxer-test-UpdateBaselineLog.txt")
        .display()
        .to_string()
}

/// The update log's default place: the user's Documents folder.
#[cfg(not(test))]
fn default_update_log() -> String {
    std::env::var_os("USERPROFILE")
        .map(|p| std::path::PathBuf::from(p).join("Documents"))
        .unwrap_or_else(std::env::temp_dir)
        .join("UpdateBaselineLog.txt")
        .display()
        .to_string()
}

/// The id of baseline type `stored` in the document's BASETYPE rows, or an
/// empty text. `stored` is an id; a name (older workspaces kept the name) is
/// looked up too.
fn type_in_document(doc: &Document, stored: &str) -> String {
    ops::type_id(doc, stored)
}

fn now_stamp() -> String {
    ops::now_stamp()
}

fn parse(row: &BaselineRow) -> Result<Document, String> {
    ops::parse(row)
}

fn next_key(doc: &Document, pid: i64, rows: &[BaselineRow]) -> i64 {
    ops::next_key(doc, pid, rows)
}

/// Store `plan` (a scheduled copy of `project` before any progress) as a
/// baseline of workspace project `parent`, make it the project baseline and
/// the Primary baseline, and point `project` at it. The first-run sample
/// gets its as-planned copy this way. Returns the baseline's row id.
pub(crate) fn add_plan_baseline(
    c: &rusqlite::Connection,
    parent: i64,
    project: &mut ProjectData,
    plan: &Document,
    name: &str,
    stamp: &str,
) -> Result<i64, String> {
    store::ensure(c)?;
    let pid = project.model.project.proj_id;
    let rows = store::list(c, parent)?;
    let key = next_key(&project.updated_document(), pid, &rows);
    let short = format!("{} - B1", project.model.project.name);
    let copy = xb::copy_as_baseline(plan, pid, key, &short, name, stamp);
    let info = xb::baseline_info(&copy, key);
    let mut row = BaselineRow {
        id: 0,
        parent,
        project_key: key,
        short_name: short,
        name: name.into(),
        baseline_type: String::new(),
        data_date: info.data_date,
        added: stamp.into(),
        updated: stamp.into(),
        xer: copy.text(),
        windows_encoding: project.document.windows_encoding,
        import_batch: None,
    };
    row.id = store::put(c, &row)?;
    xb::set_project_value(
        &mut project.document,
        pid,
        "sum_base_proj_id",
        &key.to_string(),
    );
    store::set_user_baselines(
        c,
        parent,
        UserBaselines([Choice::Baseline(row.id), Choice::None, Choice::None]),
    )?;
    Ok(row.id)
}

/// Store the baselines that a file carries for project `pid` (PROJECT rows
/// whose `orig_proj_id` is `pid`) under workspace project `parent`. Baselines
/// already stored with the same project id are kept as they are. Returns how
/// many were added.
pub(crate) fn attach_file_baselines(
    c: &rusqlite::Connection,
    parent: i64,
    doc: &Document,
    pid: i64,
    import_batch: Option<i64>,
) -> Result<usize, String> {
    let existing = store::list(c, parent)?;
    let mut added = 0;
    for (key, owner) in xb::baselines_in(doc) {
        if owner != pid || existing.iter().any(|r| r.project_key == key) {
            continue;
        }
        let copy = xb::extract_project(doc, key);
        // Refuse a copy that does not load as a project.
        ProjectData::from_document(copy.clone(), key, String::new())?;
        let info = xb::baseline_info(&copy, key);
        let baseline_type = xb::project_value(&copy, key, "base_type_id")
            .trim()
            .to_string();
        store::put(
            c,
            &BaselineRow {
                id: 0,
                parent,
                project_key: key,
                short_name: info.short_name,
                name: info.name,
                baseline_type,
                data_date: info.data_date,
                added: info.added,
                // A baseline never updated (one converted from a project)
                // keeps its Last Update Date blank.
                updated: xb::project_value(&copy, key, "last_baseline_update_date")
                    .trim()
                    .to_string(),
                xer: copy.text(),
                windows_encoding: doc.windows_encoding,
                import_batch,
            },
        )?;
        added += 1;
    }
    Ok(added)
}

/// Append the stored baselines `ids` of workspace project `parent` (project
/// `pid` in `doc`) to an export. The parent's project baseline field follows
/// a baseline whose id changed on the way. Returns how many were written.
pub(crate) fn export_baselines(
    c: &rusqlite::Connection,
    doc: &mut Document,
    parent: i64,
    pid: i64,
) -> Result<usize, String> {
    let rows = store::list(c, parent)?;
    let assigned = xb::project_value(doc, pid, "sum_base_proj_id");
    let mut written = 0;
    for meta in rows {
        let Some(row) = store::get(c, meta.id)? else {
            continue;
        };
        let mut bl = parse(&row)?;
        // The baseline's type is named by id; the parent's BASETYPE rows are
        // the dictionary, so the copy's own rows are dropped.
        bl.tables.retain(|t| t.name != "BASETYPE");
        let known = type_in_document(doc, &row.baseline_type);
        xb::set_project_value(&mut bl, row.project_key, "base_type_id", &known);
        xb::mark_baseline(
            &mut bl,
            row.project_key,
            pid,
            &row.short_name,
            &row.name,
            &row.added,
        );
        xb::set_project_value(
            &mut bl,
            row.project_key,
            "last_baseline_update_date",
            &row.updated,
        );
        let id = xb::append_project(doc, &bl, row.project_key);
        if assigned.trim() == row.project_key.to_string() && id != row.project_key {
            xb::set_project_value(doc, pid, "sum_base_proj_id", &id.to_string());
        }
        written += 1;
    }
    Ok(written)
}

/// Workspace preference holding the earned value date choice.
const EV_DATES_KEY: &str = "earned_value_baseline_dates";

impl App {
    // ------------------------------------------------------------ lookups

    /// Read the earned value date choice from the workspace.
    pub(crate) fn restore_baseline_prefs(&mut self) {
        if let Ok(Some(text)) = self.store.preference(EV_DATES_KEY) {
            self.baselines.basis = data::DateBasis::from_key(&text);
        }
    }

    /// The dates of a baseline that BL date columns and earned value read.
    pub(crate) fn date_basis(&self) -> data::DateBasis {
        self.baselines.basis
    }

    /// Admin Preferences > Earned Value: choose planned or current dates of
    /// the baseline. Stored in the workspace, applied at once.
    pub(crate) fn set_date_basis(&mut self, basis: data::DateBasis) {
        if basis == self.baselines.basis {
            return;
        }
        if let Err(e) = self.store.set_preference(EV_DATES_KEY, basis.key()) {
            self.error(e);
            return;
        }
        self.baselines.basis = basis;
        self.baselines.revision += 1;
    }

    /// The open project's workspace row, if it has been saved.
    pub(crate) fn workspace_row(&self) -> Option<i64> {
        let key = (
            self.project.source_name.clone(),
            self.project.model.project.proj_id,
            self.baselines.revision,
        );
        if let Some((k, id)) = self.baselines.parent.borrow().as_ref() {
            if *k == key {
                return *id;
            }
        }
        let id = store::parent_row(self.store.connection(), &key.0, key.1);
        *self.baselines.parent.borrow_mut() = Some((key, id));
        id
    }

    fn ensure_workspace_row(&mut self) -> Result<i64, String> {
        if let Some(id) = self.workspace_row() {
            return Ok(id);
        }
        self.save();
        self.baselines.revision += 1;
        self.workspace_row()
            .ok_or_else(|| "The project could not be saved to the workspace".into())
    }

    /// Rebuild baseline comparisons on the next use.
    pub(crate) fn baselines_changed(&mut self) {
        self.baselines.revision += 1;
    }

    /// The parent's `sum_base_proj_id` text.
    fn project_baseline_field(&self) -> String {
        xb::project_value(
            &self.project.document,
            self.project.model.project.proj_id,
            "sum_base_proj_id",
        )
    }

    fn load_baseline(&self, meta: &BaselineRow) -> data::Source {
        let origin = self.project.model.origin_serial;
        let revision = self.baselines.revision;
        let key = (meta.id, revision, origin);
        if let Some(l) = self.baselines.loaded.borrow().get(&key) {
            return data::Source::Baseline(l.clone());
        }
        let loaded = store::get(self.store.connection(), meta.id)
            .ok()
            .flatten()
            .and_then(|row| {
                let doc = parse(&row).ok()?;
                data::load(&row.name, doc, row.project_key, origin).ok()
            });
        match loaded {
            Some(l) => {
                let l = Arc::new(l);
                let mut cache = self.baselines.loaded.borrow_mut();
                cache.retain(|k, _| k.1 == revision);
                cache.insert(key, l.clone());
                data::Source::Baseline(l)
            }
            None => data::Source::None,
        }
    }

    /// The open project's assigned baselines, cached until something changes.
    pub(crate) fn baseline_compare(&self) -> Arc<Compare> {
        let parent = self.workspace_row();
        let field = self.project_baseline_field();
        let key = (
            parent,
            self.baselines.revision,
            field.clone(),
            self.project.model.origin_serial,
        );
        if let Some((k, c)) = self.baselines.compare.borrow().as_ref() {
            if *k == key {
                return c.clone();
            }
        }
        let mut compare = Compare {
            basis: self.baselines.basis,
            ..Default::default()
        };
        compare.slots[0] = data::Source::Current;
        compare.slots[1] = data::Source::Current;
        if let Some(parent) = parent {
            let c = self.store.connection();
            let rows = store::list(c, parent).unwrap_or_default();
            if let Some(meta) = rows
                .iter()
                .find(|r| field.trim() == r.project_key.to_string())
            {
                compare.slots[0] = self.load_baseline(meta);
            }
            let users = store::user_baselines(c, parent);
            for (k, choice) in users.0.iter().enumerate() {
                compare.slots[k + 1] = match choice {
                    Choice::None => data::Source::None,
                    Choice::Current => data::Source::Current,
                    Choice::Baseline(id) => rows
                        .iter()
                        .find(|r| r.id == *id)
                        .map_or(data::Source::None, |m| self.load_baseline(m)),
                };
            }
        }
        let compare = Arc::new(compare);
        *self.baselines.compare.borrow_mut() = Some((key, compare.clone()));
        compare
    }

    /// Name of the project baseline for the status bar.
    pub(crate) fn project_baseline_name(&self) -> String {
        self.baseline_compare().name(Slot::Project)
    }

    /// Whether the open project has stored baselines.
    pub(crate) fn has_baselines(&self) -> bool {
        self.workspace_row()
            .is_some_and(|id| store::count(self.store.connection(), id) > 0)
    }

    // ------------------------------------------------------------ undo

    fn push_op(&mut self, op: Op) -> u64 {
        self.baselines.next_op += 1;
        let id = self.baselines.next_op;
        self.baselines.ops.insert(id, op);
        self.baselines.revision += 1;
        id
    }

    /// Write one side of an op to the workspace (and the open project).
    fn apply_images(&mut self, op: &Op, forward: bool) -> Result<(), String> {
        let ch = &op.change;
        ops::apply(self.store.connection(), ch, forward)?;
        if !forward {
            let c = self.store.connection();
            for (id, users) in &op.users {
                if store::project_row(c, *id)?.is_some() {
                    store::set_user_baselines(c, *id, *users)?;
                }
            }
        }
        if let Some((before, after)) = &ch.project_baseline {
            if self.workspace_row() == Some(ch.parent) {
                let pid = self.project.model.project.proj_id;
                let value = if forward { after } else { before };
                xb::set_project_value(&mut self.project.document, pid, "sum_base_proj_id", value);
                self.dirty = true;
            }
        }
        if ch.workspace.is_some() {
            self.enterprise.cache = None;
        }
        if let Some((delta, record)) = &op.sample {
            let mut open: Vec<(Option<i64>, &Document)> =
                vec![(self.open_set.active, &self.project.document)];
            open.extend(
                self.open_set
                    .parked
                    .iter()
                    .map(|p| (Some(p.id), &p.project.document)),
            );
            self.store
                .apply_dictionary_delta(delta, forward, None, &open)?;
            openxer_automation::workspace_db::write_sample(
                self.store.connection(),
                (!forward).then_some(record),
            )?;
        }
        if !ch.projects.is_empty() {
            self.saved_projects = self.store.list().unwrap_or_default();
            self.views.projects.invalidate();
        }
        self.baselines.revision += 1;
        Ok(())
    }

    /// Write whole workspace project rows (with their baselines and the
    /// workspace EPS tables) as one undo step: the Projects view's Add, Copy
    /// and Delete. Each entry is (id, before, after); `None` is absent.
    pub(crate) fn run_project_rows(
        &mut self,
        projects: Vec<(i64, Option<ProjectRow>, Option<ProjectRow>)>,
        rows: Vec<(i64, Option<BaselineRow>, Option<BaselineRow>)>,
        workspace: Option<(String, String)>,
        users: Option<(UserBaselines, UserBaselines)>,
        sample: Option<(
            openxer_xer::workspace_scope::delta::Delta,
            openxer_automation::workspace_db::SampleRecord,
        )>,
        time: f64,
    ) -> Result<(), String> {
        let op = Op {
            change: ops::Change {
                parent: projects.first().map_or(0, |p| p.0),
                rows,
                projects,
                workspace,
                users,
                ..Default::default()
            },
            sample,
            users: vec![],
        };
        self.run_op(op, time)
    }

    /// Remove several workspace project rows with their baselines, and
    /// change the workspace EPS tables, as one undo step (an EPS node
    /// deleted with what it holds). `users` are each project's baseline
    /// choices, put back on undo; `sample` as for [`Self::run_project_rows`].
    pub(crate) fn run_projects_removal(
        &mut self,
        projects: Vec<(i64, Option<ProjectRow>, Option<ProjectRow>)>,
        rows: Vec<(i64, Option<BaselineRow>, Option<BaselineRow>)>,
        workspace: (String, String),
        users: Vec<(i64, UserBaselines)>,
        sample: Option<(
            openxer_xer::workspace_scope::delta::Delta,
            openxer_automation::workspace_db::SampleRecord,
        )>,
        time: f64,
    ) -> Result<(), String> {
        let op = Op {
            change: ops::Change {
                parent: 0,
                rows,
                projects,
                workspace: Some(workspace),
                ..Default::default()
            },
            sample,
            users,
        };
        self.run_op(op, time)
    }

    /// Undo (`redo` false) or redo a baseline op named by the history.
    pub(crate) fn baseline_history(&mut self, id: u64, redo: bool) {
        let Some(op) = self.baselines.ops.get(&id).cloned() else {
            return;
        };
        if let Err(e) = self.apply_images(&op, redo) {
            self.error(format!("The baseline change could not be reverted: {e}"));
        }
    }

    /// Run an op as one undo step: pending project edits are recorded
    /// first, then the op's new state is applied and recorded. A change to
    /// the project's own PROJECT row bumps the detail revision so the history
    /// snapshot takes it in.
    fn run_op(&mut self, op: Op, time: f64) -> Result<(), String> {
        self.history
            .record(&self.project, self.records_revision, None, time);
        self.apply_images(&op, true)?;
        if op.change.project_baseline.is_some() {
            self.records_serial += 1;
            self.records_revision = self.records_serial;
        }
        let id = self.push_op(op);
        self.history
            .record_external(&self.project, self.records_revision, id, time);
        Ok(())
    }

    // ------------------------------------------------------------ owners

    /// The open project with workspace row `id` as the owner of its
    /// baselines (the active one, or a parked one), handed to `f`.
    fn with_owner<T>(&self, id: i64, f: impl FnOnce(&ops::Owner) -> T) -> Option<T> {
        let p = if self.workspace_row() == Some(id) || self.open_set.active == Some(id) {
            &self.project
        } else {
            &self.open_set.parked.iter().find(|p| p.id == id)?.project
        };
        let doc = p.updated_document();
        let project_id = p.model.project.name.clone();
        let name = views::project_title(p);
        let owner = ops::Owner {
            id,
            doc: &doc,
            pid: p.model.project.proj_id,
            project_id: &project_id,
            name: &name,
            windows_encoding: p.document.windows_encoding,
        };
        Some(f(&owner))
    }

    /// The open project's workspace row (saved first if needed), as owner.
    fn active_owner(&mut self) -> Result<i64, ops::Error> {
        Ok(self.ensure_workspace_row()?)
    }

    /// Make the open project with workspace row `id` the current one, so a
    /// command on its baselines runs where its undo history is.
    pub(crate) fn activate_owner(&mut self, id: i64) {
        if self.workspace_row() != Some(id) {
            self.activate_id(id);
        }
    }

    /// Whether baseline `row` is in use as a project or user baseline.
    pub(crate) fn baseline_in_use(&self, row: &BaselineRow) -> bool {
        let c = self.store.connection();
        self.with_owner(row.parent, |o| {
            !ops::assigned_roles(c, row, &o.project_baseline()).is_empty()
        })
        .unwrap_or_else(|| !ops::assigned_roles(c, row, "").is_empty())
    }

    /// A command that did not run: a refusal is a message answered with OK,
    /// anything else an error.
    pub(crate) fn baseline_failed(&mut self, e: ops::Error) {
        match e {
            ops::Error::Refused(m) => self.baselines.notice = Some(m),
            ops::Error::Failed(m) => self.error(m),
        }
    }

    /// Run a shared change as one undo step.
    fn run_change(&mut self, change: ops::Change, time: f64) -> Result<(), ops::Error> {
        Ok(self.run_op(Op::from(change), time)?)
    }

    // ------------------------------------------------------------ commands

    /// Maintain Baselines > Add > Save a copy of the current project.
    pub(crate) fn add_baseline_copy(&mut self, time: f64) -> Result<i64, ops::Error> {
        let parent = self.active_owner()?;
        let stamp = now_stamp();
        let c = self.store.connection();
        let change = self
            .with_owner(parent, |o| ops::add_copy(c, o, &stamp))
            .ok_or("The project is not open")??;
        let id = change.made.expect("a new baseline");
        self.run_change(change, time)?;
        self.status = "Baseline saved".into();
        Ok(id)
    }

    /// The workspace projects Select Project lists for Convert: (workspace
    /// id, Project ID, Project Name). Open projects are listed too; picking
    /// one is refused.
    pub(crate) fn convertible_projects(&self) -> Vec<(i64, String, String)> {
        let c = self.store.connection();
        let mut out: Vec<(i64, String, String)> = self
            .store
            .list()
            .unwrap_or_default()
            .into_iter()
            .map(|(id, title)| {
                let name = store::project_row(c, id)
                    .ok()
                    .flatten()
                    .and_then(|r| {
                        let doc = Document::parse(&r.xer).ok()?;
                        Some(xb::project_name(&doc, r.project_key))
                    })
                    .unwrap_or_else(|| title.clone());
                (id, title, name)
            })
            .collect();
        out.sort_by_key(|p| p.1.to_lowercase());
        out
    }

    /// Maintain Baselines > Add > Convert another project to a new baseline.
    /// An open project is refused.
    pub(crate) fn convert_to_baseline(
        &mut self,
        project: i64,
        time: f64,
    ) -> Result<i64, ops::Error> {
        let parent = self.active_owner()?;
        let open = self.open_ids();
        if project == parent || open.contains(&project) {
            return Err(ops::Error::Refused(ops::OPEN_REFUSED.into()));
        }
        let stamp = now_stamp();
        let c = self.store.connection();
        let source = ops::source_document(c, project)?;
        let change = self
            .with_owner(parent, |o| {
                ops::convert(c, o, project, &source, &open, &stamp)
            })
            .ok_or("The project is not open")??;
        let id = change.made.expect("a new baseline");
        self.run_change(change, time)?;
        self.status = "Project converted to a baseline".into();
        Ok(id)
    }

    /// Maintain Baselines > Delete. A baseline in use is refused.
    pub(crate) fn delete_baseline(&mut self, id: i64, time: f64) -> Result<(), ops::Error> {
        let row =
            store::get(self.store.connection(), id)?.ok_or("The baseline no longer exists")?;
        self.activate_owner(row.parent);
        let c = self.store.connection();
        let change = self
            .with_owner(row.parent, |o| ops::delete(c, o, id))
            .ok_or("The project is not open")??;
        self.run_change(change, time)?;
        self.status = "Baseline deleted".into();
        Ok(())
    }

    /// Maintain Baselines > Copy: another baseline of the same project with
    /// the next number.
    pub(crate) fn copy_baseline(&mut self, id: i64, time: f64) -> Result<i64, ops::Error> {
        let row =
            store::get(self.store.connection(), id)?.ok_or("The baseline no longer exists")?;
        self.activate_owner(row.parent);
        let stamp = now_stamp();
        let c = self.store.connection();
        let change = self
            .with_owner(row.parent, |o| ops::copy(c, o, id, &stamp))
            .ok_or("The project is not open")??;
        let new = change.made.expect("a new baseline");
        self.run_change(change, time)?;
        self.status = "Baseline copied".into();
        Ok(new)
    }

    /// Maintain Baselines > Restore: the baseline leaves the list and becomes
    /// a separate workspace project in the owner's EPS folder. A baseline in
    /// use is refused. Returns the new project's workspace row.
    pub(crate) fn restore_baseline(&mut self, id: i64, time: f64) -> Result<i64, ops::Error> {
        let row =
            store::get(self.store.connection(), id)?.ok_or("The baseline no longer exists")?;
        self.activate_owner(row.parent);
        let c = self.store.connection();
        let change = self
            .with_owner(row.parent, |o| ops::restore(c, o, id))
            .ok_or("The project is not open")??;
        let new = change.made.expect("a new project");
        self.run_change(change, time)?;
        self.status = format!("Baseline {} is now a separate project", row.short_name);
        Ok(new)
    }

    /// Activity IDs of the open project's activities that pass the filter
    /// `key` (`std:` standard filter or `user:` saved filter).
    #[cfg(test)]
    pub(crate) fn filtered_codes(&self, key: &str) -> BTreeSet<String> {
        self.filtered_codes_of(&[key.to_string()], true)
    }

    /// Activity IDs of the open project's activities that pass every one
    /// (`match_all`) or any one of the filters `keys`.
    pub(crate) fn filtered_codes_of(&self, keys: &[String], match_all: bool) -> BTreeSet<String> {
        use crate::grid::filters::{FilterSelection, PreparedSelection};
        let ix = self.grid_index();
        let catalog = crate::grid::columns::catalog(&self.project);
        let selection = FilterSelection {
            all_activities: false,
            selected: keys.to_vec(),
            match_all,
            highlight: false,
        };
        let prepared =
            PreparedSelection::new(&self.project, &catalog, &selection, &self.grid.user_filters);
        self.project
            .model
            .activities
            .iter()
            .enumerate()
            .filter(|(i, _)| prepared.matches(&self.project, &ix, *i))
            .map(|(_, a)| a.task_code.clone())
            .collect()
    }

    /// The Update Baseline options as last saved by this user (the
    /// dialog's defaults before that), with the log file in Documents when
    /// none was chosen.
    pub(crate) fn saved_update_options(&self) -> UpdateOptions {
        let mut o = self
            .store
            .preference(UPDATE_OPTIONS_KEY)
            .ok()
            .flatten()
            .and_then(|text| serde_json::from_str::<UpdateOptions>(&text).ok())
            .unwrap_or_default();
        if o.log_path.trim().is_empty() {
            o.log_path = default_update_log();
        }
        o
    }

    /// The dialog's defaults (the Default button).
    pub(crate) fn default_update_options() -> UpdateOptions {
        UpdateOptions {
            log_path: default_update_log(),
            ..UpdateOptions::default()
        }
    }

    /// Keep the Update Baseline options for the next opening.
    pub(crate) fn save_update_options(&mut self, o: &UpdateOptions) {
        let text = serde_json::to_string(o).unwrap_or_default();
        if let Err(e) = self.store.set_preference(UPDATE_OPTIONS_KEY, &text) {
            self.error(format!("The update options were not saved: {e}"));
        }
    }

    /// The activities the options' filter passes (`None`: all of them).
    fn update_scope(&self, o: &UpdateOptions) -> Result<Option<BTreeSet<String>>, ops::Error> {
        if !o.use_filter {
            return Ok(None);
        }
        if o.filters.iter().all(|f| f.trim().is_empty()) {
            return Err(ops::Error::Refused(
                "Tick the filters the activities must pass, or choose All Activities.".into(),
            ));
        }
        Ok(Some(self.filtered_codes_of(&o.filters, o.filter_match_all)))
    }

    /// The shared update of baseline `id` by `o`, not applied: the change
    /// and its list of changes.
    fn baseline_update_change(
        &self,
        id: i64,
        o: &UpdateOptions,
        stamp: &str,
    ) -> Result<(ops::Change, xu::Preview), ops::Error> {
        let row =
            store::get(self.store.connection(), id)?.ok_or("The baseline no longer exists")?;
        let only = self.update_scope(o)?;
        let c = self.store.connection();
        self.with_owner(row.parent, |owner| {
            ops::update(c, owner, id, o, only.as_ref(), stamp)
        })
        .ok_or("The project is not open")?
    }

    /// Update Baseline > Preview: what Update would do with these options.
    pub(crate) fn preview_baseline_update(
        &self,
        id: i64,
        o: &UpdateOptions,
    ) -> Result<xu::Preview, ops::Error> {
        Ok(self.baseline_update_change(id, o, &now_stamp())?.1)
    }

    /// Update Baseline > Update: the baseline takes the project's data by
    /// `o`, its Last Update Date is now, and with Log to file on the log is
    /// written. One undo step; no question first. Returns the log written.
    pub(crate) fn update_baseline(
        &mut self,
        id: i64,
        o: &UpdateOptions,
        time: f64,
    ) -> Result<Option<std::path::PathBuf>, ops::Error> {
        let row =
            store::get(self.store.connection(), id)?.ok_or("The baseline no longer exists")?;
        self.activate_owner(row.parent);
        let stamp = now_stamp();
        let (change, preview) = self.baseline_update_change(id, o, &stamp)?;
        let log = if o.log_to_file && !o.log_path.trim().is_empty() {
            let path = std::path::PathBuf::from(o.log_path.trim());
            let text = self
                .with_owner(row.parent, |owner| {
                    ops::update_log(owner, &row, o, &preview, &stamp)
                })
                .unwrap_or_default();
            match std::fs::write(&path, text) {
                Ok(()) => Some(path),
                Err(e) => {
                    self.error(format!("The update log could not be written: {e}"));
                    None
                }
            }
        } else {
            None
        };
        self.run_change(change, time)?;
        let (added, deleted, updated) = preview.counts();
        self.status =
            format!("Baseline updated: {added} added, {deleted} deleted, {updated} updated");
        Ok(log)
    }
    /// Project > Revert Project to Baseline: the open project takes the data
    /// of baseline `id` and keeps its ID, name, project baseline and other
    /// settings; the baseline stays in the list. One undo step.
    pub(crate) fn revert_to_baseline(&mut self, id: i64, time: f64) -> Result<(), ops::Error> {
        let row =
            store::get(self.store.connection(), id)?.ok_or("The baseline no longer exists")?;
        self.activate_owner(row.parent);
        let pid = self.project.model.project.proj_id;
        let out = self
            .with_owner(row.parent, |o| ops::reverted(o, &row))
            .ok_or("The project is not open")??;
        let mut reverted = ProjectData::from_document(out, pid, self.project.source_name.clone())?;
        reverted.model.validate()?;
        reverted.model.options.do_level = self.project.model.options.do_level;
        let before = std::mem::replace(&mut self.project, reverted);
        let before_revision = self.records_revision;
        self.records_serial += 1;
        self.records_revision = self.records_serial;
        self.history.record_bulk(
            &before,
            before_revision,
            &self.project,
            self.records_revision,
            time,
        );
        self.baselines.revision += 1;
        self.dirty = true;
        self.edit_cell = None;
        self.date_edits.clear();
        self.selected = self
            .selected
            .filter(|&i| i < self.project.model.activities.len());
        self.status = format!("The project was reverted to baseline {}", row.short_name);
        Ok(())
    }

    /// The Baseline ID, Baseline Name and Baseline Type fields of Maintain
    /// Baselines (`None` leaves a field as it is). One undo step.
    pub(crate) fn set_baseline_fields(
        &mut self,
        id: i64,
        baseline_id: Option<&str>,
        name: Option<&str>,
        baseline_type: Option<&str>,
        time: f64,
    ) -> Result<(), ops::Error> {
        let row =
            store::get(self.store.connection(), id)?.ok_or("The baseline no longer exists")?;
        let c = self.store.connection();
        let change = self
            .with_owner(row.parent, |o| {
                ops::set(c, o, id, baseline_id, name, baseline_type)
            })
            .ok_or("The project is not open")??;
        if change.is_empty() {
            return Ok(());
        }
        self.activate_owner(row.parent);
        self.run_change(change, time)
    }

    /// Rename a baseline and set its type (`""` is none).
    #[cfg(test)]
    pub(crate) fn rename_baseline(
        &mut self,
        id: i64,
        name: &str,
        baseline_type: &str,
        time: f64,
    ) -> Result<(), ops::Error> {
        self.set_baseline_fields(id, None, Some(name), Some(baseline_type), time)
    }

    /// The Baseline Type choices: (id as text, name), `<None>` first, then
    /// the types the file defines in name order.
    pub(crate) fn baseline_type_list(&self) -> Vec<(String, String)> {
        ops::type_choices(&self.project.document)
    }

    /// The open project's stored baselines that have baseline type `type_id`.
    pub(crate) fn baselines_with_type(&self, type_id: i64) -> Vec<BaselineRow> {
        let Some(parent) = self.workspace_row() else {
            return vec![];
        };
        let doc = &self.project.document;
        let wanted = type_id.to_string();
        store::list(self.store.connection(), parent)
            .unwrap_or_default()
            .into_iter()
            .filter(|r| type_in_document(doc, &r.baseline_type) == wanted)
            .collect()
    }

    /// Admin Categories > Baseline Types > Delete: the type leaves the file's
    /// list and the baselines that used it lose their type, as one undo step.
    pub(crate) fn delete_baseline_type(&mut self, type_id: i64) -> Result<Option<i64>, String> {
        let used = self.baselines_with_type(type_id);
        let doc = &mut self.project.document;
        if used.is_empty() {
            crate::dictionaries::ops::delete_category(
                doc,
                crate::dictionaries::Category::Baseline,
                type_id,
            );
            self.touch_records();
            return Ok(None);
        }
        let c = self.store.connection();
        let mut op = Op::from(ops::Change {
            parent: used[0].parent,
            ..Default::default()
        });
        for meta in used {
            let Some(before) = store::get(c, meta.id)? else {
                continue;
            };
            let after = BaselineRow {
                baseline_type: String::new(),
                ..before.clone()
            };
            op.change.rows.push((meta.id, Some(before), Some(after)));
        }
        // Record pending edits, change the dictionary, then the baselines.
        self.history
            .record(&self.project, self.records_revision, None, 0.);
        crate::dictionaries::ops::delete_category(
            &mut self.project.document,
            crate::dictionaries::Category::Baseline,
            type_id,
        );
        self.touch_records();
        self.apply_images(&op, true)?;
        let id = self.push_op(op);
        self.history
            .record_external(&self.project, self.records_revision, id, 0.);
        Ok(None)
    }

    /// The open project's baseline choices: project baseline, then user.
    pub(crate) fn baseline_choices(&self) -> (Choice, UserBaselines) {
        match self.workspace_row() {
            Some(parent) => self.baseline_choices_of(parent),
            None => (Choice::Current, UserBaselines::default()),
        }
    }

    /// The baseline choices of the open project with workspace row `id`.
    pub(crate) fn baseline_choices_of(&self, id: i64) -> (Choice, UserBaselines) {
        let c = self.store.connection();
        self.with_owner(id, |o| ops::choices(c, o))
            .unwrap_or((Choice::Current, UserBaselines::default()))
    }

    /// Assign Baselines > OK for the open project. `project` is `None` when
    /// the project baseline was left alone (its field is then not rewritten).
    pub(crate) fn assign_baselines(
        &mut self,
        project: Option<Choice>,
        users: UserBaselines,
        time: f64,
    ) -> Result<(), ops::Error> {
        let parent = self.active_owner()?;
        let (old_project, old_users) = self.baseline_choices();
        let c = self.store.connection();
        let change = self
            .with_owner(parent, |o| ops::assign(c, o, project, users))
            .ok_or("The project is not open")??;
        if change.is_empty() {
            return Ok(());
        }
        let mut newly = vec![];
        if change.project_baseline.is_some()
            && matches!(project, Some(Choice::Baseline(_)))
            && project != Some(old_project)
        {
            newly.push(Slot::Project);
        }
        for (k, slot) in [Slot::Primary, Slot::Secondary, Slot::Tertiary]
            .into_iter()
            .enumerate()
        {
            if matches!(users.0[k], Choice::Baseline(_)) && users.0[k] != old_users.0[k] {
                newly.push(slot);
            }
        }
        self.run_change(change, time)?;
        self.show_baseline_bars(&newly);
        self.status = "Baselines assigned".into();
        Ok(())
    }

    /// Turn on the Gantt bars of slots that just got a stored baseline.
    fn show_baseline_bars(&mut self, slots: &[Slot]) {
        use crate::gantt::settings::BarSource;
        let mut changed = false;
        for bar in &mut self.gantt.bars {
            let slot = match bar.timescale {
                BarSource::ProjectBaselineBar => Slot::Project,
                BarSource::PrimaryBaselineBar => Slot::Primary,
                BarSource::SecondaryBaselineBar => Slot::Secondary,
                BarSource::TertiaryBaselineBar => Slot::Tertiary,
                _ => continue,
            };
            if slots.contains(&slot) && !bar.display {
                bar.display = true;
                changed = true;
            }
        }
        if changed {
            self.save_gantt();
        }
    }

    /// Renumber Activity IDs with "Renumber selected activities in
    /// baselines": the same renames in every baseline of the open project,
    /// attached to the renumber's undo step. A baseline where a new ID is
    /// already taken is left alone. Returns (baselines changed, skipped names).
    pub(crate) fn renumber_in_baselines(
        &mut self,
        renames: &BTreeMap<String, String>,
    ) -> (usize, Vec<String>) {
        let Some(parent) = self.workspace_row() else {
            return (0, vec![]);
        };
        let c = self.store.connection();
        let mut op = Op::from(ops::Change {
            parent,
            ..Default::default()
        });
        let mut skipped = vec![];
        for meta in store::list(c, parent).unwrap_or_default() {
            let Ok(Some(before)) = store::get(c, meta.id) else {
                continue;
            };
            let Ok(mut doc) = parse(&before) else {
                continue;
            };
            match xb::rename_activities(&mut doc, before.project_key, renames) {
                Ok(0) => {}
                Ok(_) => {
                    let after = BaselineRow {
                        xer: doc.text(),
                        ..before.clone()
                    };
                    op.change.rows.push((meta.id, Some(before), Some(after)));
                }
                Err(_) => skipped.push(before.short_name.clone()),
            }
        }
        let changed = op.change.rows.len();
        if changed > 0 {
            match self.apply_images(&op, true) {
                Ok(()) => {
                    let id = self.push_op(op);
                    self.history.tag_last(id);
                }
                Err(e) => self.error(e),
            }
        }
        (changed, skipped)
    }

    /// After File > Open of an XER: store the baselines it carries for the
    /// project just opened.
    pub(crate) fn adopt_file_baselines(&mut self) {
        let pid = self.project.model.project.proj_id;
        if !xb::baselines_in(&self.project.document)
            .iter()
            .any(|(_, owner)| *owner == pid)
        {
            return;
        }
        let Ok(parent) = self.ensure_workspace_row() else {
            return;
        };
        match attach_file_baselines(
            self.store.connection(),
            parent,
            &self.project.document,
            pid,
            None,
        ) {
            Ok(n) if n > 0 => {
                self.baselines.revision += 1;
                self.status = format!(
                    "{} stored with the project",
                    crate::util::count(n, "baseline", "baselines")
                );
            }
            Ok(_) => {}
            Err(e) => self.error(format!("Baselines in the file were not stored: {e}")),
        }
    }
}

impl App {
    /// After File > Import: store the baselines each imported file carries
    /// for its projects. `ids` are the workspace rows written, `origins` the
    /// (file, project id) of each. Returns how many baselines were stored.
    pub(crate) fn import_file_baselines(
        &mut self,
        ids: &[i64],
        origins: &[(usize, i64)],
        files: &[(std::path::PathBuf, Document)],
    ) -> usize {
        let batch = crate::interchange::workspace::last_import(&self.store)
            .ok()
            .flatten()
            .map(|(b, _, _)| b);
        let mut added = 0;
        for (id, (file, pid)) in ids.iter().zip(origins) {
            let Some((_, doc)) = files.get(*file) else {
                continue;
            };
            match attach_file_baselines(self.store.connection(), *id, doc, *pid, batch) {
                Ok(n) => added += n,
                Err(e) => self.error(format!("Baselines of project {pid} were not stored: {e}")),
            }
        }
        self.baselines.revision += 1;
        added
    }
}
