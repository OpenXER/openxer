//! Baselines through the production App: add, delete, copy, restore,
//! assign, columns and variances, Gantt bars, Check Schedule, undo, file
//! import and export, and software renders of the dialogs and a Gantt with
//! baseline bars (PNG files in the gitignored artifacts folder).
use super::store::{self, Choice, UserBaselines};
use super::*;
use crate::grid::columns;
use crate::headless_render::SoftwarePainter;
use crate::schedule_check as check;
use openxer_engine::calendar::DAY;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("openxer-bl-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn scheduled_starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn app(dir: &Path) -> (App, egui::Context) {
    let store = Store::open(dir.join("workspace.db")).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, scheduled_starter());
    app.save();
    (app, ctx)
}

fn rows(app: &App) -> Vec<store::BaselineRow> {
    store::list(app.store.connection(), app.workspace_row().unwrap()).unwrap()
}

fn column_text(app: &App, id: &str, i: usize) -> String {
    let ix = app.grid_index();
    let catalog = columns::catalog(&app.project);
    let column = columns::find(&catalog, id).unwrap_or_else(|| panic!("column {id}"));
    let value = columns::value(&app.project, &ix, column, i);
    columns::display(&app.project, Some(i), &value)
}

fn value(app: &App, id: &str, i: usize) -> columns::Value {
    let ix = app.grid_index();
    let catalog = columns::catalog(&app.project);
    columns::value(&app.project, &ix, columns::find(&catalog, id).unwrap(), i)
}

/// Delay one not-started activity by two days without rescheduling.
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
fn add_delete_copy_and_undo_through_the_history() {
    let dir = temp("ops");
    let (mut app, ctx) = app(&dir);
    assert!(!app.has_baselines());
    let id = app.add_baseline_copy(1.).unwrap();
    let list = rows(&app);
    assert_eq!(list.len(), 1);
    let pid = app.project.model.project.proj_id;
    let b = &list[0];
    assert_eq!(
        b.short_name,
        format!("{} - B1", app.project.model.project.name)
    );
    assert_ne!(b.project_key, pid);
    assert!(!b.added.is_empty() && b.added == b.updated);
    assert!(app.has_baselines());
    // A second one gets the next number and key.
    let second = app.add_baseline_copy(2.).unwrap();
    let list = rows(&app);
    assert_eq!(
        list[1].short_name,
        format!("{} - B2", app.project.model.project.name)
    );
    assert!(list[1].project_key > list[0].project_key);
    // The stored copy is a baseline of the project and loads on its own.
    let full = store::get(app.store.connection(), id).unwrap().unwrap();
    let doc = Document::parse(&full.xer).unwrap();
    assert_eq!(
        xb::project_value(&doc, b.project_key, "orig_proj_id"),
        pid.to_string()
    );
    let loaded = ProjectData::from_document(doc, b.project_key, String::new()).unwrap();
    assert_eq!(
        loaded.model.activities.len(),
        app.project.model.activities.len()
    );
    // Baselines never show in the project lists.
    assert_eq!(app.store.list().unwrap().len(), 1);
    assert_eq!(
        crate::interchange::workspace::rows(&app.store)
            .unwrap()
            .len(),
        1
    );

    app.delete_baseline(second, 3.).unwrap();
    assert_eq!(rows(&app).len(), 1);
    app.history_move(&ctx, false);
    assert_eq!(rows(&app).len(), 2, "undo puts the deleted baseline back");
    assert_eq!(rows(&app)[1].id, second, "with its id");
    app.history_move(&ctx, true);
    assert_eq!(rows(&app).len(), 1, "redo deletes it again");
    app.history_move(&ctx, false);
    app.history_move(&ctx, false);
    assert_eq!(rows(&app).len(), 1, "undo of the second add");
    app.history_move(&ctx, false);
    assert!(rows(&app).is_empty(), "undo of the first add");
    app.history_move(&ctx, true);
    assert_eq!(rows(&app).len(), 1);

    // Copy: another baseline of the project with the next free number and
    // the source's last update date; undo removes it.
    let copied = app.copy_baseline(id, 4.).unwrap();
    let list = rows(&app);
    assert_eq!(list.len(), 2);
    let copy = list.iter().find(|r| r.id == copied).unwrap();
    assert_eq!(
        copy.short_name,
        format!("{} - B2", app.project.model.project.name)
    );
    assert_eq!(copy.updated, list[0].updated);
    assert_eq!(copy.data_date, list[0].data_date);
    assert_ne!(copy.project_key, list[0].project_key);
    assert_eq!(app.store.list().unwrap().len(), 1, "a copy is no project");
    let full = store::get(app.store.connection(), copied).unwrap().unwrap();
    let doc = Document::parse(&full.xer).unwrap();
    assert_eq!(
        xb::project_value(&doc, copy.project_key, "orig_proj_id"),
        pid.to_string()
    );
    app.history_move(&ctx, false);
    assert_eq!(rows(&app).len(), 1);

    // Rename and type change, undone.
    app.rename_baseline(id, "Approved plan", "", 5.).unwrap();
    assert_eq!(rows(&app)[0].name, "Approved plan");
    app.history_move(&ctx, false);
    assert_ne!(rows(&app)[0].name, "Approved plan");
    assert!(app.error.is_none(), "{:?}", app.error);
}

#[test]
fn assign_drives_columns_variances_bars_and_check_schedule() {
    let dir = temp("assign");
    let (mut app, ctx) = app(&dir);
    let i = app
        .project
        .model
        .activities
        .iter()
        .position(|a| {
            a.status == openxer_engine::model::StatusType::NotStarted
                && a.task_type == openxer_engine::model::TaskType::Task
        })
        .unwrap();
    let planned = data::current(&app.project, &app.project.model.activities[i]);
    let id = app.add_baseline_copy(1.).unwrap();
    // Unassigned: the project is its own baseline (its planned dates).
    assert_eq!(app.project_baseline_name(), "Current Project");
    assert_eq!(
        value(&app, "bl_project_start", i),
        columns::Value::Date {
            minute: planned.start.unwrap(),
            mark: crate::format::DateMark::Plain
        }
    );
    assert_eq!(value(&app, "bl2_start", i), columns::Value::Empty);

    let bars_before = app.gantt.bars.iter().filter(|b| b.display).count();
    let users = UserBaselines([Choice::Baseline(id), Choice::Current, Choice::None]);
    app.assign_baselines(Some(Choice::Baseline(id)), users, 2.)
        .unwrap();
    let name = rows(&app)[0].name.clone();
    assert_eq!(app.project_baseline_name(), name);
    assert_eq!(app.baseline_choices(), (Choice::Baseline(id), users));
    let key = rows(&app)[0].project_key;
    assert_eq!(
        xb::project_value(
            &app.project.document,
            app.project.model.project.proj_id,
            "sum_base_proj_id"
        ),
        key.to_string()
    );
    // The project and primary baseline bars (and the baseline milestone) turn on.
    let shown: Vec<&str> = app
        .gantt
        .bars
        .iter()
        .filter(|b| b.display)
        .map(|b| b.name.as_str())
        .collect();
    assert!(shown.contains(&"Project Baseline Bar") && shown.contains(&"Primary Baseline Bar"));
    assert!(shown.contains(&"Baseline Milestone"));
    assert!(!shown.contains(&"Secondary Baseline Bar"));
    assert!(shown.len() > bars_before);

    // Move the activity two days later: the baseline keeps its dates.
    delay(&mut app, i);
    assert_eq!(
        value(&app, "bl_project_start", i),
        columns::Value::Date {
            minute: planned.start.unwrap(),
            mark: crate::format::DateMark::Plain
        }
    );
    assert_eq!(
        value(&app, "bl1_finish", i),
        value(&app, "bl_project_finish", i)
    );
    let a = &app.project.model.activities[i];
    let cal = &app.project.model.calendars[a.cal];
    let lost = cal.between(planned.start.unwrap(), a.early_start.unwrap());
    assert!(lost > 0);
    assert_eq!(
        value(&app, "variance_bl_project_start", i),
        columns::Value::Duration(-lost)
    );
    assert!(column_text(&app, "variance_bl_project_finish", i).starts_with('-'));
    assert_eq!(
        value(&app, "variance_bl1_start", i),
        columns::Value::Duration(-lost)
    );
    // Durations: the baseline keeps the original duration.
    let original = planned.duration;
    assert_eq!(
        value(&app, "bl_project_duration", i),
        columns::Value::Duration(original)
    );
    app.project.model.activities[i].rem += 8 * 60;
    assert_eq!(
        value(&app, "variance_bl_project_duration", i),
        columns::Value::Duration(-8 * 60)
    );
    assert!(!column_text(&app, "bl_project_start", i).is_empty());

    // Gantt spans come from the baseline.
    let spans = app
        .baseline_compare()
        .spans(&app.project, &app.project.model.activities[i]);
    assert_eq!(
        spans[0],
        Some((planned.start.unwrap(), planned.finish.unwrap()))
    );
    assert_eq!(
        spans[2],
        Some((planned.start.unwrap(), planned.finish.unwrap())),
        "secondary is the project itself"
    );
    assert_eq!(spans[3], None);

    // Check Schedule: the delayed activity finishes after its baseline.
    let finishes = app.baseline_compare().project_finishes(&app.project);
    let late = check::run_check_with(
        &app.project.model,
        &check::CheckParams::default(),
        check::Check::LateActivities,
        &Default::default(),
        Some(&finishes),
    );
    assert!(late.note.is_none());
    assert!(late.items.contains(&check::Item::Activity(i)));
    let bei = check::run_check_with(
        &app.project.model,
        &check::CheckParams::default(),
        check::Check::Bei,
        &Default::default(),
        Some(&finishes),
    );
    assert!(bei.note.is_none());

    // Undo the edits above (recorded as one step on the way), then the
    // assignment: back to the project itself, field restored.
    app.history_move(&ctx, false);
    assert_eq!(app.baseline_choices().0, Choice::Baseline(id));
    app.history_move(&ctx, false);
    assert_eq!(
        app.baseline_choices(),
        (Choice::Current, UserBaselines::default())
    );
    assert!(xb::project_value(
        &app.project.document,
        app.project.model.project.proj_id,
        "sum_base_proj_id"
    )
    .is_empty());
    app.history_move(&ctx, true);
    assert_eq!(app.baseline_choices().0, Choice::Baseline(id));
    // An assigned baseline is neither deleted nor restored.
    let refused = app.delete_baseline(id, 3.).unwrap_err();
    assert!(matches!(refused, ops::Error::Refused(_)), "{refused:?}");
    let refused = app.restore_baseline(id, 3.).unwrap_err();
    assert!(matches!(refused, ops::Error::Refused(_)), "{refused:?}");
    assert_eq!(rows(&app).len(), 1);
    assert_eq!(app.baseline_choices().0, Choice::Baseline(id));
    assert!(app.error.is_none(), "{:?}", app.error);
}

#[test]
fn assignment_survives_save_and_reopen() {
    let dir = temp("reopen");
    let id;
    {
        let (mut app, _ctx) = app(&dir);
        id = app.add_baseline_copy(1.).unwrap();
        let users = UserBaselines([Choice::Current, Choice::Baseline(id), Choice::None]);
        app.assign_baselines(Some(Choice::Baseline(id)), users, 2.)
            .unwrap();
        app.save();
    }
    let store = Store::open(dir.join("workspace.db")).unwrap();
    let project = store.last().unwrap().unwrap();
    let ctx = egui::Context::default();
    let app = App::new(&ctx, store, project);
    assert_eq!(
        app.baseline_choices(),
        (
            Choice::Baseline(id),
            UserBaselines([Choice::Current, Choice::Baseline(id), Choice::None])
        )
    );
    assert_eq!(app.project_baseline_name(), rows(&app)[0].name);
}

#[test]
fn update_restore_and_convert() {
    let dir = temp("update");
    let (mut app, ctx) = app(&dir);
    let id = app.add_baseline_copy(1.).unwrap();
    let pid = app.project.model.project.proj_id;
    let count = app.project.model.activities.len();
    // A new activity in the project; Update adds it to the baseline.
    let wbs = app.project.wbs[0].id;
    let added = app.project.add_activity(wbs);
    let code = app.project.model.activities[added].task_code.clone();
    let codes = |app: &App| {
        let row = store::get(app.store.connection(), id).unwrap().unwrap();
        let doc = Document::parse(&row.xer).unwrap();
        xb::activity_codes(&doc, row.project_key)
    };
    assert!(!codes(&app).values().any(|c| *c == code));
    let mut all = update_dialog::all_update_options();
    all.add_new = false;
    app.update_baseline(id, &all, 2.).unwrap();
    assert!(!codes(&app).values().any(|c| *c == code), "add new was off");
    app.update_baseline(id, &update_dialog::all_update_options(), 3.)
        .unwrap();
    assert!(codes(&app).values().any(|c| *c == code));
    assert_eq!(codes(&app).len(), count + 1);
    app.history_move(&ctx, false);
    assert!(!codes(&app).values().any(|c| *c == code), "undo of update");

    // Revert Project to Baseline: the baseline (without the new activity)
    // becomes the project's data; the baseline stays.
    app.revert_to_baseline(id, 4.).unwrap();
    assert_eq!(
        app.project.model.project.proj_id, pid,
        "the project keeps its id"
    );
    assert_eq!(app.project.model.activities.len(), count);
    assert_eq!(rows(&app).len(), 1, "the baseline stays in the list");
    assert!(app
        .project
        .model
        .activities
        .iter()
        .all(|a| a.task_code != code));
    app.history_move(&ctx, false);
    assert_eq!(app.project.model.activities.len(), count + 1);
    assert_eq!(rows(&app).len(), 1);

    // Convert another workspace project into a baseline of this one.
    let mut other = scheduled_starter();
    other.source_name = "other.xer".into();
    let other_id = app.store.save(&other).unwrap();
    // `save` marks the other project as last opened; that does not matter here.
    assert!(app
        .convertible_projects()
        .iter()
        .any(|(p, _, _)| *p == other_id));
    let bl = app.convert_to_baseline(other_id, 5.).unwrap();
    assert!(app
        .store
        .list()
        .unwrap()
        .iter()
        .all(|(p, _)| *p != other_id));
    assert_eq!(rows(&app).len(), 2);
    let row = store::get(app.store.connection(), bl).unwrap().unwrap();
    let doc = Document::parse(&row.xer).unwrap();
    assert_eq!(
        xb::project_value(&doc, row.project_key, "orig_proj_id"),
        pid.to_string()
    );
    app.history_move(&ctx, false);
    assert!(app
        .store
        .list()
        .unwrap()
        .iter()
        .any(|(p, _)| *p == other_id));
    assert_eq!(rows(&app).len(), 1);
    assert!(app.error.is_none(), "{:?}", app.error);
}

#[test]
fn renumber_carries_into_baselines_and_undoes_with_the_project() {
    let dir = temp("renumber");
    let (mut app, ctx) = app(&dir);
    let id = app.add_baseline_copy(1.).unwrap();
    let i = 0;
    let old = app.project.model.activities[i].task_code.clone();
    app.editing.options.renumber.baselines = true;
    app.apply_renumber(&[(i, "NEW-1".into())], 2.);
    let codes = |app: &App| {
        let row = store::get(app.store.connection(), id).unwrap().unwrap();
        let doc = Document::parse(&row.xer).unwrap();
        xb::activity_codes(&doc, row.project_key)
    };
    assert!(codes(&app).values().any(|c| c == "NEW-1"));
    assert!(app.status.contains("baseline"), "{}", app.status);
    // Matched by the new ID, the baseline still lines up.
    assert_ne!(value(&app, "bl1_start", i), columns::Value::Empty);
    app.history_move(&ctx, false);
    assert_eq!(app.project.model.activities[i].task_code, old);
    assert!(
        codes(&app).values().any(|c| *c == old),
        "one undo reverts both"
    );
    app.history_move(&ctx, true);
    assert!(codes(&app).values().any(|c| c == "NEW-1"));
}

#[test]
fn file_baselines_are_stored_assigned_and_exported() {
    let dir = temp("file");
    let (mut app, _ctx) = app(&dir);
    // A file with the project and a baseline of it, assigned as its project
    // baseline.
    let p = app.project.clone();
    let pid = p.model.project.proj_id;
    let mut doc = p.updated_document();
    let copy = xb::copy_as_baseline(
        &doc,
        pid,
        900,
        "RB - BL",
        "Riverbend baseline",
        "2026-10-01 08:00",
    );
    xb::append_project(&mut doc, &copy, 900);
    xb::set_project_value(&mut doc, pid, "sum_base_proj_id", "900");
    assert_eq!(
        xb::schedule_projects(&doc).len(),
        1,
        "the baseline is not a pick"
    );
    let file = dir.join("with-baseline.xer");
    doc.write(&file).unwrap();
    // File > Open.
    let doc = Document::read(&file).unwrap();
    let mut opened = ProjectData::from_document(doc, pid, file.display().to_string()).unwrap();
    opened.source_name = file.display().to_string();
    app.dirty = false;
    app.load(opened);
    app.adopt_file_baselines();
    let list = rows(&app);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].project_key, 900);
    assert_eq!(list[0].short_name, "RB - BL");
    assert_eq!(list[0].added, "2026-10-01 08:00");
    assert_eq!(app.baseline_choices().0, Choice::Baseline(list[0].id));
    // Opening again does not duplicate it.
    app.adopt_file_baselines();
    assert_eq!(rows(&app).len(), 1);

    // Export with baselines: the baseline travels, still assigned.
    let parent = app.workspace_row().unwrap();
    let mut out =
        openxer_interchange::xer_export::export_document(&[&app.project], "25.12").unwrap();
    assert!(
        xb::baselines_in(&out).is_empty(),
        "without the option no baseline"
    );
    assert_eq!(
        export_baselines(app.store.connection(), &mut out, parent, pid).unwrap(),
        1
    );
    let text = out.text();
    let back = Document::parse(&text).unwrap();
    let found = xb::baselines_in(&back);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].1, pid);
    assert_eq!(
        xb::project_value(&back, pid, "sum_base_proj_id"),
        found[0].0.to_string()
    );
    assert_eq!(
        xb::activity_codes(&back, found[0].0).len(),
        app.project.model.activities.len()
    );
    // A second workspace reads it back as a baseline of the project.
    let other = Store::open(dir.join("second.db")).unwrap();
    let mut q = ProjectData::from_document(back.clone(), pid, "second".into()).unwrap();
    q.source_name = "second".into();
    let mut other = other;
    let row = other.save(&q).unwrap();
    assert_eq!(
        attach_file_baselines(other.connection(), row, &back, pid, None).unwrap(),
        1
    );
    assert_eq!(store::list(other.connection(), row).unwrap().len(), 1);
}

#[test]
fn render_baseline_dialogs_and_gantt() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/baselines");
    std::fs::create_dir_all(&out).unwrap();
    let dir = temp("render");
    let (mut app, ctx) = app(&dir);
    let id = app.add_baseline_copy(1.).unwrap();
    app.add_baseline_copy(2.).unwrap();
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    let [r, g, b] = crate::theme::bytes(crate::theme::BASELINE);
    let yellow =
        |image: &image::RgbaImage| image.pixels().filter(|p| p.0 == [r, g, b, 255]).count();
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (plain, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    // Assign, then move a few activities later so the bars separate.
    let users = UserBaselines([Choice::Baseline(id), Choice::None, Choice::None]);
    app.assign_baselines(Some(Choice::Baseline(id)), users, 3.)
        .unwrap();
    for i in 2..6 {
        delay(&mut app, i);
    }
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (gantt, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    gantt
        .save(out.join("gantt-with-baseline-bars.png"))
        .unwrap();
    assert!(
        yellow(&gantt) > yellow(&plain) + 50,
        "baseline bars are drawn: {} baseline-grey pixels before, {} after",
        yellow(&plain),
        yellow(&gantt)
    );
    let crop = |image: &image::RgbaImage, app: &App, salt: &str, client: Vec2| {
        let close = app.action_rects[&format!("{salt}_title_close")];
        let size = client + Vec2::new(2., 32.);
        let left = (close[2] + 1. - size.x).round() as u32;
        let top = (close[1] - 1.).round() as u32;
        image::imageops::crop_imm(image, left, top, size.x as u32, size.y as u32).to_image()
    };
    app.open_maintain_baselines();
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "baselines_row_0");
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    crop(&image, &app, "baselines_maintain", Vec2::new(900., 331.))
        .save(out.join("maintain-baselines.png"))
        .unwrap();
    assert_eq!(app.baselines.maintain.as_ref().unwrap().selected, Some(id));
    painter.click_action(&mut app, &ctx, &mut time, "baselines_add");
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    crop(&image, &app, "baselines_add", Vec2::new(500., 110.))
        .save(out.join("add-new-baseline.png"))
        .unwrap();
    painter.click_action(&mut app, &ctx, &mut time, "add_ok");
    assert_eq!(rows(&app).len(), 3, "Add > OK saves a copy");
    painter.click_action(&mut app, &ctx, &mut time, "baselines_close");
    assert!(app.baselines.maintain.is_none());
    app.open_assign_baselines();
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    crop(&image, &app, "baselines_assign", Vec2::new(500., 323.))
        .save(out.join("assign-baselines.png"))
        .unwrap();
    painter.click_action(&mut app, &ctx, &mut time, "assign_lookup_2_button");
    painter.click_action(&mut app, &ctx, &mut time, "lookup_row_0");
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    crop(&image, &app, "baselines_lookup", Vec2::new(400., 270.))
        .save(out.join("assign-baselines-lookup.png"))
        .unwrap();
    // Secondary: the first baseline from the lookup.
    painter.click_action(&mut app, &ctx, &mut time, "lookup_select");
    painter.click_action(&mut app, &ctx, &mut time, "assign_ok");
    assert!(app.baselines.assign.is_none());
    assert_eq!(app.baseline_choices().1 .0[1], Choice::Baseline(id));
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image
        .save(out.join("main-window-with-baselines.png"))
        .unwrap();
    assert!(app.error.is_none(), "{:?}", app.error);
}

/// Maintain Baselines and Select Project scroll: every baseline and every
/// project can be reached and picked, however many there are.
#[test]
fn baseline_and_project_lists_scroll_to_every_entry() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/bugfix-help-audit");
    std::fs::create_dir_all(&out).unwrap();
    let dir = temp("scroll");
    let (mut app, ctx) = app(&dir);
    for i in 0..14 {
        app.add_baseline_copy(1. + i as f64).unwrap();
    }
    for i in 0..24 {
        let mut q = scheduled_starter();
        q.source_name = format!("other-{i:02}");
        app.store.save(&q).unwrap();
    }
    let projects = app.convertible_projects();
    assert!(projects.len() >= 20, "{}", projects.len());
    let all = rows(&app);
    assert_eq!(all.len(), 14);
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    app.open_maintain_baselines();
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let last = all.len() - 1;
    assert!(!app
        .action_rects
        .contains_key(&format!("baselines_row_{last}")));
    for _ in 0..last {
        if app
            .action_rects
            .contains_key(&format!("baselines_row_{last}"))
        {
            break;
        }
        painter.click_action(&mut app, &ctx, &mut time, "baselines_list_scroll_down");
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image
        .save(out.join("maintain-baselines-scrolled.png"))
        .unwrap();
    painter.click_action(&mut app, &ctx, &mut time, &format!("baselines_row_{last}"));
    assert_eq!(
        app.baselines.maintain.as_ref().unwrap().selected,
        Some(all[last].id),
        "the last baseline is reachable"
    );
    app.baselines.select = Some(super::dialogs::SelectProject::default());
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let last = projects.len() - 1;
    assert!(!app
        .action_rects
        .contains_key(&format!("select_project_{last}")));
    for _ in 0..last {
        if app
            .action_rects
            .contains_key(&format!("select_project_{last}"))
        {
            break;
        }
        painter.click_action(&mut app, &ctx, &mut time, "select_project_list_scroll_down");
    }
    painter.click_action(&mut app, &ctx, &mut time, &format!("select_project_{last}"));
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("select-project-scrolled.png")).unwrap();
    assert_eq!(
        app.baselines.select.as_ref().unwrap().selected,
        Some(projects[last].0),
        "the last project is reachable"
    );
}

/// Baseline columns and names follow every change to the baseline, even
/// when two changes land in the same minute or keep the update stamp
/// (a rename).
#[test]
fn baseline_columns_follow_a_same_minute_update_and_a_rename() {
    let dir = temp("stale");
    let (mut app, _ctx) = app(&dir);
    let i = app
        .project
        .model
        .activities
        .iter()
        .position(|a| {
            a.status == openxer_engine::model::StatusType::NotStarted
                && a.task_type == openxer_engine::model::TaskType::Task
        })
        .unwrap();
    let id = app.add_baseline_copy(1.).unwrap();
    let users = UserBaselines([Choice::Baseline(id), Choice::None, Choice::None]);
    app.assign_baselines(Some(Choice::Baseline(id)), users, 2.)
        .unwrap();
    assert_ne!(value(&app, "bl_project_start", i), columns::Value::Empty);
    let first = rows(&app)[0].name.clone();
    assert_eq!(app.project_baseline_name(), first);
    // A new activity, then Update within the same minute: the baseline
    // columns show the activity's baseline dates at once.
    let wbs = app.project.wbs[0].id;
    let added = app.project.add_activity(wbs);
    assert_eq!(
        value(&app, "bl_project_start", added),
        columns::Value::Empty
    );
    app.update_baseline(id, &update_dialog::all_update_options(), 3.)
        .unwrap();
    assert_ne!(
        value(&app, "bl_project_start", added),
        columns::Value::Empty,
        "the updated baseline is shown"
    );
    // A rename keeps the update stamp; the new name still shows.
    app.rename_baseline(id, "Renamed baseline", "", 4.).unwrap();
    assert_eq!(app.project_baseline_name(), "Renamed baseline");
}

/// On a workspace with a shared dictionary a project's stored row leaves out
/// the workspace calendars, resources and codes; a project converted to a
/// baseline keeps them, since its source row is deleted.
#[test]
fn convert_on_a_shared_dictionary_workspace_keeps_the_calendars() {
    let dir = temp("convert-shared");
    let store = Store::open(dir.join("workspace.db")).unwrap();
    openxer_automation::workspace_db::ensure(store.connection()).unwrap();
    store
        .set_preference(
            openxer_automation::workspace_db::SCHEMA_KEY,
            &openxer_automation::workspace_db::DICTIONARY_SCHEMA.to_string(),
        )
        .unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, scheduled_starter());
    app.save();
    let mut other = scheduled_starter();
    other.source_name = "other.xer".into();
    let other_id = app.store.save(&other).unwrap();
    let calendars = |doc: &Document| doc.table("CALENDAR").map_or(0, |t| t.rows.len());
    let raw = Document::parse(&app.store.source(other_id).unwrap().2).unwrap();
    let expected = calendars(&other.document);
    assert!(expected > 0);
    assert!(
        calendars(&raw) < expected,
        "the stored row leaves the calendars to the dictionary"
    );
    let bl = app.convert_to_baseline(other_id, 5.).unwrap();
    let row = store::get(app.store.connection(), bl).unwrap().unwrap();
    let doc = Document::parse(&row.xer).unwrap();
    // Every activity's calendar is in the baseline.
    let ids: Vec<String> = doc.table("CALENDAR").map_or(vec![], |t| {
        t.rows
            .iter()
            .map(|r| t.value(r, "clndr_id").to_string())
            .collect()
    });
    let tasks = doc.table("TASK").unwrap();
    assert!(!tasks.rows.is_empty());
    for r in &tasks.rows {
        let cal = tasks.value(r, "clndr_id");
        assert!(ids.iter().any(|id| id == cal), "calendar {cal} in {ids:?}");
    }
}

/// Escape closes Maintain Baselines without saving a half-typed name.
#[test]
fn escape_in_maintain_baselines_keeps_the_stored_name() {
    let dir = temp("escape-name");
    let (mut app, ctx) = app(&dir);
    app.add_baseline_copy(1.).unwrap();
    let name = rows(&app)[0].name.clone();
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    app.open_maintain_baselines();
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "baselines_row_0");
    painter.click_action(&mut app, &ctx, &mut time, "baselines_name");
    let _ = painter.frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::Text(" half typed".into())],
        false,
    );
    assert!(app
        .baselines
        .maintain
        .as_ref()
        .unwrap()
        .name
        .ends_with("half typed"));
    for pressed in [true, false] {
        let _ = painter.frame(
            &mut app,
            &ctx,
            &mut time,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Default::default(),
            }],
            false,
        );
    }
    assert!(app.baselines.maintain.is_none());
    assert_eq!(rows(&app)[0].name, name);
}

/// With two projects open, Maintain Baselines shows a band for each with
/// its own baselines, and Assign Baselines offers both; a command on the
/// other project's baseline makes that project current first.
#[test]
fn each_open_project_gets_its_own_band() {
    let dir = temp("bands");
    let (mut app, _ctx) = app(&dir);
    let first = app.workspace_row().unwrap();
    app.add_baseline_copy(1.).unwrap();
    let mut other = scheduled_starter();
    other.source_name = "other.xer".into();
    other.model.project.name = "ZZ-OTHER".into();
    let second = app.store.save(&other).unwrap();
    app.open_project_set(&[first, second]);
    assert_eq!(app.open_ids().len(), 2);
    let bands = app.baseline_bands();
    assert_eq!(bands.len(), 2);
    assert_eq!(bands[0].id, Some(first), "bands in Project ID order");
    assert_eq!(bands[0].rows.len(), 1);
    assert_eq!(bands[1].id, Some(second));
    assert!(bands[1].rows.is_empty());
    // A copy of the second project's own baseline: made current first.
    app.activate_owner(second);
    assert_eq!(app.workspace_row(), Some(second));
    let id = app.add_baseline_copy(2.).unwrap();
    let row = store::get(app.store.connection(), id).unwrap().unwrap();
    assert_eq!(row.parent, second);
    assert_eq!(row.short_name, "ZZ-OTHER - B1");
    let copy = app.copy_baseline(id, 3.).unwrap();
    assert_eq!(
        store::get(app.store.connection(), copy)
            .unwrap()
            .unwrap()
            .short_name,
        "ZZ-OTHER - B2"
    );
    // Convert refuses either open project.
    for p in [first, second] {
        let e = app.convert_to_baseline(p, 4.).unwrap_err();
        assert_eq!(e, ops::Error::Refused(ops::OPEN_REFUSED.into()));
    }
    assert!(app.error.is_none(), "{:?}", app.error);
}

/// Every role counts as assigned (tertiary too); a converted baseline's
/// blank Last Update Date goes out in the XER and comes back blank.
#[test]
fn tertiary_counts_as_assigned_and_a_blank_update_date_round_trips() {
    let dir = temp("tertiary");
    let (mut app, _ctx) = app(&dir);
    let b1 = app.add_baseline_copy(1.).unwrap();
    let users = UserBaselines([Choice::Current, Choice::None, Choice::Baseline(b1)]);
    app.assign_baselines(None, users, 2.).unwrap();
    for e in [
        app.delete_baseline(b1, 3.).unwrap_err(),
        app.restore_baseline(b1, 3.).unwrap_err(),
    ] {
        assert!(matches!(e, ops::Error::Refused(_)), "{e:?}");
    }
    // A converted project: blank Last Update Date.
    let mut other = scheduled_starter();
    other.source_name = "other.xer".into();
    other.model.project.name = "REV-2".into();
    let other_id = app.store.save(&other).unwrap();
    let bl = app.convert_to_baseline(other_id, 4.).unwrap();
    let row = store::get(app.store.connection(), bl).unwrap().unwrap();
    assert_eq!(row.short_name, "REV-2", "keeps its own ID");
    assert_eq!(row.updated, "");
    // Export with baselines and read into another workspace.
    let parent = app.workspace_row().unwrap();
    let pid = app.project.model.project.proj_id;
    let mut out =
        openxer_interchange::xer_export::export_document(&[&app.project], "25.12").unwrap();
    assert_eq!(
        export_baselines(app.store.connection(), &mut out, parent, pid).unwrap(),
        2
    );
    let back = Document::parse(&out.text()).unwrap();
    let mut second = Store::open(dir.join("second.db")).unwrap();
    let mut q = ProjectData::from_document(back.clone(), pid, "second".into()).unwrap();
    q.source_name = "second".into();
    let row_id = second.save(&q).unwrap();
    attach_file_baselines(second.connection(), row_id, &back, pid, None).unwrap();
    let listed = store::list(second.connection(), row_id).unwrap();
    let converted = listed.iter().find(|r| r.short_name == "REV-2").unwrap();
    assert_eq!(converted.updated, "", "blank after the round trip");
    let copy = listed.iter().find(|r| r.short_name != "REV-2").unwrap();
    assert!(!copy.updated.is_empty());
}

/// Revert Project to Baseline, drawn: the dialog with a baseline selected.
#[test]
fn render_revert_project_to_baseline() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/baselines");
    std::fs::create_dir_all(&out).unwrap();
    let dir = temp("render-revert");
    let (mut app, ctx) = app(&dir);
    app.add_baseline_copy(1.).unwrap();
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    app.open_revert_baseline();
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "revert_row_0");
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image
        .save(out.join("revert-project-to-baseline.png"))
        .unwrap();
    assert!(app.baselines.revert.as_ref().unwrap().selected.is_some());
    // Escape closes it and changes nothing.
    let before = app.project.model.activities.len();
    for pressed in [true, false] {
        let _ = painter.frame(
            &mut app,
            &ctx,
            &mut time,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Default::default(),
            }],
            false,
        );
    }
    assert!(app.baselines.revert.is_none());
    assert_eq!(app.project.model.activities.len(), before);
}
