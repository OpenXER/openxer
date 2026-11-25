//! Workflow: dropping the dragged bar of an activity that has not started
//! asks the same question as a typed Start (one shared path, `start_hold`).
//! Yes holds the activity at the drop with a Start On or After constraint,
//! keeping the time of day it was dropped at, and the next schedule keeps it
//! there; No moves it with no constraint, and the next schedule puts it
//! back. Each answer is one undo step.
use super::*;

const DAY: i64 = 24 * 60;

fn activity(p: &ProjectData, code: &str) -> Activity {
    p.model.activities[index_of(p, code)].clone()
}

/// Drag the body of `code`'s bar `minutes` along the time axis with real
/// pointer events (press, moves in steps, release) and return the instant
/// the drop asks about: the shifted start on the activity's calendar.
pub(super) fn drag_body(d: &mut Driver, code: &str, minutes: i64) -> i64 {
    d.home();
    d.frames(2);
    let a = activity(&d.app.project, code);
    let from = d.rect(&format!("bar_body_{}", a.task_id)).center();
    let dx = d.app.pixels_per_day * minutes as f32 / DAY as f32;
    let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let _ = d.frame(vec![egui::Event::PointerMoved(from), button(from, true)]);
    for step in 1..=6 {
        let pos = from + Vec2::new(dx * step as f32 / 6., 0.);
        let _ = d.frame(vec![egui::Event::PointerMoved(pos)]);
    }
    let to = from + Vec2::new(dx, 0.);
    let _ = d.frame(vec![button(to, false)]);
    let _ = d.frame(vec![egui::Event::PointerGone]);
    d.frames(1);
    let cal = &d.app.project.model.calendars[a.cal];
    cal.snap_start(a.early_start.unwrap() + minutes)
}

/// A shift of whole days plus hours that drops code's start in work
/// time on its calendar, so the drop keeps that time of day.
pub(super) fn shift_into_work(p: &ProjectData, code: &str, hours: i64) -> i64 {
    let a = activity(p, code);
    let cal = &p.model.calendars[a.cal];
    let s = a.early_start.unwrap();
    (1..=7)
        .map(|k| k * DAY + hours * 60)
        .find(|m| cal.snap_start(s + m) == s + m)
        .expect("a work day within a week")
}

fn asked(d: &Driver) -> bool {
    d.app.sched.start_hold.is_some() && d.has("start_hold_yes") && d.has("start_hold_no")
}

#[test]
fn workflow_dropped_bar_asks_to_hold_it_with_a_constraint() {
    let mut d = fresh("wf-bar-drag-hold");
    let start = d.app.project.clone();
    let undo = d.app.history.counts().0;
    assert!(d.app.sched.start_hold_prefs.ask, "asks on a first start");

    // RB-1050, some days and three hours later: the question, nothing moved.
    let a0 = activity(&start, "RB-1050");
    let s0 = a0.early_start.unwrap();
    assert_eq!(a0.cstr, CstrType::None);
    let shift = shift_into_work(&start, "RB-1050", 3);
    let v = drag_body(&mut d, "RB-1050", shift);
    assert!(asked(&d), "the drop asks, as a typed Start does");
    assert_eq!(d.app.project.model, start.model, "nothing changes yet");
    assert_eq!(
        v,
        s0 + shift,
        "the drop lands in work time, three hours into the day"
    );
    assert_ne!(v % DAY, s0 % DAY, "the drop keeps its own time of day");

    // Yes: held at the drop, the time of day kept, the duration kept.
    d.click("start_hold_yes");
    d.frames(2);
    d.assert_clean("drag Yes");
    let a = activity(&d.app.project, "RB-1050");
    assert_eq!((a.cstr, a.cstr_date), (CstrType::StartOnAfter, Some(v)));
    assert_eq!((a.early_start, a.target_start), (Some(v), Some(v)));
    assert_eq!(a.rem, a0.rem, "the duration stays");
    let cal = &d.app.project.model.calendars[a.cal];
    assert_eq!(
        a.early_finish,
        Some(cal.add(v, a.rem)),
        "the bar moves whole"
    );
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");

    // RB-1060 some days and two hours later: No moves it with no constraint.
    let b0 = activity(&start, "RB-1060");
    let m0 = b0.early_start.unwrap();
    // Where its logic puts it once RB-1050 is held.
    let mut planned = d.app.project.clone();
    schedule_model(&mut planned.model, &planned.resources).unwrap();
    let logic = activity(&planned, "RB-1060").early_start;
    let w = drag_body(&mut d, "RB-1060", shift_into_work(&start, "RB-1060", 2));
    assert!(asked(&d));
    assert_ne!(w, m0);
    d.click("start_hold_no");
    d.frames(2);
    d.assert_clean("drag No");
    let b = activity(&d.app.project, "RB-1060");
    assert_eq!(b.cstr, CstrType::None, "No adds no constraint");
    assert_eq!((b.early_start, b.target_start), (Some(w), Some(w)));
    assert_eq!(b.rem, b0.rem, "the duration stays");
    assert_eq!(d.app.history.counts().0, undo + 2, "one step per answer");

    // One undo takes the No drop back; redo puts it again.
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(activity(&d.app.project, "RB-1060"), b0, "undone");
    assert_eq!(activity(&d.app.project, "RB-1050"), a, "the Yes stays");
    d.click("menu_bar_edit");
    d.click("redo");
    d.frames(2);
    assert_eq!(activity(&d.app.project, "RB-1060").early_start, Some(w));

    // F9: the held activity stays at the drop, the other goes back.
    assert!(d.f9());
    let p = &d.app.project;
    let a = activity(p, "RB-1050");
    assert_eq!(a.early_start, Some(v), "held at the drop");
    assert_eq!((a.cstr, a.cstr_date), (CstrType::StartOnAfter, Some(v)));
    let b = activity(p, "RB-1060");
    assert_eq!(b.early_start, logic, "back to its logic");
    assert_eq!(b.cstr, CstrType::None);

    // The constraint survives an XER round trip.
    let reread = xer_round_trip(&mut d, "wf-bar-drag-hold");
    let r = activity(&reread, "RB-1050");
    assert_eq!((r.cstr, r.cstr_date), (CstrType::StartOnAfter, Some(v)));
}
