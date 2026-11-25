//! Show Actual and Remaining on the usage profiles: the right-click menu of
//! each bottom profile, the Profile Options check box, the layout, and how it combines with Stacked
//! Histogram (each series split into its actual part, darker, at the bottom
//! and its remaining part above).
use super::stacked::{self, Series, StackBy};
use super::stacked_tests::two_crews;
use super::*;
use crate::qa_sweep::Driver;
use crate::views::assignments as asg;
use egui::Pos2;

/// `two_crews` with the survey activity (1010) under way: started a week
/// before the data date, PIPE has 8 h actual and 16 h remaining on it.
fn under_way() -> ProjectData {
    let mut doc = two_crews().updated_document();
    let tasks = doc.table_mut("TASK");
    let row = tasks
        .rows
        .iter()
        .position(|r| tasks.value(r, "task_id") == "1010")
        .unwrap();
    tasks.set(row, "status_code", "TK_Active".into());
    tasks.set(row, "act_start_date", "2026-10-26 08:00".into());
    let assignments = doc.table_mut("TASKRSRC");
    let row = assignments
        .rows
        .iter()
        .position(|r| assignments.value(r, "rsrc_id") == "7")
        .unwrap();
    assignments.set(row, "act_reg_qty", "8".into());
    assignments.set(row, "remain_qty", "16".into());
    let mut p = ProjectData::from_document(doc, 1, "under-way".into()).unwrap();
    openxer_engine::scheduler::schedule_with_resources(&mut p.model, &p.resources).unwrap();
    p
}

fn series(label: &str, values: &[f64], actual: &[f64]) -> Series {
    Series {
        label: label.into(),
        values: values.to_vec(),
        actual: actual.to_vec(),
        other: false,
    }
}

#[test]
fn split_segments_put_each_series_actual_under_its_remaining() {
    let list = [series("A", &[10.], &[4.]), series("B", &[6.], &[0.])];
    let plain = stacked::segments(&list, 0, false);
    assert_eq!(
        plain.iter().map(|s| (s.0, s.1)).collect::<Vec<_>>(),
        [(0., 10.), (10., 16.)]
    );
    let split = stacked::segments(&list, 0, true);
    assert_eq!(
        split.iter().map(|s| (s.0, s.1)).collect::<Vec<_>>(),
        [(0., 4.), (4., 10.), (10., 10.), (10., 16.)]
    );
    assert_eq!(split[0].2, stacked::actual_colors(0, false));
    assert_eq!(split[1].2, stacked::colors(0, false));
    assert_ne!(split[0].2 .0, split[1].2 .0, "actual is a darker shade");
    // The stack is as tall either way.
    assert_eq!(plain.last().unwrap().1, split.last().unwrap().1);
}

#[test]
fn series_carry_their_actual_part() {
    let p = under_way();
    let periods: Vec<(i64, i64)> = (-30..60).map(|d| (d * DAY, (d + 1) * DAY)).collect();
    let list = asg::assignments(&p);
    let refs: Vec<&asg::Assignment> = list.iter().collect();
    let all = stacked::series(&p, &refs, &periods, false, StackBy::Resource);
    let pipe = all.iter().find(|s| s.label == "PIPE").unwrap();
    let actual: f64 = pipe.actual.iter().sum();
    let total: f64 = pipe.values.iter().sum();
    assert!((actual - 8.).abs() < 1e-6, "{actual}");
    assert!((total - 24.).abs() < 1e-6, "{total}");
    for (a, v) in pipe.actual.iter().zip(&pipe.values) {
        assert!(a <= &(v + 1e-9));
    }
    let weld = all.iter().find(|s| s.label == "WELD").unwrap();
    assert_eq!(weld.actual.iter().sum::<f64>(), 0.);
}

fn secondary(d: &mut Driver, pos: Pos2) {
    d.press_at(pos, egui::PointerButton::Secondary, Default::default());
}

fn open_profile_menu(d: &mut Driver) {
    let body = d.app.panes.body;
    secondary(d, Pos2::new(body.center().x + 80., body.center().y));
}

fn fills(output: &egui::FullOutput, color: egui::Color32) -> usize {
    output
        .shapes
        .iter()
        .filter(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.fill == color))
        .count()
}

#[test]
fn the_profile_menu_and_profile_options_toggle_it_per_profile_and_it_persists() {
    let mut d = Driver::new("actual-remaining-bottom", under_way());
    d.home();
    d.app.set_bottom(Some(BottomView::ActivityUsageProfile));
    d.frames(4);
    assert!(
        !d.app.panes.usage.stacked_activity,
        "activity profile: side by side"
    );
    open_profile_menu(&mut d);
    assert!(d.has("usage_profile_menu_actual_remaining"));
    d.click_at(d.rect("usage_profile_menu_actual_remaining").center());
    assert!(d.app.panes.usage.stacked_activity);
    assert!(
        d.app.panes.usage.stacked_resource,
        "the other keeps its own"
    );
    // With Stacked Histogram too, the actual part draws in the darker shade.
    d.app.panes.usage.stack_activity = true;
    d.frames(3);
    let split = d.frame(vec![]);
    let darker = stacked::actual_colors(0, false).0;
    assert!(fills(&split, darker) > 0, "actual part drawn darker");
    d.app.panes.usage.stacked_activity = false;
    d.frames(3);
    assert_eq!(fills(&d.frame(vec![]), darker), 0, "one block per series");
    // Profile Options: the check box is this profile's; Cancel keeps it.
    d.app.open_profile_options();
    d.frames(2);
    assert!(d.has("usage_profile_option_actual_remaining"));
    d.click("usage_profile_option_actual_remaining");
    d.click("usage_profile_options_cancel");
    assert!(!d.app.panes.usage.stacked_activity);
    d.app.open_profile_options();
    d.frames(2);
    d.click("usage_profile_option_actual_remaining");
    d.click("usage_profile_options_ok");
    assert!(d.app.panes.usage.stacked_activity);
    // The resource profile has its own.
    d.app.panes.usage.resource = Some(7);
    d.app.set_bottom(Some(BottomView::ResourceUsageProfile));
    d.frames(4);
    open_profile_menu(&mut d);
    d.click_at(d.rect("usage_profile_menu_actual_remaining").center());
    assert!(!d.app.panes.usage.stacked_resource);
    assert!(d.app.panes.usage.stacked_activity);
    // Saved with the layout.
    let json = d.app.panes.json();
    assert_eq!(json["usage"]["stacked_activity"], true);
    assert_eq!(json["usage"]["stacked_resource"], false);
    let mut again = PanesState::default();
    again.restore(&json);
    assert!(again.usage.stacked_activity && !again.usage.stacked_resource);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn the_resource_usage_profile_splits_its_stacked_histogram() {
    let mut d = Driver::new("actual-remaining-resources", under_way());
    d.home();
    d.app.panes.usage.resource = Some(7);
    d.app.set_bottom(Some(BottomView::ResourceUsageProfile));
    d.frames(4);
    assert!(d.app.panes.usage.stacked_resource, "one bar by default");
    // Stacked Histogram with it on: the actual part in the darker shade.
    d.app.panes.usage.stack_resource = true;
    d.frames(3);
    let darker = stacked::actual_colors(0, false).0;
    assert!(fills(&d.frame(vec![]), darker) > 0);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

/// Renders to the ignored `artifacts/batch4/`: the profile menu with Show
/// Actual and Remaining, the activity profile in one bar and side by side,
/// the Stacked Histogram split into actual and remaining, Profile Options,
/// and the Resource Usage Profile stacked and split.
#[test]
fn render_show_actual_and_remaining() {
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/batch4");
    std::fs::create_dir_all(&out).unwrap();
    let shot = |d: &mut Driver, name: &str| {
        d.frames(2);
        d.screenshot(&out.join(format!("{name}.png")));
    };
    let mut d = Driver::probed(
        "batch4-actual-remaining",
        under_way(),
        egui::Vec2::new(1500., 900.),
        Some(out.clone()),
    );
    d.home();
    d.app.set_bottom(Some(BottomView::ActivityUsageProfile));
    d.app.zoom_ladder(true);
    d.app.zoom_ladder(true);
    // Start the charts a few days before the activity under way started.
    let dd = d.app.project.model.project.data_date.unwrap();
    d.app.timeline_origin = dd - 10 * DAY;
    d.frames(3);
    shot(&mut d, "profile-side-by-side");
    open_profile_menu(&mut d);
    let hover = d.rect("usage_profile_menu_actual_remaining").center();
    let _ = d.frame(vec![egui::Event::PointerMoved(hover)]);
    shot(&mut d, "profile-menu-show-actual-and-remaining");
    d.key(egui::Key::Escape);
    d.app.panes.usage.stacked_activity = true;
    shot(&mut d, "profile-one-bar");
    d.app.panes.usage.stack_activity = true;
    shot(&mut d, "profile-stacked-split");
    d.app.panes.usage.stacked_activity = false;
    shot(&mut d, "profile-stacked-plain");
    d.app.open_profile_options();
    shot(&mut d, "profile-options");
    d.key(egui::Key::Escape);
    d.app.panes.usage.resource = Some(7);
    d.app.set_bottom(Some(BottomView::ResourceUsageProfile));
    d.app.panes.usage.stack_resource = true;
    d.app.panes.usage.stacked_resource = true;
    shot(&mut d, "resource-profile-stacked-split");
}
