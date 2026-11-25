//! Workflows: copying a WBS element with its activities, and deleting a
//! calendar that is in use.
//!
//! WBS: Copy then Paste in the WBS view opens Copy WBS Options (Notebook,
//! WBS Milestones, Documents, Activities, all ticked at first); with
//! Activities, Copy Activity Options follows; the copied element, the
//! element below it, its milestones and its activities are pasted, each
//! activity under the copy of its own element, with new Activity IDs, and
//! one undo takes all of it away. Cancel pastes nothing. With the
//! reference defaults the Activities table's Renumber Activity IDs step
//! follows Copy Activity Options: its rule numbers the copies, Cancel or
//! Escape there pastes nothing, and "Skip this dialog next time" turns
//! the step off for later pastes.
//!
//! Calendars: Delete on a calendar nothing uses asks the plain question; on
//! one that activities use it opens Calendars in Use with no question
//! first. Select Replacement Calendar opens the picker of global calendars;
//! Use Default Global Calendar takes the default. Every user moves to the
//! replacement and the calendar goes, one undo step. The default global
//! calendar is refused.
use super::*;
use crate::calendars::ops as cal;
use openxer_xer::edit::records;

fn wbs_of(p: &ProjectData, code: &str) -> i64 {
    let task = p.model.activities[index_of(p, code)].task_id;
    p.details[&task].wbs
}

fn parent_of(p: &ProjectData, id: i64) -> Option<i64> {
    p.wbs.iter().find(|w| w.id == id).and_then(|w| w.parent)
}

fn undo(d: &mut Driver) {
    d.home();
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
}

/// Rows of `table` that name WBS element `wbs`.
fn rows_on(p: &ProjectData, table: &str, wbs: i64) -> usize {
    p.document.table(table).map_or(0, |t| {
        t.rows
            .iter()
            .filter(|r| t.value(r, "wbs_id").trim() == wbs.to_string())
            .count()
    })
}

/// The sample with a child element under Procurement holding RB-1040, a
/// milestone and a notebook topic on Procurement.
fn wbs_sample() -> (ProjectData, i64, i64, i64) {
    let mut p = openxer_xer::starter();
    let procurement = wbs_of(&p, "RB-1050");
    let construction = wbs_of(&p, "RB-1070");
    assert_eq!(wbs_of(&p, "RB-1040"), procurement);
    let child = openxer_xer::edit::wbs::add(&mut p, Some(procurement)).unwrap();
    openxer_xer::edit::wbs::shift_right(&mut p, child).unwrap();
    assert_eq!(parent_of(&p, child), Some(procurement));
    let task = p.model.activities[index_of(&p, "RB-1040")].task_id;
    p.details.get_mut(&task).unwrap().wbs = child;
    let pid = p.model.project.proj_id.to_string();
    records::add_row(
        &mut p.document,
        "WBSSTEP",
        &[
            ("wbs_step_id", "1".into()),
            ("proj_id", pid.clone()),
            ("wbs_id", procurement.to_string()),
            ("seq_num", "1".into()),
            ("step_name", "Orders placed".into()),
            ("step_wt", "1".into()),
            ("complete_flag", "N".into()),
        ],
    );
    let memo_type = match p.document.table("WBSMEMO") {
        Some(t) if !t.rows.is_empty() => t.value(&t.rows[0], "memo_type_id").to_string(),
        _ => "1".into(),
    };
    let next = records::next_id(&p.document, "WBSMEMO", "wbs_memo_id");
    let memo = p.document.table_mut("WBSMEMO");
    if memo.fields.is_empty() {
        memo.fields = "wbs_memo_id proj_id memo_type_id wbs_id wbs_memo"
            .split(' ')
            .map(String::from)
            .collect();
    }
    memo.add(&[
        ("wbs_memo_id", next.to_string()),
        ("proj_id", pid),
        ("memo_type_id", memo_type),
        ("wbs_id", procurement.to_string()),
        ("wbs_memo", "Buy early".into()),
    ]);
    schedule_model(&mut p.model, &p.resources).unwrap();
    (p, procurement, child, construction)
}

#[test]
fn workflow_copy_a_wbs_element_with_its_activities() {
    let (p, procurement, child, construction) = wbs_sample();
    let mut d = Driver::new("wf-wbs-copy", p);
    let start = d.app.project.clone();
    let count = start.model.activities.len();
    let undo_steps = d.app.history.counts().0;
    show(&mut d, "wbs");

    // Copy Procurement, select Construction, Paste: Copy WBS Options opens
    // with everything ticked. Cancel pastes nothing.
    wbs_click(&mut d, procurement);
    d.click("dock_copy");
    wbs_click(&mut d, construction);
    d.click("dock_paste");
    d.frames(2);
    assert!(
        d.has("copy_wbs_options_ok"),
        "Paste of a copied element opens Copy WBS Options: {}",
        d.app.status
    );
    let opened = d.app.views.wbs.paste.clone().expect("a paste waits");
    assert_eq!(
        opened.options,
        views::wbs_copy::WbsCopyOptions::default(),
        "everything is ticked at first"
    );
    assert!(opened.options.notebook && opened.options.activities);
    d.click("copy_wbs_options_cancel");
    d.frames(2);
    assert!(d.app.views.wbs.paste.is_none());
    assert_eq!(d.app.project.wbs, start.wbs, "Cancel pastes nothing");
    assert_eq!(d.app.history.counts().0, undo_steps);

    // Paste again, untick Notebook, OK: Copy Activity Options follows.
    d.click("dock_paste");
    d.frames(2);
    d.click("copy_wbs_options_notebook");
    d.click("copy_wbs_options_ok");
    d.frames(2);
    assert!(
        d.app.editing.dialogs.copy_options.is_some() && d.has("copy_options_ok"),
        "Activities ticked: Copy Activity Options asks next"
    );
    d.click("copy_options_ok");
    d.frames(2);
    d.assert_clean("paste the copy");

    // The copy of Procurement is the last child of Construction, with the
    // copy of the child element below it.
    let p = d.app.project.clone();
    let copy = d.app.views.wbs.selected.expect("the copy is selected");
    assert!(!start.wbs.iter().any(|w| w.id == copy), "a new element");
    assert_eq!(parent_of(&p, copy), Some(construction));
    let name = |p: &ProjectData, id: i64| p.wbs.iter().find(|w| w.id == id).unwrap().name.clone();
    assert_eq!(name(&p, copy), name(&start, procurement));
    let below: Vec<i64> = p
        .wbs
        .iter()
        .filter(|w| w.parent == Some(copy))
        .map(|w| w.id)
        .collect();
    assert_eq!(below.len(), 1, "the child element came along");
    let child_copy = below[0];
    assert_eq!(name(&p, child_copy), name(&start, child));
    // The milestone came, the unticked notebook topic did not.
    assert_eq!(rows_on(&p, "WBSSTEP", copy), 1, "milestone copied");
    assert_eq!(rows_on(&p, "WBSMEMO", copy), 0, "notebook not copied");
    assert_eq!(
        rows_on(&p, "WBSSTEP", procurement),
        1,
        "the original keeps its own"
    );

    // Two new activities: each a copy under the copy of its own element,
    // with an Activity ID of its own.
    assert_eq!(p.model.activities.len(), count + 2, "{}", d.app.status);
    let new: Vec<usize> = (count..p.model.activities.len()).collect();
    let by_name = |i: usize| p.name(i).to_string();
    let original = |code: &str| start.name(index_of(&start, code)).to_string();
    let i1050 = *new
        .iter()
        .find(|&&i| by_name(i) == original("RB-1050"))
        .expect("a copy of RB-1050");
    let i1040 = *new
        .iter()
        .find(|&&i| by_name(i) == original("RB-1040"))
        .expect("a copy of RB-1040");
    let wbs = |i: usize| p.details[&p.model.activities[i].task_id].wbs;
    assert_eq!(wbs(i1050), copy, "RB-1050's copy is under the copy");
    assert_eq!(wbs(i1040), child_copy, "RB-1040's copy: the child copy");
    for i in [i1040, i1050] {
        let code = &p.model.activities[i].task_code;
        assert!(
            start.model.activities.iter().all(|a| &a.task_code != code),
            "{code} is a new Activity ID"
        );
    }
    // The originals did not move.
    assert_eq!(wbs_of(&p, "RB-1050"), procurement);
    assert_eq!(wbs_of(&p, "RB-1040"), child);
    // One undo step for all of it.
    assert_eq!(d.app.history.counts().0, undo_steps + 1, "one undo step");
    assert!(d.app.status.contains("2 activities"), "{}", d.app.status);

    // It survives an XER round trip.
    let reread = xer_round_trip(&mut d, "wf-wbs-copy");
    let task = |p: &ProjectData, i: usize| p.model.activities[i].task_id;
    let code1050 = p.model.activities[i1050].task_code.clone();
    assert_eq!(
        reread.details[&task(&reread, index_of(&reread, &code1050))].wbs,
        copy
    );

    // One undo takes the elements, their milestone and the activities away.
    undo(&mut d);
    let back = &d.app.project;
    assert_eq!(back.wbs, start.wbs, "the elements are gone");
    assert_eq!(back.model.activities.len(), count, "activities gone");
    assert_eq!(rows_on(back, "WBSSTEP", copy), 0, "the milestone is gone");
    assert_eq!(back.model, start.model);
    assert_eq!(d.app.history.counts().0, undo_steps);
}

#[test]
fn workflow_copy_wbs_options_can_be_turned_off() {
    let (p, procurement, _, construction) = wbs_sample();
    let mut d = Driver::new("wf-wbs-copy-hide", p);
    let start = d.app.project.clone();
    let undo_steps = d.app.history.counts().0;
    show(&mut d, "wbs");

    // "Skip this dialog next time" is unticked at first; ticked with
    // OK, the next paste goes straight to Copy Activity Options.
    assert!(views::wbs_copy::WbsCopyOptions::default().show_dialog);
    wbs_click(&mut d, procurement);
    press(&mut d, "dock_copy");
    wbs_click(&mut d, construction);
    press(&mut d, "dock_paste");
    press(&mut d, "copy_wbs_options_hide");
    press(&mut d, "copy_wbs_options_ok");
    press(&mut d, "copy_options_cancel");
    d.frames(2);
    assert!(!d.app.views.wbs.copy_options.show_dialog, "turned off");
    assert_eq!(d.app.project.model, start.model, "Cancel pastes nothing");
    press(&mut d, "dock_paste");
    d.frames(2);
    assert!(!d.has("copy_wbs_options_ok"), "not shown again");
    press(&mut d, "copy_options_cancel");
    d.frames(2);
    assert_eq!(d.app.project.wbs, start.wbs, "nothing pasted");
    assert_eq!(d.app.history.counts().0, undo_steps);
}

/// Copy Procurement, select Construction, Paste, OK on Copy WBS Options and
/// on Copy Activity Options.
fn paste_through_options(d: &mut Driver, procurement: i64, construction: i64) {
    wbs_click(d, procurement);
    d.click("dock_copy");
    wbs_click(d, construction);
    d.click("dock_paste");
    d.frames(2);
    assert!(d.has("copy_wbs_options_ok"), "Copy WBS Options opens");
    d.click("copy_wbs_options_ok");
    d.frames(2);
    assert!(d.has("copy_options_ok"), "Copy Activity Options follows");
    d.click("copy_options_ok");
    d.frames(2);
}

#[test]
fn workflow_copy_a_wbs_element_renumbers_its_copies() {
    // The reference's out-of-the-box defaults: paste asks for new IDs.
    crate::defaults::use_reference();
    let (p, procurement, child, construction) = wbs_sample();
    let mut d = Driver::new("wf-wbs-copy-renumber", p);
    assert!(d.app.editing.options.renumber.on_paste);
    let start = d.app.project.clone();
    let count = start.model.activities.len();
    let undo_steps = d.app.history.counts().0;
    show(&mut d, "wbs");

    // After Copy Activity Options, the Activities table's Renumber
    // Activity IDs step asks for the copies' IDs; nothing is pasted yet.
    paste_through_options(&mut d, procurement, construction);
    assert!(
        d.has("renumber_ok"),
        "Renumber Activity IDs follows Copy Activity Options: {:?}",
        d.app.status
    );
    assert_eq!(d.app.project.wbs, start.wbs, "nothing pasted yet");
    assert_eq!(d.app.project.model.activities.len(), count);

    // Cancel there pastes nothing: no element, no activity, no undo step.
    d.click("renumber_cancel");
    d.frames(2);
    assert!(!d.has("renumber_ok"));
    assert!(d.app.views.wbs.paste.is_none(), "the paste is dropped");
    assert_eq!(d.app.project.wbs, start.wbs, "Cancel pastes nothing");
    assert_eq!(d.app.project.model, start.model);
    assert_eq!(d.app.history.counts().0, undo_steps);
    // Escape does the same.
    paste_through_options(&mut d, procurement, construction);
    assert!(d.has("renumber_ok"));
    d.key(Key::Escape);
    d.frames(2);
    assert!(!d.has("renumber_ok"));
    assert!(d.app.views.wbs.paste.is_none());
    assert_eq!(d.app.project.model, start.model, "Escape pastes nothing");

    // Auto-number CP-100 by 10: the copies, in table order, get CP-100 and
    // CP-110, each under the copy of its own element.
    paste_through_options(&mut d, procurement, construction);
    d.click("renumber_mode_auto");
    type_into(&mut d, "renumber_prefix", "CP-");
    type_into(&mut d, "renumber_suffix", "100");
    type_into(&mut d, "renumber_auto_step", "10");
    d.frames(1);
    d.click("renumber_ok");
    d.frames(2);
    d.assert_clean("paste with renumbered copies");
    assert!(!d.has("duplicates_ok"), "no ID in use");
    let p = d.app.project.clone();
    assert_eq!(p.model.activities.len(), count + 2, "{}", d.app.status);
    let copy = d.app.views.wbs.selected.expect("the copy is selected");
    assert_eq!(parent_of(&p, copy), Some(construction));
    let child_copy = p
        .wbs
        .iter()
        .find(|w| w.parent == Some(copy))
        .map(|w| w.id)
        .expect("the child element came along");
    // Table order of the copied activities in the project before the paste.
    let order: Vec<&str> = start
        .model
        .activities
        .iter()
        .map(|a| a.task_code.as_str())
        .filter(|c| ["RB-1040", "RB-1050"].contains(c))
        .collect();
    assert_eq!(order.len(), 2);
    for (code, want) in order.iter().zip(["CP-100", "CP-110"]) {
        let i = index_of(&p, want);
        assert_eq!(
            p.name(i),
            start.name(index_of(&start, code)),
            "{want} is the copy of {code}"
        );
        let under = p.details[&p.model.activities[i].task_id].wbs;
        let own = match *code {
            "RB-1040" => child_copy,
            _ => copy,
        };
        assert_eq!(under, own, "{want} is under the copy of its element");
    }
    let pasted = &p.model.activities[count..];
    let new_codes: Vec<&str> = pasted.iter().map(|a| a.task_code.as_str()).collect();
    assert_eq!(new_codes.len(), 2);
    assert!(
        new_codes.iter().all(|c| c.starts_with("CP-")),
        "{new_codes:?}"
    );
    // The originals kept their IDs and elements.
    assert_eq!(wbs_of(&p, "RB-1050"), procurement);
    assert_eq!(wbs_of(&p, "RB-1040"), child);
    assert!(d.app.views.wbs.paste.is_none());
    assert_eq!(d.app.history.counts().0, undo_steps + 1, "one undo step");

    // One undo takes the elements and the renumbered activities away.
    undo(&mut d);
    assert_eq!(d.app.project.wbs, start.wbs, "the elements are gone");
    assert_eq!(d.app.project.model, start.model, "the activities are gone");
    assert_eq!(d.app.history.counts().0, undo_steps);

    // "Skip this dialog next time" on that step: OK pastes, and the
    // next WBS paste gives the copies free IDs without asking.
    show(&mut d, "wbs");
    paste_through_options(&mut d, procurement, construction);
    d.click("renumber_hide");
    d.frames(1);
    d.click("renumber_ok");
    d.frames(2);
    assert!(!d.app.editing.options.renumber.on_paste, "turned off");
    assert_eq!(d.app.project.model.activities.len(), count + 2);
    undo(&mut d);
    show(&mut d, "wbs");
    paste_through_options(&mut d, procurement, construction);
    assert!(!d.has("renumber_ok"), "the step is not shown again");
    assert_eq!(d.app.project.model.activities.len(), count + 2);
    assert_eq!(d.app.history.counts().0, undo_steps + 1, "one undo step");
    undo(&mut d);
    assert_eq!(d.app.project.model, start.model);
}

/// A WBS paste follows the same Renumber Activity IDs rules as a paste in
/// the Activities table: Increment by 10 where the copied ID plus 10 is in
/// use takes the highest number in use plus 10 (free numbers in between are
/// not used), and Cancel on Duplicates asks whether to go back (Yes) or
/// paste nothing (No).
#[test]
fn workflow_wbs_paste_follows_the_paste_increment_and_duplicates_rules() {
    crate::defaults::use_reference();
    let (mut p, procurement, _, construction) = wbs_sample();
    // A gap in the numbering: RB-1130 becomes RB-1500, so RB-1130 is free
    // but is not the highest number in use plus 10.
    let last = index_of(&p, "RB-1130");
    p.model.activities[last].task_code = "RB-1500".into();
    schedule_model(&mut p.model, &p.resources).unwrap();
    let mut d = Driver::new("wf-wbs-copy-rules", p);
    let start = d.app.project.clone();
    let count = start.model.activities.len();
    let undo_steps = d.app.history.counts().0;
    let codes = |p: &ProjectData| -> Vec<String> {
        p.model
            .activities
            .iter()
            .map(|a| a.task_code.clone())
            .collect()
    };
    let taken = codes(&start);
    for code in ["RB-1050", "RB-1060"] {
        assert!(taken.contains(&code.to_string()), "{code} is in use");
    }
    assert!(!taken.contains(&"RB-1130".to_string()), "RB-1130 is free");
    show(&mut d, "wbs");

    // Increment by 10, the default choice: no Duplicates, the copies take
    // RB-1510 and RB-1520.
    paste_through_options(&mut d, procurement, construction);
    assert!(d.has("renumber_ok"), "Renumber Activity IDs is shown");
    let shown = d.app.editing.dialogs.renumber.as_ref().unwrap();
    assert_eq!(shown.settings.mode, 0, "Increment is the default choice");
    assert_eq!(shown.settings.step, "10");
    d.click("renumber_ok");
    d.frames(2);
    d.assert_clean("WBS paste by increment");
    assert!(!d.has("duplicates_ok"), "no clash dialog");
    let p = d.app.project.clone();
    assert_eq!(p.model.activities.len(), count + 2, "{}", d.app.status);
    let mut new_codes = codes(&p).split_off(count);
    new_codes.sort();
    assert_eq!(new_codes, ["RB-1510", "RB-1520"], "{:?}", d.app.status);
    assert_eq!(d.app.history.counts().0, undo_steps + 1, "one undo step");
    undo(&mut d);
    assert_eq!(d.app.project.model, start.model, "undone");

    // Replace the first 3 characters with RB-: the copies keep IDs in use
    // and Duplicates opens. Cancel asks; Yes goes back to the list.
    show(&mut d, "wbs");
    paste_through_options(&mut d, procurement, construction);
    d.click("renumber_mode_replace");
    type_into(&mut d, "renumber_count", "3");
    type_into(&mut d, "renumber_with", "RB-");
    d.click("renumber_ok");
    d.frames(2);
    assert!(d.has("duplicates_ok"), "the clash opens Duplicates");
    assert!(d.app.views.wbs.paste.is_some(), "the WBS paste waits");
    d.click("duplicates_cancel");
    assert!(
        d.has("resolve_duplicates_yes"),
        "Cancel on Duplicates asks whether to resolve them"
    );
    d.click("resolve_duplicates_yes");
    d.frames(2);
    assert!(d.has("duplicates_ok"), "Yes goes back to Duplicates");
    assert!(d.app.views.wbs.paste.is_some(), "the WBS paste still waits");
    // Cancel and No: nothing is pasted and the WBS paste is dropped.
    d.click("duplicates_cancel");
    d.click("resolve_duplicates_no");
    d.frames(2);
    assert!(!d.has("duplicates_ok"), "No closes Duplicates");
    assert!(d.app.views.wbs.paste.is_none(), "the WBS paste is dropped");
    assert_eq!(d.app.project.wbs, start.wbs, "No pastes no element");
    assert_eq!(codes(&d.app.project), taken, "No pastes no activity");
    assert_eq!(d.app.history.counts().0, undo_steps, "no undo step");
}

/// Click `action`, failing here (with the caller's line) when it is not
/// drawn.
#[track_caller]
fn press(d: &mut Driver, action: &str) {
    d.frames(2);
    assert!(d.has(action), "{action} is not drawn");
    d.click(action);
}

/// The sample with its calendar marked the default global calendar, a
/// second global calendar two activities use, and a third nothing uses:
/// (default, used, unused).
fn calendar_sample() -> (ProjectData, String, String, String) {
    let mut p = openxer_xer::starter();
    let keep = cal::list(&p.document, cal::Kind::Global, 0)[0].id.clone();
    cal::set_default(&mut p, &keep).unwrap();
    let stamp = "2026-01-05 08:00";
    let used = cal::add(&mut p, cal::Kind::Global, Some(&keep), stamp).unwrap();
    cal::rename(&mut p, &used, "Night shift").unwrap();
    let other = cal::add(&mut p, cal::Kind::Global, Some(&keep), stamp).unwrap();
    cal::rename(&mut p, &other, "Weekend crew").unwrap();
    let k = p.model.calendars.iter().position(|c| c.id == used).unwrap();
    for i in [1, 2] {
        p.model.activities[i].cal = k;
    }
    schedule_model(&mut p.model, &p.resources).unwrap();
    (p, keep, used, other)
}

#[track_caller]
fn open_calendars(d: &mut Driver) {
    d.home();
    press(d, "menu_bar_enterprise");
    press(d, "menu_calendars");
    d.frames(2);
    assert!(d.app.calendars.is_some(), "Calendars opens");
}

/// Select calendar `id` in the Global list and click Delete.
#[track_caller]
fn delete_calendar(d: &mut Driver, id: &str) {
    let row = cal::list(&d.app.project.document, cal::Kind::Global, 0)
        .iter()
        .position(|e| e.id == id)
        .expect("listed");
    // A click on the selected row would start renaming it.
    let selected = d.app.calendars.as_ref().and_then(|s| s.selected.clone());
    if selected.as_deref() != Some(id) {
        press(d, &format!("calendar_list_row_{row}"));
    }
    press(d, "calendars_delete");
    d.frames(2);
}

/// Activities using calendar `id`, by Activity ID.
fn users(p: &ProjectData, id: &str) -> Vec<String> {
    p.model
        .activities
        .iter()
        .filter(|a| p.model.calendars[a.cal].id == id)
        .map(|a| a.task_code.clone())
        .collect()
}

fn resources_on(p: &ProjectData, id: &str) -> usize {
    p.document.table("RSRC").map_or(0, |t| {
        t.rows
            .iter()
            .filter(|r| t.value(r, "clndr_id") == id)
            .count()
    })
}

#[test]
fn workflow_delete_a_calendar_in_use_with_a_replacement() {
    let (p, default, gone, other) = calendar_sample();
    let mut d = Driver::new("wf-calendar-in-use", p);
    let start = d.app.project.clone();
    let moving = users(&start, &gone);
    assert!(!moving.is_empty());
    let steps = d.app.history.counts().0;
    open_calendars(&mut d);

    // A calendar activities use: Calendars in Use, no question first, with
    // Select Replacement Calendar chosen. Cancel changes nothing.
    delete_calendar(&mut d, &gone);
    let st = d.app.calendars.as_ref().unwrap();
    assert!(st.confirm_delete.is_none(), "no question first");
    assert_eq!(st.in_use, Some((gone.clone(), false)), "Calendars in Use");
    assert!(d.has("calendars_in_use_ok"));
    press(&mut d, "calendars_in_use_cancel");
    d.frames(1);
    assert!(d.app.calendars.as_ref().unwrap().in_use.is_none());
    assert_eq!(d.app.project.model, start.model, "Cancel changes nothing");
    assert_eq!(d.app.history.counts().0, steps);

    // The default global calendar is refused, with nothing changed.
    delete_calendar(&mut d, &default);
    let st = d.app.calendars.as_ref().unwrap();
    assert!(st.in_use.is_none() && st.confirm_delete.is_none());
    let notice = st.notice.clone().expect("the default is refused");
    assert!(notice.contains("default global calendar"), "{notice}");
    press(&mut d, "calendars_notice_ok");
    d.frames(1);
    assert_eq!(d.app.project.model, start.model, "nothing changed");

    // Use Default Global Calendar: everything that used it moves to the
    // default and the calendar goes.
    delete_calendar(&mut d, &gone);
    press(&mut d, "calendars_in_use_default");
    press(&mut d, "calendars_in_use_ok");
    d.frames(2);
    let p = d.app.project.clone();
    assert!(cal::row_of(&p.document, &gone).is_none(), "deleted");
    assert!(users(&p, &default)
        .iter()
        .all(|c| users(&start, &default).contains(c) || moving.contains(c)));
    for code in &moving {
        let a = &p.model.activities[index_of(&p, code)];
        assert_eq!(p.model.calendars[a.cal].id, default, "{code} moved");
    }
    assert_eq!(resources_on(&p, &gone), 0, "no resource keeps it");
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    assert!(d.app.calendars.as_ref().unwrap().notice.is_none());

    // One undo brings the calendar back and every activity onto it.
    press(&mut d, "calendars_close");
    undo(&mut d);
    let back = d.app.project.clone();
    assert!(cal::row_of(&back.document, &gone).is_some(), "restored");
    assert_eq!(users(&back, &gone), moving, "its activities are back on it");
    assert_eq!(resources_on(&back, &gone), resources_on(&start, &gone));
    assert_eq!(d.app.history.counts().0, steps);

    // Select Replacement Calendar: the picker lists the global calendars
    // but the one going; Select moves the users to the one picked.
    open_calendars(&mut d);
    delete_calendar(&mut d, &gone);
    press(&mut d, "calendars_in_use_ok");
    d.frames(2);
    let st = d.app.calendars.as_ref().unwrap();
    assert!(st.in_use.is_none());
    assert_eq!(
        st.replace,
        Some((gone.clone(), Some(default.clone()))),
        "the picker opens on the default global calendar"
    );
    let choices = cal::replacements(&d.app.project.document, &gone);
    assert!(choices.iter().all(|e| e.id != gone));
    let row = choices.iter().position(|e| e.id == other).unwrap();
    press(&mut d, &format!("calendar_replacement_row_{row}"));
    press(&mut d, "calendars_replacement_select");
    d.frames(2);
    let p = d.app.project.clone();
    assert!(cal::row_of(&p.document, &gone).is_none(), "deleted");
    for code in &moving {
        let a = &p.model.activities[index_of(&p, code)];
        assert_eq!(p.model.calendars[a.cal].id, other, "{code} moved");
    }
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    press(&mut d, "calendars_close");
    undo(&mut d);
    assert_eq!(users(&d.app.project, &gone), moving);

    // A calendar nothing uses: the plain question, and Yes deletes it.
    open_calendars(&mut d);
    press(&mut d, "calendars_add");
    press(&mut d, "calendars_copy_select");
    d.frames(2);
    let added = d
        .app
        .calendars
        .as_ref()
        .and_then(|s| s.selected.clone())
        .expect("the new calendar is selected");
    assert!(cal::delete_check(&d.app.project, &added) == Ok(false));
    delete_calendar(&mut d, &added);
    let st = d.app.calendars.as_ref().unwrap();
    assert!(st.in_use.is_none(), "not in use: no Calendars in Use");
    assert_eq!(st.confirm_delete, Some(added.clone()), "the plain question");
    press(&mut d, "calendars_delete_yes");
    d.frames(2);
    assert!(cal::row_of(&d.app.project.document, &added).is_none());
}
