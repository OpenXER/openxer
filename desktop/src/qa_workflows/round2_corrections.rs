//! Workflow: corrections from the second round of reference observations.
//!
//! - The question that holds a typed Start or a dropped bar with a Start On
//!   or After constraint frames Yes as its default: Enter adds the
//!   constraint.
//! - Resources view Add with the wizard off inserts Resource ID "R" named
//!   "(New Resource)" (the next free ID, `R-1`..., when taken), the same
//!   start the wizard's first page shows.
//! - Predecessor Details and Successor Details, added from the General
//!   group of the Columns dialog, read `A1000: FS, A1010: SS 1` (the lag
//!   only when not zero, without its unit letter) and go out in the XLSX
//!   export.
use super::*;
use crate::kit_window::Question;

fn activity(p: &ProjectData, code: &str) -> Activity {
    p.model.activities[index_of(p, code)].clone()
}

fn asked(d: &Driver) -> bool {
    d.app.sched.start_hold.is_some() && d.has("start_hold_yes") && d.has("start_hold_no")
}

#[test]
fn workflow_enter_holds_a_typed_start_and_a_dropped_bar() {
    let mut d = fresh("wf-round2-start-hold");
    let start = d.app.project.clone();
    assert!(Question::StartHold.is_default("Yes"));

    // A typed Start: the question frames Yes, Enter adds the constraint.
    let s0 = activity(&start, "RB-1050").early_start.unwrap();
    let v = s0 + 7 * 24 * 60;
    let text = format::date(d.app.project.model.origin_serial, Some(v));
    d.home();
    let i = index_of(&d.app.project, "RB-1050");
    cell_edit(&mut d, i, "start", &text);
    d.frames(2);
    assert!(asked(&d), "a typed Start asks");
    assert_eq!(
        crate::dialog_keys::top_default_label(&d.ctx).as_deref(),
        Some("Yes"),
        "Yes is the default"
    );
    d.key(Key::Enter);
    d.frames(2);
    d.assert_clean("Enter on the typed Start");
    assert!(d.app.sched.start_hold.is_none(), "Enter answered");
    let a = activity(&d.app.project, "RB-1050");
    assert_eq!((a.cstr, a.cstr_date), (CstrType::StartOnAfter, Some(v)));

    // A dropped bar asks with the same box: Enter holds it too.
    let shift = super::bar_drag_hold::shift_into_work(&d.app.project, "RB-1060", 2);
    let w = super::bar_drag_hold::drag_body(&mut d, "RB-1060", shift);
    assert!(asked(&d), "the drop asks");
    assert_eq!(
        crate::dialog_keys::top_default_label(&d.ctx).as_deref(),
        Some("Yes")
    );
    d.key(Key::Enter);
    d.frames(2);
    d.assert_clean("Enter on the dropped bar");
    let b = activity(&d.app.project, "RB-1060");
    assert_eq!((b.cstr, b.cstr_date), (CstrType::StartOnAfter, Some(w)));
}

fn resource(p: &ProjectData, id: i64, field: &str) -> String {
    crate::views::resource_edit::value(&p.document, id, field)
}

#[test]
fn workflow_resources_add_without_the_wizard_matches_its_first_page() {
    let mut d = fresh("wf-round2-resource-add");
    assert!(
        !d.app.activity_wizard.assistance.resource_wizard,
        "test harnesses start with the wizard off"
    );
    show(&mut d, "resources");
    let undo = d.app.history.counts().0;

    // Add: a row at once, "R" named "(New Resource)", labor, 8/d.
    d.click("add");
    d.frames(2);
    d.assert_clean("Add");
    assert!(d.app.activity_wizard.resource.is_none(), "no wizard");
    let first = d.app.views.resources.selected.expect("the new resource");
    let p = &d.app.project;
    assert_eq!(resource(p, first, "rsrc_short_name"), "R");
    assert_eq!(resource(p, first, "rsrc_name"), "(New Resource)");
    assert_eq!(resource(p, first, "rsrc_type"), "RT_Labor");
    assert_eq!(
        resource(p, first, "def_qty_per_hr").parse::<f64>().ok(),
        Some(1.),
        "8/d"
    );
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");

    // Add again: "R" is taken, so the next free ID.
    d.click("add");
    d.frames(2);
    let second = d.app.views.resources.selected.unwrap();
    assert_ne!(second, first);
    let p = &d.app.project;
    assert_eq!(resource(p, second, "rsrc_short_name"), "R-1");
    assert_eq!(resource(p, second, "rsrc_name"), "(New Resource)");

    // With the wizard on, its first page starts the same way.
    d.app.set_resource_wizard(true);
    d.click("add");
    d.frames(2);
    let w = d.app.activity_wizard.resource.clone().expect("the wizard");
    assert_eq!((w.id.as_str(), w.name.as_str()), ("R-2", "(New Resource)"));
    assert!(d.has("new_resource_id"), "page 1 shows the ID field");
    d.click("new_resource_wizard_finish");
    d.frames(2);
    d.assert_clean("wizard Finish");
    assert!(d.app.activity_wizard.resource.is_none(), "Finish closed it");
    let t = d.app.project.document.table("RSRC").unwrap();
    let names: Vec<(String, String)> = t
        .rows
        .iter()
        .map(|r| {
            (
                t.value(r, "rsrc_short_name").to_string(),
                t.value(r, "rsrc_name").to_string(),
            )
        })
        .collect();
    assert!(
        names.contains(&("R-2".into(), "(New Resource)".into())),
        "the wizard added R-2: {names:?}"
    );
}

/// The scheduled sample project with only these relationships (added in
/// this order, so the details must sort by Activity ID): RB-1010 SS 1d to
/// RB-1020, RB-1000 FS 2d to RB-1020, RB-1020 FS to RB-1030.
fn linked_project() -> ProjectData {
    use openxer_engine::model::{Link, LinkType};
    let mut p = openxer_xer::starter();
    let at = |code: &str| index_of(&p, code);
    let day = 8 * 60;
    let links = [
        ("RB-1010", "RB-1020", LinkType::SS, day),
        ("RB-1000", "RB-1020", LinkType::FS, 2 * day),
        ("RB-1020", "RB-1030", LinkType::FS, 0),
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

/// Scroll the Columns dialog's Available Options until the row of column
/// `id` is drawn whole.
fn reveal_available(d: &mut Driver, id: &str) {
    let key = format!("columns_available_{id}");
    let at = d.rect("columns_group_General").center();
    for _ in 0..60 {
        if d.has(&key) && d.rect(&key).height() >= 12. {
            return;
        }
        let _ = d.frame(vec![
            egui::Event::PointerMoved(at),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0., -16.),
                modifiers: Default::default(),
            },
        ]);
        d.frames(1);
    }
    panic!("{id} is not offered under General");
}

#[test]
fn workflow_predecessor_and_successor_details_columns() {
    use crate::grid::columns;
    use openxer_interchange::xlsx_import::{self as xi, Cell};
    let mut d = Driver::new("wf-round2-link-details", linked_project());
    let model = d.app.project.model.clone();

    // Columns dialog, General group: add both, OK.
    d.home();
    d.app.open_columns_dialog();
    d.frames(3);
    d.click("columns_group_General");
    d.frames(1);
    for id in ["predecessor_details", "successor_details"] {
        reveal_available(&mut d, id);
        d.click(&format!("columns_available_{id}"));
        d.click("columns_add");
        d.frames(1);
    }
    d.click("columns_ok");
    d.frames(2);
    d.assert_clean("Columns OK");
    let shown: Vec<String> = d
        .app
        .grid
        .layout
        .columns
        .iter()
        .map(|c| c.id.clone())
        .collect();
    for id in ["predecessor_details", "successor_details"] {
        assert!(shown.iter().any(|s| s == id), "{id} shown: {shown:?}");
    }
    assert_eq!(d.app.project.model, model, "nothing changed in the project");

    // The text, as the table shows it; read-only, under General.
    let p = d.app.project.clone();
    let catalog = columns::catalog(&p);
    let ix = d.app.grid_index();
    let text = |id: &str, code: &str| {
        let column = columns::find(&catalog, id).unwrap();
        assert_eq!(column.editor, columns::Editor::ReadOnly, "{id}");
        assert_eq!(column.group, columns::GENERAL, "{id}");
        let i = index_of(&p, code);
        columns::display(&p, Some(i), &columns::value(&p, &ix, column, i))
    };
    assert_eq!(
        text("predecessor_details", "RB-1020"),
        "RB-1000: FS 2, RB-1010: SS 1"
    );
    assert_eq!(text("successor_details", "RB-1020"), "RB-1030: FS");
    assert_eq!(text("successor_details", "RB-1000"), "RB-1020: FS 2");
    assert_eq!(text("predecessor_details", "RB-1030"), "RB-1020: FS");
    assert_eq!(text("predecessor_details", "RB-1000"), "");

    // The XLSX export carries both columns with the same text.
    let file = d.dir.join("link-details.xlsx");
    d.app.export_excel_to(&file);
    assert!(file.exists(), "{}", d.app.status);
    let wb = xi::read_workbook(&file).unwrap();
    let s = wb.sheet("Activities").unwrap();
    let headers: Vec<String> = wb.sheets[s].rows[0].iter().map(Cell::text).collect();
    let col = |h: &str| {
        headers
            .iter()
            .position(|x| x == h)
            .unwrap_or_else(|| panic!("no {h} in {headers:?}"))
    };
    let (id, pred, succ) = (
        col("Activity ID"),
        col("Predecessor Details"),
        col("Successor Details"),
    );
    let row = wb.sheets[s]
        .rows
        .iter()
        .find(|r| r.get(id).map(Cell::text).as_deref() == Some("RB-1020"))
        .expect("RB-1020 is exported");
    assert_eq!(row[pred].text(), "RB-1000: FS 2, RB-1010: SS 1");
    assert_eq!(row[succ].text(), "RB-1030: FS");
}
