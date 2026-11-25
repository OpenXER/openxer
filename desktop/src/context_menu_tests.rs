//! The shared context-menu system through the production UI: right-clicks,
//! Shift+F10, the Menu key edge detector, enablement that is read on every
//! open, and the WBS menu. egui `RawInput` only; no native window or
//! system pointer.
use super::*;
use crate::grid::layout::GroupLevel;
use crate::qa_sweep::Driver;
use std::path::PathBuf;

fn starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn index_of(app: &App, code: &str) -> usize {
    app.project
        .model
        .activities
        .iter()
        .position(|a| a.task_code == code)
        .unwrap_or_else(|| panic!("no activity {code}"))
}

/// A point in activity `code`'s Activity Name cell.
fn row(d: &Driver, code: &str) -> Pos2 {
    let task_id = d.app.project.model.activities[index_of(&d.app, code)].task_id;
    let r = d
        .app
        .bar_geometry
        .iter()
        .find(|b| b.task_id == task_id)
        .unwrap()
        .row;
    Pos2::new(d.rect("grid_header_activity_name").center().x, r.center().y)
}

fn shift() -> egui::Modifiers {
    egui::Modifiers {
        shift: true,
        ..Default::default()
    }
}

/// A point on the status bar: a click there is a click outside any menu.
fn nowhere() -> Pos2 {
    Pos2::new(700., 893.)
}

fn right_click(d: &mut Driver, pos: Pos2) {
    d.press_at(pos, egui::PointerButton::Secondary, Default::default());
}

fn left_click(d: &mut Driver, pos: Pos2) {
    d.press_at(pos, egui::PointerButton::Primary, Default::default());
}

/// The rectangles of the open context menu, by action name.
fn menu_keys(d: &Driver, prefix: &str) -> Vec<String> {
    d.app
        .action_rects
        .keys()
        .filter(|k| k.starts_with(prefix))
        .cloned()
        .collect()
}

/// Whether the item runs when clicked: an enabled item closes the menu, a
/// disabled one leaves it open. Opens nothing; the menu must be open.
fn closes_on_click(d: &mut Driver, probe: &str, item: &str) -> bool {
    let pos = d.rect(item).center();
    d.press_at(pos, egui::PointerButton::Primary, Default::default());
    !d.has(probe)
}

#[test]
fn right_click_selects_an_unselected_row_and_keeps_a_multi_selection() {
    let mut d = Driver::new("ctx-select", starter());
    d.app.selected = None;
    d.frames(2);
    let pos = row(&d, "RB-1020");
    right_click(&mut d, pos);
    assert_eq!(d.app.selected, Some(index_of(&d.app, "RB-1020")));
    assert!(d.has("ctx_activity_details"), "the menu is open");
    d.key(Key::Escape);
    left_click(&mut d, nowhere());
    // A row inside a three-row selection keeps the selection.
    let first = row(&d, "RB-1010");
    left_click(&mut d, first);
    let last = row(&d, "RB-1030");
    d.press_at(last, egui::PointerButton::Primary, shift());
    assert_eq!(d.app.selected_in_order().len(), 3);
    let middle = row(&d, "RB-1020");
    right_click(&mut d, middle);
    assert!(d.has("ctx_activity_details"));
    assert_eq!(d.app.selected_in_order().len(), 3, "selection stays");
}

#[test]
fn right_click_below_the_last_row_disables_the_selection_commands() {
    let mut d = Driver::new("ctx-empty", starter());
    let pos = row(&d, "RB-1020");
    left_click(&mut d, pos);
    assert!(d.app.selected.is_some());
    let last = d
        .app
        .bar_geometry
        .iter()
        .map(|b| b.row.bottom())
        .fold(0., f32::max);
    let below = Pos2::new(pos.x, last + 30.);
    right_click(&mut d, below);
    assert!(
        d.has("ctx_activity_details"),
        "the menu opens on empty space"
    );
    assert_eq!(d.app.selected, None, "nothing is selected there");
    for item in [
        "ctx_cut",
        "ctx_copy",
        "ctx_delete",
        "ctx_dissolve",
        "ctx_renumber",
        "ctx_link",
        "ctx_unlink",
        "ctx_assign",
    ] {
        assert!(d.has(item), "{item} is listed");
        assert!(
            !closes_on_click(&mut d, "ctx_activity_details", item),
            "{item} is disabled"
        );
        assert!(d.has("ctx_activity_details"), "the menu stays open");
    }
    let before = d.app.project.model.activities.len();
    assert!(
        closes_on_click(&mut d, "ctx_activity_details", "ctx_add"),
        "Add is enabled"
    );
    assert_eq!(d.app.project.model.activities.len(), before + 1);
    // Table-wide commands stay available.
    right_click(&mut d, below);
    for item in ["ctx_columns", "ctx_filters", "ctx_group_sort"] {
        assert!(d.has(item), "{item} is listed");
    }
    assert!(closes_on_click(
        &mut d,
        "ctx_activity_details",
        "ctx_columns"
    ));
    assert!(d.app.grid.dialogs.columns.is_some());
}

#[test]
fn a_menu_is_rebuilt_from_live_state_on_every_open() {
    let mut d = Driver::new("ctx-reeval", starter());
    let pos = row(&d, "RB-1020");
    left_click(&mut d, pos);
    right_click(&mut d, pos);
    assert!(d.has("ctx_cut"));
    d.key(Key::Escape);
    left_click(&mut d, nowhere());
    // Cut is enabled with a row selected: it runs and closes the menu.
    right_click(&mut d, pos);
    assert!(d.app.selected.is_some());
    d.key(Key::Escape);
    left_click(&mut d, nowhere());
    d.app.selected = None;
    d.app.editing.selection.clear();
    d.frames(2);
    // The same row's menu with nothing selected: right-click selects the
    // row again, so clear it after the menu is open instead.
    right_click(&mut d, pos);
    assert!(d.has("ctx_cut"));
    d.app.selected = None;
    d.frames(2);
    assert!(d.has("ctx_cut"), "still open");
    assert!(
        !closes_on_click(&mut d, "ctx_activity_details", "ctx_cut"),
        "Cut follows the cleared selection, with no stale state"
    );
    assert!(d.app.project.model.activities.len() > 2);
}

#[test]
fn shift_f10_opens_the_same_menu_as_a_right_click_and_does_not_save() {
    let mut d = Driver::new("ctx-shift-f10", starter());
    let pos = row(&d, "RB-1020");
    left_click(&mut d, pos);
    right_click(&mut d, pos);
    let by_mouse = menu_keys(&d, "ctx_");
    assert!(by_mouse.len() > 20, "{by_mouse:?}");
    d.key(Key::Escape);
    left_click(&mut d, nowhere());
    assert!(!d.has("ctx_activity_details"), "closed");
    left_click(&mut d, pos);
    d.app.status = "ready".into();
    d.app.dirty = true;
    d.app.edit_cell = None;
    d.frames(2);
    d.key_with(Key::F10, shift());
    assert!(d.has("ctx_activity_details"), "Shift+F10 opens the menu");
    assert_eq!(menu_keys(&d, "ctx_"), by_mouse);
    assert_eq!(d.app.status, "ready", "Shift+F10 does not save");
    assert!(d.app.dirty);
    d.key(Key::Escape);
    left_click(&mut d, nowhere());
    // F10 alone still saves.
    d.key(Key::F10);
    assert_eq!(d.app.status, "Saved to workspace");
    assert!(!d.has("ctx_activity_details"));
}

#[test]
fn the_menu_key_edge_fires_once_per_press() {
    let mut key = context_menu::MenuKey::default();
    let presses: Vec<bool> = [false, true, true, true, false, false, true, false]
        .into_iter()
        .map(|down| key.pressed(|| down))
        .collect();
    assert_eq!(
        presses,
        [false, true, false, false, false, false, true, false]
    );
    // The request is raised only while the window has the focus, but the
    // key state is read either way, so holding the key through a focus
    // change does not fire later.
    let mut state = context_menu::KeyState::default();
    state.poll(false, || true);
    assert!(!state.take(), "pressed while another window had the focus");
    state.poll(true, || true);
    assert!(!state.take(), "still held: no new press");
    state.poll(true, || false);
    state.poll(true, || true);
    assert!(state.take(), "a new press with the focus");
    assert!(!state.take(), "taken once");
    state.request();
    state.clear();
    assert!(!state.take(), "an untaken request is dropped");
}

#[test]
fn a_group_band_selects_first_and_opens_the_table_menu() {
    let mut d = Driver::new("ctx-band", starter());
    d.app.grid.layout.group_sort.levels = vec![GroupLevel {
        column: "activity_status".into(),
        to_level: None,
        indent: true,
        interval: String::new(),
    }];
    d.app.drop_frame_cache();
    d.frames(3);
    let band = d.rect("grid_band_0");
    let pos = Pos2::new(band.left() + 120., band.center().y);
    right_click(&mut d, pos);
    assert!(d.has("band_group_sort"), "the band menu is open");
    assert!(d.app.grid.cells.selected_band.is_some(), "the band selects");
    // The band opens the table menu itself, as the reference does.
    for item in [
        "band_activity_details",
        "band_cut",
        "band_add",
        "band_assign",
        "band_link",
        "band_expand_all",
        "band_collapse_all",
    ] {
        assert!(d.has(item), "{item}");
    }
    // The band's own Display submenu (View > Show on Top) is gone.
    for item in ["band_display", "band_add_group", "band_begin_group"] {
        assert!(!d.has(item), "{item} is gone");
    }
    // Group and Sort... opens its dialog.
    assert!(closes_on_click(
        &mut d,
        "band_group_sort",
        "band_group_sort"
    ));
    assert!(d.app.grid.dialogs.group_sort.is_some());
}

#[test]
fn shift_f10_on_a_band_opens_the_band_menu() {
    let mut d = Driver::new("ctx-band-key", starter());
    d.app.grid.layout.group_sort.levels = vec![GroupLevel {
        column: "activity_status".into(),
        to_level: None,
        indent: true,
        interval: String::new(),
    }];
    d.app.drop_frame_cache();
    d.frames(3);
    let band = d.rect("grid_band_0");
    left_click(&mut d, Pos2::new(band.left() + 120., band.center().y));
    assert!(d.app.grid.cells.selected_band.is_some());
    assert!(!d.has("band_group_sort"));
    d.key_with(Key::F10, shift());
    assert!(d.has("band_group_sort"));
}

/// The starter project under a project node, as the WBS view shows a
/// project with a root.
fn with_project_node() -> ProjectData {
    let mut doc = openxer_xer::starter().document;
    let table = doc.table_mut("PROJWBS");
    for i in 0..table.rows.len() {
        table.set(i, "parent_wbs_id", "5".into());
        table.set(i, "proj_node_flag", "N".into());
    }
    table.add(&[
        ("wbs_id", "5".into()),
        ("proj_id", "1".into()),
        ("wbs_short_name", "RIVERBEND".into()),
        ("wbs_name", "Riverbend Plant".into()),
        ("proj_node_flag", "Y".into()),
        ("parent_wbs_id", "900".into()),
        ("seq_num", "0".into()),
    ]);
    ProjectData::from_document(doc, 1, "node".into()).unwrap()
}

fn open_wbs(d: &mut Driver) {
    d.click("directory_wbs");
    d.frames(2);
}

/// Right-click WBS row `index` of the view.
fn wbs_menu_at(d: &mut Driver, index: usize) {
    let r = d.rect(&format!("wbs_row_{index}"));
    right_click(d, Pos2::new(r.left() + 20., r.center().y));
    assert!(d.has("wbs_menu_add"), "the WBS menu is open on row {index}");
}

#[test]
fn the_wbs_menu_enables_by_state() {
    let mut d = Driver::new("ctx-wbs", with_project_node());
    open_wbs(&mut d);
    // Row 0 is the project row: nothing to cut, copy or merge.
    wbs_menu_at(&mut d, 0);
    for item in [
        "wbs_menu_cut",
        "wbs_menu_copy",
        "wbs_menu_delete",
        "wbs_menu_shift_up",
        "wbs_menu_paste",
    ] {
        assert!(d.has(item), "{item} is listed");
        assert!(
            !closes_on_click(&mut d, "wbs_menu_add", item),
            "{item} is disabled on the project row"
        );
    }
    assert_eq!(d.app.views.wbs.clipboard, None);
    d.key(Key::Escape);
    // A top-level element: Cut and Copy work; Shift left has no parent.
    wbs_menu_at(&mut d, 1);
    assert!(!closes_on_click(
        &mut d,
        "wbs_menu_add",
        "wbs_menu_shift_left"
    ));
    assert!(!closes_on_click(&mut d, "wbs_menu_add", "wbs_menu_paste"));
    assert!(closes_on_click(&mut d, "wbs_menu_add", "wbs_menu_copy"));
    let clip = d
        .app
        .views
        .wbs
        .clipboard
        .clone()
        .expect("Copy fills the clipboard");
    assert!(!clip.cut);
    // With a clipboard, Paste is enabled.
    wbs_menu_at(&mut d, 1);
    assert!(closes_on_click(&mut d, "wbs_menu_add", "wbs_menu_paste"));
}

#[test]
fn the_wbs_menu_shift_items_follow_the_position() {
    let mut d = Driver::new("ctx-wbs-shift", starter());
    open_wbs(&mut d);
    let first = d.app.views.wbs.selected;
    let _ = first;
    // The first element of its level cannot shift up or right; the last
    // cannot shift down.
    wbs_menu_at(&mut d, 1);
    assert!(!closes_on_click(
        &mut d,
        "wbs_menu_add",
        "wbs_menu_shift_up"
    ));
    assert!(!closes_on_click(
        &mut d,
        "wbs_menu_add",
        "wbs_menu_shift_right"
    ));
    assert!(closes_on_click(
        &mut d,
        "wbs_menu_add",
        "wbs_menu_shift_down"
    ));
    let last = d
        .app
        .action_rects
        .keys()
        .filter(|k| k.starts_with("wbs_row_"))
        .count()
        - 1;
    wbs_menu_at(&mut d, last);
    assert!(!closes_on_click(
        &mut d,
        "wbs_menu_add",
        "wbs_menu_shift_down"
    ));
    assert!(closes_on_click(&mut d, "wbs_menu_add", "wbs_menu_shift_up"));
}

#[test]
fn the_wbs_menu_shows_and_hides_the_details_pane() {
    let mut d = Driver::new("ctx-wbs-details", starter());
    open_wbs(&mut d);
    assert!(d.app.views.wbs.show_details);
    // WBS Details is one check item that shows and hides the pane.
    wbs_menu_at(&mut d, 1);
    assert!(!d.has("wbs_menu_hide_details"), "no separate Hide item");
    assert!(closes_on_click(&mut d, "wbs_menu_add", "wbs_menu_details"));
    assert!(!d.app.views.wbs.show_details);
    wbs_menu_at(&mut d, 1);
    assert!(closes_on_click(&mut d, "wbs_menu_add", "wbs_menu_details"));
    assert!(d.app.views.wbs.show_details);
}

#[test]
fn the_context_menus_keep_their_shape() {
    let mut d = Driver::new("ctx-shape", starter());
    // The table menu: 27 entries, 21 commands and 6 separators.
    let pos = row(&d, "RB-1010");
    left_click(&mut d, pos);
    right_click(&mut d, pos);
    let table: Vec<String> = menu_keys(&d, "ctx_")
        .into_iter()
        .filter(|k| !k.starts_with("ctx_assign_"))
        .collect();
    assert_eq!(table.len(), 21, "{table:?}");
    d.key(Key::Escape);
    left_click(&mut d, nowhere());
    // The Activity Details tab strip menu: Hide first, then Customize.
    d.app.show_details = true;
    d.app.set_bottom(Some(panes::BottomView::Details));
    d.frames(3);
    let tab = d
        .app
        .action_rects
        .iter()
        .find(|(k, _)| k.starts_with("detail_tab_"))
        .map(|(_, r)| Pos2::new((r[0] + r[2]) / 2., (r[1] + r[3]) / 2.));
    if let Some(tab) = tab {
        right_click(&mut d, tab);
        let hide = d.rect("detail_menu_hide");
        let customize = d.rect("detail_menu_customize");
        assert!(hide.top() < customize.top(), "Hide, then Customize");
        assert_eq!(menu_keys(&d, "detail_menu_").len(), 2);
    }
}

/// Opens Activity Details on `tab` for the activity `code`.
fn show_tab(d: &mut Driver, code: &str, tab: DetailTab) {
    let pos = row(d, code);
    left_click(d, pos);
    d.app.show_detail_tab(tab);
    d.frames(4);
}

#[test]
fn the_steps_menu_offers_add_from_template() {
    let mut d = Driver::new("ctx-steps", starter());
    show_tab(&mut d, "RB-1010", DetailTab::Steps);
    let add = d.rect("detail_steps_add");
    // The grid sits above its button row.
    let inside = Pos2::new(add.left() + 40., add.top() - 40.);
    right_click(&mut d, inside);
    for item in [
        "steps_grid_menu_customize",
        "steps_grid_menu_show_all",
        "steps_grid_menu_copy",
        "steps_grid_menu_paste",
        "steps_grid_menu_template",
        "steps_grid_menu_from_template",
    ] {
        assert!(d.has(item), "{item} is listed");
    }
    // No template is defined in the starter project: the item stays
    // enabled, like the tab's Add from Template button (the rule of
    // `details::form::add_button`), closes the menu and opens the template
    // list empty.
    assert!(d.app.dictionaries.template_pick.is_none());
    assert!(closes_on_click(
        &mut d,
        "steps_grid_menu_customize",
        "steps_grid_menu_from_template"
    ));
    d.frames(2);
    let pick = d.app.dictionaries.template_pick.as_ref();
    assert!(pick.is_some(), "the template list opens");
    assert!(
        pick.is_some_and(|p| p.selected.is_none()),
        "with nothing in it"
    );
}

#[test]
fn the_document_menu_maps_to_the_tab_buttons() {
    let mut d = Driver::new("ctx-docs", starter());
    show_tab(&mut d, "RB-1010", DetailTab::Documents);
    let tab = d.rect("detail_tab_documents");
    let inside = Pos2::new(tab.left() + 60., tab.bottom() + 60.);
    right_click(&mut d, inside);
    for item in [
        "documents_grid_menu_customize",
        "documents_grid_menu_assign",
        "documents_grid_menu_remove",
        "documents_grid_menu_details",
    ] {
        assert!(d.has(item), "{item} is listed");
    }
    // Nothing is assigned, so Remove and Details... have no row to act on.
    for item in ["documents_grid_menu_remove", "documents_grid_menu_details"] {
        assert!(!closes_on_click(
            &mut d,
            "documents_grid_menu_customize",
            item
        ));
    }
}

#[test]
fn the_gantt_menu_still_has_four_items_and_attachments() {
    let mut d = Driver::new("ctx-gantt", starter());
    let bar = d.app.bar_geometry.first().unwrap().start;
    right_click(&mut d, bar + Vec2::new(4., 0.));
    let keys = menu_keys(&d, "gantt_menu_");
    assert!(
        keys.iter().filter(|k| !k.contains("attach")).count() >= 4,
        "{keys:?}"
    );
    for item in [
        "gantt_menu_details",
        "gantt_menu_bars",
        "gantt_menu_options",
        "gantt_menu_timescale",
        "gantt_menu_attachments",
    ] {
        assert!(d.has(item), "{item}");
    }
    assert!(!d.has("ctx_activity_details"), "not the table menu");
    let top = |k: &str| d.rect(k).top();
    assert!(top("gantt_menu_details") < top("gantt_menu_bars"));
    assert!(top("gantt_menu_timescale") < top("gantt_menu_attachments"));
}

/// An app whose every frame goes through the software painter, so it sees
/// every texture upload.
struct Shot {
    app: App,
    ctx: egui::Context,
    painter: crate::headless_render::SoftwarePainter,
    time: f64,
    dir: PathBuf,
}

impl Shot {
    fn new(name: &str, project: ProjectData) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "openxer-ctx-shot-{name}-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let app = App::new(&ctx, Store::open(dir.clone()).unwrap(), project);
        let mut shot = Self {
            app,
            ctx,
            painter: Default::default(),
            time: 0.,
            dir,
        };
        for _ in 0..4 {
            shot.frame(vec![]);
        }
        shot
    }
    fn frame(&mut self, events: Vec<egui::Event>) {
        let _ = self
            .painter
            .frame(&mut self.app, &self.ctx, &mut self.time, events, false);
    }
    fn rect(&self, action: &str) -> Rect {
        let [l, t, r, b] = self.app.action_rects[action];
        Rect::from_min_max(Pos2::new(l, t), Pos2::new(r, b))
    }
    fn press(&mut self, at: Pos2, button: egui::PointerButton) {
        for pressed in [true, false] {
            self.frame(vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button,
                    pressed,
                    modifiers: Default::default(),
                },
            ]);
        }
        for _ in 0..3 {
            self.frame(vec![]);
        }
    }
    fn save(&mut self, out: &std::path::Path, name: &str) {
        let (image, _) = self
            .painter
            .frame(&mut self.app, &self.ctx, &mut self.time, vec![], true)
            .unwrap();
        image.save(out.join(format!("{name}.png"))).unwrap();
    }
}

impl Drop for Shot {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.dir);
    }
}

/// Renders of the new menus for a look: `artifacts/ctxmenu/*.png`.
#[test]
fn render_the_new_context_menus() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/ctxmenu");
    std::fs::create_dir_all(&out).unwrap();
    let secondary = egui::PointerButton::Secondary;
    // Empty area of the activity table.
    let mut s = Shot::new("empty", starter());
    let x = s.rect("grid_header_activity_name").center().x;
    let last = s
        .app
        .bar_geometry
        .iter()
        .map(|b| b.row.bottom())
        .fold(0., f32::max);
    s.press(Pos2::new(x, last + 30.), secondary);
    assert!(s.app.action_rects.contains_key("ctx_add"));
    s.save(&out, "table-empty-area");
    // A group band.
    let mut s = Shot::new("band", starter());
    s.app.grid.layout.group_sort.levels = vec![GroupLevel {
        column: "activity_status".into(),
        to_level: None,
        indent: true,
        interval: String::new(),
    }];
    s.app.drop_frame_cache();
    for _ in 0..3 {
        s.frame(vec![]);
    }
    let band = s.rect("grid_band_0");
    s.press(Pos2::new(band.left() + 120., band.center().y), secondary);
    assert!(s.app.action_rects.contains_key("band_group_sort"));
    s.save(&out, "group-band");
    // WBS, with a project node, on a child.
    let mut s = Shot::new("wbs", with_project_node());
    let tab = s.rect("directory_wbs").center();
    s.press(tab, egui::PointerButton::Primary);
    let r = s.rect("wbs_row_1");
    s.press(Pos2::new(r.left() + 20., r.center().y), secondary);
    assert!(s.app.action_rects.contains_key("wbs_menu_add"));
    s.save(&out, "wbs");
    // Steps grid of Activity Details.
    let mut s = Shot::new("steps", starter());
    s.app.show_detail_tab(DetailTab::Steps);
    for _ in 0..4 {
        s.frame(vec![]);
    }
    let add = s.rect("detail_steps_add");
    s.press(Pos2::new(add.left() + 40., add.top() - 40.), secondary);
    assert!(s.app.action_rects.contains_key("steps_grid_menu_copy"));
    s.save(&out, "steps");
}
