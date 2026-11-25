//! Workflow: notebooks and project codes on a small job built here (the
//! synthetic starter with a project node over its four WBS elements, two
//! notebook topics, Scope for activities, WBS elements and projects and
//! Access for activities and projects only, and a project code Region with
//! the values N North and S South).
//!
//! - Project Details, Codes tab: Assign opens Assign Project Codes; Assign
//!   there gives the project a value and a second value of the same code
//!   replaces it; Remove asks first (Enter is Yes, as for an activity code)
//!   and takes it off. Each change is one undo step. Its button column is
//!   that of every Assign dialog (Close, Assign, Remove, New..., Help) with
//!   the enabled rules of Assign Activity Codes.
//! - Project Details and WBS Details, Notebook tab: Add opens Assign
//!   Notebook Topic with the topics offered there; Modify opens the text,
//!   typing writes it, the toolbar makes the selection bold and a list, and
//!   Delete takes the topic away. Each command is one undo step.
//! - Activity Details uses the same notebook pane and toolbar.
//!
//! The checks read the file's own tables (`PROJPCAT`, `WBSMEMO`, `TASKMEMO`)
//! and the controls by their action keys only, so the scenario also runs on
//! a build without these features and fails there on the rule it checks.
use super::*;

/// The project node the job adds over the starter's elements.
const NODE: i64 = 5;
/// One of the starter's WBS elements.
const ELEMENT: i64 = 30;

fn job() -> ProjectData {
    let mut doc = openxer_xer::starter().document;
    let wbs = doc.table_mut("PROJWBS");
    for r in 0..wbs.rows.len() {
        wbs.set(r, "parent_wbs_id", NODE.to_string());
        wbs.set(r, "proj_node_flag", "N".into());
    }
    wbs.add(&[
        ("wbs_id", NODE.to_string()),
        ("proj_id", "1".into()),
        ("wbs_short_name", "RIVERBEND".into()),
        ("wbs_name", "Riverbend".into()),
        ("seq_num", "0".into()),
        ("proj_node_flag", "Y".into()),
    ]);
    for (id, name, wbs_flag) in [(901, "Scope", "Y"), (902, "Access", "N")] {
        doc.table_mut("MEMOTYPE").add(&[
            ("memo_type_id", id.to_string()),
            ("seq_num", id.to_string()),
            ("eps_flag", "N".into()),
            ("proj_flag", "Y".into()),
            ("wbs_flag", wbs_flag.into()),
            ("task_flag", "Y".into()),
            ("memo_type", name.into()),
        ]);
    }
    doc.table_mut("PCATTYPE").add(&[
        ("proj_catg_type_id", "31".into()),
        ("seq_num", "1".into()),
        ("proj_catg_short_len", "10".into()),
        ("proj_catg_type", "Region".into()),
    ]);
    for (id, short, name) in [(310, "N", "North"), (311, "S", "South")] {
        doc.table_mut("PCATVAL").add(&[
            ("proj_catg_id", id.to_string()),
            ("proj_catg_type_id", "31".into()),
            ("seq_num", id.to_string()),
            ("proj_catg_short_name", short.into()),
            ("proj_catg_name", name.into()),
        ]);
    }
    let mut p = ProjectData::from_document(doc, 1, "notebooks".into()).unwrap();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn rows(d: &Driver, table: &str) -> Vec<Vec<String>> {
    d.app
        .project
        .document
        .table(table)
        .map(|t| t.rows.clone())
        .unwrap_or_default()
}

/// A column of every row of `table` whose `key` column is `id`.
fn column(d: &Driver, table: &str, key: &str, id: i64, column: &str) -> Vec<String> {
    let Some(t) = d.app.project.document.table(table) else {
        return vec![];
    };
    t.rows
        .iter()
        .filter(|r| t.value(r, key) == id.to_string())
        .map(|r| t.value(r, column).to_string())
        .collect()
}

fn undo_steps(d: &Driver) -> usize {
    d.app.history.counts().0
}

fn undo(d: &mut Driver) {
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
}

fn shown(d: &mut Driver, text: &str) -> bool {
    d.texts().iter().any(|(t, _)| t == text)
}

/// Click the visible text `text` (a list line).
#[track_caller]
fn pick(d: &mut Driver, text: &str) {
    d.frames(2);
    assert!(d.click_text(text), "{text:?} is listed");
    d.frames(2);
}

/// Show Project Details tab `name` for the open project.
fn project_tab(d: &mut Driver, name: &str) {
    let id = d.app.open_set.active.expect("the open project has a row");
    projects_select(d, id);
    let tab = views::projects::TABS
        .iter()
        .position(|t| *t == name)
        .unwrap();
    d.click(&format!("projects_tab_{tab}"));
    d.frames(2);
}

#[test]
fn workflow_project_codes_assign_replace_and_remove() {
    let mut d = Driver::new("wf-project-codes", job());
    project_tab(&mut d, "Codes");
    assert!(rows(&d, "PROJPCAT").is_empty());
    let steps = undo_steps(&d);

    // Assign opens Assign Project Codes with the button column of Assign
    // Activity Codes.
    d.click("project_codes_assign");
    for key in [
        "project_code_pick_close",
        "project_code_pick_assign",
        "project_code_pick_remove",
        "project_code_pick_new",
        "project_code_pick_help",
    ] {
        assert!(d.has(key), "{key} is drawn");
    }
    pick(&mut d, "South");
    d.click("project_code_pick_assign");
    d.assert_clean("assign South");
    assert_eq!(
        column(&d, "PROJPCAT", "proj_id", 1, "proj_catg_id"),
        vec!["311".to_string()]
    );
    assert_eq!(undo_steps(&d), steps + 1, "one undo step");

    // A second value of the same code replaces the first.
    pick(&mut d, "North");
    d.click("project_code_pick_assign");
    assert_eq!(
        column(&d, "PROJPCAT", "proj_id", 1, "proj_catg_id"),
        vec!["310".to_string()]
    );
    assert_eq!(undo_steps(&d), steps + 2);
    d.click("project_code_pick_close");
    d.frames(2);
    assert!(!d.has("project_code_pick_assign"), "Close closes it");
    assert!(shown(&mut d, "Region"), "the Codes tab lists the code");
    assert!(shown(&mut d, "North"));

    // Remove asks first; Enter is Yes and takes the value off.
    pick(&mut d, "North");
    d.click("project_codes_remove");
    d.frames(2);
    assert!(d.has("remove_project_code_yes"), "Remove asks first");
    assert_eq!(
        crate::dialog_keys::top_default_label(&d.ctx).as_deref(),
        Some("Yes"),
        "Yes is the default, as for an activity code"
    );
    d.key(Key::Enter);
    d.frames(2);
    d.assert_clean("remove");
    assert!(column(&d, "PROJPCAT", "proj_id", 1, "proj_catg_id").is_empty());
    assert_eq!(undo_steps(&d), steps + 3, "one undo step");

    // One undo brings the value back.
    undo(&mut d);
    assert_eq!(
        column(&d, "PROJPCAT", "proj_id", 1, "proj_catg_id"),
        vec!["310".to_string()]
    );
}

/// Assign Project Codes keeps the column of every Assign dialog: Close
/// first, then Assign, Remove, New..., Help last, and the same enabled
/// rules as Assign Activity Codes (Remove only on a value the project has,
/// New only once a value is selected).
#[test]
fn workflow_assign_project_codes_keeps_the_assign_dialog_column() {
    let mut d = Driver::new("wf-project-codes-column", job());
    project_tab(&mut d, "Codes");
    d.click("project_codes_assign");
    d.frames(2);
    let keys = [
        "project_code_pick_close",
        "project_code_pick_assign",
        "project_code_pick_remove",
        "project_code_pick_new",
        "project_code_pick_help",
    ];
    for key in keys {
        assert!(d.has(key), "{key} is drawn");
    }
    let tops: Vec<f32> = keys.iter().map(|k| d.rect(k).top()).collect();
    assert!(
        tops.windows(2).all(|w| w[0] < w[1]),
        "Close, Assign, Remove, New..., Help from the top: {tops:?}"
    );
    let steps = undo_steps(&d);

    // Nothing selected: Assign, Remove and New... do nothing.
    d.click("project_code_pick_assign");
    d.click("project_code_pick_remove");
    d.click("project_code_pick_new");
    d.frames(2);
    assert!(rows(&d, "PROJPCAT").is_empty(), "Assign is off");
    assert!(d.app.detail.removal.is_none(), "Remove is off");
    assert!(d.app.detail.new_code.is_none(), "New... is off");
    assert_eq!(undo_steps(&d), steps);

    // A value the project does not have: Remove stays off, New... opens
    // Add Code Value for a project code, and Cancel leaves it unchanged.
    pick(&mut d, "South");
    d.click("project_code_pick_remove");
    d.frames(2);
    assert!(d.app.detail.removal.is_none(), "Remove is off on South");
    d.click("project_code_pick_new");
    d.frames(2);
    assert!(
        d.app.detail.new_code.as_ref().is_some_and(|c| c.project),
        "New... opens Add Code Value for the project code"
    );
    d.click("new_code_cancel");
    d.frames(2);
    assert!(d.app.detail.new_code.is_none(), "Cancel closes it");
    assert_eq!(undo_steps(&d), steps, "Cancel changes nothing");

    // Once the project has South, Remove asks; No keeps it.
    d.click("project_code_pick_assign");
    d.frames(2);
    assert_eq!(
        column(&d, "PROJPCAT", "proj_id", 1, "proj_catg_id"),
        vec!["311".to_string()]
    );
    d.click("project_code_pick_remove");
    d.frames(2);
    assert!(d.has("remove_project_code_no"), "Remove asks first");
    d.click("remove_project_code_no");
    d.frames(2);
    assert_eq!(
        column(&d, "PROJPCAT", "proj_id", 1, "proj_catg_id"),
        vec!["311".to_string()],
        "No keeps the value"
    );
    assert_eq!(undo_steps(&d), steps + 1, "only the Assign is a step");
    d.click("project_code_pick_close");
    d.frames(2);
    assert!(!d.has("project_code_pick_assign"), "Close closes it");
    d.assert_clean("assign project codes column");
}

#[test]
fn workflow_project_notebook_add_type_format_and_delete() {
    let mut d = Driver::new("wf-project-notebook", job());
    project_tab(&mut d, "Notebook");
    let memo = |d: &Driver| column(d, "WBSMEMO", "wbs_id", NODE, "wbs_memo");

    // Add: both topics are offered to a project.
    d.click("project_notebook_add");
    assert!(d.has("topic_pick_assign"), "Add opens the topic picker");
    assert!(shown(&mut d, "Access"), "a project topic is offered");
    pick(&mut d, "Scope");
    d.click("topic_pick_assign");
    d.assert_clean("add topic");
    assert!(!d.has("topic_pick_assign"), "the picker closes");
    assert_eq!(memo(&d).len(), 1, "the project has the topic");
    assert!(shown(&mut d, "Scope"));

    // Modify opens the text; typing writes it.
    d.click("project_notebook_modify");
    d.type_text("Crane on site");
    d.frames(2);
    assert!(memo(&d)[0].contains("Crane on site"), "{:?}", memo(&d));

    // Select it all and make it bold, then a bulleted list.
    d.key_with(Key::A, ctrl());
    let steps = undo_steps(&d);
    d.click("project_notebook_bold");
    assert!(
        memo(&d)[0].contains("<b>Crane on site</b>"),
        "{:?}",
        memo(&d)
    );
    assert_eq!(undo_steps(&d), steps + 1, "bold is one undo step");
    d.click("project_notebook_bullets");
    assert!(memo(&d)[0].contains("<ul><li><b>Crane on site</b></li></ul>"));
    assert_eq!(undo_steps(&d), steps + 2, "the list is one undo step");

    // One undo takes the list away and keeps the bold text.
    undo(&mut d);
    let text = memo(&d)[0].clone();
    assert!(text.contains("<b>Crane on site</b>") && !text.contains("<ul>"));

    // Delete takes the topic away, one undo step.
    let steps = undo_steps(&d);
    d.click("project_notebook_delete");
    d.assert_clean("delete topic");
    assert!(memo(&d).is_empty());
    assert_eq!(undo_steps(&d), steps + 1);
    undo(&mut d);
    assert_eq!(memo(&d).len(), 1, "undo brings it back");
}

#[test]
fn workflow_wbs_and_activity_notebooks_share_the_pane() {
    let mut d = Driver::new("wf-wbs-notebook", job());
    show(&mut d, "wbs");
    wbs_click(&mut d, ELEMENT);
    d.click("wbs_tab_1");
    let memo = |d: &Driver| column(d, "WBSMEMO", "wbs_id", ELEMENT, "wbs_memo");

    // A WBS element is offered the WBS topics only.
    d.click("wbs_notebook_add");
    assert!(d.has("topic_pick_assign"), "Add opens the topic picker");
    assert!(
        !shown(&mut d, "Access"),
        "a project-only topic is not offered"
    );
    pick(&mut d, "Scope");
    d.click("topic_pick_assign");
    assert_eq!(memo(&d).len(), 1, "the element has the topic");

    // Type two lines and number them.
    d.click("wbs_notebook_modify");
    d.type_text("Set pipe racks");
    d.key(Key::Enter);
    d.type_text("Hydrotest");
    d.frames(2);
    d.key_with(Key::A, ctrl());
    let steps = undo_steps(&d);
    d.click("wbs_notebook_numbering");
    assert!(
        memo(&d)[0].contains("<ol><li>Set pipe racks</li><li>Hydrotest</li></ol>"),
        "{:?}",
        memo(&d)
    );
    assert_eq!(undo_steps(&d), steps + 1, "one undo step");
    undo(&mut d);
    assert!(memo(&d)[0].contains("Set pipe racks<br>Hydrotest"));

    // Activity Details: the same pane, its toolbar keyed for the tab.
    d.home();
    let i = index_of(&d.app.project, "RB-1020");
    select_row(&mut d, i);
    d.app.show_detail_tab(crate::details::DetailTab::Notebook);
    d.frames(2);
    d.click("detail_tab_notebook");
    d.click("detail_notebook_add");
    let task_id = d.app.project.model.activities[i].task_id;
    pick(&mut d, "Access");
    d.click("picker_assign");
    let notes = |d: &Driver| column(d, "TASKMEMO", "task_id", task_id, "task_memo");
    assert_eq!(notes(&d).len(), 1, "the activity has the topic");
    d.click("detail_notebook_modify");
    d.type_text("Gate 4 only");
    d.key_with(Key::A, ctrl());
    d.click("detail_notebook_italic");
    assert!(
        notes(&d)[0].contains("<i>Gate 4 only</i>"),
        "{:?}",
        notes(&d)
    );
    undo(&mut d);
    assert!(
        !notes(&d)[0].contains("<i>"),
        "one undo takes the italic away"
    );
}
