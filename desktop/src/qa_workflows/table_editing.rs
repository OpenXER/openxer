//! Workflow: the table editing commands.
//! - Paste of copied activities asks for their new Activity IDs after Copy
//!   Activity Options (Renumber Activity IDs); IDs already in use go to
//!   Duplicates, where free ones are typed; Cancel pastes nothing.
//! - Fill Down copies from the row the selection was started on, also to a
//!   row above it.
//! - The Assign dialogs take several choices at once (Ctrl+click,
//!   Shift+click), and Assign Activity Codes makes a new code value with
//!   New... without assigning it.
//! - Fill Down works in the Resources table, Find and Replace in the WBS and
//!   Resources tables, through the shared table.
//!
//! Each change is checked, then one Undo puts the project back.
use super::*;

/// Choose an Edit menu command.
fn edit_menu(d: &mut Driver, item: &str) {
    d.home();
    d.click("menu_bar_edit");
    d.click(item);
    d.frames(2);
}

/// Choose an Edit menu command in the view shown (no return to the
/// Activities window first).
fn edit_menu_here(d: &mut Driver, item: &str) {
    d.click("menu_bar_edit");
    d.click(item);
    d.frames(2);
}

/// Choose Edit > Assign > `item`.
fn assign_menu(d: &mut Driver, item: &str) {
    d.home();
    d.click("menu_bar_edit");
    assert!(d.hover_text("Assign"), "Edit > Assign");
    d.frames(1);
    d.click(item);
    d.frames(2);
}

fn shift() -> egui::Modifiers {
    egui::Modifiers::SHIFT
}

/// The center of the last drawn `text`: a dialog's list draws over the
/// table behind it.
fn last_text(d: &mut Driver, text: &str) -> Pos2 {
    d.texts()
        .iter()
        .rev()
        .find(|(t, _)| t == text)
        .map(|(_, r)| r.center())
        .unwrap_or_else(|| panic!("{text:?} is drawn"))
}

/// Click the last drawn `text` with `modifiers` held.
fn click_text_with(d: &mut Driver, text: &str, modifiers: egui::Modifiers) {
    let at = last_text(d, text);
    d.press_at(at, egui::PointerButton::Primary, modifiers);
    d.frames(1);
}

fn codes(p: &ProjectData) -> Vec<String> {
    p.model
        .activities
        .iter()
        .map(|a| a.task_code.clone())
        .collect()
}

#[test]
fn workflow_paste_asks_for_new_ids_and_settles_duplicates() {
    // The reference's out-of-the-box defaults: the step is on.
    crate::defaults::use_reference();
    let mut d = fresh("wf-te-paste");
    let start = d.app.project.clone();
    let count = start.model.activities.len();
    let undo = d.app.history.counts().0;
    let i = index_of(&start, "RB-1010");
    select_row(&mut d, i);
    edit_menu(&mut d, "edit_copy");
    select_row(&mut d, index_of(&start, "RB-1020"));

    // Paste: Copy Activity Options, then Renumber Activity IDs.
    edit_menu(&mut d, "edit_paste");
    d.click("copy_options_ok");
    d.frames(2);
    assert!(
        d.has("renumber_ok"),
        "Renumber Activity IDs follows Copy Activity Options; status {:?}",
        d.app.status
    );
    assert_eq!(d.app.project.model.activities.len(), count, "nothing yet");

    // Replace beginning characters giving an ID in use: Duplicates opens.
    d.click("renumber_mode_replace");
    type_into(&mut d, "renumber_count", "3");
    type_into(&mut d, "renumber_with", "RB-");
    d.click("renumber_ok");
    d.frames(2);
    assert!(d.has("duplicates_ok"), "the clash opens Duplicates");
    assert_eq!(d.app.project.model.activities.len(), count, "nothing yet");
    // Cancel asks whether to go back and resolve them; Yes (the observed
    // default, Enter) goes back to the list.
    d.click("duplicates_cancel");
    assert!(
        d.has("resolve_duplicates_yes"),
        "Cancel on Duplicates asks whether to resolve them"
    );
    assert_eq!(
        crate::dialog_keys::top_default_label(&d.ctx).as_deref(),
        Some("Yes"),
        "Yes is the default, as observed"
    );
    d.key(Key::Enter);
    d.frames(2);
    assert!(!d.has("resolve_duplicates_yes"), "Enter answers Yes");
    assert!(d.has("duplicates_ok"), "Yes goes back to Duplicates");
    // OK waits for a free ID; one is typed.
    d.click("duplicates_ok");
    assert!(
        d.has("duplicates_ok"),
        "OK is unavailable while the ID is taken"
    );
    assert_eq!(d.app.project.model.activities.len(), count);
    type_into(&mut d, "duplicates_new_0", "RB-9010");
    d.frames(1);
    d.click("duplicates_ok");
    d.frames(2);
    d.assert_clean("paste with a typed ID");
    let p = &d.app.project;
    assert_eq!(p.model.activities.len(), count + 1, "one copy pasted");
    let copy = index_of(p, "RB-9010");
    assert_eq!(p.name(copy), start.name(i), "the copy of RB-1010");
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(codes(&d.app.project), codes(&start), "undone");

    // Auto-number passes over IDs in use: prefix RB- from 1010 by 10.
    select_row(&mut d, index_of(&start, "RB-1020"));
    edit_menu(&mut d, "edit_paste");
    d.click("copy_options_ok");
    d.frames(2);
    d.click("renumber_mode_auto");
    type_into(&mut d, "renumber_prefix", "RB-");
    type_into(&mut d, "renumber_suffix", "1010");
    type_into(&mut d, "renumber_auto_step", "10");
    let taken = codes(&start);
    let want = (0..)
        .map(|k| format!("RB-{}", 1010 + 10 * k))
        .find(|c| !taken.contains(c))
        .unwrap();
    d.click("renumber_ok");
    d.frames(2);
    assert!(!d.has("duplicates_ok"), "no clash left to settle");
    assert!(
        codes(&d.app.project).contains(&want),
        "auto-numbered {want}: {:?}",
        d.app.status
    );
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(codes(&d.app.project), codes(&start));

    // Replace beginning characters again (the dialog remembers it), Cancel
    // on Duplicates and No: nothing is pasted.
    edit_menu(&mut d, "edit_paste");
    d.click("copy_options_ok");
    d.frames(2);
    d.click("renumber_mode_replace");
    type_into(&mut d, "renumber_count", "3");
    type_into(&mut d, "renumber_with", "RB-");
    d.click("renumber_ok");
    d.frames(2);
    assert!(d.has("duplicates_ok"), "the clash opens Duplicates");
    d.click("duplicates_cancel");
    d.click("resolve_duplicates_no");
    d.frames(2);
    assert!(!d.has("duplicates_ok"), "No closes Duplicates");
    assert_eq!(codes(&d.app.project), codes(&start), "No pastes nothing");
    assert_eq!(d.app.history.counts().0, undo, "no undo step");

    // Cancel stops the paste.
    edit_menu(&mut d, "edit_paste");
    d.click("copy_options_ok");
    d.frames(2);
    d.click("renumber_cancel");
    d.frames(2);
    assert_eq!(
        codes(&d.app.project),
        codes(&start),
        "Cancel pastes nothing"
    );
    assert_eq!(d.app.history.counts().0, undo);
}

/// The default choice, Increment by 10, where the copied ID plus 10 is in
/// use: the paste completes at once with the highest number in use under
/// the same prefix plus 10, as observed in the reference. No Duplicates.
#[test]
fn workflow_paste_increment_clash_takes_the_highest_number_plus_the_increment() {
    crate::defaults::use_reference();
    // A gap in the numbering: the last activity is renumbered RB-1500, so
    // the free numbers after RB-1010 are not the highest plus 10.
    let mut p = openxer_xer::starter();
    let last = (0..p.model.activities.len())
        .max_by_key(|&i| p.model.activities[i].task_code.clone())
        .unwrap();
    p.model.activities[last].task_code = "RB-1500".into();
    schedule_model(&mut p.model, &p.resources).unwrap();
    let mut d = Driver::new("wf-te-paste-increment", p);
    let start = d.app.project.clone();
    let count = start.model.activities.len();
    let undo = d.app.history.counts().0;
    let taken = codes(&start);
    assert!(
        taken.contains(&"RB-1020".to_string()),
        "RB-1010 plus 10 is in use"
    );
    let highest = taken
        .iter()
        .filter_map(|c| c.strip_prefix("RB-").and_then(|n| n.parse::<i64>().ok()))
        .max()
        .unwrap();
    let want = format!("RB-{}", highest + 10);
    select_row(&mut d, index_of(&start, "RB-1010"));
    edit_menu(&mut d, "edit_copy");
    edit_menu(&mut d, "edit_paste");
    d.click("copy_options_ok");
    d.frames(2);
    assert!(d.has("renumber_ok"), "Renumber Activity IDs is shown");
    let shown = d.app.editing.dialogs.renumber.as_ref().unwrap();
    assert_eq!(shown.settings.mode, 0, "Increment is the default choice");
    assert_eq!(shown.settings.step, "10");
    d.click("renumber_ok");
    d.frames(2);
    d.assert_clean("increment paste");
    assert!(!d.has("duplicates_ok"), "no clash dialog");
    let p = &d.app.project;
    assert_eq!(p.model.activities.len(), count + 1, "pasted at once");
    let copy = index_of(p, &want);
    assert_eq!(p.name(copy), start.name(index_of(&start, "RB-1010")));
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(codes(&d.app.project), codes(&start), "undone");
}

#[test]
fn workflow_fill_down_copies_from_the_first_selected_row_upward() {
    let mut d = fresh("wf-te-fill");
    d.app.grid.layout.columns = vec![
        crate::grid::layout::ColumnSetting::new("activity_id", 90.),
        crate::grid::layout::ColumnSetting::new("activity_name", 200.),
        crate::grid::layout::ColumnSetting::new("original_duration", 90.),
    ];
    d.app.drop_frame_cache();
    d.frames(2);
    let start = d.app.project.clone();
    let duration = |p: &ProjectData, code: &str| {
        let task = p.model.activities[index_of(p, code)].task_id;
        p.details[&task].original_duration
    };
    let (low, high) = ("RB-1040", "RB-1010");
    assert_ne!(duration(&start, low), duration(&start, high));
    let undo = d.app.history.counts().0;
    // Click the lower row first, then Ctrl+click the one above it.
    let first = index_of(&start, low);
    let pos = row_cell(&mut d, first, "original_duration");
    d.click_at(pos);
    if d.app.edit_cell.is_some() {
        d.key(Key::Escape);
    }
    let pos = row_cell(&mut d, index_of(&start, high), "original_duration");
    d.press_at(pos, egui::PointerButton::Primary, ctrl());
    d.frames(1);
    assert_eq!(d.app.selected_in_order().len(), 2, "two rows selected");
    d.key_with(Key::E, ctrl());
    d.frames(2);
    let p = &d.app.project;
    assert_eq!(
        duration(p, high),
        duration(&start, low),
        "the upper row takes the first selected row's value: {}",
        d.app.status
    );
    assert_eq!(duration(p, low), duration(&start, low), "the source stays");
    for between in ["RB-1020", "RB-1030"] {
        assert_eq!(duration(p, between), duration(&start, between));
    }
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(duration(&d.app.project, high), duration(&start, high));
}

/// The sample project with resources (PIPE, its child PIPE.CRANE, IDLE)
/// and a global activity code Phase with the values PL and DS.
fn assign_project() -> ProjectData {
    let mut p = views::resources::tests::with_resource();
    let doc = &mut p.document;
    doc.table_mut("ACTVTYPE").add(&[
        ("actv_code_type_id", "50".into()),
        ("actv_short_len", "20".into()),
        ("seq_num", "1".into()),
        ("actv_code_type", "Phase".into()),
        ("actv_code_type_scope", "AS_Global".into()),
    ]);
    for (id, code, name, seq) in [("501", "PL", "Plan", "1"), ("502", "DS", "Design", "2")] {
        doc.table_mut("ACTVCODE").add(&[
            ("actv_code_id", id.into()),
            ("actv_code_type_id", "50".into()),
            ("seq_num", seq.into()),
            ("short_name", code.into()),
            ("actv_code_name", name.into()),
        ]);
    }
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

/// The current choice of the open Assign Resources window.
fn resource_choice(d: &Driver) -> Option<i64> {
    use crate::details::assignments::AssignPicker;
    match &d.app.detail.assign {
        Some(AssignPicker::Resource { selected, .. }) => *selected,
        _ => None,
    }
}

/// Resource ids assigned to activity `code`, sorted.
fn assigned(p: &ProjectData, code: &str) -> Vec<i64> {
    let task = p.model.activities[index_of(p, code)].task_id;
    let mut ids: Vec<i64> = p
        .assignment_rows(task)
        .into_iter()
        .filter_map(|row| p.assignment_view(row).and_then(|v| v.rsrc_id))
        .collect();
    ids.sort();
    ids
}

/// Code values (short names) of type 50 assigned to activity `code`.
fn phase_of(p: &ProjectData, code: &str) -> Vec<String> {
    let task = p.model.activities[index_of(p, code)].task_id.to_string();
    let values: BTreeMap<String, String> = p
        .document
        .table("ACTVCODE")
        .map(|t| {
            t.rows
                .iter()
                .map(|r| {
                    (
                        t.value(r, "actv_code_id").to_string(),
                        t.value(r, "short_name").to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    p.document.table("TASKACTV").map_or(vec![], |t| {
        t.rows
            .iter()
            .filter(|r| t.value(r, "task_id") == task)
            .filter(|r| t.value(r, "actv_code_type_id") == "50")
            .filter_map(|r| values.get(t.value(r, "actv_code_id")).cloned())
            .collect()
    })
}

#[test]
fn workflow_assign_dialogs_take_several_choices_and_make_code_values() {
    let mut d = Driver::new("wf-te-assign", assign_project());
    let start = d.app.project.clone();
    let undo = d.app.history.counts().0;

    // Assign Resources: click PIPE, Ctrl+click IDLE, Assign: both at once.
    select_row(&mut d, index_of(&start, "RB-1020"));
    assert!(assigned(&start, "RB-1020").is_empty());
    assign_menu(&mut d, "menu_assign_resources");
    assert!(d.app.detail.assign.is_some(), "Assign Resources is open");
    click_text_with(&mut d, "PIPE", Default::default());
    click_text_with(&mut d, "IDLE", ctrl());
    d.click("rsrc_picker_assign");
    d.frames(2);
    assert_eq!(
        assigned(&d.app.project, "RB-1020"),
        vec![7, 9],
        "two resources assigned by one Assign: {}",
        d.app.status
    );
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");
    d.click("rsrc_picker_close");
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert!(assigned(&d.app.project, "RB-1020").is_empty(), "undone");

    // Shift+click selects the run between.
    select_row(&mut d, index_of(&start, "RB-1020"));
    let before = assigned(&start, "RB-1020");
    assign_menu(&mut d, "menu_assign_resources");
    click_text_with(&mut d, "PIPE", Default::default());
    click_text_with(&mut d, "IDLE", shift());
    let rows = &d.app.detail.assign_rows["assign_resources"];
    let current = resource_choice(&d);
    assert_eq!(current, Some(9), "Shift+click makes IDLE current");
    assert_eq!(rows.picked(current.as_ref()), [7, 8, 9], "PIPE to IDLE");
    d.click("rsrc_picker_assign");
    d.frames(2);
    let now = assigned(&d.app.project, "RB-1020");
    assert!(now.contains(&7), "PIPE assigned");
    assert!(now.contains(&9), "IDLE assigned");
    assert!(now.contains(&8), "PIPE.CRANE assigned");
    assert_eq!(now.len(), before.len() + 3, "three added");
    d.click("rsrc_picker_close");
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(assigned(&d.app.project, "RB-1020"), before);

    // Assign Activity Codes, New...: Add Code Value makes the value, selects
    // it and does not assign it. New waits for a selected value, as
    // observed.
    select_row(&mut d, index_of(&start, "RB-1020"));
    let steps = d.app.history.counts().0;
    assign_menu(&mut d, "menu_assign_codes");
    assert!(
        d.app.detail.picker.is_some(),
        "Assign Activity Codes is open"
    );
    click_text_with(&mut d, "PL", Default::default());
    d.click("picker_code_new");
    d.frames(2);
    type_into(&mut d, "new_code_value", "ZP");
    type_into(&mut d, "new_code_description", "Probe value");
    d.click("new_code_ok");
    d.frames(2);
    let made = d.app.project.document.table("ACTVCODE").is_some_and(|t| {
        t.rows.iter().any(|r| {
            t.value(r, "short_name") == "ZP"
                && t.value(r, "actv_code_name") == "Probe value"
                && t.value(r, "actv_code_type_id") == "50"
        })
    });
    assert!(made, "the new value is in the dictionary: {}", d.app.status);
    assert!(
        phase_of(&d.app.project, "RB-1020").is_empty(),
        "not assigned"
    );
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");

    // Two values of one code: Assign gives the activity the first.
    click_text_with(&mut d, "PL", Default::default());
    click_text_with(&mut d, "DS", ctrl());
    d.click("picker_assign");
    d.frames(2);
    assert_eq!(phase_of(&d.app.project, "RB-1020"), vec!["PL".to_string()]);
    d.click("picker_close");
    d.frames(1);

    // Assign Predecessors: two activities, one Assign.
    let target = start
        .model
        .activities
        .iter()
        .map(|a| a.task_code.clone())
        .max()
        .unwrap();
    let t = index_of(&start, &target);
    let preds = |p: &ProjectData| -> Vec<String> {
        let mut v: Vec<String> = p
            .model
            .links
            .iter()
            .filter(|l| l.succ == t)
            .map(|l| p.model.activities[l.pred].task_code.clone())
            .collect();
        v.sort();
        v
    };
    let before = preds(&d.app.project);
    assert!(!before.iter().any(|c| c == "RB-1010" || c == "RB-1020"));
    let links_step = d.app.history.counts().0;
    select_row(&mut d, t);
    assign_menu(&mut d, "menu_assign_predecessors");
    click_text_with(&mut d, "RB-1010", Default::default());
    click_text_with(&mut d, "RB-1020", ctrl());
    d.click("picker_assign");
    d.frames(2);
    let after = preds(&d.app.project);
    assert!(
        after.contains(&"RB-1010".to_string()) && after.contains(&"RB-1020".to_string()),
        "both predecessors added: {after:?}, {}",
        d.app.status
    );
    assert_eq!(d.app.history.counts().0, links_step + 1, "one undo step");
    d.click("picker_close");
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(preds(&d.app.project), before, "undone");
}

/// The enabled buttons (but Help and the like) of the dialog on top.
fn live(d: &mut Driver) -> Vec<String> {
    d.frames(2);
    crate::dialog_keys::top_live_buttons(&d.ctx).0
}

/// Whether the buttons `keys` stand in one column from top to bottom.
fn in_column(d: &mut Driver, keys: &[&str]) -> bool {
    d.frames(2);
    let rects: Vec<egui::Rect> = keys.iter().map(|k| d.rect(k)).collect();
    rects
        .windows(2)
        .all(|w| (w[0].left() - w[1].left()).abs() < 0.5 && w[0].top() < w[1].top())
}

/// The Assign dialogs as observed in the reference: Assign Resources has
/// Close, Assign, Remove, Replace, Help; Assign Activity Codes has Close,
/// Assign, Remove, New, Help. A resource the activity already has turns
/// Assign off and Remove and Replace on; Replace puts the next resource
/// chosen with Assign in its place; Remove asks the Resources tab's
/// question first. Each change is one undo step.
#[test]
fn workflow_assign_dialogs_follow_the_observed_buttons() {
    let mut d = Driver::new("wf-te-assign-buttons", assign_project());
    let start = d.app.project.clone();
    let undo = d.app.history.counts().0;
    select_row(&mut d, index_of(&start, "RB-1020"));
    assign_menu(&mut d, "menu_assign_resources");
    assert!(
        in_column(
            &mut d,
            &[
                "rsrc_picker_close",
                "rsrc_picker_assign",
                "rsrc_picker_remove",
                "rsrc_picker_replace",
                "rsrc_picker_help",
            ],
        ),
        "Close, Assign, Remove, Replace, Help"
    );
    click_text_with(&mut d, "PIPE", Default::default());
    d.click("rsrc_picker_assign");
    d.frames(2);
    assert_eq!(assigned(&d.app.project, "RB-1020"), vec![7]);
    assert_eq!(d.app.history.counts().0, undo + 1);
    // PIPE is the activity's resource now: Assign off, Remove and Replace on.
    let buttons = live(&mut d);
    assert!(
        !buttons.contains(&"Assign".to_string()),
        "Assign is off for a resource the activity has: {buttons:?}"
    );
    for b in ["Remove", "Replace"] {
        assert!(buttons.contains(&b.to_string()), "{b} is on: {buttons:?}");
    }
    // Another resource: Assign on, Remove and Replace off.
    click_text_with(&mut d, "IDLE", Default::default());
    let buttons = live(&mut d);
    assert!(buttons.contains(&"Assign".to_string()), "{buttons:?}");
    for b in ["Remove", "Replace"] {
        assert!(!buttons.contains(&b.to_string()), "{b} is off: {buttons:?}");
    }
    // Replace: PIPE, Replace, then IDLE and Assign puts IDLE in its place.
    click_text_with(&mut d, "PIPE", Default::default());
    d.click("rsrc_picker_replace");
    click_text_with(&mut d, "IDLE", Default::default());
    d.click("rsrc_picker_assign");
    d.frames(2);
    if d.has("assignment_question_ok") {
        d.click("assignment_question_ok");
        d.frames(2);
    }
    assert_eq!(
        assigned(&d.app.project, "RB-1020"),
        vec![9],
        "IDLE replaces PIPE: {}",
        d.app.status
    );
    assert_eq!(d.app.history.counts().0, undo + 2, "one undo step");
    d.click("rsrc_picker_close");
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(assigned(&d.app.project, "RB-1020"), vec![7], "undone");

    // Remove: the Resources tab's question (No is the default), Yes removes.
    select_row(&mut d, index_of(&start, "RB-1020"));
    assign_menu(&mut d, "menu_assign_resources");
    click_text_with(&mut d, "PIPE", Default::default());
    d.click("rsrc_picker_remove");
    assert!(d.has("remove_assignment_yes"), "Remove asks first");
    assert_eq!(
        crate::dialog_keys::top_default_label(&d.ctx).as_deref(),
        Some("No"),
        "No is the default, as observed for an assignment"
    );
    d.click("remove_assignment_yes");
    d.frames(2);
    assert!(assigned(&d.app.project, "RB-1020").is_empty(), "removed");
    d.click("rsrc_picker_close");
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(assigned(&d.app.project, "RB-1020"), vec![7], "undone");

    // Assign Activity Codes: Close, Assign, Remove, New, Help; Remove takes
    // the value off the activity after the question (Yes is the default).
    select_row(&mut d, index_of(&start, "RB-1020"));
    assign_menu(&mut d, "menu_assign_codes");
    assert!(
        in_column(
            &mut d,
            &[
                "picker_close",
                "picker_assign",
                "picker_code_remove",
                "picker_code_new",
                "picker_help",
            ],
        ),
        "Close, Assign, Remove, New, Help"
    );
    let buttons = live(&mut d);
    for b in ["Assign", "Remove", "New..."] {
        assert!(
            !buttons.contains(&b.to_string()),
            "nothing selected: {b} is off: {buttons:?}"
        );
    }
    click_text_with(&mut d, "PL", Default::default());
    d.click("picker_assign");
    d.frames(2);
    assert_eq!(phase_of(&d.app.project, "RB-1020"), vec!["PL".to_string()]);
    let steps = d.app.history.counts().0;
    click_text_with(&mut d, "PL", Default::default());
    d.click("picker_code_remove");
    assert!(d.has("remove_code_yes"), "Remove asks first");
    d.click("remove_code_yes");
    d.frames(2);
    assert!(phase_of(&d.app.project, "RB-1020").is_empty(), "removed");
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    d.click("picker_close");
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(phase_of(&d.app.project, "RB-1020"), vec!["PL".to_string()]);
}

/// Click the cell of row `row`, field `field` of view table `table`.
fn view_cell(d: &mut Driver, table: &str, row: usize, field: usize, modifiers: egui::Modifiers) {
    d.frames(1);
    let x = d.rect(&format!("{table}_col_{field}")).center().x;
    let y = d.rect(&format!("{table}_row_{row}")).center().y;
    d.press_at(Pos2::new(x, y), egui::PointerButton::Primary, modifiers);
    d.frames(1);
}

fn resource_name(p: &ProjectData, id: i64) -> String {
    views::resource_edit::value(&p.document, id, "rsrc_name")
}

#[test]
fn workflow_fill_down_and_replace_work_in_the_other_tables() {
    let mut d = Driver::new("wf-te-views", assign_project());
    show(&mut d, "resources");
    d.app.views.resources.assigned_only = false;
    d.frames(3);
    let undo = d.app.history.counts().0;
    // Rows: PIPE (7), its child PIPE.CRANE (8), IDLE (9). Click IDLE's
    // Resource Name, then Ctrl+click PIPE's above it; Edit > Fill Down.
    view_cell(&mut d, "resources", 2, 1, Default::default());
    view_cell(&mut d, "resources", 0, 1, ctrl());
    edit_menu_here(&mut d, "edit_fill_down");
    let p = &d.app.project;
    assert_eq!(
        resource_name(p, 7),
        "Unused",
        "PIPE takes IDLE's name: {}",
        d.app.status
    );
    assert_eq!(resource_name(p, 8), "Crane", "an unselected row stays");
    assert_eq!(resource_name(p, 9), "Unused");
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(resource_name(&d.app.project, 7), "Pipefitter", "undone");

    // Find in the Resources table selects the row with the text.
    edit_menu_here(&mut d, "menu_find");
    type_into(&mut d, "find_what", "Crane");
    d.click("find_next");
    d.frames(2);
    assert_eq!(d.app.views.resources.selected, Some(8), "{}", d.app.status);
    d.click("find_close");
    d.frames(1);

    // Replace in the WBS table: every WBS name with the text.
    show(&mut d, "wbs");
    let names = |p: &ProjectData| -> Vec<String> { p.wbs.iter().map(|w| w.name.clone()).collect() };
    let before = names(&d.app.project);
    let word = before
        .iter()
        .flat_map(|n| n.split_whitespace())
        .find(|w| w.len() > 4)
        .expect("a WBS name with a word")
        .to_string();
    let steps = d.app.history.counts().0;
    edit_menu_here(&mut d, "menu_replace");
    type_into(&mut d, "find_what", &word);
    type_into(&mut d, "replace_with", "Zq");
    d.click("replace_all");
    d.frames(2);
    let after = names(&d.app.project);
    assert!(
        after.iter().any(|n| n.contains("Zq")) && after.iter().all(|n| !n.contains(&word)),
        "{word} replaced in the WBS names: {after:?}, {}",
        d.app.status
    );
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    d.click("find_close");
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(names(&d.app.project), before, "undone");
}
