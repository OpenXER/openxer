//! Workflow: Edit > Dissolve through the menu and its confirmation, with
//! Retain Lag off and then on (User Preferences, Calculations), the
//! disabled state for an activity with only a predecessor, and one undo.
use super::*;
use openxer_engine::model::{Link, LinkType};

const DAY: i64 = 8 * 60;

/// The scheduled sample project with only these relationships:
/// RB-1010 SS RB-1020 lag 2d and RB-1000 FS RB-1020 (two predecessors of
/// RB-1020), RB-1020 FF RB-1030 lag 3d (its successor), RB-1000 FS RB-1030
/// lag 1d (already there, must stay as it is), and RB-1040 FS RB-1050
/// (RB-1050 has a predecessor only).
fn project() -> ProjectData {
    let mut p = openxer_xer::starter();
    let at = |code: &str| index_of(&p, code);
    let links = [
        ("RB-1010", "RB-1020", LinkType::SS, 2 * DAY),
        ("RB-1000", "RB-1020", LinkType::FS, 0),
        ("RB-1020", "RB-1030", LinkType::FF, 3 * DAY),
        ("RB-1000", "RB-1030", LinkType::FS, DAY),
        ("RB-1040", "RB-1050", LinkType::FS, 0),
    ]
    .map(|(a, b, link_type, lag)| Link {
        pred: at(a),
        succ: at(b),
        link_type,
        lag,
        driving_path: false,
    });
    p.model.links = links.to_vec();
    p.model.rebuild_links().unwrap();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

/// Every relationship by Activity ID, sorted.
fn links(p: &ProjectData) -> Vec<(String, String, LinkType, i64)> {
    let code = |i: usize| p.model.activities[i].task_code.clone();
    let mut all: Vec<_> = p
        .model
        .links
        .iter()
        .map(|l| (code(l.pred), code(l.succ), l.link_type, l.lag))
        .collect();
    all.sort_by(|x, y| (&x.0, &x.1).cmp(&(&y.0, &y.1)));
    all
}

/// An activity's ID with its early start, early finish and late finish.
type Dates = (String, Option<i64>, Option<i64>, Option<i64>);

/// Every activity's dates, by Activity ID.
fn dates(p: &ProjectData) -> Vec<Dates> {
    p.model
        .activities
        .iter()
        .map(|a| {
            (
                a.task_code.clone(),
                a.early_start,
                a.early_finish,
                a.late_finish,
            )
        })
        .collect()
}

/// Select `code` and choose Edit > Dissolve.
fn edit_dissolve(d: &mut Driver, code: &str) {
    d.home();
    let i = index_of(&d.app.project, code);
    select_row(d, i);
    d.click("menu_bar_edit");
    d.click("edit_dissolve");
    d.frames(2);
}

/// User Preferences > Calculations > Retain Lag, ticked or cleared.
fn set_retain_lag(d: &mut Driver, on: bool) {
    d.home();
    d.click("menu_bar_edit");
    d.click("menu_user_preferences");
    d.click("prefs_tab_calculations");
    if d.app.editing.options.retain_lag != on {
        d.click("prefs_retain_lag");
    }
    d.click("prefs_close");
    d.frames(1);
    assert_eq!(d.app.editing.options.retain_lag, on, "Retain Lag {on}");
}

fn s(x: &str) -> String {
    x.to_string()
}

#[test]
fn workflow_dissolve_rejoins_logic_with_and_without_retain_lag() {
    let mut d = Driver::new("wf-dissolve", project());
    assert!(
        !d.app.editing.options.retain_lag,
        "Retain Lag is off on a first start"
    );
    let start = d.app.project.clone();
    let undo = d.app.history.counts().0;

    // RB-1050 has a predecessor but no successor: Dissolve is disabled, so
    // the click runs nothing.
    edit_dissolve(&mut d, "RB-1050");
    assert!(d.app.editing.dialogs.dissolve.is_none(), "disabled");
    d.key(Key::Escape);
    d.frames(1);
    assert_eq!(d.app.project.model, start.model);

    // RB-1020: the confirmation; No changes nothing.
    edit_dissolve(&mut d, "RB-1020");
    assert!(d.app.editing.dialogs.dissolve.is_some(), "asks first");
    assert!(d.has("dissolve_yes") && d.has("dissolve_no"));
    d.click("dissolve_no");
    d.frames(2);
    assert!(d.app.editing.dialogs.dissolve.is_none());
    assert_eq!(d.app.project.model, start.model, "No changes nothing");
    assert_eq!(d.app.history.counts().0, undo);

    // Yes, Retain Lag off: SS from RB-1010 and FF to RB-1030 give SF with
    // no lag; RB-1000 to RB-1030 was already related and keeps FS 1d.
    edit_dissolve(&mut d, "RB-1020");
    d.click("dissolve_yes");
    d.frames(2);
    d.assert_clean("dissolve");
    let p = &d.app.project;
    assert!(p.model.activities.iter().all(|a| a.task_code != "RB-1020"));
    assert_eq!(
        links(p),
        vec![
            (s("RB-1000"), s("RB-1030"), LinkType::FS, DAY),
            (s("RB-1010"), s("RB-1030"), LinkType::SF, 0),
            (s("RB-1040"), s("RB-1050"), LinkType::FS, 0),
        ]
    );
    let mut kept = dates(&start);
    kept.retain(|(code, ..)| code != "RB-1020");
    assert_eq!(dates(p), kept, "no reschedule");
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");

    // One undo brings the activity and all its relationships back.
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(d.app.project.model, start.model, "undo restores it all");
    assert_eq!(d.app.history.counts().0, undo);

    // Retain Lag on: the new relationship gets 2d + 3d, not B's duration.
    set_retain_lag(&mut d, true);
    edit_dissolve(&mut d, "RB-1020");
    d.click("dissolve_yes");
    d.frames(2);
    d.assert_clean("dissolve with Retain Lag");
    assert_eq!(
        links(&d.app.project),
        vec![
            (s("RB-1000"), s("RB-1030"), LinkType::FS, DAY),
            (s("RB-1010"), s("RB-1030"), LinkType::SF, 5 * DAY),
            (s("RB-1040"), s("RB-1050"), LinkType::FS, 0),
        ]
    );
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");
    // The relationships survive an XER round trip.
    let reread = xer_round_trip(&mut d, "wf-dissolve");
    assert_eq!(links(&reread), links(&d.app.project));
}
