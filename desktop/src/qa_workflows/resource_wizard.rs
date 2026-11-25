//! Workflow: the Resources view's Add with the New Resource wizard, against
//! the pages as observed (private observation notes). The switch is turned
//! on in User Preferences, Assistance (test harnesses start with it off),
//! then: every page to Finish, with a material's unit of measure, a role and
//! a personal calendar; Finish on the first page; the Resource ID refusals;
//! Cancel part way and Escape; "Skip this wizard next time" turning the
//! wizard off so the next Add inserts the resource at once; and back on in
//! User Preferences. A workspace with the default switches shows the box
//! ticked and Add opens the wizard. Each result, with or without the wizard,
//! is one undo step.
use super::*;
use crate::resource_wizard::{Page, ResourceWizard};

/// User Preferences > Assistance > Add resources through the wizard,
/// ticked or cleared; returns what the box showed before.
fn set_wizard(d: &mut Driver, on: bool) -> bool {
    d.home();
    d.click("menu_bar_edit");
    d.click("menu_user_preferences");
    d.click("prefs_tab_assistance");
    let before = d.app.activity_wizard.assistance.resource_wizard;
    if before != on {
        d.click("prefs_wizard_resource");
    }
    d.click("prefs_close");
    d.frames(1);
    assert_eq!(d.app.activity_wizard.assistance.resource_wizard, on);
    before
}

/// The switch as the workspace keeps it.
fn stored_switch(d: &Driver) -> bool {
    let text = d
        .app
        .store
        .preference(openxer_automation::user_prefs::ASSISTANCE_KEY)
        .unwrap()
        .expect("the switch is stored");
    openxer_automation::user_prefs::Assistance::from_json(&text).resource_wizard
}

/// The Resources view and its Add on the Edit bar.
fn resources_add(d: &mut Driver) {
    show(d, "resources");
    d.click("add");
    d.frames(2);
}

fn wizard(d: &Driver) -> &ResourceWizard {
    d.app
        .activity_wizard
        .resource
        .as_ref()
        .expect("the wizard is open")
}

fn page(d: &Driver) -> Page {
    wizard(d).page
}

fn next(d: &mut Driver, to: Page) {
    d.click("new_resource_wizard_next");
    assert_eq!(page(d), to, "Next goes to {to:?}");
}

fn finish(d: &mut Driver) {
    d.click("new_resource_wizard_finish");
    d.frames(2);
    d.assert_clean("finish");
    assert!(
        d.app.activity_wizard.resource.is_none(),
        "Finish closes the wizard: {:?}",
        d.app
            .activity_wizard
            .resource
            .as_ref()
            .and_then(|w| w.message.clone())
    );
}

/// The refusal shown, then OK.
fn refused(d: &mut Driver) -> String {
    let text = wizard(d)
        .message
        .as_ref()
        .map(|(_, t)| t.clone())
        .expect("a refusal is shown");
    d.click("new_resource_message_ok");
    d.frames(2);
    assert!(wizard(d).message.is_none(), "OK closes the message");
    text
}

/// Click a text field, select what it holds and delete it.
fn clear(d: &mut Driver, action: &str) {
    d.click(action);
    d.key_with(Key::A, ctrl());
    d.key(Key::Backspace);
    d.frames(1);
}

/// The RSRC rows, as (Resource ID, rsrc_id).
fn resources(p: &ProjectData) -> Vec<(String, i64)> {
    p.document.table("RSRC").map_or(vec![], |t| {
        t.rows
            .iter()
            .map(|r| {
                (
                    t.value(r, "rsrc_short_name").to_string(),
                    crate::records::num(t.value(r, "rsrc_id")),
                )
            })
            .collect()
    })
}

fn id_of(p: &ProjectData, code: &str) -> Option<i64> {
    resources(p)
        .into_iter()
        .find(|(c, _)| c == code)
        .map(|(_, id)| id)
}

fn value(p: &ProjectData, rsrc: i64, field: &str) -> String {
    crate::views::resource_edit::value(&p.document, rsrc, field)
}

/// The sample project with a unit of measure and a role to pick, and one
/// resource ("PF") for the duplicate ID.
fn project() -> ProjectData {
    let mut p = openxer_xer::starter();
    let doc = &mut p.document;
    doc.table_mut("UMEASURE").add(&[
        ("unit_id", "7".into()),
        ("unit_abbrev", "t".into()),
        ("unit_name", "Tonne".into()),
    ]);
    doc.table_mut("ROLES").add(&[
        ("role_id", "4".into()),
        ("role_short_name", "FIT".into()),
        ("role_name", "Pipefitter".into()),
    ]);
    let id = crate::views::resource_edit::add_resource(doc, None, "1", "", "2026-11-02 00:00");
    crate::views::resource_edit::set_value(doc, id, "rsrc_short_name", "PF".into());
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

#[test]
fn workflow_new_resource_wizard_all_pages_finish_and_undo() {
    let mut d = Driver::new("wf-resource-wizard", project());
    assert!(
        openxer_automation::user_prefs::Assistance::default().resource_wizard,
        "the wizard is on by default"
    );
    assert!(!set_wizard(&mut d, true), "test harnesses start it off");
    assert!(stored_switch(&d), "kept in the workspace");
    let start = d.app.project.clone();
    let undo = d.app.history.counts().0;
    let calendars = d.app.project.document.table("CALENDAR").unwrap().rows.len();

    resources_add(&mut d);
    {
        let w = wizard(&d);
        assert_eq!(w.page, Page::Name);
        assert_eq!(w.id, "R", "observed default ID");
        assert_eq!(w.name, "(New Resource)", "observed default name");
        assert_eq!(w.kind, "RT_Labor");
        assert!(w.auto_compute, "actuals computed by default");
        assert!(!w.personal, "an existing calendar by default");
    }
    assert_eq!(
        resources(&d.app.project),
        resources(&start),
        "nothing added yet"
    );
    let cancel = d.rect("new_resource_wizard_cancel");
    let prev = d.rect("new_resource_wizard_back");
    assert!(
        cancel.right() < prev.left() - 50. && (cancel.top() - prev.top()).abs() < 1.,
        "Cancel at the left of the button line"
    );
    // The ID is selected, so typing replaces it.
    d.type_text("WR-1");
    d.frames(1);
    type_into(&mut d, "new_resource_name", "Steel erection crew");
    let mut visited = vec![Page::Name];

    next(&mut d, Page::Type);
    visited.push(Page::Type);
    d.click("new_resource_material");
    assert_eq!(wizard(&d).kind, "RT_Mat");
    d.click("new_resource_unit_more");
    let admin = d.app.dictionaries.admin.as_ref().expect("Admin Categories");
    assert!(admin.select.is_some(), "opens in select mode");
    d.click("admin_cell_0_0");
    d.click("admin_select");
    assert_eq!(wizard(&d).unit.as_deref(), Some("7"), "Tonne picked");

    next(&mut d, Page::Units);
    visited.push(Page::Units);
    type_into(&mut d, "new_resource_price", "85");
    type_into(&mut d, "new_resource_max_units", "16/d");

    next(&mut d, Page::Contact);
    visited.push(Page::Contact);
    type_into(&mut d, "new_resource_phone", "555-0100");
    type_into(&mut d, "new_resource_email", "crew@example.com");

    next(&mut d, Page::Roles);
    visited.push(Page::Roles);
    d.click("new_resource_roles_add");
    assert!(d.app.enterprise.ui.select.is_some(), "Roles in select mode");
    d.click("roles_list_row_0");
    d.click("roles_select");
    assert_eq!(wizard(&d).roles, vec![4]);

    next(&mut d, Page::Calendar);
    visited.push(Page::Calendar);
    d.click("new_resource_calendar_personal");
    assert!(wizard(&d).personal);
    next(&mut d, Page::CalendarName);
    visited.push(Page::CalendarName);
    assert!(
        d.texts()
            .iter()
            .any(|(t, _)| t == "WR-1 - Steel erection crew"),
        "the personal calendar's name is shown"
    );
    next(&mut d, Page::AutoCompute);
    visited.push(Page::AutoCompute);
    d.click("new_resource_auto_off");
    next(&mut d, Page::Done);
    visited.push(Page::Done);
    assert_eq!(visited.len(), 9, "nine pages: {visited:?}");
    assert_eq!(
        d.app.project.document.table("CALENDAR").unwrap().rows.len(),
        calendars,
        "no calendar until Finish"
    );
    finish(&mut d);

    let p = &d.app.project;
    let id = id_of(p, "WR-1").expect("the resource was added");
    assert_eq!(d.app.views.resources.selected, Some(id), "selected");
    assert_eq!(value(p, id, "rsrc_name"), "Steel erection crew");
    assert_eq!(value(p, id, "rsrc_type"), "RT_Mat");
    assert_eq!(value(p, id, "unit_id"), "7");
    assert_eq!(value(p, id, "office_phone"), "555-0100");
    assert_eq!(value(p, id, "email_addr"), "crew@example.com");
    assert_eq!(value(p, id, "role_id"), "4", "the role is primary");
    assert_eq!(value(p, id, "auto_compute_act_flag"), "N");
    let rate = crate::views::resource_edit::rate_rows(&p.document, id)[0];
    let rates = p.document.table("RSRCRATE").unwrap();
    let rate = &rates.rows[rate];
    assert_eq!(
        rates.value(rate, "cost_per_qty").parse::<f64>().ok(),
        Some(85.)
    );
    assert_eq!(
        rates.value(rate, "max_qty_per_hr").parse::<f64>().ok(),
        Some(2.)
    );
    let calendar = value(p, id, "clndr_id");
    assert_eq!(
        openxer_xer::edit::calendars::name(&p.document, &calendar),
        "WR-1 - Steel erection crew",
        "a personal calendar named after the resource"
    );
    assert!(
        openxer_xer::edit::calendars::kind(&p.document, &calendar)
            == openxer_xer::edit::calendars::Kind::Resource,
        "a resource calendar"
    );
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");

    // One undo takes the resource, its role and its calendar back.
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(resources(&d.app.project), resources(&start));
    assert_eq!(
        d.app.project.document.table("CALENDAR").unwrap().rows.len(),
        calendars
    );
    assert_eq!(d.app.history.counts().0, undo);
}

#[test]
fn workflow_new_resource_wizard_refusals_early_finish_cancel_and_switch() {
    let mut d = Driver::new("wf-resource-wizard-2", project());
    set_wizard(&mut d, true);
    let start = d.app.project.clone();
    let undo = d.app.history.counts().0;

    // Cancel part way adds nothing; so does Escape.
    resources_add(&mut d);
    next(&mut d, Page::Type);
    next(&mut d, Page::Units);
    d.click("new_resource_wizard_cancel");
    d.frames(2);
    assert!(d.app.activity_wizard.resource.is_none());
    assert_eq!(
        resources(&d.app.project),
        resources(&start),
        "Cancel adds nothing"
    );
    assert_eq!(d.app.history.counts().0, undo);
    resources_add(&mut d);
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.activity_wizard.resource.is_none(), "Escape cancels");

    // Only the Resource ID is required: blank, then used, are refused by
    // Next on the first page and by Finish.
    resources_add(&mut d);
    clear(&mut d, "new_resource_id");
    d.click("new_resource_wizard_next");
    assert!(refused(&mut d).contains("Resource ID"));
    assert_eq!(page(&d), Page::Name);
    type_into(&mut d, "new_resource_id", "PF");
    d.click("new_resource_wizard_next");
    assert!(refused(&mut d).contains("PF"));
    d.click("new_resource_wizard_finish");
    d.frames(2);
    assert!(refused(&mut d).contains("PF"));
    assert_eq!(resources(&d.app.project), resources(&start));

    // Finish on the first page: the resource with the defaults of the
    // later pages.
    type_into(&mut d, "new_resource_id", "R");
    finish(&mut d);
    let p = &d.app.project;
    let id = id_of(p, "R").expect("R was added");
    assert_eq!(value(p, id, "rsrc_name"), "(New Resource)");
    assert_eq!(value(p, id, "rsrc_type"), "RT_Labor");
    assert_eq!(value(p, id, "def_qty_per_hr").parse::<f64>().ok(), Some(1.));
    assert_eq!(value(p, id, "auto_compute_act_flag"), "Y");
    assert_eq!(value(p, id, "clndr_id"), "1", "the project calendar");
    assert_eq!(d.app.history.counts().0, undo + 1);
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(resources(&d.app.project), resources(&start));

    // "Skip this wizard next time", then Cancel: the switch is off and the
    // next Add inserts the resource at once.
    resources_add(&mut d);
    d.click("new_resource_skip");
    d.click("new_resource_wizard_cancel");
    d.frames(2);
    assert!(!d.app.activity_wizard.assistance.resource_wizard);
    assert!(!stored_switch(&d), "off in the workspace");
    let undo = d.app.history.counts().0;
    resources_add(&mut d);
    assert!(d.app.activity_wizard.resource.is_none(), "no wizard");
    assert_eq!(
        resources(&d.app.project).len(),
        resources(&start).len() + 1,
        "the resource is in at once"
    );
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(resources(&d.app.project), resources(&start), "undone");
    // User Preferences shows it off; tick it again and Add opens the wizard.
    assert!(!set_wizard(&mut d, true), "Assistance reads off");
    assert!(stored_switch(&d));
    resources_add(&mut d);
    assert_eq!(page(&d), Page::Name, "the wizard is back");
    d.click("new_resource_wizard_cancel");
}

#[test]
fn workflow_new_resource_wizard_switch_shown_and_on_by_default() {
    let mut d = Driver::new("wf-resource-wizard-3", project());
    // A workspace that holds the default switches, as a new one starts.
    let defaults = openxer_automation::user_prefs::Assistance::default();
    d.app
        .store
        .set_preference(
            openxer_automation::user_prefs::ASSISTANCE_KEY,
            &defaults.to_json(),
        )
        .unwrap();
    d.app.restore_assistance();
    d.home();
    d.click("menu_bar_edit");
    d.click("menu_user_preferences");
    d.click("prefs_tab_assistance");
    assert!(
        d.has("prefs_wizard_resource"),
        "the box is on the Assistance tab"
    );
    assert!(d.app.activity_wizard.assistance.resource_wizard, "ticked");
    d.click("prefs_close");
    d.frames(1);
    let start = d.app.project.clone();
    resources_add(&mut d);
    assert_eq!(page(&d), Page::Name, "Add opens the wizard");
    d.click("new_resource_wizard_cancel");
    d.frames(2);
    assert_eq!(resources(&d.app.project), resources(&start));
}
