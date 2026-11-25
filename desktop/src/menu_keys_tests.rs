//! Keyboard tests: F3, the accelerator gate, and the menu bar keys.
use super::*;
use crate::qa_sweep::Driver;

fn starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn driver() -> Driver {
    let mut d = Driver::new("keys", starter());
    d.home();
    d.select(0);
    d
}

fn alt() -> egui::Modifiers {
    egui::Modifiers::ALT
}

fn ctrl() -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    }
}

fn name_of(d: &Driver, row: Option<usize>) -> String {
    let Some(i) = row else { return String::new() };
    let id = d.app.project.model.activities[i].task_id.to_string();
    let table = d.app.project.document.table("TASK").unwrap();
    let row = table.rows.iter().find(|r| table.value(r, "task_id") == id);
    row.map(|r| table.value(r, "task_name").to_string())
        .unwrap_or_default()
}

/// Presses Down until the item with this caption is highlighted.
fn down_to(d: &mut Driver, label: &str) {
    for _ in 0..60 {
        if menu_keys::hot_label(&d.ctx).as_deref() == Some(label) {
            return;
        }
        d.key(Key::ArrowDown);
    }
    panic!(
        "Down never reached {label:?}; highlighted {:?}",
        menu_keys::hot_label(&d.ctx)
    );
}

#[test]
fn f3_finds_the_next_match_and_does_nothing_without_a_search() {
    let mut d = driver();
    d.select(2);
    d.key(Key::F3);
    assert_eq!(d.app.selected, Some(2), "F3 without a search moves nothing");
    // Ctrl+F opens Find; type a name that two activities share.
    d.key_with(Key::F, ctrl());
    d.click("find_what");
    d.type_text("structural");
    d.frames(2);
    d.click("find_next");
    let first = name_of(&d, d.app.selected);
    assert!(first.contains("structural"), "Find Next went to {first:?}");
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.grid.dialogs.find.is_none(), "Escape closes Find");
    d.key(Key::F3);
    let second = name_of(&d, d.app.selected);
    assert!(second.contains("structural"), "F3 went to {second:?}");
    assert_ne!(first, second, "F3 selects the next match");
    d.key(Key::F3);
    assert_eq!(name_of(&d, d.app.selected), first, "F3 wraps around");
}

/// Ctrl+F opens Find and Ctrl+H opens Replace with the keyboard in "Find
/// what": typed text lands there without a click.
#[test]
fn ctrl_f_and_ctrl_h_open_find_with_the_keyboard_in_find_what() {
    for (key, replace) in [(Key::F, false), (Key::H, true)] {
        let mut d = driver();
        d.select(0);
        d.key_with(key, ctrl());
        d.frames(2);
        d.type_text("q");
        d.frames(2);
        let find = d.app.grid.dialogs.find.as_ref().expect("Find is open");
        assert_eq!(find.replace, replace, "{key:?} opens the right dialog");
        assert_eq!(find.what, "q", "{key:?}: the typed text is in Find what");
        assert!(
            find.with.is_empty(),
            "{key:?}: nothing went to Replace with"
        );
    }
}

#[test]
fn f3_does_not_step_down_a_row_when_nothing_matches() {
    let mut d = driver();
    d.select(0);
    d.key_with(Key::F, ctrl());
    d.click("find_what");
    d.type_text("zzzqqq");
    d.frames(2);
    d.click("find_next");
    d.key(Key::Escape);
    let at = d.app.selected;
    d.key(Key::F3);
    assert_eq!(d.app.selected, at);
}

type Probe = (usize, String, usize, usize, bool);

fn state(d: &Driver) -> (ProjectData, Probe) {
    (
        d.app.project.clone(),
        (
            d.app.history.counts().0,
            d.app.project.model.project.name.clone(),
            d.app.saved_projects.len(),
            d.app.open_ids().len(),
            d.app.open_set.open_dialog.is_some(),
        ),
    )
}

#[test]
fn accelerators_work_with_no_dialog_open() {
    let mut d = driver();
    d.key_with(Key::N, ctrl());
    assert!(
        d.app.new_project.is_some(),
        "Ctrl+N opens the New Project dialog"
    );
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.new_project.is_none(), "Escape closes the wizard");
    d.key_with(Key::O, ctrl());
    assert!(
        d.app.open_set.open_dialog.is_some(),
        "Ctrl+O opens the Open dialog"
    );
}

#[test]
fn accelerators_wait_while_a_dialog_is_open() {
    type Open = fn(&mut App);
    let openers: [(&str, Open); 4] = [
        ("Columns", |a| a.open_columns_dialog()),
        ("Filters", |a| a.open_filters_dialog()),
        ("Calendars", |a| a.open_calendars()),
        ("Schedule Options", |a| a.open_options()),
    ];
    for (name, open) in openers {
        let mut d = driver();
        open(&mut d.app);
        d.frames(4);
        let (project, probe) = state(&d);
        let status = d.app.status.clone();
        let schedule_open = d.app.sched.schedule_open;
        for (key, modifiers) in [
            (Key::W, ctrl()),
            (Key::N, ctrl()),
            (Key::O, ctrl()),
            (Key::P, ctrl()),
            (Key::S, ctrl()),
            (Key::F5, Default::default()),
            (Key::F9, Default::default()),
            (Key::F10, Default::default()),
            (Key::F12, egui::Modifiers::SHIFT),
            (Key::F1, alt()),
        ] {
            d.key_with(key, modifiers);
            let (after, now) = state(&d);
            assert!(
                after.model == project.model,
                "{name}: {modifiers:?} {key:?} changed the project"
            );
            assert_eq!(now, probe, "{name}: {modifiers:?} {key:?}");
            assert_eq!(d.app.status, status, "{name}: {modifiers:?} {key:?}");
            assert_eq!(
                d.app.sched.schedule_open, schedule_open,
                "{name}: {modifiers:?} {key:?} opened the Schedule dialog"
            );
        }
    }
}

#[test]
fn text_field_keeps_its_keys() {
    let mut d = driver();
    // A cell editor is a text field: Ctrl+N would add a project, F5 refresh.
    d.key(Key::F2);
    assert!(d.app.edit_cell.is_some(), "F2 opens the cell editor");
    let name = d.app.project.model.project.name.clone();
    d.app.status.clear();
    d.key_with(Key::N, ctrl());
    d.key_with(Key::O, ctrl());
    d.key(Key::F5);
    assert_eq!(d.app.project.model.project.name, name);
    assert!(d.app.open_set.open_dialog.is_none());
    assert_ne!(d.app.status, "View refreshed");
    // F5 in the Find dialog's field edits the field, not the view.
    d.key(Key::Escape);
    d.key_with(Key::F, ctrl());
    d.click("find_what");
    d.type_text("F5 ");
    assert_eq!(d.app.grid.dialogs.find.as_ref().unwrap().what, "F5 ");
    assert_ne!(d.app.status, "View refreshed");
}

#[test]
fn alt_letter_then_down_and_enter_runs_a_menu_item() {
    let mut d = driver();
    d.key_with(Key::E, alt());
    assert!(d.has("menu_find"), "Alt+E opens the Edit menu");
    // Undo is disabled and Cut is the first enabled item.
    d.key(Key::ArrowDown);
    assert_eq!(menu_keys::hot_label(&d.ctx).as_deref(), Some("Cut"));
    down_to(&mut d, "Find...");
    d.key(Key::Enter);
    d.frames(2);
    assert!(d.app.grid.dialogs.find.is_some(), "Enter runs Find");
    assert!(!d.has("menu_find"), "the menu closes");
}

#[test]
fn disabled_items_are_skipped_and_up_wraps() {
    let mut d = driver();
    d.key_with(Key::E, alt());
    down_to(&mut d, "Find...");
    // Find Next is disabled with no search: Replace comes next.
    d.key(Key::ArrowDown);
    assert_eq!(menu_keys::hot_label(&d.ctx).as_deref(), Some("Replace..."));
    d.key(Key::ArrowUp);
    assert_eq!(menu_keys::hot_label(&d.ctx).as_deref(), Some("Find..."));
    d.key(Key::End);
    assert_eq!(
        menu_keys::hot_label(&d.ctx).as_deref(),
        Some("User Preferences...")
    );
    d.key(Key::ArrowDown);
    assert_eq!(menu_keys::hot_label(&d.ctx).as_deref(), Some("Cut"));
    d.key(Key::Escape);
    assert!(!d.has("menu_find"), "Escape closes the menu");
}

#[test]
fn right_opens_a_submenu_and_left_closes_it() {
    let mut d = driver();
    d.key_with(Key::T, alt());
    assert!(d.has("menu_schedule"), "Alt+T opens the Tools menu");
    down_to(&mut d, "Reports");
    d.key(Key::ArrowRight);
    d.frames(2);
    assert!(d.has("menu_reports"), "Right opens Reports");
    assert_eq!(menu_keys::hot_label(&d.ctx).as_deref(), Some("Reports"));
    d.key(Key::ArrowLeft);
    d.frames(2);
    assert!(!d.has("menu_reports"), "Left closes the submenu");
    assert!(d.has("menu_schedule"), "and keeps the menu");
    assert_eq!(menu_keys::hot_label(&d.ctx).as_deref(), Some("Reports"));
    d.key(Key::ArrowRight);
    d.frames(2);
    assert!(d.has("menu_reports"));
    d.key(Key::Escape);
    assert!(!d.has("menu_reports"));
    assert!(!d.has("menu_schedule"), "Escape closes the menu");
}

#[test]
fn enter_on_a_submenu_item_runs_what_it_holds() {
    let mut d = driver();
    d.key_with(Key::T, alt());
    down_to(&mut d, "Reports");
    d.key(Key::Enter);
    d.frames(2);
    assert!(d.has("menu_reports"));
    d.key(Key::Enter);
    d.frames(2);
    assert!(!d.has("menu_reports"), "the menu closes after a run");
    assert_eq!(d.app.views.main, crate::views::MainView::Reports);
    d.assert_clean("Tools > Reports > Reports");
}

#[test]
fn a_letter_runs_the_item_underlined_with_it() {
    let mut d = driver();
    d.key_with(Key::E, alt());
    d.key(Key::F);
    d.frames(2);
    assert!(d.app.grid.dialogs.find.is_some(), "F runs Find");
}

#[test]
fn left_and_right_move_between_menus() {
    let mut d = driver();
    d.key_with(Key::F, alt());
    assert!(d.has("menu_new"), "Alt+F opens the File menu");
    d.key(Key::ArrowRight);
    d.frames(2);
    assert!(d.has("menu_find"), "Right opens the Edit menu");
    d.key(Key::ArrowLeft);
    d.key(Key::ArrowLeft);
    d.frames(2);
    assert!(
        d.has("menu_help_about") || d.has("menu_about"),
        "wraps to Help"
    );
}

#[test]
fn alt_alone_takes_the_menu_bar() {
    let mut d = driver();
    let _ = d.frame_with(vec![], alt());
    let _ = d.frame(vec![]);
    assert_eq!(menu_keys::bar_title(&d.ctx).as_deref(), Some("File"));
    d.key(Key::ArrowRight);
    assert_eq!(menu_keys::bar_title(&d.ctx).as_deref(), Some("Edit"));
    d.key(Key::ArrowDown);
    d.frames(2);
    assert!(d.has("menu_find"), "Down opens the Edit menu");
    d.key(Key::Escape);
    assert!(!d.has("menu_find"));
    // Alt again, then Escape, leaves the bar.
    let _ = d.frame_with(vec![], alt());
    let _ = d.frame(vec![]);
    assert!(menu_keys::bar_title(&d.ctx).is_some());
    d.key(Key::Escape);
    assert!(menu_keys::bar_title(&d.ctx).is_none());
}

#[test]
fn menu_keys_wait_while_a_dialog_is_open() {
    let mut d = driver();
    d.app.open_columns_dialog();
    d.frames(4);
    d.key_with(Key::E, alt());
    assert!(!d.has("menu_find"), "Alt+E opens no menu behind a dialog");
}

#[test]
fn main_edit_menu_shows_no_key_text_for_cut_copy_paste_add_and_delete() {
    let mut d = driver();
    d.click("menu_bar_edit");
    let texts: Vec<String> = d.texts().into_iter().map(|(t, _)| t).collect();
    for key in ["Ctrl+X", "Ctrl+C", "Ctrl+V", "Insert", "Ins", "Del"] {
        assert!(!texts.iter().any(|t| t == key), "Edit menu shows {key}");
    }
    for key in ["Ctrl+Z", "Ctrl+E", "Ctrl+A", "Ctrl+F", "F3", "Ctrl+R", "F7"] {
        assert!(texts.iter().any(|t| t == key), "Edit menu lacks {key}");
    }
}

fn weekly_hours(d: &mut Driver, detailed: bool) {
    d.app.open_calendars();
    d.frames(3);
    d.click("calendars_modify");
    if detailed {
        let ed = d.app.calendars.as_mut().unwrap().editor.as_mut().unwrap();
        ed.data.show_total = Some(false);
    }
    d.click("calendar_workweek");
    d.frames(3);
    assert!(d.has("workweek_ok"), "Calendar Weekly Hours is open");
}

fn workweek_texts(d: &Driver) -> Vec<String> {
    let ed = d.app.calendars.as_ref().unwrap().editor.as_ref().unwrap();
    ed.workweek.as_ref().unwrap().texts.to_vec()
}

#[test]
fn weekly_hours_opens_on_the_first_day_and_enter_runs_ok() {
    let mut d = driver();
    weekly_hours(&mut d, false);
    // The Sun field has the keyboard with its text selected: typing replaces it.
    d.type_text("5");
    d.frames(2);
    assert_eq!(workweek_texts(&d)[0], "5", "{:?}", workweek_texts(&d));
    d.key(Key::Enter);
    d.frames(2);
    assert!(!d.has("workweek_ok"), "Enter runs OK and closes the dialog");
    let ed = d.app.calendars.as_ref().unwrap().editor.as_ref().unwrap();
    assert_eq!(
        ed.data.week[0].iter().map(|s| s.1 - s.0).sum::<i64>(),
        300,
        "OK kept the 5 hours"
    );
}

#[test]
fn weekly_hours_escape_cancels_and_alt_letter_presses_a_button() {
    let mut d = driver();
    weekly_hours(&mut d, false);
    d.type_text("7");
    d.key(Key::Escape);
    d.frames(2);
    assert!(!d.has("workweek_ok"), "Escape closes the dialog");
    let ed = d.app.calendars.as_ref().unwrap().editor.as_ref().unwrap();
    assert!(ed.data.week[0].is_empty(), "Escape kept nothing");
    // Detailed hours: Alt+N presses Nonwork, Alt+W presses Work.
    let mut d = driver();
    weekly_hours(&mut d, true);
    let days = |d: &Driver| {
        let ed = d.app.calendars.as_ref().unwrap().editor.as_ref().unwrap();
        ed.workweek.as_ref().unwrap().week[1].len()
    };
    assert!(days(&d) > 0, "Monday has work time");
    d.key_with(Key::N, alt());
    assert_eq!(days(&d), 0, "Alt+N presses Nonwork");
    d.key_with(Key::W, alt());
    assert!(days(&d) > 0, "Alt+W presses Work");
}

#[test]
fn enter_runs_the_default_button_and_escape_cancels() {
    // Columns: OK applies and closes.
    let mut d = driver();
    d.app.open_columns_dialog();
    d.frames(3);
    d.key(Key::Enter);
    d.frames(2);
    assert!(d.app.grid.dialogs.columns.is_none(), "Enter runs OK");
    // Schedule: its own button is the default.
    let mut d = driver();
    d.app.open_schedule_dialog();
    d.frames(3);
    assert_eq!(
        crate::dialog_keys::top_default_label(&d.ctx).as_deref(),
        Some("Schedule")
    );
    d.key(Key::Escape);
    assert!(!d.app.sched.schedule_open, "Escape cancels");
    // Calendars: Close.
    let mut d = driver();
    d.app.open_calendars();
    d.frames(3);
    assert_eq!(
        crate::dialog_keys::top_default_label(&d.ctx).as_deref(),
        Some("Close")
    );
    d.key(Key::Enter);
    d.frames(2);
    assert!(d.app.calendars.is_none(), "Enter runs Close");
}

#[test]
fn enter_in_the_find_field_finds_one_match_per_press() {
    let mut d = driver();
    d.select(0);
    d.key_with(Key::F, ctrl());
    d.click("find_what");
    d.type_text("structural");
    d.key(Key::Enter);
    d.frames(2);
    let first = name_of(&d, d.app.selected);
    assert!(first.contains("structural"), "went to {first:?}");
    d.key(Key::Enter);
    d.frames(2);
    let second = name_of(&d, d.app.selected);
    assert!(
        second.contains("structural") && second != first,
        "{second:?}"
    );
}

#[test]
fn a_printable_key_on_an_editable_cell_opens_its_editor_with_that_character() {
    let mut d = driver();
    d.select(2);
    d.app.grid.focus_column = Some("activity_name".into());
    d.type_text("Z");
    d.frames(2);
    assert_eq!(d.app.edit_cell.as_ref().map(|(i, _)| *i), Some(2));
    assert!(d.app.edit_text.starts_with('Z'), "{:?}", d.app.edit_text);
    d.type_text("q");
    d.frames(2);
    assert_eq!(d.app.edit_text, "Zq");
    d.key(Key::Escape);
    assert!(d.app.edit_cell.is_none());
    // A read-only column ignores the key.
    d.app.grid.focus_column = Some("total_float".into());
    d.type_text("Z");
    d.frames(2);
    assert!(d.app.edit_cell.is_none());
}

/// Enter in the delete question of the Activities table answers No, the
/// button framed as the default, as in the WBS delete confirmation.
#[test]
fn enter_in_the_activity_delete_question_deletes_nothing() {
    let mut d = driver();
    let count = d.app.project.model.activities.len();
    d.select(2);
    d.key(Key::Delete);
    assert!(d.app.delete_confirm.is_some(), "Delete asks first");
    d.frames(2);
    assert_eq!(
        crate::dialog_keys::top_default_label(&d.ctx).as_deref(),
        Some("No")
    );
    d.key(Key::Enter);
    d.frames(2);
    assert!(d.app.delete_confirm.is_none(), "Enter answers");
    assert_eq!(
        d.app.project.model.activities.len(),
        count,
        "nothing deleted"
    );
}

/// Clicks a field, clears it and types `text`.
fn retype(d: &mut Driver, action: &str, text: &str) {
    d.click(action);
    d.key_with(Key::A, ctrl());
    d.key(Key::Backspace);
    d.type_text(text);
    d.frames(1);
}

/// A font size typed in Timescale is applied when Enter presses OK, and
/// dropped by Escape.
#[test]
fn enter_in_the_timescale_font_size_applies_the_typed_size() {
    let mut d = driver();
    let before = d.app.gantt.timescale.font_points;
    assert_ne!(before, 12.);
    d.click("menu_bar_view");
    d.click("view_timescale");
    d.frames(2);
    assert!(d.has("ts_ok"), "Timescale is open");
    retype(&mut d, "ts_font_size", "7");
    d.key(Key::Escape);
    d.frames(2);
    assert!(!d.has("ts_ok"), "Escape closes Timescale");
    assert_eq!(d.app.gantt.timescale.font_points, before, "Escape drops 7");
    d.click("menu_bar_view");
    d.click("view_timescale");
    d.frames(2);
    retype(&mut d, "ts_font_size", "12");
    d.key(Key::Enter);
    d.frames(2);
    assert!(!d.has("ts_ok"), "Enter runs OK");
    assert_eq!(
        d.app.gantt.timescale.font_points, 12.,
        "OK applied the typed size"
    );
}

/// A name typed in Details is committed before Ctrl+S saves and before F9
/// schedules: the command runs with the typed value.
#[test]
fn an_accelerator_commits_the_details_field_being_typed_in() {
    let mut d = driver();
    d.select(1);
    d.frames(2);
    assert!(d.has("detail_activity_name"), "Details shows the activity");
    retype(&mut d, "detail_activity_name", "Typed before saving");
    d.key_with(Key::S, ctrl());
    d.frames(2);
    assert_eq!(d.app.project.name(1), "Typed before saving");
    assert!(!d.app.any_dirty(), "Ctrl+S saved the typed name");
    let id = d.app.open_set.active.unwrap();
    let stored = d.app.store.load(id).unwrap();
    let task = d.app.project.model.activities[1].task_id;
    assert_eq!(stored.details[&task].name, "Typed before saving");
    retype(&mut d, "detail_activity_name", "Typed before F9");
    d.key(Key::F9);
    d.frames(3);
    assert_eq!(d.app.project.name(1), "Typed before F9");
}
