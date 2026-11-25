//! Headless checks for the Activity Details pane. They drive the production
//! `App::update_ui` through egui input events; no native window is opened.
use super::*;
use crate::store::Store;
use std::path::PathBuf;

pub(super) fn frame(
    app: &mut App,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    time: &mut f64,
) -> egui::FullOutput {
    *time += 0.1;
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500., 900.))),
            time: Some(*time),
            events,
            focused: true,
            ..Default::default()
        },
        |ctx| app.update_ui(ctx),
    )
}
pub(super) fn click_at(app: &mut App, ctx: &egui::Context, pos: Pos2, time: &mut f64) {
    for pressed in [true, false] {
        let _ = frame(
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
    let _ = frame(app, ctx, vec![], time);
}
pub(super) fn click(app: &mut App, ctx: &egui::Context, action: &str, time: &mut f64) {
    if !app.action_rects.contains_key(action) {
        if let Some(menu) = crate::menu::menu_holding(action) {
            // Commands without a toolbar button live in a main menu.
            let output = frame(app, ctx, vec![], time);
            let pos = output
                .shapes
                .iter()
                .find_map(|clipped| match &clipped.shape {
                    egui::Shape::Text(shape) if shape.galley.text() == menu => {
                        Some(shape.visual_bounding_rect().center())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("No menu {menu}"));
            click_at(app, ctx, pos, time);
        }
    }
    let [l, t, r, b] = *app
        .action_rects
        .get(action)
        .unwrap_or_else(|| panic!("No action {action}"));
    click_at(app, ctx, Pos2::new((l + r) / 2., (t + b) / 2.), time);
}
pub(super) fn texts(output: &egui::FullOutput) -> Vec<String> {
    output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(t) if clipped.clip_rect.is_positive() => {
                Some(t.galley.text().to_string())
            }
            _ => None,
        })
        .collect()
}
pub(super) fn temp_db(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "openxer-details-{name}-{}-{unique}.db",
        std::process::id()
    ))
}
pub(super) fn open_app(path: &std::path::Path) -> (egui::Context, App, f64) {
    let store = Store::open(path.to_path_buf()).unwrap();
    let mut project = openxer_xer::starter();
    schedule_model(&mut project.model, &project.resources).unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx, store, project);
    let mut time = 0.;
    for _ in 0..3 {
        let _ = frame(&mut app, &ctx, vec![], &mut time);
    }
    (ctx, app, time)
}
pub(super) fn cleanup(path: PathBuf) {
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
}

#[test]
fn tab_strip_renders_the_configured_tabs_in_order() {
    let path = temp_db("strip");
    let (ctx, mut app, mut time) = open_app(&path);
    let output = frame(&mut app, &ctx, vec![], &mut time);
    let shown = texts(&output);
    let mut lefts = vec![];
    for tab in DetailTab::default_tabs() {
        assert!(
            shown.iter().any(|t| t == tab.caption()),
            "{} must be drawn",
            tab.caption()
        );
        lefts.push(app.action_rects[&format!("detail_tab_{}", tab.key())][0]);
    }
    assert!(
        lefts.windows(2).all(|w| w[0] < w[1]),
        "Tabs keep their configured order"
    );
    for hidden in [
        DetailTab::Predecessors,
        DetailTab::Successors,
        DetailTab::UserDefinedFields,
    ] {
        assert!(!app
            .action_rects
            .contains_key(&format!("detail_tab_{}", hidden.key())));
    }
    // Every tab renders without errors for the selected activity.
    for tab in DetailTab::default_tabs() {
        click(
            &mut app,
            &ctx,
            &format!("detail_tab_{}", tab.key()),
            &mut time,
        );
        assert!(app.detail.tab == tab);
        assert!(app.error.is_none(), "{}: {:?}", tab.caption(), app.error);
    }
    // A custom layout changes what the strip shows.
    app.detail.tabs = vec![DetailTab::Notebook, DetailTab::General];
    app.detail.tab = DetailTab::Notebook;
    let output = frame(&mut app, &ctx, vec![], &mut time);
    let shown = texts(&output);
    assert!(shown.iter().any(|t| t == "Notebook"));
    assert!(!app.action_rects.contains_key("detail_tab_status"));
    assert!(app.action_rects["detail_tab_notebook"][0] < app.action_rects["detail_tab_general"][0]);
    drop(app);
    cleanup(path);
}

#[test]
fn tab_layout_preference_saves_restores_and_rejects_invalid_data() {
    let path = temp_db("layout");
    let (ctx, mut app, mut time) = open_app(&path);
    app.open_customize_details();
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    app.customize_pick(DetailTab::Status);
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "customize_tabs_up", &mut time);
    click(&mut app, &ctx, "customize_tabs_ok", &mut time);
    assert!(app.detail.customize.is_none());
    assert_eq!(
        &app.detail.tabs[..2],
        &[DetailTab::Status, DetailTab::General]
    );
    // Activating a hidden tab makes it visible and saves the layout.
    app.show_detail_tab(DetailTab::UserDefinedFields);
    let expected = app.detail.tabs.clone();
    assert!(expected.contains(&DetailTab::UserDefinedFields));
    drop(app);

    let ctx = egui::Context::default();
    let mut reopened = App::new(
        &ctx,
        Store::open(path.clone()).unwrap(),
        openxer_xer::starter(),
    );
    assert_eq!(reopened.detail.tabs, expected);
    let _ = frame(&mut reopened, &ctx, vec![], &mut time);
    assert!(reopened.action_rects.contains_key("detail_tab_udf"));
    // A layout saved with the removed placeholder tabs keeps the rest.
    reopened
        .store
        .set_preference(
            LAYOUT_KEY,
            "[\"general\",\"discussion\",\"tasks\",\"notebook\"]",
        )
        .unwrap();
    assert_eq!(
        parse_layout(&reopened.store.preference(LAYOUT_KEY).unwrap().unwrap()),
        Some(vec![DetailTab::General, DetailTab::Notebook])
    );
    reopened
        .store
        .set_preference(LAYOUT_KEY, "[\"general\",\"no_such_tab\"]")
        .unwrap();
    drop(reopened);
    let ctx = egui::Context::default();
    let fresh = App::new(
        &ctx,
        Store::open(path.clone()).unwrap(),
        openxer_xer::starter(),
    );
    assert_eq!(fresh.detail.tabs, DetailTab::default_tabs());
    drop(fresh);
    cleanup(path);
}

#[test]
fn detail_table_edits_undo_and_redo_from_the_toolbar() {
    let path = temp_db("undo");
    let (ctx, mut app, mut time) = open_app(&path);
    let (task_id, pid) = app.ids(app.selected.unwrap());
    click(&mut app, &ctx, "detail_tab_steps", &mut time);
    click(&mut app, &ctx, "detail_steps_add", &mut time);
    let count =
        |app: &App| records::rows_for(&app.project.document, "TASKPROC", task_id, pid).len();
    assert_eq!(count(&app), 1);
    assert!(app.dirty);
    click(&mut app, &ctx, "detail_steps_add", &mut time);
    assert_eq!(count(&app), 2);
    click(&mut app, &ctx, "undo", &mut time);
    assert_eq!(count(&app), 1);
    assert_eq!(app.records_revision, app.history.revision());
    click(&mut app, &ctx, "undo", &mut time);
    assert_eq!(count(&app), 0);
    click(&mut app, &ctx, "redo", &mut time);
    assert_eq!(count(&app), 1);
    assert_eq!(app.history.counts().1, 1);
    drop(app);
    cleanup(path);
}

#[test]
fn started_and_finished_checkboxes_progress_the_activity() {
    let path = temp_db("status");
    let (ctx, mut app, mut time) = open_app(&path);
    click(&mut app, &ctx, "detail_tab_status", &mut time);
    let i = app.selected.unwrap();
    click(&mut app, &ctx, "detail_started", &mut time);
    let a = &app.project.model.activities[i];
    assert_eq!(a.status, StatusType::InProgress);
    assert!(a.act_start.is_some());
    click(&mut app, &ctx, "detail_finished", &mut time);
    let a = &app.project.model.activities[i];
    assert_eq!(a.status, StatusType::Completed);
    assert!(a.act_finish >= a.act_start);
    assert_eq!(a.rem, 0);
    click(&mut app, &ctx, "detail_started", &mut time);
    let a = &app.project.model.activities[i];
    assert_eq!(a.status, StatusType::NotStarted);
    assert!(a.act_start.is_none() && a.act_finish.is_none());
    drop(app);
    cleanup(path);
}

#[test]
fn unchecked_started_and_finished_show_the_dates_checking_them_records() {
    let path = temp_db("status_dates");
    let (ctx, mut app, mut time) = open_app(&path);
    click(&mut app, &ctx, "detail_tab_status", &mut time);
    let origin = app.project.model.origin_serial;
    let date = |v: i64| crate::format::date(origin, Some(v));
    let shown = |app: &App, i: usize| {
        assert_eq!(
            app.project.model.activities[i].status,
            StatusType::NotStarted
        );
        (app.default_actual_start(i), app.default_actual_finish(i))
    };
    // An actual is never after the data date, so a planned date later than
    // it shows the data date. Put the data date after every planned date so
    // the planned dates show.
    let latest = app
        .project
        .model
        .activities
        .iter()
        .filter_map(|a| a.target_finish.max(a.early_finish))
        .max()
        .unwrap();
    app.project.model.project.data_date = Some(latest + 1440);
    let first = app.selected.unwrap();
    let other = (0..app.project.model.activities.len())
        .find(|&j| {
            j != first
                && app.project.model.activities[j].status == StatusType::NotStarted
                && shown(&app, j) != shown(&app, first)
        })
        .expect("a second activity with other dates");
    // The planned dates are the defaults, not the data date.
    let a = &app.project.model.activities[other];
    assert_eq!(
        shown(&app, other).0,
        a.target_start.or(a.early_start).unwrap()
    );
    assert_eq!(
        shown(&app, other).1,
        a.target_finish.or(a.early_finish).unwrap()
    );
    let mut boxes = vec![];
    for i in [first, other] {
        app.selected = Some(i);
        let _ = frame(&mut app, &ctx, vec![], &mut time);
        let output = frame(&mut app, &ctx, vec![], &mut time);
        // The text drawn inside a field's own box, not the same date elsewhere
        // (the table's Start and Finish columns show it too).
        let inside = |key: &str| -> Vec<String> {
            let [l, t, r, b] = app.action_rects[&format!("detail_date_{i}-{key}")];
            let rect = egui::Rect::from_min_max(egui::pos2(l, t), egui::pos2(r, b));
            output
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Text(s) if rect.contains(s.visual_bounding_rect().center()) => {
                        Some(s.galley.text().to_string())
                    }
                    _ => None,
                })
                .collect()
        };
        let (start, finish) = shown(&app, i);
        assert_eq!(inside("act_start"), vec![date(start)], "Started");
        assert_eq!(inside("act_finish"), vec![date(finish)], "Finished");
        boxes.push((start, finish));
    }
    // Showing the dates does not start or edit anything.
    let a = &app.project.model.activities[other];
    assert!(a.act_start.is_none() && a.act_finish.is_none());
    assert!(!app.dirty);
    // Checking the boxes records exactly the dates they showed.
    let (start, finish) = boxes[1];
    // Starting ahead of predecessors asks first; Yes keeps the change.
    let keep = |app: &mut App, time: &mut f64| {
        if app.sched.sequence.is_some() {
            click(app, &ctx, "sequence_yes", time);
        }
    };
    click(&mut app, &ctx, "detail_started", &mut time);
    keep(&mut app, &mut time);
    let a = &app.project.model.activities[other];
    assert_eq!(
        (a.status, a.act_start),
        (StatusType::InProgress, Some(start))
    );
    click(&mut app, &ctx, "detail_finished", &mut time);
    keep(&mut app, &mut time);
    let a = &app.project.model.activities[other];
    assert_eq!(a.status, StatusType::Completed);
    assert_eq!((a.act_start, a.act_finish), (Some(start), Some(finish)));
    drop(app);
    cleanup(path);
}

#[test]
fn starting_an_activity_out_of_sequence_prompts_and_no_reverts() {
    let path = temp_db("status-oos");
    let (ctx, mut app, mut time) = open_app(&path);
    app.editing.options.warn_out_of_sequence = true;
    let model = &app.project.model;
    let succ = model
        .links
        .iter()
        .find(|l| {
            l.link_type == LinkType::FS
                && model.activities[l.pred].act_finish.is_none()
                && model.activities[l.succ].status == StatusType::NotStarted
        })
        .map(|l| l.succ)
        .expect("an FS link with an unfinished predecessor");
    app.jump_to(succ);
    click(&mut app, &ctx, "detail_tab_status", &mut time);
    assert_eq!(app.selected, Some(succ));
    click(&mut app, &ctx, "detail_started", &mut time);
    let prompt = app.sched.sequence.clone().expect("out-of-sequence prompt");
    assert_eq!(prompt.activity, succ);
    assert!(prompt.status_revert.is_some());
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "sequence_no", &mut time);
    assert!(app.sched.sequence.is_none());
    let a = &app.project.model.activities[succ];
    assert_eq!(
        a.status,
        StatusType::NotStarted,
        "No reverts the status change"
    );
    assert!(a.act_start.is_none());
    click(&mut app, &ctx, "detail_started", &mut time);
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "sequence_yes", &mut time);
    assert_eq!(
        app.project.model.activities[succ].status,
        StatusType::InProgress,
        "Yes keeps it"
    );
    // The table's Activity Status cell uses the same rules: the same
    // prompt, and No reverts the status and its actual dates.
    assert!(app.apply_cell(succ, "activity_status", "Not Started"));
    assert!(app.sched.sequence.is_none(), "unstarting is in sequence");
    assert!(app.apply_cell(succ, "activity_status", "In Progress"));
    let a = &app.project.model.activities[succ];
    assert!(a.act_start.is_some(), "the actual start is filled");
    let prompt = app.sched.sequence.clone().expect("the table prompts too");
    assert_eq!(prompt.activity, succ);
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "sequence_no", &mut time);
    let a = &app.project.model.activities[succ];
    assert_eq!(a.status, StatusType::NotStarted);
    assert!(a.act_start.is_none());
    // Completed from the table fills both actual dates, as Finished does.
    assert!(app.apply_cell(succ, "activity_status", "Completed"));
    assert!(app.sched.sequence.is_some());
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "sequence_yes", &mut time);
    let a = &app.project.model.activities[succ];
    assert_eq!(a.status, StatusType::Completed);
    assert!(a.act_start.is_some() && a.act_finish >= a.act_start);
    assert_eq!(a.rem, 0);
    drop(app);
    cleanup(path);
}

#[test]
fn steps_notebook_codes_and_udf_edits_survive_export_and_reopen() {
    let path = temp_db("roundtrip");
    let (ctx, mut app, mut time) = open_app(&path);
    {
        let doc = &mut app.project.document;
        doc.table_mut("MEMOTYPE").add(&[
            ("memo_type_id", "47".into()),
            ("seq_num", "1".into()),
            ("task_flag", "Y".into()),
            ("memo_type", "Schedule Basis".into()),
        ]);
        doc.table_mut("ACTVTYPE").add(&[
            ("actv_code_type_id", "205".into()),
            ("seq_num", "0".into()),
            ("actv_code_type", "Area".into()),
            ("proj_id", "1".into()),
            ("actv_code_type_scope", "AS_Project".into()),
        ]);
        for (id, short, name) in [(3174, "N", "North"), (3175, "S", "South")] {
            doc.table_mut("ACTVCODE").add(&[
                ("actv_code_id", id.to_string()),
                ("actv_code_type_id", "205".into()),
                ("actv_code_name", name.into()),
                ("short_name", short.into()),
                ("seq_num", "0".into()),
            ]);
        }
        for (id, label, kind) in [
            (128, "Priority", "FT_TEXT"),
            (129, "Crew Size", "FT_INT"),
            (130, "Permit", "FT_END_DATE"),
        ] {
            doc.table_mut("UDFTYPE").add(&[
                ("udf_type_id", id.to_string()),
                ("table_name", "TASK".into()),
                ("udf_type_name", format!("user_field_{id}")),
                ("udf_type_label", label.into()),
                ("logical_data_type", kind.into()),
            ]);
        }
    }
    let i = app.selected.unwrap();
    let (task_id, pid) = app.ids(i);

    // Steps through the real buttons, then a name edit as the grid writes it.
    click(&mut app, &ctx, "detail_tab_steps", &mut time);
    click(&mut app, &ctx, "detail_steps_add", &mut time);
    let step = records::rows_for(&app.project.document, "TASKPROC", task_id, pid)[0];
    records::set(
        &mut app.project.document,
        "TASKPROC",
        step,
        "proc_name",
        clean("Set\tforms"),
    );

    // Notebook through the topic picker.
    click(&mut app, &ctx, "detail_tab_notebook", &mut time);
    click(&mut app, &ctx, "detail_notebook_add", &mut time);
    assert!(matches!(app.detail.picker, Some(Picker::Topic { .. })));
    app.detail.picker = Some(Picker::Topic { selected: Some(47) });
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "picker_assign", &mut time);
    let memo = records::rows_for(&app.project.document, "TASKMEMO", task_id, pid);
    assert_eq!(memo.len(), 1);
    records::set(
        &mut app.project.document,
        "TASKMEMO",
        memo[0],
        "task_memo",
        records::text_to_memo("Pour after <inspection>\nCrew of 6"),
    );

    // Codes: a second value of the same type replaces the first.
    app.assign_code(i, 205, 3174);
    app.assign_code(i, 205, 3175);
    assert_eq!(
        records::rows_for(&app.project.document, "TASKACTV", task_id, pid).len(),
        1
    );

    // User defined fields; clearing removes the row.
    let doc = &mut app.project.document;
    record_tabs::set_udf(doc, 128, task_id, pid, "udf_text", Some("High".into()));
    record_tabs::set_udf(doc, 129, task_id, pid, "udf_number", Some("6".into()));
    record_tabs::set_udf(
        doc,
        130,
        task_id,
        pid,
        "udf_date",
        Some("2026-12-01 17:00".into()),
    );
    record_tabs::set_udf(doc, 128, task_id, pid, "udf_text", Some("Urgent".into()));
    record_tabs::set_udf(doc, 129, task_id, pid, "udf_number", None);
    app.touch_records();
    app.save();
    assert!(!app.dirty);

    let check = |p: &openxer_xer::ProjectData| {
        let doc = &p.document;
        let steps = records::rows_for(doc, "TASKPROC", task_id, pid);
        assert_eq!(
            records::get(doc, "TASKPROC", steps[0], "proc_name"),
            "Set forms"
        );
        let memo = records::rows_for(doc, "TASKMEMO", task_id, pid);
        assert_eq!(
            records::memo_to_text(&records::get(doc, "TASKMEMO", memo[0], "task_memo")),
            "Pour after <inspection>\nCrew of 6"
        );
        let codes = records::rows_for(doc, "TASKACTV", task_id, pid);
        assert_eq!(
            records::get(doc, "TASKACTV", codes[0], "actv_code_id"),
            "3175"
        );
        let fields = record_tabs::udf_fields(doc, task_id, p.model.origin_serial);
        let value = |id: i64| fields.iter().find(|f| f.type_id == id).unwrap().clone();
        assert_eq!(value(128).text, "Urgent");
        assert_eq!(value(129).number, None);
        assert_eq!(
            date_string(p.model.origin_serial, value(130).date),
            "2026-12-01 17:00"
        );
    };
    // Export text, reparse.
    let text = app.project.updated_document().text();
    let reparsed = openxer_xer::ProjectData::from_document(
        openxer_xer::Document::parse(&text).unwrap(),
        pid,
        "export".into(),
    )
    .unwrap();
    check(&reparsed);
    // Saved workspace reopen.
    let id = app.store.list().unwrap()[0].0;
    let reopened = app.store.load(id).unwrap();
    check(&reopened);
    drop(app);
    cleanup(path);
}

/// Writes `artifacts/headless-details-<tab>.png` for visual review of every tab.
/// Software rasterizing every tab is slow in debug builds, so run it on demand:
/// `cargo.exe test -p openxer-desktop render_every_detail_tab -- --ignored`.
#[test]
#[ignore = "writes review renders to artifacts/ and is slow in debug builds; run on demand"]
fn render_every_detail_tab_to_png() {
    use crate::headless_render::SoftwarePainter;
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let path = temp_db("render");
    let store = Store::open(path.clone()).unwrap();
    let mut project = openxer_xer::starter();
    schedule_model(&mut project.model, &project.resources).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    let i = app
        .project
        .model
        .activities
        .iter()
        .position(|a| a.task_id == 1080)
        .unwrap();
    app.selected = Some(i);
    let pid = app.project.model.project.proj_id;
    {
        let doc = &mut app.project.document;
        for (k, (name, pct)) in [
            ("Excavate to grade", "100"),
            ("Form and rebar", "50"),
            ("Pour and cure", "0"),
        ]
        .into_iter()
        .enumerate()
        {
            records::add_row(
                doc,
                "TASKPROC",
                &[
                    ("proc_id", (k + 1).to_string()),
                    ("task_id", "1080".into()),
                    ("proj_id", pid.to_string()),
                    ("seq_num", (k + 1).to_string()),
                    ("proc_name", name.into()),
                    ("complete_flag", if pct == "100" { "Y" } else { "N" }.into()),
                    ("proc_wt", "1".into()),
                    ("complete_pct", pct.into()),
                ],
            );
        }
        doc.table_mut("TASKRSRC").add(&[
            ("taskrsrc_id", "1".into()),
            ("task_id", "1080".into()),
            ("proj_id", pid.to_string()),
            ("rsrc_id", "7".into()),
            ("rsrc_type", "RT_Labor".into()),
            ("target_qty", "40".into()),
            ("remain_qty", "40".into()),
            ("remain_qty_per_hr", "1".into()),
            ("target_cost", "3200".into()),
            ("remain_cost", "3200".into()),
            ("rollup_dates_flag", "Y".into()),
        ]);
    }
    app.project.resource_names.insert(7, "Concrete crew".into());
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    for tab in DetailTab::default_tabs() {
        painter.click_action(
            &mut app,
            &ctx,
            &mut time,
            &format!("detail_tab_{}", tab.key()),
        );
        if matches!(tab, DetailTab::Steps | DetailTab::Predecessors) {
            app.detail.rows[0] = Some(1);
        }
        let (image, _) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        // Crop to the bottom pane to keep the previews small.
        let top = 520;
        let cropped =
            image::imageops::crop_imm(&image, 0, top, image.width(), image.height() - top)
                .to_image();
        cropped
            .save(artifacts.join(format!("headless-details-{}.png", tab.key())))
            .unwrap();
        assert!(app.error.is_none(), "{}: {:?}", tab.caption(), app.error);
    }
    // Full-window previews of the customize dialog and the relationship picker.
    app.open_customize_details();
    let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image
        .save(artifacts.join("headless-details-customize.png"))
        .unwrap();
    painter.click_action(&mut app, &ctx, &mut time, "customize_tabs_cancel");
    app.show_detail_tab(DetailTab::Predecessors);
    painter.click_action(&mut app, &ctx, &mut time, "detail_tab_predecessors");
    painter.click_action(&mut app, &ctx, &mut time, "detail_pred_assign_0");
    assert!(app.detail.picker.is_some());
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image
        .save(artifacts.join("headless-details-assign.png"))
        .unwrap();
    drop(app);
    cleanup(path);
}

#[test]
fn workspace_with_removed_assistant_settings_still_opens() {
    // The in-app AI assistant was removed; workspaces that stored its
    // settings and chats must open as before, with those rows left alone.
    let path = temp_db("old-assistant");
    let settings = r#"{"enabled":true,"executable":"claude","model":""}"#;
    {
        let store = Store::open(path.clone()).unwrap();
        store
            .set_preference("assistant_settings_v1", settings)
            .unwrap();
        store
            .set_preference("assistant_chats_v1:1", "[{\"title\":\"old\"}]")
            .unwrap();
    }
    let (_ctx, app, _time) = open_app(&path);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(
        app.store
            .preference("assistant_settings_v1")
            .unwrap()
            .as_deref(),
        Some(settings)
    );
    drop(app);
    cleanup(path);
}
