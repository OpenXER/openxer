//! The Resources window's bottom pane: View > Show on Bottom and the table's
//! right-click Show on Bottom offer No Bottom Layout and Resource Details
//! only. The Resource Usage Profile stays in the Activities window's bottom
//! pane. Driven through the production UI with egui `RawInput` only (no
//! native window).
use super::resources as rs;
use super::*;
use crate::panes::stacked_tests::two_crews;
use crate::panes::{usage, BottomView};
use crate::qa_sweep::Driver;

/// The Resources window over `two_crews`, every resource listed, PIPE
/// selected.
fn resources_window(name: &str) -> Driver {
    let mut d = Driver::new(name, two_crews());
    d.click("directory_resources");
    d.app.views.resources.assigned_only = false;
    d.app.views.resources.selected = Some(7);
    d.frames(4);
    d
}

#[test]
fn view_show_on_bottom_offers_details_or_nothing_in_the_resources_window() {
    let mut d = resources_window("rb-show-on-bottom");
    for (item, details) in [
        ("menu_resources_bottom_none", false),
        ("menu_resources_bottom_details", true),
    ] {
        d.click("menu_bar_view");
        assert!(d.hover_text("Show on Bottom"), "Show on Bottom is listed");
        assert!(d.has(item), "{item} is drawn");
        assert!(
            !d.has("menu_resources_bottom_profile") && !d.has("menu_bottom_activity_table"),
            "only the two Resources choices are offered"
        );
        assert!(!d.texts().iter().any(|(t, _)| t == "Resource Usage Profile"));
        d.click(item);
        d.frames(2);
        assert_eq!(d.app.views.resources.show_bottom, details, "{item}");
        assert!(!d.has("usage_profile_display"), "{item}: no profile");
    }
    // Bottom Layout Options belongs to the Activities window.
    d.click("menu_bar_view");
    d.click("menu_bottom_layout_options");
    d.frames(2);
    assert!(d.app.panes.profile_dialog.is_none(), "disabled here");
    d.key(egui::Key::Escape);
    d.frames(2);
    // The Activities window keeps its own Show on Bottom.
    d.click("directory_activities");
    d.click("menu_bar_view");
    assert!(d.hover_text("Show on Bottom"));
    assert!(d.has("menu_bottom_activity_table"));
    assert!(!d.has("menu_resources_bottom_details"));
    d.key(egui::Key::Escape);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn layouts_saved_with_the_resources_profile_restore_to_details() {
    let mut d = resources_window("rb-old-json");
    let old = serde_json::json!({
        "resources": {
            "tab": 2,
            "bottom": "profile",
            "show_bottom": true,
            "details_height": 300.0,
            "assigned_only": false,
            "stacked": false,
            "stack": true,
            "stack_by": "role",
            "selected_activity_only": true,
            "profile_periods": [[0, 1440]],
            "profile_legend_width": 260.0,
            "rate_columns": []
        }
    });
    let mut state = rs::ResourcesState::default();
    state.restore(&old["resources"]);
    assert!(state.show_bottom);
    assert_eq!((state.tab, state.details_height), (2, 300.));
    let json = state.json();
    assert_eq!(json["bottom"], "details");
    for key in ["stacked", "stack", "stack_by", "profile_legend_width"] {
        assert!(json.get(key).is_none(), "{key} is no longer saved");
    }
    d.app.views.restore(&old.to_string());
    d.frames(3);
    assert!(d.app.views.resources.show_bottom);
    assert_eq!(d.app.views.resources.tab, 2);
    assert!(!d.has("usage_profile_display"), "the details, no profile");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn the_activities_bottom_resource_profile_is_unaffected() {
    let mut d = resources_window("rb-activities-profile");
    d.app.views.resources.show_bottom = false;
    d.click("directory_activities");
    d.app.panes.usage.resource = Some(7);
    d.app.set_bottom(Some(BottomView::ResourceUsageProfile));
    d.frames(4);
    assert!(d.has("usage_profile_display"), "the profile's Display bar");
    let body = d.rect("usage_profile_timescale_body");
    let output = d.frame(vec![]);
    let bars = output
        .shapes
        .iter()
        .filter(|s| {
            matches!(&s.shape, egui::Shape::Rect(r)
                if r.fill == usage::REMAINING.0 && body.contains_rect(r.rect))
        })
        .count();
    assert!(bars > 0, "PIPE's remaining units are drawn");
    // View > Bottom Layout Options opens Profile Options here.
    d.click("menu_bar_view");
    d.click("menu_bottom_layout_options");
    d.frames(2);
    assert!(d.app.panes.profile_dialog.is_some());
    d.key(egui::Key::Escape);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

/// Renders of the Resources window with Resource Details on the bottom and
/// with View > Show on Bottom open, to the ignored
/// `artifacts/resources-bottom/`.
#[test]
fn render_the_resources_bottom_pane() {
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/resources-bottom");
    std::fs::create_dir_all(&out).unwrap();
    let mut d = Driver::probed(
        "rb-render",
        two_crews(),
        egui::Vec2::new(1500., 900.),
        Some(out.clone()),
    );
    d.click("directory_resources");
    d.app.views.resources.assigned_only = false;
    d.app.views.resources.selected = Some(7);
    d.app.views.resources.show_bottom = true;
    d.frames(3);
    d.screenshot(&out.join("details.png"));
    d.click("menu_bar_view");
    assert!(d.hover_text("Show on Bottom"));
    d.frames(2);
    d.screenshot(&out.join("view-show-on-bottom.png"));
    d.key(egui::Key::Escape);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}
