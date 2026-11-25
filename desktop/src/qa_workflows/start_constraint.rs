//! Workflow: a Start typed for an activity that has not started asks
//! whether to hold it there with a Start On or After constraint. Yes moves
//! Start and Planned Start with the constraint (the cell shows the `*`
//! mark and the date survives F9); No moves them without one (F9 puts the
//! activity back); Escape changes nothing; "Always use this answer" stops
//! the question until User Preferences brings it back. The Status tab's
//! Start asks the same question. A typed Finish never asks: the start stays
//! and the durations fit. Each answer is one undo step.
use super::*;

const DAY: i64 = 24 * 60;

fn activity(p: &ProjectData, code: &str) -> Activity {
    p.model.activities[index_of(p, code)].clone()
}

fn original(p: &ProjectData, code: &str) -> i64 {
    p.details[&activity(p, code).task_id].original_duration
}

/// A date a week after `minute`, as the user's date format shows it.
fn week_later(d: &Driver, minute: i64) -> (i64, String) {
    let v = minute + 7 * DAY;
    (v, format::date(d.app.project.model.origin_serial, Some(v)))
}

/// Type `text` into the Start (or Finish) cell of `code`.
fn type_cell(d: &mut Driver, code: &str, column: &str, text: &str) {
    d.home();
    let i = index_of(&d.app.project, code);
    cell_edit(d, i, column, text);
    d.frames(1);
}

fn asked(d: &Driver) -> bool {
    d.app.sched.start_hold.is_some() && d.has("start_hold_yes") && d.has("start_hold_no")
}

/// The finish a start of `v` gives `code` by its remaining duration.
fn finish_from(p: &ProjectData, code: &str, v: i64) -> i64 {
    let a = activity(p, code);
    let cal = &p.model.calendars[a.cal];
    let f = a.early_finish.unwrap();
    assert_eq!(cal.between(v, f), a.rem, "{code}: finish by the duration");
    f
}

#[test]
fn workflow_typed_start_asks_to_hold_it_with_a_constraint() {
    let mut d = fresh("wf-start-hold");
    let start = d.app.project.clone();
    let undo = d.app.history.counts().0;
    assert!(d.app.sched.start_hold_prefs.ask, "asks on a first start");

    // Yes: RB-1050 moves a week later and is held there.
    let s0 = activity(&start, "RB-1050").early_start.unwrap();
    let (v, text) = week_later(&d, s0);
    type_cell(&mut d, "RB-1050", "start", &text);
    assert!(asked(&d), "a typed Start asks first");
    assert_eq!(d.app.project.model, start.model, "nothing changes yet");
    d.click("start_hold_yes");
    d.frames(2);
    d.assert_clean("Yes");
    assert!(d.app.sched.start_hold.is_none());
    let p = &d.app.project;
    let a = activity(p, "RB-1050");
    assert_eq!(a.cstr, CstrType::StartOnAfter, "Start On or After");
    assert_eq!(a.cstr_date, Some(v), "on the typed date");
    assert_eq!((a.early_start, a.target_start), (Some(v), Some(v)));
    finish_from(p, "RB-1050", v);
    assert_eq!(original(p, "RB-1050"), original(&start, "RB-1050"));
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");
    let shown = drawn_cells(&mut d, index_of(&start, "RB-1050"), &["start", "finish"]);
    assert!(
        shown[0].ends_with('*'),
        "the Start cell is marked: {shown:?}"
    );

    // One undo takes it all back.
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(d.app.project.model, start.model, "undone");

    // Escape: the typed date is dropped.
    type_cell(&mut d, "RB-1050", "start", &text);
    assert!(asked(&d));
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.sched.start_hold.is_none(), "Escape closes it");
    assert_eq!(d.app.project.model, start.model, "Escape changes nothing");
    assert_eq!(d.app.history.counts().0, undo);

    // Yes again, then No on RB-1060: its dates move with no constraint.
    type_cell(&mut d, "RB-1050", "start", &text);
    d.click("start_hold_yes");
    d.frames(2);
    let m0 = activity(&start, "RB-1060").early_start.unwrap();
    let (w, text) = week_later(&d, m0);
    type_cell(&mut d, "RB-1060", "start", &text);
    assert!(asked(&d));
    d.click("start_hold_no");
    d.frames(2);
    d.assert_clean("No");
    let p = &d.app.project;
    let b = activity(p, "RB-1060");
    assert_eq!(b.cstr, CstrType::None, "No adds no constraint");
    assert_eq!((b.early_start, b.target_start), (Some(w), Some(w)));
    finish_from(p, "RB-1060", w);
    assert_eq!(d.app.history.counts().0, undo + 2, "one step per answer");

    // F9: the held activity stays, the other goes back to its logic.
    assert!(d.f9());
    let p = &d.app.project;
    assert_eq!(activity(p, "RB-1050").early_start, Some(v), "held");
    assert_eq!(activity(p, "RB-1060").early_start, Some(m0), "back");

    // A typed Finish never asks: the start stays, both durations fit.
    let before = d.app.project.clone();
    let c = activity(&before, "RB-1070");
    let (f, text) = week_later(&d, c.early_finish.unwrap());
    type_cell(&mut d, "RB-1070", "finish", &text);
    assert!(!asked(&d), "no question for a Finish");
    let p = &d.app.project;
    let c2 = activity(p, "RB-1070");
    assert_eq!(c2.cstr, CstrType::None, "no constraint");
    assert_eq!(c2.early_start, c.early_start, "the start stays");
    assert_eq!(c2.early_finish, Some(f), "the typed finish");
    let five_days = 5 * 8 * 60;
    assert_eq!(c2.rem, c.rem + five_days, "Remaining Duration fits");
    assert_eq!(
        original(p, "RB-1070"),
        original(&before, "RB-1070") + five_days
    );

    // "Always use this answer" with No: the next typed Start is not asked.
    let e0 = activity(&before, "RB-1080").early_start.unwrap();
    let (x, text) = week_later(&d, e0);
    type_cell(&mut d, "RB-1080", "start", &text);
    assert!(asked(&d));
    d.click("start_hold_keep");
    d.click("start_hold_no");
    d.frames(2);
    assert!(!d.app.sched.start_hold_prefs.ask, "kept");
    assert!(!d.app.sched.start_hold_prefs.hold);
    let g0 = activity(&before, "RB-1090").early_start.unwrap();
    let (y, text) = week_later(&d, g0);
    type_cell(&mut d, "RB-1090", "start", &text);
    assert!(!asked(&d), "the kept answer is used");
    let p = &d.app.project;
    assert_eq!(activity(p, "RB-1080").early_start, Some(x));
    let g = activity(p, "RB-1090");
    assert_eq!((g.early_start, g.cstr), (Some(y), CstrType::None));

    // User Preferences brings the question back.
    d.home();
    d.click("menu_bar_edit");
    d.click("menu_user_preferences");
    d.click("prefs_tab_application");
    d.click("prefs_ask_start_hold");
    d.click("prefs_close");
    d.frames(1);
    assert!(d.app.sched.start_hold_prefs.ask, "asks again");

    // The Status tab's Start asks the same question.
    let i = index_of(&d.app.project, "RB-1110");
    d.home();
    select_row(&mut d, i);
    if !d.has("detail_tab_status") {
        d.app.set_bottom(Some(panes::BottomView::Details));
        d.frames(2);
    }
    d.click("detail_tab_status");
    d.frames(2);
    let h0 = activity(&d.app.project, "RB-1110").early_start.unwrap();
    let (z, text) = week_later(&d, h0);
    let undo_now = d.app.history.counts().0;
    type_into(&mut d, &format!("detail_date_{i}-act_start"), &text);
    d.key(Key::Enter);
    d.frames(2);
    assert!(asked(&d), "the Status tab asks too");
    d.click("start_hold_yes");
    d.frames(2);
    d.assert_clean("Status tab Yes");
    let h = activity(&d.app.project, "RB-1110");
    assert_eq!((h.cstr, h.cstr_date), (CstrType::StartOnAfter, Some(z)));
    assert_eq!((h.early_start, h.target_start), (Some(z), Some(z)));
    assert_eq!(d.app.history.counts().0, undo_now + 1);

    // The constraint survives an XER round trip.
    let reread = xer_round_trip(&mut d, "wf-start-hold");
    let r = activity(&reread, "RB-1110");
    assert_eq!((r.cstr, r.cstr_date), (CstrType::StartOnAfter, Some(z)));
}
