//! Production-UI tests for tab groups and the Additional Activities View,
//! driven through `qa_sweep::Driver` (egui input only, no window).
use super::tab_groups::Split;
use super::*;
use crate::qa_sweep::Driver;

fn driver(name: &str) -> Driver {
    let mut project = openxer_xer::starter();
    schedule_model(&mut project.model, &project.resources).unwrap();
    Driver::new(name, project)
}

fn ctrl() -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    }
}

/// Open View > Tab Groups and click one of its commands.
fn tab_groups_command(d: &mut Driver, action: &str) {
    d.click("menu_bar_view");
    assert!(d.hover_text("Tab Groups"), "View > Tab Groups is drawn");
    d.click(action);
    d.key(Key::Escape);
}

fn names(d: &Driver) -> Vec<Vec<MainView>> {
    d.app
        .views
        .groups
        .groups
        .iter()
        .map(|g| g.tabs.clone())
        .collect()
}

#[test]
fn tab_group_commands_split_focus_resize_close_and_merge() {
    let mut d = driver("tab-groups");
    // One group: nothing to merge, and the single tab cannot be split off
    // until a second view is open in the group (Projects is).
    tab_groups_command(&mut d, "menu_tab_merge");
    assert_eq!(d.app.views.groups.groups.len(), 1);
    d.click("directory_projects");
    assert_eq!(d.app.views.main, MainView::Projects);
    d.click("tool_tab_vertical");
    assert_eq!(
        names(&d),
        vec![vec![MainView::Activities], vec![MainView::Projects]]
    );
    assert_eq!(d.app.views.groups.split, Split::Vertical);
    assert_eq!(d.app.views.main, MainView::Projects);
    // Both groups are drawn: the other group's controls carry its index.
    d.frames(2);
    assert!(d.has("dock_tab_activities@g0") && d.has("dock_tab_projects"));
    assert!(d.has("layout_button@g0") && d.has("tab_splitter_0"));
    let left = d.rect("dock_tab_activities@g0");
    let right = d.rect("dock_tab_projects");
    assert!(left.right() < right.left(), "groups sit side by side");
    assert_eq!(left.top(), right.top());
    // A click anywhere in a group focuses it.
    let pos = d.rect("layout_button@g0").center();
    d.click_at(pos);
    d.key(Key::Escape);
    assert_eq!(d.app.views.groups.focused, 0);
    assert_eq!(d.app.views.main, MainView::Activities);
    assert!(
        !d.app.bar_geometry.is_empty(),
        "the Gantt of the focused view"
    );
    assert!(d.has("dock_tab_projects@g1"));
    // The splitter moves the border.
    let splitter = d.rect("tab_splitter_0");
    let before = d.app.views.groups.shares[0];
    let start = splitter.center();
    let end = start + Vec2::new(-120., 0.);
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let _ = d.frame(vec![egui::Event::PointerMoved(start), button(start, true)]);
    for step in 1..=4 {
        let pos = start + (end - start) * step as f32 / 4.;
        let _ = d.frame(vec![egui::Event::PointerMoved(pos)]);
    }
    let _ = d.frame(vec![button(end, false)]);
    d.frames(2);
    assert!(d.app.views.groups.shares[0] < before - 0.05);
    assert!(d.rect("tab_splitter_0").left() < splitter.left() - 100.);
    // Split commands follow the reference rules: same direction only.
    d.click("directory_wbs");
    assert_eq!(names(&d)[0], vec![MainView::Activities, MainView::Wbs]);
    tab_groups_command(&mut d, "menu_tab_horizontal");
    assert_eq!(d.app.views.groups.groups.len(), 2, "horizontal is off");
    tab_groups_command(&mut d, "menu_tab_vertical");
    assert_eq!(
        names(&d),
        vec![
            vec![MainView::Activities],
            vec![MainView::Wbs],
            vec![MainView::Projects]
        ]
    );
    // The caption's close button closes the focused group's tab; the empty
    // group goes away.
    assert_eq!(d.app.views.main, MainView::Wbs);
    d.click("view_close");
    assert_eq!(
        names(&d),
        vec![vec![MainView::Activities], vec![MainView::Projects]]
    );
    assert_eq!(d.app.views.main, MainView::Activities);
    // Activities never closes.
    d.click("view_close");
    assert_eq!(d.app.views.groups.groups.len(), 2);
    // Merge All Tab Groups.
    tab_groups_command(&mut d, "menu_tab_merge");
    assert_eq!(
        names(&d),
        vec![vec![MainView::Activities, MainView::Projects]]
    );
    assert_eq!(d.app.views.main, MainView::Activities);
    // Horizontal groups stack.
    d.click("dock_tab_projects");
    tab_groups_command(&mut d, "menu_tab_horizontal");
    d.frames(2);
    let top = d.rect("dock_tab_activities@g0");
    let bottom = d.rect("dock_tab_projects");
    assert!(top.bottom() < bottom.top(), "groups stack");
    d.click("tool_tab_merge");
    assert_eq!(d.app.views.groups.groups.len(), 1);
    assert!(d.app.error.is_none());
    d.assert_clean("tab groups");
}

#[test]
fn ctrl_tab_cycles_the_focused_groups_tabs() {
    let mut d = driver("ctrl-tab");
    d.click("directory_wbs");
    d.click("directory_activities");
    d.key_with(Key::Tab, ctrl());
    assert_eq!(d.app.views.main, MainView::Projects);
    d.key_with(Key::Tab, ctrl());
    assert_eq!(d.app.views.main, MainView::Wbs);
    d.key_with(Key::Tab, ctrl());
    assert_eq!(d.app.views.main, MainView::Activities);
    let back = egui::Modifiers {
        shift: true,
        ..ctrl()
    };
    d.key_with(Key::Tab, back);
    assert_eq!(d.app.views.main, MainView::Wbs);
}

#[test]
fn additional_activities_view_keeps_its_own_layout_and_selection() {
    let mut d = driver("additional");
    let first_selected = d.app.selected;
    d.click("menu_bar_project");
    d.click("menu_additional_activities");
    assert_eq!(d.app.views.main, MainView::Activities2);
    assert_eq!(
        d.app.views.opened(),
        vec![
            MainView::Activities,
            MainView::Projects,
            MainView::Activities2
        ]
    );
    assert!(d.has("dock_tab_additional_activities"));
    // It shows the same activities, with its own Gantt.
    assert_eq!(d.app.bar_geometry.len(), 14);
    // Its own selection and collapsed bands.
    let rows = d.visible_activities();
    d.select(rows[3]);
    d.click("menu_bar_view");
    d.click("menu_collapse_all");
    assert!(!d.app.collapsed.is_empty());
    d.click("dock_tab_activities");
    assert_eq!(d.app.views.main, MainView::Activities);
    assert!(
        d.app.collapsed.is_empty(),
        "the first view is not collapsed"
    );
    assert_eq!(d.app.selected, first_selected);
    // Its own filter, saved under its own key.
    d.click("dock_additional_activities");
    assert_eq!(d.app.views.main, MainView::Activities2);
    assert_eq!(d.app.selected, Some(rows[3]));
    assert!(!d.app.collapsed.is_empty());
    d.click("menu_bar_view");
    d.click("menu_expand_all");
    d.app.grid.layout.filter.all_activities = false;
    d.app.grid.layout.filter.selected = vec![crate::grid::filters::Standard::ALL[0].key()];
    d.frames(12);
    d.click("directory_activities");
    assert!(!d.app.grid.layout.filter.active());
    d.app.save_layout();
    let first: serde_json::Value = serde_json::from_str(
        &d.app
            .store
            .preference(crate::grid::CURRENT_KEY)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let second: serde_json::Value = serde_json::from_str(
        &d.app
            .store
            .preference(activity_views::PREF_KEY)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_ne!(first["filter"], second["layout"]["filter"]);
    // Closing the additional view leaves the Activities view.
    d.click("dock_tab_additional_activities");
    d.click("view_close");
    assert!(!d.app.views.opened().contains(&MainView::Activities2));
    assert_eq!(d.app.views.main, MainView::Projects);
    d.click("directory_activities");
    assert!(!d.app.grid.layout.filter.active());
    d.assert_clean("additional view");
}

#[test]
fn edits_in_the_additional_view_show_in_the_first_and_undo_across_views() {
    let mut d = driver("additional-edit");
    // Activities on the left, the additional view on the right.
    d.click("dock_additional_activities");
    d.click("tool_tab_vertical");
    assert_eq!(
        names(&d),
        vec![
            vec![MainView::Activities, MainView::Projects],
            vec![MainView::Activities2]
        ]
    );
    assert_eq!(d.app.views.main, MainView::Activities2);
    // The group the tab left shows its neighbour.
    assert_eq!(d.app.views.groups.groups[0].view(), MainView::Projects);
    let rows = d.visible_activities();
    let target = rows[2];
    let task_id = d.app.project.model.activities[target].task_id;
    let old = d.app.project.details[&task_id].name.clone();
    d.select(target);
    let pos = d.cell(target, "activity_name").unwrap();
    d.double_click_at(pos);
    assert_eq!(d.app.edit_cell, Some((target, "activity_name".to_string())));
    d.key_with(Key::A, ctrl());
    d.type_text("Renamed in the second view");
    d.key(Key::Enter);
    assert_eq!(
        d.app.project.details[&task_id].name,
        "Renamed in the second view"
    );
    // Both views draw the new name in the same frame.
    let drawn = d
        .texts()
        .into_iter()
        .filter(|(t, _)| t == "Renamed in the second view")
        .count();
    assert!(drawn >= 2, "both tables show the edit ({drawn})");
    // Focus the first view and undo there.
    d.click("dock_tab_activities@g0");
    assert_eq!(d.app.views.main, MainView::Activities);
    d.key_with(Key::Z, ctrl());
    assert_eq!(d.app.project.details[&task_id].name, old);
    let drawn = d
        .texts()
        .into_iter()
        .filter(|(t, _)| t == "Renamed in the second view")
        .count();
    assert_eq!(drawn, 0, "the undo shows in both views");
    // Redo from the additional view.
    let pos = d.rect("layout_button@g1").center();
    d.click_at(pos);
    d.key(Key::Escape);
    assert_eq!(d.app.views.main, MainView::Activities2);
    d.key_with(Key::Y, ctrl());
    assert_eq!(
        d.app.project.details[&task_id].name,
        "Renamed in the second view"
    );
    d.assert_clean("edit across views");
}

#[test]
fn a_deleted_activity_leaves_the_parked_view_with_a_valid_selection() {
    let mut d = driver("additional-delete");
    d.click("dock_additional_activities");
    let rows = d.visible_activities();
    let last = *rows.last().unwrap();
    d.select(last);
    d.click("directory_activities");
    // Delete the last activity in the first view.
    d.select(last);
    d.key(Key::Delete);
    if d.has("delete_yes") {
        d.click("delete_yes");
    }
    let n = d.app.project.model.activities.len();
    assert!(last >= n, "the activity was deleted");
    d.click("dock_tab_additional_activities");
    assert!(d.app.selected.is_none_or(|i| i < n));
    d.frames(2);
    d.assert_clean("parked view after delete");
}
