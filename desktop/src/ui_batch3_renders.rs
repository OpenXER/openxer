//! Software renders, for review, to the ignored
//! `artifacts/ui-batch3/`: Resources > Del. / Merge with its preview (merge
//! and delete), the Stacked Histogram on the bottom activity and resource
//! profiles (by resource and by role), the WBS Earned Value tab edited, and the Issues view after Monitor
//! Thresholds raised cost and earned value issues.
use super::*;
use crate::qa_sweep::Driver;
use crate::views::MainView;
use egui::Pos2;
use openxer_xer::edit::earned_value as ev;

fn right_click(d: &mut Driver, pos: Pos2) {
    d.press_at(pos, egui::PointerButton::Secondary, Default::default());
}

#[test]
fn render_ui_batch3_surfaces() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/ui-batch3");
    std::fs::create_dir_all(&out).unwrap();
    let shot = |d: &mut Driver, name: &str| {
        d.frames(2);
        d.screenshot(&out.join(format!("{name}.png")));
    };
    let screen = Vec2::new(1500., 900.);

    // 1. Del. / Merge: the duplicate Pipefitter PIPE-2 into PIPE, then the
    // delete mode on an unused resource.
    let mut d = Driver::probed(
        "batch3-merge",
        crate::views::resource_merge_tests::duplicates(),
        screen,
        Some(out.clone()),
    );
    d.home();
    d.click("directory_resources");
    d.app.views.resources.assigned_only = false;
    d.app.views.resources.selected = Some(11);
    d.frames(3);
    let r = d.rect("resources_row_0");
    right_click(&mut d, Pos2::new(r.left() + 90., r.center().y));
    d.app.views.resources.selected = Some(11);
    shot(&mut d, "01-resources-menu-del-merge");
    d.click("resources_menu_del_merge");
    shot(&mut d, "02-merge-preview");
    d.click("resource_merge_apply");
    shot(&mut d, "03-after-merge");
    d.app.views.resources.selected = Some(9);
    d.app.open_resource_merge();
    shot(&mut d, "04-delete-unused");
    d.key(egui::Key::Escape);

    // 2. Stacked Histogram on the bottom profiles.
    let mut d = Driver::probed(
        "batch3-stacked",
        crate::panes::stacked_tests::two_crews(),
        screen,
        Some(out.clone()),
    );
    d.home();
    d.app
        .set_bottom(Some(crate::panes::BottomView::ActivityUsageProfile));
    d.frames(3);
    let body = d.app.panes.body;
    right_click(&mut d, Pos2::new(body.center().x + 80., body.center().y));
    let hover = d.rect("usage_profile_menu_stack_by").center();
    let _ = d.frame(vec![egui::Event::PointerMoved(hover)]);
    shot(&mut d, "05-profile-menu-stacked");
    d.key(egui::Key::Escape);
    d.app.panes.usage.stack_activity = true;
    shot(&mut d, "06-activity-profile-by-resource");
    d.app.panes.usage.stack_by = crate::panes::stacked::StackBy::Role;
    shot(&mut d, "07-activity-profile-by-role");
    d.app.panes.usage.resource = Some(7);
    d.app.panes.usage.stack_resource = true;
    d.app.panes.usage.stack_by = crate::panes::stacked::StackBy::Resource;
    d.app
        .set_bottom(Some(crate::panes::BottomView::ResourceUsageProfile));
    shot(&mut d, "08-resource-profile-by-resource");

    // 3. WBS Earned Value tab, edited: Custom 35 % and PF = 1.25.
    let mut p = crate::grid::tests_h3::progressed_starter();
    let wbs = p.details[&p.model.activities[1].task_id].wbs;
    ev::set(
        &mut p,
        wbs,
        Some("custom"),
        Some("35"),
        Some("pf"),
        Some("1.25"),
    )
    .unwrap();
    let mut d = Driver::probed("batch3-ev", p, screen, Some(out.clone()));
    d.home();
    d.click("directory_wbs");
    d.app.views.wbs.selected = Some(wbs);
    d.app.views.wbs.show_details = true;
    d.app.views.wbs.tab = crate::views::wbs::TABS
        .iter()
        .position(|t| *t == "Earned Value")
        .unwrap();
    shot(&mut d, "10-wbs-earned-value-tab");

    // 4. Monitor Thresholds on CPI and CV: the Issues view.
    for (parm, high) in [(6, "1.2"), (2, "100")] {
        let id = crate::views::thresholds::add_threshold(&mut d.app.project);
        for (column, value) in [
            ("thresh_parm_id", parm.to_string()),
            ("hi_parm_value", high.to_string()),
            ("lo_parm_value", String::new()),
        ] {
            crate::views::record_view::set_value(
                &mut d.app.project.document,
                "PROJTHRS",
                "thresh_id",
                id,
                column,
                value,
            );
        }
    }
    ev::set(
        &mut d.app.project,
        wbs,
        Some("50/50"),
        None,
        Some("remaining"),
        None,
    )
    .unwrap();
    d.app.touch_records();
    d.app.open_monitor_thresholds();
    shot(&mut d, "11-monitor-thresholds");
    d.click("monitor_run");
    d.app.show_view(MainView::Issues);
    shot(&mut d, "12-issues-from-cost-thresholds");
}
