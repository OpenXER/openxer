//! The new-project wizard: the entry rules, the project it builds, Finish
//! and Cancel through the production UI, one undo step, the sweep.
use super::*;
use crate::qa_sweep::Driver;

fn starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    crate::scheduling::schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn ctrl() -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    }
}

fn dialog() -> Dialog {
    Dialog {
        eps: 1,
        project_id: "TA-2027".into(),
        name: "Turnaround 2027".into(),
        planned_start: "04-Jan-27".into(),
        must_finish: "30-Mar-27".into(),
        manager: String::new(),
        rate_type: String::new(),
        error: None,
        ..Default::default()
    }
}

fn day_text(minutes: i64) -> String {
    openxer_xer::date_string(origin(), Some(minutes))
}

#[test]
fn a_good_entry_becomes_a_spec_with_the_dates_and_times_of_the_day() {
    let spec = dialog().check(&[], &[1]).unwrap();
    assert_eq!(spec.project_id, "TA-2027");
    assert_eq!(spec.name, "Turnaround 2027");
    assert_eq!(day_text(spec.planned_start), "2027-01-04 08:00");
    assert_eq!(day_text(spec.must_finish.unwrap()), "2027-03-30 17:00");
    // A time that is typed is kept; a blank name falls back to the ID.
    let d = Dialog {
        name: " ".into(),
        planned_start: "04-Jan-27 06:00".into(),
        must_finish: String::new(),
        ..dialog()
    };
    let spec = d.check(&[], &[1]).unwrap();
    assert_eq!(spec.name, "TA-2027");
    assert_eq!(day_text(spec.planned_start), "2027-01-04 06:00");
    assert_eq!(spec.must_finish, None);
}

#[test]
fn bad_entries_are_refused_with_a_reason() {
    let taken = ["TA-2027".to_string()];
    for (d, taken, why) in [
        (
            Dialog {
                project_id: " ".into(),
                ..dialog()
            },
            vec![],
            "Project ID",
        ),
        (dialog(), taken.to_vec(), "already exists"),
        (
            Dialog {
                project_id: "x".repeat(21),
                ..dialog()
            },
            vec![],
            "longer",
        ),
        (
            Dialog {
                project_id: "A\tB".into(),
                ..dialog()
            },
            vec![],
            "control",
        ),
        (
            Dialog {
                planned_start: String::new(),
                ..dialog()
            },
            vec![],
            "Planned Start",
        ),
        (
            Dialog {
                planned_start: "not a date".into(),
                ..dialog()
            },
            vec![],
            "not a valid date",
        ),
        (
            Dialog {
                must_finish: "01-Jan-27".into(),
                ..dialog()
            },
            vec![],
            "before the Planned Start",
        ),
        (
            Dialog {
                eps: 99,
                ..dialog()
            },
            vec![],
            "EPS",
        ),
    ] {
        let err = d.check(&taken, &[1]).unwrap_err();
        assert!(err.contains(why), "{err} should mention {why}");
    }
    // The ID check ignores letter case.
    let d = Dialog {
        project_id: "ta-2027".into(),
        ..dialog()
    };
    assert!(d.check(&taken, &[1]).is_err());
}

#[test]
fn the_built_project_is_empty_with_one_project_node_and_the_entered_dates() {
    let mut spec = dialog().check(&[], &[1]).unwrap();
    spec.manager = "7".into();
    let mut obs = openxer_xer::Table {
        name: "OBS".into(),
        fields: vec!["obs_id".into(), "obs_name".into()],
        rows: vec![],
    };
    obs.add(&[("obs_id", "7".into()), ("obs_name", "Plant Manager".into())]);
    let p = build(&spec, Some(&obs), 1).unwrap();
    assert!(p.model.activities.is_empty());
    assert!(p.model.links.is_empty());
    assert_eq!(p.model.project.name, "TA-2027");
    assert_eq!(p.model.project.plan_start, Some(spec.planned_start));
    assert_eq!(p.model.project.data_date, Some(spec.planned_start));
    assert_eq!(p.model.project.plan_end, spec.must_finish);
    assert_eq!(p.wbs.len(), 1);
    let node = views::wbs::project_node(&p).expect("a project node");
    assert_eq!(views::wbs::project_name(&p), "Turnaround 2027");
    assert_eq!(views::wbs::projwbs_value(&p, node, "obs_id"), "7");
    assert_eq!(
        views::wbs::obs_names(&p),
        vec![("7".into(), "Plant Manager".into())]
    );
    // The XER round trip keeps the dates and the node.
    let text = p.updated_document().text();
    let back = ProjectData::from_document(Document::parse(&text).unwrap(), 1, "rt".into()).unwrap();
    let shown =
        |p: &ProjectData, v: Option<i64>| openxer_xer::date_string(p.model.origin_serial, v);
    assert_eq!(
        shown(&back, back.model.project.plan_start),
        "2027-01-04 08:00"
    );
    assert_eq!(
        shown(&back, back.model.project.plan_end),
        "2027-03-30 17:00"
    );
    assert_eq!(
        shown(&back, back.model.project.data_date),
        "2027-01-04 08:00"
    );
    assert_eq!(views::wbs::project_name(&back), "Turnaround 2027");
}

#[test]
fn ctrl_n_opens_the_wizard_and_cancel_adds_nothing() {
    let mut d = Driver::new("newproj-cancel", starter());
    let projects = d.app.store.list().unwrap().len();
    let model = d.app.project.model.clone();
    d.key_with(Key::N, ctrl());
    d.frames(3);
    assert!(d.app.new_project.is_some(), "Ctrl+N opens the wizard");
    assert!(d.has("new_project_finish"));
    assert_eq!(d.app.project.model, model, "the starter is not replaced");
    d.click("new_project_cancel");
    assert!(d.app.new_project.is_none());
    assert_eq!(d.app.store.list().unwrap().len(), projects);
    assert_eq!(d.app.project.model, model);
    // File > New and the Standard bar's New open it too; Escape closes it.
    d.click("menu_new");
    assert!(d.app.new_project.is_some());
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.new_project.is_none());
    d.click("new");
    assert!(d.app.new_project.is_some());
    d.click("new_project_cancel");
    assert_eq!(d.app.store.list().unwrap().len(), projects);
}

#[test]
fn finish_adds_one_project_under_the_chosen_node_with_the_dates_and_opens_it() {
    let mut d = Driver::new("newproj-finish", starter());
    let before = d.app.store.list().unwrap().len();
    // A second EPS node to choose.
    let ran = d.app.run_workspace(
        Box::new(|ws| {
            crate::enterprise::tree::add(
                &mut ws.doc,
                &crate::enterprise::workspace::EPS,
                None,
                Some(1),
                &[],
            )?;
            Ok(crate::enterprise::Done::Changed(None))
        }),
        0.,
    );
    assert!(ran.is_ok(), "{ran:?}");
    let nodes: Vec<i64> = d.app.workspace().eps().iter().map(|n| n.id).collect();
    assert!(nodes.len() >= 2, "{nodes:?}");
    let chosen = *nodes.last().unwrap();
    d.key_with(Key::N, ctrl());
    d.frames(3);
    {
        let w = d.app.new_project.as_mut().unwrap();
        *w = Dialog {
            eps: chosen,
            ..dialog()
        };
    }
    d.click("new_project_finish");
    d.frames(3);
    assert!(d.app.new_project.is_none(), "Finish closes the wizard");
    assert_eq!(d.app.store.list().unwrap().len(), before + 1);
    // It is open, empty and dated.
    assert_eq!(d.app.project.model.project.name, "TA-2027");
    assert!(d.app.project.model.activities.is_empty());
    let shown =
        |p: &ProjectData, v: Option<i64>| openxer_xer::date_string(p.model.origin_serial, v);
    let start = d.app.project.model.project.plan_start.unwrap();
    assert_eq!(shown(&d.app.project, Some(start)), "2027-01-04 08:00");
    assert_eq!(
        shown(&d.app.project, d.app.project.model.project.plan_end),
        "2027-03-30 17:00"
    );
    assert_eq!(d.app.views.main, views::MainView::Activities);
    // It sits under the chosen node.
    let id = d.app.open_set.active.expect("a workspace row");
    assert_eq!(d.app.workspace().node_of(Some(id)), chosen);
    // Reopened from the workspace it is the same project.
    let again = d.app.store.load(id).unwrap();
    assert_eq!(again.model.project.name, "TA-2027");
    assert_eq!(
        shown(&again, again.model.project.plan_start),
        "2027-01-04 08:00"
    );
}

#[test]
fn a_refused_finish_keeps_the_wizard_open_and_says_why() {
    let mut d = Driver::new("newproj-refuse", starter());
    let before = d.app.store.list().unwrap().len();
    d.key_with(Key::N, ctrl());
    d.frames(3);
    *d.app.new_project.as_mut().unwrap() = Dialog {
        must_finish: "01-Jan-20".into(),
        ..dialog()
    };
    d.click("new_project_finish");
    d.frames(2);
    let w = d.app.new_project.as_ref().expect("still open");
    assert!(w
        .error
        .as_ref()
        .unwrap()
        .contains("before the Planned Start"));
    assert_eq!(d.app.store.list().unwrap().len(), before);
}

#[test]
fn the_add_is_one_undo_step_and_undo_removes_the_project() {
    let mut d = Driver::new("newproj-undo", starter());
    let before = d.app.store.list().unwrap().len();
    let steps = d.app.history.steps();
    let spec = dialog().check(&d.app.taken_project_ids(), &[1]).unwrap();
    let id = d.app.create_wizard_project(&spec, 1.).unwrap();
    assert_eq!(d.app.store.list().unwrap().len(), before + 1);
    assert_eq!(d.app.history.steps(), steps + 1, "one step");
    d.app.history_move(&d.ctx.clone(), false);
    assert_eq!(d.app.store.list().unwrap().len(), before);
    assert!(d.app.store.load(id).is_err(), "the project is gone");
    d.app.history_move(&d.ctx.clone(), true);
    assert_eq!(d.app.store.list().unwrap().len(), before + 1);
    assert_eq!(d.app.store.load(id).unwrap().model.project.name, "TA-2027");
}

#[test]
fn the_wizard_offers_the_open_projects_managers_and_default_id() {
    let mut p = starter();
    p.document
        .table_mut("OBS")
        .add(&[("obs_id", "3".into()), ("obs_name", "Site Lead".into())]);
    let mut d = Driver::new("newproj-managers", p);
    d.key_with(Key::N, ctrl());
    d.frames(3);
    assert!(d.has("new_project_manager"));
    let w = d.app.new_project.as_ref().unwrap();
    assert!(w.project_id.starts_with("NEW-"));
    assert!(!w.planned_start.is_empty());
}
