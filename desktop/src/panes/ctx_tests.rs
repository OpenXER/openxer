//! The Activity Network and Trace Logic right-click menus and gestures
//! through the production UI: box and background menus, Dissolve, Link and
//! Unlink, band collapse, a double-click on a relationship line, dragging
//! one box to another, Alt+drag zoom, the hover tooltip, the WBS tree and
//! the Trace Logic zoom. egui `RawInput` only; no native window.
use super::*;
use crate::qa_sweep::Driver;
use egui::PointerButton::{Primary, Secondary};

fn starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn ctrl() -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    }
}

fn alt() -> egui::Modifiers {
    egui::Modifiers {
        alt: true,
        ..Default::default()
    }
}

fn network_driver(name: &str) -> Driver {
    let mut d = Driver::new(name, starter());
    d.app.set_top(TopView::Network);
    d.app.set_bottom(None);
    d.frames(4);
    d
}

/// The screen rectangle of the box of activity `i`.
fn box_of(d: &Driver, i: usize) -> Rect {
    let task_id = d.app.project.model.activities[i].task_id;
    d.app
        .panes
        .network
        .boxes
        .iter()
        .find(|(t, _)| *t == task_id)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| panic!("no box for activity {i}"))
}

/// Model indices of the boxes on screen, in drawing order.
fn drawn(d: &Driver) -> Vec<usize> {
    d.app
        .panes
        .network
        .boxes
        .iter()
        .map(|(t, _)| {
            d.app
                .project
                .model
                .activities
                .iter()
                .position(|a| a.task_id == *t)
                .unwrap()
        })
        .collect()
}

/// A point on the canvas under every box and band.
fn blank(d: &Driver) -> Pos2 {
    let low = d
        .app
        .panes
        .network
        .boxes
        .iter()
        .map(|(_, r)| r.bottom())
        .fold(0., f32::max);
    let left = d
        .app
        .panes
        .network
        .boxes
        .iter()
        .map(|(_, r)| r.left())
        .fold(f32::MAX, f32::min);
    Pos2::new(left + 40., low + 60.)
}

fn right_click(d: &mut Driver, pos: Pos2) {
    d.press_at(pos, Secondary, Default::default());
}

fn tie_free(d: &mut Driver) {
    d.app.project.model.links.clear();
    d.app.project.model.rebuild_links().unwrap();
    d.frames(3);
}

const MENU: [&str; 13] = [
    "activity_details",
    "options",
    "trace_options",
    "reorganize",
    "reorganize_all",
    "fit",
    "dissolve",
    "link",
    "unlink",
    "assign_codes",
    "assign_roles",
    "assign_by_role",
    "work_products",
];

/// Whether the item runs when clicked: an enabled item closes the menu, a
/// disabled one leaves it open.
fn closes_on_click(d: &mut Driver, item: &str) -> bool {
    let pos = d.rect(&format!("net_{item}")).center();
    d.press_at(pos, Primary, Default::default());
    !d.has("net_activity_details")
}

#[test]
fn box_right_click_selects_the_box_and_lists_the_thirteen_items_in_order() {
    let mut d = network_driver("net-menu-box");
    d.app.selected = None;
    d.frames(2);
    let i = drawn(&d)[1];
    let at = box_of(&d, i).center();
    right_click(&mut d, at);
    assert_eq!(d.app.selected, Some(i), "the right-click selects the box");
    let tops: Vec<f32> = MENU
        .iter()
        .map(|k| d.rect(&format!("net_{k}")).top())
        .collect();
    assert!(
        tops.windows(2).all(|w| w[0] < w[1]),
        "items in order: {tops:?}"
    );
    let listed = d
        .app
        .action_rects
        .keys()
        .filter(|k| k.starts_with("net_"))
        .count();
    assert_eq!(listed, 13);
    // One box selected: Link and Unlink need two; the rest apply.
    for item in ["link", "unlink"] {
        assert!(!closes_on_click(&mut d, item), "{item} needs two boxes");
    }
    assert!(closes_on_click(&mut d, "activity_details"));
    assert!(d.app.bottom_showing(BottomView::Details));
    assert!(d.app.error.is_none());
}

#[test]
fn background_right_click_disables_the_selection_items() {
    let mut d = network_driver("net-menu-background");
    let i = drawn(&d)[0];
    d.app.selected = Some(i);
    d.frames(2);
    let at = blank(&d);
    assert!(at.x < 1480. && at.y < 880., "{at:?} is on screen");
    right_click(&mut d, at);
    assert!(
        d.has("net_activity_details"),
        "the menu opens on the canvas"
    );
    assert_eq!(d.app.selected, None, "nothing is selected there");
    for item in [
        "activity_details",
        "dissolve",
        "link",
        "unlink",
        "assign_codes",
        "assign_roles",
        "assign_by_role",
        "work_products",
    ] {
        assert!(!closes_on_click(&mut d, item), "{item} is disabled");
        assert!(d.has("net_activity_details"), "the menu stays open");
    }
    // The network-wide items apply.
    assert!(closes_on_click(&mut d, "options"));
    assert!(d.app.panes.network_dialog.is_some());
    d.key(Key::Escape);
    assert!(d.app.panes.network_dialog.is_none());
    right_click(&mut d, at);
    assert!(closes_on_click(&mut d, "trace_options"));
    assert!(d.app.panes.trace_dialog.is_some());
    d.key(Key::Escape);
    assert!(d.app.panes.trace_dialog.is_none());
    // Zoom to Best Fit runs.
    d.app.panes.network.zoom = 0.3;
    right_click(&mut d, at);
    assert!(closes_on_click(&mut d, "fit"));
    d.frames(3);
    assert!(d.app.panes.network.zoom > 0.3);
}

#[test]
fn work_products_and_roles_open_the_assign_tabs() {
    let mut d = network_driver("net-menu-assign");
    let i = drawn(&d)[0];
    use crate::details::assignments::ResourceAssign;
    for (item, tab, command) in [
        (
            "assign_codes",
            DetailTab::Codes,
            editing::Command::Assign(DetailTab::Codes),
        ),
        (
            "assign_roles",
            DetailTab::Resources,
            editing::Command::AssignResource(ResourceAssign::Roles),
        ),
        (
            "assign_by_role",
            DetailTab::Resources,
            editing::Command::AssignResource(ResourceAssign::ByRole),
        ),
        (
            "work_products",
            DetailTab::Documents,
            editing::Command::Assign(DetailTab::Documents),
        ),
    ] {
        let at = box_of(&d, i).center();
        right_click(&mut d, at);
        assert!(closes_on_click(&mut d, item), "{item}");
        assert_eq!(d.app.editing.last, Some(command));
        assert_eq!(d.app.detail.tab, tab, "{item}");
        // The details pane replaced the network's bottom view; back.
        d.app.set_bottom(None);
        d.app.detail.picker = None;
        d.app.detail.assign = None;
        d.frames(2);
    }
    assert!(d.app.error.is_none());
}

#[test]
fn dissolve_link_and_unlink_are_one_undo_step_each() {
    let mut d = network_driver("net-menu-commands");
    let order = drawn(&d);
    let (a, b) = (order[0], order[1]);
    tie_free(&mut d);
    let ties = d.app.project.model.links.len();
    let undo = d.app.history.counts().0;
    let ra = box_of(&d, a);
    d.press_at(ra.center(), Primary, Default::default());
    let rb = box_of(&d, b);
    d.press_at(rb.center(), Primary, ctrl());
    assert_eq!(d.app.selected_in_order().len(), 2, "Ctrl+click adds a box");
    let rb = box_of(&d, b);
    right_click(&mut d, rb.center());
    assert_eq!(d.app.selected_in_order().len(), 2, "selection stays");
    assert!(closes_on_click(&mut d, "link"));
    d.frames(3);
    assert_eq!(d.app.project.model.links.len(), ties + 1);
    assert_eq!(d.app.history.counts().0, undo + 1, "Link is one undo step");
    let rb = box_of(&d, b);
    right_click(&mut d, rb.center());
    assert!(closes_on_click(&mut d, "unlink"));
    d.frames(3);
    assert_eq!(d.app.project.model.links.len(), ties);
    assert_eq!(
        d.app.history.counts().0,
        undo + 2,
        "Unlink is one undo step"
    );
    // Without a predecessor and a successor Dissolve is disabled.
    let rb = box_of(&d, b);
    right_click(&mut d, rb.center());
    assert!(!closes_on_click(&mut d, "dissolve"), "nothing to rejoin");
    d.key(egui::Key::Escape);
    d.frames(2);
    // Give both a predecessor and a successor: c -> a -> b -> e.
    let (c, e) = (order[2], order[3]);
    let task = |i: usize| d.app.project.model.activities[i].task_id;
    let ends = (task(c), task(e));
    for (p, s) in [(c, a), (a, b), (b, e)] {
        let fs = openxer_engine::model::LinkType::FS;
        crate::relationships::add(&mut d.app.project.model, p, s, fs, 0).unwrap();
    }
    d.frames(3);
    assert_eq!(d.app.selected_in_order().len(), 2, "a and b stay selected");
    // Dissolve asks first, then removes both activities in one step.
    let count = d.app.project.model.activities.len();
    let undo = d.app.history.counts().0;
    let rb = box_of(&d, b);
    right_click(&mut d, rb.center());
    assert!(closes_on_click(&mut d, "dissolve"));
    d.frames(2);
    assert_eq!(
        d.app.editing.dialogs.dissolve.as_ref().map(Vec::len),
        Some(2)
    );
    assert_eq!(d.app.project.model.activities.len(), count, "not yet");
    d.click("dissolve_yes");
    d.frames(3);
    assert_eq!(d.app.project.model.activities.len(), count - 2);
    let m = &d.app.project.model;
    let left: Vec<_> = m
        .links
        .iter()
        .map(|l| (m.activities[l.pred].task_id, m.activities[l.succ].task_id))
        .collect();
    assert_eq!(left, vec![ends], "c is tied to e");
    assert_eq!(
        d.app.history.counts().0,
        undo + 1,
        "Dissolve is one undo step"
    );
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

/// The longest stretch of a drawn relationship line that no box covers.
fn line_midpoint(d: &Driver) -> (usize, Pos2) {
    let n = &d.app.panes.network;
    let mut best: Option<(f32, usize, Pos2)> = None;
    for (index, points) in &n.links {
        for pair in points.windows(2) {
            let mid = pair[0].lerp(pair[1], 0.5);
            let length = pair[0].distance(pair[1]);
            let covered = n.boxes.iter().any(|(_, r)| r.expand(6.).contains(mid));
            if !covered && best.is_none_or(|b| length > b.0) {
                best = Some((length, *index, mid));
            }
        }
    }
    let (_, index, mid) = best.expect("a relationship line between boxes");
    (index, mid)
}

#[test]
fn double_clicking_a_relationship_line_edits_it_and_delete_removes_it() {
    let mut d = network_driver("net-line");
    assert!(!d.app.panes.network.links.is_empty(), "lines are drawn");
    let (_, mid) = line_midpoint(&d);
    let expected = crate::gantt::pick_link(mid, &d.app.panes.network.links).unwrap();
    let ties = d.app.project.model.links.len();
    let undo = d.app.history.counts().0;
    d.double_click_at(mid);
    d.frames(2);
    let editor = d.app.link_editor.as_ref().expect("Edit Relationship opens");
    assert_eq!(editor.index, expected);
    // Cancel changes nothing.
    d.click("link_cancel");
    assert!(d.app.link_editor.is_none());
    assert_eq!(d.app.project.model.links.len(), ties);
    assert_eq!(d.app.history.counts().0, undo);
    d.double_click_at(mid);
    d.frames(2);
    assert!(d.app.link_editor.is_some());
    let before = d.app.panes.network.links.len();
    d.click("link_delete");
    d.click("delete_relationship_yes");
    d.frames(4);
    assert!(d.app.link_editor.is_none());
    assert_eq!(d.app.project.model.links.len(), ties - 1);
    assert_eq!(
        d.app.panes.network.links.len(),
        before - 1,
        "the line is gone"
    );
    assert_eq!(d.app.history.counts().0, undo + 1);
}

#[test]
fn collapse_all_and_expand_all_hide_and_restore_boxes_and_are_saved_with_the_layout() {
    let mut d = network_driver("net-collapse");
    let all = d.app.panes.network.boxes.len();
    assert!(all > 1);
    d.click("menu_bar_view");
    d.click("menu_collapse_all");
    d.frames(3);
    assert!(d.app.panes.network.boxes.is_empty(), "boxes are hidden");
    let net = d.app.network_layout();
    assert!(!net.bands.is_empty() && net.bands.iter().all(|b| b.collapsed));
    assert!(
        d.app.collapsed.is_empty(),
        "the table's groups are untouched"
    );
    // Saved with the pane, a layout save and load keeps it.
    d.app.save_layout_as("Collapsed").unwrap();
    let json = d.app.panes.json();
    assert_eq!(
        json["network"]["collapsed"].as_array().map(Vec::len),
        Some(net.bands.len())
    );
    d.click("menu_bar_view");
    d.click("menu_expand_all");
    d.frames(3);
    assert_eq!(d.app.panes.network.boxes.len(), all, "boxes are back");
    assert!(d.app.panes.network.collapsed.is_empty());
    d.app.open_layout("Collapsed", true).unwrap();
    d.frames(3);
    assert!(d.app.panes.network.boxes.is_empty(), "collapse state loads");
    // One band's box toggles it back.
    d.click("network_band_toggle_0");
    d.frames(3);
    assert!(!d.app.panes.network.boxes.is_empty());
    assert_eq!(
        d.app.panes.network.collapsed.len(),
        net.bands.len() - 1,
        "only the first band opened"
    );
    // Garbage in a saved layout is ignored or bounded.
    let mut state = network::NetworkState::default();
    state.restore(&serde_json::json!({"collapsed": ["A", 3, "", "B"], "show_tree": true}));
    assert_eq!(state.collapsed.len(), 2);
    assert!(state.show_tree);
}

#[test]
fn collapsing_a_band_keeps_the_other_boxes_in_place() {
    let mut d = network_driver("net-collapse-stable");
    let before = d.app.network_layout();
    assert!(before.bands.len() > 1, "the starter has several bands");
    let first = before.bands[0].label.clone();
    d.app.network_toggle_band(&first);
    let after = d.app.network_layout();
    assert!(after.bands[0].collapsed);
    for b in &after.boxes {
        let old = before.find(b.activity).unwrap();
        assert_eq!(b.column, old.column, "columns do not shift");
        assert!(b.rect.top() < old.rect.top(), "the later bands move up");
    }
    assert!(after.boxes.len() < before.boxes.len());
}

fn drag(d: &mut Driver, from: Pos2, to: Pos2, modifiers: egui::Modifiers) {
    let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: Primary,
        pressed,
        modifiers,
    };
    let _ = d.frame_with(vec![egui::Event::PointerMoved(from)], modifiers);
    let _ = d.frame_with(vec![button(from, true)], modifiers);
    for k in 1..=6 {
        let at = from + (to - from) * (k as f32 / 6.);
        let _ = d.frame_with(vec![egui::Event::PointerMoved(at)], modifiers);
    }
    let _ = d.frame_with(vec![button(to, false)], modifiers);
    d.frames(3);
}

#[test]
fn dragging_from_a_box_edge_to_another_box_creates_a_finish_to_start_tie() {
    let mut d = network_driver("net-drag-link");
    tie_free(&mut d);
    let order = drawn(&d);
    let (a, b) = (order[0], order[1]);
    let undo = d.app.history.counts().0;
    let (ra, rb) = (box_of(&d, a), box_of(&d, b));
    let from = Pos2::new(ra.right() - 3., ra.center().y);
    drag(&mut d, from, rb.center(), Default::default());
    assert_eq!(d.app.project.model.links.len(), 1);
    let link = &d.app.project.model.links[0];
    assert_eq!((link.pred, link.succ, link.link_type), (a, b, LinkType::FS));
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");
    assert!(d.app.status.starts_with("Created"), "{}", d.app.status);
    assert!(
        d.app.panes.network.positions.is_empty(),
        "no box was moved by the drag"
    );
    // The way back would close a loop: refused with a message.
    let (ra, rb) = (box_of(&d, a), box_of(&d, b));
    let from = Pos2::new(rb.right() - 3., rb.center().y);
    drag(&mut d, from, ra.center(), Default::default());
    assert_eq!(d.app.project.model.links.len(), 1, "no new tie");
    assert!(
        d.app.status.starts_with("Relationship not created"),
        "{}",
        d.app.status
    );
    assert_eq!(d.app.history.counts().0, undo + 1);
    // A drag from the middle of a box still moves it.
    let ra = box_of(&d, a);
    drag(
        &mut d,
        ra.center(),
        ra.center() + Vec2::new(0., 40.),
        Default::default(),
    );
    assert_eq!(d.app.panes.network.positions.len(), 1);
    assert_eq!(d.app.project.model.links.len(), 1);
    // Released on empty canvas: cancelled.
    let ra = box_of(&d, a);
    let empty = blank(&d);
    drag(
        &mut d,
        Pos2::new(ra.left() + 3., ra.center().y),
        empty,
        Default::default(),
    );
    assert_eq!(d.app.project.model.links.len(), 1);
    assert!(
        d.app.status.starts_with("Relationship cancelled"),
        "{}",
        d.app.status
    );
}

#[test]
fn alt_drag_zooms_to_the_rectangle() {
    let mut d = network_driver("net-alt-zoom");
    d.app.panes.network.zoom = 0.5;
    d.frames(3);
    let order = drawn(&d);
    let ra = box_of(&d, order[0]);
    // Pressing on a box with Alt held zooms; it does not move the box.
    let from = ra.center();
    let to = ra.center() + Vec2::new(150., 90.);
    let before = d.app.panes.network.zoom;
    drag(&mut d, from, to, alt());
    let after = d.app.panes.network.zoom;
    assert!(after > before * 1.5, "{before} -> {after}");
    assert!((network::MIN_ZOOM..=network::MAX_ZOOM).contains(&after));
    assert!(d.app.panes.network.zoom_drag.is_none());
    assert!(d.app.panes.network.positions.is_empty(), "no box moved");
    assert!(d.app.project.model.links.is_empty() || d.app.history.counts().0 == 0);
    // The box now fills more of the view: its width grew with the zoom.
    let grown = box_of(&d, order[0]).width();
    assert!((grown - 160. * after).abs() < 2., "{grown} at {after}");
    // A click with Alt held is not a rectangle.
    let spot = blank(&d);
    drag(&mut d, spot, spot + Vec2::new(2., 2.), alt());
    assert_eq!(d.app.panes.network.zoom, after);
}

#[test]
fn hovering_a_box_for_half_a_second_shows_id_name_and_dates() {
    let mut d = network_driver("net-tooltip");
    let i = drawn(&d)[0];
    let r = box_of(&d, i);
    let a = &d.app.project.model.activities[i];
    let (code, name) = (a.task_code.clone(), d.app.project.name(i).to_string());
    let has = |texts: &[(String, Rect)]| {
        texts.iter().any(|(t, _)| t.starts_with("Start: "))
            && texts.iter().any(|(t, _)| t.starts_with("Finish: "))
            && texts.iter().any(|(t, _)| *t == format!("{code}  {name}"))
    };
    let _ = d.frame(vec![egui::Event::PointerMoved(r.center())]);
    d.frames(1);
    assert!(!has(&d.texts()), "no tooltip yet");
    d.frames(5);
    assert!(has(&d.texts()), "tooltip after the delay");
    // Moving away hides it.
    let _ = d.frame(vec![egui::Event::PointerMoved(blank(&d))]);
    d.frames(2);
    assert!(!has(&d.texts()));
}

#[test]
fn the_wbs_tree_lists_bands_and_scrolls_the_network_to_the_one_picked() {
    let mut d = network_driver("net-tree");
    d.app.panes.network.show_tree = true;
    d.frames(3);
    let net = d.app.network_layout();
    assert!(net.bands.len() > 1);
    assert!(d.has("network_tree_0") && d.has("network_tree_1"));
    // The tree took the left of the pane: the first box moved right.
    let last = 1;
    d.click(&format!("network_tree_{last}"));
    d.frames(4);
    let zoom = d.app.panes.network.zoom;
    let expected = (net.bands[last].top - 4.).max(0.) * zoom;
    let limit = (net.size.y * zoom - d.app.panes.network.view_size.y).max(0.);
    let offset = d.app.panes.network.offset.y;
    assert!(
        offset > 0. && offset <= expected.min(limit) + 20.,
        "{offset} for a band at {expected}"
    );
    assert_eq!(
        d.app.panes.network.tree_pick.as_deref(),
        Some(net.bands[last].label.as_str())
    );
    // The tree's box collapses a band; the menu item hides the tree.
    d.click("network_tree_toggle_0");
    d.frames(3);
    assert!(d.app.panes.network.collapsed.contains(&net.bands[0].label));
    d.click("menu_bar_view");
    assert!(d.hover_text("Layout"));
    assert!(d.hover_text("Activity Network"));
    d.click("menu_network_tree");
    d.frames(2);
    assert!(!d.app.panes.network.show_tree);
}

#[test]
fn reorganize_returns_the_selected_branch_and_reorganize_all_every_box() {
    let mut d = network_driver("net-reorganize");
    let order = drawn(&d);
    let codes: Vec<String> = order
        .iter()
        .take(3)
        .map(|&i| d.app.project.model.activities[i].task_code.clone())
        .collect();
    for (k, code) in codes.iter().enumerate() {
        d.app
            .panes
            .network
            .positions
            .insert(code.clone(), Pos2::new(400. + 30. * k as f32, 300.));
    }
    // The selected box has no successors among the dragged ones here: only
    // its own place is dropped.
    tie_free(&mut d);
    d.app.selected = Some(order[0]);
    d.app.editing.selection.clear();
    d.frames(2);
    d.app.network_reorganize_branch();
    assert!(!d.app.panes.network.positions.contains_key(&codes[0]));
    assert!(d.app.panes.network.positions.contains_key(&codes[1]));
    // With a tie from the first to the second, the branch includes it.
    d.app
        .panes
        .network
        .positions
        .insert(codes[0].clone(), Pos2::new(1., 1.));
    d.app
        .network_link_boxes(order[0], order[1], LinkType::FS, 0.);
    d.app.network_reorganize_branch();
    assert!(!d.app.panes.network.positions.contains_key(&codes[1]));
    assert!(d.app.panes.network.positions.contains_key(&codes[2]));
    // Reorganize All through the menu clears the rest.
    let at = box_of(&d, order[2]).center();
    right_click(&mut d, at);
    assert!(closes_on_click(&mut d, "reorganize_all"));
    d.frames(2);
    assert!(d.app.panes.network.positions.is_empty());
}

#[test]
fn trace_logic_zooms_from_its_menu_within_bounds() {
    let mut d = Driver::new("trace-zoom", starter());
    d.app.set_bottom(Some(BottomView::TraceLogic));
    let i = d
        .app
        .project
        .model
        .links
        .first()
        .map(|l| l.succ)
        .expect("the starter has relationships");
    d.app.selected = Some(i);
    d.frames(4);
    assert_eq!(d.app.panes.trace.zoom, 1.);
    let width = |d: &Driver| d.app.panes.trace.boxes[0].1.width();
    assert!((width(&d) - 160.).abs() < 0.5);
    let at = d.app.panes.trace.boxes[0].1.center();
    right_click(&mut d, at);
    d.click("trace_menu_zoom_in");
    d.frames(2);
    assert_eq!(d.app.panes.trace.zoom, 1.25);
    assert!((width(&d) - 200.).abs() < 1.5, "{}", width(&d));
    let at = d.app.panes.trace.boxes[0].1.center();
    right_click(&mut d, at);
    d.click("trace_menu_zoom_out");
    d.click_at(Pos2::new(700., 893.));
    d.frames(2);
    assert_eq!(d.app.panes.trace.zoom, 1.);
    // Bounds.
    for _ in 0..30 {
        d.app.trace_zoom(true);
    }
    assert_eq!(d.app.panes.trace.zoom, network::MAX_ZOOM);
    for _ in 0..60 {
        d.app.trace_zoom(false);
    }
    assert_eq!(d.app.panes.trace.zoom, network::MIN_ZOOM);
    // Best Fit from the background menu.
    d.frames(3);
    let spot = Pos2::new(
        d.app.panes.body.right() - 30.,
        d.app.panes.body.bottom() - 20.,
    );
    right_click(&mut d, spot);
    d.click("trace_menu_fit");
    d.frames(3);
    let zoom = d.app.panes.trace.zoom;
    assert!(
        zoom > network::MIN_ZOOM && zoom <= network::MAX_ZOOM,
        "{zoom}"
    );
    assert!(d.app.error.is_none());
}

#[test]
fn shift_f10_opens_the_network_menu_below_the_selected_box() {
    let mut d = network_driver("net-keyboard");
    let i = drawn(&d)[0];
    d.app.selected = Some(i);
    d.frames(2);
    d.key_with(
        Key::F10,
        egui::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert!(d.has("net_activity_details"), "the menu opened");
    let r = box_of(&d, i);
    let item = d.rect("net_activity_details");
    assert!(item.top() >= r.bottom() - 1., "{item:?} below {r:?}");
}

/// The production UI drawn by the software painter, with its own app.
struct Shots {
    painter: crate::headless_render::SoftwarePainter,
    time: f64,
    out: PathBuf,
    app: App,
    ctx: egui::Context,
}

impl Shots {
    fn new(name: &str) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("openxer-cmb-{name}-{unique}"));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(dir.join("workspace.db")).unwrap();
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let app = App::new(&ctx, store, starter());
        let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("artifacts/cmb");
        std::fs::create_dir_all(&out).unwrap();
        let mut s = Self {
            painter: Default::default(),
            time: 100.,
            out,
            app,
            ctx,
        };
        s.frames(3);
        s
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        let _ = self
            .painter
            .frame(&mut self.app, &self.ctx, &mut self.time, events, false);
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.frame(vec![]);
        }
    }

    /// Runs `events`, settles, and saves the next frame as `name`.png.
    fn shot(&mut self, events: Vec<egui::Event>, name: &str) {
        self.frame(events);
        self.frames(2);
        self.save(name);
    }

    fn save(&mut self, name: &str) {
        let (image, triangles) = self
            .painter
            .frame(&mut self.app, &self.ctx, &mut self.time, vec![], true)
            .unwrap();
        assert!(triangles > 300, "{name}");
        image.save(self.out.join(format!("{name}.png"))).unwrap();
    }

    fn box_of(&self, i: usize) -> Rect {
        let task_id = self.app.project.model.activities[i].task_id;
        self.app
            .panes
            .network
            .boxes
            .iter()
            .find(|(t, _)| *t == task_id)
            .map(|(_, r)| *r)
            .unwrap()
    }
}

fn pointer(pos: Pos2, button: egui::PointerButton, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: Default::default(),
    }
}

fn click_events(pos: Pos2, button: egui::PointerButton) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        pointer(pos, button, true),
        pointer(pos, button, false),
    ]
}

/// Renders the new network surfaces to PNG files under `artifacts/cmb`
/// (ignored by git) for looking at.
#[test]
fn render_network_menus_tooltip_tree_and_link_drag() {
    let mut s = Shots::new("render");
    s.app.set_top(TopView::Network);
    s.app.set_bottom(None);
    s.frames(4);
    let order: Vec<usize> = s
        .app
        .panes
        .network
        .boxes
        .iter()
        .map(|(t, _)| {
            s.app
                .project
                .model
                .activities
                .iter()
                .position(|a| a.task_id == *t)
                .unwrap()
        })
        .collect();
    let r = s.box_of(order[1]);
    s.frame(vec![egui::Event::PointerMoved(r.center())]);
    s.frames(7);
    s.save("network-tooltip");
    s.shot(click_events(r.center(), Secondary), "network-menu-box");
    s.frame(vec![egui::Event::Key {
        key: Key::Escape,
        physical_key: Some(Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    }]);
    let low = s
        .app
        .panes
        .network
        .boxes
        .iter()
        .map(|(_, r)| r.bottom())
        .fold(0., f32::max);
    let spot = Pos2::new(r.left() + 40., low + 60.);
    s.shot(click_events(spot, Secondary), "network-menu-background");
    s.frame(vec![egui::Event::Key {
        key: Key::Escape,
        physical_key: Some(Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    }]);
    s.app.panes.network.show_tree = true;
    let second = s.app.network_layout().bands[1].label.clone();
    s.app.network_toggle_band(&second);
    s.shot(vec![egui::Event::PointerGone], "network-tree-collapsed");
    // A relationship drag in progress.
    s.app.panes.network.collapsed.clear();
    s.frames(3);
    let (ra, rb) = (s.box_of(order[0]), s.box_of(order[2]));
    let from = Pos2::new(ra.right() - 3., ra.center().y);
    let to = rb.center();
    s.frame(vec![egui::Event::PointerMoved(from)]);
    s.frame(vec![pointer(from, Primary, true)]);
    for k in 1..=4 {
        let p = from + (to - from) * (k as f32 / 4.);
        s.frame(vec![egui::Event::PointerMoved(p)]);
    }
    s.save("network-link-drag");
    s.frame(vec![pointer(to, Primary, false)]);
    // Trace Logic zoomed, with its menu.
    s.app.set_top(TopView::Gantt);
    s.app.set_bottom(Some(BottomView::TraceLogic));
    s.app.selected = Some(order[1]);
    s.frames(3);
    s.app.panes.trace.zoom = 0.8;
    s.frames(2);
    let t = s.app.panes.trace.boxes[0].1.center();
    s.shot(click_events(t, Secondary), "trace-menu-zoomed");
}
