//! Production-UI tests of the project record views (Expenses, WPs & Docs,
//! Risks, Issues, Thresholds with Monitor Thresholds and the Issue
//! Navigator, Tracking). They drive `App::update_ui` through egui
//! `RawInput` only, check every add, edit and delete against the XER tables,
//! undo and an export round trip, and render each view and tab to the
//! ignored `artifacts/`.
use super::expenses::Group;
use super::*;
use crate::records;

fn frame(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>, time: &mut f64) {
    *time += 0.1;
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500., 900.))),
            time: Some(*time),
            events,
            focused: true,
            ..Default::default()
        },
        |ctx| app.update_ui(ctx),
    );
}

fn press(app: &mut App, ctx: &egui::Context, pos: Pos2, time: &mut f64) {
    for pressed in [true, false] {
        frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
            time,
        );
    }
    frame(app, ctx, vec![egui::Event::PointerGone], time);
    frame(app, ctx, vec![], time);
}

fn click_text(app: &mut App, ctx: &egui::Context, text: &str, time: &mut f64) {
    *time += 0.1;
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500., 900.))),
            time: Some(*time),
            focused: true,
            ..Default::default()
        },
        |ctx| app.update_ui(ctx),
    );
    let pos = output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::Shape::Text(shape) if shape.galley.text() == text => {
                Some(shape.visual_bounding_rect().center())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("No visible UI text {text:?}"));
    press(app, ctx, pos, time);
}

fn click(app: &mut App, ctx: &egui::Context, action: &str, time: &mut f64) {
    if !app.action_rects.contains_key(action) {
        if let Some(menu) = crate::menu::menu_holding(action) {
            click_text(app, ctx, menu, time);
        }
    }
    let [l, t, r, b] = *app
        .action_rects
        .get(action)
        .unwrap_or_else(|| panic!("No rendered control {action:?}"));
    press(app, ctx, Pos2::new((l + r) / 2., (t + b) / 2.), time);
}

/// Click a field, replace its text and press Enter.
fn type_into(app: &mut App, ctx: &egui::Context, action: &str, text: &str, time: &mut f64) {
    type_text(app, ctx, action, text, time);
    let key = egui::Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    };
    frame(app, ctx, vec![key], time);
    frame(app, ctx, vec![], time);
}

/// Click a field or memo and replace its text.
fn type_text(app: &mut App, ctx: &egui::Context, action: &str, text: &str, time: &mut f64) {
    click(app, ctx, action, time);
    let key = |key, modifiers| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    frame(
        app,
        ctx,
        vec![
            key(Key::A, egui::Modifiers::COMMAND),
            egui::Event::Text(text.into()),
        ],
        time,
    );
    frame(app, ctx, vec![], time);
}

fn setup(name: &str, project: ProjectData) -> (PathBuf, egui::Context, App, f64) {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "openxer-records-{name}-{}-{unique}.db",
        std::process::id()
    ));
    let mut store = Store::open(path.clone()).unwrap();
    store.save(&project).unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx, store, project);
    let mut time = 0.;
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![], &mut time);
    }
    (path, ctx, app, time)
}

/// Undo until `done` holds (at most 40 steps).
fn undo_until(app: &mut App, ctx: &egui::Context, done: impl Fn(&App) -> bool) {
    for _ in 0..40 {
        if done(app) {
            return;
        }
        app.history_move(ctx, false);
    }
    assert!(done(app), "undo did not get back");
}

fn exported(app: &App) -> Document {
    Document::parse(&app.project.updated_document().text()).unwrap()
}

fn cell(doc: &Document, table: &str, key: &str, id: i64, column: &str) -> String {
    record_view::value(doc, table, key, id, column)
}

/// Table position in an exported document.
fn rank(doc: &Document, table: &str) -> usize {
    doc.tables
        .iter()
        .position(|t| t.name == table)
        .unwrap_or_else(|| panic!("{table} is exported"))
}

#[test]
fn expenses_view_lists_adds_edits_deletes_with_undo_and_export() {
    let mut p = resources::tests::with_resource();
    p.document.table_mut("COSTTYPE").add(&[
        ("cost_type_id", "3".into()),
        ("cost_type", "Rentals".into()),
    ]);
    records::add_row(
        &mut p.document,
        "PROJCOST",
        &[
            ("cost_item_id", "1".into()),
            ("proj_id", "1".into()),
            ("task_id", "1010".into()),
            ("cost_type_id", "3".into()),
            ("cost_name", "Crane rental".into()),
            ("target_cost", "500".into()),
            ("act_cost", "100".into()),
            ("remain_cost", "400".into()),
            ("cost_load_type", "CL_Uniform".into()),
            ("vendor_name", "Lift Co".into()),
        ],
    );
    let (path, ctx, mut app, mut time) = setup("expenses", p);
    click(&mut app, &ctx, "dock_expenses", &mut time);
    assert_eq!(app.views.main, MainView::Expenses);
    // One WBS band and the item under it.
    assert!(app.action_rects.contains_key("expenses_row_1"));
    assert!(!app.action_rects.contains_key("expenses_row_2"));
    let list = expenses::expenses(&app.project);
    assert_eq!(list.len(), 1);
    let e = &list[0];
    assert_eq!(
        (e.category.as_str(), e.accrual.as_str(), e.at_completion()),
        ("Rentals", "Uniform", 500.)
    );
    assert_eq!(e.activity_id, "RB-1010");
    // Add goes on the selected item's activity and selects the new item.
    click(&mut app, &ctx, "add", &mut time);
    let list = expenses::expenses(&app.project);
    assert_eq!(list.len(), 2);
    let id = app.views.records.expenses.list.selected.unwrap();
    assert_ne!(id, 1);
    assert_eq!(list.iter().find(|e| e.id == id).unwrap().task_id, 1010);
    // Edit on the General and Cost tabs.
    click(&mut app, &ctx, "expenses_tab_0", &mut time);
    type_into(&mut app, &ctx, "expense_name", "Scaffold hire", &mut time);
    click(&mut app, &ctx, "expenses_tab_2", &mut time);
    type_into(&mut app, &ctx, "expense_target_cost", "1200", &mut time);
    let doc = &app.project.document;
    assert_eq!(
        cell(doc, "PROJCOST", "cost_item_id", id, "cost_name"),
        "Scaffold hire"
    );
    assert_eq!(
        cell(doc, "PROJCOST", "cost_item_id", id, "target_cost"),
        "1200"
    );
    assert_eq!(
        cell(doc, "PROJCOST", "cost_item_id", id, "remain_cost"),
        "1200",
        "an item without actuals keeps its budget remaining"
    );
    // Moving the item to another activity through the picker.
    click(&mut app, &ctx, "expenses_tab_1", &mut time);
    click(&mut app, &ctx, "expense_activity_pick", &mut time);
    click(&mut app, &ctx, "pick_row_0", &mut time);
    click(&mut app, &ctx, "pick_assign", &mut time);
    assert!(app.views.records.picker.is_none());
    assert_eq!(
        cell(
            &app.project.document,
            "PROJCOST",
            "cost_item_id",
            id,
            "task_id"
        ),
        "1000"
    );
    // Grouping by activity gives a band per activity.
    app.views.records.expenses.group = Group::Activity;
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(app.action_rects.contains_key("expenses_row_3"));
    // Export round trip.
    let doc = exported(&app);
    assert_eq!(
        cell(&doc, "PROJCOST", "cost_item_id", id, "cost_name"),
        "Scaffold hire"
    );
    assert_eq!(
        cell(&doc, "PROJCOST", "cost_item_id", id, "target_cost"),
        "1200"
    );
    let reread = ProjectData::from_document(doc, 1, "x".into()).unwrap();
    assert_eq!(
        expenses::expenses(&reread),
        expenses::expenses(&app.project)
    );
    // Delete, then undo back through every step.
    click(&mut app, &ctx, "dock_delete", &mut time);
    assert_eq!(expenses::expenses(&app.project).len(), 1);
    app.history_move(&ctx, false);
    assert_eq!(expenses::expenses(&app.project).len(), 2);
    undo_until(&mut app, &ctx, |app| {
        expenses::expenses(&app.project).len() == 1
    });
    assert_eq!(expenses::expenses(&app.project)[0].name, "Crane rental");
    app.history_move(&ctx, true);
    assert_eq!(expenses::expenses(&app.project).len(), 2);
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn documents_view_outline_assignments_add_delete_undo_and_export() {
    let mut p = resources::tests::with_resource();
    p.document.table_mut("DOCSTAT").add(&[
        ("doc_status_id", "2".into()),
        ("doc_status_code", "Approved".into()),
    ]);
    for (id, parent, name) in [(1, "", "Specification"), (2, "1", "Appendix A")] {
        records::add_row(
            &mut p.document,
            "DOCUMENT",
            &[
                ("doc_id", id.to_string()),
                ("doc_seq_num", id.to_string()),
                ("doc_name", name.into()),
                ("parent_doc_id", parent.into()),
                ("proj_id", "1".into()),
                ("doc_status_id", "2".into()),
                ("deliv_flag", "Y".into()),
            ],
        );
    }
    let (path, ctx, mut app, mut time) = setup("documents", p);
    click(&mut app, &ctx, "dock_wps_docs", &mut time);
    assert_eq!(app.views.main, MainView::Documents);
    let list = documents::documents(&app.project);
    assert_eq!(documents::outline(&list), vec![(1, 0), (2, 1)]);
    assert_eq!(list[0].status, "Approved");
    assert!(app.action_rects.contains_key("documents_row_1"));
    // Collapsing the parent hides the child.
    app.views.records.documents.collapsed.insert(1);
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(!app.action_rects.contains_key("documents_row_1"));
    app.views.records.documents.collapsed.clear();
    // Add a sibling of the selected top-level document and rename it.
    click(&mut app, &ctx, "add", &mut time);
    let id = app.views.records.documents.selected.unwrap();
    assert_eq!(documents::documents(&app.project).len(), 3);
    click(&mut app, &ctx, "documents_tab_0", &mut time);
    type_into(
        &mut app,
        &ctx,
        "document_title",
        "Inspection test plan",
        &mut time,
    );
    type_into(&mut app, &ctx, "document_author_name", "QA lead", &mut time);
    // Assign to an activity and to a WBS element.
    click(&mut app, &ctx, "documents_tab_3", &mut time);
    click(&mut app, &ctx, "document_assign_buttons_0", &mut time);
    click(&mut app, &ctx, "pick_row_1", &mut time);
    click(&mut app, &ctx, "pick_assign", &mut time);
    click(&mut app, &ctx, "document_assign_buttons_1", &mut time);
    click(&mut app, &ctx, "pick_row_0", &mut time);
    click(&mut app, &ctx, "pick_assign", &mut time);
    let assigned = documents::assignments(&app.project.document, 1, id);
    assert_eq!(assigned.len(), 2);
    assert_eq!((assigned[0].task_id, assigned[0].wbs_id), (1010, 0));
    assert_eq!((assigned[1].task_id, assigned[1].wbs_id), (0, 10));
    // Work product flag on the first assignment.
    click(&mut app, &ctx, "document_assignments_row_0", &mut time);
    click(&mut app, &ctx, "document_assign_buttons_3", &mut time);
    assert!(documents::assignments(&app.project.document, 1, id)[0].work_product);
    // The activity's own WPs & Docs tab sees the same row.
    let rows = records::rows_for(&app.project.document, "TASKDOC", 1010, 1);
    assert_eq!(rows.len(), 1);
    // Export round trip, with documents ahead of their assignments.
    let doc = exported(&app);
    assert!(rank(&doc, "DOCUMENT") < rank(&doc, "TASKDOC"));
    assert_eq!(
        cell(&doc, "DOCUMENT", "doc_id", id, "doc_name"),
        "Inspection test plan"
    );
    assert_eq!(
        cell(&doc, "DOCUMENT", "doc_id", id, "author_name"),
        "QA lead"
    );
    let reread = ProjectData::from_document(doc, 1, "x".into()).unwrap();
    assert_eq!(
        documents::documents(&reread),
        documents::documents(&app.project)
    );
    assert_eq!(documents::assignments(&reread.document, 1, id).len(), 2);
    // Deleting the parent takes its child; undo brings both back.
    app.views.records.documents.selected = Some(1);
    click(&mut app, &ctx, "dock_delete", &mut time);
    // Two documents go: the delete asks first.
    assert_eq!(documents::documents(&app.project).len(), 3);
    click(&mut app, &ctx, "view_delete_yes", &mut time);
    assert_eq!(documents::documents(&app.project).len(), 1);
    app.history_move(&ctx, false);
    assert_eq!(documents::documents(&app.project).len(), 3);
    undo_until(&mut app, &ctx, |app| {
        documents::documents(&app.project).len() == 2
    });
    assert!(app
        .project
        .document
        .table("TASKDOC")
        .is_none_or(|t| t.rows.is_empty()));
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn risks_view_works_without_risk_tables_and_round_trips() {
    let mut p = resources::tests::with_resource();
    p.document.table_mut("RISKTYPE").add(&[
        ("risk_type_id", "4".into()),
        ("risk_type", "Weather".into()),
    ]);
    assert!(p.document.table("PROJRISK").is_none());
    let (path, ctx, mut app, mut time) = setup("risks", p);
    click(&mut app, &ctx, "dock_risks", &mut time);
    assert_eq!(app.views.main, MainView::Risks);
    assert!(risks::risks(&app.project).is_empty());
    assert!(!app.action_rects.contains_key("risks_row_0"));
    click(&mut app, &ctx, "add", &mut time);
    let id = app.views.records.risks.selected.unwrap();
    let list = risks::risks(&app.project);
    assert_eq!(list.len(), 1);
    assert_eq!(
        (
            list[0].code.as_str(),
            list[0].status.as_str(),
            list[0].kind.as_str()
        ),
        ("RISK-1", "Proposed", "Threat")
    );
    click(&mut app, &ctx, "risks_tab_0", &mut time);
    type_into(&mut app, &ctx, "risk_name", "Crane wind standby", &mut time);
    click(&mut app, &ctx, "risks_tab_4", &mut time);
    type_text(&mut app, &ctx, "risk_cause", "High winds", &mut time);
    click(&mut app, &ctx, "risks_tab_2", &mut time);
    click(&mut app, &ctx, "risk_activity_buttons_0", &mut time);
    click(&mut app, &ctx, "pick_row_1", &mut time);
    click(&mut app, &ctx, "pick_assign", &mut time);
    assert_eq!(
        risks::risk_activities(&app.project.document, 1, id),
        vec![1010]
    );
    // Category set directly through the shared helper the combo uses.
    assert!(record_view::set_value(
        &mut app.project.document,
        "PROJRISK",
        "risk_id",
        id,
        "risk_type_id",
        "4".into()
    ));
    app.touch_records();
    frame(&mut app, &ctx, vec![], &mut time);
    assert_eq!(risks::risks(&app.project)[0].category, "Weather");
    let doc = exported(&app);
    assert!(rank(&doc, "RISKTYPE") < rank(&doc, "PROJRISK"));
    assert!(rank(&doc, "PROJRISK") < rank(&doc, "TASKRISK"));
    assert_eq!(
        cell(&doc, "PROJRISK", "risk_id", id, "risk_name"),
        "Crane wind standby"
    );
    assert_eq!(
        cell(&doc, "PROJRISK", "risk_id", id, "risk_cause"),
        "High winds"
    );
    let reread = ProjectData::from_document(doc, 1, "x".into()).unwrap();
    assert_eq!(risks::risks(&reread), risks::risks(&app.project));
    assert_eq!(risks::risk_activities(&reread.document, 1, id), vec![1010]);
    click(&mut app, &ctx, "dock_delete", &mut time);
    assert!(risks::risks(&app.project).is_empty());
    assert!(risks::risk_activities(&app.project.document, 1, id).is_empty());
    app.history_move(&ctx, false);
    assert_eq!(
        risks::risk_activities(&app.project.document, 1, id),
        vec![1010]
    );
    undo_until(&mut app, &ctx, |app| {
        app.project.document.table("PROJRISK").is_none()
    });
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_file_with_a_risk_table_is_read_in_place() {
    let mut p = openxer_xer::starter();
    p.document.table_mut("RISK").add(&[
        ("risk_id", "6".into()),
        ("risk_code", "R6".into()),
        ("risk_name", "Late permit".into()),
        ("proj_id", "1".into()),
        ("status_code", "RS_Open".into()),
    ]);
    let list = risks::risks(&p);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].status, "Open");
    let id = risks::add_risk(&mut p, "2026-11-04 00:00");
    assert!(p.document.table("PROJRISK").is_none());
    assert_eq!(risks::risks(&p).len(), 2);
    assert!(risks::delete_risk(&mut p, id));
}

#[test]
fn issues_view_and_navigator_edit_and_navigate() {
    let mut p = resources::tests::with_resource();
    p.document
        .table_mut("OBS")
        .add(&[("obs_id", "5".into()), ("obs_name", "Site manager".into())]);
    let (path, ctx, mut app, mut time) = setup("issues", p);
    click_text(&mut app, &ctx, "Project", &mut time);
    click(&mut app, &ctx, "menu_issues", &mut time);
    assert_eq!(app.views.main, MainView::Issues);
    click(&mut app, &ctx, "add", &mut time);
    let id = app.views.records.issues.list.selected.unwrap();
    click(&mut app, &ctx, "issues_tab_0", &mut time);
    type_into(&mut app, &ctx, "issue_name", "Survey crew late", &mut time);
    type_into(&mut app, &ctx, "issue_identified_by", "Planner", &mut time);
    click(&mut app, &ctx, "issues_tab_1", &mut time);
    click(&mut app, &ctx, "issue_activity_pick", &mut time);
    click(&mut app, &ctx, "pick_row_1", &mut time);
    click(&mut app, &ctx, "pick_assign", &mut time);
    let issue = issues::issues(&app.project).pop().unwrap();
    assert_eq!(issue.name, "Survey crew late");
    assert_eq!(
        (issue.task_id, issue.status_code.as_str()),
        (1010, "IS_Open")
    );
    let doc = exported(&app);
    assert_eq!(
        cell(&doc, "PROJISSU", "issue_id", id, "issue_name"),
        "Survey crew late"
    );
    assert_eq!(
        cell(&doc, "PROJISSU", "issue_id", id, "add_by_name"),
        "Planner"
    );
    assert!(rank(&doc, "TASK") < rank(&doc, "PROJISSU"));
    let reread = ProjectData::from_document(doc, 1, "x".into()).unwrap();
    assert_eq!(issues::issues(&reread), issues::issues(&app.project));
    // Tools > Issue Navigator goes to the issue's activity.
    click(&mut app, &ctx, "menu_issue_navigator", &mut time);
    assert!(app.views.records.navigator.is_some());
    click(&mut app, &ctx, "navigator_row_0", &mut time);
    click(&mut app, &ctx, "navigator_activities", &mut time);
    assert_eq!(app.views.main, MainView::Activities);
    assert_eq!(
        app.selected
            .map(|i| app.project.model.activities[i].task_id),
        Some(1010)
    );
    click(&mut app, &ctx, "navigator_wbs", &mut time);
    assert_eq!(app.views.main, MainView::Wbs);
    assert_eq!(app.views.wbs.selected, Some(10));
    click(&mut app, &ctx, "navigator_details", &mut time);
    assert_eq!(app.views.main, MainView::Issues);
    click(&mut app, &ctx, "navigator_close", &mut time);
    assert!(app.views.records.navigator.is_none());
    // Closed issues leave the Open Issues display.
    issues::issues(&app.project);
    record_view::set_value(
        &mut app.project.document,
        "PROJISSU",
        "issue_id",
        id,
        "status_code",
        "IS_Closed".into(),
    );
    app.touch_records();
    app.views.records.issues.filter = issues::Filter::Open;
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(!app.action_rects.contains_key("issues_row_0"));
    app.views.records.issues.filter = issues::Filter::All;
    // The dock is drawn before the view: its Delete follows the selection
    // the view restores from the next frame on.
    frame(&mut app, &ctx, vec![], &mut time);
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "dock_delete", &mut time);
    assert!(issues::issues(&app.project).is_empty());
    app.history_move(&ctx, false);
    assert_eq!(issues::issues(&app.project).len(), 1);
    undo_until(&mut app, &ctx, |app| {
        issues::issues(&app.project).is_empty()
    });
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// Total float in days of every open activity, by task id.
fn floats(p: &ProjectData) -> Vec<(i64, f64)> {
    p.model
        .activities
        .iter()
        .enumerate()
        .filter(|(_, a)| !a.is_completed())
        .filter_map(|(i, a)| {
            let day = format::activity_periods(p, i).day as f64;
            Some((a.task_id, a.total_float? as f64 / day))
        })
        .collect()
}

#[test]
fn monitor_thresholds_raises_the_expected_issues() {
    let mut p = resources::tests::with_resource();
    // A project finish deadline gives the late work negative float.
    let finish = p
        .model
        .activities
        .iter()
        .filter_map(|a| a.early_finish)
        .max()
        .unwrap();
    p.model.project.plan_end = Some(finish - 3 * openxer_engine::calendar::DAY);
    schedule_model(&mut p.model, &p.resources).unwrap();
    let negative: Vec<i64> = floats(&p)
        .into_iter()
        .filter(|(_, d)| *d < 0.)
        .map(|(id, _)| id)
        .collect();
    assert!(!negative.is_empty() && negative.len() < p.model.activities.len());
    // A finish one day past its planned finish.
    let late = p
        .model
        .activities
        .iter()
        .position(|a| a.task_id == 1010)
        .unwrap();
    let planned = p.model.activities[late].early_finish.unwrap() - openxer_engine::calendar::DAY;
    p.model.activities[late].target_finish = Some(planned);
    let (path, ctx, mut app, mut time) = setup("thresholds", p);
    click_text(&mut app, &ctx, "Project", &mut time);
    click(&mut app, &ctx, "menu_thresholds", &mut time);
    assert_eq!(app.views.main, MainView::Thresholds);
    // Add: total float below 0 days on activities.
    click(&mut app, &ctx, "add", &mut time);
    let float_id = app.views.records.thresholds.list.selected.unwrap();
    let t = &thresholds::thresholds(&app.project.document, 1)[0];
    assert_eq!(
        (t.low, t.high, t.wbs_level, t.enabled),
        (Some(0.), None, false, true)
    );
    assert_eq!(thresholds::parameters(&app.project.document).len(), 14);
    // A second threshold: finish date variance below zero.
    let variance = thresholds::add_threshold(&mut app.project);
    let parm = thresholds::parameters(&app.project.document)
        .into_iter()
        .find(|q| q.field == "var_end_date_drtn_hr_cnt")
        .unwrap()
        .id;
    record_view::set_value(
        &mut app.project.document,
        "PROJTHRS",
        "thresh_id",
        variance,
        "thresh_parm_id",
        parm.to_string(),
    );
    // A third, at WBS level: total float below 0 per element.
    let wbs_level = thresholds::add_threshold(&mut app.project);
    record_view::set_value(
        &mut app.project.document,
        "PROJTHRS",
        "thresh_id",
        wbs_level,
        "thresh_type",
        "TM_WBS".into(),
    );
    // A fourth on an earned value parameter (CPI) is evaluated; the
    // starter carries no costs, so the index has no value and raises nothing.
    let skipped = thresholds::add_threshold(&mut app.project);
    record_view::set_value(
        &mut app.project.document,
        "PROJTHRS",
        "thresh_id",
        skipped,
        "thresh_parm_id",
        "6".into(),
    );
    app.touch_records();
    frame(&mut app, &ctx, vec![], &mut time);
    // Tools > Monitor Thresholds with the thresholds' own windows.
    click(&mut app, &ctx, "menu_monitor_thresholds", &mut time);
    click(&mut app, &ctx, "monitor_run", &mut time);
    assert!(app.views.records.monitor.is_none());
    assert_eq!(app.views.main, MainView::Issues);
    let raised = issues::issues(&app.project);
    let by =
        |t: i64| -> Vec<&issues::Issue> { raised.iter().filter(|i| i.threshold_id == t).collect() };
    let mut float_tasks: Vec<i64> = by(float_id).iter().map(|i| i.task_id).collect();
    float_tasks.sort();
    let mut expected = negative.clone();
    expected.sort();
    assert_eq!(
        float_tasks, expected,
        "one issue per activity below zero float"
    );
    let variance_issues = by(variance);
    assert_eq!(variance_issues.len(), 1, "{variance_issues:?}");
    assert_eq!(variance_issues[0].task_id, 1010);
    assert_eq!(variance_issues[0].value, "-1");
    let wbs_issues = by(wbs_level);
    assert!(!wbs_issues.is_empty() && wbs_issues.iter().all(|i| i.wbs_id != 0 && i.task_id == 0));
    assert!(by(skipped).is_empty());
    assert!(!app.status.contains("skipped"), "{}", app.status);
    let first = raised.len();
    // Each issue carries its threshold's values; a second run adds nothing.
    let i = by(float_id)[0];
    assert_eq!((i.low.as_str(), i.status_code.as_str()), ("0", "IS_Open"));
    let result = app.run_monitor(None);
    assert_eq!(
        (result.created, result.thresholds, result.skipped),
        (0, 4, 0)
    );
    assert_eq!(issues::issues(&app.project).len(), first);
    // A window before the project finds nothing new to raise.
    let before = issues::issues(&app.project).len();
    let result = app.run_monitor(Some((
        Some(-10 * openxer_engine::calendar::DAY),
        Some(-5 * openxer_engine::calendar::DAY),
    )));
    assert_eq!(result.created, 0);
    assert_eq!(issues::issues(&app.project).len(), before);
    // The Details tab lists the threshold's issues.
    app.views.records.thresholds.list.selected = Some(float_id);
    click(&mut app, &ctx, "dock_tab_thresholds", &mut time);
    click(&mut app, &ctx, "thresholds_tab_1", &mut time);
    assert!(app.action_rects.contains_key("threshold_issues_row_0"));
    // Export round trip of thresholds and issues.
    let doc = exported(&app);
    assert!(rank(&doc, "THRSPARM") < rank(&doc, "PROJTHRS"));
    assert!(rank(&doc, "PROJTHRS") < rank(&doc, "PROJISSU"));
    let reread = ProjectData::from_document(doc, 1, "x".into()).unwrap();
    assert_eq!(
        thresholds::thresholds(&reread.document, 1),
        thresholds::thresholds(&app.project.document, 1)
    );
    assert_eq!(issues::issues(&reread), issues::issues(&app.project));
    // Undo takes the raised issues away in one step.
    app.history_move(&ctx, false);
    assert!(issues::issues(&app.project).is_empty());
    assert_eq!(thresholds::thresholds(&app.project.document, 1).len(), 4);
    // Delete a threshold, then undo.
    click(&mut app, &ctx, "dock_delete", &mut time);
    assert_eq!(thresholds::thresholds(&app.project.document, 1).len(), 3);
    app.history_move(&ctx, false);
    assert_eq!(thresholds::thresholds(&app.project.document, 1).len(), 4);
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn right_dock_add_and_delete_act_on_record_views() {
    let (path, ctx, mut app, mut time) = setup("dock", resources::tests::with_resource());
    for (view, count) in [
        (
            MainView::Documents,
            &(|p: &ProjectData| documents::documents(p).len()) as &dyn Fn(&ProjectData) -> usize,
        ),
        (MainView::Risks, &|p: &ProjectData| risks::risks(p).len()),
        (MainView::Issues, &|p: &ProjectData| issues::issues(p).len()),
        (MainView::Thresholds, &|p: &ProjectData| {
            thresholds::thresholds(&p.document, 1).len()
        }),
        (MainView::Expenses, &|p: &ProjectData| {
            expenses::expenses(p).len()
        }),
    ] {
        app.show_view(view);
        frame(&mut app, &ctx, vec![], &mut time);
        click(&mut app, &ctx, "add", &mut time);
        assert_eq!(count(&app.project), 1, "{view:?} add");
        click(&mut app, &ctx, "dock_delete", &mut time);
        assert_eq!(count(&app.project), 0, "{view:?} delete");
    }
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn record_view_layouts_round_trip() {
    let mut v = Views::default();
    v.records.expenses.group = Group::None;
    v.records.expenses.list.columns.widths[2] = 222.;
    v.records.risks.tab = 3;
    v.records.issues.filter = issues::Filter::Closed;
    v.records.thresholds.list.details_height = 210.;
    let json = v.json();
    let mut w = Views::default();
    w.restore(&json);
    assert_eq!(w.records.expenses.group, Group::None);
    assert_eq!(
        w.records.expenses.list.columns,
        v.records.expenses.list.columns
    );
    assert_eq!(w.records.risks.tab, 3);
    assert_eq!(w.records.issues.filter, issues::Filter::Closed);
    assert_eq!(w.records.thresholds.list.details_height, 210.);
}

/// Software renders of every record view and details tab, for review.
/// Images go to the ignored `artifacts/review/projviews/`.
#[test]
fn render_record_views_and_tabs() {
    use crate::headless_render::SoftwarePainter;
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/review/projviews");
    std::fs::create_dir_all(&out).unwrap();
    let mut p = resources::tests::with_resource();
    let id = expenses::add_expense(&mut p, 1010);
    expenses::set_expense(&mut p, id, "target_cost", "750".into());
    let doc_id = documents::add_document(&mut p, None);
    documents::assign(&mut p, doc_id, Some(1010), None);
    documents::assign(&mut p, doc_id, None, Some(20));
    let risk = risks::add_risk(&mut p, "2026-11-04 00:00");
    risks::assign(&mut p.document, 1, risk, 1010);
    issues::add_issue(&mut p, "2026-11-04 00:00");
    thresholds::add_threshold(&mut p);
    let path =
        std::env::temp_dir().join(format!("openxer-records-render-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut store = Store::open(path.clone()).unwrap();
    store.save(&p).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, p);
    let mut time = 0.;
    let mut painter = SoftwarePainter::default();
    painter.screen = Some(Vec2::new(1920., 1040.));
    let views: [(MainView, &str, &[&str]); 5] = [
        (MainView::Expenses, "expenses", &expenses::TABS),
        (MainView::Documents, "documents", &documents::TABS),
        (MainView::Risks, "risks", &risks::TABS),
        (MainView::Issues, "issues", &issues::TABS),
        (MainView::Thresholds, "thresholds", &thresholds::TABS),
    ];
    for (view, name, tabs) in views {
        app.show_view(view);
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        let shot = |app: &mut App, painter: &mut SoftwarePainter, time: &mut f64, file: String| {
            let _ = painter.frame(app, &ctx, time, vec![], false);
            let (image, triangles) = painter.frame(app, &ctx, time, vec![], true).unwrap();
            assert!(triangles > 300, "{file} draws the view");
            image.save(out.join(format!("{file}.png"))).unwrap();
        };
        shot(&mut app, &mut painter, &mut time, format!("{name}-view"));
        for (i, tab) in tabs.iter().enumerate() {
            painter.click_action(&mut app, &ctx, &mut time, &format!("{name}_tab_{i}"));
            let slug: String = tab
                .to_lowercase()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect();
            shot(
                &mut app,
                &mut painter,
                &mut time,
                format!("{name}-tab-{slug}"),
            );
        }
    }
    // The WBS view's WPs & Docs tab with the WBS assignment.
    app.show_view(MainView::Wbs);
    app.views.wbs.selected = Some(20);
    app.views.wbs.tab = 6;
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("wbs-tab-wps-docs.png")).unwrap();
    app.open_monitor_thresholds();
    let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("monitor-thresholds.png")).unwrap();
    app.views.records.monitor = None;
    app.open_issue_navigator();
    let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("issue-navigator.png")).unwrap();
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// The WBS view's WPs & Docs tab lists the documents assigned to the WBS
/// element itself, sorted by title, and assigns, flags, shows and removes
/// them as single undo steps that survive export.
#[test]
fn wbs_documents_tab_assigns_flags_and_removes() {
    let mut p = resources::tests::with_resource();
    p.document.table_mut("DOCSTAT").add(&[
        ("doc_status_id", "2".into()),
        ("doc_status_code", "Approved".into()),
    ]);
    for (id, name) in [(1, "Specification"), (2, "Appendix A"), (3, "Drawing list")] {
        records::add_row(
            &mut p.document,
            "DOCUMENT",
            &[
                ("doc_id", id.to_string()),
                ("doc_seq_num", id.to_string()),
                ("doc_name", name.into()),
                ("proj_id", "1".into()),
                ("doc_status_id", "2".into()),
            ],
        );
    }
    // An activity assignment of document 3 is not a WBS assignment.
    documents::assign(&mut p, 3, Some(1010), None);
    let (path, ctx, mut app, mut time) = setup("wbs-documents", p);
    click(&mut app, &ctx, "directory_wbs", &mut time);
    app.views.wbs.selected = Some(20);
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "wbs_tab_6", &mut time);
    assert!(!app.action_rects.contains_key("wbs_documents_row_0"));
    // Assign Specification, then Appendix A, from the picker.
    for _ in 0..2 {
        click(&mut app, &ctx, "wbs_documents_buttons_0", &mut time);
        assert!(app.views.records.picker.is_some());
        click(&mut app, &ctx, "pick_row_0", &mut time);
        click(&mut app, &ctx, "pick_assign", &mut time);
        assert!(app.views.records.picker.is_none());
    }
    let rows = documents::wbs_assignments(&app.project.document, 1, 20);
    let titles: Vec<&str> = rows.iter().map(|r| r.2.as_str()).collect();
    assert_eq!(titles, ["Appendix A", "Specification"]);
    assert_eq!(rows[0].3, "Approved");
    assert!(app.action_rects.contains_key("wbs_documents_row_1"));
    // Cancel and Escape leave the project alone.
    let before = app.project.document.text();
    click(&mut app, &ctx, "wbs_documents_buttons_0", &mut time);
    click(&mut app, &ctx, "pick_row_0", &mut time);
    click(&mut app, &ctx, "pick_cancel", &mut time);
    assert!(app.views.records.picker.is_none());
    assert_eq!(app.project.document.text(), before);
    // Work Product check box.
    click(&mut app, &ctx, "wbs_documents_wp_1", &mut time);
    let rows = documents::wbs_assignments(&app.project.document, 1, 20);
    assert!(!rows[0].0.work_product && rows[1].0.work_product);
    // Details... opens the document's details; Escape closes them.
    click(&mut app, &ctx, "wbs_documents_row_0", &mut time);
    click(&mut app, &ctx, "wbs_documents_details", &mut time);
    assert_eq!(app.detail.document_details, Some(2));
    let escape = egui::Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    };
    frame(&mut app, &ctx, vec![escape], &mut time);
    assert_eq!(app.detail.document_details, None);
    // Export: WBS rows carry the element and no activity.
    let doc = exported(&app);
    let reread = ProjectData::from_document(doc, 1, "x".into()).unwrap();
    let back = documents::wbs_assignments(&reread.document, 1, 20);
    assert_eq!(back.len(), 2);
    assert!(back[1].0.work_product);
    assert_eq!(
        records::rows_for(&reread.document, "TASKDOC", 1010, 1).len(),
        1
    );
    // Remove, then undo each step back to no WBS documents.
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "wbs_documents_row_0", &mut time);
    click(&mut app, &ctx, "wbs_documents_buttons_1", &mut time);
    let rows = documents::wbs_assignments(&app.project.document, 1, 20);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].2, "Specification");
    app.history_move(&ctx, false);
    assert_eq!(
        documents::wbs_assignments(&app.project.document, 1, 20).len(),
        2
    );
    undo_until(&mut app, &ctx, |app| {
        documents::wbs_assignments(&app.project.document, 1, 20).is_empty()
    });
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn documents_view_lists_documents_nested_deeper_than_64_levels_and_in_parent_loops() {
    let mut p = resources::tests::with_resource();
    // A chain 100 levels deep (ids 1..=100), then a parent loop 201 <-> 202.
    for id in 1..=100 {
        let parent = if id == 1 {
            String::new()
        } else {
            (id - 1).to_string()
        };
        records::add_row(
            &mut p.document,
            "DOCUMENT",
            &[
                ("doc_id", id.to_string()),
                ("doc_seq_num", id.to_string()),
                ("doc_name", format!("Chain {id}")),
                ("parent_doc_id", parent),
                ("proj_id", "1".into()),
            ],
        );
    }
    for (id, parent) in [(201, 202), (202, 201)] {
        records::add_row(
            &mut p.document,
            "DOCUMENT",
            &[
                ("doc_id", id.to_string()),
                ("doc_seq_num", id.to_string()),
                ("doc_name", format!("Loop {id}")),
                ("parent_doc_id", parent.to_string()),
                ("proj_id", "1".into()),
            ],
        );
    }
    let list = documents::documents(&p);
    let rows = documents::outline(&list);
    assert_eq!(rows.len(), 102, "every document is listed");
    assert_eq!(rows[99], (100, 99), "the 100th level is shown at depth 99");
    let loop_rows: Vec<(i64, usize)> = rows.iter().copied().filter(|r| r.0 > 200).collect();
    assert_eq!(
        loop_rows,
        [(201, 0), (202, 1)],
        "a loop starts at its first member"
    );
    let (path, ctx, mut app, mut time) = setup("documents-deep", p);
    click(&mut app, &ctx, "dock_wps_docs", &mut time);
    assert!(app.action_rects.contains_key("documents_row_0"));
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}
