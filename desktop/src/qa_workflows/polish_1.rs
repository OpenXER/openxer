//! Workflows: small look follow-ups.
//!
//! The check box that turns a paste step off reads "Skip this dialog next
//! time" on Copy WBS Options, Copy Activity Options and Renumber Activity
//! IDs; Calendars in Use draws its bottom row in the observed order Help,
//! Cancel, OK; the Add under Notes to Resources is always enabled and puts
//! the cursor in the empty entry box; Collapse To is one drop-down with OK,
//! Cancel, Apply and Help; the Funding tab lists rows by Funding Source and
//! a row moves only on a committed change; Resource Shifts and Resource
//! Curves carry menu icons. Each data change is checked against one undo.
use super::*;
use crate::calendars::ops as cal;
use crate::details::DetailTab;
use crate::views::projects::TABS;
use openxer_xer::edit::{funding, records};

const SKIP: &str = "Skip this dialog next time";
const OLD: &str = "Do not show this dialog again.";

fn shown(d: &mut Driver, text: &str) -> bool {
    d.texts().iter().any(|(t, _)| t == text)
}

fn undo(d: &mut Driver) {
    d.home();
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
}

/// Click `action`, failing here when it is not drawn.
#[track_caller]
fn press(d: &mut Driver, action: &str) {
    d.frames(2);
    assert!(d.has(action), "{action} is not drawn");
    d.click(action);
}

/// Keys from left to right by their drawn rectangles.
fn left_to_right(d: &Driver, keys: &[&str]) -> Vec<String> {
    let mut found: Vec<(f32, String)> = keys
        .iter()
        .map(|k| (d.rect(k).left(), k.to_string()))
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|(_, k)| k).collect()
}

fn scheduled() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

#[test]
fn workflow_the_skip_dialog_box_reads_our_own_words_on_every_paste_step() {
    // The reference defaults: a paste asks for new Activity IDs too.
    crate::defaults::use_reference();
    let mut d = Driver::new("wf-polish-skip", scheduled());
    let start = d.app.project.clone();
    let count = start.model.activities.len();
    let steps = d.app.history.counts().0;
    show(&mut d, "wbs");
    // Copy Procurement (WBS 20, two activities) onto Construction (30).
    wbs_click(&mut d, 20);
    press(&mut d, "dock_copy");
    wbs_click(&mut d, 30);
    press(&mut d, "dock_paste");
    d.frames(2);
    for (step, ok) in [
        ("Copy WBS Options", "copy_wbs_options_ok"),
        ("Copy Activity Options", "copy_options_ok"),
        ("Renumber Activity IDs", "renumber_ok"),
    ] {
        assert!(d.has(ok), "{step} is shown");
        assert!(shown(&mut d, SKIP), "{step} says {SKIP:?}");
        assert!(!shown(&mut d, OLD), "{step} no longer says {OLD:?}");
        press(&mut d, ok);
        d.frames(2);
    }
    d.assert_clean("paste through the three steps");
    assert_eq!(d.app.project.model.activities.len(), count + 2, "pasted");
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    undo(&mut d);
    assert_eq!(d.app.project.wbs, start.wbs, "undo takes the elements");
    assert_eq!(d.app.project.model, start.model, "and the activities");
}

/// The starter with a second global calendar two activities use, the
/// first marked the default: (default, used).
fn calendar_sample() -> (ProjectData, String, String) {
    let mut p = openxer_xer::starter();
    let keep = cal::list(&p.document, cal::Kind::Global, 0)[0].id.clone();
    cal::set_default(&mut p, &keep).unwrap();
    let used = cal::add(&mut p, cal::Kind::Global, Some(&keep), "2026-01-05 08:00").unwrap();
    cal::rename(&mut p, &used, "Night shift").unwrap();
    let k = p.model.calendars.iter().position(|c| c.id == used).unwrap();
    for i in [1, 2] {
        p.model.activities[i].cal = k;
    }
    schedule_model(&mut p.model, &p.resources).unwrap();
    (p, keep, used)
}

#[track_caller]
fn delete_calendar(d: &mut Driver, id: &str) {
    if d.app.calendars.is_none() {
        d.home();
        press(d, "menu_bar_enterprise");
        press(d, "menu_calendars");
        d.frames(2);
    }
    let row = cal::list(&d.app.project.document, cal::Kind::Global, 0)
        .iter()
        .position(|e| e.id == id)
        .expect("listed");
    let selected = d.app.calendars.as_ref().and_then(|s| s.selected.clone());
    if selected.as_deref() != Some(id) {
        press(d, &format!("calendar_list_row_{row}"));
    }
    press(d, "calendars_delete");
    d.frames(2);
}

#[test]
fn workflow_calendars_in_use_draws_help_cancel_ok_as_observed() {
    let (p, default, used) = calendar_sample();
    let mut d = Driver::new("wf-polish-in-use", p);
    let start = d.app.project.clone();
    let steps = d.app.history.counts().0;
    delete_calendar(&mut d, &used);
    assert!(d.has("calendars_in_use_ok"), "Calendars in Use opens");
    assert_eq!(
        left_to_right(
            &d,
            &[
                "calendars_in_use_ok",
                "calendars_in_use_cancel",
                "calendars_in_use_help",
            ]
        ),
        [
            "calendars_in_use_help",
            "calendars_in_use_cancel",
            "calendars_in_use_ok",
        ],
        "the bottom row reads Help, Cancel, OK"
    );
    d.assert_clean("Calendars in Use");
    // Cancel changes nothing.
    press(&mut d, "calendars_in_use_cancel");
    d.frames(1);
    assert_eq!(d.app.project.model, start.model, "Cancel changes nothing");
    // OK with the default global calendar deletes it, one undo step.
    delete_calendar(&mut d, &used);
    press(&mut d, "calendars_in_use_default");
    press(&mut d, "calendars_in_use_ok");
    d.frames(2);
    assert!(
        cal::row_of(&d.app.project.document, &used).is_none(),
        "deleted"
    );
    for i in [1, 2] {
        let a = &d.app.project.model.activities[i];
        assert_eq!(d.app.project.model.calendars[a.cal].id, default);
    }
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    press(&mut d, "calendars_close");
    undo(&mut d);
    assert!(
        cal::row_of(&d.app.project.document, &used).is_some(),
        "restored"
    );
    for i in [1, 2] {
        let a = &d.app.project.model.activities[i];
        assert_eq!(d.app.project.model.calendars[a.cal].id, used, "back on it");
    }
}

#[test]
fn workflow_notes_to_resources_add_is_always_enabled() {
    let mut d = Driver::new("wf-polish-feedback", scheduled());
    d.home();
    let i = index_of(&d.app.project, "RB-1020");
    let task_id = d.app.project.model.activities[i].task_id;
    let pid = d.app.project.model.project.proj_id;
    select_row(&mut d, i);
    d.frames(2);
    if !d.has("detail_tab_feedback") {
        d.app.show_detail_tab(DetailTab::Feedback);
        d.frames(2);
    }
    press(&mut d, "detail_tab_feedback");
    d.frames(2);
    assert_eq!(d.app.detail.tab, DetailTab::Feedback);
    let notes = |d: &Driver| {
        records::rows_for(&d.app.project.document, "TASKNOTE", task_id, pid)
            .first()
            .map(|&r| {
                records::memo_to_text(&records::get(
                    &d.app.project.document,
                    "TASKNOTE",
                    r,
                    "task_notes",
                ))
            })
            .unwrap_or_default()
    };
    let before = notes(&d);
    let steps = d.app.history.counts().0;
    // Nothing typed: Add answers by putting the cursor in the entry box.
    press(&mut d, "detail_feedback_add");
    d.frames(2);
    let entry = egui::Id::new(("note_draft", task_id));
    assert!(
        d.ctx.memory(|m| m.has_focus(entry)),
        "Add with nothing typed puts the cursor in the entry box"
    );
    assert_eq!(notes(&d), before, "and adds nothing");
    assert_eq!(d.app.history.counts().0, steps);
    // Typed there, Add appends the note: one undo step.
    d.type_text("Bring the torque wrench");
    d.frames(1);
    press(&mut d, "detail_feedback_add");
    d.frames(2);
    assert!(
        notes(&d).contains("Bring the torque wrench"),
        "{:?}",
        notes(&d)
    );
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    undo(&mut d);
    assert_eq!(notes(&d), before, "undo takes the note away");
}

/// View > Collapse To...
fn open_collapse_to(d: &mut Driver) {
    d.click("menu_bar_view");
    d.click("menu_collapse_to");
    d.frames(2);
}

#[test]
fn workflow_collapse_to_is_one_drop_down_with_apply() {
    let mut d = Driver::new("wf-polish-collapse", scheduled());
    d.home();
    let start = d.app.project.clone();
    let all = d.app.rows().len();
    open_collapse_to(&mut d);
    assert!(d.has("collapse_to_level"), "one drop-down of levels");
    assert!(d.has("collapse_to_apply"), "Apply beside OK and Cancel");
    assert!(shown(&mut d, "Band level to collapse to"));
    let draft = d.app.layout_opts.drafts.collapse.as_ref().unwrap();
    assert_eq!(draft.selected, None, "blank until a level is picked");
    // Level 1, Apply: the bands fold and the window stays open.
    press(&mut d, "collapse_to_level");
    press(&mut d, "collapse_to_level_item_0");
    press(&mut d, "collapse_to_apply");
    d.frames(2);
    assert!(d.app.layout_opts.drafts.collapse.is_some(), "still open");
    assert!(d.app.rows().len() < all, "folded to level 1");
    // OK closes; Cancel and Escape leave the bands as they are.
    press(&mut d, "collapse_to_ok");
    d.frames(2);
    assert!(d.app.layout_opts.drafts.collapse.is_none(), "OK closes");
    let folded = d.app.rows().len();
    open_collapse_to(&mut d);
    d.key(Key::Escape);
    d.frames(2);
    assert_eq!(d.app.rows().len(), folded, "Escape changes nothing");
    // A view change only: the project is untouched.
    assert_eq!(d.app.project.model, start.model);
    d.assert_clean("Collapse To");
}

/// The starter under a project node with three funding sources, two of
/// them assigned in the order Operating Grant, Capital.
fn funded() -> ProjectData {
    let mut p = crate::views::wbs::tests::with_project_node();
    for (id, name) in [
        ("7", "Capital"),
        ("9", "Operating Grant"),
        ("11", "Bond issue"),
    ] {
        crate::records::add_row(
            &mut p.document,
            funding::SOURCES,
            &[("fund_id", id.into()), ("fund_name", name.into())],
        );
    }
    funding::add_by_source(&mut p, "Operating Grant", Some("3000"), Some("75")).unwrap();
    funding::add_by_source(&mut p, "Capital", Some("1250"), Some("25")).unwrap();
    p
}

/// The texts drawn on row `i` of the Funding table.
fn funding_row(d: &mut Driver, i: usize) -> Vec<String> {
    d.frames(1);
    let row = d.rect(&format!("project_funding_row_{i}"));
    d.texts()
        .into_iter()
        .filter(|(_, r)| row.contains(r.center()))
        .map(|(t, _)| t)
        .collect()
}

#[test]
fn workflow_the_funding_tab_lists_by_source_and_moves_rows_on_commit() {
    let mut d = Driver::new("wf-polish-funding", funded());
    d.click("directory_projects");
    let p = &d.app.project;
    d.app.views.projects.selected = Some((p.source_name.clone(), p.model.project.proj_id));
    d.app.views.projects.show_details = true;
    d.app.views.projects.tab = TABS.iter().position(|t| *t == "Funding").unwrap();
    d.frames(3);
    let start = funding::list(&d.app.project);
    let steps = d.app.history.counts().0;
    // Stored Operating Grant first; listed by source, Capital first.
    assert!(funding_row(&mut d, 0).contains(&"Capital".to_string()));
    assert!(funding_row(&mut d, 1).contains(&"Operating Grant".to_string()));
    // A typed amount on the second row: the row stays where it is.
    press(&mut d, "project_funding_row_1");
    d.frames(2);
    assert_eq!(d.app.views.projects.funding_row, Some(1));
    press(&mut d, "project_funding_cell_1");
    d.key_with(Key::A, ctrl());
    d.type_text("5000");
    d.frames(1);
    assert!(funding_row(&mut d, 1).contains(&"Operating Grant".to_string()));
    d.key(Key::Enter);
    d.frames(2);
    assert_eq!(d.app.views.projects.funding_row, Some(1), "no jump");
    assert!(funding_row(&mut d, 1).contains(&"Operating Grant".to_string()));
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    // A new source is committed: the row moves to its place and stays
    // selected.
    press(&mut d, "project_funding_cell_0");
    d.frames(2);
    assert!(d.click_text("Bond issue"), "the source list opens");
    d.frames(2);
    assert_eq!(
        d.app.views.projects.funding_row,
        Some(0),
        "the selection follows"
    );
    assert!(funding_row(&mut d, 0).contains(&"Bond issue".to_string()));
    assert_eq!(d.app.history.counts().0, steps + 2, "one undo step");
    for _ in 0..2 {
        d.app.history_move(&d.ctx.clone(), false);
    }
    d.frames(2);
    assert_eq!(funding::list(&d.app.project), start, "undo restores both");
}

#[test]
fn workflow_resource_shifts_and_curves_carry_menu_icons() {
    use crate::enterprise::Dialog;
    for dialog in [Dialog::Shifts, Dialog::Curves] {
        assert!(dialog.icon().is_some(), "{} has an icon", dialog.title());
    }
}
