//! Workflow: financial periods through the real menus. Admin > Financial
//! Period Calendars makes weekly periods around the data date, actual units
//! go in on the Status tab, Tools > Save Period Actuals stores them (into
//! the last period that ended before the data date), then Delete Period is
//! refused at once for that period (an OK-only message, nothing asked,
//! nothing deleted) while an empty period asks Yes / No, goes on Yes and
//! comes back with one undo.
use super::*;
use crate::enterprise::{ops, Dialog};

fn open_fin_calendars(d: &mut Driver) {
    d.home();
    d.click(&format!(
        "menu_bar_{}",
        Dialog::FinCalendars.menu().to_lowercase()
    ));
    d.click(Dialog::FinCalendars.action());
    d.frames(2);
    assert_eq!(d.app.enterprise.ui.open, Some(Dialog::FinCalendars));
}

/// The calendar's periods as (name, start, end).
fn periods(d: &Driver, cal: i64) -> Vec<ops::Period> {
    ops::periods(&d.app.project.document, cal)
}

/// Stored performance rows (activities) per period ID.
fn stored_in(d: &Driver, period: i64) -> usize {
    d.app.project.document.table("TASKFIN").map_or(0, |t| {
        t.rows
            .iter()
            .filter(|r| crate::records::num(t.value(r, "fin_dates_id")) == period)
            .count()
    })
}

/// Select period `id` in the dialog's period list and press Delete Period.
fn delete_period(d: &mut Driver, cal: i64, id: i64) {
    let k = periods(d, cal)
        .iter()
        .position(|p| p.id == id)
        .expect("the period is listed");
    d.click(&format!("fin_periods_row_{k}"));
    d.frames(1);
    assert_eq!(d.app.enterprise.ui.sub, Some(id), "the period is selected");
    if d.app.enterprise.ui.sub_edit.is_some() {
        d.key(Key::Escape);
        d.frames(1);
    }
    d.click("fin_calendars_delete_period");
    d.frames(2);
}

#[test]
fn workflow_a_period_holding_stored_actuals_cannot_be_deleted() {
    let mut d = fresh("wf-period-delete");
    let data_date = d.app.project.model.project.data_date.unwrap()
        + d.app.project.model.origin_serial * openxer_engine::calendar::DAY;
    let day = ops::day_of_minutes(data_date);

    // Admin > Financial Period Calendars: Add, then weekly periods from two
    // weeks before the data date to two weeks after it.
    open_fin_calendars(&mut d);
    d.click("fin_calendars_add");
    d.frames(2);
    let cal = ops::default_fin_calendar(&d.app.project.document).expect("a calendar");
    d.click("fin_batch_weekly");
    type_into(
        &mut d,
        "fin_batch_from",
        &crate::format::day(day - chrono::Duration::days(14)),
    );
    type_into(
        &mut d,
        "fin_batch_to",
        &crate::format::day(day + chrono::Duration::days(14)),
    );
    d.click("fin_batch_create");
    d.frames(2);
    d.assert_clean("batch create");
    let all = periods(&d, cal);
    assert!(all.len() >= 4, "weekly periods: {all:?}");
    let closed = all
        .iter()
        .rev()
        .find(|p| p.end < day)
        .map(|p| p.id)
        .expect("a period ended before the data date");
    let holding = ops::period_containing(&d.app.project.document, cal, day)
        .expect("a period holds the data date");
    let last = all.last().unwrap().id;
    assert!(closed != holding && last != holding);
    d.click("fin_calendars_close");
    d.frames(2);

    // Actual units on the Status tab of an activity with no predecessors,
    // started first if it is not yet.
    show(&mut d, "activities");
    let m = &d.app.project.model;
    let i = (0..m.activities.len())
        .find(|&i| {
            m.activities[i].preds.is_empty()
                && m.activities[i].status != openxer_engine::model::StatusType::Completed
        })
        .expect("an open activity without predecessors");
    select_row(&mut d, i);
    d.click("detail_tab_status");
    if d.app.project.model.activities[i].act_start.is_none() {
        d.click("detail_started");
        d.frames(2);
    }
    assert!(d.app.project.model.activities[i].act_start.is_some());
    type_into(&mut d, "detail_units_actual", "40");
    d.key(Key::Enter);
    d.frames(2);
    d.assert_clean("actual units");

    // Tools > Save Period Actuals, Store Now: into the period that ended
    // before the data date, not the one that holds it.
    d.home();
    d.click("menu_bar_tools");
    d.click("menu_store_period");
    d.frames(2);
    assert_eq!(
        d.app.enterprise.store_period,
        Some(closed),
        "the period offered is the last one that ended before the data date"
    );
    d.click("store_now");
    d.frames(2);
    d.assert_clean("store period");
    assert!(stored_in(&d, closed) > 0, "the actuals are stored there");
    assert_eq!(stored_in(&d, holding), 0);

    // Delete Period on the period holding stored values: an OK-only message
    // at once, no question, and the period stays.
    open_fin_calendars(&mut d);
    let undo = d.app.history.counts().0;
    delete_period(&mut d, cal, closed);
    assert!(
        d.app.enterprise.ui.confirm.is_none(),
        "nothing is asked first"
    );
    let notice = d.app.enterprise.ui.notice.clone().expect("a message");
    assert!(notice.contains("stored values"), "{notice}");
    assert!(d.has("enterprise_msg_ok") && !d.has("enterprise_msg_yes"));
    d.click("enterprise_msg_ok");
    d.frames(2);
    assert!(d.app.enterprise.ui.notice.is_none());
    assert!(
        periods(&d, cal).iter().any(|p| p.id == closed),
        "still there"
    );
    assert!(stored_in(&d, closed) > 0, "its stored values too");
    assert_eq!(d.app.history.counts().0, undo, "nothing changed");

    // An empty period: Yes / No, Enter answers No and keeps it.
    delete_period(&mut d, cal, last);
    assert!(d.app.enterprise.ui.notice.is_none());
    assert!(d.has("enterprise_msg_yes") && d.has("enterprise_msg_no"));
    d.key(Key::Enter);
    d.frames(2);
    assert!(d.app.enterprise.ui.confirm.is_none());
    assert!(periods(&d, cal).iter().any(|p| p.id == last), "No keeps it");
    assert_eq!(d.app.history.counts().0, undo);
    // Yes deletes it as one undo step.
    delete_period(&mut d, cal, last);
    d.click("enterprise_msg_yes");
    d.frames(2);
    d.assert_clean("delete period");
    assert!(!periods(&d, cal).iter().any(|p| p.id == last), "gone");
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");
    d.click("fin_calendars_close");
    d.frames(2);

    // Edit > Undo brings it back.
    d.home();
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert!(
        periods(&d, cal).iter().any(|p| p.id == last),
        "undo restores the period"
    );
    assert_eq!(d.app.history.counts().0, undo);
}
