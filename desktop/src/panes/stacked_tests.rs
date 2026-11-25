//! Stacked Histogram on the usage profiles (bottom pane activity and
//! resource profiles): the series
//! add up to the unstacked bars, the right-click menu toggles stacking and
//! picks Resource or Role, the legend names each series in its theme
//! color, and the choice persists with the layout.
use super::stacked::{self, StackBy};
use super::usage;
use super::*;
use crate::qa_sweep::Driver;
use crate::theme;
use crate::views::assignments as asg;
use crate::views::resources as rs;
use egui::Pos2;

/// The resource fixture (PIPE, 24 h on 1010) plus WELD, a second labor
/// resource with the Fitter role on 1010 (16 h) and 8 h on 1020.
pub(crate) fn two_crews() -> ProjectData {
    let mut p = rs::tests::with_resource();
    let d = &mut p.document;
    d.table_mut("RSRC").add(&[
        ("rsrc_id", "10".into()),
        ("clndr_id", "1".into()),
        ("rsrc_short_name", "WELD".into()),
        ("rsrc_name", "Welder".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("rsrc_seq_num", "5".into()),
    ]);
    d.table_mut("ROLES").add(&[
        ("role_id", "3".into()),
        ("role_short_name", "FIT".into()),
        ("role_name", "Fitter".into()),
    ]);
    for (id, task, qty, role) in [("2", "1010", "16", "3"), ("3", "1020", "8", "")] {
        d.table_mut("TASKRSRC").add(&[
            ("taskrsrc_id", id.into()),
            ("task_id", task.into()),
            ("proj_id", "1".into()),
            ("rsrc_id", "10".into()),
            ("role_id", role.into()),
            ("remain_qty", qty.into()),
            ("target_qty", qty.into()),
            ("remain_qty_per_hr", "1".into()),
            ("rollup_dates_flag", "N".into()),
        ]);
    }
    let mut p = ProjectData::from_document(p.document, 1, "two-crews".into()).unwrap();
    openxer_engine::scheduler::schedule_with_resources(&mut p.model, &p.resources).unwrap();
    p
}

fn days(first: i64, count: i64) -> Vec<(i64, i64)> {
    (0..count)
        .map(|d| (first + d * DAY, first + (d + 1) * DAY))
        .collect()
}

#[test]
fn series_split_the_unstacked_totals_by_resource_and_by_role() {
    let p = two_crews();
    let first = p
        .model
        .activities
        .iter()
        .filter_map(|a| a.early_start)
        .min()
        .unwrap();
    let periods = days(first.div_euclid(DAY) * DAY, 20);
    let by_task = usage::labor_assignments(&p);
    let all: Vec<&asg::Assignment> = by_task.values().flatten().collect();
    let total = usage::activity_usage_of(&p, &by_task, &periods, None, false);
    for by in [StackBy::Resource, StackBy::Role] {
        let series = stacked::series(&p, &all, &periods, false, by);
        for k in 0..periods.len() {
            let sum: f64 = series.iter().map(|s| s.values[k]).sum();
            let want = total.actual[k] + total.remaining[k];
            assert!(
                (sum - want).abs() < 1e-6,
                "{by:?} period {k}: {sum} vs {want}"
            );
        }
    }
    let labels = |by| -> Vec<String> {
        stacked::series(&p, &all, &periods, false, by)
            .into_iter()
            .map(|s| s.label)
            .collect()
    };
    // Largest first: PIPE 24 h, WELD 24 h (ties by name), roles: none 32, Fitter 16.
    assert_eq!(labels(StackBy::Resource), ["PIPE", "WELD"]);
    assert_eq!(labels(StackBy::Role), ["No role", "Fitter"]);
}

#[test]
fn series_beyond_the_palette_are_summed_into_other() {
    let mut p = two_crews();
    for n in 0..12 {
        let id = 100 + n;
        p.document.table_mut("RSRC").add(&[
            ("rsrc_id", id.to_string()),
            ("rsrc_short_name", format!("CREW{n:02}")),
            ("rsrc_type", "RT_Labor".into()),
        ]);
        p.document.table_mut("TASKRSRC").add(&[
            ("taskrsrc_id", (200 + n).to_string()),
            ("task_id", "1020".into()),
            ("proj_id", "1".into()),
            ("rsrc_id", id.to_string()),
            ("remain_qty", "1".into()),
            ("remain_qty_per_hr", "1".into()),
        ]);
    }
    let mut p = ProjectData::from_document(p.document, 1, "many".into()).unwrap();
    openxer_engine::scheduler::schedule_with_resources(&mut p.model, &p.resources).unwrap();
    let periods = days(-100 * DAY, 400);
    let list = asg::assignments(&p);
    let refs: Vec<&asg::Assignment> = list.iter().collect();
    let series = stacked::series(&p, &refs, &periods, false, StackBy::Resource);
    assert_eq!(series.len(), theme::STACK.len());
    let other = series.last().unwrap();
    assert!(
        other.other && other.label.starts_with("Other ("),
        "{}",
        other.label
    );
    let sum: f64 = series.iter().flat_map(|s| s.values.iter()).sum();
    assert!((sum - (24. + 24. + 12.)).abs() < 1e-6, "{sum}");
}

fn secondary(d: &mut Driver, pos: Pos2) {
    d.press_at(pos, egui::PointerButton::Secondary, Default::default());
}

fn text_shown(output: &egui::FullOutput, text: &str) -> bool {
    output
        .shapes
        .iter()
        .any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == text))
}

fn fills(output: &egui::FullOutput, color: egui::Color32) -> usize {
    output
        .shapes
        .iter()
        .filter(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.fill == color))
        .count()
}

fn open_profile_menu(d: &mut Driver) {
    let body = d.app.panes.body;
    secondary(d, Pos2::new(body.center().x + 80., body.center().y));
}

#[test]
fn the_profile_menu_stacks_each_bottom_profile_by_resource_or_role_and_persists() {
    let mut d = Driver::new("stacked-bottom", two_crews());
    d.home();
    d.app.set_bottom(Some(BottomView::ActivityUsageProfile));
    d.frames(4);
    let plain = d.frame(vec![]);
    assert!(fills(&plain, usage::REMAINING.0) > 0);
    assert!(!text_shown(&plain, "WELD"));
    open_profile_menu(&mut d);
    assert!(d.has("usage_profile_menu_stack_by"));
    d.click_at(d.rect("usage_profile_menu_stacked").center());
    assert!(d.app.panes.usage.stack_activity);
    assert!(
        !d.app.panes.usage.stack_resource,
        "each profile has its own"
    );
    d.frames(3);
    let on = d.frame(vec![]);
    let (first, second) = (stacked::colors(0, false).0, stacked::colors(1, false).0);
    assert!(
        fills(&on, first) > 0 && fills(&on, second) > 0,
        "two series stacked"
    );
    for label in ["By Resource", "PIPE", "WELD"] {
        assert!(text_shown(&on, label), "legend {label}");
    }
    // Stack By > Role.
    open_profile_menu(&mut d);
    d.click_at(d.rect("usage_profile_menu_stack_by").center());
    d.frames(2);
    d.click_at(d.rect("usage_profile_menu_stack_by_role").center());
    assert_eq!(d.app.panes.usage.stack_by, StackBy::Role);
    d.frames(3);
    let roles = d.frame(vec![]);
    for label in ["By Role", "Fitter", "No role"] {
        assert!(text_shown(&roles, label), "legend {label}");
    }
    // The resource profile stacks the selected resource's type.
    d.app.panes.usage.resource = Some(7);
    d.app.set_bottom(Some(BottomView::ResourceUsageProfile));
    d.frames(4);
    open_profile_menu(&mut d);
    d.click_at(d.rect("usage_profile_menu_stacked").center());
    assert!(d.app.panes.usage.stack_resource);
    d.frames(3);
    let resource = d.frame(vec![]);
    assert!(text_shown(&resource, "Fitter"));
    // Saved with the layout.
    let json = d.app.panes.json();
    assert_eq!(json["usage"]["stack_activity"], true);
    assert_eq!(json["usage"]["stack_resource"], true);
    assert_eq!(json["usage"]["stack_by"], "role");
    let mut again = PanesState::default();
    again.restore(&json);
    assert_eq!(again.usage.stack_by, StackBy::Role);
    assert!(again.usage.stack_activity && again.usage.stack_resource);
    // Off again: the plain bars come back.
    open_profile_menu(&mut d);
    d.click_at(d.rect("usage_profile_menu_stacked").center());
    assert!(!d.app.panes.usage.stack_resource);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}
