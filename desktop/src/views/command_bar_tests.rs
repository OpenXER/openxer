//! View tables beside the right dock: each edit command shows once (the
//! dock's Edit and Move bars, or the view's own bar while those are
//! hidden), and the Units & Prices controls lie inside their boxes.
use super::*;
use crate::qa_sweep::Driver;
use egui::Rect;

/// The text shapes of one frame with their whole (unclipped) bounds.
fn galleys(d: &mut Driver) -> Vec<(String, Rect)> {
    d.frame(vec![])
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some((t.galley.text().to_string(), t.visual_bounding_rect())),
            _ => None,
        })
        .collect()
}

fn inside(inner: Rect, outer: Rect) -> bool {
    inner.left() >= outer.left() - 0.01
        && inner.right() <= outer.right() + 0.01
        && inner.top() >= outer.top() - 0.01
        && inner.bottom() <= outer.bottom() + 0.01
}

const EDIT_KEYS: [&str; 5] = ["add", "dock_delete", "dock_cut", "dock_copy", "dock_paste"];

#[test]
fn projects_and_wbs_show_each_edit_command_once() {
    let mut d = Driver::new("cmdbar-once", resources::tests::with_resource());
    d.home();
    for (view, own) in [
        (MainView::Projects, "projects_cmd_add"),
        (MainView::Wbs, "wbs_cmd_Add"),
        (MainView::Expenses, "expenses_cmd_add"),
    ] {
        d.app.show_view(view);
        d.frames(3);
        // The dock's Edit bar runs them; the view's bar does not repeat them.
        for key in EDIT_KEYS {
            assert!(d.has(key), "{view:?}: the dock draws {key}");
        }
        assert!(!d.has(own), "{view:?}: {own} is not drawn twice");
    }
    // Projects keeps Open and Close, which the dock does not have.
    d.app.show_view(MainView::Projects);
    d.frames(3);
    assert!(d.has("projects_cmd_open") && d.has("projects_cmd_close"));
    // WBS: no bar of its own at all while Edit and Move are in the dock,
    // so the table reaches the dock.
    d.app.show_view(MainView::Wbs);
    d.frames(3);
    for key in ["wbs_cmd_Add", "wbs_cmd_Del. / Merge", "wbs_cmd_Shift up"] {
        assert!(!d.has(key), "{key}");
    }
    assert!(d.has("dock_move_up"));
    // Hide the Edit toolbar: the views' own buttons come back, once.
    d.app.views.hidden_toolbars.insert("Edit".into());
    for (view, own) in [
        (MainView::Projects, "projects_cmd_add"),
        (MainView::Wbs, "wbs_cmd_Add"),
        (MainView::Expenses, "expenses_cmd_add"),
    ] {
        d.app.show_view(view);
        d.frames(3);
        assert!(d.has(own), "{view:?}: {own} with the Edit bar hidden");
        assert!(!d.has("add"), "{view:?}: and only there");
    }
    // The WBS shift buttons stay with the Move bar, still shown.
    assert!(!d.has("wbs_cmd_Shift up") && d.has("dock_move_up"));
    // The dock's Projects commands follow the view's own rules: the open
    // project cannot be deleted.
    d.app.views.hidden_toolbars.remove("Edit");
    d.app.show_view(MainView::Projects);
    let row = d
        .app
        .workspace_projects()
        .iter()
        .position(|p| p.current)
        .unwrap()
        + 1;
    d.click(&format!("projects_row_{row}"));
    d.frames(2);
    assert_eq!(d.app.view_edit_enabled(ViewEdit::Delete), Some(false));
    assert_eq!(d.app.view_edit_enabled(ViewEdit::Copy), Some(true));
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn units_and_prices_controls_lie_inside_their_boxes() {
    let mut d = Driver::new("cmdbar-rates", resources::tests::with_resource());
    d.home();
    d.click("directory_resources");
    d.click("resources_tab_3");
    d.frames(3);
    let texts = galleys(&mut d);
    // The Shift spinner shows its number inside the box, clear of the
    // arrows.
    let shift = d.rect("resource_shift");
    let arrows = d.rect("resource_shift_up");
    let one = texts
        .iter()
        .filter(|(t, _)| t == "1")
        .map(|(_, r)| *r)
        .find(|r| r.center().y > shift.top() && r.center().y < shift.bottom())
        .expect("the shift number is drawn");
    assert!(
        inside(one, shift) && one.right() <= arrows.left(),
        "{one:?} in {shift:?}, left of {arrows:?}"
    );
    // Each in-place rate editor and its text lie inside their column.
    let table = d.rect("resource_rates_row_0");
    for k in 0..3 {
        let cell = d.rect(&format!("resource_rate_cell_{k}"));
        assert!(inside(cell, table), "cell {k}: {cell:?} in {table:?}");
        let drawn: Vec<Rect> = texts
            .iter()
            .filter(|(t, r)| {
                !t.is_empty()
                    && r.center().x > cell.left()
                    && r.center().x < cell.right()
                    && r.center().y > cell.top()
                    && r.center().y < cell.bottom()
            })
            .map(|(_, r)| *r)
            .collect();
        assert!(!drawn.is_empty(), "cell {k} shows its value");
        for r in drawn {
            assert!(inside(r, cell), "cell {k}: text {r:?} inside {cell:?}");
        }
    }
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}
