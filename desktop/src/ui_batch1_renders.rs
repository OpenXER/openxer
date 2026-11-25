//! Software renders, for review, to the ignored
//! `artifacts/ui-batch1/`: the Projects view's Funding tab (empty, then with
//! rows), Customize Columns on a record view and the view with a column
//! removed, Customize Resource Rate Columns and the rate grid with Price /
//! Unit 2, the Resource Assignments Open Layout and Save Layout As windows,
//! the Resource Usage Spreadsheet on top, and the usage sheet menu opened
//! by a right-click made in the frame the pointer jumped.
use super::*;
use crate::qa_sweep::Driver;
use crate::views::MainView;
use egui::Pos2;
use openxer_xer::edit::funding;

fn right_click(d: &mut Driver, pos: Pos2) {
    d.press_at(pos, egui::PointerButton::Secondary, Default::default());
}

#[test]
fn render_ui_batch1_surfaces() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/ui-batch1");
    std::fs::create_dir_all(&out).unwrap();
    let shot = |d: &mut Driver, name: &str| {
        d.frames(2);
        d.screenshot(&out.join(format!("{name}.png")));
    };
    let mut p = crate::views::wbs::tests::with_project_node();
    for (id, name) in [("7", "Capital Budget"), ("9", "Operating Grant")] {
        crate::records::add_row(
            &mut p.document,
            funding::SOURCES,
            &[("fund_id", id.into()), ("fund_name", name.into())],
        );
    }
    {
        // A resource with a rate row and an assignment, from the resource
        // fixture.
        let r = crate::views::resources::tests::with_resource();
        for table in ["RSRC", "RSRCRATE", "TASKRSRC"] {
            if let Some(t) = r.document.table(table) {
                *p.document.table_mut(table) = t.clone();
            }
        }
        p = ProjectData::from_document(p.document, p.model.project.proj_id, "batch1".into())
            .unwrap();
    }
    let screen = Vec2::new(1500., 900.);
    let mut d = Driver::probed("batch1-renders", p, screen, Some(out.clone()));
    d.home();

    // 1. Funding tab: empty, then two rows with the first selected.
    d.click("directory_projects");
    let pr = &d.app.project;
    d.app.views.projects.selected = Some((pr.source_name.clone(), pr.model.project.proj_id));
    d.app.views.projects.show_details = true;
    d.app.views.projects.tab = crate::views::projects::TABS
        .iter()
        .position(|t| *t == "Funding")
        .unwrap();
    shot(&mut d, "01-funding-empty");
    d.click("project_funding_buttons_0");
    d.click("project_funding_buttons_0");
    let rows: Vec<usize> = funding::list(&d.app.project)
        .iter()
        .map(|f| f.row)
        .collect();
    for (row, (amount, share)) in rows.iter().zip([("250000", "40"), ("375000", "60")]) {
        funding::set(&mut d.app.project, *row, funding::Field::Amount, amount).unwrap();
        funding::set(&mut d.app.project, *row, funding::Field::Share, share).unwrap();
    }
    d.app.touch_records();
    d.app.views.projects.funding_row = Some(0);
    shot(&mut d, "02-funding-rows");

    // 2. Customize Columns on the Risks view, then the view without Risk Type.
    d.app.show_view(MainView::Risks);
    d.frames(3);
    let r = d.rect("risks_layout");
    right_click(&mut d, Pos2::new(r.center().x, r.bottom() + 60.));
    shot(&mut d, "03-risks-menu");
    d.click("risks_menu_customize");
    shot(&mut d, "04-risks-customize-columns");
    d.click("detail_columns_selected_1");
    d.click("detail_columns_remove");
    shot(&mut d, "05-risks-customize-removed");
    d.click("detail_columns_ok");
    shot(&mut d, "06-risks-view-after");

    // 3. Customize Resource Rate Columns and the grid with Price / Unit 2.
    d.click("directory_resources");
    let res = &mut d.app.views.resources;
    res.assigned_only = false;
    res.selected = Some(7);
    res.show_bottom = true;
    res.tab = 3;
    d.frames(3);
    let at = d.rect("resource_rates_row_0").center();
    right_click(&mut d, at);
    d.click("resource_rates_menu_customize");
    d.click("detail_columns_selected_2");
    d.click("detail_columns_available_0");
    d.click("detail_columns_add");
    shot(&mut d, "07-rate-columns-dialog");
    d.click("detail_columns_ok");
    shot(&mut d, "08-rate-grid-price-2");

    // 4. Resource Assignments layouts.
    d.app.show_view(MainView::Assignments);
    d.frames(3);
    d.app.views.assignments.zoom = 140.;
    d.app.save_assignment_layout_as("Crew Lookahead").unwrap();
    let header = d.rect("assignments_col_0");
    let at = Pos2::new(header.center().x, header.bottom() + 27.);
    right_click(&mut d, at);
    let hover = d.rect("assignments_menu_layout").center();
    let _ = d.frame(vec![egui::Event::PointerMoved(hover)]);
    shot(&mut d, "09-assignments-layout-menu");
    d.click("assignments_menu_layout_open");
    shot(&mut d, "10-assignments-open-layout");
    d.key(egui::Key::Escape);
    d.frames(2);
    d.app.open_assignment_save_as();
    shot(&mut d, "11-assignments-save-layout-as");
    d.click("save_as_cancel");

    // 5. Resource Usage Spreadsheet on top.
    d.click("directory_activities");
    d.app.set_bottom(None);
    d.app.set_top(crate::panes::TopView::ResourceUsageSheet);
    d.app.panes.usage.resource = Some(7);
    shot(&mut d, "12-resource-usage-on-top");

    // 6. A right-click in the frame the pointer jumps, after a menu click.
    d.app.set_top(crate::panes::TopView::Gantt);
    d.app
        .set_bottom(Some(crate::panes::BottomView::ActivityUsageSheet));
    d.frames(3);
    let k = d
        .app
        .rows()
        .iter()
        .position(|r| matches!(r, crate::grid::Row::Activity(_)))
        .unwrap();
    let r = d.rect(&format!("usage_sheet_row_{k}"));
    let row = Pos2::new(r.left() + 20., r.center().y);
    right_click(&mut d, row);
    let item = d.rect("usage_sheet_menu_zoom_in").center();
    d.click_at(item);
    let m = egui::Modifiers::default();
    let button = |pressed| egui::Event::PointerButton {
        pos: row,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: m,
    };
    let _ = d.frame_with(vec![egui::Event::PointerMoved(row), button(true)], m);
    let _ = d.frame_with(vec![button(false)], m);
    assert!(d.has("usage_sheet_menu_zoom_in"), "the menu opened");
    shot(&mut d, "13-usage-sheet-menu-after-jump");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}
