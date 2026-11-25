//! The movable bars through the production UI: egui `RawInput` only, no
//! native window and no system pointer. Each test drags real grips and
//! titles with synthetic pointer events.
use super::*;
use crate::dockbars::*;
use crate::qa_sweep::Driver;

fn driver(name: &str) -> Driver {
    let mut project = openxer_xer::starter();
    schedule_model(&mut project.model, &project.resources).unwrap();
    Driver::new(name, project)
}

fn layout(d: &Driver) -> &BarLayout {
    &d.app.views.bars.layout
}

fn ctrl() -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    }
}

fn button(pos: Pos2, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    }
}

/// Press at `points[0]`, carry through the rest in steps of at most 16 px
/// and release at the last one.
fn drag(d: &mut Driver, points: &[Pos2], modifiers: egui::Modifiers) {
    d.frames(2);
    let first = points[0];
    d.frame_with(vec![egui::Event::PointerMoved(first)], modifiers);
    d.frame_with(vec![button(first, true, modifiers)], modifiers);
    let mut at = first;
    for target in &points[1..] {
        let steps = ((*target - at).length() / 16.).ceil().max(1.) as usize;
        for step in 1..=steps {
            let p = at + (*target - at) * (step as f32 / steps as f32);
            d.frame_with(vec![egui::Event::PointerMoved(p)], modifiers);
        }
        at = *target;
    }
    d.frame_with(vec![], modifiers);
    d.frame_with(vec![button(at, false, modifiers)], modifiers);
    d.frame(vec![egui::Event::PointerGone]);
    d.frames(3);
}

/// Carry the bar `name` by its grip (or floating title) to `to`.
fn carry(d: &mut Driver, name: &str, to: Pos2, modifiers: egui::Modifiers) {
    let from = grab_point(d, name);
    drag(d, &[from, to], modifiers);
}

/// Where a bar is picked up: its title when floating, else its grip.
fn grab_point(d: &Driver, name: &str) -> Pos2 {
    let title = format!("float_title_{name}");
    if d.has(&title) {
        let r = d.rect(&title);
        return Pos2::new(r.left() + 20., r.center().y);
    }
    d.rect(&format!("bar_grip_{name}")).center()
}

fn slot(side: Side, row: usize, index: usize) -> Slot {
    Slot { side, row, index }
}

#[test]
fn the_default_layout_draws_where_it_always_did() {
    let mut d = driver("bars-default");
    d.frames(3);
    let menu = d.rect("menu_bar_file");
    assert_eq!(menu.top(), 2.);
    assert!(d.rect("tool_print").top() >= menu::MENU_ROW);
    assert_eq!(d.rect("bar_grip_Menu Bar").left(), 0.);
    // Rows: menu 25, toolbar rows 28 each; the second row is hidden bars
    // but Dictionaries.
    let dock = d.app.views.bars.rt.geo.dock[Side::Top.index()].unwrap();
    assert_eq!(dock.height(), menu::MENU_ROW + 2. * menu::TOOLBAR_ROW);
    let left = d.app.views.bars.rt.geo.dock[Side::Left.index()].unwrap();
    let right = d.app.views.bars.rt.geo.dock[Side::Right.index()].unwrap();
    assert_eq!(left.width(), 28.);
    assert_eq!(right.width(), 28.);
    assert!(d.app.views.bars.rt.geo.dock[Side::Bottom.index()].is_none());
    assert_eq!(layout(&d), &BarLayout::default());
}

#[test]
fn a_move_cursor_shows_over_a_grip_and_an_outline_follows_the_pointer() {
    let mut d = driver("bars-outline");
    d.frames(2);
    let grip = d.rect("bar_grip_Layout").center();
    let out = d.frame(vec![egui::Event::PointerMoved(grip)]);
    assert_eq!(out.platform_output.cursor_icon, egui::CursorIcon::Move);
    // Away from any grip the cursor is the plain arrow.
    let out = d.frame(vec![egui::Event::PointerMoved(Pos2::new(700., 500.))]);
    assert_ne!(out.platform_output.cursor_icon, egui::CursorIcon::Move);
    // Pick the bar up and carry it into the middle of the window.
    d.frame(vec![egui::Event::PointerMoved(grip)]);
    d.frame(vec![button(grip, true, Default::default())]);
    for x in [grip.x + 20., grip.x + 60., 400., 700.] {
        d.frame(vec![egui::Event::PointerMoved(Pos2::new(x, 500.))]);
    }
    let out = d.frame(vec![egui::Event::PointerMoved(Pos2::new(700., 520.))]);
    let drag = d.app.views.bars.rt.drag.clone().expect("a drag is going");
    assert_eq!(drag.name, "Layout");
    let outline = drag.outline.expect("an outline");
    assert!(matches!(drag.target, Some(Target::Float(_))));
    assert!(outline.contains(Pos2::new(700., 520.)), "{outline:?}");
    assert_eq!(out.platform_output.cursor_icon, egui::CursorIcon::Move);
    // The outline is a gray frame in the output.
    let gray = out.shapes.iter().any(|clipped| {
        matches!(&clipped.shape,
            egui::Shape::Rect(r) if r.stroke.color == Color32::from_rgb(0x80, 0x80, 0x80)
                && r.stroke.width >= 3.)
    });
    assert!(gray, "the outline is drawn");
    // Nothing has moved yet.
    assert_eq!(layout(&d), &BarLayout::default());
    d.frame(vec![button(
        Pos2::new(700., 520.),
        false,
        Default::default(),
    )]);
    d.frames(3);
    assert!(d.app.views.bars.rt.drag.is_none());
    assert!(layout(&d).is_floating("Layout"));
}

#[test]
fn a_bar_reorders_in_its_row_and_moves_to_another_row_or_a_new_row() {
    let mut d = driver("bars-reorder");
    // Print to just before Tools (Print, Top Layout, Bottom Layout, Layout,
    // Tools, Display).
    let tools = d.rect("bar_grip_Tools").center();
    carry(&mut d, "Print", tools, Default::default());
    let row = &layout(&d).rows(Side::Top)[1];
    assert_eq!(
        row[..5],
        ["Top Layout", "Bottom Layout", "Layout", "Print", "Tools"]
    );
    let xs: Vec<f32> = ["Layout", "Print", "Tools"]
        .iter()
        .map(|b| d.rect(&format!("bar_grip_{b}")).left())
        .collect();
    assert!(
        xs[0] < xs[1] && xs[1] < xs[2],
        "drawn in that order: {xs:?}"
    );
    // Display to the second row, after Dictionaries.
    let dictionaries = d.rect("bar_grip_Dictionaries");
    carry(
        &mut d,
        "Display",
        Pos2::new(900., dictionaries.center().y),
        Default::default(),
    );
    assert_eq!(layout(&d).slot_of("Display"), Some(slot(Side::Top, 2, 1)));
    assert!(d.rect("bar_grip_Display").top() > d.rect("bar_grip_Layout").top());
    // Layout to a new row between the first and second toolbar rows.
    let boundary = d.rect("bar_grip_Dictionaries").top() - 3.;
    carry(
        &mut d,
        "Layout",
        Pos2::new(300., boundary),
        Default::default(),
    );
    assert_eq!(layout(&d).rows(Side::Top).len(), 4);
    assert_eq!(layout(&d).rows(Side::Top)[2], vec!["Layout"]);
    // Four rows now: menu, toolbars, Layout alone, Dictionaries and Display.
    let dock = d.app.views.bars.rt.geo.dock[Side::Top.index()].unwrap();
    assert_eq!(dock.height(), menu::MENU_ROW + 3. * menu::TOOLBAR_ROW);
    // Buttons of each bar sit in their rows.
    assert!(d.rect("tool_bars").top() > d.rect("tool_print").bottom());
    assert!(d.rect("tool_resource_codes").top() > d.rect("tool_bars").bottom());
    d.assert_clean("reorder bars");
}

#[test]
fn a_bar_floats_moves_and_docks_left_and_bottom() {
    let mut d = driver("bars-float");
    let layout_bar_buttons = ["tool_bars", "tool_columns", "timescale"];
    // Pull Layout off the dock into the middle of the window.
    carry(&mut d, "Layout", Pos2::new(700., 420.), Default::default());
    assert!(layout(&d).is_floating("Layout"));
    assert!(d.has("float_title_Layout") && d.has("float_close_Layout"));
    let window = d.app.views.bars.rt.geo.floats["Layout"];
    assert!(window.contains(Pos2::new(700., 420.)));
    // Its buttons sit in one row under the title.
    let title = d.rect("float_title_Layout");
    let bars = d.rect("tool_bars");
    let columns = d.rect("tool_columns");
    assert!(bars.top() >= title.bottom() && bars.center().y == columns.center().y);
    assert!(columns.left() > bars.left());
    // The top rows lost the bar.
    assert_eq!(
        layout(&d).rows(Side::Top)[1],
        ["Print", "Top Layout", "Bottom Layout", "Tools", "Display"]
    );
    // Move the window.
    carry(&mut d, "Layout", Pos2::new(900., 300.), Default::default());
    let moved = d.app.views.bars.rt.geo.floats["Layout"];
    assert!(moved.left() > window.left() + 100. && moved.top() < window.top() - 50.);
    // It cannot be carried off the window.
    carry(
        &mut d,
        "Layout",
        Pos2::new(-400., -400.),
        Default::default(),
    );
    let corner = d.app.views.bars.rt.geo.floats["Layout"];
    assert!(corner.left() >= 0. && corner.top() >= 0., "{corner:?}");
    carry(
        &mut d,
        "Layout",
        Pos2::new(5000., 5000.),
        Default::default(),
    );
    let corner = d.app.views.bars.rt.geo.floats["Layout"];
    assert!(
        corner.right() <= 1500. && corner.bottom() <= 900.,
        "{corner:?}"
    );
    // Dock it on the left: the buttons stack.
    carry(&mut d, "Layout", Pos2::new(14., 400.), Default::default());
    assert!(!layout(&d).is_floating("Layout"));
    let slot_left = layout(&d).slot_of("Layout").unwrap();
    assert_eq!(slot_left.side, Side::Left);
    let rects: Vec<Rect> = layout_bar_buttons.iter().map(|b| d.rect(b)).collect();
    assert!(rects[0].left() == rects[1].left() && rects[1].left() == rects[2].left());
    assert!(rects[0].bottom() <= rects[1].top() + 1. && rects[1].bottom() <= rects[2].top() + 1.);
    assert!(rects[0].right() <= 38.);
    // The bar leaves the left dock and goes to the bottom.
    carry(&mut d, "Layout", Pos2::new(700., 876.), Default::default());
    assert_eq!(layout(&d).slot_of("Layout").unwrap().side, Side::Bottom);
    let rects: Vec<Rect> = layout_bar_buttons.iter().map(|b| d.rect(b)).collect();
    assert!(rects[0].center().y == rects[1].center().y && rects[1].left() > rects[0].left());
    assert!(rects[0].bottom() <= 882., "above the status bar: {rects:?}");
    let dock = d.app.views.bars.rt.geo.dock[Side::Bottom.index()].unwrap();
    assert_eq!(dock.bottom(), 900. - 18.);
    // Pull it off again and double-click its grip to float it in place.
    d.frames(2);
    let grip = d.rect("bar_grip_Layout").center();
    d.double_click_at(grip);
    assert!(layout(&d).is_floating("Layout"));
    // Double-clicking the title returns it to the bottom dock.
    let title = grab_point(&d, "Layout");
    d.double_click_at(title);
    assert!(!layout(&d).is_floating("Layout"));
    assert_eq!(layout(&d).slot_of("Layout").unwrap().side, Side::Bottom);
    d.assert_clean("float and dock");
}

#[test]
fn double_clicking_a_title_returns_a_bar_to_its_last_place() {
    let mut d = driver("bars-redock");
    carry(&mut d, "Tools", Pos2::new(600., 500.), Default::default());
    assert!(layout(&d).is_floating("Tools"));
    let title = grab_point(&d, "Tools");
    d.double_click_at(title);
    assert_eq!(layout(&d), &BarLayout::default());
    // Double-clicking a docked grip floats the bar where it stands.
    let grip = d.rect("bar_grip_Display");
    d.double_click_at(grip.center());
    assert!(layout(&d).is_floating("Display"));
    let window = d.app.views.bars.rt.geo.floats["Display"];
    assert!(
        (window.left() - grip.left()).abs() < 1.5,
        "{window:?} {grip:?}"
    );
    d.assert_clean("redock");
}

#[test]
fn ctrl_keeps_a_carried_bar_from_docking() {
    let mut d = driver("bars-ctrl");
    carry(&mut d, "Tools", Pos2::new(600., 500.), Default::default());
    // Ctrl over the left dock: it stays a floating window.
    carry(&mut d, "Tools", Pos2::new(14., 400.), ctrl());
    assert!(layout(&d).is_floating("Tools"));
    assert!(d.app.views.bars.rt.geo.floats["Tools"].left() < 40.);
    // Without it the same drop docks.
    carry(&mut d, "Tools", Pos2::new(14., 400.), Default::default());
    assert_eq!(layout(&d).slot_of("Tools").unwrap().side, Side::Left);
    // Ctrl on a docked grip pulls the bar out even over its own row.
    let near = d.rect("bar_grip_Tools").center() + Vec2::new(30., 0.);
    carry(&mut d, "Tools", near, ctrl());
    assert!(layout(&d).is_floating("Tools"));
}

#[test]
fn the_menu_bar_floats_docks_left_and_runs_commands_from_both() {
    let mut d = driver("bars-menu");
    carry(
        &mut d,
        "Menu Bar",
        Pos2::new(600., 420.),
        Default::default(),
    );
    assert!(layout(&d).is_floating("Menu Bar"));
    assert!(d.has("float_title_Menu Bar"));
    assert!(
        !d.has("float_close_Menu Bar"),
        "the menu bar cannot be closed"
    );
    // The rows below moved up: the menu row is gone.
    assert!(d.rect("tool_print").top() < 30.);
    // A menu opens and runs a command from the floating window.
    d.click("menu_bar_help");
    d.click("menu_about");
    assert!(d.app.show_about);
    d.key(Key::Escape);
    d.frames(2);
    assert!(!d.app.show_about);
    // Dock it on the left: titles stack, dropdowns open to the right.
    carry(&mut d, "Menu Bar", Pos2::new(14., 400.), Default::default());
    assert_eq!(layout(&d).slot_of("Menu Bar").unwrap().side, Side::Left);
    let file = d.rect("menu_bar_file");
    let edit = d.rect("menu_bar_edit");
    assert!(edit.top() >= file.bottom() - 0.5 && edit.left() == file.left());
    d.click("menu_bar_help");
    let help = d.rect("menu_bar_help");
    let about = d.rect("menu_about");
    assert!(
        about.left() >= help.right() - 1.,
        "{about:?} opens right of {help:?}"
    );
    d.click("menu_about");
    assert!(d.app.show_about);
    d.key(Key::Escape);
    d.frames(2);
    // And back to the top, in its own row.
    carry(&mut d, "Menu Bar", Pos2::new(700., 4.), Default::default());
    assert_eq!(layout(&d).slot_of("Menu Bar").unwrap().side, Side::Top);
    d.click("menu_bar_help");
    assert!(d.rect("menu_about").top() > d.rect("menu_bar_help").bottom() - 1.);
    d.key(Key::Escape);
    // A bar on the right opens its menus to the left.
    carry(
        &mut d,
        "Menu Bar",
        Pos2::new(1486., 400.),
        Default::default(),
    );
    assert_eq!(layout(&d).slot_of("Menu Bar").unwrap().side, Side::Right);
    d.click("menu_bar_help");
    assert!(d.rect("menu_about").right() <= d.rect("menu_bar_help").left() + 1.);
    d.key(Key::Escape);
    d.assert_clean("menu bar");
}

#[test]
fn closing_a_floating_bar_hides_it_and_view_toolbars_shows_it_in_place() {
    let mut d = driver("bars-close");
    carry(&mut d, "Layout", Pos2::new(700., 420.), Default::default());
    let at = d.app.views.bars.rt.geo.floats["Layout"];
    d.click("float_close_Layout");
    assert!(!d.app.toolbar_visible("Layout"));
    assert!(!d.has("float_title_Layout") && !d.has("tool_bars"));
    assert!(layout(&d).is_floating("Layout"), "its place is kept");
    d.click("menu_bar_view");
    assert!(d.hover_text("Toolbars"));
    d.click("menu_toolbar_Layout");
    d.frames(2);
    assert!(d.app.toolbar_visible("Layout"));
    assert_eq!(d.app.views.bars.rt.geo.floats["Layout"], at);
    // The toolbar popup of the dock shows the same state.
    d.assert_clean("close floating bar");
}

#[test]
fn a_floating_bar_wraps_when_its_edge_is_dragged_in() {
    let mut d = driver("bars-wrap");
    carry(&mut d, "Tools", Pos2::new(600., 400.), Default::default());
    let one_row = d.app.views.bars.rt.geo.floats["Tools"];
    let handle = d.rect("float_resize_Tools");
    let from = handle.center();
    drag(
        &mut d,
        &[from, from - Vec2::new(90., 0.)],
        Default::default(),
    );
    let narrow = d.app.views.bars.rt.geo.floats["Tools"];
    assert!(
        narrow.width() < one_row.width() - 60.,
        "{narrow:?} {one_row:?}"
    );
    assert!(
        narrow.height() > one_row.height() + 10.,
        "wrapped to more rows"
    );
    // Every button is still inside the window.
    for action in ["schedule", "tool_auto_reorganize"] {
        let r = d.rect(action);
        assert!(narrow.contains(r.center()), "{action} {r:?} {narrow:?}");
    }
    // The width is kept with the layout.
    assert!(layout(&d).floating["Tools"].width < 200.);
}

#[test]
fn locked_bars_have_no_grips_and_cannot_be_moved() {
    let mut d = driver("bars-lock");
    carry(&mut d, "Tools", Pos2::new(600., 400.), Default::default());
    let title = grab_point(&d, "Tools");
    d.app.views.toolbars_locked = true;
    d.frames(3);
    assert!(!d.has("bar_grip_Print") && !d.has("bar_grip_Menu Bar"));
    assert!(!d.has("float_close_Tools") && !d.has("float_resize_Tools"));
    let before = layout(&d).clone();
    drag(&mut d, &[title, Pos2::new(300., 200.)], Default::default());
    d.double_click_at(title);
    assert_eq!(layout(&d), &before);
    // Buttons still work.
    assert!(d.has("schedule"));
    d.app.views.toolbars_locked = false;
    d.frames(3);
    assert!(d.has("bar_grip_Print"));
}

#[test]
fn escape_while_carrying_puts_the_bar_back() {
    let mut d = driver("bars-escape");
    let grip = d.rect("bar_grip_Layout").center();
    d.frame(vec![egui::Event::PointerMoved(grip)]);
    d.frame(vec![button(grip, true, Default::default())]);
    for y in [100., 200., 300.] {
        d.frame(vec![egui::Event::PointerMoved(Pos2::new(700., y))]);
    }
    assert!(d.app.views.bars.rt.drag.is_some());
    d.key(Key::Escape);
    d.frame(vec![button(
        Pos2::new(700., 300.),
        false,
        Default::default(),
    )]);
    d.frames(3);
    assert!(d.app.views.bars.rt.drag.is_none());
    assert_eq!(layout(&d), &BarLayout::default());
}

#[test]
fn the_view_area_gives_way_to_bars_docked_on_any_edge() {
    let mut d = driver("bars-room");
    d.click("directory_activities");
    d.frames(3);
    let view = |d: &Driver| {
        // The caption's close mark is the view's top-right corner and the
        // details strip its bottom-left.
        let close = d.rect("view_close");
        let tab = d.rect("detail_tab_general");
        (close.right(), tab.left(), tab.bottom())
    };
    let (right0, left0, bottom0) = view(&d);
    // A bar on the left: the view starts after it.
    carry(&mut d, "Layout", Pos2::new(14., 400.), Default::default());
    let (right1, left1, bottom1) = view(&d);
    let left_dock = d.app.views.bars.rt.geo.dock[Side::Left.index()].unwrap();
    assert!(left1 >= left_dock.right(), "{left1} vs {left_dock:?}");
    assert!(left1 >= left0, "no overlap on the left");
    assert_eq!(right1, right0);
    assert_eq!(bottom1, bottom0);
    // A bar on the right.
    carry(&mut d, "Tools", Pos2::new(1486., 400.), Default::default());
    let right_dock = d.app.views.bars.rt.geo.dock[Side::Right.index()].unwrap();
    let (right2, _, _) = view(&d);
    assert!(
        right2 <= right_dock.left() + 1.,
        "{right2} vs {right_dock:?}"
    );
    // A new column on the right makes the dock wider and the view narrower.
    carry(
        &mut d,
        "Display",
        Pos2::new(1495., 400.),
        Default::default(),
    );
    let wider = d.app.views.bars.rt.geo.dock[Side::Right.index()].unwrap();
    assert!(wider.width() > right_dock.width());
    // A bar at the bottom: the view ends above it and the status bar stays
    // at the very bottom.
    carry(&mut d, "Print", Pos2::new(700., 876.), Default::default());
    let bottom_dock = d.app.views.bars.rt.geo.dock[Side::Bottom.index()].unwrap();
    let (_, _, bottom3) = view(&d);
    assert!(
        bottom3 <= bottom_dock.top() + 1.,
        "{bottom3} vs {bottom_dock:?}"
    );
    assert_eq!(bottom_dock.bottom(), 900. - 18.);
    d.assert_clean("room for bars");
}

#[test]
fn directory_and_edit_bars_work_from_any_edge() {
    let mut d = driver("bars-directory");
    // The Enterprise bar to the top row, where it lays out horizontally.
    carry(
        &mut d,
        "Enterprise",
        Pos2::new(900., 40.),
        Default::default(),
    );
    assert_eq!(layout(&d).slot_of("Enterprise").unwrap().side, Side::Top);
    let wbs = d.rect("directory_projects");
    let resources = d.rect("directory_resources");
    assert!(resources.left() > wbs.right() - 1. && wbs.center().y == resources.center().y);
    d.click("directory_resources");
    assert_eq!(d.app.views.main, views::MainView::Resources);
    d.click("directory_projects");
    assert_eq!(d.app.views.main, views::MainView::Projects);
    // The Edit bar floats and still acts.
    carry(&mut d, "Edit", Pos2::new(700., 450.), Default::default());
    d.click("directory_activities");
    assert!(d.has("add") && d.rect("add").top() > 200.);
    d.assert_clean("directory bars");
}

#[test]
fn the_layout_is_kept_in_the_workspace_and_reset_puts_everything_back() {
    let mut d = driver("bars-persist");
    carry(&mut d, "Layout", Pos2::new(700., 420.), Default::default());
    carry(&mut d, "Print", Pos2::new(14., 400.), Default::default());
    carry(
        &mut d,
        "Dictionaries",
        Pos2::new(700., 876.),
        Default::default(),
    );
    let saved = layout(&d).clone();
    assert_ne!(saved, BarLayout::default());
    d.app.save_views();
    let stored = d.app.store.preference("views_layout_v1").unwrap().unwrap();
    let mut restored = views::Views::default();
    restored.restore(&stored);
    assert_eq!(restored.bars.layout, saved);
    // A new session starts from it.
    d.app.views = restored;
    d.frames(3);
    assert!(d.has("float_title_Layout"));
    assert_eq!(layout(&d), &saved);
    // View > Toolbars > Reset Toolbars puts everything back.
    d.click("menu_bar_view");
    assert!(d.hover_text("Toolbars"));
    d.click("menu_reset_toolbar_layout");
    d.frames(2);
    assert_eq!(layout(&d), &BarLayout::default());
    assert!(!d.has("float_title_Layout"));
    // So does Reset All Toolbars.
    carry(&mut d, "Layout", Pos2::new(700., 420.), Default::default());
    d.click("menu_bar_view");
    d.click("menu_reset_toolbars");
    d.frames(2);
    assert_eq!(layout(&d), &BarLayout::default());
}

#[test]
fn garbage_in_the_saved_layout_is_ignored() {
    for bar_layout in [
        serde_json::json!("junk"),
        serde_json::json!(12),
        serde_json::json!([[], {}]),
        serde_json::json!({"top": [["Nope"]], "floating": {"Layout": {"x": 1e30, "y": 0}}}),
        serde_json::json!({"top": [[]], "bottom": 4, "left": [[7, 8]], "right": null}),
        serde_json::json!({"floating": {"Tools": {"x": -9000.0, "y": 99999.0, "w": 5.0}}}),
    ] {
        let json = serde_json::json!({ "bar_layout": bar_layout }).to_string();
        let mut views = views::Views::default();
        views.restore(&json);
        let names = views.bars.layout.names();
        assert_eq!(names.len(), 20, "every bar once: {names:?}");
        let mut d = driver("bars-garbage");
        d.app.views = views;
        d.frames(4);
        assert!(d.has("menu_bar_file"), "the menu bar is always there");
        d.assert_clean("garbage layout");
    }
    // Invalid JSON as a whole leaves the default.
    let mut views = views::Views::default();
    views.restore("{ not json");
    assert_eq!(views.bars.layout, BarLayout::default());
}

#[test]
fn an_off_screen_saved_position_is_pulled_back_into_the_window() {
    let mut d = driver("bars-offscreen");
    d.app
        .views
        .bars
        .layout
        .float("Tools", Pos2::new(9000., 8000.), 100.);
    d.frames(4);
    let window = d.app.views.bars.rt.geo.floats["Tools"];
    assert!(
        window.right() <= 1500. && window.bottom() <= 900.,
        "{window:?}"
    );
    d.app
        .views
        .bars
        .layout
        .float("Display", Pos2::new(-900., -300.), 100.);
    d.frames(4);
    let window = d.app.views.bars.rt.geo.floats["Display"];
    assert!(window.left() >= 0. && window.top() >= 0., "{window:?}");
}

/// A software-painted app for the review pictures.
struct Shots {
    app: App,
    ctx: egui::Context,
    painter: crate::headless_render::SoftwarePainter,
    time: f64,
}

impl Shots {
    fn run(&mut self, events: Vec<egui::Event>, paint: bool) -> Option<image::RgbaImage> {
        self.painter
            .frame(&mut self.app, &self.ctx, &mut self.time, events, paint)
            .map(|(image, _)| image)
    }

    fn rect(&self, key: &str) -> Rect {
        let [l, t, r, b] = self.app.action_rects[key];
        Rect::from_min_max(Pos2::new(l, t), Pos2::new(r, b))
    }

    /// Carry a bar to `to`; with `hold` the button stays down.
    fn carry(&mut self, name: &str, to: Pos2, hold: bool) {
        let none = egui::Modifiers::default();
        let title = format!("float_title_{name}");
        let from = if self.app.action_rects.contains_key(&title) {
            let r = self.rect(&title);
            Pos2::new(r.left() + 20., r.center().y)
        } else {
            self.rect(&format!("bar_grip_{name}")).center()
        };
        self.run(vec![egui::Event::PointerMoved(from)], false);
        self.run(vec![button(from, true, none)], false);
        let steps = ((to - from).length() / 16.).ceil() as usize;
        for step in 1..=steps {
            let p = from + (to - from) * (step as f32 / steps as f32);
            self.run(vec![egui::Event::PointerMoved(p)], false);
        }
        self.run(vec![], false);
        if !hold {
            self.release(to);
        }
    }

    fn release(&mut self, at: Pos2) {
        self.run(vec![button(at, false, egui::Modifiers::default())], false);
        for _ in 0..3 {
            self.run(vec![], false);
        }
    }

    fn save(&mut self, out: &std::path::Path, name: &str) {
        self.run(vec![], false);
        let image = self.run(vec![], true).unwrap();
        image.save(out.join(name)).unwrap();
    }
}

/// Renders the bars in the states a reviewer wants to look at, through the
/// software painter (no window): a floating toolbar, a left-docked toolbar,
/// a floating menu bar with a menu open, and the drag outline. The pictures
/// go to `artifacts/dockbars/` (not tracked).
#[test]
fn render_the_bars_for_review() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/dockbars");
    std::fs::create_dir_all(&out).unwrap();
    let database = std::env::temp_dir().join(format!(
        "openxer-dockbars-render-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut project = openxer_xer::starter();
    schedule_model(&mut project.model, &project.resources).unwrap();
    let mut store = Store::open(database.clone()).unwrap();
    store.save(&project).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let app = App::new(&ctx, store, project);
    let mut painter = crate::headless_render::SoftwarePainter::default();
    painter.screen = Some(Vec2::new(1300., 700.));
    let mut shots = Shots {
        app,
        ctx,
        painter,
        time: 0.,
    };
    for _ in 0..4 {
        shots.run(vec![], false);
    }
    let none = egui::Modifiers::default();
    // A floating toolbar over the Gantt, another one, and a toolbar docked
    // on the left beside the directory bars.
    shots.carry("Layout", Pos2::new(640., 330.), false);
    shots.carry("Tools", Pos2::new(840., 200.), false);
    shots.carry("Print", Pos2::new(14., 420.), false);
    shots.save(&out, "floating-and-docked.png");
    // The menu bar floating, with the View menu open.
    shots.carry("Menu Bar", Pos2::new(420., 120.), false);
    let at = shots.rect("menu_bar_view").center();
    for pressed in [true, false] {
        shots.run(
            vec![egui::Event::PointerMoved(at), button(at, pressed, none)],
            false,
        );
    }
    shots.save(&out, "floating-menu-bar-open.png");
    shots.run(
        vec![egui::Event::Key {
            key: Key::Escape,
            physical_key: Some(Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: none,
        }],
        false,
    );
    // The outline while a docked bar is carried inside its row, and over the
    // left dock.
    let target = shots.rect("bar_grip_Bottom Layout").center();
    shots.carry("Display", target, true);
    shots.save(&out, "drag-outline-row.png");
    shots.release(target);
    shots.carry("Display", Pos2::new(14., 520.), true);
    shots.save(&out, "drag-outline-left.png");
    shots.release(Pos2::new(14., 520.));
    shots.save(&out, "after-docking-left.png");
    // The menu bar docked on the left (titles stacked, menus open to the
    // right) and Layout, with its dropdown buttons, in a left column and
    // along the bottom.
    shots.carry("Menu Bar", Pos2::new(14., 300.), false);
    shots.carry("Layout", Pos2::new(14., 600.), false);
    let at = shots.rect("menu_bar_view").center();
    for pressed in [true, false] {
        shots.run(
            vec![egui::Event::PointerMoved(at), button(at, pressed, none)],
            false,
        );
    }
    shots.save(&out, "menu-bar-docked-left.png");
    shots.run(
        vec![egui::Event::Key {
            key: Key::Escape,
            physical_key: Some(Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: none,
        }],
        false,
    );
    shots.carry("Layout", Pos2::new(600., 676.), false);
    shots.carry("Tools", Pos2::new(1286., 400.), false);
    shots.save(&out, "bottom-and-right-docks.png");
    let _ = std::fs::remove_file(database);
}
