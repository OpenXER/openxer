//! Multi-row selection in the view tables built on `grid`: plain, Ctrl and
//! Shift clicks, Ctrl+A, Shift+Down and the right-click rule in the
//! Resources view, multi-row Delete as one undo step, and the WBS and
//! Projects tables painting a multi-selection. Driven through the
//! production UI with egui `RawInput` only (no native window or pointer).
use super::*;
use crate::chrome::SELECT;
use crate::qa_sweep::Driver;

const CTRL: egui::Modifiers = egui::Modifiers::COMMAND;
const SHIFT: egui::Modifiers = egui::Modifiers::SHIFT;

/// A point in row `index` of the view table `id`, clear of the expander.
fn row_point(d: &Driver, id: &str, index: usize) -> Pos2 {
    let r = d.rect(&format!("{id}_row_{index}"));
    Pos2::new(r.left() + 90., r.center().y)
}

fn click_row(d: &mut Driver, id: &str, index: usize, modifiers: egui::Modifiers) {
    // Clear of the double-click window of the last click.
    d.frames(4);
    let at = row_point(d, id, index);
    d.press_at(at, egui::PointerButton::Primary, modifiers);
    let _ = d.frame(vec![egui::Event::PointerGone]);
}

fn right_click_row(d: &mut Driver, id: &str, index: usize) {
    d.frames(4);
    let at = row_point(d, id, index);
    d.press_at(at, egui::PointerButton::Secondary, Default::default());
}

/// Rows of table `id` (0..count) painted in the selection color.
fn painted(d: &mut Driver, id: &str, count: usize) -> Vec<usize> {
    let output = d.frame(vec![]);
    let mut fills = vec![];
    fn walk(shape: &egui::Shape, fills: &mut Vec<Rect>) {
        match shape {
            egui::Shape::Rect(r) if r.fill == SELECT => fills.push(r.rect),
            egui::Shape::Vec(list) => list.iter().for_each(|s| walk(s, fills)),
            _ => {}
        }
    }
    for clipped in &output.shapes {
        walk(&clipped.shape, &mut fills);
    }
    (0..count)
        .filter(|i| {
            let row = d.rect(&format!("{id}_row_{i}"));
            fills
                .iter()
                .any(|f| f.height() > 4. && row.contains(f.center()))
        })
        .collect()
}

fn rsrc_rows(d: &Driver) -> usize {
    d.app
        .project
        .document
        .table("RSRC")
        .map_or(0, |t| t.rows.len())
}

/// The Resources view with all resources shown: PIPE (7), its child
/// PIPE.CRANE (8), IDLE (9) and two new top-level resources.
fn resources_view() -> (Driver, Vec<i64>) {
    let mut d = Driver::new("multiselect", resources::tests::with_resource());
    d.click("directory_resources");
    d.app.views.resources.assigned_only = false;
    d.app.views.resources.selected = Some(9);
    d.app.resource_command("Add", 0.);
    d.app.resource_command("Add", 0.1);
    d.frames(3);
    let order = d.app.views.resources.picked();
    assert_eq!(order.len(), 1);
    let s = &d.app.views.resources;
    let current = s.selected;
    let mut rows = s.rows.clone();
    let all = rows.select_all(current.as_ref());
    let ids = rows.picked(all.as_ref());
    assert_eq!(ids.len(), 5, "five resources shown: {ids:?}");
    assert_eq!(&ids[..3], [7, 8, 9]);
    (d, ids)
}

#[test]
fn resources_table_ctrl_shift_ctrl_a_and_shift_down_select_rows() {
    let (mut d, ids) = resources_view();
    click_row(&mut d, "resources", 0, Default::default());
    assert_eq!(d.app.views.resources.picked(), [7]);
    // Shift+click: the anchor to the row, in table order.
    click_row(&mut d, "resources", 2, SHIFT);
    assert_eq!(d.app.views.resources.picked(), [7, 8, 9]);
    assert_eq!(d.app.views.resources.selected, Some(9), "the last click");
    assert_eq!(painted(&mut d, "resources", 5), [0, 1, 2]);
    // Ctrl+click toggles a row off and another on.
    click_row(&mut d, "resources", 1, CTRL);
    assert_eq!(d.app.views.resources.picked(), [7, 9]);
    click_row(&mut d, "resources", 4, CTRL);
    assert_eq!(d.app.views.resources.picked(), [7, 9, ids[4]]);
    assert_eq!(d.app.views.resources.selected, Some(ids[4]));
    assert_eq!(painted(&mut d, "resources", 5), [0, 2, 4]);
    // Right-click inside the selection keeps it.
    right_click_row(&mut d, "resources", 0);
    assert_eq!(d.app.views.resources.picked(), [7, 9, ids[4]]);
    let _ = d.frame(vec![egui::Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    }]);
    d.frames(2);
    // Ctrl+A: every row shown, the current row kept.
    d.key_with(Key::A, CTRL);
    assert_eq!(d.app.views.resources.picked(), ids);
    assert_eq!(d.app.views.resources.selected, Some(ids[4]));
    assert_eq!(painted(&mut d, "resources", 5), [0, 1, 2, 3, 4]);
    // Plain click, then Shift+Down twice extends from the anchor.
    click_row(&mut d, "resources", 1, Default::default());
    d.key_with(Key::ArrowDown, SHIFT);
    d.key_with(Key::ArrowDown, SHIFT);
    assert_eq!(d.app.views.resources.picked(), [8, 9, ids[3]]);
    assert_eq!(d.app.views.resources.selected, Some(ids[3]));
    d.key_with(Key::ArrowUp, SHIFT);
    assert_eq!(d.app.views.resources.picked(), [8, 9]);
    // A plain key moves the current row alone; the details follow it.
    d.key(Key::ArrowDown);
    assert_eq!(d.app.views.resources.picked(), [ids[3]]);
    d.key(Key::Home);
    assert_eq!(d.app.views.resources.picked(), [7]);
    d.key(Key::End);
    assert_eq!(d.app.views.resources.picked(), [ids[4]]);
    // Right-click outside the selection selects that row alone.
    click_row(&mut d, "resources", 0, Default::default());
    click_row(&mut d, "resources", 1, SHIFT);
    right_click_row(&mut d, "resources", 3);
    assert_eq!(d.app.views.resources.picked(), [ids[3]]);
    assert!(d.app.error.is_none());
}

/// Enter in the multi-row delete question answers No, the button framed as
/// the default, as in the WBS delete confirmation: nothing is deleted.
#[test]
fn enter_in_the_multi_row_delete_question_deletes_nothing() {
    let (mut d, ids) = resources_view();
    let count = rsrc_rows(&d);
    click_row(&mut d, "resources", 1, Default::default());
    click_row(&mut d, "resources", 4, SHIFT);
    assert_eq!(d.app.views.resources.picked(), ids[1..]);
    let steps = d.app.history.counts().0;
    d.key(Key::Delete);
    assert!(d.app.views.records.confirm_delete.is_some(), "it asks");
    d.frames(2);
    assert_eq!(
        crate::dialog_keys::top_default_label(&d.ctx).as_deref(),
        Some("No")
    );
    d.key(Key::Enter);
    d.frames(2);
    assert!(
        d.app.views.records.confirm_delete.is_none(),
        "Enter answers"
    );
    assert_eq!(rsrc_rows(&d), count, "nothing deleted");
    assert_eq!(d.app.history.counts().0, steps, "no undo step");
}

#[test]
fn deleting_selected_resources_is_one_undo_step_and_all_or_nothing() {
    let (mut d, ids) = resources_view();
    let count = rsrc_rows(&d);
    // PIPE is assigned: a selection holding it deletes nothing.
    click_row(&mut d, "resources", 0, Default::default());
    click_row(&mut d, "resources", 3, CTRL);
    let steps = d.app.history.counts().0;
    d.app.resource_command("Delete", 1.);
    // Two resources go: the delete asks first.
    assert!(d.app.views.records.confirm_delete.is_some());
    d.frames(2);
    d.click("view_delete_yes");
    assert_eq!(rsrc_rows(&d), count, "nothing deleted");
    assert_eq!(d.app.history.counts().0, steps, "no undo step");
    d.app.error = None;
    d.frames(2);
    // PIPE.CRANE, IDLE and the two new ones go through the row menu.
    click_row(&mut d, "resources", 1, Default::default());
    click_row(&mut d, "resources", 4, SHIFT);
    assert_eq!(d.app.views.resources.picked(), ids[1..]);
    right_click_row(&mut d, "resources", 2);
    assert_eq!(d.app.views.resources.picked(), ids[1..], "kept");
    let delete = d.rect("resources_menu_delete").center();
    d.click_at(delete);
    d.frames(2);
    assert_eq!(rsrc_rows(&d), count, "the delete asks first");
    // No and Escape change nothing.
    d.click("view_delete_no");
    assert_eq!(rsrc_rows(&d), count);
    d.key(Key::Delete);
    assert!(
        d.app.views.records.confirm_delete.is_some(),
        "the Delete key asks too"
    );
    d.key(Key::Escape);
    assert!(d.app.views.records.confirm_delete.is_none());
    assert_eq!(rsrc_rows(&d), count);
    d.key(Key::Delete);
    d.click("view_delete_yes");
    d.frames(2);
    assert_eq!(rsrc_rows(&d), count - 4);
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    assert!(
        d.app.status.contains("4 resources deleted"),
        "{}",
        d.app.status
    );
    // One undo brings all four back.
    d.key_with(Key::Z, CTRL);
    assert_eq!(rsrc_rows(&d), count);
    assert!(d.app.error.is_none());
}

#[test]
fn commands_never_act_on_selected_rows_a_collapse_or_filter_hid() {
    let (mut d, ids) = resources_view();
    let count = rsrc_rows(&d);
    click_row(&mut d, "resources", 4, Default::default());
    d.key_with(Key::A, CTRL);
    assert_eq!(d.app.views.resources.picked(), ids);
    // Collapse PIPE: PIPE.CRANE (8) is no longer shown.
    let collapsed = &mut d.app.views.resources.list.collapsed;
    collapsed.insert(crate::panes::usage_list::ListKey::Resource(7));
    d.frames(2);
    assert_eq!(
        d.app.views.resources.picked(),
        [7, 9, ids[3], ids[4]],
        "the hidden child left the selection"
    );
    // Expanding again does not bring it back.
    d.app.views.resources.list.collapsed.clear();
    d.frames(2);
    assert_eq!(d.app.views.resources.picked(), [7, 9, ids[3], ids[4]]);
    // Current Project's Resources shows only PIPE: Cut takes the current
    // row alone, never the rows the filter hid.
    click_row(&mut d, "resources", 3, Default::default());
    click_row(&mut d, "resources", 4, SHIFT);
    d.app
        .views
        .resources
        .set_filter(crate::panes::usage_list::ListFilter::Project);
    d.frames(2);
    d.app.resource_command("Cut", 1.);
    assert!(
        d.app.views.resources.cut.len() <= 1,
        "{:?}",
        d.app.views.resources.cut
    );
    assert_eq!(rsrc_rows(&d), count);
    assert!(d.app.error.is_none());
}

#[test]
fn the_delete_key_never_removes_a_resource_a_collapse_or_filter_hid() {
    let (mut d, ids) = resources_view();
    let exists = |d: &Driver, id: i64| resource_edit::resource_row(&d.app.project.document, id);
    // Select PIPE.CRANE (8), then collapse PIPE above it with its expander.
    click_row(&mut d, "resources", 1, Default::default());
    assert_eq!(d.app.views.resources.selected, Some(8));
    d.click("resources_expand_0");
    d.frames(2);
    assert!(
        d.app
            .views
            .resources
            .list
            .collapsed
            .contains(&crate::panes::usage_list::ListKey::Resource(7)),
        "PIPE collapsed"
    );
    assert!(!d.has("resources_row_4"), "PIPE.CRANE is no longer shown");
    let count = rsrc_rows(&d);
    let steps = d.app.history.counts().0;
    let document = format!("{:?}", d.app.project.document);
    d.key(Key::Delete);
    d.app.error = None;
    d.frames(2);
    assert!(
        exists(&d, 8).is_some(),
        "the hidden PIPE.CRANE still exists"
    );
    assert_ne!(
        d.app.views.resources.selected,
        Some(8),
        "the current row left the hidden resource"
    );
    // PIPE, now current, is assigned and cannot go: nothing changed.
    assert_eq!(rsrc_rows(&d), count);
    assert_eq!(d.app.history.counts().0, steps, "no undo step");
    assert!(
        format!("{:?}", d.app.project.document) == document,
        "the project is unchanged"
    );
    // Cut and Copy never take it either.
    d.app.resource_command("Cut", 1.);
    assert!(!d.app.views.resources.cut.contains(&8));
    // Filter By Current Project's Resources hides a new resource that is
    // current: Delete leaves it too.
    d.app.views.resources.list.collapsed.clear();
    d.frames(2);
    click_row(&mut d, "resources", 4, Default::default());
    assert_eq!(d.app.views.resources.selected, Some(ids[4]));
    d.app
        .views
        .resources
        .set_filter(crate::panes::usage_list::ListFilter::Project);
    d.frames(2);
    let count = rsrc_rows(&d);
    d.key(Key::Delete);
    d.app.error = None;
    d.frames(2);
    assert!(
        exists(&d, ids[4]).is_some(),
        "the filtered-out resource stays"
    );
    assert_eq!(rsrc_rows(&d), count);
}

#[test]
fn paste_after_cut_moves_the_cut_resources_that_still_exist() {
    let (mut d, ids) = resources_view();
    let parent_of = |d: &Driver, id: i64| {
        crate::records::num(&resource_edit::value(
            &d.app.project.document,
            id,
            "parent_rsrc_id",
        ))
    };
    // Cut the two new resources, then delete the first of them.
    click_row(&mut d, "resources", 3, Default::default());
    click_row(&mut d, "resources", 4, SHIFT);
    d.app.resource_command("Cut", 1.);
    assert_eq!(d.app.views.resources.cut, [ids[3], ids[4]]);
    click_row(&mut d, "resources", 3, Default::default());
    d.key(Key::Delete);
    assert!(resource_edit::resource_row(&d.app.project.document, ids[3]).is_none());
    // Paste beside PIPE.CRANE: the one left moves under PIPE, as one step.
    click_row(&mut d, "resources", 1, Default::default());
    assert_eq!(d.app.views.resources.selected, Some(8));
    let steps = d.app.history.counts().0;
    d.key_with(Key::V, CTRL);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    assert_eq!(
        parent_of(&d, ids[4]),
        7,
        "moved under PIPE: {}",
        d.app.status
    );
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    assert!(d.app.views.resources.cut.is_empty(), "the cut list is done");
}

#[test]
fn cut_and_copy_of_a_parent_with_its_child_keep_the_hierarchy() {
    let (mut d, _) = resources_view();
    let doc = |d: &Driver| d.app.project.document.clone();
    let parent_of = |d: &Driver, id: i64| {
        crate::records::num(&resource_edit::value(&doc(d), id, "parent_rsrc_id"))
    };
    assert_eq!(parent_of(&d, 8), 7, "PIPE.CRANE is under PIPE");
    // Copy PIPE and PIPE.CRANE, paste beside IDLE (top level).
    d.app.views.resources.selected = Some(7);
    d.app.views.resources.rows.select(&[7, 8]);
    d.app.resource_command("Copy", 1.);
    d.app.views.resources.selected = Some(9);
    d.app.views.resources.rows.select(&[9]);
    d.app.resource_command("Paste", 2.);
    let added = d.app.views.resources.picked();
    assert_eq!(added.len(), 2, "{added:?}");
    assert_eq!(parent_of(&d, added[0]), 0, "the copy of PIPE is top level");
    assert_eq!(
        parent_of(&d, added[1]),
        added[0],
        "the copy of PIPE.CRANE is under the copy of PIPE"
    );
    // Cut PIPE and PIPE.CRANE, paste beside the copy of PIPE.CRANE: PIPE
    // goes under the copy of PIPE, PIPE.CRANE stays under PIPE.
    d.frames(2);
    d.app.views.resources.selected = Some(7);
    d.app.views.resources.rows.select(&[7, 8]);
    d.app.resource_command("Cut", 3.);
    d.app.views.resources.selected = Some(added[1]);
    d.app.views.resources.rows.select(&[added[1]]);
    d.app.resource_command("Paste", 4.);
    assert_eq!(parent_of(&d, 7), added[0], "PIPE moved");
    assert_eq!(parent_of(&d, 8), 7, "PIPE.CRANE stays under PIPE");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn copy_and_cut_take_every_selected_resource() {
    let (mut d, ids) = resources_view();
    let count = rsrc_rows(&d);
    click_row(&mut d, "resources", 3, Default::default());
    click_row(&mut d, "resources", 4, SHIFT);
    d.app.resource_command("Copy", 1.);
    assert_eq!(d.app.views.resources.clipboard.len(), 2);
    let steps = d.app.history.counts().0;
    d.app.resource_command("Paste", 2.);
    assert_eq!(rsrc_rows(&d), count + 2);
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    assert_eq!(
        d.app.views.resources.picked().len(),
        2,
        "the copies selected"
    );
    // Cut two top-level resources and paste them under PIPE.
    d.app.views.resources.selected = Some(ids[3]);
    d.app.views.resources.rows.select(&[ids[3], ids[4]]);
    d.app.resource_command("Cut", 3.);
    assert_eq!(d.app.views.resources.cut, [ids[3], ids[4]]);
    d.app.views.resources.selected = Some(8);
    d.app.resource_command("Paste", 4.);
    for id in [ids[3], ids[4]] {
        let parent = resource_edit::value(&d.app.project.document, id, "parent_rsrc_id");
        assert_eq!(parent, "7", "moved under PIPE");
    }
    assert_eq!(d.app.history.counts().0, steps + 2);
    assert!(d.app.views.resources.cut.is_empty());
}

#[test]
fn wbs_tables_select_one_row_at_a_time_and_projects_several() {
    // The WBS commands act on one row: Ctrl, Shift and Ctrl+A do not
    // select more, so nothing highlighted is left out of a command.
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    let mut d = Driver::new("multiselect-wbs", p);
    d.click("directory_wbs");
    d.frames(2);
    let count = (0..).take_while(|i| d.has(&format!("wbs_row_{i}"))).count();
    assert!(count >= 4, "the starter WBS shows {count} rows");
    click_row(&mut d, "wbs", 1, Default::default());
    click_row(&mut d, "wbs", 3, CTRL);
    assert_eq!(painted(&mut d, "wbs", count), [3]);
    d.key_with(Key::ArrowDown, SHIFT);
    assert_eq!(painted(&mut d, "wbs", count), [4]);
    let current = d.app.views.wbs.selected;
    d.open_menu("menu_bar_edit");
    d.click("edit_select_all");
    assert_eq!(painted(&mut d, "wbs", count), [4]);
    assert_eq!(d.app.views.wbs.selected, current);
    // Projects: Shift+click selects several rows, as observed in the
    // reference (Fill Down works on them); Down moves back to one row.
    d.click("directory_projects");
    d.frames(2);
    let count = (0..)
        .take_while(|i| d.has(&format!("projects_row_{i}")))
        .count();
    assert!(count >= 2, "the Projects view shows {count} rows");
    click_row(&mut d, "projects", 0, Default::default());
    click_row(&mut d, "projects", 1, SHIFT);
    assert_eq!(painted(&mut d, "projects", count), [0, 1]);
    d.key(Key::ArrowDown);
    assert_eq!(painted(&mut d, "projects", count).len(), 1);
    assert!(d.app.error.is_none());
}

#[test]
fn record_views_delete_the_selection_as_one_step_and_select_the_neighbour() {
    let mut p = openxer_xer::starter();
    let ids: Vec<i64> = (0..5)
        .map(|_| issues::add_issue(&mut p, "2026-01-05 00:00"))
        .collect();
    let mut d = Driver::new("record-delete", p);
    d.app.show_view(MainView::Issues);
    d.frames(3);
    let count = |d: &Driver| issues::issues(&d.app.project).len();
    let current = |d: &Driver| d.app.views.records.issues.list.selected;
    assert_eq!(count(&d), 5);
    // One record goes at once; the record after it becomes current.
    click_row(&mut d, "issues", 1, Default::default());
    d.key(Key::Delete);
    assert_eq!(count(&d), 4);
    assert_eq!(current(&d), Some(ids[2]), "the next record, not the first");
    // The last record: the one before it.
    click_row(&mut d, "issues", 3, Default::default());
    d.key(Key::Delete);
    assert_eq!(count(&d), 3);
    assert_eq!(current(&d), Some(ids[3]));
    // Ctrl+A, then Delete: it asks, then every record goes as one step.
    let steps = d.app.history.counts().0;
    d.key_with(Key::A, CTRL);
    d.key(Key::Delete);
    assert_eq!(count(&d), 3, "nothing yet");
    d.click("view_delete_yes");
    d.frames(2);
    assert_eq!(count(&d), 0);
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    d.key_with(Key::Z, CTRL);
    assert_eq!(count(&d), 3);
    // Insert adds a record, as the right-click menu's Add does.
    d.frames(2);
    d.key(Key::Insert);
    assert_eq!(count(&d), 4);
    assert!(d.app.error.is_none());
}

#[test]
fn edit_menu_and_keys_run_the_view_commands_in_resources_and_wbs() {
    let (mut d, _) = resources_view();
    click_row(&mut d, "resources", 2, Default::default());
    // Edit > Copy and Paste are enabled here and act on the selection.
    d.open_menu("menu_bar_edit");
    d.click("edit_copy");
    assert_eq!(d.app.views.resources.clipboard.len(), 1);
    let count = rsrc_rows(&d);
    d.key_with(Key::V, CTRL);
    assert_eq!(rsrc_rows(&d), count + 1, "Ctrl+V pastes");
    d.key_with(Key::C, CTRL);
    d.key(Key::Insert);
    assert_eq!(rsrc_rows(&d), count + 2, "Insert adds");
    // WBS: Ctrl+X then Ctrl+V on another element moves it.
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    let mut d = Driver::new("wbs-keys", p);
    d.click("directory_wbs");
    d.frames(2);
    d.app.views.wbs.selected = Some(40);
    d.key_with(Key::X, CTRL);
    assert!(d.app.views.wbs.clipboard.as_ref().is_some_and(|c| c.cut));
    d.app.views.wbs.selected = Some(30);
    d.key_with(Key::V, CTRL);
    let parent = d
        .app
        .project
        .wbs
        .iter()
        .find(|w| w.id == 40)
        .unwrap()
        .parent;
    assert_eq!(parent, Some(30), "pasted under RB.3");
    assert!(d.app.error.is_none());
}

#[test]
fn a_new_resource_takes_the_base_currency() {
    let mut p = resources::tests::with_resource();
    // The base currency (exchange rate 1), then a second one listed first.
    let t = p.document.table_mut("CURRTYPE");
    t.add(&[
        ("curr_id", "7".into()),
        ("curr_short_name", "USD".into()),
        ("curr_type", "US Dollar".into()),
        ("base_exch_rate", "1".into()),
    ]);
    let base = 7;
    t.add(&[
        ("curr_id", "991".into()),
        ("curr_short_name", "EUR".into()),
        ("curr_type", "Euro".into()),
        ("base_exch_rate", "0.92".into()),
    ]);
    let last = t.rows.pop().unwrap();
    t.rows.insert(0, last);
    assert_eq!(
        crate::enterprise::ops::base_currency(&p.document),
        Some(base)
    );
    let mut d = Driver::new("new-resource-currency", p);
    d.click("directory_resources");
    d.app.resource_command("Add", 0.);
    let id = d.app.views.resources.selected.unwrap();
    let currency = resource_edit::value(&d.app.project.document, id, "curr_id");
    assert_eq!(currency, base.to_string());
}
