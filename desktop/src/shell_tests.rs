//! Headless tests for the application shell: keys reaching only the layer
//! on top, menus, the status bar and the shell's dialogs. They drive
//! `App::update_ui` through `qa_sweep::Driver` with synthetic input only.
use crate::qa_sweep::Driver;
use eframe::egui::{self, Key};

fn driver(name: &str) -> Driver {
    let mut d = Driver::new(name, crate::reports::test_support::scheduled_starter());
    d.home();
    d
}

fn ctrl() -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    }
}

/// Escape on an error box closes the box, not the dialog beneath it.
#[test]
fn escape_on_an_error_box_leaves_the_dialog_beneath_open() {
    let mut d = driver("escape-error");
    d.app.open_new_layout();
    d.frames(3);
    d.app.layout_opts.drafts.new_layout.as_mut().unwrap().name = "Typed name".into();
    d.app.error("A layout with this name exists.".into());
    d.frames(3);
    d.key(Key::Escape);
    assert!(d.app.error.is_none(), "the error box closed");
    let draft = d.app.layout_opts.drafts.new_layout.as_ref();
    assert_eq!(
        draft.map(|n| n.name.as_str()),
        Some("Typed name"),
        "New Layout stays open with what was typed"
    );
    d.key(Key::Escape);
    assert!(
        d.app.layout_opts.drafts.new_layout.is_none(),
        "then Escape closes it"
    );
}

/// Escape on an open drop-down list closes the list, not the dialog.
#[test]
fn escape_on_an_open_drop_down_closes_only_the_list() {
    let mut d = driver("escape-combo");
    d.app.open_user_preferences();
    d.app.prefs.tab = crate::user_prefs::PrefsTab::Application;
    d.frames(3);
    d.click("prefs_startup");
    assert!(d.has("prefs_startup_item_0"), "the list is open");
    d.key(Key::Escape);
    assert!(d.app.prefs.open, "User Preferences stays open");
    assert!(!d.has("prefs_startup_item_0"), "the list closed");
    d.key(Key::Escape);
    assert!(!d.app.prefs.open, "a second Escape closes the dialog");
}

/// Enter in the Help search field does not commit Edit Relationship.
#[test]
fn enter_in_help_search_leaves_edit_relationship_open() {
    let mut d = driver("help-enter");
    let links = d.app.project.model.links.clone();
    d.app.edit_link(0);
    d.frames(3);
    d.app.link_editor.as_mut().unwrap().lag_text = "3d".into();
    d.click("link_help");
    assert!(d.app.help.open);
    d.click("help_search");
    d.type_text("lag");
    d.key(Key::Enter);
    d.frames(2);
    assert!(
        d.app.link_editor.is_some(),
        "Edit Relationship is still open"
    );
    assert_eq!(d.app.project.model.links, links, "nothing was committed");
    // With Help closed, Enter runs OK.
    d.key(Key::Escape);
    assert!(!d.app.help.open);
    d.key(Key::Enter);
    d.frames(2);
    assert!(d.app.link_editor.is_none(), "Enter ran OK");
    assert_ne!(d.app.project.model.links, links, "the new lag was kept");
}

/// Ctrl+S saves while a Details field has the keyboard; the field's
/// text is part of what is saved.
#[test]
fn ctrl_s_saves_while_a_details_field_has_the_keyboard() {
    let mut d = driver("ctrl-s-typing");
    d.select(0);
    d.frames(2);
    let name = d.app.project.name(0).to_string();
    let task = d.app.project.model.activities[0].task_id;
    // The Details pane is the lower copy of the name.
    let (_, rect) = d
        .texts()
        .into_iter()
        .filter(|(t, _)| *t == name)
        .max_by(|a, b| a.1.top().total_cmp(&b.1.top()))
        .expect("the name is shown in Details");
    d.click_at(rect.center());
    assert!(
        d.ctx.wants_keyboard_input(),
        "the name field has the keyboard"
    );
    d.key_with(Key::A, ctrl());
    d.type_text("Renamed while typing");
    d.frames(1);
    d.key_with(Key::S, ctrl());
    d.frames(2);
    assert!(!d.app.any_dirty(), "saved: {}", d.app.status);
    let store = crate::store::Store::open(d.dir.join("workspace.db")).unwrap();
    let saved = store.last().unwrap().unwrap();
    assert_eq!(saved.details[&task].name, "Renamed while typing");
}

/// P3: an accelerator needs exactly its modifiers.
#[test]
fn accelerators_need_their_exact_modifiers() {
    let mut d = driver("exact-modifiers");
    d.app.dirty = true;
    let ctrl_shift = egui::Modifiers {
        shift: true,
        ..ctrl()
    };
    let alt_gr = egui::Modifiers {
        alt: true,
        ..ctrl()
    };
    d.key_with(Key::S, ctrl_shift);
    assert!(d.app.dirty, "Ctrl+Shift+S is not Save");
    d.key_with(Key::S, alt_gr);
    assert!(d.app.dirty, "AltGr+S is not Save");
    d.key_with(Key::S, ctrl());
    assert!(!d.app.dirty, "Ctrl+S saves");
}

/// Recent Projects lists the ten most recently saved, newest first.
#[test]
fn recent_projects_lists_the_newest_ten_first() {
    let mut d = driver("recent-projects");
    // The store lists newest first.
    d.app.saved_projects = (1..=12)
        .map(|n| (100 + n, format!("Project {n}")))
        .collect();
    d.open_menu("menu_bar_file");
    assert!(d.hover_text("Recent Projects"), "the submenu is drawn");
    assert!(d.has("menu_recent_101"), "the newest is listed");
    assert!(d.has("menu_recent_110"), "the tenth is listed");
    assert!(!d.has("menu_recent_111"), "the eleventh is not");
    let first = d.rect("menu_recent_101").top();
    assert!(
        first < d.rect("menu_recent_102").top(),
        "the newest is entry 1"
    );
}

/// The Edit toolbar's Copy, Paste, Delete and Cut act on the selected
/// activities as the Edit menu does.
#[test]
fn edit_toolbar_works_in_the_activities_view() {
    let mut d = driver("edit-toolbar");
    let count = d.app.project.model.activities.len();
    d.select(0);
    d.frames(2);
    d.click("dock_copy");
    assert!(
        d.app.editing.clipboard.is_some(),
        "Copy filled the clipboard"
    );
    d.click("dock_delete");
    assert_eq!(
        d.app.delete_confirm.as_ref().map(Vec::len),
        Some(1),
        "Delete asks first, as Edit > Delete does"
    );
    d.key(Key::Escape);
    d.app.delete_confirm = None;
    assert_eq!(d.app.project.model.activities.len(), count);
    d.app.editing.options.copy.show_dialog = false;
    d.click("dock_paste");
    d.frames(2);
    assert_eq!(
        d.app.project.model.activities.len(),
        count + 1,
        "Paste added a copy"
    );
    d.select(0);
    d.frames(2);
    d.click("dock_cut");
    d.frames(2);
    // Cut runs Edit > Cut, the command Ctrl+X runs: it marks the activity
    // for a move without asking and without removing it.
    assert!(d.app.delete_confirm.is_none(), "Cut does not ask");
    assert_eq!(d.app.project.model.activities.len(), count + 1);
    assert!(d.app.editing.clipboard.as_ref().is_some_and(|c| c.cut));
}

/// About shows the version and names the licenses and the notices.
#[test]
fn about_shows_the_version_and_licenses() {
    let mut d = driver("about");
    d.app.show_about = true;
    d.frames(3);
    let texts: Vec<String> = d.texts().into_iter().map(|(t, _)| t).collect();
    let version = format!("Version {}", env!("CARGO_PKG_VERSION"));
    assert!(texts.contains(&version), "{texts:?}");
    let all = texts.join(" ");
    for needle in ["MIT License", "Apache License 2.0", "THIRD_PARTY.md"] {
        assert!(all.contains(needle), "About names {needle}");
    }
    d.key(Key::Escape);
    assert!(!d.app.show_about);
}

/// A bad lag is reported inside Edit Relationship, beside the field.
#[test]
fn a_bad_lag_is_reported_inside_edit_relationship() {
    let mut d = driver("bad-lag");
    let links = d.app.project.model.links.clone();
    d.app.edit_link(0);
    d.frames(3);
    d.app.link_editor.as_mut().unwrap().lag_text = "soon".into();
    d.click("link_ok");
    let editor = d.app.link_editor.as_ref().expect("the dialog stays open");
    let error = editor.error.clone().expect("the reason is kept");
    let lag = d.rect("link_lag");
    let shown = d
        .texts()
        .into_iter()
        .find(|(t, _)| *t == error)
        .expect("the reason is drawn");
    assert!(
        shown.1.top() >= lag.bottom() - 1. && shown.1.top() < lag.bottom() + 12.,
        "under the Lag field: {:?} vs {lag:?}",
        shown.1
    );
    assert_eq!(d.app.project.model.links, links);
}

/// Table, Font and Row refuses a row height it cannot use and says so
/// beside the field instead of ignoring it.
#[test]
fn a_bad_row_height_is_reported_inside_table_font() {
    let mut d = driver("bad-row-height");
    let height = d.app.view.row_height;
    d.app.open_table_font();
    d.frames(3);
    {
        let t = d.app.layout_opts.drafts.table.as_mut().unwrap();
        t.keep = false;
        t.optimize = false;
        t.height = "tall".into();
    }
    d.click("table_font_ok");
    let draft = d.app.layout_opts.drafts.table.as_ref().expect("still open");
    let error = draft.error.clone().expect("the reason is kept");
    assert!(
        d.texts().iter().any(|(t, _)| *t == error),
        "the reason is drawn"
    );
    assert_eq!(d.app.view.row_height, height, "nothing changed");
    d.app.layout_opts.drafts.table.as_mut().unwrap().height = "30".into();
    d.click("table_font_ok");
    assert!(d.app.layout_opts.drafts.table.is_none(), "OK closes");
    assert_eq!(d.app.view.row_height, 30.);
}

/// New Project opens with the keyboard in Project Name, Enter runs
/// Finish, a refusal shows under its field, and the buttons have the kit
/// size.
#[test]
fn new_project_is_a_kit_dialog() {
    let mut d = driver("new-project-kit");
    let before = d.app.store.list().unwrap().len();
    d.key_with(Key::N, ctrl());
    d.frames(3);
    let finish = d.rect("new_project_finish");
    assert_eq!(
        finish.size(),
        crate::chrome::DIALOG_BUTTON,
        "kit button size"
    );
    let name = d.rect("new_project_name");
    assert!(d.ctx.wants_keyboard_input(), "a field has the keyboard");
    d.type_text("Typed first");
    d.frames(1);
    assert_eq!(d.app.new_project.as_ref().unwrap().name, "Typed first");
    // A taken ID: Enter runs Finish, which refuses under the ID field.
    let taken = d.app.taken_project_ids();
    d.app.new_project.as_mut().unwrap().project_id = taken[0].clone();
    d.key(Key::Enter);
    d.frames(2);
    let w = d.app.new_project.as_ref().expect("still open");
    assert_eq!(w.error_field, Some("new_project_id"), "{:?}", w.error);
    let error = w.error.clone().unwrap();
    let id = d.rect("new_project_id");
    let (_, shown) = d
        .texts()
        .into_iter()
        .find(|(t, _)| *t == error)
        .expect("the reason is drawn");
    assert!(
        shown.top() > id.bottom() && shown.bottom() < name.top() + 40.,
        "under the ID"
    );
    assert_eq!(d.app.store.list().unwrap().len(), before);
    // A free ID: Enter adds the project.
    d.app.new_project.as_mut().unwrap().project_id = "KIT-1".into();
    d.key(Key::Enter);
    d.frames(3);
    assert!(d.app.new_project.is_none(), "Enter ran Finish");
    assert_eq!(d.app.store.list().unwrap().len(), before + 1);
}

/// External Applications opens with the keyboard in Name, shows a
/// refusal under its field, has kit buttons, and closes on Escape.
#[test]
fn external_applications_is_a_kit_dialog() {
    let mut d = driver("external-apps-kit");
    d.app.open_external_apps();
    d.frames(3);
    assert_eq!(
        d.rect("external_apps_close").size(),
        crate::chrome::DIALOG_BUTTON
    );
    assert!(d.ctx.wants_keyboard_input(), "Name has the keyboard");
    // No command line: Add refuses under Command Line.
    d.type_text("Viewer");
    d.click("external_apps_add");
    d.frames(2);
    let error = d.app.external.dialog.as_ref().unwrap().error.clone();
    let error = error.expect("refused");
    let command = d.rect("external_apps_command");
    let (_, shown) = d
        .texts()
        .into_iter()
        .find(|(t, _)| *t == error)
        .expect("the reason is drawn");
    assert!(shown.top() > command.bottom(), "under Command Line");
    assert!(shown.top() < d.rect("external_apps_arguments").top());
    assert!(d.app.external.apps.is_empty());
    d.key(Key::Escape);
    assert!(d.app.external.dialog.is_none(), "Escape closes");
    d.app.open_external_apps();
    d.frames(3);
    d.key(Key::Enter);
    d.frames(2);
    assert!(d.app.external.dialog.is_none(), "Enter closes");
}

/// Drag with the primary button from `from` by `by`.
fn drag(d: &mut Driver, from: egui::Pos2, by: egui::Vec2) {
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let _ = d.frame(vec![egui::Event::PointerMoved(from)]);
    let _ = d.frame(vec![button(from, true)]);
    for step in 1..=4 {
        let at = from + by * (step as f32 / 4.);
        let _ = d.frame(vec![egui::Event::PointerMoved(at)]);
    }
    let _ = d.frame(vec![button(from + by, false)]);
    d.frames(2);
}

/// A fixed-layout dialog moves by its title bar and stays on screen.
#[test]
fn a_fixed_layout_dialog_moves_by_its_title_bar_and_stays_on_screen() {
    let mut d = driver("dialog-drag");
    d.app.edit_link(0);
    d.frames(3);
    let close = d.rect("edit_relationship_title_close");
    let grip = close.left_center() - egui::vec2(150., 0.);
    drag(&mut d, grip, egui::vec2(-120., 80.));
    let moved = d.rect("edit_relationship_title_close");
    assert_eq!(
        moved.min - close.min,
        egui::vec2(-120., 80.),
        "moved with the pointer"
    );
    assert!(d.app.link_editor.is_some(), "still open");
    // Dragged far off the screen, it stops at the edge.
    let screen = d.ctx.content_rect();
    drag(
        &mut d,
        moved.left_center() - egui::vec2(150., 0.),
        egui::vec2(4000., 4000.),
    );
    let edge = d.rect("edit_relationship_title_close");
    assert!(edge.right() <= screen.right() && edge.right() > screen.right() - 4.);
    let client = crate::dialog_kit::noted_client(&d.ctx, "edit_relationship").unwrap();
    assert!(
        client.bottom() <= screen.bottom() + 0.5,
        "{client:?} in {screen:?}"
    );
}

/// A fixed-layout dialog taller than the window scrolls instead of
/// running off the screen.
#[test]
fn a_tall_dialog_scrolls_in_a_short_window() {
    let project = crate::reports::test_support::scheduled_starter();
    let mut d = Driver::probed("dialog-short", project, egui::vec2(1100., 480.), None);
    d.home();
    d.app.open_user_preferences();
    d.frames(4);
    let screen = d.ctx.content_rect();
    let layer = d
        .ctx
        .memory(|m| {
            m.layer_ids()
                .find(|l| l.id == egui::Id::new(("schedule_dialog", "user_preferences")))
        })
        .expect("User Preferences is drawn");
    let area = d.ctx.memory(|m| m.area_rect(layer.id)).unwrap();
    assert!(area.top() >= screen.top() && area.bottom() <= screen.bottom() + 0.5);
    // The client is taller than what shows: it scrolls.
    let client = crate::dialog_kit::noted_client(&d.ctx, "user_preferences").unwrap();
    assert!(
        client.height() > area.height() - 31.,
        "the client is taller"
    );
    assert!(d.app.prefs.open);
    d.key(Key::Escape);
    assert!(!d.app.prefs.open, "Escape still closes it");
}

/// The help window's outer rectangle.
fn help_rect(d: &Driver) -> egui::Rect {
    d.ctx
        .memory(|m| m.area_rect(egui::Id::new("help_viewer")))
        .expect("Help is drawn")
}

/// Help opens beside the dialog it documents, moves by its title bar,
/// grows by its corner grip, and its text can be selected and copied.
#[test]
fn help_opens_beside_its_dialog_moves_grows_and_copies() {
    let mut d = driver("help-window");
    d.app.edit_link(0);
    d.frames(3);
    d.click("link_help");
    d.frames(3);
    let dialog = crate::dialog_kit::noted_client(&d.ctx, "edit_relationship").unwrap();
    let help = help_rect(&d);
    assert!(
        !help.intersects(dialog),
        "Help {help:?} covers the dialog {dialog:?}"
    );
    // The title bar moves it (left of Edit Relationship it opens close to
    // the screen's left edge, so the move stays short of it).
    let grip = help.left_top() + egui::vec2(200., 15.);
    drag(&mut d, grip, egui::vec2(-20., 20.));
    let moved = help_rect(&d);
    assert_eq!(moved.min - help.min, egui::vec2(-20., 20.), "moved");
    // Up in the corner, the grip resizes it, never narrower than it started.
    drag(
        &mut d,
        moved.left_top() + egui::vec2(200., 15.),
        egui::vec2(-3000., -3000.),
    );
    assert_eq!(
        help_rect(&d).min,
        d.ctx.content_rect().min,
        "kept on screen"
    );
    let corner = d.rect("help_resize").center();
    drag(&mut d, corner, egui::vec2(-40., -10.));
    let shrunk = help_rect(&d).size() - moved.size();
    assert_eq!(shrunk, egui::vec2(0., -10.), "shorter, not narrower");
    let corner = d.rect("help_resize").center();
    drag(&mut d, corner, egui::vec2(50., 40.));
    let grown = help_rect(&d).size() - moved.size();
    assert_eq!(grown, egui::vec2(50., 30.), "wider and taller");
    // A drag across the topic text selects it; Ctrl+C copies it.
    let title = d
        .texts()
        .into_iter()
        .rev()
        .find(|(t, _)| t == "Edit Relationship");
    let (_, title) = title.expect("the topic title shows");
    drag(
        &mut d,
        title.left_center() + egui::vec2(1., 0.),
        egui::vec2(title.width(), 0.),
    );
    let out = d.frame(vec![egui::Event::Copy]);
    let copied: Vec<String> = out
        .platform_output
        .commands
        .iter()
        .filter_map(|c| match c {
            egui::OutputCommand::CopyText(t) => Some(t.clone()),
            _ => None,
        })
        .collect();
    assert!(
        copied.iter().any(|t| t.starts_with("Edit Rel")),
        "copied {copied:?}"
    );
}

/// Save and the Modified panel count every open project with
/// changes, and the panel says that changes are saved on close.
#[test]
fn save_and_modified_count_every_open_project() {
    use crate::ProjectData;
    let doc = openxer_xer::starter_portfolio();
    let rb = ProjectData::from_document(doc.clone(), 1, "portfolio".into()).unwrap();
    let ny = ProjectData::from_document(doc, 2, "portfolio".into()).unwrap();
    let mut d = Driver::new("modified-parked", rb);
    d.home();
    let ny_id = d.app.store.save(&ny).unwrap();
    d.app.restore_open_set();
    let rb_id = d
        .app
        .open_set
        .active
        .expect("the open project is in the workspace");
    d.app.open_project_set(&[rb_id, ny_id]);
    d.frames(2);
    assert!(!d.app.any_dirty());
    assert!(!d.texts().iter().any(|(t, _)| t == crate::frame::MODIFIED));
    // Only the project in the background has changes.
    d.app.dirty = false;
    for p in d.app.open_set.parked.iter_mut() {
        p.dirty = true;
    }
    d.frames(2);
    assert!(
        d.texts().iter().any(|(t, _)| t == crate::frame::MODIFIED),
        "the panel shows"
    );
    d.click("save");
    d.frames(2);
    assert!(!d.app.any_dirty(), "File > Save saved it: {}", d.app.status);
}

/// User Preferences > Group and Sorting > Show ID / Code and Show Name /
/// Description shape the group band labels.
#[test]
fn group_band_preferences_shape_the_band_labels() {
    let mut d = driver("band-prefs");
    d.frames(2);
    let both = d.app.band_label(0);
    let (code, name) = both.split_once("  ").expect("code and name");
    d.app.formats.band_name = false;
    d.app.drop_frame_cache();
    d.frames(2);
    assert_eq!(d.app.band_label(0), code, "ID / Code only");
    d.app.formats.band_name = true;
    d.app.formats.band_code = false;
    d.app.drop_frame_cache();
    d.frames(2);
    assert_eq!(d.app.band_label(0), name, "Name / Description only");
}

/// The Schedule button shows a wait cursor, status message and modal progress
/// dialog while the background worker runs, then commits when it finishes.
#[test]
fn schedule_shows_a_wait_cursor_and_a_message_first() {
    let mut d = driver("busy-schedule");
    let runs = d.app.schedule_runs;
    d.key(Key::F9);
    assert!(d.app.sched.schedule_open);
    d.frames(2);
    let pos = d.rect("schedule_run").center();
    let press = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let _ = d.frame(vec![egui::Event::PointerMoved(pos), press(true)]);
    let out = d.frame(vec![press(false)]);
    assert_eq!(d.app.status, "Scheduling...");
    assert_eq!(out.platform_output.cursor_icon, egui::CursorIcon::Wait);
    assert_eq!(d.app.schedule_runs, runs, "not yet");
    assert!(d.app.busy_active(), "background worker started");
    assert!(d.has("busy_cancel"), "progress dialog Cancel button shown");
    d.wait_busy();
    assert_eq!(d.app.schedule_runs, runs + 1, "then it ran");
    assert!(!d.app.sched.schedule_open);
}

/// P3: each mnemonic letter is used once per menu: N is User Preferences in
/// the Edit menu (U is Undo, S Spell Check; Find Next moved to X), L is Chart
/// Font and Colors in the View menu (N is Table Font and Row).
#[test]
fn menu_mnemonics_are_unique() {
    let alt = egui::Modifiers {
        alt: true,
        ..Default::default()
    };
    let mut d = driver("mnemonics");
    d.key_with(Key::E, alt);
    d.key(Key::N);
    d.frames(2);
    assert!(d.app.prefs.open, "Alt+E, N opens User Preferences");
    d.key(Key::Escape);
    d.home();
    d.key_with(Key::V, alt);
    d.key(Key::L);
    d.frames(2);
    assert!(
        d.app.layout_opts.drafts.chart.is_some(),
        "Alt+V, L opens Chart Font and Colors"
    );
    d.key(Key::Escape);
    d.home();
    d.key_with(Key::V, alt);
    d.key(Key::N);
    d.frames(2);
    assert!(
        d.app.layout_opts.drafts.table.is_some(),
        "Alt+V, N opens Table Font and Row"
    );
}

/// P4: the toolbar's Progress Spotlight and Progress Line are off outside
/// the Activities view, as their View menu items are.
#[test]
fn progress_toolbar_buttons_follow_the_menu() {
    let mut d = driver("toolbar-enable");
    d.click("directory_projects");
    d.frames(2);
    assert!(d.has("tool_spotlight"), "the button is shown");
    let before = (
        d.app.gantt.options.spotlight,
        d.app.gantt.options.progress_line.show,
    );
    d.click("tool_spotlight");
    d.click("tool_progress_line");
    let after = (
        d.app.gantt.options.spotlight,
        d.app.gantt.options.progress_line.show,
    );
    assert_eq!(after, before, "disabled outside Activities");
}

/// P5: the first start says the sample project is open and how to import;
/// an XER file dropped on the window opens File > Import with it chosen.
#[test]
fn first_run_note_and_dropped_xer() {
    let starter = openxer_xer::starter();
    let mut d = Driver::new("first-run", starter);
    d.frames(2);
    assert_eq!(d.app.status, crate::first_run::SAMPLE_NOTE);
    // A file that is not an XER is refused.
    d.home();
    d.app
        .open_dropped(&[std::path::PathBuf::from("C:/plans/notes.txt")], false);
    assert!(d.app.interchange.xer.is_none());
    // Over a dialog, nothing opens.
    d.app
        .open_dropped(&[std::path::PathBuf::from("C:/plans/job.XER")], true);
    assert!(d.app.interchange.xer.is_none());
    d.app
        .open_dropped(&[std::path::PathBuf::from("C:/plans/job.XER")], false);
    let w = d
        .app
        .interchange
        .xer
        .as_ref()
        .expect("File > Import opened");
    assert_eq!(
        w.path_text,
        std::path::PathBuf::from("C:/plans/job.XER")
            .display()
            .to_string()
    );
}
