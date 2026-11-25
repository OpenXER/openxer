//! Workflow: Edit > Cut marks activities without asking or changing
//! anything; Edit > Paste of a cut selection in its own project moves the
//! activities to the target's WBS with no dialogs, keeping their IDs,
//! relationships and everything else, in one undo step. Pasting into the
//! WBS they are in is a no-op that selects them. A later Copy or Cut
//! replaces the cut, and Copy then Paste still makes new activities through
//! Copy Activity Options. Pasted into another open project, the cut
//! activities move there too: same Activity ID, gone from their project,
//! relationships with activities left behind kept as relationships between
//! the projects, one undo step for both; an Activity ID the target already
//! uses refuses the paste and keeps the cut.
use super::*;
use crate::grid::group::BandTarget;

/// Every relationship by Activity ID, sorted.
fn links(p: &ProjectData) -> Vec<(String, String)> {
    let code = |i: usize| p.model.activities[i].task_code.clone();
    let mut all: Vec<_> = p
        .model
        .links
        .iter()
        .map(|l| (code(l.pred), code(l.succ)))
        .collect();
    all.sort();
    all
}

fn wbs_of(p: &ProjectData, code: &str) -> i64 {
    let task = p.model.activities[index_of(p, code)].task_id;
    p.details[&task].wbs
}

/// Choose an Edit menu command.
fn edit_menu(d: &mut Driver, item: &str) {
    d.home();
    d.click("menu_bar_edit");
    d.click(item);
    d.frames(2);
}

/// Select activity `code` in the table.
fn select(d: &mut Driver, code: &str) {
    d.home();
    let i = index_of(&d.app.project, code);
    select_row(d, i);
}

/// Click the band of WBS element `wbs` in the activity table.
fn select_wbs_band(d: &mut Driver, wbs: i64) {
    d.home();
    d.frames(1);
    let k = d
        .app
        .grid
        .bands
        .borrow()
        .iter()
        .position(|b| b.target == BandTarget::Wbs(wbs))
        .expect("the table shows the WBS band");
    let r = d.rect(&format!("grid_band_{k}"));
    d.click_at(Pos2::new(r.left() + 120., r.center().y));
    d.frames(1);
    assert!(d.app.grid.cells.selected_band.is_some(), "the band selects");
    assert_eq!(d.app.selected, None);
}

/// No dialog of the paste path is open.
fn no_paste_dialogs(d: &Driver, what: &str) {
    assert!(d.app.delete_confirm.is_none(), "{what}: no delete question");
    assert!(
        d.app.editing.dialogs.copy_options.is_none() && !d.has("copy_options_ok"),
        "{what}: no Copy Activity Options"
    );
    assert!(!d.has("renumber_ok"), "{what}: no Renumber Activity IDs");
}

#[test]
fn workflow_cut_and_paste_moves_activities_between_wbs_elements() {
    let mut d = fresh("wf-cut-paste");
    // RB-1050 (Procurement, RB.2) has predecessor RB-1030 and successor
    // RB-1080. RB-1040 is in the same element; RB-1070 is in Construction.
    let start = d.app.project.clone();
    let procurement = wbs_of(&start, "RB-1050");
    let construction = wbs_of(&start, "RB-1070");
    assert_eq!(wbs_of(&start, "RB-1040"), procurement);
    assert_ne!(procurement, construction);
    let task = start.model.activities[index_of(&start, "RB-1050")].task_id;
    let count = start.model.activities.len();
    let undo = d.app.history.counts().0;

    // Cut: no question, nothing changes, the row stays.
    select(&mut d, "RB-1050");
    edit_menu(&mut d, "edit_cut");
    no_paste_dialogs(&d, "cut");
    assert_eq!(d.app.project.model, start.model, "cut changes nothing");
    assert_eq!(d.app.history.counts().0, undo, "cut is no undo step");
    assert!(d.app.editing.clipboard.as_ref().is_some_and(|c| c.cut));

    // Paste on another activity of the same WBS: no dialogs, no new
    // activity, nothing recorded; the cut activity becomes the selection.
    select(&mut d, "RB-1040");
    edit_menu(&mut d, "edit_paste");
    no_paste_dialogs(&d, "paste in the same WBS");
    d.assert_clean("paste in the same WBS");
    assert_eq!(d.app.project.model, start.model);
    assert_eq!(d.app.history.counts().0, undo, "a no-op records nothing");
    assert_eq!(d.app.selected, Some(index_of(&start, "RB-1050")));

    // Paste with the Construction band selected: a move. Same ID, same
    // task, same relationships and dates, WBS changed, one undo step.
    select_wbs_band(&mut d, construction);
    edit_menu(&mut d, "edit_paste");
    no_paste_dialogs(&d, "paste on a band");
    d.assert_clean("paste on a band");
    let p = &d.app.project;
    assert_eq!(p.model.activities.len(), count, "no new activity");
    assert_eq!(wbs_of(p, "RB-1050"), construction, "moved");
    assert_eq!(p.model.activities[index_of(p, "RB-1050")].task_id, task);
    assert_eq!(p.model, start.model, "ID, links and dates unchanged");
    assert_eq!(links(p), links(&start));
    let mut details = p.details[&task].clone();
    details.wbs = procurement;
    assert_eq!(details, start.details[&task], "only the WBS changed");
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");
    assert_eq!(d.app.selected, Some(index_of(p, "RB-1050")));
    // It survives an XER round trip.
    let reread = xer_round_trip(&mut d, "wf-cut-paste");
    assert_eq!(wbs_of(&reread, "RB-1050"), construction);
    assert_eq!(links(&reread), links(&start));

    // One undo puts it back.
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(wbs_of(&d.app.project, "RB-1050"), procurement, "undone");
    assert_eq!(d.app.history.counts().0, undo);

    // The cut is still marked: Paste with an activity of Construction
    // selected moves it there too.
    select(&mut d, "RB-1070");
    edit_menu(&mut d, "edit_paste");
    no_paste_dialogs(&d, "paste on an activity");
    assert_eq!(wbs_of(&d.app.project, "RB-1050"), construction);
    assert_eq!(d.app.project.model.activities.len(), count);
    assert_eq!(d.app.history.counts().0, undo + 1);
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(wbs_of(&d.app.project, "RB-1050"), procurement);

    // A later Cut replaces the cut: only the last one moves.
    select(&mut d, "RB-1050");
    edit_menu(&mut d, "edit_cut");
    select(&mut d, "RB-1040");
    edit_menu(&mut d, "edit_cut");
    select(&mut d, "RB-1070");
    edit_menu(&mut d, "edit_paste");
    no_paste_dialogs(&d, "second cut");
    assert_eq!(wbs_of(&d.app.project, "RB-1040"), construction);
    assert_eq!(wbs_of(&d.app.project, "RB-1050"), procurement);
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(d.app.project.model, start.model);

    // A later Copy replaces the cut: Paste then makes a new activity
    // through Copy Activity Options and RB-1050 stays where it is.
    select(&mut d, "RB-1050");
    edit_menu(&mut d, "edit_cut");
    edit_menu(&mut d, "edit_copy");
    assert!(d.app.editing.clipboard.as_ref().is_some_and(|c| !c.cut));
    select(&mut d, "RB-1070");
    edit_menu(&mut d, "edit_paste");
    assert!(
        d.app.editing.dialogs.copy_options.is_some(),
        "Copy then Paste asks Copy Activity Options"
    );
    d.click("copy_options_ok");
    d.frames(2);
    d.assert_clean("copy and paste");
    let p = &d.app.project;
    assert_eq!(p.model.activities.len(), count + 1, "a new activity");
    assert_eq!(wbs_of(p, "RB-1050"), procurement, "the original stays");
    assert_eq!(d.app.history.counts().0, undo + 1);
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(d.app.project.model, start.model, "back to the start");
}

/// The two-project sample file (RIVERBEND, project ID 1, and NORTHYARD,
/// project ID 2, joined by RB-1030 FS NY-2000), both in the workspace and
/// open together.
fn two_open_projects(name: &str) -> (Driver, i64, i64) {
    let doc = openxer_xer::starter_portfolio();
    let mut rb = ProjectData::from_document(doc.clone(), 1, "portfolio.xer".into()).unwrap();
    schedule_model(&mut rb.model, &rb.resources).unwrap();
    let mut ny = ProjectData::from_document(doc, 2, "portfolio.xer".into()).unwrap();
    schedule_model(&mut ny.model, &ny.resources).unwrap();
    let mut d = Driver::new(name, rb);
    // Both projects' rows on screen at once, so a row click lands.
    d.screen = Vec2::new(1600., 1800.);
    let rb_id = d
        .app
        .open_set
        .active
        .expect("the first project is in the workspace");
    let ny_id = d.app.store.save(&ny).unwrap();
    open_projects(&mut d, &[rb_id, ny_id]);
    show(&mut d, "activities");
    (d, rb_id, ny_id)
}

/// Click the row of Activity ID `code`, whichever open project holds it.
fn select_code(d: &mut Driver, code: &str) {
    d.home();
    d.frames(1);
    let i = index_of(d.app.shown(), code);
    // The rows of every open project, as the table draws them.
    let rows = d.app.shown_rows();
    let k = rows
        .iter()
        .position(|r| *r == grid::Row::Activity(i))
        .expect("the activity is listed");
    let (top, height) = d.app.grid_row_tops(&rows)[k];
    let pos = Pos2::new(
        d.rect("grid_header_activity_id").center().x,
        top + height / 2.,
    );
    d.click_at(pos);
    if d.app.edit_cell.is_some() {
        d.key(Key::Escape);
    }
    let selected = d
        .app
        .selected
        .map(|k| d.app.project.model.activities[k].task_code.clone());
    assert_eq!(
        selected.as_deref(),
        Some(code),
        "row click at {pos:?} (row {i}) selects {code}; current project {}, status {:?}",
        d.app.project.model.project.name,
        d.app.status
    );
}

/// Relationships between open projects, owned by `p`, as (predecessor
/// project ID, predecessor task, successor task).
fn cross(p: &ProjectData) -> Vec<(i64, i64, i64)> {
    let mut all: Vec<_> = p
        .external_links()
        .iter()
        .map(|x| (x.pred_project, x.pred_task, x.succ_task))
        .collect();
    all.sort();
    all
}

fn codes(p: &ProjectData) -> Vec<String> {
    p.model
        .activities
        .iter()
        .map(|a| a.task_code.clone())
        .collect()
}

#[test]
fn workflow_cut_and_paste_moves_an_activity_into_another_open_project() {
    let (mut d, rb_id, ny_id) = two_open_projects("wf-cut-across");
    let rb0 = open_project(&d, rb_id).clone();
    let ny0 = open_project(&d, ny_id).clone();
    let key = rb0.model.activities[index_of(&rb0, "RB-1050")].task_id;
    let (k1030, k1080) = (
        rb0.model.activities[index_of(&rb0, "RB-1030")].task_id,
        rb0.model.activities[index_of(&rb0, "RB-1080")].task_id,
    );
    let key_free = !ny0.model.activities.iter().any(|a| a.task_id == key)
        && ny0.document.table("TASK").is_none_or(|t| {
            !t.rows
                .iter()
                .any(|r| t.value(r, "task_id").trim() == key.to_string())
        });
    let steps = d.app.open_set.undo.len();

    // Cut RB-1050 (RB-1030 FS RB-1050 FS RB-1080): nothing asks or changes.
    select_code(&mut d, "RB-1050");
    edit_menu(&mut d, "edit_cut");
    no_paste_dialogs(&d, "cut");
    assert_eq!(codes(open_project(&d, rb_id)), codes(&rb0));

    // Paste on NY-2010 of the other project: no dialogs, a move.
    select_code(&mut d, "NY-2010");
    let target_wbs = d.app.project.details
        [&d.app.project.model.activities[index_of(&d.app.project, "NY-2010")].task_id]
        .wbs;
    edit_menu(&mut d, "edit_paste");
    no_paste_dialogs(&d, "paste into the other project");
    d.assert_clean("paste into the other project");
    assert!(
        d.app.status.contains("moved to NORTHYARD"),
        "{}",
        d.app.status
    );
    let rb = open_project(&d, rb_id).clone();
    let ny = open_project(&d, ny_id).clone();
    assert!(
        !codes(&rb).contains(&"RB-1050".to_string()),
        "it left its project"
    );
    assert_eq!(rb.model.activities.len(), rb0.model.activities.len() - 1);
    assert_eq!(ny.model.activities.len(), ny0.model.activities.len() + 1);
    let moved = &ny.model.activities[index_of(&ny, "RB-1050")];
    let new_key = moved.task_id;
    if key_free {
        assert_eq!(new_key, key, "the internal key is kept");
    }
    assert_eq!(ny.details[&new_key].wbs, target_wbs, "in the target's WBS");
    // Its relationships with RB-1030 and RB-1080 are now between the
    // projects, each owned by the successor's project.
    let mut want_ny = cross(&ny0);
    want_ny.push((1, k1030, new_key));
    want_ny.sort();
    assert_eq!(cross(&ny), want_ny, "RB-1030 FS RB-1050 is kept");
    assert_eq!(
        cross(&rb),
        vec![(2, new_key, k1080)],
        "RB-1050 FS RB-1080 is kept"
    );
    assert!(links(&rb)
        .iter()
        .all(|(p, s)| p != "RB-1050" && s != "RB-1050"));
    assert_eq!(d.app.open_set.undo.len(), steps + 1, "one undo step");

    // One undo puts it back in its project with its relationships.
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    let rb = open_project(&d, rb_id).clone();
    let ny = open_project(&d, ny_id).clone();
    assert_eq!(codes(&rb), codes(&rb0), "undone in RIVERBEND");
    assert_eq!(links(&rb), links(&rb0));
    assert_eq!(codes(&ny), codes(&ny0), "undone in NORTHYARD");
    assert_eq!(cross(&ny), cross(&ny0));
    assert!(cross(&rb).is_empty());

    // An Activity ID the target already uses: the paste is refused, nothing
    // moves and the cut is kept.
    select_code(&mut d, "NY-2020");
    let i = d.app.selected.unwrap();
    assert!(d.app.apply_cell(i, "activity_id", "RB-1060"));
    // RIVERBEND's own RB-1060: with its project current, its rows come
    // first, so the code finds it rather than the renamed one.
    select_code(&mut d, "RB-1070");
    select_code(&mut d, "RB-1060");
    assert_eq!(d.app.open_set.active, Some(rb_id));
    edit_menu(&mut d, "edit_cut");
    select_code(&mut d, "NY-2010");
    let before_rb = codes(open_project(&d, rb_id));
    let before_ny = codes(open_project(&d, ny_id));
    edit_menu(&mut d, "edit_paste");
    no_paste_dialogs(&d, "refused paste");
    let message = d.app.error.take().expect("the paste is refused");
    assert!(message.contains("RB-1060"), "{message}");
    assert_eq!(codes(open_project(&d, rb_id)), before_rb, "nothing moved");
    assert_eq!(codes(open_project(&d, ny_id)), before_ny);
    assert!(
        d.app.editing.clipboard.as_ref().is_some_and(|c| c.cut),
        "the cut is kept"
    );
}
