//! Workflow: internal project numbers. Two projects made with File > New
//! each get a number of their own, so an activity of one can be linked to
//! an activity of the other through Activity Details, Predecessors, Assign;
//! the link survives a save, a restart with both reopened and an XER export.
//! A pasted copy gets its own number too. Two stored projects that share a
//! number (as File > New made them before) are told apart when they are
//! opened together: the later one gets a fresh number, with every row that
//! carries it, and links between them then work.
use super::*;
use crate::open_set::compose;

/// The internal number workspace project `id` is stored under.
fn number(d: &Driver, id: i64) -> i64 {
    d.app.store.source(id).unwrap().1
}

/// Every project number the stored text of workspace project `id` holds.
fn stored_numbers(d: &Driver, id: i64) -> BTreeSet<i64> {
    let doc = Document::parse(&d.app.store.source(id).unwrap().2).unwrap();
    let mut out = BTreeSet::new();
    for t in &doc.tables {
        if let Some(c) = t.field("proj_id") {
            out.extend(
                t.rows
                    .iter()
                    .filter_map(|r| r.get(c).and_then(|v| v.trim().parse::<i64>().ok())),
            );
        }
    }
    out
}

/// The composite row of the activity named `name`.
fn shown_index(d: &Driver, name: &str) -> usize {
    let shown = d.app.shown();
    (0..shown.model.activities.len())
        .find(|&i| shown.name(i) == name)
        .unwrap_or_else(|| panic!("{name} is listed"))
}

#[test]
fn workflow_two_new_projects_get_their_own_numbers_and_keep_a_link_between_them() {
    let mut d = fresh("qa_project_numbers");
    let sample = d.app.open_set.active.unwrap();
    let a = new_project(&mut d, "PN-A", "Alpha Line", "04-Jan-27", "");
    show(&mut d, "activities");
    add_activity(&mut d, "Demo old line at TP-1", "2d");
    d.key_with(Key::S, ctrl());
    let b = new_project(&mut d, "PN-B", "Bravo Line", "04-Jan-27", "");
    show(&mut d, "activities");
    add_activity(&mut d, "Install new line at TP-1", "3d");
    d.key_with(Key::S, ctrl());
    let (ns, na, nb) = (number(&d, sample), number(&d, a), number(&d, b));
    assert!(
        ns != na && na != nb && ns != nb,
        "each project has its own number: sample {ns}, A {na}, B {nb}"
    );
    assert_eq!(stored_numbers(&d, a), BTreeSet::from([na]));
    assert_eq!(stored_numbers(&d, b), BTreeSet::from([nb]));

    // Both open; in B's activity, Predecessors, Assign, pick A's activity.
    open_projects(&mut d, &[a, b]);
    if d.app.open_set.active != Some(b) {
        d.app.activate_id(b);
    }
    show(&mut d, "activities");
    let succ = shown_index(&d, "Install new line at TP-1");
    // A click on B's row: B becomes (or stays) the current project and its
    // activity is selected (the selection is an index into B's own rows).
    // (The table's own row-click handler: a pointer click on a composite
    // row is not reliable in the headless driver with two projects open.)
    d.app.click_row(succ, egui::Modifiers::default());
    d.frames(2);
    assert_eq!(d.app.open_set.active, Some(b), "B is current");
    let picked = d.app.selected.unwrap_or_else(|| {
        panic!(
            "a row is selected: row {succ} of {}, status {:?}",
            d.app.shown().model.activities.len(),
            d.app.status
        )
    });
    assert_eq!(
        d.app.project.name(picked),
        "Install new line at TP-1",
        "B's activity is selected"
    );
    d.app
        .show_detail_tab(crate::details::DetailTab::Predecessors);
    d.frames(2);
    d.click("detail_tab_predecessors");
    d.click("detail_pred_assign_0");
    d.frames(2);
    assert!(d.app.detail.picker.is_some(), "Assign Predecessors opens");
    assert!(
        d.click_text("Demo old line at TP-1"),
        "A's activity is offered"
    );
    d.click("picker_assign");
    d.frames(2);
    let said = d.app.status.clone();
    if d.has("picker_close") {
        d.click("picker_close");
    }
    d.assert_clean("assign predecessor");
    let links = open_project(&d, b).external_links();
    assert_eq!(links.len(), 1, "B holds the link: {said}");
    assert_eq!(links[0].pred_project, na, "it names A by A's number");
    assert_eq!(
        compose::cross_links(&d.app.open_projects()).len(),
        1,
        "the link resolves to A"
    );

    // Save, quit and start again: both reopen and the link is still there.
    d.key_with(Key::S, ctrl());
    assert!(!d.app.any_dirty());
    restart(&mut d);
    let mut open = d.app.open_ids();
    open.sort();
    assert_eq!(open, vec![a, b]);
    let links = open_project(&d, b).external_links();
    assert_eq!(links.len(), 1, "the link survives the restart");
    assert_eq!(links[0].pred_project, na);
    let resolved = compose::cross_links(&d.app.open_projects());
    assert_eq!(resolved.len(), 1, "and still resolves");
    assert_eq!(
        (number(&d, a), number(&d, b)),
        (na, nb),
        "nothing was renumbered"
    );

    // B's XER text, written and parsed again: one relationship row owned by
    // B names A's number as the predecessor's project. (File > Export is not used
    // here: two File > New projects also share WBS and activity ids, so the
    // export of both into one file is refused.)
    let text = open_project(&d, b).updated_document().text();
    let doc = Document::parse(&text).unwrap();
    let pred = doc.table("TASKPRED").expect("B's relationships");
    let rows: Vec<(String, String)> = pred
        .rows
        .iter()
        .map(|r| {
            (
                pred.value(r, "proj_id").trim().to_string(),
                pred.value(r, "pred_proj_id").trim().to_string(),
            )
        })
        .collect();
    let cross = rows
        .iter()
        .filter(|(own, other)| *own == nb.to_string() && *other == na.to_string())
        .count();
    if cross != 1 {
        let live = open_project(&d, b);
        let held: Vec<(String, String, String, String)> = live
            .document
            .table("TASKPRED")
            .map(|t| {
                t.rows
                    .iter()
                    .map(|r| {
                        let v = |k: &str| t.value(r, k).trim().to_string();
                        (
                            v("proj_id"),
                            v("pred_proj_id"),
                            v("pred_task_id"),
                            v("task_id"),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        // One diagnostic line shown by test runners.
        panic!(
            "the XER text carries the link\nerror: written {rows:?}; held {held:?}; B is {} with links {:?}; A {na}, B {nb}",
            live.model.project.proj_id,
            live.external_links()
        );
    }

    // Projects view Copy and Paste: the copy has a number of its own.
    let before = d.app.workspace_ids();
    projects_select(&mut d, a);
    d.click("dock_copy");
    d.click("projects_row_0");
    d.click("dock_paste");
    d.click("copy_project_ok");
    d.frames(2);
    d.assert_clean("paste project");
    let copy = *d
        .app
        .workspace_ids()
        .iter()
        .find(|id| !before.contains(id))
        .expect("Paste adds a project");
    let nc = number(&d, copy);
    let all: BTreeSet<i64> = d
        .app
        .workspace_ids()
        .iter()
        .map(|&w| number(&d, w))
        .collect();
    assert_eq!(
        all.len(),
        d.app.workspace_ids().len(),
        "no two share a number"
    );
    assert_eq!(stored_numbers(&d, copy), BTreeSet::from([nc]));
    assert_eq!(number(&d, a), na, "the original keeps its number");
}

#[test]
fn workflow_two_stored_projects_with_one_number_are_told_apart_when_opened_together() {
    let mut d = fresh("qa_project_numbers_clash");
    // Two projects stored the way File > New made them before: both 1, as
    // the sample is.
    let mut ids = vec![];
    for (source, name) in [("x.xer", "XRAY"), ("y.xer", "YANKEE")] {
        let mut p = openxer_xer::starter();
        p.source_name = source.into();
        p.model.project.name = name.into();
        schedule_model(&mut p.model, &p.resources).unwrap();
        ids.push(d.app.store.save(&p).unwrap());
    }
    let (x, y) = (ids[0], ids[1]);
    let sample = d.app.open_set.active.unwrap();
    assert_eq!((number(&d, x), number(&d, y)), (1, 1));
    let y_before = stored(&d, y);

    open_projects(&mut d, &[x, y]);
    let (nx, ny) = (number(&d, x), number(&d, y));
    assert_eq!(nx, 1, "the first one opened keeps its number");
    assert!(ny > 1, "the later one gets a fresh number: {ny}");
    assert_eq!(number(&d, sample), 1, "a project not opened is untouched");
    assert!(
        d.app
            .status
            .contains(&format!("internal project number {ny}")),
        "the status line says so: {}",
        d.app.status
    );
    // Every row of the renumbered project follows; its schedule is the same.
    assert_eq!(stored_numbers(&d, y), BTreeSet::from([ny]));
    let y_open = open_project(&d, y);
    assert_eq!(y_open.model.project.proj_id, ny);
    assert_eq!(
        y_open.model.activities.len(),
        y_before.model.activities.len()
    );
    for (p, q) in y_open
        .model
        .activities
        .iter()
        .zip(&y_before.model.activities)
    {
        assert_eq!(
            (p.early_start, p.early_finish, p.total_float),
            (q.early_start, q.early_finish, q.total_float),
            "{}",
            p.task_code
        );
    }
    assert_eq!(y_open.model.links.len(), y_before.model.links.len());

    // A link between them can now be made, and it survives a restart.
    let (px, py) = (open_project(&d, x).clone(), open_project(&d, y).clone());
    let pred = (x, px.model.activities[0].task_id);
    let succ = (y, py.model.activities.last().unwrap().task_id);
    d.app
        .add_cross_link(pred, succ, openxer_engine::model::LinkType::FS, 0)
        .unwrap();
    d.key_with(Key::S, ctrl());
    restart(&mut d);
    let links = open_project(&d, y).external_links();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].pred_project, nx);
    assert_eq!(compose::cross_links(&d.app.open_projects()).len(), 1);
    assert_eq!(
        (number(&d, x), number(&d, y)),
        (nx, ny),
        "reopening renumbers nothing more"
    );
}
