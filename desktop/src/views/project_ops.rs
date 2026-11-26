//! Projects view Add, Copy / Paste and Delete. A copy is a new workspace
//! project with its own Project ID, placed under the EPS node it is pasted
//! on; the Copy Project Options dialog chooses whether its WBS, activities,
//! relationships, baselines, project codes, notebook, funding and expenses
//! come along. Delete removes a closed project
//! (with its baselines) after a confirmation. Delete of an EPS node (here or
//! in the EPS dialog) removes it with the nodes under it and every project
//! filed in them, after one question that counts what goes; open projects
//! among them are saved and closed first. Each is one undo step of the
//! open project's history, which puts the workspace rows and the EPS
//! placement back.
use super::*;
use crate::baselines::store::{self as bstore, BaselineRow, ProjectRow, UserBaselines};

/// What a pasted copy takes from its source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CopyChoice {
    pub wbs: bool,
    pub activities: bool,
    pub relationships: bool,
    pub baselines: bool,
    /// Project code assignments (PROJPCAT).
    pub project_codes: bool,
    /// Notebook entries of the project, its WBS elements and activities
    /// (WBSMEMO, TASKMEMO).
    pub notebook: bool,
    /// Funding source rows of the project (PROJFUND).
    pub funding: bool,
    /// Activity expenses (PROJCOST).
    pub expenses: bool,
}

impl Default for CopyChoice {
    fn default() -> Self {
        Self {
            wbs: true,
            activities: true,
            relationships: true,
            baselines: true,
            project_codes: true,
            notebook: true,
            funding: true,
            expenses: true,
        }
    }
}

/// Remove the rows of `table` that belong to project `pid` (every row when
/// the table has no project column).
fn drop_project_rows(doc: &mut Document, table: &str, pid: i64) {
    let Some(t) = doc.tables.iter_mut().find(|t| t.name == table) else {
        return;
    };
    let scoped = t.field("proj_id").is_some();
    let rows = std::mem::take(&mut t.rows);
    t.rows = rows
        .into_iter()
        .filter(|r| scoped && t.value(r, "proj_id").trim().parse::<i64>().ok() != Some(pid))
        .collect();
}

/// The Copy Project Options dialog while open.
#[derive(Clone, Debug, PartialEq)]
pub struct CopyDialog {
    pub source: i64,
    pub eps: i64,
    pub title: String,
    pub choice: CopyChoice,
    /// The source has a project WBS node to gather activities under, so its
    /// other WBS elements can be left out.
    pub wbs_optional: bool,
}

/// A Delete waiting for its confirmation.
#[derive(Clone, Debug, PartialEq)]
pub struct DeleteConfirm {
    pub id: i64,
    pub title: String,
    pub activities: usize,
    pub baselines: usize,
    /// The project is currently open and will be closed first.
    pub open: bool,
    /// The project is the first-run sample: its own dictionary rows go too.
    pub sample: bool,
}

/// A Projects view Delete of an EPS node waiting for its confirmation.
#[derive(Clone, Debug, PartialEq)]
pub struct FolderDeleteConfirm {
    pub eps: i64,
    pub text: String,
}

/// What deleting an EPS node takes with it.
struct FolderPlan {
    nodes: Vec<i64>,
    projects: Vec<i64>,
    baselines: usize,
}

/// A Project ID no workspace project uses: `NAME-1`, `NAME-2`, ...
pub fn free_project_id(base: &str, taken: &[String]) -> String {
    (1..)
        .map(|n| format!("{base}-{n}"))
        .find(|c| !taken.iter().any(|t| t.eq_ignore_ascii_case(c)))
        .expect("a free Project ID")
}

/// The copy of `src` a paste makes: the new Project ID, and only what
/// `choice` keeps. Without WBS the activities move to the project node;
/// without activities their relationships and child rows go too.
pub fn copy_project(
    src: &ProjectData,
    project_id: &str,
    choice: CopyChoice,
) -> Result<ProjectData, String> {
    let mut p = src.clone();
    let old = p.model.project.name.clone();
    p.model.project.name = project_id.to_string();
    let node = wbs::project_node(&p);
    // The project node is named after the project.
    if let Some(node) = node {
        if let Some(w) = p.wbs.iter_mut().find(|w| w.id == node && w.code == old) {
            w.code = project_id.to_string();
        }
    }
    if !choice.activities {
        let all: Vec<i64> = p.model.activities.iter().map(|a| a.task_id).collect();
        p.delete_activities(&all)?;
    } else if !choice.relationships {
        p.model.links.clear();
        p.model.rebuild_links()?;
        let ids: BTreeSet<i64> = p.model.activities.iter().map(|a| a.task_id).collect();
        if let Some(t) = p.document.tables.iter_mut().find(|t| t.name == "TASKPRED") {
            let rows = std::mem::take(&mut t.rows);
            t.rows = rows
                .into_iter()
                .filter(|r| {
                    let id = |k: &str| t.value(r, k).trim().parse::<i64>().unwrap_or(0);
                    !ids.contains(&id("task_id")) && !ids.contains(&id("pred_task_id"))
                })
                .collect();
        }
    }
    if !choice.wbs {
        let node =
            node.ok_or("The project has no project WBS node to keep its activities under")?;
        for d in p.details.values_mut() {
            d.wbs = node;
        }
        p.wbs.retain(|w| w.id == node);
        if let Some(t) = p.document.tables.iter_mut().find(|t| t.name == "WBSMEMO") {
            let rows = std::mem::take(&mut t.rows);
            t.rows = rows
                .into_iter()
                .filter(|r| t.value(r, "wbs_id").trim().parse::<i64>().ok() == Some(node))
                .collect();
        }
        wbs::sync_projwbs(&mut p);
    }
    let pid = p.model.project.proj_id;
    for (keep, table) in [
        (choice.project_codes, "PROJPCAT"),
        (choice.notebook, "WBSMEMO"),
        (choice.notebook, "TASKMEMO"),
        (choice.funding, "PROJFUND"),
        (choice.expenses, "PROJCOST"),
    ] {
        if !keep {
            drop_project_rows(&mut p.document, table, pid);
        }
    }
    p.model.validate()?;
    Ok(p)
}

/// Start of the status line `App::repair_add_sample_wbs` leaves.
pub const SAMPLE_WBS_REPAIRED: &str = "Removed the sample WBS";

fn next_row_id(store: &Store, table: &str) -> Result<i64, String> {
    store
        .connection()
        .query_row(
            &format!("SELECT COALESCE(MAX(id),0)+1 FROM {table}"),
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())
}

impl App {
    fn workspace_titles(&self) -> Vec<String> {
        self.store
            .list()
            .unwrap_or_default()
            .into_iter()
            .map(|(_, t)| t)
            .chain(std::iter::once(self.project.model.project.name.clone()))
            .collect()
    }

    /// The source of a copy as stored, or the open project as it is now.
    fn project_for_copy(&mut self, id: i64) -> Result<ProjectData, String> {
        if self.open_set.active == Some(id) || self.workspace_row() == Some(id) {
            return Ok(self.project.clone());
        }
        if self.is_open(id) {
            self.save();
        }
        self.store.load(id)
    }

    /// Place project row `id` under `eps` in the workspace tables: (before, after).
    fn placed(&mut self, id: i64, eps: Option<i64>) -> Result<Option<(String, String)>, String> {
        let Some(eps) = eps else {
            return Ok(None);
        };
        let mut ws = self.workspace().clone();
        let before = ws.to_text();
        ws.place(id, eps)?;
        Ok(Some((before, ws.to_text())))
    }

    /// When the current project is one the Projects view Add made with the
    /// sample's WBS, remove those untouched sample elements and give it its
    /// own project node (`xer::edit::wbs::repair_add_sample_wbs`), as one
    /// undo step the status bar names. Run when a project becomes current.
    pub(crate) fn repair_add_sample_wbs(&mut self) {
        let before = self.project.clone();
        let before_revision = self.records_revision;
        let Some(removed) = openxer_xer::edit::wbs::repair_add_sample_wbs(&mut self.project) else {
            return;
        };
        self.edit_step(before, before_revision, 0.);
        self.views.wbs.selected = None;
        self.views.wbs.pending = None;
        self.status = format!(
            "{SAMPLE_WBS_REPAIRED} {} that Add copied into {}; Undo puts it back",
            removed.join(", "),
            self.project.model.project.name
        );
    }

    /// Puts the finished project `p` into the workspace under `eps` as one
    /// undo step (the New Project dialog, which the Projects view Add opens).
    pub(crate) fn insert_workspace_project(
        &mut self,
        mut p: ProjectData,
        eps: Option<i64>,
        time: f64,
    ) -> Result<i64, String> {
        let name = p.model.project.name.clone();
        p.source_name =
            bstore::unique_source(self.store.connection(), &name, p.model.project.proj_id);
        let id = next_row_id(&self.store, "projects")?;
        let row = ProjectRow {
            id,
            source_name: p.source_name.clone(),
            project_key: p.model.project.proj_id,
            title: name.clone(),
            xer: p.updated_document().text(),
            windows_encoding: false,
            level_resources: false,
        };
        let ws = self.placed(id, eps)?;
        self.run_project_rows(vec![(id, None, Some(row))], vec![], ws, None, None, time)?;
        self.views.projects.selected = Some((p.source_name, p.model.project.proj_id));
        self.status = format!("Project {name} added");
        Ok(id)
    }

    /// An internal project number for a project the workspace makes: none
    /// of its projects or baselines, and none of the open projects, uses it.
    pub(crate) fn free_project_number(&self) -> Result<i64, String> {
        let open: Vec<i64> = self
            .open_projects()
            .iter()
            .flat_map(|p| {
                std::iter::once(p.model.project.proj_id)
                    .chain(openxer_xer::workspace::numbering::in_document(&p.document))
            })
            .collect();
        bstore::next_project_number(self.store.connection(), &open)
    }

    /// [`App::free_project_number`], also above every project `doc` holds.
    pub(crate) fn free_project_number_for(&self, doc: &Document) -> Result<i64, String> {
        let own = openxer_xer::workspace::numbering::in_document(doc);
        Ok(self
            .free_project_number()?
            .max(openxer_xer::workspace::numbering::next(own)))
    }

    /// The Project IDs the workspace uses.
    pub(crate) fn taken_project_ids(&self) -> Vec<String> {
        self.workspace_titles()
    }

    /// Copy the open choice of project `source` under `eps`.
    pub(crate) fn paste_project_copy(
        &mut self,
        source: i64,
        eps: Option<i64>,
        choice: CopyChoice,
        time: f64,
    ) -> Result<i64, String> {
        let src = self.project_for_copy(source)?;
        let taken = self.workspace_titles();
        let name = free_project_id(&src.model.project.name, &taken);
        let mut p = copy_project(&src, &name, choice)?;
        // The source name is told apart from the original's under the
        // original's number, so the copy never reads as the same file.
        let source_name = bstore::unique_source(
            self.store.connection(),
            &src.source_name,
            src.model.project.proj_id,
        );
        // The copy gets an internal number of its own: relationships between
        // the two projects are told apart by it.
        let number = self.free_project_number_for(&p.document)?;
        p.renumber_project(number)?;
        let key = p.model.project.proj_id;
        let c = self.store.connection();
        p.source_name = bstore::unique_source(c, &source_name, key);
        let level = p.model.options.do_level;
        let id = next_row_id(&self.store, "projects")?;
        let row = ProjectRow {
            id,
            source_name: p.source_name.clone(),
            project_key: key,
            title: name.clone(),
            xer: p.updated_document().text(),
            windows_encoding: p.document.windows_encoding,
            level_resources: level,
        };
        let mut rows = vec![];
        if choice.baselines {
            let c = self.store.connection();
            bstore::ensure(c)?;
            let mut next = next_row_id(&self.store, "baselines")?;
            for meta in bstore::list(c, source)? {
                let Some(full) = bstore::get(c, meta.id)? else {
                    continue;
                };
                let copy = BaselineRow {
                    id: next,
                    parent: id,
                    import_batch: None,
                    ..full
                };
                rows.push((next, None, Some(copy)));
                next += 1;
            }
        }
        let ws = self.placed(id, eps)?;
        self.run_project_rows(vec![(id, None, Some(row))], rows, ws, None, None, time)?;
        self.views.projects.selected = Some((p.source_name, key));
        self.status = format!("Project copied as {name}");
        Ok(id)
    }

    /// Projects view Delete: ask first. Open projects are closed before removal.
    pub(crate) fn request_project_delete(&mut self, id: i64) {
        let open = self.is_open(id) || self.workspace_row() == Some(id);
        let Ok(Some(row)) = bstore::project_row(self.store.connection(), id) else {
            return;
        };
        let activities = if self.open_set.active == Some(id) || self.workspace_row() == Some(id) {
            self.project.model.activities.len()
        } else if let Some(p) = self.open_set.parked.iter().find(|p| p.id == id) {
            p.project.model.activities.len()
        } else {
            Document::parse(&row.xer)
                .ok()
                .and_then(|d| {
                    d.table("TASK").map(|t| {
                        t.rows
                            .iter()
                            .filter(|r| t.value(r, "proj_id").trim() == row.project_key.to_string())
                            .count()
                    })
                })
                .unwrap_or(0)
        };
        let sample = openxer_automation::workspace_db::read_sample(self.store.connection())
            .ok()
            .flatten()
            .is_some_and(|s| s.row == id);
        self.views.projects.delete = Some(DeleteConfirm {
            id,
            title: row.title,
            activities,
            baselines: bstore::count(self.store.connection(), id),
            open,
            sample,
        });
    }

    /// Remove workspace project `id` with its baselines, as one undo step.
    /// If the project is open, it is saved and closed first; closing the last
    /// open project leaves the workspace in the empty state on Projects view.
    pub(crate) fn delete_workspace_project(&mut self, id: i64, time: f64) -> Result<(), String> {
        crate::first_run::mark_seeded(&self.store);
        if self.open_set.parked.iter().any(|p| p.id == id) {
            self.close_project_id(id);
        }
        if !self.open_set.empty
            && (self.open_set.active == Some(id) || self.workspace_row() == Some(id))
        {
            if self.dirty {
                self.try_save()?;
            }
            if !self.open_set.parked.is_empty() {
                self.close_active_project();
            } else {
                self.enter_empty_workspace();
            }
        }
        if self.is_open(id) || self.workspace_row() == Some(id) {
            return Err(
                "The project could not be saved and closed, so nothing was deleted.".into(),
            );
        }
        let c = self.store.connection();
        let row = bstore::project_row(c, id)?.ok_or("The project no longer exists")?;
        bstore::ensure(c)?;
        let mut rows = vec![];
        for meta in bstore::list(c, id)? {
            if let Some(full) = bstore::get(c, meta.id)? {
                rows.push((meta.id, Some(full), None));
            }
        }
        let users = bstore::user_baselines(c, id);
        let mut ws = self.workspace().clone();
        let before = ws.to_text();
        ws.forget(id);
        let after = ws.to_text();
        let title = row.title.clone();
        // The first-run sample takes the dictionary rows only it used.
        let sample = self.sample_removal(id)?;
        let note = sample.as_ref().map(|(removal, _)| {
            let mut note = format!(
                ", with {} dictionary row(s) only the sample used",
                removal.removed
            );
            for line in &removal.renamed {
                note.push_str(&format!("; {line}"));
            }
            note
        });
        // The Primary, Secondary and Tertiary baseline choices go with the
        // project and come back with it on undo.
        self.run_project_rows(
            vec![(id, Some(row), None)],
            rows,
            (before != after).then_some((before, after)),
            Some((users, UserBaselines::default())),
            sample.map(|(removal, record)| (removal.delta, record)),
            time,
        )?;
        self.views.projects.selected = None;
        self.views.projects.copy = self.views.projects.copy.filter(|c| *c != id);
        self.views.projects.cut = self.views.projects.cut.filter(|c| *c != id);
        self.status = match note {
            Some(note) => format!("Sample project {title} removed{note}"),
            None => format!("Project {title} deleted"),
        };
        Ok(())
    }

    /// What deleting EPS node `eps` takes: the node and the nodes under it,
    /// the workspace projects filed in them and their baselines. Refused
    /// for the last top-level node.
    fn folder_delete_plan(&mut self, eps: i64) -> Result<FolderPlan, String> {
        let ws = self.workspace().clone();
        if !ws.eps().iter().any(|n| n.id == eps) {
            return Err("That EPS node no longer exists.".into());
        }
        if ws.heir(eps).is_none() {
            return Err("The EPS needs at least one node.".into());
        }
        let ids = self.workspace_ids();
        let (nodes, projects) = ws.folder_contents(eps, &ids);
        let c = self.store.connection();
        let baselines = projects.iter().map(|p| bstore::count(c, *p)).sum();
        Ok(FolderPlan {
            nodes,
            projects,
            baselines,
        })
    }

    /// The one question a Delete of EPS node `eps` asks (EPS dialog and
    /// Projects view): how many items go, nodes plus projects, as counted
    /// in the scheduler users know. `Err` is a refusal to show instead.
    pub(crate) fn folder_delete_question(&mut self, eps: i64) -> Result<String, String> {
        let plan = self.folder_delete_plan(eps)?;
        let name = self.workspace().eps_value(eps, "eps_name");
        if plan.nodes.len() == 1 && plan.projects.is_empty() {
            return Ok(format!("Delete the EPS node {name}?"));
        }
        Ok(format!(
            "Delete the EPS node {name} and everything under it?\n{} items go: {} EPS node(s) and {} project(s) with {} baseline(s). Open projects among them are saved and closed first. Undo brings it all back.",
            plan.nodes.len() + plan.projects.len(),
            plan.nodes.len(),
            plan.projects.len(),
            plan.baselines
        ))
    }

    /// Projects view Delete on an EPS node: ask first.
    pub(crate) fn request_folder_delete(&mut self, eps: i64) {
        match self.folder_delete_question(eps) {
            Ok(text) => self.views.projects.folder_delete = Some(FolderDeleteConfirm { eps, text }),
            Err(e) => self.error(e),
        }
    }

    /// Close the open projects among `going`, saving their changes first,
    /// so the deletion that follows can be undone with nothing lost. When
    /// the current project goes, another open one takes its place, else the
    /// first workspace project outside `going` is opened.
    fn close_for_folder_delete(&mut self, going: &[i64]) -> Result<(), String> {
        let parked: Vec<i64> = self
            .open_set
            .parked
            .iter()
            .map(|p| p.id)
            .filter(|id| going.contains(id))
            .collect();
        for id in parked {
            self.close_project_id(id);
        }
        let active_inside = self.open_set.active.is_some_and(|a| going.contains(&a))
            || self.workspace_row().is_some_and(|a| going.contains(&a));
        if active_inside {
            if self.open_set.parked.is_empty() {
                if self.dirty {
                    self.try_save()?;
                }
                if let Some(other) = self
                    .workspace_ids()
                    .into_iter()
                    .find(|id| !going.contains(id))
                {
                    let p = self.store.load(other)?;
                    self.load(p);
                } else {
                    self.enter_empty_workspace();
                }
            } else {
                self.close_active_project();
            }
        }
        if going
            .iter()
            .any(|id| self.is_open(*id) || self.workspace_row() == Some(*id))
        {
            return Err(
                "A project under this EPS node could not be saved and closed, so nothing was deleted."
                    .into(),
            );
        }
        Ok(())
    }

    /// Delete EPS node `eps` with everything under it: the nodes below it,
    /// the projects filed in any of them and their baselines, as observed in
    /// the scheduler users know. Open projects among them are closed first
    /// (their unsaved changes saved). One undo step of the project that is
    /// current afterwards brings back the nodes, the projects (closed), their
    /// baselines and baseline choices.
    pub(crate) fn delete_folder(&mut self, eps: i64, time: f64) -> Result<(), String> {
        crate::first_run::mark_seeded(&self.store);
        let plan = self.folder_delete_plan(eps)?;
        let name = self.workspace().eps_value(eps, "eps_name");
        self.close_for_folder_delete(&plan.projects)?;
        let mut ws = self.workspace().clone();
        let before = ws.to_text();
        let ids = self.workspace_ids();
        let going = ws.delete_eps(eps, &ids)?;
        let after = ws.to_text();
        let sample = going
            .iter()
            .find_map(|id| self.sample_removal(*id).transpose())
            .transpose()?;
        let c = self.store.connection();
        bstore::ensure(c)?;
        let (mut projects, mut rows, mut users) = (vec![], vec![], vec![]);
        for &id in &going {
            let Some(row) = bstore::project_row(c, id)? else {
                continue;
            };
            for meta in bstore::list(c, id)? {
                if let Some(full) = bstore::get(c, meta.id)? {
                    rows.push((meta.id, Some(full), None));
                }
            }
            users.push((id, bstore::user_baselines(c, id)));
            projects.push((id, Some(row), None));
        }
        let (count, baselines) = (projects.len(), rows.len());
        self.run_projects_removal(
            projects,
            rows,
            (before, after),
            users,
            sample.map(|(removal, record)| (removal.delta, record)),
            time,
        )?;
        let s = &mut self.views.projects;
        s.selected_eps = None;
        s.copy = s.copy.filter(|c| !going.contains(c));
        s.cut = s.cut.filter(|c| !going.contains(c));
        self.status = format!(
            "EPS node {name} deleted with {} node(s) under it, {count} project(s) and {baselines} baseline(s); Undo brings them back",
            plan.nodes.len() - 1
        );
        Ok(())
    }

    /// The dictionary change that removing workspace project `id` brings
    /// when it is the first-run sample (`workspace_db::plan_sample_removal`),
    /// with the sample record to put back on undo; `None` for any other
    /// project.
    fn sample_removal(
        &self,
        id: i64,
    ) -> Result<
        Option<(
            openxer_automation::workspace_db::SampleRemoval,
            openxer_automation::workspace_db::SampleRecord,
        )>,
        String,
    > {
        use openxer_automation::workspace_db;
        let c = self.store.connection();
        let Some(record) = workspace_db::read_sample(c)?.filter(|s| s.row == id) else {
            return Ok(None);
        };
        let mut open: Vec<(Option<i64>, &Document)> = vec![];
        if !self.open_set.empty {
            open.push((self.open_set.active, &self.project.document));
        }
        open.extend(
            self.open_set
                .parked
                .iter()
                .map(|p| (Some(p.id), &p.project.document)),
        );
        Ok(workspace_db::plan_sample_removal(c, id, &open)?.map(|r| (r, record)))
    }

    /// The Copy Project Options and delete confirmation dialogs.
    pub(super) fn project_ops_dialogs(&mut self, ctx: &egui::Context) {
        let time = ctx.input(|i| i.time);
        let escape = ctx.input(|i| i.key_pressed(Key::Escape));
        if let Some(mut d) = self.views.projects.copy_dialog.take() {
            let mut answer = None;
            // 420 x 224 px inside the frame: what to copy, OK and Cancel in
            // the button column.
            const W: f32 = 420.;
            const H: f32 = 224.;
            use crate::dialog_kit::{self as kit, Entry};
            let ((), closed, actions) = kit::window(
                ctx,
                "copy_project_options",
                "Copy Project Options",
                Vec2::new(W, H),
                |c| {
                    kit::strip(c, W, H);
                    kit::text(
                        c,
                        8.,
                        8.,
                        &format!("Copy {} and paste it as a new project with:", d.title),
                        true,
                    );
                    let wbs_optional = d.wbs_optional;
                    let ch = &mut d.choice;
                    let activities = ch.activities;
                    let checks: [(&str, &str, &mut bool, f32, bool); 8] = [
                        ("copy_project_wbs", "WBS", &mut ch.wbs, 12., wbs_optional),
                        (
                            "copy_project_activities",
                            "Activities",
                            &mut ch.activities,
                            12.,
                            true,
                        ),
                        (
                            "copy_project_relationships",
                            "Relationships",
                            &mut ch.relationships,
                            30.,
                            activities,
                        ),
                        (
                            "copy_project_baselines",
                            "Baselines",
                            &mut ch.baselines,
                            12.,
                            true,
                        ),
                        (
                            "copy_project_codes",
                            "Project Codes",
                            &mut ch.project_codes,
                            12.,
                            true,
                        ),
                        (
                            "copy_project_notebook",
                            "Notebook Topics",
                            &mut ch.notebook,
                            12.,
                            true,
                        ),
                        (
                            "copy_project_funding",
                            "Funding",
                            &mut ch.funding,
                            12.,
                            true,
                        ),
                        (
                            "copy_project_expenses",
                            "Expenses",
                            &mut ch.expenses,
                            12.,
                            true,
                        ),
                    ];
                    for (k, (key, caption, value, x, enabled)) in checks.into_iter().enumerate() {
                        c.checkbox(
                            key,
                            x,
                            30. + 22. * k as f32,
                            260.,
                            18.,
                            caption,
                            value,
                            enabled,
                        );
                    }
                    if !ch.activities {
                        ch.relationships = false;
                    }
                    let out = kit::button_column(
                        c,
                        W,
                        &[
                            Entry::ok("copy_project_ok"),
                            Entry::cancel("copy_project_cancel"),
                        ],
                    );
                    answer = out.clicked.map(|key| key == "copy_project_ok");
                },
            );
            self.record_dialog_actions(actions);
            if closed && answer.is_none() {
                answer = Some(false);
            }
            match answer {
                Some(true) => {
                    if let Err(e) = self.paste_project_copy(d.source, Some(d.eps), d.choice, time) {
                        self.error(e);
                    }
                }
                Some(false) => {}
                None if escape => {}
                None => self.views.projects.copy_dialog = Some(d),
            }
            return;
        }
        if let Some(d) = self.views.projects.delete.take() {
            let mut rects = vec![];
            let open_note = if d.open {
                "It is currently open and will be closed first. "
            } else {
                ""
            };
            let clicked = crate::kit_window::message_box(
                ctx,
                "delete_project",
                "Delete Project",
                crate::kit_window::MessageIcon::Question,
                &if d.sample {
                    format!(
                        "Remove the sample project {}?\n{open_note}Its {} activities and {} baseline(s) are removed from the workspace, with the calendars, resources, codes and notebook topics only the sample uses.",
                        d.title, d.activities, d.baselines
                    )
                } else {
                    format!(
                        "Delete project {}?\n{open_note}{} activities and {} baseline(s) are removed from the workspace.",
                        d.title, d.activities, d.baselines
                    )
                },
                &["Yes", "No"],
                crate::kit_window::Question::DeleteProject.default_in(&["Yes", "No"]),
                &mut rects,
            );
            self.record_dialog_actions(rects);
            let answer = clicked.map(|k| k == 0);
            match answer {
                Some(true) => {
                    if let Err(e) = self.delete_workspace_project(d.id, time) {
                        self.error(e);
                    }
                }
                Some(false) => {}
                None if escape => {}
                None => self.views.projects.delete = Some(d),
            }
            return;
        }
        if let Some(d) = self.views.projects.folder_delete.take() {
            let mut rects = vec![];
            let clicked = crate::kit_window::message_box(
                ctx,
                "delete_folder",
                "Delete EPS Node",
                crate::kit_window::MessageIcon::Question,
                &d.text,
                &["Yes", "No"],
                crate::kit_window::Question::EnterpriseDelete.default_in(&["Yes", "No"]),
                &mut rects,
            );
            self.record_dialog_actions(rects);
            match clicked.map(|k| k == 0) {
                Some(true) => {
                    if let Err(e) = self.delete_folder(d.eps, time) {
                        self.error(e);
                    }
                }
                Some(false) => {}
                None if escape => {}
                None => self.views.projects.folder_delete = Some(d),
            }
        }
    }

    /// Open the Copy Project Options for a paste of `source` under `eps`.
    pub(crate) fn open_copy_options(&mut self, source: i64, eps: i64) {
        let Ok(p) = self.project_for_copy(source) else {
            self.views.projects.copy = None;
            return;
        };
        let wbs_optional = wbs::project_node(&p).is_some();
        self.views.projects.copy_dialog = Some(CopyDialog {
            source,
            eps,
            title: p.model.project.name.clone(),
            choice: CopyChoice::default(),
            wbs_optional,
        });
    }
}

#[cfg(test)]
mod tests;
