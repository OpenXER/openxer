//! Workflow: the WBS and Projects tables (backlog 4.2, 4.3, 4.7).
//! - WBS: the Columns chooser offers the full column list under its
//!   groups; a rolled-up column shows the same total the Activities table
//!   draws on a band of the same activities; code, name, status,
//!   responsible manager and anticipated start edit in the cells, one undo
//!   step each, and Undo takes each back.
//! - Projects: the new columns; the open project's status, leveling
//!   priority and manager edit in the cells, one undo step each; Filter By
//!   and Group and Sort By on the right-click menu.
//! - Both windows: Table Font and Row sets the window's own row height,
//!   Group and Sort By > List, Collapse To offers the window's levels, and
//!   the View menu's Expand All and Collapse All work there.
use super::*;
use crate::views::wbs_columns as wcols;

/// The scheduled sample project, with a project node over its WBS and
/// two OBS elements to pick from.
fn project() -> ProjectData {
    let mut p = views::wbs::tests::with_project_node();
    schedule_model(&mut p.model, &p.resources).unwrap();
    for (id, name) in [("71", "Qz site lead"), ("72", "Qz area lead")] {
        p.document.table_mut("OBS").add(&[
            ("obs_id", id.into()),
            ("obs_name", name.into()),
            ("parent_obs_id", String::new()),
        ]);
    }
    p
}

/// Click control `key`, drawn now (a failure names the caller's line).
#[track_caller]
fn tap(d: &mut Driver, key: &str) {
    d.frames(2);
    assert!(d.has(key), "{key} is drawn");
    d.click(key);
}

#[track_caller]
fn rect_of(d: &Driver, key: &str) -> egui::Rect {
    assert!(d.has(key), "{key} is drawn");
    d.rect(key)
}

/// Right-click row `row` of view table `table`.
#[track_caller]
fn menu_on(d: &mut Driver, table: &str, row: usize) {
    d.frames(1);
    let at = rect_of(d, &format!("{table}_row_{row}"));
    d.press_at(
        Pos2::new(at.left() + 40., at.center().y),
        egui::PointerButton::Secondary,
        Default::default(),
    );
    d.frames(2);
}

/// In the open Columns chooser: column `title` goes after the first
/// selected column, then OK. The chooser lists Available Options as one
/// alphabetical list first.
#[track_caller]
fn add_column(d: &mut Driver, title: &str) {
    assert!(d.app.detail.column_dialog.is_some(), "Columns... is open");
    tap(d, "detail_columns_available_header");
    assert!(d.hover_text("Group and Sort By"), "the header menu");
    tap(d, "available_view_list");
    d.frames(1);
    tap(d, "detail_columns_selected_0");
    assert!(pick_available(d, title), "{title} is offered");
    tap(d, "detail_columns_add");
    tap(d, "detail_columns_ok");
    d.frames(2);
    assert!(d.app.detail.column_dialog.is_none(), "OK closes it");
}

/// In the open Columns chooser: scroll Available Options until `title`
/// shows, then click it. False when the list holds no such column.
pub(super) fn pick_available(d: &mut Driver, title: &str) -> bool {
    let head = rect_of(d, "detail_columns_available_header");
    let inside = Pos2::new(head.center().x, head.bottom() + 100.);
    // The list under the header: only a title wholly inside it is clicked
    // (the same words elsewhere on screen, such as a details label, are
    // not the column).
    let list = egui::Rect::from_min_max(
        Pos2::new(head.left(), head.bottom() + 4.),
        Pos2::new(head.right(), head.bottom() + 220.),
    );
    for _ in 0..80 {
        let texts = d.texts();
        let hit = texts
            .iter()
            .find(|(t, r)| t == title && list.contains_rect(*r));
        if let Some((_, r)) = hit {
            let at = r.center();
            d.click_at(at);
            d.frames(1);
            return true;
        }
        d.frame(vec![
            egui::Event::PointerMoved(inside),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::Vec2::new(0., -54.),
                modifiers: Default::default(),
            },
        ]);
        d.frames(2);
    }
    false
}

/// Hover the submenu `caption` of the open menu.
#[track_caller]
fn sub(d: &mut Driver, caption: &str) {
    assert!(d.hover_text(caption), "the {caption} submenu");
}

#[track_caller]
fn click_cell(d: &mut Driver, table: &str, row: usize, field: usize) {
    d.frames(1);
    let x = rect_of(d, &format!("{table}_col_{field}")).center().x;
    let y = rect_of(d, &format!("{table}_row_{row}")).center().y;
    d.press_at(
        Pos2::new(x, y),
        egui::PointerButton::Primary,
        Default::default(),
    );
    d.frames(1);
}

/// Open the cell's editor: a click selects the row, a click on the
/// focused cell opens it.
#[track_caller]
fn open_cell(d: &mut Driver, table: &str, row: usize, field: usize) {
    click_cell(d, table, row, field);
    click_cell(d, table, row, field);
    assert!(
        d.has(&format!("{table}_edit")),
        "the {table} cell editor opens on row {row}, field {field}: {}",
        d.app.status
    );
}

#[track_caller]
fn type_cell(d: &mut Driver, table: &str, row: usize, field: usize, text: &str) {
    open_cell(d, table, row, field);
    d.key_with(Key::A, ctrl());
    d.type_text(text);
    d.key(Key::Enter);
    d.frames(2);
    d.assert_clean(text);
}

#[track_caller]
fn pick_cell(d: &mut Driver, table: &str, row: usize, field: usize, item: usize) {
    open_cell(d, table, row, field);
    tap(d, &format!("{table}_choice_{item}"));
    d.frames(2);
    d.assert_clean("pick");
}

/// The texts drawn in the cell at (`row`, `field`).
#[track_caller]
fn cell_texts(d: &mut Driver, table: &str, row: usize, field: usize) -> Vec<String> {
    d.frames(1);
    let column = rect_of(d, &format!("{table}_col_{field}"));
    let line = rect_of(d, &format!("{table}_row_{row}"));
    d.texts()
        .into_iter()
        .filter(|(_, r)| {
            column.x_range().contains(r.center().x) && line.y_range().contains(r.center().y)
        })
        .map(|(t, _)| t)
        .collect()
}

fn undo(d: &mut Driver) {
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
}

/// The table row whose drawn text `text` it holds.
#[track_caller]
fn row_with(d: &mut Driver, table: &str, text: &str) -> usize {
    let texts = d.texts();
    (0..60)
        .find(|r| {
            let key = format!("{table}_row_{r}");
            d.has(&key)
                && texts
                    .iter()
                    .any(|(t, rect)| t == text && d.rect(&key).contains(rect.center()))
        })
        .expect("a row of the table shows the text")
}

fn field(id: &str) -> usize {
    wcols::field_of(id).unwrap_or_else(|| panic!("{id} is a WBS column"))
}

/// The position of OBS element `obs_id` in the manager picker's list.
fn obs_item(d: &Driver, obs_id: &str) -> usize {
    let obs = views::wbs::obs_names(&d.app.project);
    obs.iter()
        .position(|o| o.0 == obs_id)
        .expect("an OBS element")
}

/// Right-click the empty table area under the rows of view table `table`.
#[track_caller]
fn menu_below_rows(d: &mut Driver, table: &str) {
    d.frames(1);
    let body = rect_of(d, &format!("{table}_body"));
    let last_bottom = (0..60)
        .filter_map(|r| {
            let key = format!("{table}_row_{r}");
            d.has(&key).then(|| d.rect(&key).bottom())
        })
        .fold(body.top(), f32::max);
    let y = (last_bottom + body.bottom()) / 2.;
    assert!(
        y > last_bottom && y < body.bottom(),
        "empty space below rows in {table}_body ({last_bottom}..{})",
        body.bottom()
    );
    d.press_at(
        Pos2::new(body.left() + 40., y),
        egui::PointerButton::Secondary,
        Default::default(),
    );
    d.frames(2);
}

/// Drag the table/chart splitter right by `dx` pixels so added columns fit.
#[track_caller]
fn widen_table(d: &mut Driver, table: &str, dx: f32) {
    let at = rect_of(d, &format!("{table}_splitter")).center();
    let to = Pos2::new(at.x + dx, at.y);
    d.frame(vec![egui::Event::PointerMoved(at)]);
    d.frame(vec![egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    }]);
    d.frame(vec![egui::Event::PointerMoved(to)]);
    d.frame(vec![egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    }]);
    d.frames(2);
}

fn projwbs(d: &Driver, id: i64, column: &str) -> String {
    views::wbs::projwbs_value(&d.app.project, id, column)
}

#[test]
fn workflow_wbs_columns_roll_up_like_the_bands_and_cells_edit_in_place() {
    let mut d = Driver::new("wf-wbs-tables", project());
    show(&mut d, "wbs");
    // WBS Details off, so the chooser's titles are the only ones drawn.
    menu_on(&mut d, "wbs", 0);
    tap(&mut d, "wbs_menu_details");
    assert!(!d.app.views.wbs.show_details);
    tap(&mut d, "wbs_layout");
    tap(&mut d, "wbs_layout_columns");
    d.frames(2);
    // The chooser's Category view groups the columns.
    assert!(d.has("detail_columns_available_group"), "group headings");
    assert!(d.hover_text("Budget"), "the first group, Budget");
    add_column(&mut d, "At Completion Total Cost");
    for title in [
        "Status",
        "Responsible Manager",
        "Anticipated Start",
        "Earned Value Cost",
    ] {
        tap(&mut d, "wbs_layout");
        tap(&mut d, "wbs_layout_columns");
        add_column(&mut d, title);
    }
    // Widen the WBS table so all eight columns fit beside the summary chart.
    widen_table(&mut d, "wbs", 560.);
    // The top row totals the whole project as an Activities band does.
    let total = |d: &Driver, id: &str| {
        let p = &d.app.project;
        let ix = d.app.grid_index();
        let column = wcols::activity_column(id).expect("an activity column");
        let all: Vec<usize> = (0..p.model.activities.len()).collect();
        let v = crate::grid::group::band_total(p, &ix, column, &all).expect("a band total");
        crate::grid::columns::display(p, None, &v)
    };
    let top = row_with(&mut d, "wbs", "Riverbend Plant");
    for id in ["at_completion_total_cost", "earned_value_cost"] {
        let want = total(&d, id);
        let shown = cell_texts(&mut d, "wbs", top, field(id));
        assert!(shown.contains(&want), "{id}: {shown:?}, band {want}");
    }

    // In-place edits on an element, one undo step each.
    let p = &d.app.project;
    let element = p
        .wbs
        .iter()
        .find(|w| !views::wbs::is_project_node(p, w.id))
        .expect("a WBS element");
    let (element, old_code, old_name) = (element.id, element.code.clone(), element.name.clone());
    let old_status = projwbs(&d, element, "status_code");
    let old_obs = projwbs(&d, element, "obs_id");
    let old_anticipated = projwbs(&d, element, "anticip_start_date");
    let area_lead = obs_item(&d, "72");
    let start = d.app.history.counts().0;
    let row = row_with(&mut d, "wbs", &old_name);
    type_cell(&mut d, "wbs", row, field("wbs_name"), "Qz north area");
    pick_cell(&mut d, "wbs", row, field("wbs_status"), 2);
    pick_cell(&mut d, "wbs", row, field("responsible_manager"), area_lead);
    type_cell(&mut d, "wbs", row, field("anticipated_start"), "12-Jan-27");
    type_cell(&mut d, "wbs", row, field("wbs_code"), "QZ9");
    let w = d.app.project.wbs.iter().find(|w| w.id == element).unwrap();
    assert_eq!((w.code.as_str(), w.name.as_str()), ("QZ9", "Qz north area"));
    assert_eq!(projwbs(&d, element, "status_code"), "WS_Inactive");
    assert_eq!(projwbs(&d, element, "obs_id"), "72");
    let anticipated = projwbs(&d, element, "anticip_start_date");
    assert!(anticipated.starts_with("2027-01-12"), "{anticipated}");
    assert_eq!(d.app.history.counts().0, start + 5, "a step a value");
    let shown = cell_texts(&mut d, "wbs", row, field("responsible_manager"));
    assert!(shown.contains(&"Qz area lead".to_string()), "{shown:?}");
    // A total column is not typed in.
    click_cell(&mut d, "wbs", row, field("earned_value_cost"));
    click_cell(&mut d, "wbs", row, field("earned_value_cost"));
    assert!(!d.has("wbs_edit"), "a rolled-up cell does not edit");

    for _ in 0..5 {
        undo(&mut d);
    }
    let w = d.app.project.wbs.iter().find(|w| w.id == element).unwrap();
    assert_eq!((w.code.clone(), w.name.clone()), (old_code, old_name));
    assert_eq!(projwbs(&d, element, "status_code"), old_status);
    assert_eq!(projwbs(&d, element, "obs_id"), old_obs);
    assert_eq!(projwbs(&d, element, "anticip_start_date"), old_anticipated);
    assert_eq!(d.app.history.counts().0, start);
}

#[test]
fn workflow_projects_columns_edit_and_filter() {
    let mut d = Driver::new("wf-projects-tables", project());
    show(&mut d, "projects");
    let id = d.app.project.model.project.name.clone();
    let row = row_with(&mut d, "projects", &id);
    for title in ["Project Leveling Priority", "Responsible Manager"] {
        menu_on(&mut d, "projects", row);
        sub(&mut d, "Columns");
        tap(&mut d, "projects_menu_customize");
        d.frames(2);
        add_column(&mut d, title);
    }
    let rollups = views::project_rollups::TITLES;
    let at = |title: &str| rollups.iter().position(|t| *t == title).unwrap();
    let pid = d.app.project.model.project.proj_id;
    let node = views::wbs::project_node(&d.app.project).expect("a project node");
    let project_value = |d: &Driver, column: &str| {
        let t = d.app.project.document.table("PROJECT").unwrap();
        let key = pid.to_string();
        let r = t.rows.iter().find(|r| t.value(r, "proj_id") == key);
        r.map(|r| t.value(r, column).to_string())
            .unwrap_or_default()
    };
    let old_priority = project_value(&d, "priority_num");
    let old_status = projwbs(&d, node, "status_code");
    let old_obs = projwbs(&d, node, "obs_id");
    let site_lead = obs_item(&d, "71");
    let start = d.app.history.counts().0;
    let row = row_with(&mut d, "projects", &id);
    let (priority, manager) = (at("Project Leveling Priority"), at("Responsible Manager"));
    type_cell(&mut d, "projects", row, priority, "7");
    let row = row_with(&mut d, "projects", &id);
    pick_cell(&mut d, "projects", row, manager, site_lead);
    let row = row_with(&mut d, "projects", &id);
    pick_cell(&mut d, "projects", row, at("Status"), 2);
    assert_eq!(project_value(&d, "priority_num"), "7");
    assert_eq!(projwbs(&d, node, "obs_id"), "71");
    assert_eq!(projwbs(&d, node, "status_code"), "WS_Inactive");
    assert_eq!(d.app.history.counts().0, start + 3, "a step a value");
    let row = row_with(&mut d, "projects", &id);
    let shown = cell_texts(&mut d, "projects", row, manager);
    assert!(shown.contains(&"Qz site lead".to_string()), "{shown:?}");

    // Filter By > Status > Active hides the now inactive project; All
    // brings it back.
    menu_on(&mut d, "projects", row);
    sub(&mut d, "Filter By");
    sub(&mut d, "Status");
    tap(&mut d, "projects_menu_filter_active");
    d.frames(2);
    assert!(d.app.views.projects.status_filter.contains("WS_Open"));
    assert!(!d.texts().iter().any(|(t, _)| *t == id), "filtered out");
    menu_below_rows(&mut d, "projects");
    sub(&mut d, "Filter By");
    tap(&mut d, "projects_menu_filter_all");
    d.frames(2);
    assert!(d.app.views.projects.status_filter.is_empty());
    let row = row_with(&mut d, "projects", &id);

    // Group and Sort By > Responsible Manager bands the project under its
    // manager; List leaves no bands.
    menu_on(&mut d, "projects", row);
    sub(&mut d, "Group and Sort By");
    tap(&mut d, "projects_menu_group_manager");
    d.frames(2);
    let band = row_with(&mut d, "projects", "Qz site lead");
    let under = row_with(&mut d, "projects", &id);
    assert!(band < under, "under its manager");
    menu_on(&mut d, "projects", 0);
    sub(&mut d, "Group and Sort By");
    tap(&mut d, "projects_menu_group_list");
    d.frames(2);
    assert_eq!(row_with(&mut d, "projects", &id), 0, "a list without bands");

    for _ in 0..3 {
        undo(&mut d);
    }
    assert_eq!(project_value(&d, "priority_num"), old_priority);
    assert_eq!(projwbs(&d, node, "obs_id"), old_obs);
    assert_eq!(projwbs(&d, node, "status_code"), old_status);
}

#[test]
fn workflow_wbs_and_projects_menus_font_group_expand_and_collapse() {
    let mut d = Driver::new("wf-wbs-projects-menus", project());
    show(&mut d, "wbs");
    // Table Font and Row: the WBS table's own row height.
    menu_on(&mut d, "wbs", 0);
    tap(&mut d, "wbs_menu_table_font");
    d.frames(2);
    tap(&mut d, "table_font_keep");
    tap(&mut d, "table_font_fixed");
    type_into(&mut d, "table_font_height", "26");
    tap(&mut d, "table_font_ok");
    d.frames(2);
    assert_eq!(d.app.views.wbs.row_height, 26.);
    assert_eq!(rect_of(&d, "wbs_row_1").height(), 26.);
    let activities = d.app.view.row_height;
    assert_eq!(activities, gantt_geometry::ROW_HEIGHT, "Activities");

    // Group and Sort By > List: every element, without the tree.
    let count = d.app.project.wbs.len();
    menu_on(&mut d, "wbs", 0);
    sub(&mut d, "Group and Sort By");
    tap(&mut d, "wbs_menu_group_list");
    d.frames(2);
    assert_eq!(d.app.views.wbs.group, views::wbs::WbsGroup::List);
    let listed = d.app.views.cells.keys.len();
    assert_eq!(listed, count, "every element once");
    assert!(!d.has("wbs_expand_0"), "no expand boxes in a list");
    menu_on(&mut d, "wbs", 0);
    sub(&mut d, "Group and Sort By");
    tap(&mut d, "wbs_menu_group_default");
    d.frames(2);

    // Collapse All and Expand All from the View menu.
    d.click("menu_bar_view");
    tap(&mut d, "menu_collapse_all");
    d.frames(2);
    assert!(!d.app.views.wbs.collapsed.is_empty(), "collapsed");
    d.click("menu_bar_view");
    tap(&mut d, "menu_expand_all");
    d.frames(2);
    assert!(d.app.views.wbs.collapsed.is_empty(), "expanded");

    // Collapse To offers the WBS levels in the single drop-down with Apply.
    menu_on(&mut d, "wbs", 0);
    tap(&mut d, "wbs_menu_collapse_to");
    d.frames(2);
    let draft = d.app.layout_opts.drafts.collapse.as_ref();
    let levels = draft.map(|c| c.levels.clone()).expect("Collapse To opens");
    assert_eq!(levels.first().map(String::as_str), Some("WBS Level 1"));
    assert_eq!(draft.unwrap().selected, None, "opens blank");
    tap(&mut d, "collapse_to_level");
    tap(&mut d, "collapse_to_level_item_0");
    tap(&mut d, "collapse_to_apply");
    d.frames(2);
    assert!(!d.app.views.wbs.collapsed.is_empty(), "collapsed");
    assert!(d.app.layout_opts.drafts.collapse.is_some(), "still open");
    tap(&mut d, "collapse_to_ok");
    d.frames(2);
    assert!(d.app.layout_opts.drafts.collapse.is_none(), "OK closes");
    d.click("menu_bar_view");
    tap(&mut d, "menu_expand_all");
    d.frames(2);

    // Projects: Table Font and Row, Collapse All and Expand All.
    show(&mut d, "projects");
    menu_on(&mut d, "projects", 0);
    tap(&mut d, "projects_menu_table_font");
    d.frames(2);
    tap(&mut d, "table_font_keep");
    tap(&mut d, "table_font_fixed");
    type_into(&mut d, "table_font_height", "22");
    tap(&mut d, "table_font_ok");
    d.frames(2);
    assert_eq!(d.app.views.projects.row_height, 22.);
    assert_eq!(d.app.views.wbs.row_height, 26., "each window keeps its own");
    menu_on(&mut d, "projects", 0);
    tap(&mut d, "projects_menu_collapse_all");
    d.frames(2);
    assert!(!d.app.views.projects.collapsed.is_empty(), "collapsed");
    menu_on(&mut d, "projects", 0);
    tap(&mut d, "projects_menu_expand_all");
    d.frames(2);
    assert!(d.app.views.projects.collapsed.is_empty(), "expanded");
}
