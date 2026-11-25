//! Baseline comparison completeness: Update Baseline groups and filter,
//! baseline units and cost columns, Baseline Types, and the earned value
//! date choice.
use super::data::DateBasis;
use super::store::{self, Choice, UserBaselines};
use super::*;
use crate::dictionaries::{Category, Cmd};
use crate::grid::columns::{self, Value};
use crate::headless_render::SoftwarePainter;
use crate::records;
use openxer_engine::calendar::DAY;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("openxer-m4-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The starter with labor and equipment assignments on two activities and
/// one expense.
fn project_with_units() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    for (id, short, kind) in [("5", "PF", "RT_Labor"), ("6", "CR", "RT_Equip")] {
        records::add_row(
            &mut p.document,
            "RSRC",
            &[
                ("rsrc_id", id.into()),
                ("rsrc_short_name", short.into()),
                ("rsrc_name", short.into()),
                ("rsrc_type", kind.into()),
            ],
        );
    }
    let first = p.model.activities[1].task_id;
    let second = p.model.activities[7].task_id;
    for (id, task, rsrc, q) in [
        ("1", first, "5", [24., 8., 16., 1200., 400., 800.]),
        ("2", second, "5", [40., 0., 40., 2000., 0., 2000.]),
        ("3", second, "6", [10., 0., 10., 500., 0., 500.]),
    ] {
        records::add_row(
            &mut p.document,
            "TASKRSRC",
            &[
                ("taskrsrc_id", id.into()),
                ("task_id", task.to_string()),
                ("proj_id", "1".into()),
                ("rsrc_id", rsrc.into()),
                ("target_qty", q[0].to_string()),
                ("act_reg_qty", q[1].to_string()),
                ("remain_qty", q[2].to_string()),
                ("target_cost", q[3].to_string()),
                ("act_reg_cost", q[4].to_string()),
                ("remain_cost", q[5].to_string()),
            ],
        );
    }
    records::add_row(
        &mut p.document,
        "PROJCOST",
        &[
            ("cost_item_id", "1".into()),
            ("task_id", second.to_string()),
            ("proj_id", "1".into()),
            ("cost_name", "Permit".into()),
            ("target_cost", "300".into()),
            ("remain_cost", "300".into()),
            ("act_cost", "0".into()),
        ],
    );
    p
}

fn app_with(dir: &Path, project: ProjectData) -> (App, egui::Context) {
    let store = Store::open(dir.join("workspace.db")).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    app.save();
    (app, ctx)
}

fn rows(app: &App) -> Vec<store::BaselineRow> {
    store::list(app.store.connection(), app.workspace_row().unwrap()).unwrap()
}

fn value(app: &App, id: &str, i: usize) -> Value {
    let ix = app.grid_index();
    let catalog = columns::catalog(&app.project);
    columns::value(&app.project, &ix, columns::find(&catalog, id).unwrap(), i)
}

fn baseline_doc(app: &App, id: i64) -> (Document, i64) {
    let row = store::get(app.store.connection(), id).unwrap().unwrap();
    (Document::parse(&row.xer).unwrap(), row.project_key)
}

/// Units and costs of the baseline copy straight from its TASKRSRC and
/// PROJCOST rows: (labor units, nonlabor units, total cost, expense cost).
fn stored_sums(doc: &Document, key: i64, code: &str) -> (f64, f64, f64, f64) {
    let task = xb::activity_codes(doc, key)
        .into_iter()
        .find(|(_, c)| c == code)
        .map(|(id, _)| id)
        .unwrap();
    let num = |s: &str| s.trim().parse::<f64>().unwrap_or(0.);
    let kinds: HashMap<String, String> = doc.table("RSRC").map_or_else(HashMap::new, |t| {
        t.rows
            .iter()
            .map(|r| {
                (
                    t.value(r, "rsrc_id").to_string(),
                    t.value(r, "rsrc_type").to_string(),
                )
            })
            .collect()
    });
    let (mut labor, mut equip, mut cost, mut expense) = (0., 0., 0., 0.);
    if let Some(t) = doc.table("TASKRSRC") {
        for r in &t.rows {
            if t.value(r, "task_id").trim() != task.to_string() {
                continue;
            }
            cost += num(t.value(r, "target_cost"));
            match kinds.get(t.value(r, "rsrc_id")).map(String::as_str) {
                Some("RT_Equip") => equip += num(t.value(r, "target_qty")),
                Some("RT_Mat") => {}
                _ => labor += num(t.value(r, "target_qty")),
            }
        }
    }
    if let Some(t) = doc.table("PROJCOST") {
        for r in &t.rows {
            if t.value(r, "task_id").trim() == task.to_string() {
                expense += num(t.value(r, "target_cost"));
            }
        }
    }
    (labor, equip, cost + expense, expense)
}

fn delay(app: &mut App, i: usize) {
    let a = &mut app.project.model.activities[i];
    for d in [
        &mut a.early_start,
        &mut a.early_finish,
        &mut a.restart,
        &mut a.reend,
    ] {
        *d = d.map(|m| m + 2 * DAY);
    }
}

#[test]
fn baseline_units_and_cost_columns_equal_the_baseline_and_variances_are_current_minus_baseline() {
    let dir = temp("cols");
    let (mut app, _ctx) = app_with(&dir, project_with_units());
    let id = app.add_baseline_copy(1.).unwrap();
    let users = UserBaselines([Choice::Baseline(id), Choice::Baseline(id), Choice::Current]);
    app.assign_baselines(Some(Choice::Baseline(id)), users, 2.)
        .unwrap();
    let (doc, key) = baseline_doc(&app, id);
    let n = app.project.model.activities.len();
    let mut seen_cost = false;
    for i in 0..n {
        let code = app.project.model.activities[i].task_code.clone();
        let (labor, equip, cost, expense) = stored_sums(&doc, key, &code);
        seen_cost |= cost > 0.;
        for slot in ["bl_project", "bl1", "bl2"] {
            assert_eq!(
                value(&app, &format!("{slot}_budgeted_labor_units"), i),
                Value::Units(labor),
                "{slot} {code}"
            );
            assert_eq!(
                value(&app, &format!("{slot}_budgeted_nonlabor_units"), i),
                Value::Units(equip)
            );
            assert_eq!(
                value(&app, &format!("{slot}_budgeted_total_cost"), i),
                Value::Cost(cost)
            );
            assert_eq!(
                value(&app, &format!("{slot}_budgeted_expense_cost"), i),
                Value::Cost(expense)
            );
        }
        // The open project stands in for BL3: its own budgets.
        assert_eq!(
            value(&app, "bl3_budgeted_total_cost", i),
            Value::Cost(cost),
            "unchanged project"
        );
    }
    assert!(seen_cost);

    // Baseline minus current: remaining labor 16 -> 20 hours and remaining
    // labor cost 800 -> 1000 on the first assigned activity (over budget,
    // so the variances are negative).
    let table = app.project.document.table_mut("TASKRSRC");
    table.set(0, "remain_qty", "20".into());
    table.set(0, "remain_cost", "1000".into());
    assert_eq!(
        value(&app, "variance_bl_project_labor_units", 1),
        Value::Units(-4.)
    );
    assert_eq!(
        value(&app, "variance_bl1_labor_units", 1),
        Value::Units(-4.)
    );
    assert_eq!(
        value(&app, "variance_bl_project_total_cost", 1),
        Value::Cost(-200.)
    );
    assert_eq!(
        value(&app, "variance_bl1_total_cost", 1),
        Value::Cost(-200.)
    );
    // The baseline's own budget did not move.
    assert_eq!(
        value(&app, "bl_project_budgeted_total_cost", 1),
        Value::Cost(1200.)
    );
    // An activity without assignments reads zero, and one the baseline
    // lacks is blank.
    assert_eq!(
        value(&app, "bl_project_budgeted_labor_units", 0),
        Value::Units(0.)
    );
    let wbs = app.project.wbs[0].id;
    let added = app.project.add_activity(wbs);
    assert_eq!(
        value(&app, "bl_project_budgeted_labor_units", added),
        Value::Empty
    );

    // BL2 date variances follow the stored baseline like BL1's; BL3 is the
    // project itself, which reads its current dates under that choice and so
    // has no variance.
    delay(&mut app, 3);
    let bl1 = value(&app, "variance_bl1_start", 3);
    assert_ne!(bl1, Value::Duration(0));
    assert_eq!(value(&app, "variance_bl2_start", 3), bl1);
    assert_eq!(
        value(&app, "variance_bl2_finish", 3),
        value(&app, "variance_bl1_finish", 3)
    );
    app.set_date_basis(DateBasis::Current);
    assert_eq!(value(&app, "variance_bl3_start", 3), Value::Duration(0));
    assert_eq!(value(&app, "variance_bl3_finish", 3), Value::Duration(0));
}

#[test]
fn baseline_columns_sit_in_their_groups_and_totals_sum() {
    let dir = temp("groups");
    let (mut app, _ctx) = app_with(&dir, project_with_units());
    let id = app.add_baseline_copy(1.).unwrap();
    let users = UserBaselines([Choice::Baseline(id), Choice::None, Choice::None]);
    app.assign_baselines(Some(Choice::Baseline(id)), users, 2.)
        .unwrap();
    let catalog = columns::catalog(&app.project);
    for slot in ["bl_project", "bl1", "bl2", "bl3"] {
        for (suffix, group) in [
            ("budgeted_labor_units", columns::UNITS),
            ("budgeted_nonlabor_units", columns::UNITS),
            ("budgeted_total_cost", columns::COSTS),
            ("budgeted_expense_cost", columns::COSTS),
        ] {
            let c = columns::find(&catalog, &format!("{slot}_{suffix}")).unwrap();
            assert_eq!(c.group, group, "{slot} {suffix}");
            assert!(c.title.starts_with(match slot {
                "bl_project" => "BL Project ",
                "bl1" => "BL1 ",
                "bl2" => "BL2 ",
                _ => "BL3 ",
            }));
        }
    }
    for id in [
        "variance_bl_project_labor_units",
        "variance_bl1_labor_units",
    ] {
        assert_eq!(columns::find(&catalog, id).unwrap().group, columns::UNITS);
    }
    for id in ["variance_bl_project_total_cost", "variance_bl1_total_cost"] {
        assert_eq!(columns::find(&catalog, id).unwrap().group, columns::COSTS);
    }
    for id in [
        "variance_bl2_start",
        "variance_bl2_finish",
        "variance_bl3_start",
        "variance_bl3_finish",
    ] {
        assert_eq!(columns::find(&catalog, id).unwrap().group, columns::DATES);
    }
    // Unassigned slots are blank; a band sums the activities' budgets.
    assert_eq!(value(&app, "bl2_budgeted_labor_units", 1), Value::Empty);
    let ix = app.grid_index();
    let members: Vec<usize> = (0..app.project.model.activities.len()).collect();
    let total = crate::grid::group::band_total(
        &app.project,
        &ix,
        columns::find(&catalog, "bl_project_budgeted_total_cost").unwrap(),
        &members,
    );
    assert_eq!(total, Some(Value::Cost(1200. + 2000. + 500. + 300.)));
}

/// The baseline's TASK dates and the sum of its assignment quantities.
fn baseline_state(app: &App, id: i64, code: &str) -> (String, f64, usize) {
    let (doc, key) = baseline_doc(app, id);
    let task = doc.table("TASK").unwrap();
    let start = task
        .rows
        .iter()
        .find(|r| {
            records::num(task.value(r, "proj_id")) == key && task.value(r, "task_code") == code
        })
        .map(|r| task.value(r, "early_start_date").to_string())
        .unwrap_or_default();
    let units = doc.table("TASKRSRC").map_or(0., |t| {
        t.rows
            .iter()
            .filter(|r| records::num(t.value(r, "proj_id")) == key)
            .map(|r| t.value(r, "target_qty").trim().parse::<f64>().unwrap_or(0.))
            .sum()
    });
    (start, units, xb::activity_codes(&doc, key).len())
}

#[test]
fn update_dialog_with_only_dates_changes_dates_and_leaves_the_rest() {
    let dir = temp("update");
    let (mut app, ctx) = app_with(&dir, project_with_units());
    let id = app.add_baseline_copy(1.).unwrap();
    let code = app.project.model.activities[3].task_code.clone();
    let before = baseline_state(&app, id, &code);
    // Changes in the project: dates, units and a new activity.
    delay(&mut app, 3);
    app.project
        .document
        .table_mut("TASKRSRC")
        .set(0, "target_qty", "99".into());
    let wbs = app.project.wbs[0].id;
    let new = app.project.add_activity(wbs);
    let new_code = app.project.model.activities[new].task_code.clone();

    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    app.open_maintain_baselines();
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "baselines_row_0");
    painter.click_action(&mut app, &ctx, &mut time, "baselines_update");
    assert!(app.baselines.update.is_some());
    // The defaults, then Activity > Dates, durations, data date, and no log.
    painter.click_action(&mut app, &ctx, &mut time, "update_opt_log_to_file");
    painter.click_action(&mut app, &ctx, &mut time, "update_category_row_3");
    painter.click_action(&mut app, &ctx, &mut time, "update_opt_dates");
    let opts = app.baselines.update.as_ref().unwrap().opts.clone();
    assert_eq!(
        opts,
        UpdateOptions {
            dates: true,
            log_to_file: false,
            ..App::default_update_options()
        }
    );
    // The dialog is laid out and drawn.
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/baselines");
    std::fs::create_dir_all(&out).unwrap();
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    let close = app.action_rects["baselines_update_title_close"];
    let size = Vec2::new(700., 470.) + Vec2::new(2., 32.);
    let crop = image::imageops::crop_imm(
        &image,
        (close[2] + 1. - size.x).round() as u32,
        (close[1] - 1.).round() as u32,
        size.x as u32,
        size.y as u32,
    )
    .to_image();
    crop.save(out.join("update-baseline.png")).unwrap();
    painter.click_action(&mut app, &ctx, &mut time, "update_ok");
    assert_eq!(app.baselines.update_done, Some(None), "done, no log");
    painter.click_action(&mut app, &ctx, &mut time, "update_done_ok");
    assert!(app.baselines.update_done.is_none());
    assert!(app.baselines.update.is_some(), "the options stay open");
    let after = baseline_state(&app, id, &code);
    assert_ne!(after.0, before.0, "dates changed");
    assert_eq!(after.1, before.1, "units left alone");
    assert_eq!(after.2, before.2, "activity list left alone");
    let (doc, key) = baseline_doc(&app, id);
    assert!(!xb::activity_codes(&doc, key)
        .values()
        .any(|c| *c == new_code));
    // One undo step puts the baseline back.
    app.history_move(&ctx, false);
    assert_eq!(baseline_state(&app, id, &code), before);

    // With Activities added: the new activity appears, one undo step.
    let added = UpdateOptions {
        add_new: true,
        update_existing: false,
        log_to_file: false,
        ..UpdateOptions::default()
    };
    app.update_baseline(id, &added, 3.).unwrap();
    let (doc, key) = baseline_doc(&app, id);
    assert!(xb::activity_codes(&doc, key)
        .values()
        .any(|c| *c == new_code));
    let after = baseline_state(&app, id, &code);
    assert_eq!(after.0, before.0, "dates left alone");
    assert_eq!(after.1, before.1, "units left alone");
    app.history_move(&ctx, false);
    assert_eq!(baseline_state(&app, id, &code), before);
    assert!(app.error.is_none(), "{:?}", app.error);
}

#[test]
fn update_filter_limits_the_activities() {
    let dir = temp("filter");
    let (mut app, _ctx) = app_with(&dir, project_with_units());
    let id = app.add_baseline_copy(1.).unwrap();
    let a = app.project.model.activities[3].task_code.clone();
    let b = app.project.model.activities[4].task_code.clone();
    let (a0, b0) = (
        baseline_state(&app, id, &a).0,
        baseline_state(&app, id, &b).0,
    );
    delay(&mut app, 3);
    delay(&mut app, 4);
    // A saved filter that passes only the first activity.
    app.grid
        .user_filters
        .push(crate::grid::filters::UserFilter {
            name: "Only one".into(),
            match_all: true,
            rows: vec![crate::grid::filters::Criterion {
                column: "activity_id".into(),
                op: crate::grid::filters::Op::Equals,
                value: a.clone(),
                high: String::new(),
                level: 0,
            }],
        });
    let passed = app.filtered_codes("user:Only one");
    assert_eq!(passed, BTreeSet::from([a.clone()]));
    let opts = UpdateOptions {
        dates: true,
        use_filter: true,
        filters: vec!["user:Only one".into()],
        log_to_file: false,
        ..UpdateOptions::default()
    };
    app.update_baseline(id, &opts, 2.).unwrap();
    assert_ne!(baseline_state(&app, id, &a).0, a0, "the filtered activity");
    assert_eq!(baseline_state(&app, id, &b).0, b0, "the other one is left");
    assert!(app.error.is_none(), "{:?}", app.error);
}

fn run(app: &mut App, cmd: Cmd) -> Option<i64> {
    app.run_dictionary(cmd).unwrap()
}

#[test]
fn baseline_types_are_edited_stored_exported_and_deleted_with_their_baselines() {
    let dir = temp("types");
    let (mut app, ctx) = app_with(&dir, project_with_units());
    let id = app.add_baseline_copy(1.).unwrap();
    let names =
        |app: &App| -> Vec<String> { app.baseline_type_list().into_iter().map(|t| t.1).collect() };
    assert_eq!(names(&app), ["<None>"]);
    // Add, name and rename types.
    let first = run(&mut app, Cmd::AddCategory(Category::Baseline, None)).unwrap();
    run(
        &mut app,
        Cmd::SetCategory(
            Category::Baseline,
            first,
            "base_type",
            "Customer Sign-Off".into(),
        ),
    );
    let second = run(&mut app, Cmd::AddCategory(Category::Baseline, Some(first))).unwrap();
    run(
        &mut app,
        Cmd::SetCategory(
            Category::Baseline,
            second,
            "base_type",
            "Mid-Project Status".into(),
        ),
    );
    assert_eq!(
        names(&app),
        ["<None>", "Customer Sign-Off", "Mid-Project Status"]
    );
    // The Maintain Baselines picker offers them; choosing one stores it.
    app.rename_baseline(id, &rows(&app)[0].name.clone(), &first.to_string(), 5.)
        .unwrap();
    assert_eq!(rows(&app)[0].baseline_type, first.to_string());
    run(
        &mut app,
        Cmd::SetCategory(
            Category::Baseline,
            first,
            "base_type",
            "Approved Plan".into(),
        ),
    );
    assert_eq!(names(&app)[1], "Approved Plan");
    assert_eq!(
        app.baselines_with_type(first).len(),
        1,
        "renaming keeps the link"
    );

    // Save and reopen.
    app.save();
    let key = app.project.model.project.proj_id;
    let source = app.project.source_name.clone();
    drop(app);
    let store = Store::open(dir.join("workspace.db")).unwrap();
    let project = store.last().unwrap().unwrap();
    let app2_ctx = egui::Context::default();
    let mut app = App::new(&app2_ctx, store, project);
    assert_eq!(
        names(&app),
        ["<None>", "Approved Plan", "Mid-Project Status"]
    );
    assert_eq!(rows(&app)[0].baseline_type, first.to_string());

    // XER export and reimport into a second workspace.
    let parent = app.workspace_row().unwrap();
    let mut out =
        openxer_interchange::xer_export::export_document(&[&app.project], "25.12").unwrap();
    assert_eq!(
        export_baselines(app.store.connection(), &mut out, parent, key).unwrap(),
        1
    );
    let back = Document::parse(&out.text()).unwrap();
    let t = back.table("BASETYPE").expect("BASETYPE exported");
    assert_eq!(t.fields, ["base_type_id", "base_type_seq_num", "base_type"]);
    assert_eq!(t.rows.len(), 2);
    let found = xb::baselines_in(&back);
    assert_eq!(found.len(), 1);
    assert_eq!(
        xb::project_value(&back, found[0].0, "base_type_id"),
        first.to_string()
    );
    let mut second_store = Store::open(dir.join("second.db")).unwrap();
    let mut q = ProjectData::from_document(back.clone(), key, "second".into()).unwrap();
    q.source_name = "second".into();
    let row = second_store.save(&q).unwrap();
    assert_eq!(
        attach_file_baselines(second_store.connection(), row, &back, key, None).unwrap(),
        1
    );
    assert_eq!(
        store::list(second_store.connection(), row).unwrap()[0].baseline_type,
        first.to_string()
    );
    let _ = source;

    // Deleting a used type clears it on its baselines, as one undo step.
    run(&mut app, Cmd::DeleteCategory(Category::Baseline, first));
    assert_eq!(names(&app), ["<None>", "Mid-Project Status"]);
    assert_eq!(rows(&app)[0].baseline_type, "");
    assert!(app.baselines_with_type(first).is_empty());
    app.history_move(&ctx, false);
    assert_eq!(
        names(&app),
        ["<None>", "Approved Plan", "Mid-Project Status"]
    );
    assert_eq!(rows(&app)[0].baseline_type, first.to_string());
    app.history_move(&ctx, true);
    assert_eq!(names(&app), ["<None>", "Mid-Project Status"]);
    assert_eq!(rows(&app)[0].baseline_type, "");
    // Deleting an unused type touches no baseline.
    run(&mut app, Cmd::DeleteCategory(Category::Baseline, second));
    assert_eq!(names(&app), ["<None>"]);
    assert!(app.error.is_none(), "{:?}", app.error);
}

#[test]
fn earned_value_date_choice_changes_bl_dates_and_earned_value_only() {
    let dir = temp("basis");
    let (mut app, _ctx) = app_with(&dir, project_with_units());
    let id = app.add_baseline_copy(1.).unwrap();
    // In the stored baseline, give every activity planned dates 80 days
    // before its current dates, so they lie before the data date.
    let mut row = store::get(app.store.connection(), id).unwrap().unwrap();
    let mut doc = Document::parse(&row.xer).unwrap();
    {
        let t = doc.table_mut("TASK");
        for i in 0..t.rows.len() {
            for (planned, current) in [
                ("target_start_date", "early_start_date"),
                ("target_end_date", "early_end_date"),
            ] {
                let v = t.value(&t.rows[i], current).to_string();
                let m = openxer_xer::parse_absolute(&v).unwrap();
                let early = m - 80 * 1440;
                t.set(i, planned, openxer_xer::date_string(0, Some(early)));
            }
        }
    }
    row.xer = doc.text();
    store::put(app.store.connection(), &row).unwrap();
    app.baselines_changed();
    let users = UserBaselines([Choice::Baseline(id), Choice::None, Choice::None]);
    app.assign_baselines(Some(Choice::Baseline(id)), users, 2.)
        .unwrap();
    let i = 8;
    let snapshot = |app: &App| {
        let ix = app.grid_index();
        let spans = app
            .baseline_compare()
            .spans(&app.project, &app.project.model.activities[i]);
        let others: Vec<Value> = [
            "start",
            "finish",
            "budgeted_total_cost",
            "bl_project_duration",
        ]
        .iter()
        .map(|c| value(app, c, i))
        .collect();
        (
            value(app, "bl_project_start", i),
            value(app, "bl_project_finish", i),
            crate::grid::columns_ev::schedule_percent(&app.project, &ix, i),
            spans,
            others,
        )
    };
    assert_eq!(app.date_basis(), DateBasis::Planned);
    let planned = snapshot(&app);
    app.set_date_basis(DateBasis::Current);
    assert_eq!(app.date_basis(), DateBasis::Current);
    let current = snapshot(&app);
    assert_ne!(planned.0, current.0, "BL Project Start");
    assert_ne!(planned.1, current.1, "BL Project Finish");
    assert_ne!(planned.2, current.2, "schedule percent complete");
    // The current dates of the baseline are its Start and Finish: the
    // project has not moved, so there is no variance.
    let (s, f) = (value(&app, "start", i), value(&app, "finish", i));
    assert_eq!(current.0, s);
    assert_eq!(current.1, f);
    // Nothing else moved: bars, other columns, the budgets.
    assert_eq!(planned.3, current.3, "Gantt bars");
    assert_eq!(planned.4, current.4, "other columns");
    // The choice is stored in the workspace.
    app.save();
    drop(app);
    let store = Store::open(dir.join("workspace.db")).unwrap();
    let project = store.last().unwrap().unwrap();
    let ctx = egui::Context::default();
    let app = App::new(&ctx, store, project);
    assert_eq!(app.date_basis(), DateBasis::Current);
}

fn crop(image: &image::RgbaImage, app: &App, salt: &str, client: Vec2) -> image::RgbaImage {
    let close = app.action_rects[&format!("{salt}_title_close")];
    let size = client + Vec2::new(2., 32.);
    image::imageops::crop_imm(
        image,
        (close[2] + 1. - size.x).round() as u32,
        (close[1] - 1.).round() as u32,
        size.x as u32,
        size.y as u32,
    )
    .to_image()
}

#[test]
fn admin_dialogs_show_baseline_types_and_the_earned_value_choice() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/baselines");
    std::fs::create_dir_all(&out).unwrap();
    let dir = temp("admin");
    let (mut app, ctx) = app_with(&dir, project_with_units());
    let id = app.add_baseline_copy(1.).unwrap();
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    // Admin Categories opens on Baseline Types, a live tab.
    app.open_dictionary(crate::dictionaries::Dictionary::AdminCategories);
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "admin_tab_baseline");
    painter.click_action(&mut app, &ctx, &mut time, "admin_add");
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    assert_eq!(app.baseline_type_list().len(), 2, "Add made a type");
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    let close = app.action_rects["admin_close"];
    image::imageops::crop_imm(
        &image,
        (close[2] + 8. - 592.).max(0.) as u32,
        (close[3] + 8. - 490.).max(0.) as u32,
        592,
        490,
    )
    .to_image()
    .save(out.join("admin-baseline-types.png"))
    .unwrap();
    // The picker in Maintain Baselines offers the new type.
    assert_eq!(app.baseline_type_list()[1].1, "New Baseline Type");
    let type_id: i64 = app.baseline_type_list()[1].0.parse().unwrap();
    painter.click_action(&mut app, &ctx, &mut time, "admin_close");
    assert!(!app.dictionary_open());
    app.rename_baseline(id, "Plan", &type_id.to_string(), 3.)
        .unwrap();
    // Admin Preferences > Earned Value: the combo changes the stored choice.
    app.open_admin_preferences();
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "admin_tab_earned_value");
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    crop(&image, &app, "admin_preferences", Vec2::new(609., 560.))
        .save(out.join("admin-earned-value.png"))
        .unwrap();
    assert_eq!(app.date_basis(), DateBasis::Planned);
    painter.click_action(&mut app, &ctx, &mut time, "admin_ev_dates");
    painter.click_action(&mut app, &ctx, &mut time, "admin_ev_dates_item_1");
    assert_eq!(app.date_basis(), DateBasis::Current);
    assert_eq!(
        app.store.preference("earned_value_baseline_dates").unwrap(),
        Some("current".to_string())
    );
    painter.click_action(&mut app, &ctx, &mut time, "admin_close");
    assert!(app.error.is_none(), "{:?}", app.error);
}

/// A user filter passing the activities whose IDs are `codes`.
fn ids_filter(name: &str, codes: &[&String]) -> crate::grid::filters::UserFilter {
    crate::grid::filters::UserFilter {
        name: name.into(),
        match_all: false,
        rows: codes
            .iter()
            .map(|c| crate::grid::filters::Criterion {
                column: "activity_id".into(),
                op: crate::grid::filters::Op::Equals,
                value: (*c).clone(),
                high: String::new(),
                level: 0,
            })
            .collect(),
    }
}

/// Several filters ticked in the picker: with Any the update reaches the
/// activities that pass either (the union), with All those that pass both
/// (the intersection).
#[test]
fn several_filters_with_any_take_the_union_and_with_all_the_intersection() {
    let dir = temp("filters-any-all");
    let (mut app, _ctx) = app_with(&dir, project_with_units());
    let id = app.add_baseline_copy(1.).unwrap();
    let code = |app: &App, i: usize| app.project.model.activities[i].task_code.clone();
    let (a, b, c) = (code(&app, 2), code(&app, 3), code(&app, 4));
    app.grid.user_filters.push(ids_filter("A or B", &[&a, &b]));
    app.grid.user_filters.push(ids_filter("B or C", &[&b, &c]));
    let keys = vec!["user:A or B".to_string(), "user:B or C".to_string()];
    assert_eq!(
        app.filtered_codes_of(&keys, false),
        BTreeSet::from([a.clone(), b.clone(), c.clone()])
    );
    assert_eq!(
        app.filtered_codes_of(&keys, true),
        BTreeSet::from([b.clone()])
    );
    let before: Vec<_> = [&a, &b, &c]
        .iter()
        .map(|x| baseline_state(&app, id, x).0)
        .collect();
    for i in [2, 3, 4] {
        delay(&mut app, i);
    }
    let changed = |app: &App| -> BTreeSet<String> {
        [&a, &b, &c]
            .iter()
            .zip(&before)
            .filter(|(x, old)| baseline_state(app, id, x).0 != **old)
            .map(|(x, _)| (*x).clone())
            .collect()
    };
    let mut o = UpdateOptions {
        dates: true,
        use_filter: true,
        filters: keys.clone(),
        filter_match_all: true,
        log_to_file: false,
        ..UpdateOptions::default()
    };
    // The preview and the update agree, and All reaches B only.
    let preview = app.preview_baseline_update(id, &o).unwrap();
    let listed: BTreeSet<String> = preview
        .activities
        .iter()
        .map(|x| x.object.clone())
        .collect();
    assert_eq!(listed, BTreeSet::from([b.clone()]));
    app.update_baseline(id, &o, 2.).unwrap();
    assert_eq!(changed(&app), BTreeSet::from([b.clone()]));
    // Any reaches all three.
    o.filter_match_all = false;
    let preview = app.preview_baseline_update(id, &o).unwrap();
    let listed: BTreeSet<String> = preview
        .activities
        .iter()
        .map(|x| x.object.clone())
        .collect();
    assert_eq!(
        listed,
        BTreeSet::from([a.clone(), c.clone()]),
        "B is already updated"
    );
    app.update_baseline(id, &o, 3.).unwrap();
    assert_eq!(
        changed(&app),
        BTreeSet::from([a.clone(), b.clone(), c.clone()])
    );
    // The choice is kept with the options.
    app.save_update_options(&o);
    let saved = app.saved_update_options();
    assert_eq!((saved.filters, saved.filter_match_all), (keys, false));
    assert!(app.error.is_none(), "{:?}", app.error);
}
