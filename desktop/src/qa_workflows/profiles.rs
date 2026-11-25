//! Workflow: the usage profiles' options (Profile Options, Data and Graph
//! tabs) in the Activities bottom layout.
//!
//! - View > Show on Bottom > Activity Usage Profile, one period a day, on a
//!   project whose one labor assignment holds 36 hours over three days,
//!   with 20 hours typed into the second day (manual period values, runs
//!   `8:8;20:8;8:8`); its budget spreads evenly, 12 a day.
//! - The right-click menu lists the reference's items in its order, then
//!   OpenXER's own.
//! - Data tab: each series switched on and off by itself (By Date: its
//!   bars; Cumulative: its running-total curve); the bars per period and
//!   the running totals' end equal the assignment's totals; the manual
//!   value shows in its own period (Show Values writes each bar's value).
//! - A series color changed through the shared color picker draws its bars
//!   in that color. Cancel and Escape change nothing; Apply applies and
//!   keeps the dialog open.
//! - Graph tab: Calculate Average with the work hours of each period
//!   divides each day's bar by 8.
//! - The Resource Usage Profile has its own series: budgeted and remaining
//!   late bars for the selected resource, adding up to the same totals.
//! - Everything survives saving and reopening the layout.
//!
//! The options are view settings, not project data: no undo step is made
//! and the project is checked unchanged at the end.
use super::*;
use crate::panes::profile_options::Series;
use crate::panes::usage::{self, Interval};
use crate::panes::BottomView;
use openxer_xer::manual_units;

/// `with_resource` (the Pipefitter on activity 1010, Monday to Wednesday)
/// with 36 hours, 20 of them typed into the Tuesday.
fn project() -> ProjectData {
    let mut p = crate::views::resources::tests::with_resource();
    let t = p.document.table_mut("TASKRSRC");
    t.set(0, "target_qty", "36".into());
    t.set(0, "remain_qty", "36".into());
    // The same three days: 1.5 an hour on average.
    t.set(0, "remain_qty_per_hr", "1.5".into());
    t.set(0, manual_units::REMAINING, "8:8;20:8;8:8".into());
    p
}

fn secondary(d: &mut Driver, pos: egui::Pos2) {
    d.press_at(pos, egui::PointerButton::Secondary, Default::default());
}

fn text_shown(output: &egui::FullOutput, text: &str) -> bool {
    texts(output, text) > 0
}

/// How many times `text` is drawn.
fn texts(output: &egui::FullOutput, text: &str) -> usize {
    output
        .shapes
        .iter()
        .filter(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == text))
        .count()
}

/// Rectangles filled with `color` inside the chart body.
fn fills(output: &egui::FullOutput, color: egui::Color32, body: egui::Rect) -> usize {
    output
        .shapes
        .iter()
        .filter(|s| {
            matches!(&s.shape, egui::Shape::Rect(r) if r.fill == color && body.contains_rect(r.rect))
        })
        .count()
}

/// Two-pixel strokes: the running-total curves.
fn curve_colors(output: &egui::FullOutput) -> Vec<egui::Color32> {
    let mut out = vec![];
    for clipped in &output.shapes {
        let mut visit = |shape: &egui::Shape| {
            if let egui::Shape::Path(p) = shape {
                if let egui::epaint::ColorMode::Solid(c) = &p.stroke.color {
                    if p.stroke.width == 2. {
                        out.push(*c);
                    }
                }
            }
        };
        match &clipped.shape {
            egui::Shape::Vec(list) => list.iter().for_each(&mut visit),
            shape => visit(shape),
        }
    }
    out
}

/// The profile's right-click menu at the middle of its chart, and the
/// labels of its top level in order (separators as "-").
fn menu_labels(d: &mut Driver) -> Vec<String> {
    let body = d.app.panes.body;
    secondary(d, egui::Pos2::new(body.center().x + 60., body.center().y));
    d.frames(1);
    crate::context_menu::drawn(&d.ctx)
        .into_iter()
        .filter(|e| e.prefix == "usage_profile_menu" && e.depth == 0)
        .map(|e| {
            if e.separator {
                "-".to_string()
            } else {
                e.label
            }
        })
        .collect()
}

/// Right-click the profile, Profile Options....
fn open_options(d: &mut Driver) {
    let body = d.app.panes.body;
    secondary(d, egui::Pos2::new(body.center().x + 60., body.center().y));
    d.click_at(d.rect("usage_profile_menu_options").center());
    d.frames(2);
    assert!(
        d.app.panes.profile_dialog.is_some(),
        "Profile Options opens"
    );
}

/// The working copy of the open Profile Options.
fn draft(d: &Driver) -> &usage::ProfileOptions {
    &d.app.panes.profile_dialog.as_ref().unwrap().options
}

/// Each series' values over `periods` and in total (before, the periods
/// and after) of the activity profile.
fn activity_series(d: &Driver, periods: &[(i64, i64)]) -> Vec<(Vec<f64>, f64)> {
    let u = d.app.activity_profile_usage(periods);
    [&u.budget, &u.actual, &u.remaining, &u.remaining_late]
        .into_iter()
        .map(|v| {
            let (_, inner, _) = usage::unedge(v);
            (inner, v.iter().sum())
        })
        .collect()
}

#[test]
fn workflow_profile_options_switch_series_colors_and_graph() {
    let mut d = Driver::new("qa_profiles", project());
    d.home();
    let steps = d.app.history.counts();
    let document = format!("{:?}", d.app.project.document.tables);

    // View > Show on Bottom > Activity Usage Profile.
    d.click("menu_bar_view");
    assert!(d.hover_text("Show on Bottom"));
    d.click("menu_bottom_activity_usage_profile");
    d.frames(3);
    assert_eq!(d.app.panes.bottom, BottomView::ActivityUsageProfile);
    d.app.panes.usage.interval = Interval::Day;
    d.frames(3);

    // The menu: the reference's items in its order, OpenXER's after.
    let labels = menu_labels(&mut d);
    let want = [
        "Show All Projects",
        "-",
        "Stacked Histogram",
        "-",
        "Timescale...",
        "Profile Options...",
        "User Preferences...",
        "-",
        "Zoom In",
        "Zoom Out",
        "-",
        "Stack By",
        "Show Actual and Remaining",
        "Show Usage for",
        "-",
        "Display",
    ];
    assert_eq!(labels, want, "the profile menu");
    d.key(Key::Escape);
    d.frames(2);

    // The plotted data: 36 budgeted hours spread evenly, the remaining
    // 8, 20, 8 with the typed Tuesday, the late remaining also 36.
    let periods = d.app.usage_export_periods();
    let series = activity_series(&d, &periods);
    for (k, total) in [(0, 36.), (1, 0.), (2, 36.), (3, 36.)] {
        assert!(
            (series[k].1 - total).abs() < 1e-6,
            "series {k}: {:?}",
            series
        );
    }
    let busy: Vec<f64> = series[2].0.iter().copied().filter(|v| *v > 1e-9).collect();
    assert_eq!(busy, [8., 20., 8.], "the manual value in its own day");
    let budget: Vec<f64> = series[0].0.iter().copied().filter(|v| *v > 1e-9).collect();
    assert_eq!(budget, [12., 12., 12.], "the budget spreads evenly");

    // Data tab: only Remaining Early as bars, with its curve, and Show
    // Values on the Graph tab, all through the dialog.
    open_options(&mut d);
    assert!(d.has("usage_profile_tab_data") && d.has("usage_profile_tab_graph"));
    for s in [Series::Budgeted, Series::Actual, Series::RemainingLate] {
        if draft(&d).activity[s.index()].by_date {
            d.click(&format!("usage_profile_series_{}_by_date", s.key()));
        }
    }
    d.click("usage_profile_series_remaining_early_cumulative");
    assert!(
        !d.has("usage_profile_graph_values"),
        "the Graph tab's controls show on the Graph tab only"
    );
    d.click("usage_profile_tab_graph");
    d.click("usage_profile_graph_values");
    // Nothing changes before OK.
    assert!(!d.app.panes.usage.profile.graph.values);
    d.click("usage_profile_options_ok");
    d.frames(3);
    assert!(d.app.panes.profile_dialog.is_none(), "OK closes it");
    let o = d.app.panes.usage.profile.clone();
    let on: Vec<(bool, bool)> = o.activity[..4]
        .iter()
        .map(|s| (s.by_date, s.cumulative))
        .collect();
    assert_eq!(
        on,
        [(false, false), (false, false), (true, true), (false, false)]
    );
    let body = d.app.panes.body;
    let drawn = d.frame(vec![]);
    assert!(
        fills(&drawn, usage::REMAINING.0, body) > 0,
        "remaining bars"
    );
    for other in [usage::BUDGET.0, usage::ACTUAL.0, usage::REMAINING_LATE.0] {
        assert_eq!(fills(&drawn, other, body), 0, "{other:?} switched off");
    }
    assert_eq!(
        curve_colors(&drawn),
        [usage::REMAINING.1],
        "one curve, remaining early's"
    );
    let with_values = drawn;
    assert!(
        text_shown(&with_values, "Remaining Labor"),
        "the legend names it"
    );
    assert!(!text_shown(&with_values, "Budgeted Labor"));

    // A color through the shared picker: the remaining bars take it.
    open_options(&mut d);
    let swatch = d.rect("usage_profile_series_remaining_early_color");
    d.click("usage_profile_series_remaining_early_color");
    d.click_at(egui::Pos2::new(swatch.left() + 12., swatch.bottom() + 12.));
    let picked = draft(&d).activity[Series::RemainingEarly.index()].color;
    assert_ne!(
        picked.color(),
        usage::REMAINING.0,
        "a basic color was picked"
    );
    // Apply applies and keeps the dialog open.
    d.click("usage_profile_options_apply");
    assert!(d.app.panes.profile_dialog.is_some(), "Apply keeps it open");
    assert_eq!(
        d.app.panes.usage.profile.activity[Series::RemainingEarly.index()].color,
        picked
    );
    // Show Values off again: the bars' values go, the typed 20 on its own
    // bar and 8 on the two others.
    d.click("usage_profile_tab_graph");
    d.click("usage_profile_graph_values");
    d.click("usage_profile_options_ok");
    d.frames(3);
    let drawn = d.frame(vec![]);
    assert!(
        fills(&drawn, picked.color(), body) > 0,
        "drawn in the color"
    );
    assert_eq!(fills(&drawn, usage::REMAINING.0, body), 0);
    let (twenty, eight) = (format::units(20.), format::units(8.));
    assert_eq!(texts(&with_values, &twenty), texts(&drawn, &twenty) + 1);
    assert_eq!(texts(&with_values, &eight), texts(&drawn, &eight) + 2);

    // Cancel and Escape change nothing.
    let kept = d.app.panes.usage.profile.clone();
    for close in ["cancel", "escape"] {
        open_options(&mut d);
        d.click("usage_profile_series_budgeted_by_date");
        d.click("usage_profile_tab_graph");
        d.click("usage_profile_graph_3d");
        if close == "cancel" {
            d.click("usage_profile_options_cancel");
        } else {
            d.key(Key::Escape);
        }
        d.frames(2);
        assert!(d.app.panes.profile_dialog.is_none(), "{close} closes");
        assert_eq!(d.app.panes.usage.profile, kept, "{close} changes nothing");
    }

    // Graph tab: Calculate Average over the work hours of each day, with
    // the values shown, then without them.
    open_options(&mut d);
    d.click("usage_profile_tab_graph");
    d.click("usage_profile_graph_values");
    d.click("usage_profile_graph_average");
    d.click("usage_profile_graph_hours");
    d.click("usage_profile_options_ok");
    d.frames(3);
    let g = &d.app.panes.usage.profile.graph;
    assert!(g.average && g.hours_per_period && g.values);
    assert_eq!(
        g.divisors(&periods[..1]),
        Some(vec![8.]),
        "a day's work hours"
    );
    let averaged = d.frame(vec![]);
    open_options(&mut d);
    d.click("usage_profile_tab_graph");
    d.click("usage_profile_graph_values");
    d.click("usage_profile_options_ok");
    d.frames(3);
    let plain = d.frame(vec![]);
    let (tuesday, other) = (format::units(2.5), format::units(1.));
    assert_eq!(
        texts(&averaged, &tuesday),
        texts(&plain, &tuesday) + 1,
        "20 / 8"
    );
    assert_eq!(texts(&averaged, &other), texts(&plain, &other) + 2, "8 / 8");

    // The Resource Usage Profile keeps its own series: switch on its
    // budget and remaining late bars.
    d.app.panes.usage.resource = Some(7);
    d.click("menu_bar_view");
    assert!(d.hover_text("Show on Bottom"));
    d.click("menu_bottom_resource_usage_profile");
    d.frames(3);
    assert_eq!(d.app.panes.bottom, BottomView::ResourceUsageProfile);
    open_options(&mut d);
    assert!(
        !d.has("usage_profile_series_project_baseline_by_date"),
        "no baselines on the resource profile"
    );
    for s in [Series::Budgeted, Series::RemainingLate] {
        if !draft(&d).resource[s.index()].by_date {
            d.click(&format!("usage_profile_series_{}_by_date", s.key()));
        }
    }
    d.click("usage_profile_options_ok");
    d.frames(3);
    let body = d.app.panes.body;
    let drawn = d.frame(vec![]);
    assert!(fills(&drawn, usage::BUDGET.0, body) > 0, "budget bars");
    assert!(
        fills(&drawn, usage::REMAINING_LATE.0, body) > 0,
        "remaining late bars"
    );
    assert!(text_shown(&drawn, "Remaining Late Units"));
    let plan = d.app.resource_plan_usage(7, &periods);
    assert!((plan.0.iter().sum::<f64>() - 36.).abs() < 1e-6, "budget");
    assert!((plan.1.iter().sum::<f64>() - 36.).abs() < 1e-6, "late");
    // The activity profile's series were left as they were.
    let mut activity = o.activity;
    activity[Series::RemainingEarly.index()].color = picked;
    assert_eq!(d.app.panes.usage.profile.activity, activity);

    // Saved with the layout and reopened.
    let saved = d.app.panes.usage.profile.clone();
    d.app.save_layout_as("Profiles").unwrap();
    d.app.panes.usage = Default::default();
    assert_ne!(d.app.panes.usage.profile, saved);
    d.app.open_layout("Profiles", true).unwrap();
    assert_eq!(d.app.panes.usage.profile, saved, "the layout keeps them");

    // View settings only: no undo step, the project unchanged.
    assert_eq!(d.app.history.counts(), steps, "no undo step");
    assert_eq!(
        format!("{:?}", d.app.project.document.tables),
        document,
        "the project is unchanged"
    );
    d.assert_clean("profile options");
}
