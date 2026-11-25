//! Workflow: the default button of each confirmation (what Enter presses)
//! follows the observed reference behaviour, from the one table in
//! `kit_window::Question`. Edit > Dissolve answers Yes on Enter and
//! dissolves, one undo step; the activity delete question answers No on
//! Enter and deletes nothing.
use super::*;
use crate::kit_window::Question;
use openxer_engine::model::{Link, LinkType};

/// The scheduled sample project with RB-1000 FS RB-1020 FS RB-1030 only,
/// so RB-1020 can be dissolved.
fn project() -> ProjectData {
    let mut p = openxer_xer::starter();
    let at = |code: &str| index_of(&p, code);
    let links = [("RB-1000", "RB-1020"), ("RB-1020", "RB-1030")].map(|(a, b)| Link {
        pred: at(a),
        succ: at(b),
        link_type: LinkType::FS,
        lag: 0,
        driving_path: false,
    });
    p.model.links = links.to_vec();
    p.model.rebuild_links().unwrap();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

/// The default caption of the dialog on top.
fn default_label(d: &Driver) -> Option<String> {
    crate::dialog_keys::top_default_label(&d.ctx)
}

/// The Delete or Merge question for a WBS element of the sample project
/// (not the project node), said to hold three activities (the box only
/// shows the counts).
fn wbs_with_activities(p: &ProjectData) -> views::wbs::PendingDelete {
    let project = views::wbs::project_node(p);
    let w = p
        .wbs
        .iter()
        .find(|w| Some(w.id) != project)
        .expect("a WBS element");
    views::wbs::PendingDelete {
        id: w.id,
        activities: 3,
        children: 0,
        delete_all: false,
    }
}

/// The observed defaults for every question OpenXER asks that the reference
/// observations cover; the rest keep the default they had.
#[test]
fn every_question_takes_the_observed_default() {
    let expected = [
        (Question::DeleteActivities, Some("No")),
        (Question::Dissolve, Some("Yes")),
        (Question::DeleteBaseline, Some("Yes")),
        (Question::RestoreBaseline, Some("Yes")),
        (Question::RevertToBaseline, Some("No")),
        (Question::DeleteCalendar, Some("No")),
        (Question::DiscardCalendarChanges, Some("No")),
        (Question::DeleteRecords, Some("No")),
        (Question::DeleteProject, Some("No")),
        (Question::SaveLayoutChanges, Some("Yes")),
        (Question::FutureActuals, Some("Yes")),
        (Question::OutOfSequence, Some("Yes")),
        (Question::WbsMergeOrDelete, Some("Delete All")),
        (Question::WbsDelete, Some("Delete")),
        (Question::UndoImport, Some("Cancel")),
        (Question::ReplaceCopy, Some("Open Copy")),
        (Question::RecoverChanges, Some("Recover")),
        (Question::SaveFailed, Some("Try Again")),
        (Question::OpenDocument, None),
        (Question::SaveUpdateBaselineSettings, Some("Yes")),
        (Question::StartHold, Some("Yes")),
        (Question::GlobalChangeDeleteLine, Some("Yes")),
        (Question::RemoveRelationship, Some("Yes")),
        (Question::DeleteLayout, Some("Yes")),
        (Question::DeleteFilter, Some("Yes")),
        (Question::RemoveAssignment, Some("No")),
        (Question::RemovePrimaryResource, Some("No")),
        (Question::RemoveCode, Some("Yes")),
        // Not observed for a project code: as for an activity code.
        (Question::RemoveProjectCode, Some("Yes")),
        (Question::ConflictingScheduleOptions, Some("Yes")),
        (Question::SaveScheduleOptions, Some("Yes")),
        // Not observed: No.
        (Question::RemoveManualCurve, Some("No")),
        (Question::ResetScheduleOptions, Some("Yes")),
        (Question::ResolveDuplicates, Some("Yes")),
        (Question::RelinkCosts, Some("No")),
        (Question::DeleteResourceAssignment, Some("No")),
        (Question::DeleteRelationship, Some("Yes")),
        // Chat's own questions: what does not send anything.
        (Question::ChatSignIn, Some("Cancel")),
        (Question::ChatSendToService, Some("Cancel")),
        (Question::ChatPlanWelcome, Some("Got it")),
    ];
    for (q, caption) in expected {
        assert_eq!(q.default_caption(), caption, "{q:?}");
    }
    // The two whose buttons vary answer with the last one (No, or Cancel).
    for q in [Question::EnterpriseDelete, Question::DictionaryDelete] {
        assert_eq!(q.default_in(&["Yes", "No"]), Some(1), "{q:?}");
        assert_eq!(q.default_in(&["Yes", "No", "Cancel"]), Some(2), "{q:?}");
    }
    assert_eq!(expected.len() + 2, Question::ALL.len(), "every question");
}

/// Each box whose default changed, opened in the app: the button framed as
/// the default (the one Enter presses) is the observed one.
#[test]
fn the_changed_boxes_frame_the_observed_default() {
    type Open = fn(&mut Driver);
    let boxes: [(&str, Open, &str); 7] = [
        (
            "Dissolve",
            |d| {
                let i = index_of(&d.app.project, "RB-1020");
                d.app.editing.dialogs.dissolve =
                    Some(vec![d.app.project.model.activities[i].task_id]);
            },
            "Yes",
        ),
        (
            "Delete baseline",
            |d| d.app.baselines.confirm = Some(crate::baselines::dialogs::Confirm::Delete(1)),
            "Yes",
        ),
        (
            "Restore baseline",
            |d| d.app.baselines.confirm = Some(crate::baselines::dialogs::Confirm::Restore(1)),
            "Yes",
        ),
        (
            "Apply Actuals to a future data date",
            |d| {
                d.app.open_apply_actuals();
                d.app.sched.progress.confirm_future = Some("04-Jan-45".into());
            },
            "Yes",
        ),
        (
            "WBS Delete or Merge, first question",
            |d| {
                show(d, "wbs");
                d.app.views.wbs.pending = Some(wbs_with_activities(&d.app.project));
            },
            "Delete All",
        ),
        (
            "WBS delete, second question",
            |d| {
                show(d, "wbs");
                let mut plan = wbs_with_activities(&d.app.project);
                plan.delete_all = true;
                d.app.views.wbs.pending = Some(plan);
            },
            "Delete",
        ),
        (
            "WBS delete of an empty element",
            |d| {
                show(d, "wbs");
                let mut plan = wbs_with_activities(&d.app.project);
                plan.activities = 0;
                plan.children = 0;
                d.app.views.wbs.pending = Some(plan);
            },
            "Delete",
        ),
    ];
    for (name, open, caption) in boxes {
        let mut d = Driver::new("wf-confirm-defaults", project());
        open(&mut d);
        d.frames(3);
        assert_eq!(default_label(&d).as_deref(), Some(caption), "{name}");
    }
}

#[test]
fn workflow_enter_dissolves_and_does_not_delete() {
    let mut d = Driver::new("wf-confirm-enter", project());
    let start = d.app.project.clone();
    let undo = d.app.history.counts().0;

    // Edit > Dissolve on RB-1020: Enter answers Yes and dissolves it.
    d.home();
    let i = index_of(&d.app.project, "RB-1020");
    select_row(&mut d, i);
    d.click("menu_bar_edit");
    d.click("edit_dissolve");
    d.frames(2);
    assert!(d.app.editing.dialogs.dissolve.is_some(), "Dissolve asks");
    assert_eq!(default_label(&d).as_deref(), Some("Yes"));
    d.key(Key::Enter);
    d.frames(2);
    d.assert_clean("dissolve by Enter");
    assert!(d.app.editing.dialogs.dissolve.is_none(), "Enter answers");
    let p = &d.app.project;
    assert!(
        p.model.activities.iter().all(|a| a.task_code != "RB-1020"),
        "Enter dissolved RB-1020"
    );
    let rejoined = p.model.links.iter().any(|l| {
        p.model.activities[l.pred].task_code == "RB-1000"
            && p.model.activities[l.succ].task_code == "RB-1030"
    });
    assert!(rejoined, "its predecessor is linked to its successor");
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");

    // One undo brings it back.
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(d.app.project.model, start.model, "undo restores it");
    assert_eq!(d.app.history.counts().0, undo);

    // Edit > Delete on RB-1030: Enter answers No and deletes nothing.
    d.home();
    let i = index_of(&d.app.project, "RB-1030");
    select_row(&mut d, i);
    d.key(Key::Delete);
    d.frames(2);
    assert!(d.app.delete_confirm.is_some(), "Delete asks first");
    assert_eq!(default_label(&d).as_deref(), Some("No"));
    d.key(Key::Enter);
    d.frames(2);
    assert!(d.app.delete_confirm.is_none(), "Enter answers");
    assert_eq!(d.app.project.model, start.model, "nothing deleted");
    assert_eq!(d.app.history.counts().0, undo, "no undo step");
}
