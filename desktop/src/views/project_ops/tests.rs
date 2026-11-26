use super::*;
use crate::qa_sweep::Driver;

fn corpus(name: &str) -> Option<ProjectData> {
    let path = crate::test_corpus::file(&format!("{name}.xer"));
    let doc = Document::read(&path).ok()?;
    let pid = doc.projects()[0].0;
    ProjectData::from_document(doc, pid, format!("{name}.xer")).ok()
}

/// Every activity's scheduled dates and floats, by activity ID.
fn schedule(p: &ProjectData) -> BTreeMap<String, Vec<Option<i64>>> {
    let mut m = p.model.clone();
    crate::scheduling::schedule_model(&mut m, &p.resources).unwrap();
    m.activities
        .iter()
        .map(|a| {
            (
                a.task_code.clone(),
                vec![
                    a.early_start,
                    a.early_finish,
                    a.late_start,
                    a.late_finish,
                    a.total_float,
                    a.free_float,
                ],
            )
        })
        .collect()
}

fn rows(d: &Driver) -> Vec<(i64, String)> {
    d.app.store.list().unwrap()
}

#[test]
fn copy_schedules_to_the_same_dates_as_its_source() {
    let Some(p) = corpus("6031") else {
        println!("SKIPPED: corpus not found");
        return;
    };
    let mut d = Driver::new("copy-corpus", p);
    let source = d.app.workspace_row().unwrap();
    let eps = d.app.workspace().node_of(Some(source));
    let copy = d
        .app
        .paste_project_copy(source, Some(eps), CopyChoice::default(), 1.)
        .unwrap();
    let copied = d.app.store.load(copy).unwrap();
    assert_ne!(copied.model.project.name, d.app.project.model.project.name);
    assert_eq!(
        copied.model.project.name,
        format!("{}-1", d.app.project.model.project.name),
        "a pasted copy gets a new Project ID"
    );
    assert_eq!(
        copied.model.activities.len(),
        d.app.project.model.activities.len()
    );
    assert_eq!(copied.model.links.len(), d.app.project.model.links.len());
    assert_eq!(schedule(&copied), schedule(&d.app.project), "same dates");
    assert_eq!(d.app.workspace().node_of(Some(copy)), eps);
}

#[test]
fn copy_options_leave_out_wbs_activities_and_relationships() {
    let p = crate::views::wbs::tests::with_project_node();
    let node = wbs::project_node(&p).unwrap();
    let all = copy_project(&p, "COPY-1", CopyChoice::default()).unwrap();
    assert_eq!(all.model.project.name, "COPY-1");
    assert_eq!(all.wbs.len(), p.wbs.len());
    let no_wbs = copy_project(
        &p,
        "COPY-2",
        CopyChoice {
            wbs: false,
            ..CopyChoice::default()
        },
    )
    .unwrap();
    assert_eq!(no_wbs.wbs.len(), 1);
    assert!(no_wbs.details.values().all(|d| d.wbs == node));
    assert_eq!(no_wbs.model.activities.len(), p.model.activities.len());
    let no_links = copy_project(
        &p,
        "COPY-3",
        CopyChoice {
            relationships: false,
            ..CopyChoice::default()
        },
    )
    .unwrap();
    assert!(no_links.model.links.is_empty());
    let reread = ProjectData::from_document(
        Document::parse(&no_links.updated_document().text()).unwrap(),
        1,
        "x".into(),
    )
    .unwrap();
    assert!(
        reread.model.links.is_empty(),
        "no relationship rows are left"
    );
    let empty = copy_project(
        &p,
        "COPY-4",
        CopyChoice {
            activities: false,
            ..CopyChoice::default()
        },
    )
    .unwrap();
    assert!(empty.model.activities.is_empty());
    assert_eq!(empty.wbs.len(), p.wbs.len());
    // The starter has no project node: its WBS cannot be left out.
    assert!(copy_project(
        &openxer_xer::starter(),
        "X-1",
        CopyChoice {
            wbs: false,
            ..CopyChoice::default()
        }
    )
    .is_err());
    assert_eq!(free_project_id("A", &["a-1".into(), "A-2".into()]), "A-3");
}

#[test]
fn projects_view_copy_paste_add_and_delete_undo_and_redo() {
    let mut d = Driver::new(
        "projects-ops",
        crate::reports::test_support::scheduled_starter(),
    );
    d.click("directory_projects");
    d.frames(2);
    let start = rows(&d).len();
    let undo = d.app.history.counts().0;
    // Rows: the EPS node, then the open project.
    d.click("projects_row_1");
    assert!(d.app.views.projects.selected.is_some());
    // The open project cannot be deleted.
    assert!(d.has("dock_delete"));
    d.click("dock_copy");
    assert!(d.app.views.projects.copy.is_some());
    d.click("projects_row_0");
    d.click("dock_paste");
    assert!(d.app.views.projects.copy_dialog.is_some());
    // Cancel and Escape leave the workspace alone.
    d.click("copy_project_cancel");
    assert!(d.app.views.projects.copy_dialog.is_none());
    d.click("dock_paste");
    d.key(Key::Escape);
    assert!(d.app.views.projects.copy_dialog.is_none());
    assert_eq!(rows(&d).len(), start);
    assert_eq!(d.app.history.counts().0, undo);
    // OK adds the copy as one undo step.
    d.click("dock_paste");
    d.click("copy_project_relationships");
    d.click("copy_project_ok");
    assert_eq!(rows(&d).len(), start + 1);
    assert_eq!(d.app.history.counts().0, undo + 1);
    let (copy, title) = rows(&d)
        .into_iter()
        .find(|(_, t)| *t != d.app.project.model.project.name)
        .unwrap();
    assert_eq!(title, format!("{}-1", d.app.project.model.project.name));
    let copied = d.app.store.load(copy).unwrap();
    assert!(copied.model.links.is_empty(), "Relationships was unchecked");
    assert_eq!(
        copied.model.activities.len(),
        d.app.project.model.activities.len()
    );
    let ctx = d.ctx.clone();
    d.app.history_move(&ctx, false);
    assert_eq!(rows(&d).len(), start, "undo removes the copy");
    d.app.history_move(&ctx, true);
    assert_eq!(rows(&d).len(), start + 1, "redo puts it back");
    // Add: the New Project dialog for the EPS node; Finish adds the project.
    d.frames(2);
    d.click("projects_row_0");
    d.click("add");
    assert!(d.app.new_project.is_some(), "Add opens the wizard");
    assert_eq!(rows(&d).len(), start + 1, "nothing is added before Finish");
    d.click("new_project_finish");
    d.click("directory_projects");
    assert_eq!(rows(&d).len(), start + 2);
    let added = d.app.store.list().unwrap();
    assert!(added.iter().any(|(_, t)| t.starts_with("NEW-")));
    // Delete the copy: No keeps it, Yes removes it, undo restores it.
    let summary = d
        .app
        .workspace_projects()
        .into_iter()
        .find(|s| s.store_id == Some(copy))
        .unwrap();
    d.app.views.projects.selected = Some((summary.source.clone(), summary.key));
    d.frames(2);
    d.click("dock_delete");
    assert!(d.app.views.projects.delete.is_some());
    d.click("delete_project_no");
    assert_eq!(rows(&d).len(), start + 2);
    d.click("dock_delete");
    d.click("delete_project_yes");
    assert_eq!(rows(&d).len(), start + 1);
    assert!(d.app.store.load(copy).is_err());
    d.app.history_move(&ctx, false);
    assert_eq!(rows(&d).len(), start + 2);
    assert_eq!(d.app.store.load(copy).unwrap().model.project.name, title);
    // Deleting the open project closes it first and enters the empty workspace;
    // Undo brings it back.
    let open = d.app.workspace_row().unwrap();
    d.app.delete_workspace_project(open, 9.).unwrap();
    assert!(d.app.open_set.empty);
    assert_eq!(rows(&d).len(), start + 1);
    d.app.history_move(&ctx, false);
    assert_eq!(rows(&d).len(), start + 2);
}

#[test]
fn deleting_a_project_takes_its_baselines_and_undo_brings_them_back() {
    let mut d = Driver::new(
        "projects-baselines",
        crate::reports::test_support::scheduled_starter(),
    );
    d.app.add_baseline_copy(1.).unwrap();
    let source = d.app.workspace_row().unwrap();
    let copy = d
        .app
        .paste_project_copy(source, None, CopyChoice::default(), 2.)
        .unwrap();
    let c = d.app.store.connection();
    assert_eq!(bstore::count(c, copy), 1, "the baseline was copied too");
    d.app.delete_workspace_project(copy, 3.).unwrap();
    assert_eq!(bstore::count(d.app.store.connection(), copy), 0);
    let ctx = d.ctx.clone();
    d.app.history_move(&ctx, false);
    assert_eq!(bstore::count(d.app.store.connection(), copy), 1);
    assert_eq!(bstore::count(d.app.store.connection(), source), 1);
}

#[test]
fn edit_toolbar_commands_act_on_the_projects_view() {
    let mut d = Driver::new(
        "projects-dock",
        crate::reports::test_support::scheduled_starter(),
    );
    d.click("directory_projects");
    d.frames(2);
    let start = rows(&d).len();
    d.click("projects_row_0");
    d.click("add");
    d.frames(2);
    assert!(
        d.app.new_project.is_some(),
        "Edit toolbar Add opens the wizard"
    );
    d.click("new_project_finish");
    d.click("directory_projects");
    d.frames(2);
    assert_eq!(rows(&d).len(), start + 1, "Finish adds a project");
    d.click("projects_row_1");
    d.click("dock_copy");
    d.frames(1);
    assert!(d.app.views.projects.copy.is_some());
    d.click("projects_row_0");
    d.click("dock_paste");
    d.frames(2);
    assert!(d.app.views.projects.copy_dialog.is_some());
    d.key(Key::Escape);
    assert_eq!(rows(&d).len(), start + 1);
}

#[test]
fn deleting_a_project_undo_restores_the_user_baseline_choices() {
    use crate::baselines::store::Choice;
    let mut d = Driver::new(
        "projects-user-baselines",
        crate::reports::test_support::scheduled_starter(),
    );
    d.app.add_baseline_copy(1.).unwrap();
    let source = d.app.workspace_row().unwrap();
    let copy = d
        .app
        .paste_project_copy(source, None, CopyChoice::default(), 2.)
        .unwrap();
    let c = d.app.store.connection();
    let baseline = bstore::list(c, copy).unwrap()[0].id;
    let chosen = UserBaselines([Choice::Baseline(baseline), Choice::Current, Choice::None]);
    bstore::set_user_baselines(c, copy, chosen).unwrap();
    d.app.delete_workspace_project(copy, 3.).unwrap();
    assert_eq!(
        bstore::user_baselines(d.app.store.connection(), copy),
        UserBaselines::default(),
        "the choices go with the project"
    );
    let ctx = d.ctx.clone();
    d.app.history_move(&ctx, false);
    assert_eq!(
        bstore::user_baselines(d.app.store.connection(), copy),
        chosen,
        "undo restores Primary, Secondary and Tertiary"
    );
    d.app.history_move(&ctx, true);
    assert!(d.app.store.load(copy).is_err(), "redo deletes it again");
    d.app.history_move(&ctx, false);
    assert_eq!(
        bstore::user_baselines(d.app.store.connection(), copy),
        chosen
    );
}

fn rows_of(p: &ProjectData, table: &str) -> usize {
    p.document.table(table).map_or(0, |t| t.rows.len())
}

fn with_copy_extras() -> ProjectData {
    let mut p = crate::views::wbs::tests::with_project_node();
    let pid = p.model.project.proj_id.to_string();
    let node = wbs::project_node(&p).unwrap().to_string();
    let task = p.model.activities[0].task_id.to_string();
    let doc = &mut p.document;
    crate::records::add_row(
        doc,
        "PROJPCAT",
        &[
            ("proj_id", pid.clone()),
            ("proj_catg_type_id", "1".into()),
            ("proj_catg_id", "1".into()),
        ],
    );
    crate::records::add_row(
        doc,
        "WBSMEMO",
        &[
            ("wbs_memo_id", "1".into()),
            ("proj_id", pid.clone()),
            ("wbs_id", node),
            ("memo_type_id", "1".into()),
            ("wbs_memo", "<p>Scope notes</p>".into()),
        ],
    );
    crate::records::add_row(
        doc,
        "TASKMEMO",
        &[
            ("memo_id", "1".into()),
            ("proj_id", pid.clone()),
            ("task_id", task.clone()),
            ("memo_type_id", "1".into()),
            ("task_memo", "<p>Crew note</p>".into()),
        ],
    );
    crate::records::add_row(
        doc,
        "PROJFUND",
        &[
            ("proj_fund_id", "1".into()),
            ("proj_id", pid.clone()),
            ("fund_id", "1".into()),
            ("fund_cost", "1000".into()),
        ],
    );
    crate::records::add_row(
        doc,
        "PROJCOST",
        &[
            ("cost_item_id", "1".into()),
            ("proj_id", pid),
            ("task_id", task),
            ("cost_name", "Crane".into()),
            ("target_cost", "500".into()),
        ],
    );
    p
}

#[test]
fn copy_options_leave_out_project_codes_notebook_funding_and_expenses() {
    let p = with_copy_extras();
    let tables = ["PROJPCAT", "WBSMEMO", "TASKMEMO", "PROJFUND", "PROJCOST"];
    let all = copy_project(&p, "COPY-1", CopyChoice::default()).unwrap();
    for t in tables {
        assert_eq!(rows_of(&all, t), 1, "{t} comes along by default");
    }
    let off = |c: CopyChoice| copy_project(&p, "COPY-2", c).unwrap();
    let d = CopyChoice::default();
    let no_codes = off(CopyChoice {
        project_codes: false,
        ..d
    });
    assert_eq!(rows_of(&no_codes, "PROJPCAT"), 0);
    assert_eq!(rows_of(&no_codes, "PROJFUND"), 1, "only the codes go");
    let no_notes = off(CopyChoice {
        notebook: false,
        ..d
    });
    assert_eq!(rows_of(&no_notes, "WBSMEMO"), 0);
    assert_eq!(rows_of(&no_notes, "TASKMEMO"), 0);
    assert_eq!(rows_of(&no_notes, "PROJPCAT"), 1);
    let no_funding = off(CopyChoice {
        funding: false,
        ..d
    });
    assert_eq!(rows_of(&no_funding, "PROJFUND"), 0);
    assert_eq!(rows_of(&no_funding, "PROJCOST"), 1);
    let no_expenses = off(CopyChoice {
        expenses: false,
        ..d
    });
    assert_eq!(rows_of(&no_expenses, "PROJCOST"), 0);
    assert_eq!(rows_of(&no_expenses, "PROJFUND"), 1);
    // The source is untouched and the left-out rows are gone from the file.
    assert_eq!(rows_of(&p, "PROJCOST"), 1);
    let text = no_expenses.updated_document().text();
    let reread = Document::parse(&text).unwrap();
    assert_eq!(reread.table("PROJCOST").map_or(0, |t| t.rows.len()), 0);
    assert_eq!(reread.table("PROJFUND").map_or(0, |t| t.rows.len()), 1);
}

#[test]
fn copy_project_options_dialog_offers_the_new_choices_and_honours_them() {
    let mut d = Driver::new("projects-copy-extras", with_copy_extras());
    d.click("directory_projects");
    d.frames(2);
    d.click("projects_row_1");
    d.click("dock_copy");
    d.click("projects_row_0");
    d.click("dock_paste");
    for id in [
        "copy_project_codes",
        "copy_project_notebook",
        "copy_project_funding",
        "copy_project_expenses",
    ] {
        assert!(d.has(id), "{id} is offered");
    }
    d.click("copy_project_notebook");
    d.click("copy_project_expenses");
    d.click("copy_project_ok");
    let (copy, _) = rows(&d)
        .into_iter()
        .find(|(_, t)| *t != d.app.project.model.project.name)
        .unwrap();
    let copied = d.app.store.load(copy).unwrap();
    assert_eq!(rows_of(&copied, "WBSMEMO"), 0, "Notebook Topics was off");
    assert_eq!(rows_of(&copied, "TASKMEMO"), 0);
    assert_eq!(rows_of(&copied, "PROJCOST"), 0, "Expenses was off");
    assert_eq!(rows_of(&copied, "PROJPCAT"), 1, "Project Codes stayed on");
    assert_eq!(rows_of(&copied, "PROJFUND"), 1, "Funding stayed on");
}
