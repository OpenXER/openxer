//! The Resources window's table: Columns... over the shared resource field
//! catalog, the header sort, Display > Filter By and Group and Sort By (the
//! bottom resource lists' choices and dialogs), the type icons, the
//! multi-selection across group headings and layouts saved before the
//! columns could be chosen. Driven through the production UI with egui
//! `RawInput` only (no native window or pointer).
use super::resource_edit as edit;
use super::resources::{self as rs, ResourcesState};
use super::*;
use crate::panes::usage_list::{ListFilter, ListGroup, ListKey};
use crate::qa_sweep::Driver;

const CTRL: egui::Modifiers = egui::Modifiers::COMMAND;
const SHIFT: egui::Modifiers = egui::Modifiers::SHIFT;

/// `with_resource` (PIPE 7 labor and assigned, its child PIPE.CRANE 8
/// nonlabor, IDLE 9 material) with a role on PIPE, a Craft code (PIPE
/// PIPING, IDLE CIVIL) and an e-mail address on IDLE.
fn fixture() -> ProjectData {
    let mut doc = rs::tests::with_resource().document;
    doc.table_mut("ROLES").add(&[
        ("role_id", "3".into()),
        ("role_short_name", "FIT".into()),
        ("role_name", "Fitter".into()),
        ("parent_role_id", String::new()),
        ("seq_num", "1".into()),
    ]);
    doc.table_mut("RCATTYPE").add(&[
        ("rsrc_catg_type_id", "1".into()),
        ("rsrc_catg_type", "Craft".into()),
    ]);
    for (id, value) in [("11", "PIPING"), ("12", "CIVIL")] {
        doc.table_mut("RCATVAL").add(&[
            ("rsrc_catg_id", id.into()),
            ("rsrc_catg_type_id", "1".into()),
            ("rsrc_catg_short_name", value.into()),
        ]);
    }
    for (rsrc, value) in [("7", "11"), ("9", "12")] {
        doc.table_mut("RSRCRCAT").add(&[
            ("rsrc_id", rsrc.into()),
            ("rsrc_catg_type_id", "1".into()),
            ("rsrc_catg_id", value.into()),
        ]);
    }
    assert!(edit::set_value(&mut doc, 7, "role_id", "3".into()));
    assert!(edit::set_value(
        &mut doc,
        9,
        "email_addr",
        "stock@example.com".into()
    ));
    let mut p = ProjectData::from_document(doc, 1, "rsrc-table".into()).unwrap();
    scheduler::schedule_with_resources(&mut p.model, &p.resources).unwrap();
    p
}

/// The Resources window showing all resources.
fn window(name: &str) -> Driver {
    let mut d = Driver::new(name, fixture());
    d.click("directory_resources");
    d.app.views.resources.assigned_only = false;
    d.app.views.resources.selected = Some(7);
    d.frames(3);
    d
}

/// The rows as arranged: resource ids, and group headings by caption.
fn order(d: &Driver) -> Vec<String> {
    let lists = d.app.resource_table_lists(&rs::resources(&d.app.project));
    lists
        .rows
        .iter()
        .map(|r| match &r.key {
            ListKey::Resource(id) => format!("{id}@{}", r.depth),
            _ => format!("[{}]", r.label),
        })
        .collect()
}

/// Display bar > `submenu` > `item`.
fn bar_item(d: &mut Driver, submenu: Option<&str>, item: &str) {
    d.click("resources_display");
    d.frames(2);
    if let Some(s) = submenu {
        let hover = d.rect(&format!("resources_display_{s}")).center();
        let _ = d.frame(vec![egui::Event::PointerMoved(hover)]);
        d.frames(3);
    }
    d.click(&format!("resources_display_{item}"));
    d.frames(2);
}

fn click_row(d: &mut Driver, index: usize, modifiers: egui::Modifiers) {
    d.frames(4);
    let r = d.rect(&format!("resources_row_{index}"));
    d.press_at(
        Pos2::new(r.left() + 90., r.center().y),
        egui::PointerButton::Primary,
        modifiers,
    );
    let _ = d.frame(vec![egui::Event::PointerGone]);
}

/// The column titles drawn in the header, left to right.
fn header_titles(d: &mut Driver) -> Vec<String> {
    let head = d.rect("resources_col_0");
    let mut found: Vec<(String, Rect)> = d
        .texts()
        .into_iter()
        .filter(|(_, r)| (r.center().y - head.center().y).abs() < 6.)
        .collect();
    found.sort_by(|a, b| a.1.left().total_cmp(&b.1.left()));
    found.into_iter().map(|(t, _)| t).collect()
}

/// A sort arrow (a filled triangle) drawn inside `cell`.
fn arrow_in(d: &mut Driver, cell: Rect) -> bool {
    let output = d.frame(vec![]);
    fn walk(shape: &egui::Shape, cell: Rect) -> bool {
        match shape {
            egui::Shape::Path(p) => {
                p.closed && p.points.len() == 3 && p.points.iter().all(|q| cell.contains(*q))
            }
            egui::Shape::Vec(list) => list.iter().any(|s| walk(s, cell)),
            _ => false,
        }
    }
    output.shapes.iter().any(|s| walk(&s.shape, cell))
}

#[test]
fn columns_add_remove_reorder_and_restore() {
    let mut d = window("rt-columns");
    let default: Vec<&str> = rs::DEFAULT_COLUMNS.iter().map(|c| c.0).collect();
    assert_eq!(d.app.views.resources.fields, default);
    // Columns... is in the right-click menu and the Display bar.
    let r = d.rect("resources_row_0");
    d.press_at(
        Pos2::new(r.left() + 90., r.center().y),
        egui::PointerButton::Secondary,
        Default::default(),
    );
    // Columns > Customize..., as in the reference.
    let hover = d.rect("resources_menu_columns_menu").center();
    let _ = d.frame(vec![egui::Event::PointerMoved(hover)]);
    d.frames(3);
    d.click("resources_menu_columns");
    let dialog = d
        .app
        .detail
        .column_dialog
        .clone()
        .expect("Columns... opens");
    assert_eq!(dialog.caption, "Resource");
    for title in [
        "Calendar",
        "Currency",
        "Overtime Factor",
        "Auto Compute Actuals",
        "Calculate costs from units",
        "E-Mail Address",
        "Parent Resource",
        "Craft",
    ] {
        assert!(
            dialog.available.iter().any(|t| t == title),
            "{title} offered"
        );
    }
    assert_eq!(dialog.selected[5], "Max Units/Time");
    d.click("detail_columns_cancel");
    assert!(d.app.detail.column_dialog.is_none());
    bar_item(&mut d, None, "columns");
    // Add Calendar and Craft, drop Unit of Measure, move Resource Name first.
    let dlg = d
        .app
        .detail
        .column_dialog
        .as_mut()
        .expect("the bar opens it");
    dlg.selected.retain(|t| t != "Unit of Measure");
    dlg.selected.push("Calendar".into());
    dlg.selected.push("Craft".into());
    dlg.selected.swap(0, 1);
    d.click("detail_columns_ok");
    let fields = d.app.views.resources.fields.clone();
    assert_eq!(
        fields,
        [
            "resource_name",
            "resource_id",
            "resource_type",
            "primary_role",
            "max_units",
            "calendar",
            "rcode_1"
        ]
    );
    d.frames(2);
    let titles = header_titles(&mut d);
    assert_eq!(titles[0], "Resource Name");
    assert!(titles.iter().any(|t| t == "Craft"), "{titles:?}");
    assert!(!titles.iter().any(|t| t == "Unit of Measure"), "{titles:?}");
    // The code column shows the value, the role its name.
    let texts: Vec<String> = d.texts().into_iter().map(|t| t.0).collect();
    assert!(texts.iter().any(|t| t == "PIPING"));
    assert!(texts.iter().any(|t| t == "Fitter"));
    // A resized column keeps its width through a save and restore.
    d.app.views.resources.columns.widths[0] = 222.;
    d.frames(2);
    let json = d.app.views.resources.json();
    let mut back = ResourcesState::default();
    back.restore(&json);
    assert_eq!(back.fields, fields);
    assert_eq!(back.columns.widths[0], 222.);
    // Through the user-level views layout too.
    let saved = d.app.views.json();
    let mut e = window("rt-columns-restore");
    e.app.views.restore(&saved);
    e.frames(2);
    assert_eq!(e.app.views.resources.fields, fields);
    assert!(header_titles(&mut e).iter().any(|t| t == "Calendar"));
    // Removing a column the header sorts by clears the sort.
    d.app.views.resources.list.sort.field = Some("calendar".into());
    d.app.views.resources.set_fields(vec!["resource_id".into()]);
    assert!(d.app.views.resources.list.sort.field.is_none());
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn a_header_click_sorts_up_then_down_and_clear_sort_restores() {
    let mut d = window("rt-sort");
    bar_item(&mut d, Some("group_by"), "group_list");
    assert_eq!(order(&d), ["9@0", "7@0", "8@0"], "a list is by Resource ID");
    // Resource Name: Crane, Pipefitter, Unused.
    d.click("resources_col_1");
    assert_eq!(
        d.app.views.resources.list.sort.field.as_deref(),
        Some("resource_name")
    );
    assert_eq!(order(&d), ["8@0", "7@0", "9@0"]);
    let cell = d.rect("resources_col_1");
    assert!(arrow_in(&mut d, cell), "the sorted column shows its arrow");
    d.click("resources_col_1");
    assert!(d.app.views.resources.list.sort.descending);
    assert_eq!(order(&d), ["9@0", "7@0", "8@0"]);
    // In the hierarchy the sort applies within each level.
    bar_item(&mut d, Some("group_by"), "group_hierarchy");
    assert_eq!(order(&d), ["9@0", "7@0", "8@1"]);
    // A number column sorts as numbers, its arrow left of the title.
    d.click("resources_col_5");
    assert_eq!(
        d.app.views.resources.list.sort.field.as_deref(),
        Some("max_units")
    );
    let cell = d.rect("resources_col_5");
    assert!(arrow_in(&mut d, cell));
    bar_item(&mut d, Some("group_by"), "sort_clear");
    assert!(d.app.views.resources.list.sort.field.is_none());
    assert_eq!(order(&d), ["7@0", "8@1", "9@0"]);
    assert!(!arrow_in(&mut d, cell));
}

#[test]
fn group_and_sort_by_arranges_the_table_like_the_bottom_lists() {
    let mut d = window("rt-group");
    // Defaults: all resources as the hierarchy, the current project's as a
    // flat list.
    assert_eq!(d.app.views.resources.group(), ListGroup::Hierarchy);
    assert_eq!(order(&d), ["7@0", "8@1", "9@0"]);
    bar_item(&mut d, Some("filter_by"), "display_project");
    assert_eq!(d.app.views.resources.group(), ListGroup::List);
    assert_eq!(order(&d), ["7@0"]);
    bar_item(&mut d, Some("filter_by"), "display_all");
    bar_item(&mut d, Some("group_by"), "group_list");
    assert_eq!(order(&d), ["9@0", "7@0", "8@0"]);
    bar_item(&mut d, Some("group_by"), "group_type");
    assert_eq!(
        order(&d),
        ["[Labor]", "7@1", "[Nonlabor]", "8@1", "[Material]", "9@1"]
    );
    // A chosen grouping stays when the filter changes.
    bar_item(&mut d, Some("filter_by"), "display_project");
    assert_eq!(order(&d), ["[Labor]", "7@1"]);
    bar_item(&mut d, Some("filter_by"), "display_all");
    bar_item(&mut d, Some("group_by"), "group_role");
    assert_eq!(
        order(&d),
        ["[FIT - Fitter]", "7@1", "[(No Primary Role)]", "9@1", "8@1"]
    );
    // Group headings collapse and expand.
    d.click("resources_expand_0");
    assert_eq!(
        order(&d),
        ["[FIT - Fitter]", "[(No Primary Role)]", "9@1", "8@1"]
    );
    bar_item(&mut d, None, "collapse_all");
    assert_eq!(order(&d), ["[FIT - Fitter]", "[(No Primary Role)]"]);
    bar_item(&mut d, None, "expand_all");
    assert_eq!(order(&d).len(), 5);
    // Resource codes: the bottom lists' Customize... dialog.
    bar_item(&mut d, Some("group_by"), "group_custom");
    let levels = &mut d.app.panes.usage_list_group.as_mut().expect("it opens").1;
    levels[0].code_type = "Craft".into();
    d.click("usage_list_group_ok");
    assert_eq!(d.app.views.resources.group(), ListGroup::Custom);
    assert_eq!(
        order(&d),
        [
            "[Craft: CIVIL]",
            "9@1",
            "[Craft: PIPING]",
            "7@1",
            "[(No Craft)]",
            "8@1"
        ]
    );
    // The choice is saved and wins over the filter's default.
    let mut back = ResourcesState::default();
    back.restore(&d.app.views.resources.json());
    assert!(back.assigned_only == d.app.views.resources.assigned_only);
    assert_eq!(back.group(), ListGroup::Custom);
    assert_eq!(back.list.levels[0].code_type, "Craft");
    let mut chosen = ResourcesState::default();
    chosen.restore(&serde_json::json!({"assigned_only": true, "group": "type"}));
    assert_eq!(chosen.group(), ListGroup::Type);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn filter_by_customize_uses_the_bottom_lists_filter() {
    let mut d = window("rt-filter");
    bar_item(&mut d, Some("filter_by"), "filter_custom");
    let f = &mut d.app.panes.usage_list_filter.as_mut().expect("it opens").1;
    f.text = "pipe".into();
    d.click("usage_list_filter_ok");
    assert_eq!(d.app.views.resources.filter(), ListFilter::Custom);
    assert!(d
        .texts()
        .iter()
        .any(|(t, _)| t == "Display: Custom Resource Filter"));
    assert_eq!(
        order(&d),
        ["7@0", "8@1"],
        "a custom filter shows the hierarchy"
    );
    // Types and a code value.
    let state = &mut d.app.views.resources.list.custom;
    state.text.clear();
    state.labor = false;
    state.nonlabor = false;
    d.frames(2);
    assert_eq!(order(&d), ["9@0"]);
    let state = &mut d.app.views.resources.list.custom;
    state.labor = true;
    state.code_type = "Craft".into();
    state.code_value = "PIPING".into();
    d.frames(2);
    assert_eq!(order(&d), ["7@0"]);
    // Cancel leaves the filter alone; All Resources turns it off.
    bar_item(&mut d, Some("filter_by"), "filter_custom");
    d.click("usage_list_filter_cancel");
    assert_eq!(d.app.views.resources.filter(), ListFilter::Custom);
    bar_item(&mut d, Some("filter_by"), "display_all");
    assert_eq!(d.app.views.resources.filter(), ListFilter::All);
    assert_eq!(order(&d).len(), 3);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn each_resource_type_has_its_own_icon() {
    let p = fixture();
    let list = rs::resources(&p);
    let mut d = window("rt-icons");
    let lists = d.app.resource_table_lists(&list);
    let fields: Vec<String> = rs::DEFAULT_COLUMNS.iter().map(|c| c.0.into()).collect();
    let (keys, rows) = rs::table_rows(&p, &list, &lists, &fields, &rs::role_names(&p));
    let icon = |id: i64| {
        let i = keys.iter().position(|k| k.resource() == Some(id)).unwrap();
        rows[i].icon
    };
    assert_eq!(icon(7), Some(chrome::Icon::Labor));
    assert_eq!(icon(8), Some(chrome::Icon::Nonlabor));
    assert_eq!(icon(9), Some(chrome::Icon::Material));
    // The Material icon is an image of its own.
    let names: std::collections::BTreeSet<&str> = [
        chrome::Icon::Labor,
        chrome::Icon::Nonlabor,
        chrome::Icon::Material,
    ]
    .map(chrome::Icon::asset_name)
    .into();
    assert_eq!(names.len(), 3);
    assert_eq!(chrome::Icon::Material.asset_name(), "OXMATRSRC16");
    d.frames(2);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn multi_select_skips_group_headings() {
    let mut d = window("rt-multiselect");
    bar_item(&mut d, Some("group_by"), "group_type");
    // [Labor] PIPE [Nonlabor] CRANE [Material] IDLE
    click_row(&mut d, 1, Default::default());
    assert_eq!(d.app.views.resources.picked(), [7]);
    click_row(&mut d, 5, SHIFT);
    assert_eq!(
        d.app.views.resources.picked(),
        [7, 8, 9],
        "headings skipped"
    );
    // A heading is not a resource: a click on it changes nothing.
    click_row(&mut d, 2, CTRL);
    assert_eq!(d.app.views.resources.picked(), [7, 8, 9]);
    click_row(&mut d, 3, CTRL);
    assert_eq!(d.app.views.resources.picked(), [7, 9]);
    // Ctrl+A selects every resource row shown.
    d.key_with(egui::Key::A, CTRL);
    d.frames(2);
    assert_eq!(d.app.views.resources.picked(), [7, 8, 9]);
    // A collapsed group keeps its resources out of a Shift range.
    click_row(&mut d, 1, Default::default());
    d.click("resources_expand_2");
    click_row(&mut d, 4, SHIFT);
    assert_eq!(order(&d)[3], "[Material]");
    assert_eq!(d.app.views.resources.picked(), [7, 9]);
    // The right-click menu of a heading has no resource commands.
    let r = d.rect("resources_row_0");
    d.press_at(
        Pos2::new(r.left() + 90., r.center().y),
        egui::PointerButton::Secondary,
        Default::default(),
    );
    d.frames(2);
    assert!(d.has("resources_menu_columns_menu"));
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn layouts_saved_with_the_six_fixed_columns_restore() {
    let old = serde_json::json!({
        "resources": {
            "columns": {"order": [1, 0, 2, 3, 4, 5],
                        "widths": [140.0, 210.0, 100.0, 90.0, 150.0, 60.0]},
            "tab": 3,
            "show_bottom": true,
            "assigned_only": true,
            "rate_columns": []
        }
    });
    let mut state = ResourcesState::default();
    state.restore(&old["resources"]);
    assert_eq!(
        state.fields,
        [
            "resource_name",
            "resource_id",
            "resource_type",
            "unit",
            "primary_role",
            "max_units"
        ]
    );
    assert_eq!(state.columns.widths, [210., 140., 100., 90., 150., 60.]);
    assert_eq!(
        state.group(),
        ListGroup::List,
        "no saved grouping: the default"
    );
    assert_eq!(state.filter(), ListFilter::Project);
    // Broken values keep the defaults.
    let mut broken = ResourcesState::default();
    broken.restore(
        &serde_json::json!({"columns": {"order": [0, 0, 9]}, "fields": [],
        "group": "nonsense", "list": {"sort": {"field": "nonsense"}}}),
    );
    assert_eq!(broken.fields.len(), 6);
    assert!(broken.list.sort.field.is_none());
    assert_eq!(broken.group, None);
    let mut d = window("rt-old-json");
    d.app.views.restore(&old.to_string());
    d.frames(3);
    assert_eq!(header_titles(&mut d)[0], "Resource Name");
    assert_eq!(order(&d), ["7@0"]);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

/// Renders to the ignored `artifacts/resources-table/`: the table grouped
/// by Resource Type with extra columns, and the Columns dialog.
#[test]
fn render_the_resources_table() {
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/resources-table");
    std::fs::create_dir_all(&out).unwrap();
    let mut d = Driver::probed(
        "rt-render",
        fixture(),
        egui::Vec2::new(1500., 900.),
        Some(out.clone()),
    );
    d.click("directory_resources");
    let s = &mut d.app.views.resources;
    s.assigned_only = false;
    s.selected = Some(7);
    s.group = Some(ListGroup::Type);
    s.list.sort.field = Some("resource_name".into());
    s.set_fields(
        [
            "resource_id",
            "resource_name",
            "resource_type",
            "unit",
            "primary_role",
            "max_units",
            "default_units",
            "price",
            "calendar",
            "email",
            "rcode_1",
        ]
        .map(String::from)
        .to_vec(),
    );
    d.frames(3);
    d.screenshot(&out.join("grouped-by-type.png"));
    d.click("resources_display");
    d.frames(2);
    let hover = d.rect("resources_display_group_by").center();
    let _ = d.frame(vec![egui::Event::PointerMoved(hover)]);
    d.frames(3);
    d.screenshot(&out.join("display-menu.png"));
    d.key(egui::Key::Escape);
    d.frames(2);
    d.app.open_resource_columns();
    d.frames(3);
    d.screenshot(&out.join("columns-dialog.png"));
    d.key(egui::Key::Escape);
    d.frames(2);
    assert!(d.app.detail.column_dialog.is_none(), "Escape closes it");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn a_large_dictionary_is_not_rebuilt_on_an_unchanged_frame() {
    // 3,000 resources with two price and limit rows each.
    let mut p = rs::tests::with_resource();
    for n in 0..3000_i64 {
        let id = 1000 + n;
        p.document.table_mut("RSRC").add(&[
            ("rsrc_id", id.to_string()),
            ("rsrc_short_name", format!("R{n:04}")),
            ("rsrc_name", format!("Resource {n}")),
            ("rsrc_type", "RT_Labor".into()),
            ("rsrc_seq_num", id.to_string()),
            (
                "parent_rsrc_id",
                if n % 10 == 0 {
                    String::new()
                } else {
                    (1000 + n / 10 * 10).to_string()
                },
            ),
        ]);
        for (k, date) in ["2026-01-01 00:00", "2026-06-01 00:00"].iter().enumerate() {
            p.document.table_mut("RSRCRATE").add(&[
                ("rsrc_rate_id", (10_000 + 2 * id + k as i64).to_string()),
                ("rsrc_id", id.to_string()),
                ("start_date", date.to_string()),
                ("max_qty_per_hr", format!("{}", 1 + k)),
                ("cost_per_qty", format!("{}", 50 + n % 7)),
            ]);
        }
    }
    let mut d = Driver::new("resources-memo", p);
    d.click("directory_resources");
    d.app.views.resources.set_filter(ListFilter::All);
    d.frames(3);
    let built = rs::ROW_BUILDS.with(|n| n.get());
    d.frames(5);
    assert_eq!(
        rs::ROW_BUILDS.with(|n| n.get()),
        built,
        "an unchanged frame does not rebuild the rows"
    );
    // The kept rows are the ones a fresh build gives.
    let app = &d.app;
    let list = rs::resources(&app.project);
    let lists = app.resource_table_lists(&list);
    let roles = rs::role_names(&app.project);
    let (keys, rows) = rs::table_rows(
        &app.project,
        &list,
        &lists,
        &app.views.resources.fields,
        &roles,
    );
    assert!(keys.len() > 3000);
    let shown: Vec<Vec<String>> = rows.iter().map(|r| r.cells.clone()).collect();
    let state = app.views.resources.list_state();
    let kept: std::rc::Rc<(Vec<ListKey>, Vec<grid::Row>)> = app.memo(
        app.project_serial(),
        "resources_rows",
        (
            (state, app.open_set.parked.len()),
            app.views.resources.fields.clone(),
            format!("{:?}", crate::format::current()),
        ),
        || unreachable!("kept from the last frame"),
    );
    assert_eq!(kept.0, keys);
    assert_eq!(
        kept.1.iter().map(|r| r.cells.clone()).collect::<Vec<_>>(),
        shown
    );
    // An edit rebuilds once.
    d.app.views.resources.selected = Some(1000);
    d.app.resource_command("Add", 1.);
    d.frames(3);
    let after = rs::ROW_BUILDS.with(|n| n.get());
    assert!(after > built);
    d.frames(3);
    assert_eq!(rs::ROW_BUILDS.with(|n| n.get()), after);
}

#[test]
fn columns_wider_than_the_table_scroll_sideways() {
    let mut d = Driver::new("resources-hscroll", fixture());
    d.click("directory_resources");
    let all: Vec<String> = crate::panes::usage_fields::available(
        &d.app.project,
        crate::panes::usage_fields::Which::Resource,
    )
    .into_iter()
    .map(|(id, _)| id)
    .collect();
    assert!(all.len() > 12, "the catalog offers {} fields", all.len());
    d.app.views.resources.set_fields(all.clone());
    d.frames(3);
    assert!(
        d.has("resources_columns_hscroll"),
        "a scroll bar under the table"
    );
    let bar = d.rect("resources_columns_hscroll");
    let last = all.len() - 1;
    // Scrolled to the end, the last column shows inside the table.
    d.app.views.resources.columns.scroll_x = 1.0e6;
    d.frames(2);
    let end = d.rect(&format!("resources_col_{last}"));
    assert!(
        end.right() <= bar.right() + 1. && end.width() > 20.,
        "the last column {end:?} is reachable ({bar:?})"
    );
    let scrolled = d.app.views.resources.columns.scroll_x;
    assert!(scrolled > 0. && scrolled < 1.0e6, "clamped: {scrolled}");
    // The right arrow of the bar at the end changes nothing; the left one
    // scrolls back.
    let at = Pos2::new(bar.left() + 4., bar.center().y);
    d.press_at(at, egui::PointerButton::Primary, Default::default());
    d.frames(2);
    assert!(d.app.views.resources.columns.scroll_x < scrolled);
    // Narrow columns: no bar, no offset.
    d.app
        .views
        .resources
        .set_fields(vec!["resource_id".into(), "resource_name".into()]);
    d.frames(2);
    assert_eq!(d.app.views.resources.columns.scroll_x, 0.);
}
