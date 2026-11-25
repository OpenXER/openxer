//! Resources view editing and assignment options, driven through the
//! production UI with egui `RawInput` only (no native window).
use super::headless::{click, frame, setup, type_into};
use super::*;
use crate::details::assignments::Staffing;

fn rsrc(app: &App, id: i64, field: &str) -> String {
    super::resource_edit::value(&app.project.document, id, field)
}

fn resource_count(app: &App) -> usize {
    app.project
        .document
        .table("RSRC")
        .map_or(0, |t| t.rows.len())
}

fn undo(app: &mut App, ctx: &egui::Context, time: &mut f64) {
    app.history_move(ctx, false);
    frame(app, ctx, vec![], time);
}

#[test]
fn dock_add_copy_paste_and_delete_resources_with_one_undo_each() {
    let (path, ctx, mut app, mut time) = setup("rsrc-commands", resources::tests::with_resource());
    click(&mut app, &ctx, "directory_resources", &mut time);
    assert_eq!(app.views.resources.selected, Some(7), "PIPE is assigned");
    let count = resource_count(&app);
    let steps = app.history.steps();
    // Add.
    click(&mut app, &ctx, "add", &mut time);
    assert_eq!(resource_count(&app), count + 1);
    let added = app.views.resources.selected.unwrap();
    assert_eq!(rsrc(&app, added, "rsrc_short_name"), "R");
    assert_eq!(rsrc(&app, added, "rsrc_name"), "(New Resource)");
    assert_eq!(app.history.steps(), steps + 1, "one undo step");
    undo(&mut app, &ctx, &mut time);
    assert_eq!(resource_count(&app), count);
    // Copy and paste PIPE with its two rates.
    click(&mut app, &ctx, "directory_resources", &mut time);
    app.views.resources.selected = Some(7);
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "dock_copy", &mut time);
    click(&mut app, &ctx, "dock_paste", &mut time);
    let pasted = app.views.resources.selected.unwrap();
    assert_ne!(pasted, 7);
    assert_eq!(rsrc(&app, pasted, "rsrc_short_name"), "PIPE-1");
    assert_eq!(
        super::resource_edit::rate_rows(&app.project.document, pasted).len(),
        2
    );
    // The pasted resource is not assigned: it deletes in one step.
    let before = app.history.steps();
    click(&mut app, &ctx, "dock_delete", &mut time);
    assert!(super::resource_edit::resource_row(&app.project.document, pasted).is_none());
    assert_eq!(app.history.steps(), before + 1);
    undo(&mut app, &ctx, &mut time);
    assert!(super::resource_edit::resource_row(&app.project.document, pasted).is_some());
    // An assigned resource is refused and nothing changes.
    app.views.resources.selected = Some(7);
    frame(&mut app, &ctx, vec![], &mut time);
    let text = app.project.document.text();
    click(&mut app, &ctx, "dock_delete", &mut time);
    assert!(app
        .error
        .as_deref()
        .is_some_and(|e| e.contains("assignment")));
    assert_eq!(app.project.document.text(), text);
    app.error = None;
    // The edits survive save and reopen.
    app.save();
    let reopened = app.store.last().unwrap().unwrap();
    assert!(super::resource_edit::resource_row(&reopened.document, pasted).is_some());
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn details_and_units_and_prices_edit_the_resource_rows() {
    let (path, ctx, mut app, mut time) = setup("rsrc-details", resources::tests::with_resource());
    click(&mut app, &ctx, "directory_resources", &mut time);
    // Details: type, overtime, default units/time, calendar.
    click(&mut app, &ctx, "resources_tab_2", &mut time);
    click(&mut app, &ctx, "resource_type_nonlabor", &mut time);
    assert_eq!(rsrc(&app, 7, "rsrc_type"), "RT_Equip");
    click(&mut app, &ctx, "resource_overtime", &mut time);
    assert_eq!(rsrc(&app, 7, "ot_flag"), "Y");
    type_into(&mut app, &ctx, "resource_overtime_factor", "1.5", &mut time);
    assert_eq!(rsrc(&app, 7, "ot_factor"), "1.5");
    type_into(&mut app, &ctx, "resource_default_units", "16h/d", &mut time);
    assert_eq!(rsrc(&app, 7, "def_qty_per_hr"), "2");
    // The calendar picker opens the Calendars dialog in select mode.
    click(&mut app, &ctx, "resource_calendar_more", &mut time);
    assert!(app.calendars.as_ref().is_some_and(|c| c.select.is_some()));
    click(&mut app, &ctx, "calendars_select", &mut time);
    assert!(app.calendars.is_none(), "Select closes the dialog");
    assert_eq!(rsrc(&app, 7, "clndr_id"), "1");
    undo(&mut app, &ctx, &mut time);
    assert_eq!(rsrc(&app, 7, "def_qty_per_hr"), "1", "undone");
    assert_eq!(rsrc(&app, 7, "ot_factor"), "1.5", "the step before stays");
    // Units & Prices: select the October row, edit in place, add and delete.
    click(&mut app, &ctx, "resources_tab_3", &mut time);
    click(&mut app, &ctx, "resource_rates_row_1", &mut time);
    let rows = super::resource_edit::rate_rows(&app.project.document, 7);
    type_into(&mut app, &ctx, "resource_rate_cell_1", "24h/d", &mut time);
    type_into(&mut app, &ctx, "resource_rate_cell_2", "$60.00", &mut time);
    let get = |app: &App, k: &str| records::get(&app.project.document, "RSRCRATE", rows[1], k);
    assert_eq!(get(&app, "max_qty_per_hr"), "3");
    assert_eq!(get(&app, "cost_per_qty"), "60.0000");
    assert_eq!(app.project.resources.limits[&7][1].1, 3., "leveling input");
    click(&mut app, &ctx, "resource_rates_buttons_0", &mut time);
    assert_eq!(
        super::resource_edit::rate_rows(&app.project.document, 7).len(),
        3
    );
    click(&mut app, &ctx, "resource_rates_buttons_1", &mut time);
    assert_eq!(
        super::resource_edit::rate_rows(&app.project.document, 7).len(),
        2
    );
    // Roles: assign the file's role, then remove it.
    app.project.document.table_mut("ROLES").add(&[
        ("role_id", "4".into()),
        ("role_short_name", "PF".into()),
        ("role_name", "Pipefitter".into()),
    ]);
    click(&mut app, &ctx, "resources_tab_4", &mut time);
    click(&mut app, &ctx, "resource_roles_buttons_0", &mut time);
    // Assign opens the Roles dialog in select mode.
    assert!(app.enterprise.ui.select.is_some());
    click(&mut app, &ctx, "roles_select", &mut time);
    assert!(!app.enterprise_open(), "Select closes the dialog");
    assert_eq!(rsrc(&app, 7, "role_id"), "4");
    click(&mut app, &ctx, "resource_roles_row_0", &mut time);
    click(&mut app, &ctx, "resource_roles_buttons_1", &mut time);
    assert_eq!(rsrc(&app, 7, "role_id"), "");
    // Notes: Modify, type, Modify again commits.
    click(&mut app, &ctx, "resources_tab_5", &mut time);
    click(&mut app, &ctx, "resource_notes_modify", &mut time);
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("Union crew".into())],
        &mut time,
    );
    click(&mut app, &ctx, "resource_notes_modify", &mut time);
    assert!(rsrc(&app, 7, "rsrc_notes").contains("Union crew"));
    // The edited rows export and reimport.
    let text = app.project.updated_document().text();
    let back = openxer_xer::Document::parse(&text).unwrap();
    assert_eq!(super::resource_edit::value(&back, 7, "ot_factor"), "1.5");
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// PIPE (rsrc 7) on RB-1040 and RB-1050 at 1 unit per hour each.
fn two_activities_one_crew() -> ProjectData {
    let mut p = resources::tests::with_resource();
    p.document.table_mut("TASKRSRC").rows.clear();
    for task in [1040, 1050] {
        p.add_assignment(task, Some(7), None).unwrap();
    }
    p
}

fn leveled(app: &App) -> Vec<(i64, Option<i64>, Option<i64>)> {
    app.project
        .model
        .activities
        .iter()
        .filter(|a| [1040, 1050].contains(&a.task_id))
        .map(|a| (a.task_id, a.restart, a.reend))
        .collect()
}

#[test]
fn max_units_feed_the_next_level_and_prices_feed_recalculated_costs() {
    let (path, ctx, mut app, mut time) = setup("rsrc-level", two_activities_one_crew());
    app.open_level_resources();
    app.run_level(time);
    assert!(app.error.is_none(), "{:?}", app.error);
    let wide = leveled(&app);
    // Max units 2/h -> 1/h on the row in effect, through the UI.
    click(&mut app, &ctx, "directory_resources", &mut time);
    click(&mut app, &ctx, "resource_rates_row_1", &mut time);
    type_into(&mut app, &ctx, "resource_rate_cell_1", "8h/d", &mut time);
    app.open_level_resources();
    app.run_level(time);
    let narrow = leveled(&app);
    assert_ne!(wide, narrow, "one crew cannot work both activities at once");
    let (a, b) = (narrow[0], narrow[1]);
    assert!(a.2 <= b.1 || b.2 <= a.1, "{narrow:?}");
    // The engine leveling the exported file directly gives the same dates.
    let mut direct =
        ProjectData::from_document(app.project.updated_document(), 1, "direct".into()).unwrap();
    direct.model.options.do_level = true;
    direct.refresh_derived_durations();
    schedule_model(&mut direct.model, &direct.resources).unwrap();
    let same: Vec<_> = direct
        .model
        .activities
        .iter()
        .filter(|a| [1040, 1050].contains(&a.task_id))
        .map(|a| (a.task_id, a.restart, a.reend))
        .collect();
    assert_eq!(same, narrow);
    // A new price reaches the assignments through Recalculate Assignment Costs.
    type_into(&mut app, &ctx, "resource_rate_cell_2", "70", &mut time);
    app.open_recalc_costs();
    app.run_recalc_costs(time);
    let t = app.project.document.table("TASKRSRC").unwrap();
    for r in &t.rows {
        assert_eq!(t.value(r, "cost_per_qty"), "70");
        let units = records::float(t.value(r, "target_qty"));
        assert_eq!(records::float(t.value(r, "target_cost")), units * 70.);
    }
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn calculation_preferences_and_schedule_option_apply() {
    let (path, ctx, mut app, mut time) = setup("rsrc-prefs", two_activities_one_crew());
    // User Preferences > Calculations: the options are live and saved.
    app.open_user_preferences();
    app.prefs.tab = crate::user_prefs::PrefsTab::Calculations;
    frame(&mut app, &ctx, vec![], &mut time);
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "prefs_calc_recalculate", &mut time);
    click(&mut app, &ctx, "prefs_staff_new", &mut time);
    assert!(app.detail.calc.recalculate);
    assert_eq!(app.detail.calc.staffing, Staffing::New);
    let saved = app
        .store
        .preference(crate::details::assignments::CALC_KEY)
        .unwrap()
        .unwrap();
    assert!(saved.contains("New"), "{saved}");
    click(&mut app, &ctx, "prefs_close", &mut time);
    // Schedule Options: recalculate costs after scheduling.
    app.open_options();
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![], &mut time);
    }
    click(&mut app, &ctx, "options_recalc_costs", &mut time);
    assert!(app.sched.recalc_costs_draft);
    use crate::qa_workflows::schedule_options_close::{close_schedule_options, Frames};
    close_schedule_options(&mut Frames(&mut app, &ctx, &mut time), Some("Yes"));
    assert!(app.recalc_costs_after_scheduling());
    let doc = &mut app.project.document;
    let rows = super::resource_edit::rate_rows(doc, 7);
    super::resource_edit::set_rate_number(doc, rows[1], "cost_per_qty", 80.).unwrap();
    app.schedule();
    let t = app.project.document.table("TASKRSRC").unwrap();
    assert!(t.rows.iter().all(|r| t.value(r, "cost_per_qty") == "80"));
    assert!(app
        .log
        .iter()
        .any(|l| l.contains("Assignment costs recalculated")));
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn assignment_view_details_edit_through_the_assignment_setters() {
    let (path, ctx, mut app, mut time) = setup("assign-view", resources::tests::with_resource());
    app.show_view(MainView::Assignments);
    frame(&mut app, &ctx, vec![], &mut time);
    app.views.assignments.selected = Some(assignments::Key::Assignment(1));
    app.views.assignments.tab = 1;
    frame(&mut app, &ctx, vec![], &mut time);
    let steps = app.history.steps();
    type_into(&mut app, &ctx, "assignment_planning_0", "40", &mut time);
    let v = app.project.assignment_view(0).unwrap();
    assert_eq!(v.budgeted.units, 40.);
    assert_eq!(v.remaining.units, 40., "linked before the start");
    assert_eq!(app.history.steps(), steps + 1);
    // General: price and the resource picker.
    app.views.assignments.tab = 0;
    frame(&mut app, &ctx, vec![], &mut time);
    type_into(&mut app, &ctx, "assignment_price", "65", &mut time);
    assert_eq!(app.project.assignment_view(0).unwrap().price, 65.);
    app.detail.calc.staffing = Staffing::Ask;
    click(&mut app, &ctx, "assignment_resource_more", &mut time);
    click(&mut app, &ctx, "assignment_pick_item_2", &mut time);
    // "Ask me" asks first; Cancel leaves the assignment alone.
    assert!(app.detail.question.is_some());
    click(&mut app, &ctx, "assignment_question_cancel", &mut time);
    assert_eq!(app.project.assignment_view(0).unwrap().rsrc_id, Some(7));
    click(&mut app, &ctx, "assignment_resource_more", &mut time);
    click(&mut app, &ctx, "assignment_pick_item_2", &mut time);
    click(&mut app, &ctx, "assignment_question_ok", &mut time);
    assert_eq!(app.project.assignment_view(0).unwrap().rsrc_id, Some(9));
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn unit_currency_calendar_and_role_pickers_open_their_dictionary_dialogs_in_select_mode() {
    let (path, ctx, mut app, mut time) = setup("rsrc-pickers", resources::tests::with_resource());
    {
        let doc = &mut app.project.document;
        for (id, abbrev, name) in [(1, "ea", "Each"), (2, "hr", "Hour")] {
            doc.table_mut("UMEASURE").add(&[
                ("unit_id", id.to_string()),
                ("unit_abbrev", abbrev.into()),
                ("unit_name", name.into()),
            ]);
        }
        for (id, short, name) in [(1, "USD", "US Dollar"), (2, "EUR", "Euro")] {
            doc.table_mut("CURRTYPE").add(&[
                ("curr_id", id.to_string()),
                ("curr_short_name", short.into()),
                ("curr_type", name.into()),
                ("base_exch_rate", if id == 1 { "1" } else { "0.9" }.into()),
            ]);
        }
        doc.table_mut("ROLES").add(&[
            ("role_id", "4".into()),
            ("role_short_name", "PF".into()),
            ("role_name", "Pipefitter".into()),
        ]);
    }
    click(&mut app, &ctx, "directory_resources", &mut time);
    click(&mut app, &ctx, "resources_tab_2", &mut time);
    let steps = app.history.steps();
    let before = app.project.document.text();

    // Unit of Measure: Admin Categories on Units of Measure; Close leaves it.
    click(&mut app, &ctx, "resource_unit_more", &mut time);
    let admin = app.dictionaries.admin.as_ref().expect("Admin Categories");
    assert!(admin.select.is_some());
    assert_eq!(admin.tab, crate::dictionaries::Category::Units);
    click(&mut app, &ctx, "admin_close", &mut time);
    assert!(app.dictionaries.admin.is_none());
    assert_eq!(app.project.document.text(), before, "Close changes nothing");
    click(&mut app, &ctx, "resource_unit_more", &mut time);
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
        &mut time,
    );
    assert!(app.dictionaries.admin.is_none(), "Escape closes it");
    assert_eq!(app.project.document.text(), before);
    // Escape also leaves the Resources view without a selection.
    app.views.resources.selected = Some(7);
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "resource_unit_more", &mut time);
    click(&mut app, &ctx, "admin_cell_1_0", &mut time);
    click(&mut app, &ctx, "admin_select", &mut time);
    assert!(app.dictionaries.admin.is_none(), "Select closes the dialog");
    assert_eq!(rsrc(&app, 7, "unit_id"), "2");

    // Currency: the Currencies dialog.
    app.views.resources.selected = Some(7);
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "resource_currency_more", &mut time);
    assert!(app.enterprise.ui.select.is_some());
    click(&mut app, &ctx, "currencies_close", &mut time);
    assert!(!app.enterprise_open());
    assert_eq!(rsrc(&app, 7, "curr_id"), "");
    app.views.resources.selected = Some(7);
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "resource_currency_more", &mut time);
    app.enterprise.ui.selected = Some(2);
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "currencies_select", &mut time);
    assert!(!app.enterprise_open());
    assert_eq!(rsrc(&app, 7, "curr_id"), "2");

    // Each pick is one undo step.
    assert_eq!(app.history.steps(), steps + 2);
    undo(&mut app, &ctx, &mut time);
    assert_eq!(rsrc(&app, 7, "curr_id"), "");
    assert_eq!(rsrc(&app, 7, "unit_id"), "2");
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// An in-progress activity 1010 (started a day before the data date) with
/// PIPE's 24 hour assignment, written back and selected on the Resources tab.
fn in_progress_assignment(app: &mut App) -> (usize, i64) {
    let i = app
        .project
        .model
        .activities
        .iter()
        .position(|a| a.task_id == 1010)
        .unwrap();
    let dd = app.project.model.project.data_date.unwrap();
    {
        let a = &mut app.project.model.activities[i];
        a.status = openxer_engine::model::StatusType::InProgress;
        a.act_start = Some(dd - 1440);
    }
    crate::scheduling::schedule_model(&mut app.project.model, &app.project.resources).unwrap();
    app.project.write_assignment_schedule();
    app.touch_records();
    app.selected = Some(i);
    app.detail.rows[0] = Some(0);
    (app.project.assignment_rows(1010)[0], dd)
}

fn cell_date(app: &App, row: usize, column: &str) -> Option<i64> {
    let t = app.project.document.table("TASKRSRC").unwrap();
    openxer_xer::parse_absolute(t.value(&t.rows[row], column))
        .map(|v| v - app.project.model.origin_serial * 1440)
}

#[test]
fn resources_tab_edits_assignment_actual_and_remaining_dates() {
    let (path, ctx, mut app, mut time) = setup("rsrc-tab-dates", resources::tests::with_resource());
    let (row, dd) = in_progress_assignment(&mut app);
    click(&mut app, &ctx, "detail_tab_resources", &mut time);
    app.detail.rows[0] = Some(0);
    // Customize Resource Columns puts the four date columns up front.
    app.detail.grid_columns.insert(
        "resources".into(),
        ["Resource ID Name", "Actual Start", "Actual Finish"]
            .into_iter()
            .chain(["Remaining Start", "Remaining Finish"])
            .map(String::from)
            .collect(),
    );
    frame(&mut app, &ctx, vec![], &mut time);
    let origin = app.project.model.origin_serial;
    let stamp = |m: i64| format::date(origin, Some(m));
    assert_eq!(cell_date(&app, row, "act_start_date"), Some(dd - 1440));
    // Actual Start (column 23) takes a date between the activity's start
    // and the data date, and is one undo step.
    let steps = app.history.steps();
    type_into(
        &mut app,
        &ctx,
        "assignment_date_0_23",
        &stamp(dd - 960),
        &mut time,
    );
    assert_eq!(cell_date(&app, row, "act_start_date"), Some(dd - 960));
    assert_eq!(app.history.steps(), steps + 1);
    // A date after the data date is refused with a message and changes nothing.
    type_into(
        &mut app,
        &ctx,
        "assignment_date_0_23",
        &stamp(dd + 1440),
        &mut time,
    );
    assert_eq!(cell_date(&app, row, "act_start_date"), Some(dd - 960));
    assert!(app.status.contains("data date"), "{}", app.status);
    // Remaining Start (25) of a started activity follows the activity: it is
    // not an editor, so no cell is offered.
    assert!(!app.action_rects.contains_key("assignment_date_0_25"));
    assert!(app.action_rects.contains_key("assignment_date_0_26"));
    undo(&mut app, &ctx, &mut time);
    assert_eq!(cell_date(&app, row, "act_start_date"), Some(dd - 1440));
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn resources_tab_customize_columns_picks_and_orders_the_columns() {
    use crate::details::grid_columns::RESOURCES;
    let (path, ctx, mut app, mut time) =
        setup("rsrc-tab-columns", resources::tests::with_resource());
    let (_, _) = in_progress_assignment(&mut app);
    click(&mut app, &ctx, "detail_tab_resources", &mut time);
    let titles = |app: &App| -> Vec<String> {
        app.detail_columns(&RESOURCES, false)
            .iter()
            .map(|c| c.0.to_string())
            .collect()
    };
    assert_eq!(titles(&app), RESOURCES.default);
    let model = app.project.model.clone();
    let steps = app.history.steps();
    // Cancel changes nothing; OK keeps the choice per grid.
    app.open_column_customize(&RESOURCES);
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "detail_columns_cancel", &mut time);
    assert!(app.detail.grid_columns.is_empty());
    app.open_column_customize(&RESOURCES);
    frame(&mut app, &ctx, vec![], &mut time);
    // Drop Rate Source (selected row 3), add Units / Time (an available row).
    click(&mut app, &ctx, "detail_columns_selected_3", &mut time);
    click(&mut app, &ctx, "detail_columns_remove", &mut time);
    // Available Options hold every column not in Selected Options (the
    // default set without Rate Source), in the grid's order.
    let pos = RESOURCES
        .available
        .iter()
        .map(|c| c.0)
        .filter(|t| *t == "Rate Source" || !RESOURCES.default.contains(t))
        .position(|t| t == "Units / Time")
        .unwrap();
    click(
        &mut app,
        &ctx,
        &format!("detail_columns_available_{pos}"),
        &mut time,
    );
    click(&mut app, &ctx, "detail_columns_add", &mut time);
    click(&mut app, &ctx, "detail_columns_ok", &mut time);
    let chosen = titles(&app);
    assert!(!chosen.iter().any(|t| t == "Rate Source"));
    assert!(chosen.iter().any(|t| t == "Units / Time"));
    assert_eq!(chosen.len(), RESOURCES.default.len());
    assert!(app.project.model == model, "the project is untouched");
    assert_eq!(app.history.steps(), steps);
    // The tab draws the chosen columns, including the date column.
    frame(&mut app, &ctx, vec![], &mut time);
    // Kept in the workspace and read back; Default puts the set back.
    app.detail.grid_columns.clear();
    app.restore_grid_columns();
    assert_eq!(titles(&app), chosen);
    app.open_column_customize(&RESOURCES);
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "detail_columns_default", &mut time);
    click(&mut app, &ctx, "detail_columns_ok", &mut time);
    assert_eq!(titles(&app), RESOURCES.default);
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn assignment_window_and_view_edit_actual_dates_through_the_same_checks() {
    let (path, ctx, mut app, mut time) = setup("rsrc-win-dates", resources::tests::with_resource());
    let (row, dd) = in_progress_assignment(&mut app);
    let origin = app.project.model.origin_serial;
    let stamp = |m: i64| format::date(origin, Some(m));
    // The Resource Assignments window: General holds the four dates.
    app.open_assignment_window();
    app.detail.assignment_window.as_mut().unwrap().selected = Some(row);
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![], &mut time);
    }
    let steps = app.history.steps();
    type_into(
        &mut app,
        &ctx,
        "assignment_window_date_0",
        &stamp(dd - 900),
        &mut time,
    );
    assert_eq!(cell_date(&app, row, "act_start_date"), Some(dd - 900));
    assert_eq!(app.history.steps(), steps + 1);
    // The actual finish may not be before the actual start.
    type_into(
        &mut app,
        &ctx,
        "assignment_window_date_1",
        &stamp(dd - 1200),
        &mut time,
    );
    assert_eq!(cell_date(&app, row, "act_end_date"), None);
    assert!(
        app.status.contains("before the actual start"),
        "{}",
        app.status
    );
    click(&mut app, &ctx, "assignment_window_close", &mut time);
    assert!(app.detail.assignment_window.is_none());
    // The Resource Assignments view's Planning tab.
    app.show_view(MainView::Assignments);
    let id = crate::views::assignments::assignments(&app.project)[0].id;
    app.views.assignments.selected = Some(crate::views::assignments::Key::Assignment(id));
    app.views.assignments.tab = 1;
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![], &mut time);
    }
    type_into(
        &mut app,
        &ctx,
        "assignment_planning_date_1",
        &stamp(dd - 300),
        &mut time,
    );
    assert_eq!(cell_date(&app, row, "act_end_date"), Some(dd - 300));
    // A remaining start is not offered for a started activity (disabled).
    let before = app.project.document.text();
    type_into(
        &mut app,
        &ctx,
        "assignment_planning_date_2",
        &stamp(dd + 960),
        &mut time,
    );
    assert_eq!(app.project.document.text(), before);
    // And a F9 keeps the entered dates.
    crate::scheduling::schedule_model(&mut app.project.model, &app.project.resources).unwrap();
    app.project.write_assignment_schedule();
    assert_eq!(cell_date(&app, row, "act_start_date"), Some(dd - 900));
    assert_eq!(cell_date(&app, row, "act_end_date"), Some(dd - 300));
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}
