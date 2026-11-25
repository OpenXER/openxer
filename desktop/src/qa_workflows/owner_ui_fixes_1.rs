//! Owner fixes, round one.
//!
//! - The data date may be any date: Update Progress and Apply Actuals take
//!   a date equal to or earlier than the current one (the data date moves,
//!   no progress is added or removed, the notice shows as usual), and so do
//!   the Dates tab, the Schedule dialog and the command line.
//! - Renders of the dialogs and the Chat window this round changed, on the
//!   synthetic sample portfolio, written to the ignored
//!   `artifacts/owner-fixes-1/` for review.
use super::*;
use crate::chat::backend::AgentEvent;
use crate::headless_render::SoftwarePainter;

fn at(p: &ProjectData, text: &str) -> i64 {
    openxer_xer::parse_relative(p.model.origin_serial, text)
        .unwrap()
        .unwrap()
}

/// A date as the dialogs show and read it.
fn day(p: &ProjectData, text: &str) -> String {
    crate::format::date(p.model.origin_serial, Some(at(p, text)))
}

type Progress = Vec<(StatusType, Option<i64>, Option<i64>, i64)>;

/// What the progress tools write on each activity.
fn progress_of(p: &ProjectData) -> Progress {
    p.model
        .activities
        .iter()
        .map(|a| (a.status, a.act_start, a.act_finish, a.rem))
        .collect()
}

fn tools(d: &mut Driver, item: &str) {
    d.home();
    d.click("menu_bar_tools");
    d.frames(1);
    d.click(item);
    d.frames(2);
}

/// Apply, answering the future date question Yes if it is asked, and
/// check no error was shown.
fn apply(d: &mut Driver, key: &str) {
    d.click(key);
    d.frames(2);
    if d.app.sched.progress.confirm_future.is_some() {
        d.key(Key::Enter);
        d.frames(2);
    }
    d.assert_clean(key);
}

/// The notice after a run, closed with OK.
fn close_notice(d: &mut Driver) {
    assert!(
        d.has("progress_done_ok"),
        "a notice follows the run: {} {:?}",
        d.app.status,
        d.app.error
    );
    d.key(Key::Enter);
    d.frames(2);
}

#[test]
fn workflow_update_progress_and_apply_actuals_take_an_earlier_data_date() {
    let mut d = fresh("qa_owner_earlier_progress");
    show(&mut d, "activities");
    let p = d.app.project.clone();
    // Two weeks of progress first, so there is recorded progress to keep.
    tools(&mut d, "menu_update_progress");
    let draft = d.app.sched.progress.update.as_mut().expect("it opens");
    draft.new_date = day(&p, "2026-11-16 08:00");
    apply(&mut d, "update_apply");
    close_notice(&mut d);
    let progressed = progress_of(&d.app.project);
    let done = progressed
        .iter()
        .filter(|a| a.0 == StatusType::Completed)
        .count();
    assert!(done > 0, "the later date records progress");
    let dd = |d: &Driver| d.app.project.model.project.data_date;
    assert_eq!(dd(&d), Some(at(&p, "2026-11-16 08:00")));

    // The same date: the run goes ahead and progresses nothing.
    tools(&mut d, "menu_update_progress");
    let draft = d.app.sched.progress.update.clone().unwrap();
    assert_eq!(draft.new_date, day(&p, "2026-11-16 08:00"), "proposed");
    apply(&mut d, "update_apply");
    close_notice(&mut d);
    assert_eq!(dd(&d), Some(at(&p, "2026-11-16 08:00")));
    assert_eq!(progress_of(&d.app.project), progressed);

    // An earlier date: the data date moves back, recorded progress stays.
    tools(&mut d, "menu_update_progress");
    let draft = d.app.sched.progress.update.as_mut().unwrap();
    draft.new_date = day(&p, "2026-11-09 08:00");
    let steps = d.app.history.counts().0;
    apply(&mut d, "update_apply");
    close_notice(&mut d);
    assert_eq!(dd(&d), Some(at(&p, "2026-11-09 08:00")), "moved back");
    assert_eq!(progress_of(&d.app.project), progressed, "nothing removed");
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");

    // Apply Actuals to a date earlier still, the same way.
    tools(&mut d, "menu_apply_actuals");
    let draft = d.app.sched.progress.actuals.as_mut().expect("it opens");
    draft.own_date = false;
    draft.new_date = day(&p, "2026-11-05 08:00");
    apply(&mut d, "actuals_apply");
    close_notice(&mut d);
    assert_eq!(dd(&d), Some(at(&p, "2026-11-05 08:00")), "moved back");
    assert_eq!(progress_of(&d.app.project), progressed, "nothing removed");

    // Undo once: the data date Update Progress set.
    d.home();
    d.click("undo");
    d.frames(2);
    assert_eq!(dd(&d), Some(at(&p, "2026-11-09 08:00")), "undo");
    assert_eq!(progress_of(&d.app.project), progressed);
}

/// Projects window, Dates tab of workspace project `id`.
fn dates_tab(d: &mut Driver, id: i64) {
    projects_select(d, id);
    let tab = views::projects::TABS
        .iter()
        .position(|t| *t == "Dates")
        .unwrap();
    d.click(&format!("projects_tab_{tab}"));
}

#[test]
fn workflow_the_dates_tab_and_schedule_dialog_take_an_earlier_data_date() {
    let mut d = fresh("qa_owner_earlier_dates");
    let a = new_project(&mut d, "OF-A", "Owner Fix Unit", "04-Jan-27", "");
    show(&mut d, "activities");
    add_activity(&mut d, "Set pump skid", "5d");
    d.f9();
    d.key_with(Key::S, ctrl());
    let p = d.app.project.clone();
    let old = p.model.project.data_date;

    // Dates tab: an earlier data date is taken as typed, no question.
    dates_tab(&mut d, a);
    let steps = d.app.history.counts().0;
    type_into(&mut d, "project_data_date", "28-Dec-26 08:00");
    d.key(Key::Enter);
    d.frames(2);
    d.assert_clean("earlier data date on the Dates tab");
    let dd = |d: &Driver| d.app.project.model.project.data_date;
    assert_eq!(dd(&d), Some(at(&p, "2026-12-28 08:00")), "taken");
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    d.home();
    d.click("undo");
    d.frames(2);
    assert_eq!(dd(&d), old, "undo");
    d.click("redo");
    d.frames(2);
    assert_eq!(dd(&d), Some(at(&p, "2026-12-28 08:00")), "redo");

    // Schedule dialog: the selected data date, earlier again.
    d.home();
    d.key(Key::F9);
    d.click("schedule_apply_date");
    d.frames(1);
    type_into(&mut d, "schedule_data_date", "21-Dec-26 08:00");
    d.click("schedule_run");
    d.frames(2);
    d.assert_clean("earlier data date in the Schedule dialog");
    assert!(!d.app.sched.schedule_open, "it scheduled and closed");
    assert_eq!(dd(&d), Some(at(&p, "2026-12-21 08:00")));
    let q = &d.app.project;
    let i = (0..q.model.activities.len())
        .find(|&i| q.name(i) == "Set pump skid")
        .unwrap();
    assert_eq!(
        d.app.project.model.activities[i].early_start,
        Some(at(&p, "2026-12-21 08:00")),
        "the work runs from the earlier data date"
    );
}

#[test]
fn workflow_the_command_line_takes_an_earlier_data_date() {
    let d = fresh("qa_owner_earlier_cli");
    let p = d.app.project.clone();
    let input = d.dir.join("owner-cli-in.xer");
    let output = d.dir.join("owner-cli-out.xer");
    p.updated_document().write(&input).unwrap();
    let run = |ops: serde_json::Value| {
        let request = serde_json::json!({"command":"apply","input":input,"output":output,
            "changes":{"format":"openxer-changes/1","operations":ops}});
        openxer_automation::response(
            openxer_automation::parse_command(request).and_then(openxer_automation::dispatch),
        )
    };
    let r = run(serde_json::json!([
        {"op":"update_progress","data_date":"2026-11-16 08:00"},
        {"op":"update_progress","data_date":"2026-11-09 08:00"},
        {"op":"apply_actuals","data_date":"2026-11-06 08:00"},
        {"op":"set_project","field":"data_date","value":"2026-11-04 08:00"}
    ]));
    assert_eq!(r["ok"], true, "{r}");
    let results = &r["result"]["report"]["results"];
    let changed = results[0]["changed_activities"].as_u64().unwrap();
    assert!(changed > 0, "{r}");
    assert_eq!(results[1]["changed_activities"], 0, "nothing removed: {r}");
    assert_eq!(results[2]["changed_activities"], 0, "nothing added: {r}");
    let back = openxer_xer::Document::read(&output).unwrap();
    let project = back.table("PROJECT").unwrap();
    let value = |k: &str| project.value(&project.rows[0], k).to_string();
    assert_eq!(value("last_recalc_date"), "2026-11-04 08:00");
    assert_eq!(value("apply_actuals_date"), "2026-11-06 08:00");
    let tasks = back.table("TASK").unwrap();
    let done = tasks
        .rows
        .iter()
        .filter(|r| tasks.value(r, "status_code") == "TK_Complete")
        .count();
    assert!(done > 0, "the recorded progress stays");
}

// ------------------------------------------------------------------ renders

/// The sample portfolio (two projects) open and scheduled in a fresh app.
fn portfolio(name: &str) -> (App, egui::Context, std::path::PathBuf) {
    let doc = openxer_xer::starter_portfolio();
    let a = ProjectData::from_document(doc.clone(), 1, "sample".into()).unwrap();
    let b = ProjectData::from_document(doc, 2, "sample".into()).unwrap();
    let file = format!("openxer-{name}-{}.db", std::process::id());
    let db = std::env::temp_dir().join(file);
    let _ = std::fs::remove_file(&db);
    let mut store = Store::open(db.clone()).unwrap();
    let b_id = store.save(&b).unwrap();
    let a_id = store.save(&a).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, a);
    app.restore_open_set();
    app.open_project_set(&[a_id, b_id]);
    app.schedule();
    (app, ctx, db)
}

/// How a render opens its dialog: `open`, a frame, then `then` (a window
/// placed after its stored place was applied).
type Open = fn(&mut App, &egui::Context);

/// Render dialog or window `client` after `open` and write it, cropped to
/// its window, as `artifacts/owner-fixes-1/<file>.png`.
fn render(file: &str, client: &str, open: Open, then: Option<Open>) {
    let (mut app, ctx, db) = portfolio(file);
    let mut painter = SoftwarePainter::default();
    painter.screen = Some(Vec2::new(1400., 900.));
    let mut time = 0.;
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    open(&mut app, &ctx);
    let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    if let Some(then) = then {
        then(&mut app, &ctx);
    }
    for _ in 0..4 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    let inner = crate::dialog_kit::noted_client(&ctx, client)
        .unwrap_or_else(|| panic!("{file}: {client} was not drawn"));
    let (border, shadow) = (Vec2::new(1., 32.), Vec2::new(4., 4.));
    let window = Rect::from_min_max(inner.min - border, inner.max + shadow);
    let x = window.left().round().max(0.) as u32;
    let y = window.top().round().max(0.) as u32;
    let w = (window.width().round() as u32).min(image.width() - x);
    let h = (window.height().round() as u32).min(image.height() - y);
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/owner-fixes-1");
    std::fs::create_dir_all(&dir).unwrap();
    image::imageops::crop_imm(&image, x, y, w, h)
        .to_image()
        .save(dir.join(format!("{file}.png")))
        .unwrap();
    drop(app);
    let _ = std::fs::remove_file(db);
}

/// A reply with a table: short IDs, dates, and a column of long text with
/// an unbreakable reference in it.
const TABLE_REPLY: &str = "Three activities drive the finish:\n\n\
| Activity | Name | Finish | Why it matters |\n|---|---|---|---|\n\
| RB-1020 | Complete detailed design | 13-Nov-26 | Holds the construction package; a slip moves handover day for day |\n\
| RB-1040 | Purchase long-lead equipment | 25-Nov-26 | Quote VQ-2026-11-0045-REV-C-FINAL expires first |\n\
| RB-1090 | Mechanical installation | 18-Dec-26 | Zero float |\n\n\
Ask me to trace any of them.";

/// The Chat window answering with [`TABLE_REPLY`].
fn chat_with_table(app: &mut App, _: &egui::Context) {
    let (fake, _log) = crate::chat::backend::FakeBackend::new();
    app.chat.backend = Box::new(fake);
    app.chat.open = true;
    let conv = &mut app.chat.conv;
    conv.send("Which activities drive the finish?");
    conv.apply(AgentEvent::Message(TABLE_REPLY.into()));
    conv.apply(AgentEvent::TurnFinished { usage: None });
}

/// The Chat window placed with its client `width` wide.
fn chat_width(ctx: &egui::Context, width: f32) {
    crate::dialog_kit::set_geometry(
        ctx,
        crate::chat::window_id(),
        crate::dialog_kit::Geometry {
            pos: Some([300., 60.]),
            client: Some([width, crate::chat::CLIENT.y]),
            rolled: false,
        },
    );
}

#[test]
fn owner_fixes_renders() {
    let import: [(&str, Open); 5] = [
        ("1-import-0-format", |app, _| app.open_import_wizard_at(0)),
        ("1-import-1-file", |app, _| app.open_import_wizard_at(1)),
        ("1-import-2-project-options", |app, _| {
            app.open_import_wizard_at(2)
        }),
        ("1-import-3-update-options", |app, _| {
            app.open_import_wizard_at(3)
        }),
        ("1-import-4-summary", |app, _| app.open_import_wizard_at(4)),
    ];
    for (file, open) in import {
        render(file, "xer_import", open, None);
    }
    const MIN_WIDTH: f32 = crate::chat::MIN_CLIENT.x;
    let chat: [(&str, Open); 3] = [
        ("2-chat-table", |_, ctx| {
            chat_width(ctx, crate::chat::CLIENT.x)
        }),
        ("2-chat-table-narrowest", |_, ctx| {
            chat_width(ctx, MIN_WIDTH)
        }),
        ("2-chat-table-wide", |_, ctx| chat_width(ctx, 620.)),
    ];
    for (file, then) in chat {
        render(file, crate::chat::SALT, chat_with_table, Some(then));
    }
    let dialogs: [(&str, &str, Open); 2] = [
        ("3-update-progress", "update", |app, _| {
            app.open_update_progress()
        }),
        ("3-apply-actuals", "actuals", |app, _| {
            app.open_apply_actuals()
        }),
    ];
    for (file, client, open) in dialogs {
        render(file, client, open, None);
    }
}
