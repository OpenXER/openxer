//! The Page Setup timescale date picker: each anchor, offsets, a specific
//! date, OK, Cancel and Escape, the round trip of an existing expression,
//! the resolved date against what printing uses, and Same as screen.
#![allow(clippy::field_reassign_with_default)]
use super::setup::{PageSetup, PrintOptions};
use super::ts_picker::{Field, Request, TsPicker};
use super::*;
use crate::qa_sweep::Driver;
use chrono::{Datelike, NaiveDate};

fn starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

/// Page Setup on its Options tab over a drawn chart.
fn driver(name: &str) -> Driver {
    let mut d = Driver::new(name, starter());
    d.frames(3);
    assert!(d.app.print.screen_seen, "the chart was drawn");
    d.app.open_page_setup();
    d.frames(3);
    d.click("page_setup_tab_4");
    d.frames(2);
    d
}

fn dialog(d: &Driver) -> &super::dialog::SetupDialog {
    d.app.print.dialog.as_ref().expect("Page Setup is open")
}

fn options(d: &Driver) -> &PrintOptions {
    &dialog(d).draft.options
}

fn ctrl() -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    }
}

fn open_picker(d: &mut Driver, field: &str) {
    d.click(&format!("page_setup_ts_{field}_pick"));
    d.frames(3);
    assert!(d.has("ts_picker_ok"), "the picker opened");
    assert!(dialog(d).picker.is_some());
}

fn choose(d: &mut Driver, combo: &str, item: usize) {
    d.click(combo);
    d.frames(2);
    d.click(&format!("{combo}_item_{item}"));
    d.frames(2);
}

fn set_count(d: &mut Driver, text: &str) {
    d.click("ts_picker_count");
    d.key_with(Key::A, ctrl());
    d.type_text(text);
    d.frames(2);
}

fn ok(d: &mut Driver) {
    d.click("ts_picker_ok");
    d.frames(3);
}

fn picker(d: &Driver) -> &TsPicker {
    dialog(d).picker.as_ref().expect("the picker is open")
}

fn untick(d: &mut Driver) {
    d.click("page_setup_same_as_screen");
    d.frames(2);
    assert!(!options(d).same_as_screen);
}

/// The text of a date in the user's format, as the boxes hold it.
fn day(d: &Driver, y: i32, m: u32, day: u32) -> String {
    let date = NaiveDate::from_ymd_opt(y, m, day).unwrap();
    let serial = (date - NaiveDate::from_ymd_opt(1899, 12, 30).unwrap()).num_days();
    d.app
        .day_text((serial - d.app.project.model.origin_serial) * DAY)
}

#[test]
fn every_anchor_writes_its_code_and_unticks_same_as_screen() {
    let mut d = driver("ts-anchors");
    assert!(options(&d).same_as_screen);
    let all = ["CD", "DD", "PS", "PF", "SW", "SM"];
    for (i, code) in all.into_iter().enumerate() {
        open_picker(&mut d, "start");
        choose(&mut d, "ts_picker_anchor", i);
        ok(&mut d);
        assert!(dialog(&d).picker.is_none(), "OK closes the picker");
        assert_eq!(options(&d).timescale_start, code);
        assert!(!options(&d).same_as_screen);
    }
    // The finish box has its own picker.
    open_picker(&mut d, "finish");
    choose(&mut d, "ts_picker_anchor", 3);
    ok(&mut d);
    assert_eq!(options(&d).timescale_finish, "PF");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn offsets_take_a_sign_a_number_and_a_unit() {
    let mut d = driver("ts-offsets");
    untick(&mut d);
    for (anchor, minus, count, unit, expected) in [
        (1, true, "2", 1, "DD-2W"),
        (3, false, "3", 2, "PF+3M"),
        (2, false, "1", 3, "PS+1Y"),
        (0, false, "10", 0, "CD+10D"),
        (4, true, "1", 1, "SW-1W"),
        (5, false, "2", 2, "SM+2M"),
        (1, false, "0", 0, "DD"),
    ] {
        open_picker(&mut d, "start");
        choose(&mut d, "ts_picker_anchor", anchor);
        choose(&mut d, "ts_picker_sign", usize::from(minus));
        set_count(&mut d, count);
        choose(&mut d, "ts_picker_unit", unit);
        ok(&mut d);
        assert_eq!(options(&d).timescale_start, expected);
    }
}

#[test]
fn an_offset_that_cannot_be_printed_leaves_ok_disabled() {
    let mut d = driver("ts-bad-offset");
    untick(&mut d);
    let before = options(&d).clone();
    open_picker(&mut d, "start");
    set_count(&mut d, "x1");
    d.click("ts_picker_ok");
    d.frames(3);
    assert!(dialog(&d).picker.is_some(), "OK does nothing");
    set_count(&mut d, "99999");
    choose(&mut d, "ts_picker_unit", 3);
    d.click("ts_picker_ok");
    d.frames(3);
    assert!(dialog(&d).picker.is_some());
    assert_eq!(*options(&d), before);
    // A usable offset brings OK back.
    set_count(&mut d, "5");
    ok(&mut d);
    assert!(dialog(&d).picker.is_none());
    assert_ne!(*options(&d), before);
}

#[test]
fn a_specific_date_is_picked_on_the_month_calendar() {
    let mut d = driver("ts-specific");
    untick(&mut d);
    d.app
        .print
        .dialog
        .as_mut()
        .unwrap()
        .draft
        .options
        .timescale_start = "02-Mar-27".into();
    d.frames(2);
    open_picker(&mut d, "start");
    // It opens on the month of the date it holds.
    assert_eq!(picker(&d).view, (2027, 3));
    d.click("ts_picker_next_month");
    assert_eq!(picker(&d).view, (2027, 4));
    d.click("ts_picker_prev_year");
    assert_eq!(picker(&d).view, (2026, 4));
    d.click("ts_picker_next_year");
    d.click("ts_picker_prev_month");
    d.click("ts_picker_day_17");
    ok(&mut d);
    assert_eq!(options(&d).timescale_start, day(&d, 2027, 3, 17));
    // What was written reads back as that date when printing.
    let mut s = dialog(&d).draft.clone();
    s.options.timescale_finish = "PF+5Y".into();
    let (start, _) = d.app.print_timescale(&s).unwrap();
    assert_eq!(start, (date_serial(2027, 3, 17) - origin(&d)) * DAY);
}

fn date_serial(y: i32, m: u32, day: u32) -> i64 {
    (NaiveDate::from_ymd_opt(y, m, day).unwrap() - NaiveDate::from_ymd_opt(1899, 12, 30).unwrap())
        .num_days()
}

fn origin(d: &Driver) -> i64 {
    d.app.project.model.origin_serial
}

#[test]
fn an_existing_expression_comes_back_unchanged() {
    let mut d = driver("ts-round-trip");
    untick(&mut d);
    let date = day(&d, 2026, 11, 9);
    for text in [
        "DD-1W", "PF+1W", "PS", "CD+3D", "SW-1W", "SM+1M", "PF+2Y", &date,
    ] {
        d.app
            .print
            .dialog
            .as_mut()
            .unwrap()
            .draft
            .options
            .timescale_finish = text.to_string();
        d.frames(2);
        open_picker(&mut d, "finish");
        ok(&mut d);
        assert_eq!(options(&d).timescale_finish, text, "{text}");
    }
    // The controls hold the parts of the expression.
    d.app
        .print
        .dialog
        .as_mut()
        .unwrap()
        .draft
        .options
        .timescale_finish = "PF+3M".into();
    d.frames(2);
    open_picker(&mut d, "finish");
    let p = picker(&d);
    assert_eq!(
        (p.choice, p.minus, p.count.as_str(), p.unit),
        (3, false, "3", 2)
    );
    d.click("ts_picker_cancel");
    d.frames(2);
    // Text that does not read starts from the data date.
    d.app
        .print
        .dialog
        .as_mut()
        .unwrap()
        .draft
        .options
        .timescale_finish = "nonsense".into();
    d.frames(2);
    open_picker(&mut d, "finish");
    assert_eq!(picker(&d).choice, 1);
}

#[test]
fn cancel_escape_and_the_close_box_change_nothing() {
    let mut d = driver("ts-cancel");
    let before = options(&d).clone();
    assert!(before.same_as_screen);
    for how in ["cancel", "escape", "close"] {
        open_picker(&mut d, "start");
        choose(&mut d, "ts_picker_anchor", 3);
        d.click("ts_picker_day_12");
        match how {
            "cancel" => d.click("ts_picker_cancel"),
            "escape" => d.key(Key::Escape),
            _ => d.click("ts_picker_title_close"),
        }
        d.frames(3);
        assert!(dialog(&d).picker.is_none(), "{how} closes the picker");
        assert_eq!(*options(&d), before, "{how}");
        assert!(d.app.print.dialog.is_some(), "{how} leaves Page Setup open");
    }
    // Escape then closes Page Setup itself.
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.print.dialog.is_none());
    assert!(d.app.error.is_none());
}

#[test]
fn the_picker_drops_down_under_its_box() {
    let mut d = driver("ts-position");
    open_picker(&mut d, "finish");
    let field = d.rect("page_setup_ts_finish_pick");
    let ok = d.rect("ts_picker_ok");
    let close = d.rect("ts_picker_title_close");
    // The window's title bar sits just under the box, left aligned with it.
    assert!(close.top() > field.bottom(), "{close:?} under {field:?}");
    assert!(close.top() < field.bottom() + 12.);
    assert!(ok.left() > field.left() - 200.);
    let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1500., 900.));
    assert!(screen.contains_rect(ok) && screen.contains_rect(close));
}

#[test]
fn the_live_line_is_the_date_printing_uses() {
    let mut d = driver("ts-live");
    untick(&mut d);
    let mut s = PageSetup::default();
    s.options.same_as_screen = false;
    for text in [
        "DD-1W", "DD", "PF+1W", "PS", "PS+2Y", "CD+3D", "SW", "SM-1M",
    ] {
        let mut p = TsPicker::open(
            &d.app,
            Request {
                field: Field::Start,
                at: Pos2::ZERO,
                text: text.into(),
            },
        );
        let (written, minutes) = p.evaluate(&d.app).unwrap();
        assert_eq!(written, text);
        assert_eq!(minutes, d.app.resolve_print_expr(text).unwrap());
        s.options.timescale_start = text.into();
        s.options.timescale_finish = "PF+5Y".into();
        let (start, _) = d.app.print_timescale(&s).unwrap();
        assert_eq!(start, minutes.div_euclid(DAY) * DAY, "{text}");
    }
    // The anchors mean what they say.
    let data_date = d.app.project.model.project.data_date.unwrap();
    assert_eq!(d.app.resolve_print_expr("DD").unwrap(), data_date);
    assert_eq!(
        d.app.resolve_print_expr("DD-1W").unwrap(),
        data_date - 7 * DAY
    );
    let serial = |text: &str| {
        let m = d.app.resolve_print_expr(text).unwrap();
        date_of_minutes(&d, m)
    };
    let week = serial("SW");
    assert_eq!(week.weekday(), chrono::Weekday::Sun);
    let today = chrono::Local::now().date_naive();
    assert!(week <= today && (today - week).num_days() < 7);
    assert_eq!(serial("SM").day(), 1);
    assert_eq!(serial("SM").month(), today.month());
}

#[test]
fn a_finish_given_as_a_date_includes_that_whole_day() {
    let d = driver("ts-finish-day");
    let mut s = PageSetup::default();
    s.options.same_as_screen = false;
    s.options.timescale_start = "PS".into();
    s.options.timescale_finish = day(&d, 2027, 1, 28);
    let (_, finish) = d.app.print_timescale(&s).unwrap();
    assert_eq!(finish, (date_serial(2027, 1, 29) - origin(&d)) * DAY);
    // An expression keeps counting to the end of its own day.
    s.options.timescale_finish = "PF".into();
    let (_, finish) = d.app.print_timescale(&s).unwrap();
    let pf = d.app.resolve_print_expr("PF").unwrap();
    assert_eq!(finish, (pf + DAY - 1).div_euclid(DAY) * DAY);
}

#[test]
fn the_expression_a_date_is_written_in_reads_back_in_the_users_format() {
    let d = driver("ts-format");
    for (y, m, dd) in [(2026, 1, 5), (2027, 12, 31), (2030, 2, 28)] {
        let text = day(&d, y, m, dd);
        let minutes = d.app.resolve_print_expr(&text).unwrap();
        assert_eq!(
            minutes,
            (date_serial(y, m, dd) - origin(&d)) * DAY,
            "{text}"
        );
    }
}

#[test]
fn typing_in_a_box_unticks_same_as_screen_and_the_printout_uses_it() {
    let mut d = driver("ts-typing");
    assert!(options(&d).same_as_screen);
    let screen = d.app.screen_timescale().unwrap();
    let (screen_start, screen_finish) = (d.app.day_text(screen.0), d.app.day_text(screen.1));
    let stored = options(&d).clone();
    // The boxes are not greyed out: they show the screen's dates and take
    // typing at once.
    let typed_minutes = screen.0.div_euclid(DAY) * DAY + 3 * DAY;
    let typed = d.app.day_text(typed_minutes);
    d.click("page_setup_ts_start");
    d.key_with(Key::A, ctrl());
    d.type_text(&typed);
    d.frames(3);
    let o = options(&d);
    assert!(!o.same_as_screen, "typing unticks Same as screen");
    assert_eq!(o.timescale_start, typed);
    // The other box keeps the date it was showing.
    assert_eq!(o.timescale_finish, screen_finish);
    assert_ne!(o.timescale_start, screen_start);
    assert_ne!(o.timescale_finish, stored.timescale_finish);
    let setup = dialog(&d).draft.clone();
    let (start, finish) = d.app.print_timescale(&setup).unwrap();
    assert_eq!(
        start,
        typed_minutes,
        "typed {typed:?} screen {screen:?} origin {}",
        origin(&d)
    );
    assert_eq!(
        finish,
        screen.1.div_euclid(DAY) * DAY + DAY * i64::from(screen.1 % DAY != 0)
    );
    let printed = d.app.print_with(&setup).unwrap();
    assert_eq!(printed.timescale.map(|t| t.0), Some(start));
    // Ticking Same as screen again goes back to the screen's span, and the
    // typed text stays stored for when it is unticked.
    d.click("page_setup_same_as_screen");
    d.frames(2);
    let o = options(&d);
    assert!(o.same_as_screen);
    assert_eq!(o.timescale_start, typed);
    let setup = dialog(&d).draft.clone();
    assert_eq!(d.app.print_timescale(&setup).unwrap(), screen);
    d.click("page_setup_same_as_screen");
    d.frames(2);
    assert_eq!(options(&d).timescale_start, typed);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn an_invalid_typed_expression_shows_the_error_it_always_did() {
    let mut d = driver("ts-invalid");
    untick(&mut d);
    d.click("page_setup_ts_start");
    d.key_with(Key::A, ctrl());
    d.type_text("DD*2");
    d.frames(3);
    assert_eq!(options(&d).timescale_start, "DD*2");
    let summary = dialog(&d).summary.as_ref();
    // The result line holds the message printing gives for it.
    let message = d
        .app
        .print_timescale(&dialog(&d).draft)
        .expect_err("it does not read");
    assert!(message.starts_with("Cannot read"), "{message}");
    assert!(
        summary.is_none_or(|(_, s)| s.is_err()),
        "no summary for an unreadable expression"
    );
}

#[test]
fn the_picker_over_same_as_screen_starts_from_the_screen_date() {
    let mut d = driver("ts-from-screen");
    let screen = d.app.screen_timescale().unwrap();
    let before = options(&d).clone();
    open_picker(&mut d, "start");
    let p = picker(&d);
    assert_eq!(p.choice, 6, "a specific date");
    let shown = (p.date - NaiveDate::from_ymd_opt(1899, 12, 30).unwrap()).num_days();
    assert_eq!((shown - origin(&d)) * DAY, screen.0.div_euclid(DAY) * DAY);
    d.click("ts_picker_cancel");
    d.frames(2);
    assert_eq!(*options(&d), before);
    // Choosing a day writes it, unticks the box and keeps the screen's
    // other date.
    open_picker(&mut d, "start");
    d.click("ts_picker_day_9");
    ok(&mut d);
    let o = options(&d);
    assert!(!o.same_as_screen);
    assert_ne!(o.timescale_start, before.timescale_start);
    assert_eq!(o.timescale_finish, d.app.day_text(screen.1));
    let (start, _) = d.app.print_timescale(&dialog(&d).draft).unwrap();
    assert_eq!(date_of_minutes(&d, start).day(), 9);
}

fn date_of_minutes(d: &Driver, minutes: i64) -> NaiveDate {
    NaiveDate::from_ymd_opt(1899, 12, 30).unwrap()
        + chrono::Duration::days(origin(d) + minutes.div_euclid(DAY))
}

#[test]
fn ok_in_page_setup_keeps_the_picked_dates() {
    let mut d = driver("ts-save");
    open_picker(&mut d, "finish");
    choose(&mut d, "ts_picker_anchor", 3);
    choose(&mut d, "ts_picker_unit", 1);
    set_count(&mut d, "4");
    ok(&mut d);
    d.click("page_setup_ok");
    d.frames(3);
    assert!(d.app.print.dialog.is_none());
    let saved = d.app.page_setup();
    assert!(!saved.options.same_as_screen);
    assert_eq!(saved.options.timescale_finish, "PF+4W");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn the_dialog_formatting_check_passes_for_the_picker() {
    let mut d = Driver::probed(
        "ts-format-sweep",
        starter(),
        crate::dialog_sweep::SMALL_SCREEN,
        None,
    );
    d.frames(3);
    d.home();
    d.app.open_page_setup();
    d.frames(3);
    d.click("page_setup_tab_4");
    for (label, steps) in [("default", vec![]), ("specific", vec!["ts_picker_day_20"])] {
        open_picker(&mut d, "start");
        for step in steps {
            d.click(step);
        }
        d.record_dialogs(&format!("picker {label}"));
        d.key(Key::Escape);
        d.frames(2);
    }
    let found: Vec<_> = d.probe.findings.iter().collect();
    assert!(found.is_empty(), "{found:?}");
}

/// Renders the picker for a look: `artifacts/review/print/tspicker-*.png`
/// (ignored by git).
#[test]
#[ignore = "writes artifacts/review/print; run explicitly"]
fn render_the_timescale_picker_for_review() {
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/review/print");
    std::fs::create_dir_all(&out).unwrap();
    let mut d = Driver::probed(
        "ts-render",
        starter(),
        Vec2::new(1000., 720.),
        Some(out.join("tspicker-sweep")),
    );
    d.frames(3);
    d.home();
    d.app.open_page_setup();
    d.frames(3);
    d.click("page_setup_tab_4");
    d.record_dialogs("options same as screen");
    open_picker(&mut d, "start");
    d.record_dialogs("picker from screen");
    d.click("ts_picker_cancel");
    untick(&mut d);
    open_picker(&mut d, "finish");
    d.record_dialogs("picker relative");
    choose(&mut d, "ts_picker_anchor", 5);
    choose(&mut d, "ts_picker_sign", 1);
    set_count(&mut d, "2");
    choose(&mut d, "ts_picker_unit", 1);
    d.record_dialogs("picker offset");
    set_count(&mut d, "x");
    d.record_dialogs("picker error");
    d.click("ts_picker_day_14");
    d.record_dialogs("picker specific");
}
