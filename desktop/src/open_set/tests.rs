//! Several open projects: composite table, F9 across projects, default
//! project, save and reopen, XER round trip, undo, headless renders.
use super::*;
use crate::headless_render::SoftwarePainter;
use crate::store::Store;
use openxer_engine::options::OutOfSequence;

const SOURCE: &str = "portfolio.xer";

fn temp_db(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "openxer-multiproj-{name}-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

/// RIVERBEND (active, default) and NORTHYARD open together; their workspace ids.
fn portfolio_app(name: &str) -> (App, egui::Context, PathBuf, i64, i64) {
    let doc = openxer_xer::starter_portfolio();
    let rb = ProjectData::from_document(doc.clone(), 1, SOURCE.into()).unwrap();
    let ny = ProjectData::from_document(doc, 2, SOURCE.into()).unwrap();
    let db = temp_db(name);
    let mut store = Store::open(db.clone()).unwrap();
    let ny_id = store.save(&ny).unwrap();
    let rb_id = store.save(&rb).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, rb);
    app.restore_open_set();
    app.open_project_set(&[rb_id, ny_id]);
    assert_eq!(app.open_ids(), vec![rb_id, ny_id]);
    assert_eq!(app.open_set.default, Some(rb_id));
    (app, ctx, db, rb_id, ny_id)
}

fn ny(app: &App) -> &ProjectData {
    app.open_projects()
        .into_iter()
        .find(|p| p.model.project.proj_id == 2)
        .unwrap()
}

fn rb(app: &App) -> &ProjectData {
    app.open_projects()
        .into_iter()
        .find(|p| p.model.project.proj_id == 1)
        .unwrap()
}

fn activity(p: &ProjectData, task: i64) -> &Activity {
    p.model
        .activities
        .iter()
        .find(|a| a.task_id == task)
        .unwrap()
}

fn absolute(p: &ProjectData, minutes: Option<i64>) -> Option<i64> {
    minutes.map(|m| m + p.model.origin_serial * DAY)
}

#[test]
fn composite_shows_every_open_project_grouped_by_project() {
    let (mut app, _ctx, db, _, ny_id) = portfolio_app("composite");
    app.refresh_composite();
    assert_eq!(app.shown().model.activities.len(), 28);
    // The cross-project relationship is drawn in the composite.
    let n = app.active_rows().unwrap();
    assert_eq!(n, 14);
    assert!(app
        .shown()
        .model
        .links
        .iter()
        .any(|l| l.pred < n && l.succ >= n));
    // rows() keeps to the active project, shown_rows() has both.
    let count = |rows: &[grid::Row]| {
        rows.iter()
            .filter(|r| matches!(r, grid::Row::Activity(_)))
            .count()
    };
    assert_eq!(count(&app.rows()), 14);
    let rows = app.shown_rows();
    assert_eq!(count(&rows), 28);
    let bands: Vec<String> = rows
        .iter()
        .filter_map(|r| match r {
            grid::Row::Group(b) => Some(app.band_label(*b)),
            _ => None,
        })
        .collect();
    assert!(bands.iter().any(|b| b.contains("NORTHYARD")), "{bands:?}");
    assert!(bands.iter().any(|b| b.contains("RIVERBEND")), "{bands:?}");
    // A click on a NORTHYARD row makes it the current project.
    let row = rows
        .iter()
        .find_map(|r| match r {
            grid::Row::Activity(i) if *i >= n => Some(*i),
            _ => None,
        })
        .unwrap();
    let code = app.shown().model.activities[row].task_code.clone();
    app.click_row(row, Default::default());
    assert_eq!(app.open_set.active, Some(ny_id));
    let selected = app.selected.unwrap();
    assert_eq!(app.project.model.activities[selected].task_code, code);
    assert!(app
        .status_panels()
        .iter()
        .any(|p| p == "Default Project: RIVERBEND"));
    drop(app);
    let _ = std::fs::remove_file(db);
}

#[test]
fn f9_moves_the_successor_across_projects_unless_ignored() {
    let (mut app, ctx, db, rb_id, ny_id) = portfolio_app("f9");
    app.schedule();
    assert!(app.error.is_none(), "{:?}", app.error);
    let rb_finish = absolute(rb(&app), activity(rb(&app), 1030).early_finish).unwrap();
    let ntp = activity(ny(&app), 2000);
    let ny_start = absolute(ny(&app), ntp.early_start).unwrap();
    let ny_dd = absolute(ny(&app), ny(&app).model.project.data_date).unwrap();
    assert!(ny_start > ny_dd, "the tie moves NORTHYARD's start");
    assert!(ny_start >= rb_finish && ny_start - rb_finish < 3 * DAY);
    // The limit is stored as the external early start.
    assert_eq!(absolute(ny(&app), ntp.ext_early_start), Some(rb_finish));
    let rb_ext = activity(rb(&app), 1030).ext_late_end;
    assert!(
        rb_ext.is_some(),
        "the predecessor stores its external late finish"
    );
    // One F9 is one undo step over both projects.
    assert_eq!(app.open_set.undo.last(), Some(&vec![rb_id, ny_id]));
    app.history_move(&ctx, false);
    assert_eq!(activity(ny(&app), 2000).early_start, None);
    assert_eq!(activity(rb(&app), 1030).early_finish, None);
    app.history_move(&ctx, true);
    assert_eq!(
        absolute(ny(&app), activity(ny(&app), 2000).early_start),
        Some(ny_start)
    );
    // Ignoring relationships to other projects leaves NORTHYARD on its data date.
    app.project.model.options.out_of_sequence = OutOfSequence::None;
    app.schedule();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(
        absolute(ny(&app), activity(ny(&app), 2000).early_start),
        Some(ny_dd)
    );
    drop(app);
    let _ = std::fs::remove_file(db);
}

#[test]
fn a_closed_linked_project_is_replaced_by_stored_external_dates() {
    let (mut app, _ctx, db, rb_id, _) = portfolio_app("closed");
    app.schedule();
    let together = activity(ny(&app), 2000).early_start;
    let finish = activity(ny(&app), 2130).early_finish;
    app.close_project_id(rb_id);
    assert_eq!(app.project.model.project.proj_id, 2);
    assert!(!app.open_set.multi());
    // Single-project F9: the stored external early start holds NORTHYARD.
    app.schedule();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(activity(&app.project, 2000).early_start, together);
    assert_eq!(activity(&app.project, 2130).early_finish, finish);
    // Without it the project would start on its data date.
    let mut alone = app.project.clone();
    for a in &mut alone.model.activities {
        a.ext_early_start = None;
    }
    crate::scheduling::schedule_model(&mut alone.model, &alone.resources).unwrap();
    assert_eq!(
        activity(&alone, 2000).early_start,
        alone.model.project.data_date
    );
    drop(app);
    let _ = std::fs::remove_file(db);
}

#[test]
fn new_activities_go_to_the_default_project() {
    let (mut app, _ctx, db, rb_id, ny_id) = portfolio_app("default");
    app.set_default_project(ny_id);
    assert_eq!(app.open_set.active, Some(rb_id));
    app.new_activity();
    assert_eq!(app.open_set.active, Some(ny_id));
    assert_eq!(ny(&app).model.activities.len(), 15);
    assert_eq!(rb(&app).model.activities.len(), 14);
    let id = app.project.model.activities.last().unwrap().task_id;
    // Unique across the file's other project too.
    assert!(rb(&app)
        .document
        .table("TASK")
        .unwrap()
        .rows
        .iter()
        .all(|r| {
            rb(&app).document.table("TASK").unwrap().value(r, "task_id") != id.to_string()
        }));
    drop(app);
    let _ = std::fs::remove_file(db);
}

#[test]
fn the_open_set_survives_save_and_reopen() {
    let (mut app, _ctx, db, rb_id, ny_id) = portfolio_app("reopen");
    app.set_default_project(ny_id);
    app.schedule();
    app.save();
    assert!(!app.any_dirty());
    drop(app);
    let store = Store::open(db.clone()).unwrap();
    let project = store.last().unwrap().unwrap();
    assert_eq!(
        project.model.project.proj_id, 1,
        "the active project is last"
    );
    let ctx = egui::Context::default();
    let mut again = App::new(&ctx, store, project);
    again.restore_open_set();
    assert_eq!(again.open_ids(), vec![rb_id, ny_id]);
    assert_eq!(again.open_set.default, Some(ny_id));
    assert!(activity(ny(&again), 2000).early_start.is_some());
    assert!(activity(ny(&again), 2000).ext_early_start.is_some());
    drop(again);
    let _ = std::fs::remove_file(db);
}

#[test]
fn multi_project_xer_import_and_export_round_trip() {
    let dir = temp_db("xer").with_extension("");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("portfolio.xer");
    openxer_xer::starter_portfolio().write(&file).unwrap();
    // Import as the wizard's Create New does: one workspace project each.
    let doc = Document::read(&file).unwrap();
    let db = temp_db("xer-db");
    let mut store = Store::open(db.clone()).unwrap();
    let writes = doc
        .projects()
        .into_iter()
        .map(|(pid, _)| {
            crate::interchange::workspace::Write::Insert(
                ProjectData::from_document(doc.clone(), pid, file.display().to_string()).unwrap(),
            )
        })
        .collect();
    let ids = crate::interchange::workspace::commit(&mut store, writes).unwrap();
    assert_eq!(ids.len(), 2);
    let first = store.load(ids[0]).unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx, store, first);
    app.open_project_set(&ids);
    assert!(app.open_set.multi());
    assert_eq!(
        ny(&app).external_links().len(),
        1,
        "the cross-project row is kept"
    );
    // Edit the cross-project relationship from RIVERBEND's Successors grid.
    let i = app
        .project
        .model
        .activities
        .iter()
        .position(|a| a.task_id == 1030)
        .unwrap();
    let lines = app.cross_lines(i, false);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].other_project, "NORTHYARD");
    app.change_cross_link(
        lines[0].owner,
        lines[0].link.row_id,
        Some((LinkType::SS, 480)),
    )
    .unwrap();
    app.schedule();
    assert!(app.error.is_none(), "{:?}", app.error);
    let out =
        openxer_interchange::xer_export::export_document(&app.open_projects(), "25.12").unwrap();
    let back = Document::parse(&out.text()).unwrap();
    assert_eq!(back.projects().len(), 2);
    for p in app.open_projects() {
        let again =
            ProjectData::from_document(back.clone(), p.model.project.proj_id, "x".into()).unwrap();
        assert_eq!(again.model, p.model);
        assert_eq!(again.external_links(), p.external_links());
    }
    let ny_back = ProjectData::from_document(back, 2, "x".into()).unwrap();
    assert_eq!(ny_back.external_links()[0].link_type, LinkType::SS);
    assert_eq!(ny_back.external_links()[0].lag, 480);
    drop(app);
    let _ = std::fs::remove_file(db);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn undo_follows_edits_across_projects() {
    let (mut app, ctx, db, rb_id, ny_id) = portfolio_app("undo");
    // An edit in RIVERBEND, then one in NORTHYARD.
    app.project.details.get_mut(&1010).unwrap().name = "Edited in RIVERBEND".into();
    app.history
        .record(&app.project, app.records_revision, None, 1.);
    app.sync_journal();
    app.activate_id(ny_id);
    app.project.details.get_mut(&2010).unwrap().name = "Edited in NORTHYARD".into();
    app.history
        .record(&app.project, app.records_revision, None, 2.);
    app.sync_journal();
    // Undo from RIVERBEND: the NORTHYARD edit goes first.
    app.activate_id(rb_id);
    app.history_move(&ctx, false);
    assert_eq!(ny(&app).details[&2010].name, "Site investigation & survey");
    assert_eq!(rb(&app).details[&1010].name, "Edited in RIVERBEND");
    assert_eq!(
        app.open_set.active,
        Some(ny_id),
        "undo shows the project it changed"
    );
    app.history_move(&ctx, false);
    assert_eq!(rb(&app).details[&1010].name, "Site investigation & survey");
    app.history_move(&ctx, true);
    app.history_move(&ctx, true);
    assert_eq!(rb(&app).details[&1010].name, "Edited in RIVERBEND");
    assert_eq!(ny(&app).details[&2010].name, "Edited in NORTHYARD");
    // A cross-project relationship added in the Gantt is one undo step.
    let before = ny(&app).external_links().len();
    app.add_cross_link((rb_id, 1130), (ny_id, 2130), LinkType::FF, 0)
        .unwrap();
    app.sync_journal();
    assert_eq!(ny(&app).external_links().len(), before + 1);
    app.history_move(&ctx, false);
    assert_eq!(ny(&app).external_links().len(), before);
    drop(app);
    let _ = std::fs::remove_file(db);
}

#[test]
fn close_all_keeps_the_default_project() {
    let (mut app, _ctx, db, _, ny_id) = portfolio_app("close-all");
    app.set_default_project(ny_id);
    app.close_all_projects();
    assert_eq!(app.open_ids(), vec![ny_id]);
    assert_eq!(app.project.model.project.proj_id, 2);
    assert!(app.open_set.composite.is_none());
    drop(app);
    let _ = std::fs::remove_file(db);
}

#[test]
fn open_dialog_selects_several_projects() {
    let row = |id: Option<i64>, folder: Option<i64>, depth: usize, code: &str| dialogs::OpenRow {
        id,
        folder,
        depth,
        project_id: code.into(),
        name: code.into(),
        open: id == Some(7),
    };
    // Workspace > A, B, and the child folder East > C.
    let mut d = OpenDialog {
        rows: vec![
            row(None, Some(1), 0, "Workspace"),
            row(None, Some(2), 1, "East"),
            row(Some(9), None, 2, "C"),
            row(Some(7), None, 1, "A"),
            row(Some(8), None, 1, "B"),
        ],
        ..Default::default()
    };
    d.click(3, false, false);
    assert_eq!(d.chosen(), vec![7]);
    d.click(4, false, true);
    assert_eq!(d.chosen(), vec![7, 8]);
    d.click(4, true, false);
    assert_eq!(d.chosen(), vec![7]);
    // A folder picks everything under it, child folders included.
    d.click(0, false, false);
    assert_eq!(d.chosen(), vec![9, 7, 8]);
    d.click(1, false, false);
    assert_eq!(d.chosen(), vec![9]);
    // Ctrl+click on a folder adds its projects, again takes them away.
    d.click(3, false, false);
    d.click(1, true, false);
    assert_eq!(d.chosen(), vec![9, 7]);
    d.click(1, true, false);
    assert_eq!(d.chosen(), vec![7]);
    // Collapsing a folder hides what it holds; rows are counted as shown.
    d.toggle(1);
    assert_eq!(d.visible(), vec![0, 1, 3, 4]);
    d.click(2, false, false);
    assert_eq!(d.chosen(), vec![7], "shown row 2 is A");
    d.toggle(1);
    assert_eq!(d.visible().len(), 5);
}
/// Software renders of the Open Project dialog and of the activity table
/// with two projects open (gitignored `artifacts/`).
#[test]
fn render_open_project_dialog_and_grouped_table() {
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let (mut app, ctx, db, _, _) = portfolio_app("render");
    app.schedule();
    // The table scrolls the selected row to the top; pick one near the end of
    // the first project so rows of both projects fit in the pane below the
    // horizontal scroll bars.
    let first_rows = app.active_rows().unwrap();
    app.selected = Some(first_rows - 3);
    app.scroll_to_selected = true;
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, triangles) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    assert!(triangles > 500);
    image
        .save(artifacts.join("multiproj-activities.png"))
        .unwrap();
    // Both projects' bands and bars are on screen.
    let n = app.active_rows().unwrap();
    assert!(app.bar_geometry.iter().any(|b| b.activity >= n));
    assert!(app.bar_geometry.iter().any(|b| b.activity < n));
    app.open_project_dialog();
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image
        .save(artifacts.join("multiproj-open-project.png"))
        .unwrap();
    assert!(app.action_rects.contains_key("open_project_open"));
    painter.click_action(&mut app, &ctx, &mut time, "open_project_cancel");
    assert!(app.open_set.open_dialog.is_none());
    app.default_project_dialog();
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image
        .save(artifacts.join("multiproj-default-project.png"))
        .unwrap();
    println!(
        "Software-rendered: {}",
        artifacts.join("multiproj-activities.png").display()
    );
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(db);
}

// ------------------------------------------- progress across open projects

/// Both projects start with a data date in the past, so a later date is not
/// a date after today.
fn portfolio_in_2020(name: &str) -> (App, egui::Context, PathBuf, SoftwarePainter, f64) {
    let (mut app, ctx, db, _, _) = portfolio_app(name);
    let base = openxer_xer::parse_relative(app.project.model.origin_serial, "2020-01-06 08:00")
        .unwrap()
        .unwrap();
    app.project.model.project.data_date = Some(base);
    let other = openxer_xer::parse_relative(
        app.open_set.parked[0].project.model.origin_serial,
        "2020-01-06 08:00",
    )
    .unwrap()
    .unwrap();
    app.open_set.parked[0].project.model.project.data_date = Some(other);
    app.data_date = openxer_xer::date_string(app.project.model.origin_serial, Some(base));
    app.schedule();
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    (app, ctx, db, painter, time)
}

fn data_dates(app: &App) -> (Option<i64>, Option<i64>) {
    let (a, b) = (rb(app), ny(app));
    (
        absolute(a, a.model.project.data_date),
        absolute(b, b.model.project.data_date),
    )
}

fn text_of(app: &App, project: &ProjectData, day: &str) -> String {
    let _ = app;
    let minutes = openxer_xer::parse_relative(project.model.origin_serial, day)
        .unwrap()
        .unwrap();
    crate::schedule_dialogs::display_date(project.model.origin_serial, Some(minutes))
}

#[test]
fn apply_actuals_with_the_same_date_moves_every_open_project_in_one_undo_step() {
    let (mut app, ctx, db, mut painter, mut time) = portfolio_in_2020("actuals-same");
    let before = data_dates(&app);
    let (rb_before, ny_before) = (rb(&app).model.clone(), ny(&app).model.clone());
    app.open_apply_actuals();
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    // The grid lists both projects, each with its own New Data Date edit.
    assert!(app.action_rects.contains_key("actuals_row_new_date"));
    assert!(app.action_rects.contains_key("actuals_row_new_date_1"));
    let day = text_of(&app, &app.project, "2020-01-13 08:00");
    {
        let draft = app.sched.progress.actuals.as_mut().unwrap();
        draft.new_date = day;
    }
    painter.click_action(&mut app, &ctx, &mut time, "actuals_same_date");
    assert!(!app.sched.progress.actuals.as_ref().unwrap().own_date);
    let undo = app.open_set.undo.len();
    painter.click_action(&mut app, &ctx, &mut time, "actuals_apply");
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.sched.progress.actuals.is_none());
    let target = openxer_xer::parse_relative(app.project.model.origin_serial, "2020-01-13 08:00")
        .unwrap()
        .map(|m| m + app.project.model.origin_serial * DAY);
    let (a, b) = data_dates(&app);
    assert_eq!((a, b), (target, target), "both data dates move");
    assert_ne!(a, before.0);
    assert_eq!(app.open_set.undo.len(), undo + 1, "one undo step");
    app.history_move(&ctx, false);
    assert_eq!(data_dates(&app), before);
    assert_eq!(rb(&app).model, rb_before);
    assert_eq!(ny(&app).model, ny_before);
    app.history_move(&ctx, true);
    assert_eq!(data_dates(&app), (target, target));
    drop(app);
    let _ = std::fs::remove_file(db);
}

#[test]
fn apply_actuals_with_own_dates_gives_each_project_its_date() {
    let (mut app, ctx, db, mut painter, mut time) = portfolio_in_2020("actuals-own");
    app.open_apply_actuals();
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (first, second) = (
        text_of(&app, &app.project, "2020-01-13 08:00"),
        text_of(&app, &app.project, "2020-01-20 08:00"),
    );
    {
        let draft = app.sched.progress.actuals.as_mut().unwrap();
        assert!(draft.own_date, "each project its own date is proposed");
        draft.new_date = first;
        draft.others[0] = second;
    }
    painter.click_action(&mut app, &ctx, &mut time, "actuals_apply");
    assert!(app.error.is_none(), "{:?}", app.error);
    let at = |p: &ProjectData, day: &str| {
        openxer_xer::parse_relative(p.model.origin_serial, day)
            .unwrap()
            .map(|m| m + p.model.origin_serial * DAY)
    };
    let (a, b) = data_dates(&app);
    assert_eq!(a, at(rb(&app), "2020-01-13 08:00"));
    assert_eq!(b, at(ny(&app), "2020-01-20 08:00"));
    drop(app);
    let _ = std::fs::remove_file(db);
}

#[test]
fn a_project_given_an_earlier_date_moves_back_with_the_others() {
    let (mut app, ctx, db, mut painter, mut time) = portfolio_in_2020("actuals-earlier");
    let before = data_dates(&app);
    let ny_before = ny(&app).model.activities.clone();
    app.open_apply_actuals();
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (later, earlier) = (
        text_of(&app, &app.project, "2020-01-13 08:00"),
        text_of(&app, &app.project, "2020-01-01 08:00"),
    );
    {
        let draft = app.sched.progress.actuals.as_mut().unwrap();
        draft.new_date = later;
        draft.others[0] = earlier;
    }
    let undo = app.open_set.undo.len();
    painter.click_action(&mut app, &ctx, &mut time, "actuals_apply");
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.sched.progress.actuals.is_none(), "the dialog closes");
    let at = |p: &ProjectData, day: &str| {
        openxer_xer::parse_relative(p.model.origin_serial, day)
            .unwrap()
            .map(|m| m + p.model.origin_serial * DAY)
    };
    let (a, b) = data_dates(&app);
    assert_eq!(a, at(rb(&app), "2020-01-13 08:00"));
    assert_eq!(b, at(ny(&app), "2020-01-01 08:00"), "moved back");
    assert_eq!(ny(&app).model.activities, ny_before, "no progress removed");
    assert_eq!(app.open_set.undo.len(), undo + 1, "one undo step");
    app.history_move(&ctx, false);
    assert_eq!(data_dates(&app), before);
    drop(app);
    let _ = std::fs::remove_file(db);
}

#[test]
fn a_data_date_after_today_asks_before_actuals_are_applied() {
    let (mut app, ctx, db, mut painter, mut time) = portfolio_in_2020("actuals-future");
    let before = data_dates(&app);
    app.open_apply_actuals();
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let far = text_of(&app, &app.project, "2045-01-04 08:00");
    {
        let draft = app.sched.progress.actuals.as_mut().unwrap();
        draft.own_date = false;
        draft.new_date = far;
    }
    painter.click_action(&mut app, &ctx, &mut time, "actuals_apply");
    assert!(
        app.sched.progress.confirm_future.is_some(),
        "it asks first: {:?} {}",
        app.error,
        app.status
    );
    assert_eq!(data_dates(&app), before, "nothing changed yet");
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    // No keeps the dialog and the data dates.
    painter.click_action(&mut app, &ctx, &mut time, "future_date_no");
    assert!(app.sched.progress.confirm_future.is_none());
    assert!(app.sched.progress.actuals.is_some());
    assert_eq!(data_dates(&app), before);
    // Escape also answers No.
    painter.click_action(&mut app, &ctx, &mut time, "actuals_apply");
    assert!(app.sched.progress.confirm_future.is_some());
    let _ = painter.frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
        false,
    );
    assert!(app.sched.progress.confirm_future.is_none());
    assert_eq!(data_dates(&app), before);
    // Yes applies it.
    painter.click_action(&mut app, &ctx, &mut time, "actuals_apply");
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "future_date_yes");
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.sched.progress.actuals.is_none());
    assert_ne!(data_dates(&app), before);
    drop(app);
    let _ = std::fs::remove_file(db);
}

#[test]
fn update_progress_covers_the_highlighted_activities_of_every_open_project() {
    let (mut app, ctx, db, mut painter, mut time) = portfolio_in_2020("update-spotlight");
    let (rb_before, ny_before) = (rb(&app).model.clone(), ny(&app).model.clone());
    let before = data_dates(&app);
    // The schedule runs from the 2020 data date (nothing holds RIVERBEND's
    // first activity later; NORTHYARD follows it), so its second week ends
    // on 20 January 2020.
    let start = rb_before
        .activities
        .iter()
        .filter_map(|a| a.early_start)
        .min();
    assert_eq!(
        start, rb_before.project.data_date,
        "the schedule starts on the data date"
    );
    const WINDOW_END: &str = "2020-01-20 08:00";
    // Progress Spotlight from the data date to the second week of the schedule.
    app.gantt.options.spotlight = true;
    let end = openxer_xer::parse_relative(app.shown().model.origin_serial, WINDOW_END)
        .unwrap()
        .unwrap();
    app.layout_opts.spotlight_end = Some(end);
    let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    app.refresh_composite();
    let lit = app.spotlight_activities();
    let origin = app.open_set.composite.as_ref().unwrap().origin.clone();
    let slots: BTreeSet<usize> = lit.iter().map(|&i| origin[i].0).collect();
    assert_eq!(slots.len(), 2, "both projects have highlighted activities");
    app.open_update_progress();
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    app.sched.progress.update.as_mut().unwrap().new_date = text_of(&app, &app.project, WINDOW_END);
    let undo = app.open_set.undo.len();
    painter.click_action(&mut app, &ctx, &mut time, "update_apply");
    assert!(app.error.is_none(), "{:?}", app.error);
    let moved = |after: &ProjectData, was: &Model| {
        after
            .model
            .activities
            .iter()
            .zip(&was.activities)
            .filter(|(a, b)| a.status != b.status)
            .count()
    };
    assert!(moved(rb(&app), &rb_before) > 0, "RIVERBEND progressed");
    assert!(moved(ny(&app), &ny_before) > 0, "NORTHYARD progressed");
    assert_ne!(data_dates(&app), before);
    assert_eq!(data_dates(&app).0, data_dates(&app).1, "one date for all");
    // Only the highlighted activities changed: later ones stay as they were.
    for (project, was) in [(rb(&app), &rb_before), (ny(&app), &ny_before)] {
        // A day past the end of the highlighted window.
        let later = openxer_xer::parse_relative(was.origin_serial, WINDOW_END)
            .unwrap()
            .unwrap()
            + DAY;
        let late = project
            .model
            .activities
            .iter()
            .zip(&was.activities)
            .filter(|(_, b)| b.early_start.is_some_and(|s| s >= later))
            .count();
        assert!(late > 0);
        assert!(project
            .model
            .activities
            .iter()
            .zip(&was.activities)
            .filter(|(_, b)| b.early_start.is_some_and(|s| s >= later))
            .all(|(a, b)| a.status == b.status));
    }
    assert_eq!(app.open_set.undo.len(), undo + 1, "one undo step");
    app.history_move(&ctx, false);
    assert_eq!(data_dates(&app), before);
    assert_eq!(rb(&app).model, rb_before);
    assert_eq!(ny(&app).model, ny_before);
    drop(app);
    let _ = std::fs::remove_file(db);
}

#[test]
fn render_apply_actuals_with_two_open_projects() {
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let (mut app, ctx, db, mut painter, mut time) = portfolio_in_2020("render-actuals");
    app.open_apply_actuals();
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image
        .save(artifacts.join("m3-apply-actuals-two-projects.png"))
        .unwrap();
    // The future date question over it.
    let far = text_of(&app, &app.project, "2045-01-04 08:00");
    app.sched.progress.actuals.as_mut().unwrap().new_date = far;
    app.sched.progress.actuals.as_mut().unwrap().own_date = false;
    painter.click_action(&mut app, &ctx, &mut time, "actuals_apply");
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image
        .save(artifacts.join("m3-apply-actuals-future.png"))
        .unwrap();
    drop(app);
    let _ = std::fs::remove_file(db);
}

/// The schedule log names the Default Project, not the active one.
#[test]
fn schedule_log_names_the_default_project() {
    let (mut app, _ctx, db, rb_id, ny_id) = portfolio_app("log_default");
    app.set_default_project(ny_id);
    assert_eq!(app.open_set.active, Some(rb_id));
    let active = app.project.model.project.name.clone();
    let default = ny(&app).model.project.name.clone();
    assert_ne!(active, default);
    app.schedule();
    let line = |label: &str| {
        app.log
            .iter()
            .find(|l| l.starts_with(&format!("\t{label}.")))
            .cloned()
            .unwrap_or_default()
    };
    assert!(line("Default Project").ends_with(&default), "{:?}", app.log);
    assert!(line("Use scheduling options from").ends_with(&default));
    drop(app);
    let _ = std::fs::remove_file(db);
}

fn ny_mut(app: &mut App) -> &mut ProjectData {
    &mut app
        .open_set
        .parked
        .iter_mut()
        .find(|p| p.project.model.project.proj_id == 2)
        .unwrap()
        .project
}

/// Tools > Save Period Actuals stores into every open project: the count
/// says so and each project's own financial period gets the rows.
#[test]
fn save_period_actuals_covers_every_open_project() {
    use crate::enterprise::ops;
    let (mut app, _ctx, db, _, _) = portfolio_app("period_all");
    let day = |m, d| chrono::NaiveDate::from_ymd_opt(2026, m, d).unwrap();
    let mut calendars = vec![];
    for which in 0..2 {
        let p = if which == 0 {
            &mut app.project
        } else {
            ny_mut(&mut app)
        };
        let doc = &mut p.document;
        let cal = ops::add_fin_calendar(doc).unwrap();
        ops::batch_create(
            doc,
            cal,
            day(11, 1),
            day(11, 30),
            ops::Interval::Monthly(30),
        )
        .unwrap();
        let pid = p.model.project.proj_id.to_string();
        let t = doc.table_mut("PROJECT");
        let row = (0..t.rows.len())
            .find(|&i| t.value(&t.rows[i], "proj_id") == pid)
            .unwrap();
        t.set(row, "fintmpl_id", cal.to_string());
        calendars.push(cal);
    }
    let period = ops::periods(&app.project.document, calendars[0])[0].id;
    let ny_period = ops::periods(&ny(&app).document, calendars[1])[0].id;
    app.store_period_now(calendars[0], period, 1.);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.status.contains("in 2 projects"), "{}", app.status);
    let stored = |p: &ProjectData, id: i64| {
        let t = p.document.table("PROJECT").unwrap();
        let pid = p.model.project.proj_id.to_string();
        t.rows
            .iter()
            .find(|r| t.value(r, "proj_id") == pid)
            .map(|r| t.value(r, "last_fin_dates_id") == id.to_string())
            .unwrap()
    };
    assert!(stored(rb(&app), period));
    assert!(stored(ny(&app), ny_period), "the other open project too");
    drop(app);
    let _ = std::fs::remove_file(db);
}
