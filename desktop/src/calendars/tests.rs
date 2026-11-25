//! Calendar edits: table operations, scheduling effect, export and reimport, undo
//! and redo through the production UI, and a software render of the dialogs.
use super::editor::{from_slots, serial, slots, total_day};
use super::*;
use crate::headless_render::SoftwarePainter;
use crate::store::Store;
use openxer_xer::calendar_data::CalendarData;

/// Monday 2026-11-02, the starter project's data date.
const MONDAY: i64 = 46328;

fn scheduled_starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}
fn start_of(p: &ProjectData, code: &str) -> String {
    let a = p
        .model
        .activities
        .iter()
        .find(|a| a.task_code == code)
        .unwrap();
    date_string(p.model.origin_serial, a.early_start)
}

#[test]
fn slot_and_total_helpers() {
    let day = vec![(480, 720), (780, 1020)];
    let s = slots(&day);
    assert_eq!(s.iter().filter(|w| **w).count(), 16);
    assert_eq!(from_slots(&s), day);
    assert_eq!(total_day(&day, 10.), vec![(480, 1080)]);
    assert_eq!(total_day(&[(1200, 1440)], 8.), vec![(960, 1440)]);
    assert!(total_day(&day, 0.).is_empty());
    assert_eq!(
        serial(chrono::NaiveDate::from_ymd_opt(2026, 11, 2).unwrap()),
        MONDAY
    );
}

#[test]
fn add_rename_default_convert_and_delete() {
    let mut p = openxer_xer::starter();
    let pid = p.model.project.proj_id;
    let stamp = "2026-11-04 09:00";
    let global = ops::add(&mut p, Kind::Global, Some("1"), stamp).unwrap();
    assert_eq!(p.calendar_names.len(), 2);
    assert_eq!(p.model.calendars.len(), 2);
    ops::rename(&mut p, &global, "Night shift").unwrap();
    assert_eq!(p.calendar_names[1], "Night shift");
    assert!(ops::rename(&mut p, &global, "  ").is_err());
    assert!(ops::rename(&mut p, &global, "standard 5-day / 8-hour").is_err());
    ops::set_default(&mut p, &global).unwrap();
    let list = ops::list(&p.document, Kind::Global, pid);
    assert_eq!(
        list.iter().map(|e| e.default).collect::<Vec<_>>(),
        [false, true]
    );
    // A project calendar copied from a global calendar inherits from it.
    let project = ops::add(&mut p, Kind::Project, Some("1"), stamp).unwrap();
    assert_eq!(ops::value(&p.document, &project, "base_clndr_id"), "1");
    assert_eq!(
        ops::value(&p.document, &project, "proj_id"),
        pid.to_string()
    );
    assert_eq!(ops::list(&p.document, Kind::Project, pid).len(), 1);
    let usage = ops::usage(&p, "1");
    assert_eq!(usage.activities, 14);
    assert_eq!(usage.projects, ["RIVERBEND"]);
    assert_eq!(usage.derived, ["New Calendar"]);
    let error = ops::delete(&mut p, "1").unwrap_err();
    assert!(error.contains("14 activities"), "{error}");
    assert!(ops::delete(&mut p, &global).is_err(), "default global");
    ops::convert(&mut p, &project, Convert::ToGlobal, stamp).unwrap();
    assert_eq!(ops::kind(&p.document, &project), Kind::Global);
    assert_eq!(ops::value(&p.document, &project, "base_clndr_id"), "");
    ops::delete(&mut p, &project).unwrap();
    assert_eq!(p.model.calendars.len(), 2);
    assert!(p.model.validate().is_ok());
    let resource = ops::add(&mut p, Kind::Resource, None, stamp).unwrap();
    assert_eq!(
        ops::assignable(&p, 0),
        [0, 1],
        "resource calendars are not offered"
    );
    ops::convert(&mut p, &resource, Convert::Personal, stamp).unwrap();
    assert!(ops::list(&p.document, Kind::Resource, pid)[0].personal);
    assert!(ops::convert(&mut p, &resource, Convert::ToGlobal, stamp).is_err());
}

#[test]
fn nonwork_day_moves_the_schedule_and_survives_export() {
    let mut p = scheduled_starter();
    assert_eq!(start_of(&p, "RB-1010"), "2026-11-02 08:00");
    let mut data = ops::data(&p.document, "1").unwrap();
    data.set_day(MONDAY, vec![]);
    assert!(ops::commit(
        &mut p,
        "1",
        &data,
        [8., 40., 172., 2000.],
        "2026-11-04 09:00"
    )
    .unwrap());
    assert!(!p.model.calendars[0].cal.exc.is_empty());
    schedule_model(&mut p.model, &p.resources).unwrap();
    assert_eq!(start_of(&p, "RB-1010"), "2026-11-03 08:00");
    let text = p.updated_document().text();
    let reread =
        ProjectData::from_document(Document::parse(&text).unwrap(), 1, "re".into()).unwrap();
    let again = CalendarData::parse(&ops::value(&reread.document, "1", "clndr_data")).unwrap();
    assert_eq!(again, data);
    assert_eq!(ops::hours(&reread.document, "1"), [8., 40., 172., 2000.]);
    assert_eq!(
        ops::value(&reread.document, "1", "last_chng_date"),
        "2026-11-04 09:00"
    );
    let mut reread = reread;
    schedule_model(&mut reread.model, &reread.resources).unwrap();
    assert_eq!(start_of(&reread, "RB-1010"), "2026-11-03 08:00");
    // An unchanged commit writes nothing.
    assert!(!ops::commit(&mut p, "1", &data, [8., 40., 172., 2000.], "x").unwrap());
}

#[test]
fn base_exceptions_flow_to_derived_calendars() {
    let mut p = openxer_xer::starter();
    let derived = ops::add(&mut p, Kind::Project, Some("1"), "s").unwrap();
    let mut own = ops::data(&p.document, &derived).unwrap();
    own.set_day(MONDAY + 1, vec![(0, 1440)]);
    ops::commit(&mut p, &derived, &own, [24., 168., 720., 8760.], "s").unwrap();
    let mut base = ops::data(&p.document, "1").unwrap();
    base.set_day(MONDAY, vec![]);
    base.set_day(MONDAY + 1, vec![]);
    let hours = ops::hours(&p.document, "1");
    ops::commit(&mut p, "1", &base, hours, "s").unwrap();
    let d = ops::data(&p.document, &derived).unwrap();
    assert_eq!(d.exceptions.get(&MONDAY), Some(&vec![]));
    assert_eq!(d.exceptions.get(&(MONDAY + 1)), Some(&vec![(0, 1440)]));
    let i = p
        .model
        .calendars
        .iter()
        .position(|c| c.id == derived)
        .unwrap();
    assert_eq!(p.model.calendars[i].cal.exc.len(), 2);
}

fn open_app() -> (egui::Context, App, SoftwarePainter, f64, std::path::PathBuf) {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "openxer-calendars-{}-{unique}.db",
        std::process::id()
    ));
    let store = Store::open(path.clone()).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, scheduled_starter());
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    (ctx, app, painter, time, path)
}

#[test]
fn edit_through_the_dialog_then_undo_and_redo() {
    let (ctx, mut app, mut painter, mut time, path) = open_app();
    app.open_calendars();
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "calendars_modify");
    assert!(app.calendars.as_ref().unwrap().editor.is_some());
    painter.click_action(&mut app, &ctx, &mut time, "calendar_day_2026-11-02");
    painter.click_action(&mut app, &ctx, &mut time, "calendar_nonwork");
    // Detailed hours: open the first half hour of Tuesday as work.
    painter.click_action(&mut app, &ctx, &mut time, "calendar_detailed_mode");
    painter.click_action(&mut app, &ctx, &mut time, "calendar_day_2026-11-03");
    painter.click_action(&mut app, &ctx, &mut time, "calendar_slots_15");
    let before = app.project.model.calendars[0].clone();
    painter.click_action(&mut app, &ctx, &mut time, "calendar_ok");
    assert!(app.calendars.as_ref().unwrap().editor.is_none());
    let cal = &app.project.model.calendars[0].cal;
    assert_eq!(cal.exc.get(&MONDAY), Some(&vec![]));
    assert_eq!(
        cal.exc.get(&(MONDAY + 1)),
        Some(&vec![(450, 720), (780, 1020)])
    );
    let data = ops::data(&app.project.document, "1").unwrap();
    assert_eq!(data.show_total, Some(false));
    assert!(app.dirty);
    app.schedule();
    assert_eq!(start_of(&app.project, "RB-1010"), "2026-11-03 07:30");
    // Undo the schedule, then the calendar edit.
    app.history_move(&ctx, false);
    app.history_move(&ctx, false);
    assert_eq!(app.project.model.calendars[0], before);
    assert!(ops::data(&app.project.document, "1")
        .unwrap()
        .exceptions
        .is_empty());
    app.history_move(&ctx, true);
    assert_eq!(
        app.project.model.calendars[0].cal.exc.get(&MONDAY),
        Some(&vec![])
    );
    // Rename and add show up in the activity calendar list and undo together.
    painter.click_action(&mut app, &ctx, &mut time, "calendars_add");
    painter.click_action(&mut app, &ctx, &mut time, "calendars_copy_select");
    let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    assert_eq!(app.project.calendar_names.len(), 2);
    assert_eq!(app.project.model.calendars.len(), 2);
    app.history_move(&ctx, false);
    assert_eq!(app.project.calendar_names.len(), 1);
    assert_eq!(app.project.model.calendars.len(), 1);
    painter.click_action(&mut app, &ctx, &mut time, "calendars_close");
    assert!(app.calendars.is_none());
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn render_calendar_dialogs_to_png() {
    let artifacts = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let (ctx, mut app, mut painter, mut time, path) = open_app();
    {
        // A holiday and a short Friday so the month view shows each day style.
        let mut data = ops::data(&app.project.document, "1").unwrap();
        data.set_day(MONDAY + 24, vec![]);
        data.set_day(MONDAY + 25, vec![]);
        data.set_day(MONDAY + 4, vec![(480, 720)]);
        ops::commit(&mut app.project, "1", &data, [8., 40., 172., 2000.], "s").unwrap();
        ops::add(&mut app.project, Kind::Global, Some("1"), "s").unwrap();
    }
    app.open_calendars();
    let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    let mut shots = vec![];
    for (name, action) in [
        ("headless-calendars.png", None),
        ("headless-calendar-editor.png", Some("calendars_modify")),
        ("headless-calendar-workweek.png", Some("calendar_workweek")),
    ] {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        if let Some(action) = action {
            painter.click_action(&mut app, &ctx, &mut time, action);
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        let (image, triangles) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        assert!(triangles > 1000);
        // The month view paints nonwork days in their gray.
        if name == "headless-calendar-editor.png" {
            let gray = image
                .pixels()
                .filter(|p| p.0 == [0xBF, 0xCD, 0xDB, 255])
                .count();
            assert!(gray > 2000, "{gray} nonwork pixels");
        }
        image.save(artifacts.join(name)).unwrap();
        shots.push(name);
    }
    assert_eq!(shots.len(), 3);
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// A real file with project calendars derived from global ones: editing the base
/// reaches the derived calendar, scheduling still runs, and every other CALENDAR
/// row exports byte for byte. The file's name identifies its job, so it comes
/// from the private fixture `corpus-names.txt` (`leveled-selected <file>`).
#[test]
fn corpus_base_edit_reaches_derived_project_calendar() {
    let Some(listed) = openxer_engine::fixture::corpus_names("leveled-selected") else {
        return;
    };
    let Some((name, _)) = listed.into_iter().next() else {
        openxer_engine::fixture::skip("corpus-names.txt lists no leveled-selected file");
        return;
    };
    let path = crate::test_corpus::file(&name);
    let Ok(doc) = Document::read(&path) else {
        openxer_engine::fixture::skip(&format!("{} is not readable", path.display()));
        return;
    };
    let pid = doc.projects()[0].0;
    let mut p = ProjectData::from_document(doc, pid, name.clone()).unwrap();
    let before = p.document.table("CALENDAR").unwrap().clone();
    let base = "1751";
    let derived = "6249";
    assert_eq!(ops::value(&p.document, derived, "base_clndr_id"), base);
    let mut data = ops::data(&p.document, base).unwrap();
    let holiday = serial(chrono::NaiveDate::from_ymd_opt(2026, 11, 25).unwrap());
    assert!(!data.shifts_on(holiday).is_empty());
    data.set_day(holiday, vec![]);
    let hours = ops::hours(&p.document, base);
    ops::commit(&mut p, base, &data, hours, "2026-11-04 10:00").unwrap();
    let d = ops::data(&p.document, derived).unwrap();
    assert_eq!(d.exceptions.get(&holiday), Some(&vec![]));
    let i = p
        .model
        .calendars
        .iter()
        .position(|c| c.id == derived)
        .unwrap();
    assert_eq!(p.model.calendars[i].cal.exc.get(&holiday), Some(&vec![]));
    let mut scheduled = p.clone();
    scheduled.refresh_derived_durations();
    schedule_model(&mut scheduled.model, &scheduled.resources).unwrap();
    let out = Document::parse(&p.updated_document().text()).unwrap();
    let after = out.table("CALENDAR").unwrap();
    let id = after.field("clndr_id").unwrap();
    for (old, new) in before.rows.iter().zip(&after.rows) {
        if new[id] != base && new[id] != derived {
            assert_eq!(old, new);
        }
    }
    let reread = ProjectData::from_document(out, pid, name).unwrap();
    assert_eq!(reread.model.calendars, p.model.calendars);
}

fn editor_shifts(d: &crate::qa_sweep::Driver, day: i64) -> Vec<(i64, i64)> {
    let ed = d.app.calendars.as_ref().unwrap().editor.as_ref().unwrap();
    ed.data.shifts_on(day).to_vec()
}

fn type_hours(d: &mut crate::qa_sweep::Driver, text: &str) {
    d.click("calendar_total_hours");
    d.key_with(egui::Key::A, egui::Modifiers::COMMAND);
    d.type_text(text);
    d.frames(1);
}

/// Total work hours apply when the edit is committed, change only the last
/// work period (so the lunch break survives), and the box follows Work,
/// Nonwork and Standard. Escape first cancels the field being typed in,
/// and a changed draft asks before it is thrown away.
#[test]
fn total_hours_commit_keep_the_lunch_break_and_escape_asks() {
    let mut d = crate::qa_sweep::Driver::new("calendar-total", scheduled_starter());
    d.app.open_calendars();
    d.frames(2);
    d.click("calendars_modify");
    assert!(
        d.app.calendars.as_ref().unwrap().editor.is_some(),
        "Modify opens the editor"
    );
    let tuesday = MONDAY + 1;
    d.click("calendar_day_2026-11-03");
    assert!(
        d.app.calendars.as_ref().unwrap().editor.is_some(),
        "a day click keeps the editor"
    );
    let standard = vec![(480, 720), (780, 1020)];
    assert_eq!(editor_shifts(&d, tuesday), standard);
    // Typing alone changes nothing; Enter applies 9 hours to the last period.
    type_hours(&mut d, "24");
    assert_eq!(editor_shifts(&d, tuesday), standard, "not per keystroke");
    type_hours(&mut d, "9");
    d.key(egui::Key::Enter);
    assert_eq!(editor_shifts(&d, tuesday), vec![(480, 720), (780, 1080)]);
    type_hours(&mut d, "8");
    d.key(egui::Key::Enter);
    assert_eq!(
        editor_shifts(&d, tuesday),
        standard,
        "back to 8 keeps lunch"
    );
    // Escape in the field puts the field back and keeps the editor open.
    type_hours(&mut d, "3");
    d.key(egui::Key::Escape);
    assert!(d.app.calendars.as_ref().unwrap().editor.is_some());
    assert_eq!(editor_shifts(&d, tuesday), standard);
    let text = |d: &crate::qa_sweep::Driver| {
        let st = d.app.calendars.as_ref().unwrap();
        st.editor.as_ref().unwrap().total_text.clone()
    };
    assert_eq!(text(&d), "8.0");
    // Nonwork, Work and Standard refresh the hours box.
    d.click("calendar_nonwork");
    assert_eq!(text(&d), "0.0");
    d.click("calendar_standard");
    assert_eq!(text(&d), "8.0");
    d.click("calendar_nonwork");
    // A changed draft: Escape asks; No keeps editing, Yes discards.
    d.key(egui::Key::Escape);
    assert!(d.app.calendars.as_ref().unwrap().editor.is_some());
    assert!(
        d.has("calendar_discard_no"),
        "the discard question is shown"
    );
    d.click("calendar_discard_no");
    assert!(d.app.calendars.as_ref().unwrap().editor.is_some());
    assert_eq!(editor_shifts(&d, tuesday), vec![]);
    d.click("calendar_cancel");
    d.click("calendar_discard_yes");
    assert!(d.app.calendars.as_ref().unwrap().editor.is_none());
    let data = ops::data(&d.app.project.document, "1").unwrap();
    assert!(data.exceptions.is_empty(), "nothing was written");
    // An unchanged draft closes on Escape without asking.
    d.click("calendars_modify");
    d.key(egui::Key::Escape);
    assert!(d.app.calendars.as_ref().unwrap().editor.is_none());
}

#[test]
fn total_hours_change_only_the_last_period() {
    use super::editor::adjust_total;
    let day = vec![(480, 720), (780, 1020)];
    assert_eq!(adjust_total(&day, 9.), vec![(480, 720), (780, 1080)]);
    assert_eq!(adjust_total(&day, 7.), vec![(480, 720), (780, 960)]);
    assert_eq!(adjust_total(&day, 3.5), vec![(480, 690)]);
    assert_eq!(adjust_total(&day, 8.25), vec![(480, 720), (780, 1035)]);
    assert!(adjust_total(&day, 0.).is_empty());
    assert_eq!(adjust_total(&[], 8.), vec![(480, 960)]);
    // Past the end of the day: the first period starts earlier, then the
    // break fills.
    assert_eq!(adjust_total(&day, 15.), vec![(480, 720), (780, 1440)]);
    assert_eq!(adjust_total(&day, 23.), vec![(0, 720), (780, 1440)]);
    assert_eq!(adjust_total(&day, 24.), vec![(0, 1440)]);
    // A night shift past midnight keeps its start.
    assert_eq!(adjust_total(&[(1320, 1800)], 10.), vec![(1320, 1920)]);
}

#[test]
fn detailed_grid_keeps_times_it_cannot_show_until_edited() {
    use super::editor::toggle;
    // A quarter-hour start and a night shift past midnight.
    let day = vec![(435, 720), (1320, 1800)];
    let s = slots(&day);
    assert!(s[14], "07:00 shows the 07:15 start");
    assert!(
        s[0] && s[11],
        "hours past midnight show at the top of the grid"
    );
    // Toggling the afternoon on adds just that half hour.
    assert_eq!(
        toggle(&day, 26, true),
        vec![(435, 720), (780, 810), (1320, 1800)]
    );
    // Toggling a midnight hour off cuts the night shift there only.
    assert_eq!(toggle(&day, 11, false), vec![(435, 720), (1320, 1770)]);
    // Turning off 08:00 keeps 07:15 as it was.
    assert_eq!(
        toggle(&day, 16, false),
        vec![(435, 480), (510, 720), (1320, 1800)]
    );
}
