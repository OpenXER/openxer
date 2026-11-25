use super::*;
use crate::qa_sweep::Driver;
use crate::records::num;

fn steps_of(p: &ProjectData, task_id: i64) -> Vec<String> {
    let pid = p.model.project.proj_id;
    let mut rows = records::rows_for(&p.document, "TASKPROC", task_id, pid);
    rows.sort_by_key(|&r| num(&records::get(&p.document, "TASKPROC", r, "seq_num")));
    rows.iter()
        .map(|&r| records::get(&p.document, "TASKPROC", r, "proc_name"))
        .collect()
}

#[test]
fn templates_round_trip_through_xer_and_apply_to_activities() {
    let mut p = crate::reports::test_support::scheduled_starter();
    let doc = &mut p.document;
    let t = add_template(doc, Some("Pipe spool")).unwrap();
    assert!(
        add_template(doc, Some("pipe SPOOL")).is_err(),
        "names are unique"
    );
    assert!(add_template(doc, Some("  ")).is_err());
    let fit = add_template_step(doc, t, "Fit up", 2., "");
    add_template_step(doc, t, "Weld out", 3., "<p>Root and cap</p>");
    set_template_step(doc, fit, StepField::Name, "Fit-up").unwrap();
    assert!(set_template_step(doc, fit, StepField::Weight, "-1").is_err());
    let other = add_template(doc, None).unwrap();
    assert_eq!(templates(doc)[1].name, "New Template");
    rename_template(doc, other, "Hydrotest").unwrap();
    // Export, reimport, save and reload.
    let text = p.updated_document().text();
    assert!(text.contains("%T\tPROCGROUP") && text.contains("%T\tPROCITEM"));
    let pid = p.model.project.proj_id;
    let q = ProjectData::from_document(Document::parse(&text).unwrap(), pid, "x".into()).unwrap();
    assert_eq!(templates(&q.document), templates(&p.document));
    assert_eq!(
        template_steps(&q.document, t),
        template_steps(&p.document, t)
    );
    let dir = crate::reports::test_support::temp("step-templates");
    let mut store = Store::open(dir.join("w.db")).unwrap();
    let id = store.save(&p).unwrap();
    assert_eq!(
        templates(&store.load(id).unwrap().document),
        templates(&p.document)
    );
    // Apply adds the steps after the activity's own, not complete.
    let task = p.model.activities[0].task_id;
    let before = steps_of(&p, task);
    assert_eq!(apply_template(&mut p.document, t, task, pid), 2);
    let after = steps_of(&p, task);
    assert_eq!(after.len(), before.len() + 2);
    assert_eq!(
        after[before.len()..],
        ["Fit-up".to_string(), "Weld out".into()]
    );
    // A template from the activity's steps, under a free name.
    let made = template_from_activity(&mut p.document, "Pipe spool", task, pid).unwrap();
    let made = templates(&p.document)
        .into_iter()
        .find(|x| x.id == made)
        .unwrap();
    assert_eq!(made.name, "Pipe spool 2");
    assert_eq!(made.steps, after.len());
    delete_template(&mut p.document, t);
    assert!(template_steps(&p.document, t).is_empty());
    assert_eq!(templates(&p.document).len(), 2);
}

#[test]
fn step_templates_dialog_and_add_from_template_on_the_steps_tab() {
    let mut d = Driver::new(
        "step-templates",
        crate::reports::test_support::scheduled_starter(),
    );
    let undo = d.app.history.counts().0;
    d.click("menu_bar_enterprise");
    d.click("menu_step_templates");
    assert!(d.app.dictionaries.steps.is_some());
    d.click("step_templates_add");
    d.click("step_templates_add_step");
    d.click("step_templates_add_step");
    let t = templates(&d.app.project.document);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].steps, 2);
    assert_eq!(
        d.app.history.counts().0,
        undo + 3,
        "each edit is one undo step"
    );
    d.click("step_templates_close");
    assert!(d.app.dictionaries.steps.is_none());
    // The toolbar button opens it too; Escape closes it.
    d.click("tool_step_templates");
    assert!(d.app.dictionaries.steps.is_some());
    d.key(Key::Escape);
    assert!(d.app.dictionaries.steps.is_none());
    // Steps tab: Add from Template, Cancel first.
    d.app.selected = Some(0);
    d.app.show_detail_tab(DetailTab::Steps);
    d.frames(2);
    let task = d.app.project.model.activities[0].task_id;
    let before = steps_of(&d.app.project, task).len();
    d.click("detail_steps_from_template");
    assert!(d.app.dictionaries.template_pick.is_some());
    d.click("template_pick_cancel");
    assert!(d.app.dictionaries.template_pick.is_none());
    assert_eq!(d.app.history.counts().0, undo + 3);
    d.click("detail_steps_from_template");
    d.click("template_pick_add");
    assert_eq!(steps_of(&d.app.project, task).len(), before + 2);
    assert_eq!(d.app.history.counts().0, undo + 4);
    let ctx = d.ctx.clone();
    d.app.history_move(&ctx, false);
    assert_eq!(steps_of(&d.app.project, task).len(), before);
}
