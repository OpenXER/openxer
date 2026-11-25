//! Resource Assignments layouts through the production UI: Layout > Save
//! As..., Save and Open... on the spreadsheet's right-click menu, with the
//! Activities layout windows (name check, save-changes question, Delete),
//! kept in the workspace preferences.
use super::assignment_layouts::{library_from_json, library_to_json, STANDARD};
use super::*;
use crate::qa_sweep::Driver;
use egui::{Key, Pos2};

fn secondary(d: &mut Driver, pos: Pos2) {
    d.press_at(pos, egui::PointerButton::Secondary, Default::default());
}

fn sheet_point(d: &Driver) -> Pos2 {
    let header = d.rect("assignments_col_0");
    Pos2::new(header.center().x, header.bottom() + grid::ROW_HEIGHT * 1.5)
}

/// Right-click the sheet and run Layout > `item`.
fn layout_item(d: &mut Driver, item: &str) {
    let at = sheet_point(d);
    secondary(d, at);
    let hover = d.rect("assignments_menu_layout").center();
    let _ = d.frame(vec![egui::Event::PointerMoved(hover)]);
    d.frames(3);
    let key = format!("assignments_menu_layout_{item}");
    assert!(d.has(&key), "{key} is listed");
    d.click_at(d.rect(&key).center());
    d.frames(2);
}

fn open_view() -> Driver {
    let mut d = Driver::new("asg-layouts", resources::tests::with_resource());
    d.home();
    d.app.show_view(MainView::Assignments);
    d.frames(4);
    d
}

#[test]
fn assignment_layouts_save_as_save_and_open_like_activity_layouts() {
    let mut d = open_view();
    assert_eq!(d.app.views.assignments.layout_name, STANDARD);
    assert!(d
        .texts()
        .iter()
        .any(|(t, _)| t == &format!("Layout: {STANDARD}")));
    // Save As... with a new name keeps the current settings under it.
    d.app.views.assignments.zoom = 140.;
    d.app.views.assignments.show_all_projects = true;
    layout_item(&mut d, "save_as");
    assert!(d.app.grid.dialogs.save_as.is_some(), "Save Layout As opens");
    d.click("save_as_name");
    d.key_with(
        Key::A,
        egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        },
    );
    d.type_text("Crew Lookahead");
    d.click("save_as_ok");
    d.frames(2);
    assert!(d.app.grid.dialogs.save_as.is_none(), "{:?}", d.app.error);
    assert_eq!(d.app.views.assignments.layout_name, "Crew Lookahead");
    assert!(d.texts().iter().any(|(t, _)| t == "Layout: Crew Lookahead"));
    let saved = d.app.assignment_layouts();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].view["zoom"], 140.);
    // Save writes the changed settings under the same name.
    d.app.views.assignments.show_details = false;
    layout_item(&mut d, "save");
    let saved = d.app.assignment_layouts();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].view["show_details"], false);
    assert!(!d.app.assignment_layout_changed());
    // Open... lists the view's own layout and the saved one; opening the
    // standard layout puts the defaults back.
    layout_item(&mut d, "open");
    assert!(
        d.app.grid.dialogs.open_layout.is_some(),
        "Open Layout opens"
    );
    assert!(d
        .texts()
        .iter()
        .any(|(t, _)| t == "Resource Assignment Layouts"));
    assert!(
        !d.has("open_layout_import"),
        "layout files are Activities only"
    );
    assert!(!d.has("open_layout_use_filter"));
    d.click("open_layout_item_0");
    d.click("open_layout_open");
    d.frames(2);
    assert_eq!(d.app.views.assignments.layout_name, STANDARD);
    assert_eq!(d.app.views.assignments.zoom, 100.);
    assert!(d.app.views.assignments.show_details);
    assert!(!d.app.views.assignments.show_all_projects);
    // A changed view asks whether to save first; No opens without saving.
    d.app.views.assignments.zoom = 70.;
    layout_item(&mut d, "open");
    d.click("open_layout_item_1");
    d.click("open_layout_open");
    d.frames(2);
    assert!(d.app.grid.dialogs.save_prompt.is_some(), "the question");
    d.click("prompt_no");
    d.frames(2);
    assert_eq!(d.app.views.assignments.layout_name, "Crew Lookahead");
    assert_eq!(d.app.views.assignments.zoom, 140.);
    assert!(!d.app.views.assignments.show_details);
    assert!(d.app.views.assignments.show_all_projects);
    assert_eq!(d.app.assignment_layouts().len(), 1, "No saved nothing");
    // Escape and Cancel close the windows and change nothing.
    layout_item(&mut d, "open");
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.grid.dialogs.open_layout.is_none());
    layout_item(&mut d, "save_as");
    d.click("save_as_cancel");
    d.frames(2);
    assert!(d.app.grid.dialogs.save_as.is_none());
    // The layout name is kept with the view settings.
    let json = d.app.views.json();
    let mut back = Views::default();
    back.restore(&json);
    assert_eq!(back.assignments.layout_name, "Crew Lookahead");
    // Delete in Open Layout removes a saved layout.
    layout_item(&mut d, "open");
    d.click("open_layout_item_1");
    d.click("open_layout_delete");
    d.frames(2);
    // It asks first; Yes deletes.
    assert!(d.app.grid.dialogs.remove_prompt.is_some(), "asks first");
    d.click("remove_layout_yes");
    d.frames(2);
    assert!(d.app.assignment_layouts().is_empty());
    d.key(Key::Escape);
    d.frames(2);
    // Activities layouts were not touched.
    assert_eq!(
        d.app.grid.dialogs.layout_target,
        crate::grid::dialogs::LayoutTarget::Assignments
    );
    d.app.open_layout_dialog();
    d.frames(2);
    assert!(d.texts().iter().any(|(t, _)| t == "Activity Layouts"));
    assert!(d.has("open_layout_import"));
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn the_stored_library_drops_bad_entries() {
    let text = serde_json::json!([
        {"name": "Good", "view": {"zoom": 140.0}},
        {"name": "", "view": {}},
        {"name": "No view"},
        {"name": "good", "view": {}},
        {"name": "Other", "view": 3},
    ])
    .to_string();
    let list = library_from_json(&text);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "Good");
    assert_eq!(library_from_json(&library_to_json(&list)), list);
    assert!(library_from_json("not json").is_empty());
}

#[test]
fn the_general_tab_fits_price_rate_type_and_primary_on_a_narrow_screen() {
    // At 1280 points wide the Primary Resource check box used to land on
    // the Rate Type combo.
    let mut d = Driver::probed(
        "asg-general-narrow",
        resources::tests::with_resource(),
        egui::Vec2::new(1280., 800.),
        None,
    );
    d.home();
    d.app.show_view(MainView::Assignments);
    d.frames(2);
    let a = assignments::assignments(&d.app.project)[0].id;
    d.app.views.assignments.selected = Some(assignments::Key::Assignment(a));
    d.app.views.assignments.tab = 0;
    d.app.views.assignments.show_details = true;
    d.frames(3);
    let (price, rate, primary) = (
        d.rect("assignment_price"),
        d.rect("assignment_rate_type"),
        d.rect("assignment_primary"),
    );
    assert!(price.right() <= rate.left(), "{price:?} then {rate:?}");
    assert!(rate.right() <= primary.left(), "{rate:?} then {primary:?}");
    assert!(d.app.error.is_none());
}
