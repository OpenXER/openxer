//! Workflow: the second comparison pass's look fixes. Open Layout lists the
//! layouts under scope bands and gains Apply; Sort and the leveling
//! priorities share one sort-key table; the record views read "Display:"
//! and open with the reference's columns; every wizard keeps Cancel alone at
//! the bottom left; the Calendar editor and Schedule draw their own observed
//! button orders; About sits after the first Help item; the Divide Into box
//! holds its value. Layouts and views are user settings, not project data,
//! so none of these is an undo step: each scenario checks the project is
//! left unchanged instead.
use super::*;
use crate::views::record_view;
use crate::views::MainView;

/// Keys from top to bottom by their drawn rectangles.
fn top_to_bottom(d: &Driver, keys: &[&str]) -> Vec<String> {
    let mut found: Vec<(f32, String)> = keys
        .iter()
        .map(|k| (d.rect(k).top(), k.to_string()))
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|(_, k)| k).collect()
}

/// Keys from left to right by their drawn rectangles.
fn left_to_right(d: &Driver, keys: &[&str]) -> Vec<String> {
    let mut found: Vec<(f32, String)> = keys
        .iter()
        .map(|k| (d.rect(k).left(), k.to_string()))
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|(_, k)| k).collect()
}

fn has_text(d: &mut Driver, text: &str) -> bool {
    d.texts().iter().any(|(t, _)| t == text)
}

/// View > Layout > `item`.
fn layout_menu(d: &mut Driver, item: &str) {
    d.home();
    d.click("menu_bar_view");
    assert!(d.hover_text("Layout"), "View > Layout");
    d.click(item);
    d.frames(2);
}

#[test]
fn workflow_open_layout_shows_scope_bands_and_apply_keeps_it_open() {
    let mut d = fresh("qa_open_layout_apply");
    let project = d.app.project.model.clone();

    // Save Layout As: the name over a bottom row Help, Save, Cancel.
    layout_menu(&mut d, "layout_save_as");
    assert_eq!(
        left_to_right(&d, &["save_as_ok", "save_as_cancel", "save_as_help"]),
        ["save_as_help", "save_as_ok", "save_as_cancel"],
        "Save Layout As: Help, Save, Cancel"
    );
    type_into(&mut d, "save_as_name", "Crew View");
    d.click("save_as_ok");
    d.frames(2);
    assert_eq!(d.app.grid.layout.name, "Crew View", "{:?}", d.app.error);

    // Open Layout: one Layout column, the shipped layouts under Global and
    // the saved one under the user's band.
    layout_menu(&mut d, "layout_open");
    assert!(d.app.grid.dialogs.open_layout.is_some());
    assert!(has_text(&mut d, "Layout"), "the column reads Layout");
    assert!(!has_text(&mut d, "Layout Name"), "no Layout Name caption");
    assert!(has_text(&mut d, "Global"), "the Global band");
    let user = format!("User - {}", crate::reports::doc::user_name());
    assert!(has_text(&mut d, &user), "the {user} band");
    let names = d.app.layout_names();
    let at = |n: &str| names.iter().position(|x| x == n).unwrap();
    let standard = at(crate::grid::layout::STANDARD_NAME);
    let crew = at("Crew View");
    assert!(
        d.rect(&format!("open_layout_item_{standard}")).top()
            < d.rect(&format!("open_layout_item_{crew}")).top(),
        "Global comes before the user's band"
    );
    // The column: Open, Cancel, Apply, Delete, Import, Export, Help.
    assert_eq!(
        top_to_bottom(
            &d,
            &[
                "open_layout_help",
                "open_layout_export",
                "open_layout_import",
                "open_layout_delete",
                "open_layout_apply",
                "open_layout_cancel",
                "open_layout_open",
            ]
        ),
        [
            "open_layout_open",
            "open_layout_cancel",
            "open_layout_apply",
            "open_layout_delete",
            "open_layout_import",
            "open_layout_export",
            "open_layout_help",
        ]
    );

    // Apply shows the layout and keeps the window open.
    d.click(&format!("open_layout_item_{standard}"));
    d.click("open_layout_apply");
    d.frames(2);
    assert_eq!(
        d.app.grid.layout.name,
        crate::grid::layout::STANDARD_NAME,
        "Apply opens the layout"
    );
    assert!(
        d.app.grid.dialogs.open_layout.is_some(),
        "Apply leaves Open Layout up"
    );
    // Open applies and closes.
    d.click(&format!("open_layout_item_{crew}"));
    d.click("open_layout_open");
    d.frames(2);
    assert_eq!(d.app.grid.layout.name, "Crew View");
    assert!(d.app.grid.dialogs.open_layout.is_none(), "Open closes");
    // Delete still asks first.
    layout_menu(&mut d, "layout_open");
    d.click(&format!("open_layout_item_{crew}"));
    d.click("open_layout_delete");
    assert!(d.app.grid.dialogs.remove_prompt.is_some(), "Delete asks");
    d.click("remove_layout_no");
    d.click("open_layout_cancel");
    d.frames(2);
    assert_eq!(d.app.project.model, project, "the project is unchanged");
    d.assert_clean("open layout");
}

#[test]
fn workflow_sort_and_leveling_priorities_share_one_sort_table() {
    let mut d = fresh("qa_one_sort_table");
    let project = d.app.project.model.clone();
    show(&mut d, "activities");
    d.home();
    d.open_menu("menu_bar_view");
    assert!(d.hover_text("Group and Sort by"));
    d.click("view_group_sort");
    d.frames(2);
    d.click("group_sort");
    d.frames(2);
    assert!(d.app.grid.dialogs.sort.is_some(), "Sort opens");
    assert!(has_text(&mut d, "Field Name"));
    assert!(has_text(&mut d, "Sort Order"));
    // The standard column with Help last.
    assert_eq!(
        top_to_bottom(
            &d,
            &[
                "sort_help",
                "sort_down",
                "sort_up",
                "sort_delete",
                "sort_add",
                "sort_cancel",
                "sort_ok",
            ]
        ),
        [
            "sort_ok",
            "sort_cancel",
            "sort_add",
            "sort_delete",
            "sort_up",
            "sort_down",
            "sort_help",
        ]
    );
    assert_eq!(d.rect("sort_help").left(), d.rect("sort_ok").left());

    // Add a key, pick its field and order from the cell drop-downs.
    let before = d.app.grid.dialogs.sort.as_ref().unwrap().keys.len();
    d.click("sort_add");
    let n = before;
    assert!(d.has(&format!("sort_row_{n}")), "the new row is drawn");
    let catalog = crate::grid::columns::catalog(&d.app.project);
    let name = catalog
        .iter()
        .position(|c| c.id == "activity_name")
        .unwrap();
    d.click("sort_field");
    d.click(&format!("sort_field_item_{name}"));
    d.click("sort_order");
    d.click("sort_order_item_1");
    let key = d.app.grid.dialogs.sort.as_ref().unwrap().keys[n].clone();
    assert_eq!(key.column, "activity_name");
    assert!(key.descending);
    d.click("sort_ok");
    d.click("group_ok");
    d.frames(2);
    let sort = &d.app.grid.layout.group_sort.sort;
    assert_eq!(sort.last().unwrap().column, "activity_name");

    // Level Resources draws its priorities through the same table.
    d.home();
    d.open_menu("menu_bar_tools");
    d.click("menu_level_resources");
    d.frames(2);
    d.click("level_add");
    assert!(d.has("level_row_0"), "the shared table's row");
    assert!(d.has("level_field"), "the selected row's field drop-down");
    assert!(has_text(&mut d, "Field Name"));
    // Escape drops the draft.
    d.key(Key::Escape);
    d.frames(2);
    assert!(
        d.app.sched.progress.level.is_none(),
        "Level Resources closed"
    );
    assert_eq!(d.app.project.model, project, "the project is unchanged");
    d.assert_clean("sort table");
}

#[test]
fn workflow_record_views_read_display_and_open_with_the_reference_columns() {
    crate::defaults::use_reference();
    let mut d = fresh("qa_record_view_columns");
    for (view, id, display, titles) in [
        (
            MainView::Expenses,
            "expenses",
            "Display: All Expenses",
            &crate::views::expenses::TITLES[..],
        ),
        (
            MainView::Documents,
            "documents",
            "Display: All Documents",
            &crate::views::documents::TITLES[..],
        ),
        (
            MainView::Risks,
            "risks",
            "Display: All Risks",
            &crate::views::risks::TITLES[..],
        ),
        (
            MainView::Issues,
            "issues",
            "Display: All Issues",
            &crate::views::issues::TITLES[..],
        ),
        (
            MainView::Thresholds,
            "thresholds",
            "Display: All Thresholds",
            &crate::views::thresholds::TITLES[..],
        ),
    ] {
        d.app.show_view(view);
        d.frames(3);
        let texts: Vec<String> = d.texts().into_iter().map(|(t, _)| t).collect();
        assert!(texts.iter().any(|t| t == display), "{id}: {display:?}");
        for old in [
            "Layout: Expenses",
            "Layout: Risks",
            "Layout: Work Products and Documents",
        ] {
            assert!(!texts.iter().any(|t| t == old), "{id}: no {old:?}");
        }
        let shown: Vec<&str> = record_view::list_state(&mut d.app.views, id)
            .unwrap()
            .columns
            .order
            .iter()
            .map(|c| titles[*c])
            .collect();
        let want: Vec<&str> = crate::defaults::RECORD_COLUMNS
            .iter()
            .find(|(v, _)| *v == id)
            .unwrap()
            .1
            .iter()
            .copied()
            .filter(|t| titles.contains(t))
            .collect();
        assert_eq!(shown, want, "{id}: the reference's columns");
    }
    let expected_risks = [
        "Risk ID",
        "Risk Name",
        "Risk Category",
        "Risk Type",
        "Risk Status",
        "Risk Owner",
        "Identified By",
        "Identified On",
        "Project",
    ];
    let risks = &d.app.views.records.risks.columns.order;
    let titles = &crate::views::risks::TITLES;
    let shown: Vec<&str> = risks.iter().map(|c| titles[*c]).collect();
    assert_eq!(shown, expected_risks);
    assert_eq!(
        d.app.views.records.expenses.group,
        crate::views::expenses::Group::None,
        "Project Expenses starts ungrouped"
    );
    // A stored column choice loads as stored.
    let mut json: serde_json::Value = serde_json::from_str(&d.app.views.json()).unwrap();
    json["records"]["risks"]["columns"]["order"] = serde_json::json!([1, 0]);
    let mut back = crate::views::Views::default();
    back.restore(&json.to_string());
    assert_eq!(
        back.records.risks.columns.order,
        [1, 0],
        "stored order kept"
    );
    d.assert_clean("record views");
}

#[test]
fn workflow_wizards_keep_cancel_alone_at_the_bottom_left() {
    let mut d = fresh("qa_wizard_cancel_left");
    d.home();
    d.click("menu_bar_file");
    d.click("file_export");
    d.frames(2);
    let client = crate::dialog_kit::noted_client(&d.ctx, "export");
    let client = client.expect("Export is drawn");
    let cancel = d.rect("export_cancel");
    assert_eq!(
        cancel.left() - client.left(),
        crate::dialog_kit::WIZARD_CANCEL_X,
        "Cancel at the bottom left"
    );
    assert_eq!(
        left_to_right(
            &d,
            &[
                "export_finish",
                "export_next",
                "export_back",
                "export_cancel",
            ]
        ),
        [
            "export_cancel",
            "export_back",
            "export_next",
            "export_finish",
        ]
    );
    assert!(
        d.rect("export_back").left() - cancel.right() > 3. * crate::dialog_kit::ROW_GAP,
        "Cancel stands apart from Prev, Next and Finish"
    );
    d.click("export_cancel");
    d.frames(2);
    d.assert_clean("wizard row");
}

#[test]
fn workflow_calendar_editor_and_schedule_draw_their_observed_orders() {
    let mut d = fresh("qa_observed_orders");
    // Tools > Schedule: Cancel, Schedule, View Log, Help, Options....
    d.home();
    d.open_menu("menu_bar_tools");
    d.click("menu_schedule");
    d.frames(2);
    assert_eq!(
        top_to_bottom(
            &d,
            &[
                "schedule_options",
                "schedule_help",
                "schedule_view_log",
                "schedule_run",
                "schedule_cancel",
            ]
        ),
        [
            "schedule_cancel",
            "schedule_run",
            "schedule_view_log",
            "schedule_help",
            "schedule_options",
        ]
    );
    d.click("schedule_cancel");
    d.frames(2);
    assert!(!d.app.sched.schedule_open);

    // The Calendar editor: OK, Cancel, Help, then the day commands.
    d.home();
    d.click("tool_calendars");
    d.frames(2);
    d.click("calendars_modify");
    d.frames(2);
    assert_eq!(
        top_to_bottom(
            &d,
            &[
                "calendar_time_periods",
                "calendar_workweek",
                "calendar_standard",
                "calendar_nonwork",
                "calendar_work",
                "calendar_help",
                "calendar_cancel",
                "calendar_ok",
            ]
        ),
        [
            "calendar_ok",
            "calendar_cancel",
            "calendar_help",
            "calendar_work",
            "calendar_nonwork",
            "calendar_standard",
            "calendar_workweek",
            "calendar_time_periods",
        ]
    );
    d.click("calendar_cancel");
    d.click("calendars_close");
    d.frames(2);
    d.assert_clean("observed orders");
}

#[test]
fn workflow_about_follows_the_first_help_item() {
    let mut d = fresh("qa_help_menu_about");
    d.home();
    d.click("menu_bar_help");
    d.frames(1);
    assert_eq!(
        top_to_bottom(
            &d,
            &[
                "menu_whats_new",
                "menu_context_help",
                "menu_about",
                "menu_local_help",
            ]
        ),
        [
            "menu_local_help",
            "menu_about",
            "menu_context_help",
            "menu_whats_new",
        ]
    );
    d.key(Key::Escape);
    d.frames(1);
}

#[test]
fn workflow_page_setup_divide_into_holds_its_value() {
    let mut d = fresh("qa_divide_into");
    d.home();
    d.click("menu_bar_file");
    d.click("menu_page_setup");
    d.frames(2);
    for (tab, which) in [(2, "header"), (3, "footer")] {
        d.click(&format!("page_setup_tab_{tab}"));
        d.frames(2);
        let edit = d.rect(&format!("page_setup_{which}_divide"));
        let up = d.rect(&format!("page_setup_{which}_divide_up"));
        // Header and footer start in three sections.
        let value = "3";
        // The value as painted, unclipped, near the edit.
        let output = d.frame(vec![]);
        let painted: Vec<egui::Rect> = output
            .shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Text(text) if text.galley.text() == value => {
                    Some(text.visual_bounding_rect())
                }
                _ => None,
            })
            .filter(|r| (r.center().y - edit.center().y).abs() < edit.height())
            .filter(|r| r.left() >= edit.left() - 1. && r.left() <= edit.right())
            .collect();
        assert!(!painted.is_empty(), "{which}: Divide Into shows {value:?}");
        for r in painted {
            assert!(
                r.right() <= up.left() - 2.,
                "{which}: the value {value:?} ends at {:.1}, past the arrows at {:.1}",
                r.right(),
                up.left()
            );
        }
    }
    d.click("page_setup_cancel");
    d.frames(2);
    d.assert_clean("divide into");
}
