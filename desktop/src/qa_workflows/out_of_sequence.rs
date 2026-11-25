//! Workflow: the out-of-sequence warning of User Preferences, Assistance.
//! Off on a first start: ticking Started on an activity whose predecessor
//! is not finished asks nothing. Ticked in User Preferences (and kept in the
//! workspace): the same on another activity asks Yes / No / View, View
//! lists the unfinished predecessors, No takes the change back and Yes
//! keeps it. F9 asks nothing; the schedule log lists both activities under
//! its warnings, and View > Check Schedule > Out of Sequence Activities
//! shows them.
use super::*;
use crate::schedule_check as check;
use openxer_engine::model::{LinkType, StatusType};

/// Two activities not started, each with a finish-to-start predecessor
/// that is not finished.
fn two_behind(p: &ProjectData) -> (usize, usize) {
    let m = &p.model;
    let mut found: Vec<usize> = vec![];
    for l in &m.links {
        let (a, b) = (&m.activities[l.pred], &m.activities[l.succ]);
        if l.link_type == LinkType::FS
            && a.status != StatusType::Completed
            && b.status == StatusType::NotStarted
            && !found.contains(&l.succ)
        {
            found.push(l.succ);
        }
    }
    assert!(found.len() >= 2, "two activities behind a predecessor");
    (found[0], found[1])
}

/// Select activity `i` and tick Started on its Status tab.
fn tick_started(d: &mut Driver, i: usize) {
    show(d, "activities");
    select_row(d, i);
    d.click("detail_tab_status");
    d.click("detail_started");
    d.frames(2);
}

fn progress(p: &ProjectData, i: usize) -> (StatusType, Option<i64>, Option<i64>, i64) {
    let a = &p.model.activities[i];
    (a.status, a.act_start, a.act_finish, a.rem)
}

#[test]
fn workflow_out_of_sequence_warning_follows_the_assistance_switch() {
    let mut d = fresh("wf-out-of-sequence");
    let (first, second) = two_behind(&d.app.project);

    // A first start: the warning is off, so Started is kept without a word.
    assert!(
        !d.app.editing.options.warn_out_of_sequence,
        "off by default"
    );
    tick_started(&mut d, first);
    assert!(
        d.app.sched.sequence.is_none(),
        "no question with the switch off"
    );
    assert!(!d.has("sequence_yes"));
    d.assert_clean("started, warning off");
    assert_eq!(
        d.app.project.model.activities[first].status,
        StatusType::InProgress
    );

    // Edit > User Preferences > Assistance: tick the warning.
    d.home();
    d.click("menu_bar_edit");
    d.click("menu_user_preferences");
    d.click("prefs_tab_assistance");
    assert!(d.has("prefs_out_of_sequence"));
    d.click("prefs_out_of_sequence");
    d.click("prefs_close");
    d.frames(1);
    assert!(d.app.editing.options.warn_out_of_sequence, "ticked");
    let stored = d
        .app
        .store
        .preference(crate::editing::OPTIONS_KEY)
        .unwrap()
        .unwrap_or_default();
    assert!(
        stored.contains("\"warn_out_of_sequence\":true"),
        "kept in the workspace: {stored}"
    );

    // Started on the second activity now asks, with Yes / No / View.
    let before = progress(&d.app.project, second);
    tick_started(&mut d, second);
    let prompt = d.app.sched.sequence.clone().expect("the question");
    assert_eq!(prompt.activity, second);
    assert!(d.has("sequence_yes") && d.has("sequence_no") && d.has("sequence_view"));

    // View lists the predecessors that are not finished.
    d.click("sequence_view");
    d.frames(2);
    assert!(d.app.sched.sequence_detail);
    let m = &d.app.project.model;
    let unfinished: Vec<String> = m.activities[second]
        .preds
        .iter()
        .map(|&l| m.links[l].pred)
        .filter(|&p| m.activities[p].status != StatusType::Completed)
        .map(|p| m.activities[p].task_code.clone())
        .collect();
    assert!(!unfinished.is_empty());
    for k in 0..unfinished.len() {
        assert!(d.has(&format!("sequence_detail_rows_row_{k}")), "row {k}");
    }
    assert!(!d.has(&format!("sequence_detail_rows_row_{}", unfinished.len())));
    let row = d.rect("sequence_detail_rows_row_0");
    let shown: Vec<String> = d
        .texts()
        .into_iter()
        .filter(|(_, r)| r.intersects(row))
        .map(|(t, _)| t)
        .collect();
    assert!(
        shown.iter().any(|t| unfinished.contains(t)),
        "the predecessor is listed: {shown:?} of {unfinished:?}"
    );
    d.click("sequence_detail_close");
    d.frames(2);
    assert!(!d.app.sched.sequence_detail && d.app.sched.sequence.is_some());

    // No: the activity is as it was.
    d.click("sequence_no");
    d.frames(2);
    assert!(d.app.sched.sequence.is_none());
    assert_eq!(
        progress(&d.app.project, second),
        before,
        "No changes nothing"
    );

    // Yes: Started is kept, at the planned start.
    let planned = d.app.default_actual_start(second);
    tick_started(&mut d, second);
    assert!(d.app.sched.sequence.is_some(), "asked again");
    d.click("sequence_yes");
    d.frames(2);
    d.assert_clean("started, kept");
    let a = &d.app.project.model.activities[second];
    assert_eq!(
        (a.status, a.act_start),
        (StatusType::InProgress, Some(planned))
    );

    // F9 asks nothing; the log lists both under its warnings.
    d.f9();
    assert!(d.app.sched.sequence.is_none(), "scheduling does not ask");
    let log = d.app.log.clone();
    let warnings = log
        .iter()
        .position(|l| l == "Warnings:")
        .expect("a warnings section");
    let at = log
        .iter()
        .position(|l| l.starts_with("\tOut-of-sequence activities."))
        .expect("the out-of-sequence line");
    assert!(at > warnings, "{log:?}");
    let out = check::out_of_sequence_activities(&d.app.project.model);
    assert!(out.contains(&first) && out.contains(&second), "{out:?}");
    assert!(log[at].ends_with(&out.len().to_string()), "{}", log[at]);
    for i in [first, second] {
        let code = d.app.project.model.activities[i].task_code.clone();
        assert!(
            log[at + 1..=at + out.len()]
                .iter()
                .any(|l| l.contains(&format!("\t{code}\t"))),
            "{code} is listed: {log:?}"
        );
    }

    // View > Check Schedule > Out of Sequence Activities shows only them.
    d.home();
    d.click("menu_bar_view");
    assert!(d.hover_text("Check Schedule"), "the submenu is drawn");
    d.click("menu_check_out_of_sequence_activities");
    d.frames(2);
    let shown: std::collections::BTreeSet<usize> = d.visible_activities().into_iter().collect();
    assert_eq!(shown, out.into_iter().collect());
}
