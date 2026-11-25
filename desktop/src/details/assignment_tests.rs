//! Headless checks for the Resources tab and Edit > Delete. They drive the
//! production `App::update_ui` through egui input; no native window opens.
use super::assignments::AssignPicker;
use super::headless_tests::{cleanup, click, frame, open_app, temp_db, texts};
use super::*;

fn add_dictionaries(app: &mut App) {
    let doc = &mut app.project.document;
    doc.table_mut("RSRC").add(&[
        ("rsrc_id", "10".into()),
        ("rsrc_short_name", "PIPE".into()),
        ("rsrc_name", "Pipe crew".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("def_qty_per_hr", "2".into()),
    ]);
    doc.table_mut("ROLES").add(&[
        ("role_id", "5".into()),
        ("role_short_name", "PF".into()),
        ("role_name", "Pipefitter".into()),
    ]);
    doc.table_mut("RSRCROLE").add(&[
        ("rsrc_id", "10".into()),
        ("role_id", "5".into()),
        ("skill_level", "3".into()),
    ]);
}
fn select(app: &mut App, task_id: i64) -> usize {
    let i = app
        .project
        .model
        .activities
        .iter()
        .position(|a| a.task_id == task_id)
        .unwrap();
    app.selected = Some(i);
    i
}
fn assignment_count(app: &App, task_id: i64) -> usize {
    app.project.assignment_rows(task_id).len()
}

#[test]
fn resources_tab_buttons_assign_staff_remove_and_undo() {
    let path = temp_db("resources-tab");
    let (ctx, mut app, mut time) = open_app(&path);
    add_dictionaries(&mut app);
    let i = select(&mut app, 1010);
    click(&mut app, &ctx, "detail_tab_resources", &mut time);
    let output = frame(&mut app, &ctx, vec![], &mut time);
    let shown = texts(&output);
    for caption in [
        "Add Resource",
        "Add Role",
        "Assign by Role",
        "Remove",
        "Budgeted Units",
    ] {
        assert!(shown.iter().any(|t| t == caption), "{caption} is drawn");
    }

    // Add Resource opens the picker; Assign adds the row.
    click(&mut app, &ctx, "detail_rsrc_add", &mut time);
    assert!(matches!(
        app.detail.assign,
        Some(AssignPicker::Resource { .. })
    ));
    app.detail.assign = Some(AssignPicker::Resource {
        query: String::new(),
        selected: Some(10),
    });
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "rsrc_picker_assign", &mut time);
    click(&mut app, &ctx, "rsrc_picker_close", &mut time);
    assert!(app.detail.assign.is_none());
    assert_eq!(assignment_count(&app, 1010), 1);
    assert_eq!(app.project.model.activities[i].assignments.len(), 1);
    let row = app.project.assignment_rows(1010)[0];
    let view = app.project.assignment_view(row).unwrap();
    assert_eq!(view.remaining.units, 48., "3 days at 2 units/hour");
    assert!(view.primary);

    // Add Role.
    click(&mut app, &ctx, "detail_rsrc_add_role", &mut time);
    app.detail.assign = Some(AssignPicker::Role { selected: Some(5) });
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "rsrc_picker_assign", &mut time);
    click(&mut app, &ctx, "rsrc_picker_close", &mut time);
    assert_eq!(assignment_count(&app, 1010), 2);
    let role_row = app.project.assignment_rows(1010)[1];
    assert_eq!(app.project.assignment_view(role_row).unwrap().rsrc_id, None);

    // Assign by Role staffs the selected role assignment.
    app.detail.rows[0] = Some(1);
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "detail_rsrc_by_role", &mut time);
    assert!(matches!(
        app.detail.assign,
        Some(AssignPicker::ByRole { role: 5, .. })
    ));
    if let Some(AssignPicker::ByRole { selected, .. }) = &mut app.detail.assign {
        *selected = Some(10);
    }
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "rsrc_picker_assign", &mut time);
    assert!(app.detail.assign.is_none(), "staffing closes the picker");
    // User Preferences default to "Ask me": the question comes first.
    assert!(app.detail.question.is_some());
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "assignment_question_ok", &mut time);
    let staffed = app.project.assignment_view(role_row).unwrap();
    assert_eq!((staffed.rsrc_id, staffed.role_id), (Some(10), Some(5)));

    // Remove the selected row.
    app.detail.rows[0] = Some(0);
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "detail_rsrc_remove", &mut time);
    // It asks first, then again for the primary resource; Yes to both.
    assert!(app.detail.removal.is_some(), "Remove asks first");
    assert_eq!(assignment_count(&app, 1010), 2, "nothing removed yet");
    click(&mut app, &ctx, "remove_assignment_yes", &mut time);
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "remove_primary_yes", &mut time);
    assert!(app.detail.removal.is_none());
    assert_eq!(assignment_count(&app, 1010), 1);
    assert_eq!(app.project.model.activities[i].assignments.len(), 1);

    // Each change is one undo step.
    app.history_move(&ctx, false);
    assert_eq!(assignment_count(&app, 1010), 2);
    assert_eq!(app.project.model.activities[i].assignments.len(), 2);
    app.history_move(&ctx, false);
    let staffed = app.project.assignment_view(role_row).unwrap();
    assert_eq!(staffed.rsrc_id, None, "staffing undone");
    app.history_move(&ctx, true);
    app.history_move(&ctx, true);
    assert_eq!(assignment_count(&app, 1010), 1);

    // F9 writes the scheduled dates into the assignment.
    app.schedule();
    assert!(app.error.is_none(), "{:?}", app.error);
    let row = app.project.assignment_rows(1010)[0];
    let t = app.project.document.table("TASKRSRC").unwrap();
    let a = &app.project.model.activities[i];
    assert_eq!(
        t.value(&t.rows[row], "restart_date"),
        openxer_xer::date_string(app.project.model.origin_serial, a.restart)
    );
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    drop(app);
    cleanup(path);
}

/// A project with no resources: Add Resource opens Assign Resources, New
/// adds the typed resource as one undo step and selects it, Assign assigns it.
#[test]
fn add_resource_adds_the_first_resource_of_a_project() {
    let path = temp_db("resources-first");
    let (ctx, mut app, mut time) = open_app(&path);
    let rows = |app: &App| {
        app.project
            .document
            .table("RSRC")
            .map_or(0, |t| t.rows.len())
    };
    assert_eq!(rows(&app), 0, "the test project starts without resources");
    select(&mut app, 1010);
    click(&mut app, &ctx, "detail_tab_resources", &mut time);
    click(&mut app, &ctx, "detail_rsrc_add", &mut time);
    assert!(
        matches!(app.detail.assign, Some(AssignPicker::Resource { .. })),
        "Add Resource is offered without resources"
    );
    let output = frame(&mut app, &ctx, vec![], &mut time);
    assert!(texts(&output)
        .iter()
        .any(|t| t == "This project has no resources yet. Click New to add one."));

    // New shows the ID and Name fields; an empty ID adds nothing.
    click(&mut app, &ctx, "rsrc_picker_new", &mut time);
    assert_eq!(
        app.detail.new_resource,
        Some((String::new(), String::new()))
    );
    click(&mut app, &ctx, "rsrc_picker_new_add", &mut time);
    assert_eq!(rows(&app), 0, "Add is disabled without an ID");

    app.detail.new_resource = Some(("PF".into(), "Pipefitters".into()));
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "rsrc_picker_new_add", &mut time);
    assert_eq!(rows(&app), 1);
    let doc = &app.project.document;
    let t = doc.table("RSRC").unwrap();
    let id: i64 = t.value(&t.rows[0], "rsrc_id").parse().unwrap();
    assert_eq!(t.value(&t.rows[0], "rsrc_short_name"), "PF");
    assert_eq!(t.value(&t.rows[0], "rsrc_name"), "Pipefitters");
    assert_eq!(t.value(&t.rows[0], "rsrc_type"), "RT_Labor");
    assert_eq!(app.detail.new_resource, None, "the fields are put away");
    assert!(
        matches!(&app.detail.assign, Some(AssignPicker::Resource { selected, .. }) if *selected == Some(id)),
        "the new resource is selected"
    );

    // An ID in use is refused and nothing is added.
    app.detail.new_resource = Some(("pf".into(), "Again".into()));
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "rsrc_picker_new_add", &mut time);
    assert_eq!(rows(&app), 1);
    assert_eq!(app.status, "Resource ID pf is already in use.");

    // Assign puts it on the activity.
    click(&mut app, &ctx, "rsrc_picker_assign", &mut time);
    click(&mut app, &ctx, "rsrc_picker_close", &mut time);
    assert_eq!(assignment_count(&app, 1010), 1);
    let row = app.project.assignment_rows(1010)[0];
    assert_eq!(app.project.assignment_view(row).unwrap().rsrc_id, Some(id));

    // Two undo steps: the assignment, then the resource.
    app.history_move(&ctx, false);
    assert_eq!(assignment_count(&app, 1010), 0);
    assert_eq!(rows(&app), 1);
    app.history_move(&ctx, false);
    assert_eq!(rows(&app), 0);
    drop(app);
    cleanup(path);
}

#[test]
fn add_role_opens_empty_without_roles_and_milestones_take_none() {
    let path = temp_db("resources-disabled");
    let (ctx, mut app, mut time) = open_app(&path);
    select(&mut app, 1010);
    click(&mut app, &ctx, "detail_tab_resources", &mut time);
    // No ROLES rows: Add Role still opens its picker, empty (the add
    // buttons are always enabled, as in the reference). Assign by Role
    // needs a selected assignment with a role.
    click(&mut app, &ctx, "detail_rsrc_add_role", &mut time);
    assert!(
        matches!(app.detail.assign, Some(AssignPicker::Role { .. })),
        "Add Role opens the role picker"
    );
    app.detail.assign = None;
    click(&mut app, &ctx, "detail_rsrc_by_role", &mut time);
    assert!(app.detail.assign.is_none(), "Assign by Role is disabled");
    add_dictionaries(&mut app);
    select(&mut app, 1000);
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "detail_rsrc_add", &mut time);
    assert!(app.detail.assign.is_none(), "milestones take no resources");
    drop(app);
    cleanup(path);
}

#[test]
fn delete_key_confirms_removes_child_rows_and_undoes() {
    let path = temp_db("delete-activity");
    let (ctx, mut app, mut time) = open_app(&path);
    add_dictionaries(&mut app);
    app.project.add_assignment(1050, Some(10), None).unwrap();
    crate::records::add_row(
        &mut app.project.document,
        "TASKNOTE",
        &[
            ("task_id", "1050".into()),
            ("proj_id", "1".into()),
            ("task_notes", "Crane booked".into()),
        ],
    );
    app.touch_records();
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    let count = app.project.model.activities.len();
    let links = app.project.model.links.len();
    select(&mut app, 1050);
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    let key = |pressed| egui::Event::Key {
        key: Key::Delete,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Default::default(),
    };
    let _ = frame(&mut app, &ctx, vec![key(true), key(false)], &mut time);
    assert_eq!(app.delete_confirm, Some(vec![1050]));
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "delete_yes", &mut time);
    assert!(app.delete_confirm.is_none());
    assert_eq!(app.project.model.activities.len(), count - 1);
    assert!(app.project.model.links.len() < links);
    assert!(app.project.assignment_rows(1050).is_empty());
    assert!(crate::records::rows_for(&app.project.document, "TASKNOTE", 1050, 1).is_empty());
    let reread = openxer_xer::ProjectData::from_document(
        openxer_xer::Document::parse(&app.project.updated_document().text()).unwrap(),
        1,
        "reimport".into(),
    )
    .unwrap();
    assert_eq!(reread.model.activities.len(), count - 1);
    assert!(reread.warnings.is_empty(), "{:?}", reread.warnings);

    app.history_move(&ctx, false);
    assert_eq!(app.project.model.activities.len(), count);
    assert_eq!(app.project.model.links.len(), links);
    assert_eq!(app.project.assignment_rows(1050).len(), 1);
    assert_eq!(
        crate::records::rows_for(&app.project.document, "TASKNOTE", 1050, 1).len(),
        1
    );
    app.history_move(&ctx, true);
    assert_eq!(app.project.model.activities.len(), count - 1);

    // Edit > Delete with No keeps the activity.
    select(&mut app, 1060);
    app.request_delete();
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "delete_no", &mut time);
    assert_eq!(app.project.model.activities.len(), count - 1);
    drop(app);
    cleanup(path);
}

#[test]
fn resource_assignments_window_lists_edits_and_deletes() {
    let path = temp_db("assignment-window");
    let (ctx, mut app, mut time) = open_app(&path);
    add_dictionaries(&mut app);
    let first = app.project.add_assignment(1010, Some(10), None).unwrap();
    app.project.add_assignment(1050, Some(10), None).unwrap();
    app.touch_records();
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    app.open_assignment_window();
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    let output = frame(&mut app, &ctx, vec![], &mut time);
    let shown = texts(&output);
    // The title is part of the kit frame; the grid header is the window's own.
    for text in ["Resource ID Name", "RB-1010", "RB-1050", "General"] {
        assert!(shown.iter().any(|t| t == text), "{text} is drawn");
    }
    app.detail.assignment_window.as_mut().unwrap().selected = Some(first);
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    let output = frame(&mut app, &ctx, vec![], &mut time);
    let shown = texts(&output);
    for text in [
        "Budgeted Units / Time",
        "Remaining Duration",
        "Primary Resource",
    ] {
        assert!(shown.iter().any(|t| t == text), "{text} is drawn");
    }
    click(&mut app, &ctx, "assignment_window_delete", &mut time);
    // Delete asks first; Yes deletes.
    assert_eq!(app.project.assignment_rows(1010).len(), 1, "asks first");
    click(&mut app, &ctx, "delete_resource_assignment_yes", &mut time);
    assert!(app.project.assignment_rows(1010).is_empty());
    assert_eq!(app.project.assignment_rows(1050).len(), 1);
    app.history_move(&ctx, false);
    assert_eq!(app.project.assignment_rows(1010).len(), 1);
    click(&mut app, &ctx, "assignment_window_close", &mut time);
    assert!(app.detail.assignment_window.is_none());
    drop(app);
    cleanup(path);
}

/// Writes `artifacts/headless-resources-tab.png` and
/// `artifacts/headless-resource-assignments.png` for visual review:
/// `cargo.exe test -p openxer-desktop render_assignment_views -- --ignored`.
#[test]
#[ignore = "writes review renders to artifacts/; run on demand"]
fn render_assignment_views_to_png() {
    use crate::headless_render::SoftwarePainter;
    let artifacts = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let path = temp_db("assignment-render");
    let store = crate::store::Store::open(path.clone()).unwrap();
    let mut project = openxer_xer::starter();
    schedule_model(&mut project.model, &project.resources).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    add_dictionaries(&mut app);
    app.project.add_assignment(1050, Some(10), None).unwrap();
    app.project.add_assignment(1050, None, Some(5)).unwrap();
    app.project.add_assignment(1080, Some(10), None).unwrap();
    select(&mut app, 1050);
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "detail_tab_resources");
    app.detail.rows[0] = Some(0);
    let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    let top = 520;
    image::imageops::crop_imm(&image, 0, top, image.width(), image.height() - top)
        .to_image()
        .save(artifacts.join("headless-resources-tab.png"))
        .unwrap();
    app.open_assignment_window();
    let first = app.project.assignment_rows(1050)[0];
    app.detail.assignment_window.as_mut().unwrap().selected = Some(first);
    let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image
        .save(artifacts.join("headless-resource-assignments.png"))
        .unwrap();
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    cleanup(path);
}

/// An activity code type with two values, for the code pickers.
fn add_code_dictionary(app: &mut App) {
    let doc = &mut app.project.document;
    doc.table_mut("ACTVTYPE").add(&[
        ("actv_code_type_id", "7".into()),
        ("actv_code_type", "Area".into()),
        ("actv_code_type_scope", "AS_Project".into()),
        ("proj_id", "1".into()),
        ("seq_num", "1".into()),
    ]);
    for (id, short) in [(71, "NORTH"), (72, "SOUTH")] {
        doc.table_mut("ACTVCODE").add(&[
            ("actv_code_id", id.to_string()),
            ("actv_code_type_id", "7".into()),
            ("short_name", short.into()),
            ("actv_code_name", short.into()),
            ("seq_num", id.to_string()),
        ]);
    }
    app.touch_records();
}

fn select_many(app: &mut App, tasks: &[i64]) {
    let rows: Vec<usize> = tasks
        .iter()
        .map(|t| {
            app.project
                .model
                .activities
                .iter()
                .position(|a| a.task_id == *t)
                .unwrap()
        })
        .collect();
    app.select_activities(&rows);
    let mut chosen = app.selected_in_order();
    chosen.sort_unstable();
    let mut expected = rows.clone();
    expected.sort_unstable();
    assert_eq!(chosen, expected);
}

#[test]
fn assign_resource_and_code_to_five_selected_activities_in_one_undo_step() {
    use super::assignments::ResourceAssign;
    let path = temp_db("assign-many");
    let (ctx, mut app, mut time) = open_app(&path);
    add_dictionaries(&mut app);
    add_code_dictionary(&mut app);
    let five = [1010, 1020, 1040, 1050, 1060];
    select_many(&mut app, &five);
    let _ = frame(&mut app, &ctx, vec![], &mut time);

    // Edit > Assign > Resources... opens the picker at once.
    app.editing.pending = Some(editing::Command::AssignResource(ResourceAssign::Resources));
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    assert!(matches!(
        app.detail.assign,
        Some(AssignPicker::Resource { .. })
    ));
    assert_eq!(app.detail.tab, DetailTab::Resources);
    let before = app.project.clone();
    let steps = app.history.counts().0;
    app.detail.assign = Some(AssignPicker::Resource {
        query: String::new(),
        selected: Some(10),
    });
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "rsrc_picker_assign", &mut time);
    for task in five {
        assert_eq!(assignment_count(&app, task), 1, "{task}");
    }
    assert_eq!(assignment_count(&app, 1070), 0, "only the selection");
    assert_eq!(app.history.counts().0, steps + 1, "one undo step");
    // A second Assign of the same resource adds nothing.
    click(&mut app, &ctx, "rsrc_picker_assign", &mut time);
    for task in five {
        assert_eq!(assignment_count(&app, task), 1, "{task} is not doubled");
    }
    click(&mut app, &ctx, "rsrc_picker_close", &mut time);
    let assigned = app.project.clone();
    app.history_move(&ctx, false);
    for task in five {
        assert_eq!(assignment_count(&app, task), 0, "{task} after undo");
    }
    assert_eq!(app.project.model, before.model);
    app.history_move(&ctx, true);
    assert_eq!(app.project.model, assigned.model);
    for task in five {
        assert_eq!(assignment_count(&app, task), 1, "{task} after redo");
    }

    // Edit > Assign > Activity Codes... gives all five the value.
    select_many(&mut app, &five);
    let steps = app.history.counts().0;
    app.editing.pending = Some(editing::Command::Assign(DetailTab::Codes));
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    assert!(matches!(app.detail.picker, Some(Picker::Code { .. })));
    app.detail.picker = Some(Picker::Code {
        type_id: Some(7),
        selected: Some(72),
    });
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "picker_assign", &mut time);
    let coded = |app: &App| {
        let doc = &app.project.document;
        doc.table("TASKACTV").map_or(0, |t| {
            t.rows
                .iter()
                .filter(|r| t.value(r, "actv_code_id") == "72")
                .count()
        })
    };
    assert_eq!(coded(&app), 5);
    assert_eq!(app.history.counts().0, steps + 1, "one undo step");
    click(&mut app, &ctx, "picker_close", &mut time);
    app.history_move(&ctx, false);
    assert_eq!(coded(&app), 0);
    app.history_move(&ctx, true);
    assert_eq!(coded(&app), 5);

    // A milestone in the selection takes no resource; the others do.
    select_many(&mut app, &[1070, 1030, 1080]);
    app.editing.pending = Some(editing::Command::AssignResource(ResourceAssign::Resources));
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    app.detail.assign = Some(AssignPicker::Resource {
        query: String::new(),
        selected: Some(10),
    });
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "rsrc_picker_assign", &mut time);
    assert_eq!(assignment_count(&app, 1070), 1);
    assert_eq!(assignment_count(&app, 1080), 1);
    assert_eq!(assignment_count(&app, 1030), 0);
    assert!(app.status.contains("2 activities"), "{}", app.status);
    assert!(app.status.contains("1 skipped"), "{}", app.status);
    drop(app);
    cleanup(path);
}

#[test]
fn roles_and_resources_by_role_open_their_pickers_and_assign_to_the_selection() {
    use super::assignments::ResourceAssign;
    let path = temp_db("assign-roles");
    let (ctx, mut app, mut time) = open_app(&path);
    add_dictionaries(&mut app);
    let three = [1010, 1020, 1040];
    select_many(&mut app, &three);
    let _ = frame(&mut app, &ctx, vec![], &mut time);

    app.editing.pending = Some(editing::Command::AssignResource(ResourceAssign::Roles));
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    assert!(matches!(app.detail.assign, Some(AssignPicker::Role { .. })));
    app.detail.assign = Some(AssignPicker::Role { selected: Some(5) });
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "rsrc_picker_assign", &mut time);
    click(&mut app, &ctx, "rsrc_picker_close", &mut time);
    for task in three {
        let rows = app.project.assignment_rows(task);
        assert_eq!(rows.len(), 1);
        let view = app.project.assignment_view(rows[0]).unwrap();
        assert_eq!((view.rsrc_id, view.role_id), (None, Some(5)));
    }

    // Resources by Role: choose the role, then a resource that has it.
    select_many(&mut app, &[1050, 1060]);
    app.editing.pending = Some(editing::Command::AssignResource(ResourceAssign::ByRole));
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    assert!(matches!(
        app.detail.assign,
        Some(AssignPicker::PickRole { .. })
    ));
    app.detail.assign = Some(AssignPicker::PickRole { selected: Some(5) });
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "rsrc_picker_assign", &mut time);
    assert!(matches!(
        app.detail.assign,
        Some(AssignPicker::StaffRole { role: 5, .. })
    ));
    app.detail.assign = Some(AssignPicker::StaffRole {
        role: 5,
        selected: Some(10),
    });
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "rsrc_picker_assign", &mut time);
    for task in [1050, 1060] {
        let rows = app.project.assignment_rows(task);
        assert_eq!(rows.len(), 1, "{task}");
        let view = app.project.assignment_view(rows[0]).unwrap();
        assert_eq!((view.rsrc_id, view.role_id), (Some(10), Some(5)));
    }
    drop(app);
    cleanup(path);
}

#[test]
fn predecessors_and_successors_apply_to_every_selected_activity_in_one_undo_step() {
    let path = temp_db("assign-links");
    let (ctx, mut app, mut time) = open_app(&path);
    let index = |app: &App, task: i64| {
        app.project
            .model
            .activities
            .iter()
            .position(|a| a.task_id == task)
            .unwrap()
    };
    let selection = [1060, 1070, 1080];
    select_many(&mut app, &selection);
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    let links_before = app.project.model.links.len();
    let steps = app.history.counts().0;
    // Predecessors...: one activity ahead of all three.
    app.editing.pending = Some(editing::Command::Assign(DetailTab::Predecessors));
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    let other = index(&app, 1000);
    app.detail.picker = Some(Picker::Relationship {
        predecessor: true,
        query: String::new(),
        selected: Some(other),
        kind: openxer_engine::model::LinkType::SS,
        lag: 0.,
    });
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "picker_assign", &mut time);
    let tied = |app: &App| {
        app.project
            .model
            .links
            .iter()
            .filter(|l| {
                app.project.model.activities[l.pred].task_id == 1000
                    && selection.contains(&app.project.model.activities[l.succ].task_id)
                    && l.link_type == openxer_engine::model::LinkType::SS
            })
            .count()
    };
    assert_eq!(tied(&app), 3);
    assert_eq!(app.history.counts().0, steps + 1, "one undo step");
    // Close the out-of-sequence prompt, if any, then undo and redo.
    app.sched.sequence = None;
    click(&mut app, &ctx, "picker_close", &mut time);
    app.history_move(&ctx, false);
    assert_eq!(app.project.model.links.len(), links_before);
    app.history_move(&ctx, true);
    assert_eq!(tied(&app), 3);
    drop(app);
    cleanup(path);
}
