//! Workflow: the dialog look decided on 2026-11-23. Dialogs opened from the
//! menus draw their section captions bold and, in a bottom button row, put
//! Help first at the left (a right-hand column keeps Help last, as the
//! reference's columns do). A first start shows one toolbar row under the
//! menu bar; a stored arrangement with a second row loads as stored.
use super::*;

/// Is a caption `text` painted this frame in the bold face?
fn painted_bold(d: &mut Driver, text: &str) -> bool {
    let bold = crate::chrome::bold_font();
    let output = d.frame(vec![]);
    let fonts: Vec<egui::FontId> = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(shape) if shape.galley.text() == text => shape
                .galley
                .job
                .sections
                .first()
                .map(|s| s.format.font_id.clone()),
            _ => None,
        })
        .collect();
    assert!(!fonts.is_empty(), "{text:?} is painted");
    fonts.contains(&bold)
}

/// The keys of a bottom row, left to right.
fn row_order(d: &Driver, keys: &[&str]) -> Vec<String> {
    let mut found: Vec<(f32, String)> = keys
        .iter()
        .map(|k| (d.rect(k).left(), k.to_string()))
        .collect();
    for (k, (_, key)) in found.iter().enumerate().skip(1) {
        assert_eq!(
            d.rect(key).top(),
            d.rect(&found[0].1).top(),
            "{key} is in the row ({k})"
        );
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|(_, k)| k).collect()
}

/// The distinct tops of the toolbar buttons drawn in the top dock.
fn toolbar_rows(d: &Driver) -> Vec<f32> {
    let mut tops: Vec<f32> = vec![];
    for bar in crate::menu::TOP_ROW_BARS
        .iter()
        .chain(crate::menu::SECOND_ROW_BARS.iter())
    {
        let buttons = crate::toolbars::TOOLBAR_BUTTONS
            .iter()
            .find(|(name, _)| name == bar)
            .map(|(_, b)| *b)
            .unwrap();
        for (action, _) in buttons {
            if d.has(action) {
                let top = d.rect(action).top();
                if !tops.iter().any(|t| (t - top).abs() < 0.5) {
                    tops.push(top);
                }
            }
        }
    }
    tops.sort_by(f32::total_cmp);
    tops
}

/// View > Toolbars > `bar`, through the menu.
fn toggle_toolbar(d: &mut Driver, bar: &str) {
    d.home();
    d.open_menu("menu_bar_view");
    assert!(d.hover_text("Toolbars"), "View has Toolbars");
    d.click(&format!("menu_toolbar_{bar}"));
    d.frames(2);
}

#[test]
fn workflow_dialogs_put_help_first_in_a_row_and_captions_bold() {
    let mut d = fresh("qa_dialog_look");

    // Edit > User Preferences: Help, then Close, at the bottom right.
    d.home();
    d.click("menu_bar_edit");
    d.click("menu_user_preferences");
    assert_eq!(
        row_order(&d, &["prefs_close", "prefs_help"]),
        ["prefs_help", "prefs_close"],
        "Help first in the User Preferences row"
    );
    let gap = d.rect("prefs_close").left() - d.rect("prefs_help").right();
    assert_eq!(gap, crate::dialog_kit::ROW_GAP, "Help sits next to Close");
    assert!(painted_bold(&mut d, "Units Format"), "section caption bold");
    d.click("prefs_close");
    d.frames(2);

    // Admin > Admin Preferences: the same row, bold captions.
    d.home();
    d.click("menu_bar_admin");
    d.click("menu_admin_preferences");
    assert_eq!(
        row_order(&d, &["admin_close", "admin_help"]),
        ["admin_help", "admin_close"],
        "Help first in the Admin Preferences row"
    );
    assert!(
        painted_bold(&mut d, "Code Separator"),
        "section caption bold"
    );
    d.click("admin_close");
    d.frames(2);

    // View > Group and Sort by > Customize...: a right-hand column keeps
    // Help last, and its section captions are bold too.
    show(&mut d, "activities");
    d.home();
    d.open_menu("menu_bar_view");
    assert!(
        d.hover_text("Group and Sort by"),
        "View has Group and Sort by"
    );
    d.click("view_group_sort");
    d.frames(2);
    let help = d.rect("group_help");
    for key in ["group_ok", "group_cancel", "group_apply", "group_default"] {
        let r = d.rect(key);
        assert_eq!(r.left(), help.left(), "{key} in the column");
        assert!(r.top() < help.top(), "{key} above Help");
    }
    assert!(
        painted_bold(&mut d, "Display Options"),
        "section caption bold"
    );
    d.click("group_cancel");
    d.frames(2);
    d.assert_clean("dialog look");
}

#[test]
fn workflow_a_first_start_has_one_toolbar_row_and_a_stored_second_row_stays() {
    crate::defaults::use_reference();
    let mut d = fresh("qa_one_toolbar_row");
    let rows = toolbar_rows(&d);
    assert_eq!(rows.len(), 1, "one toolbar row on a first start: {rows:?}");
    for action in [
        "tool_print",
        "tool_table",
        "details",
        "tool_bars",
        "schedule",
        "zoom_in",
    ] {
        assert!(d.has(action), "{action} on the one row");
    }
    assert!(!d.has("tool_resource_codes"), "Dictionaries starts off");

    // Switched on from View > Toolbars it takes a second row, and that
    // arrangement is kept across a restart.
    toggle_toolbar(&mut d, "Dictionaries");
    assert_eq!(toolbar_rows(&d).len(), 2, "Dictionaries on a second row");
    d.app.save_views();
    restart(&mut d);
    d.frames(2);
    assert_eq!(toolbar_rows(&d).len(), 2, "the stored second row stays");

    // A workspace stored before this default (every toolbar on, the old
    // two-row layout) loads exactly as stored.
    let stored = serde_json::json!({
        "hidden_toolbars": [],
        "shown_toolbars": [],
        "bar_layout": {
            "top": [
                ["Menu Bar"],
                crate::menu::TOP_ROW_BARS,
                ["Dictionaries"],
            ],
            "bottom": [],
            "left": [["Standard", "Enterprise", "Project"]],
            "right": [["Edit", "Assign", "Move"]],
            "floating": {},
        },
    });
    toggle_toolbar(&mut d, "Dictionaries");
    assert_eq!(toolbar_rows(&d).len(), 1, "switched off again");
    restart(&mut d);
    assert_eq!(toolbar_rows(&d).len(), 1, "kept off");
    d.app
        .store
        .set_preference("views_layout_v1", &stored.to_string())
        .unwrap();
    // Start on that workspace without saving the running app's views over
    // the stored record first.
    let store = Store::open(d.app.store.path.clone()).unwrap();
    let project = store.last().unwrap().expect("a saved project");
    let mut app = App::new(&d.ctx, store, project);
    app.ai_agents.env = Some(crate::ai_agents::temp_profile(&d.dir));
    app.sched.check_report_path = d.dir.join("check.html").display().to_string();
    app.sched.log_path = d.dir.join("SCHEDLOG.TXT").display().to_string();
    d.app = app;
    d.frames(3);
    assert_eq!(
        toolbar_rows(&d).len(),
        2,
        "a stored two-row arrangement loads as two rows"
    );
    assert!(d.has("tool_resource_codes"), "Dictionaries as stored");
    d.assert_clean("toolbar rows");
}

/// The Dictionaries entry of the open toolbar popup, as drawn.
fn popup_dictionaries(d: &Driver) -> crate::context_menu::Drawn {
    crate::context_menu::drawn(&d.ctx)
        .into_iter()
        .find(|e| e.prefix == "toolbar_popup" && e.label == "Dictionaries")
        .expect("the toolbar popup lists Dictionaries")
}

#[test]
fn workflow_the_toolbar_popup_shows_dictionaries_off_and_ticking_it_adds_the_row() {
    crate::defaults::use_reference();
    let mut d = fresh("qa_toolbar_popup_dictionaries");
    assert_eq!(
        toolbar_rows(&d).len(),
        1,
        "one toolbar row on a first start"
    );

    // A right-click on a toolbar opens the popup with Dictionaries unticked,
    // the same state View > Toolbars has.
    let button = d.rect("tool_print").center();
    d.press_at(button, egui::PointerButton::Secondary, Default::default());
    d.frames(2);
    assert!(
        !popup_dictionaries(&d).checked,
        "Dictionaries unticked on a fresh setup"
    );
    assert!(
        crate::context_menu::drawn(&d.ctx)
            .iter()
            .any(|e| e.prefix == "toolbar_popup" && e.label == "Print" && e.checked),
        "a shown toolbar is ticked"
    );

    // Ticking it brings the second row back.
    d.click("toolbar_popup_toolbar_Dictionaries");
    d.frames(2);
    assert_eq!(toolbar_rows(&d).len(), 2, "Dictionaries on a second row");
    assert!(d.has("tool_resource_codes"), "Dictionaries buttons drawn");

    // The popup now shows it ticked.
    let button = d.rect("tool_print").center();
    d.press_at(button, egui::PointerButton::Secondary, Default::default());
    d.frames(2);
    assert!(popup_dictionaries(&d).checked, "Dictionaries ticked");
    d.key(egui::Key::Escape);
    d.frames(2);
    d.assert_clean("toolbar popup");
}
