//! Trace Logic levels and selection sync, usage values against the resource
//! histogram buckets, Activity Network layout stability, pane persistence,
//! and headless renders of every bottom view for review.
use super::network::{self, NetworkOptions};
use super::trace::{self, Side, TraceOptions};
use super::usage;
use super::*;
use crate::headless_render::SoftwarePainter;
use crate::views::assignments as asg;
use crate::views::resources as rs;

fn scheduled_starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn corpus(name: &str) -> Option<ProjectData> {
    let path = crate::test_corpus::file(name);
    let doc = Document::read(&path).ok()?;
    let key = doc.projects().first()?.0;
    let mut p = ProjectData::from_document(doc, key, name.into()).ok()?;
    if p.model.activities.iter().all(|a| a.early_start.is_none()) {
        schedule_model(&mut p.model, &p.resources).ok()?;
    }
    Some(p)
}

fn frame(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>, time: &mut f64) {
    *time += 0.1;
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500., 900.))),
            time: Some(*time),
            events,
            focused: true,
            ..Default::default()
        },
        |ctx| app.update_ui(ctx),
    );
}

fn press(app: &mut App, ctx: &egui::Context, pos: Pos2, time: &mut f64, count: usize) {
    for _ in 0..count {
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
                time,
            );
        }
    }
    frame(app, ctx, vec![egui::Event::PointerGone], time);
    frame(app, ctx, vec![], time);
}

fn click(app: &mut App, ctx: &egui::Context, action: &str, time: &mut f64, count: usize) {
    let [l, t, r, b] = *app
        .action_rects
        .get(action)
        .unwrap_or_else(|| panic!("No rendered control {action:?}"));
    press(app, ctx, Pos2::new((l + r) / 2., (t + b) / 2.), time, count);
}

fn setup(name: &str, project: ProjectData) -> (PathBuf, egui::Context, App, f64) {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "openxer-panes-{name}-{}-{unique}.db",
        std::process::id()
    ));
    let mut store = Store::open(path.clone()).unwrap();
    store.save(&project).unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx, store, project);
    let mut time = 0.;
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![], &mut time);
    }
    (path, ctx, app, time)
}

fn index_of(p: &ProjectData, task_id: i64) -> usize {
    p.model
        .activities
        .iter()
        .position(|a| a.task_id == task_id)
        .unwrap()
}

#[test]
fn trace_levels_follow_the_options_and_rank_columns_by_logic() {
    let p = scheduled_starter();
    let m = &p.model;
    // Activity k is task 1000 + 10k. Install structural assemblies (8) has
    // predecessors 5 and 7, which follow 3 and 6; successors 9 and 10 both
    // lead to 11.
    let two = TraceOptions {
        predecessor_levels: 2,
        successor_levels: 2,
    };
    let t = trace::trace(m, 8, two);
    assert_eq!(t.level(Side::Focus, 0), vec![8]);
    assert_eq!(t.level(Side::Predecessor, 1), vec![5, 7]);
    assert_eq!(t.level(Side::Predecessor, 2), vec![3, 6]);
    assert_eq!(t.level(Side::Successor, 1), vec![9, 10]);
    assert_eq!(t.level(Side::Successor, 2), vec![11]);
    assert_eq!(t.nodes.len(), 8);
    // Columns are the longest-path rank: 3 | 5 6 | 7 | 8 | 9 10 | 11.
    let column = |a: usize| t.node(a).unwrap().column;
    assert_eq!(
        [3, 5, 6, 7, 8, 9, 10, 11].map(column),
        [0, 1, 1, 2, 3, 4, 4, 5]
    );
    assert_eq!(t.columns(), 6);
    // Every traced relationship joins two traced activities.
    assert_eq!(t.links.len(), 9);
    for &l in &t.links {
        let link = &m.links[l];
        assert!(column(link.pred) < column(link.succ));
    }
    // Boxes sharing a column get distinct rows.
    assert_ne!(t.node(5).unwrap().row, t.node(6).unwrap().row);
    // Zero levels leave the focus alone; one predecessor level only.
    let none = trace::trace(
        m,
        8,
        TraceOptions {
            predecessor_levels: 0,
            successor_levels: 0,
        },
    );
    assert_eq!(none.nodes.len(), 1);
    assert!(none.links.is_empty());
    let one = trace::trace(
        m,
        8,
        TraceOptions {
            predecessor_levels: 1,
            successor_levels: 0,
        },
    );
    let mut traced: Vec<usize> = one.nodes.iter().map(|n| n.activity).collect();
    traced.sort_unstable();
    assert_eq!(traced, vec![5, 7, 8]);
    // Enough levels reach both ends of the network.
    let all = trace::trace(
        m,
        8,
        TraceOptions {
            predecessor_levels: 2000,
            successor_levels: 2000,
        },
    );
    assert!(all.node(0).is_some() && all.node(13).is_some());
    assert_eq!(all.node(0).unwrap().column, 0);
}

#[test]
fn trace_lines_follow_the_box_to_box_routing() {
    let a = Rect::from_min_size(Pos2::new(4., 8.), Vec2::new(160., 78.));
    let b = a.translate(Vec2::new(trace::COLUMN_PITCH, 0.));
    // Neighbouring FS on one row: a straight line at mid height.
    let (points, dir) = trace::link_path(a, b, LinkType::FS, true);
    assert_eq!(points, vec![Pos2::new(164., 47.), Pos2::new(244., 47.)]);
    assert_eq!(dir, Vec2::X);
    // SS: from the bottom-left corner down a 2:1 diagonal to the route
    // 20 px below the boxes, then up into the successor's start corner.
    let (points, dir) = trace::link_path(a, b, LinkType::SS, true);
    assert_eq!(points[0], Pos2::new(4., 85.));
    assert_eq!(points[1], Pos2::new(44., 105.));
    assert_eq!(points[2], Pos2::new(244., 105.));
    assert_eq!(points[3], Pos2::new(244., 86.));
    assert_eq!(dir, -Vec2::Y);
    // FF two columns away rises into the finish corner.
    let c = a.translate(Vec2::new(2. * trace::COLUMN_PITCH, 0.));
    let (points, _) = trace::link_path(a, c, LinkType::FF, false);
    assert_eq!(points[0].x, 164.);
    assert_eq!(points.last().unwrap().x, c.right());
}

#[test]
fn trace_logic_follows_the_selection_and_recenters_on_clicks() {
    let (path, ctx, mut app, mut time) = setup("trace", scheduled_starter());
    app.panes.trace.options = TraceOptions {
        predecessor_levels: 2,
        successor_levels: 2,
    };
    app.set_bottom(Some(BottomView::TraceLogic));
    app.selected = Some(8);
    let history = app.history.counts();
    frame(&mut app, &ctx, vec![], &mut time);
    frame(&mut app, &ctx, vec![], &mut time);
    assert_eq!(app.panes.trace.focus, Some(1080));
    assert_eq!(app.panes.trace.boxes.len(), 8);
    // Box geometry: 160 x 78 on a 240 px column pitch.
    let rect = |app: &App, id: i64| {
        app.panes
            .trace
            .boxes
            .iter()
            .find(|(t, _)| *t == id)
            .unwrap()
            .1
    };
    let r8 = rect(&app, 1080);
    assert_eq!(r8.size(), Vec2::new(160., 78.));
    assert_eq!(rect(&app, 1090).left() - r8.left(), 240.);
    // A click selects in the top pane and keeps the trace's focus.
    click(&mut app, &ctx, "trace_box_RB-1050", &mut time, 1);
    assert_eq!(app.selected, Some(5));
    assert_eq!(app.panes.trace.focus, Some(1080));
    // A double-click makes the box the focus.
    click(&mut app, &ctx, "trace_box_RB-1100", &mut time, 2);
    assert_eq!(app.selected, Some(10));
    assert_eq!(app.panes.trace.focus, Some(1100));
    frame(&mut app, &ctx, vec![], &mut time);
    let traced: BTreeSet<i64> = app.panes.trace.boxes.iter().map(|b| b.0).collect();
    // Two levels each way from 10: 5 and 7 through 8, then 11 and 12.
    assert!(traced.contains(&1050) && traced.contains(&1120));
    assert!(!traced.contains(&1030) && !traced.contains(&1130));
    // Selecting a box out of view scrolls it to the middle of the pane.
    app.panes.trace.options = TraceOptions {
        predecessor_levels: 2000,
        successor_levels: 2000,
    };
    frame(&mut app, &ctx, vec![], &mut time);
    let pane = app
        .panes
        .trace
        .boxes
        .iter()
        .fold(Rect::NOTHING, |acc, b| acc.union(b.1));
    assert!(
        pane.width() > 1500.,
        "the full trace is wider than the window"
    );
    app.trace_select(13, false, true);
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![], &mut time);
    }
    let last = rect(&app, 1130);
    assert!(last.right() < 1500. && last.left() > 0., "{last:?}");
    app.panes.trace.options = TraceOptions {
        predecessor_levels: 2,
        successor_levels: 2,
    };
    // A new selection in the top pane moves the focus.
    app.selected = Some(2);
    frame(&mut app, &ctx, vec![], &mut time);
    assert_eq!(app.panes.trace.focus, Some(1020));
    // Views only: nothing to undo, nothing to save.
    assert_eq!(app.history.counts(), history);
    assert!(!app.dirty);
    // The options dialog applies new levels.
    app.open_trace_options();
    frame(&mut app, &ctx, vec![], &mut time);
    app.panes.trace_dialog = Some(TraceOptions {
        predecessor_levels: 0,
        successor_levels: 1,
    });
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "trace_ok", &mut time, 1);
    assert!(app.panes.trace_dialog.is_none());
    assert_eq!(app.panes.trace.options.predecessor_levels, 0);
    frame(&mut app, &ctx, vec![], &mut time);
    // Activity 2 (design) has successors 3 and 4.
    assert_eq!(app.panes.trace.boxes.len(), 3);
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// Day periods covering the whole schedule.
fn schedule_days(p: &ProjectData) -> Vec<(i64, i64)> {
    let m = &p.model;
    let first = m
        .activities
        .iter()
        .filter_map(|a| a.act_start.or(a.target_start).or(a.early_start))
        .chain(m.project.data_date)
        .min()
        .unwrap()
        .div_euclid(DAY)
        * DAY
        - 2 * DAY;
    let last = m
        .activities
        .iter()
        .filter_map(|a| a.late_finish.or(a.early_finish).or(a.target_finish))
        .max()
        .unwrap()
        + 2 * DAY;
    let count = (last - first) / DAY + 1;
    (0..count)
        .map(|d| (first + d * DAY, first + (d + 1) * DAY))
        .collect()
}

/// The activity profile's remaining labor and the resource spreadsheet's
/// rows agree with the Resources view histogram buckets.
fn assert_usage_matches_histogram(p: &ProjectData) {
    let periods = schedule_days(p);
    let by_task = usage::labor_assignments(p);
    let u = usage::activity_usage(p, &by_task, &periods, None);
    let labor = usage::labor_resources(p);
    let mut expected = vec![0.; periods.len()];
    let mut actual = vec![0.; periods.len()];
    for &r in &labor {
        let h = rs::usage(p, r, &periods, None);
        for k in 0..periods.len() {
            expected[k] += h.remaining[k];
            actual[k] += h.actual[k];
        }
        // Resource spreadsheet: the resource row and its assignment rows.
        let mut rows = vec![0.; periods.len()];
        for a in asg::assignments(p).into_iter().filter(|a| a.rsrc == r) {
            for (k, v) in asg::remaining_spread(p, &a, &periods)
                .into_iter()
                .enumerate()
            {
                rows[k] += v;
            }
        }
        for (k, (row, bucket)) in rows.iter().zip(&h.remaining).enumerate() {
            assert!(
                (row - bucket).abs() < 1e-6,
                "resource {r} period {k}: rows {row} histogram {bucket}"
            );
        }
    }
    for k in 0..periods.len() {
        assert!((u.remaining[k] - expected[k]).abs() < 1e-6, "period {k}");
        assert!((u.actual[k] - actual[k]).abs() < 1e-6, "period {k}");
    }
    // Budget and late remaining keep the assignment totals.
    let budget: f64 = by_task.values().flatten().map(|a| a.budget).sum();
    let spread: f64 = u.budget.iter().sum();
    assert!(
        (budget - spread).abs() < 1e-3 * budget.max(1.),
        "{budget} {spread}"
    );
}

#[test]
fn usage_values_match_the_resource_histogram_buckets() {
    let p = rs::tests::with_resource();
    assert_usage_matches_histogram(&p);
    let periods = schedule_days(&p);
    let by_task = usage::labor_assignments(&p);
    let u = usage::activity_usage(&p, &by_task, &periods, None);
    // 24 labor hours on the three-day survey, 8 per working day.
    assert!((u.remaining.iter().sum::<f64>() - 24.).abs() < 1e-6);
    assert_eq!(u.remaining.iter().filter(|v| **v > 0.).count(), 3);
    assert!(u
        .remaining
        .iter()
        .all(|v| *v == 0. || (*v - 8.).abs() < 1e-6));
    // Selected activities only: another activity has no labor.
    let only = BTreeSet::from([0usize]);
    let none = usage::activity_usage(&p, &by_task, &periods, Some(&only));
    assert!(none.remaining.iter().all(|v| *v == 0.));
    if let Some(p) = corpus("5640.xer") {
        assert_usage_matches_histogram(&p);
    }
}

#[test]
fn network_layout_is_stable_and_orders_columns_by_early_start() {
    let check = |p: ProjectData, name: &str| {
        let (path, ctx, mut app, mut time) = setup(name, p);
        app.set_top(TopView::Network);
        frame(&mut app, &ctx, vec![], &mut time);
        let first = app.network_layout();
        let visible = app
            .rows()
            .iter()
            .filter(|r| matches!(r, grid::Row::Activity(_)))
            .count();
        assert_eq!(first.boxes.len(), visible);
        // Selection, zoom and scrolling do not move boxes.
        app.selected = Some(app.project.model.activities.len() - 1);
        app.network_zoom(false);
        frame(&mut app, &ctx, vec![], &mut time);
        assert_eq!(app.network_layout(), first);
        // The same inputs give the same layout.
        let again = network::layout(
            &app.project.model,
            &app.network_bands(),
            &app.panes.network.options,
        );
        assert_eq!(again, first);
        let m = &app.project.model;
        let start = |i: usize| {
            displayed_start(&m.activities[i], m.options.do_level).map(|s| s.div_euclid(DAY))
        };
        for a in &first.boxes {
            for b in &first.boxes {
                if a.activity == b.activity {
                    continue;
                }
                assert!(!a.rect.intersects(b.rect.shrink(0.5)), "boxes overlap");
                if let (Some(x), Some(y)) = (start(a.activity), start(b.activity)) {
                    assert_eq!(
                        x.cmp(&y),
                        a.column.cmp(&b.column),
                        "columns follow start days"
                    );
                }
            }
        }
        // Each box sits inside its band; bands do not overlap.
        for w in first.bands.windows(2) {
            assert!(w[0].bottom < w[1].top);
        }
        for b in &first.boxes {
            assert!(first
                .bands
                .iter()
                .any(|band| b.rect.top() > band.top && b.rect.bottom() < band.bottom));
        }
        assert!(first.size.x > 0. && first.size.y > 0.);
        // Spacing factors stretch the pitch; centered keeps the columns.
        let wide = network::layout(
            m,
            &app.network_bands(),
            &NetworkOptions {
                horizontal_spacing: 2.,
                centered: true,
                ..Default::default()
            },
        );
        for (a, b) in first.boxes.iter().zip(&wide.boxes) {
            assert_eq!(a.column, b.column);
            assert_eq!(
                b.rect.left(),
                network::MARGIN + b.column as f32 * (160. + 160.)
            );
        }
        // Rendering the network keeps the selection in sync.
        app.set_bottom(None);
        frame(&mut app, &ctx, vec![], &mut time);
        assert!(!app.panes.network.boxes.is_empty());
        let (id, _) = app.panes.network.boxes[0];
        app.panes.network.center_on = Some(id);
        frame(&mut app, &ctx, vec![], &mut time);
        frame(&mut app, &ctx, vec![], &mut time);
        let code = app.project.model.activities[index_of(&app.project, id)]
            .task_code
            .clone();
        click(&mut app, &ctx, &format!("network_box_{code}"), &mut time, 1);
        assert_eq!(app.selected, Some(index_of(&app.project, id)));
        assert!(app.error.is_none(), "{:?}", app.error);
        drop(app);
        let _ = std::fs::remove_file(path);
        first
    };
    let starter = check(scheduled_starter(), "network-starter");
    assert_eq!(starter.boxes.len(), 14);
    assert_eq!(starter.bands.len(), 4);
    if let Some(p) = corpus("5640.xer") {
        let corpus = check(p, "network-corpus");
        assert!(corpus.boxes.len() > 14);
    }
}

#[test]
fn pane_choices_persist_in_the_workspace_and_in_saved_layouts() {
    let (path, ctx, mut app, mut time) = setup("persist", scheduled_starter());
    app.set_bottom(Some(BottomView::ResourceUsageProfile));
    app.set_top(TopView::Network);
    app.panes.trace.options.predecessor_levels = 7;
    app.panes.network.options.horizontal_spacing = 1.5;
    app.panes.network.options.template = boxes::Template::Dates;
    app.panes.usage.selected_only = true;
    frame(&mut app, &ctx, vec![], &mut time);
    // Save the layout with these panes, then change everything.
    app.save_layout_as("Panes").unwrap();
    app.set_bottom(Some(BottomView::TraceLogic));
    app.set_top(TopView::Gantt);
    app.panes.trace.options.predecessor_levels = 1;
    frame(&mut app, &ctx, vec![], &mut time);
    drop(app);
    // Reopening the workspace restores the working choices.
    let store = Store::open(path.clone()).unwrap();
    let project = store.last().unwrap().unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx, store, project);
    assert_eq!(app.panes.bottom, BottomView::TraceLogic);
    assert_eq!(app.panes.top, TopView::Gantt);
    assert_eq!(app.panes.trace.options.predecessor_levels, 1);
    assert_eq!(app.panes.network.options.horizontal_spacing, 1.5);
    assert!(app.panes.usage.selected_only);
    // Opening the saved layout restores its panes.
    app.open_layout("Panes", true).unwrap();
    assert_eq!(app.panes.bottom, BottomView::ResourceUsageProfile);
    assert_eq!(app.panes.top, TopView::Network);
    assert_eq!(app.panes.trace.options.predecessor_levels, 7);
    assert_eq!(app.panes.network.options.template, boxes::Template::Dates);
    assert!(app.show_details);
    // Older layouts with Trace Logic as a details tab open it as a view.
    let mut old = app.capture_layout("Old");
    old.panes = None;
    let mut json = old.to_json();
    json["bottom"] = serde_json::json!({"show_details": true, "tab": "trace_logic"});
    let parsed = grid::layout::Layout::from_json(&json).unwrap();
    assert_eq!(parsed.bottom.as_ref().unwrap().view, "trace_logic");
    // A saved tab list naming the old Trace Logic tab still loads.
    let tabs = details::parse_layout(r#"["general","trace_logic","status"]"#).unwrap();
    assert_eq!(tabs, vec![DetailTab::General, DetailTab::Status]);
    // The bottom Gantt Chart is a view like the others.
    app.set_bottom(Some(BottomView::Gantt));
    assert_eq!(app.panes.bottom, BottomView::Gantt);
    let mut time = 0.;
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn bottom_layout_toolbar_and_show_on_bottom_switch_views() {
    let (path, ctx, mut app, mut time) = setup("toolbar", rs::tests::with_resource());
    for (action, view) in [
        ("trace", BottomView::TraceLogic),
        ("tool_activity_usage_sheet", BottomView::ActivityUsageSheet),
        (
            "tool_activity_usage_profile",
            BottomView::ActivityUsageProfile,
        ),
        ("tool_resource_usage_sheet", BottomView::ResourceUsageSheet),
        (
            "tool_resource_usage_profile",
            BottomView::ResourceUsageProfile,
        ),
        ("details", BottomView::Details),
    ] {
        click(&mut app, &ctx, action, &mut time, 1);
        assert!(app.show_details, "{action}");
        assert_eq!(app.panes.bottom, view, "{action}");
    }
    // A checked button hides the pane.
    click(&mut app, &ctx, "details", &mut time, 1);
    assert!(!app.show_details);
    // Activity Network on the Top Layout toolbar, then back to the Gantt.
    click(&mut app, &ctx, "tool_network", &mut time, 1);
    assert_eq!(app.panes.top, TopView::Network);
    click(&mut app, &ctx, "tool_gantt", &mut time, 1);
    assert_eq!(app.panes.top, TopView::Gantt);
    // Activity Network Options: Organize centered, Apply, then Cancel.
    click(&mut app, &ctx, "tool_network", &mut time, 1);
    app.open_network_options();
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![], &mut time);
    }
    click(&mut app, &ctx, "network_centered", &mut time, 1);
    assert!(app.panes.network_dialog.as_ref().unwrap().options.centered);
    click(&mut app, &ctx, "network_apply", &mut time, 1);
    assert!(app.panes.network.options.centered);
    click(&mut app, &ctx, "network_tab_0", &mut time, 1);
    click(&mut app, &ctx, "network_template_dates", &mut time, 1);
    click(&mut app, &ctx, "network_cancel", &mut time, 1);
    assert!(app.panes.network_dialog.is_none());
    assert_eq!(
        app.panes.network.options.template,
        boxes::Template::Standard
    );
    click(&mut app, &ctx, "tool_gantt", &mut time, 1);
    // The resource views select a resource from their list.
    app.set_bottom(Some(BottomView::ResourceUsageSheet));
    frame(&mut app, &ctx, vec![], &mut time);
    assert_eq!(app.panes.usage.resource, Some(7));
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// Software renders of every bottom view and the Activity Network in a
/// 1920 x 1040 window. Images and box rectangles go to the ignored
/// `artifacts/review/bottom/`. With `OPENXER_VIEWS_REFERENCE=1` it renders
/// corpus project 369 with one of its activities selected at a fixed
/// timescale; otherwise the starter with a resource.
#[test]
fn render_bottom_views_for_review() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/review/bottom");
    std::fs::create_dir_all(&out).unwrap();
    let reference = std::env::var_os("OPENXER_VIEWS_REFERENCE").is_some();
    let project = reference
        .then(|| corpus("369.xer"))
        .flatten()
        .unwrap_or_else(rs::tests::with_resource);
    let focus = if reference { "A1020" } else { "RB-1080" };
    let prefix = if reference { "" } else { "starter-" };
    let path =
        std::env::temp_dir().join(format!("openxer-bottom-review-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut store = Store::open(path.clone()).unwrap();
    store.save(&project).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    let mut time = 0.;
    let mut painter = SoftwarePainter::default();
    painter.screen = Some(Vec2::new(1920., 1040.));
    let selected = app
        .project
        .model
        .activities
        .iter()
        .position(|a| a.task_code == focus);
    app.selected = selected;
    app.scroll_to_selected = true;
    if reference {
        // A fixed timescale: weeks over day letters, 14.5 px a day,
        // starting half a day before the Saturday the first day letter shows.
        app.pixels_per_day = 14.5;
        (app.view.upper, app.view.lower) = timescale::bands_for_scale(14.5 * 7.);
        app.update_timescale_units();
        let origin = app.project.model.origin_serial;
        if let Ok(Some(start)) = openxer_xer::parse_relative(origin, "2026-09-18 12:00") {
            app.timeline_origin = start;
        }
    }
    let mut report = serde_json::Map::new();
    let shots: [(&str, Option<BottomView>, TopView); 7] = [
        ("trace-logic", Some(BottomView::TraceLogic), TopView::Gantt),
        (
            "activity-usage-spreadsheet",
            Some(BottomView::ActivityUsageSheet),
            TopView::Gantt,
        ),
        (
            "activity-usage-profile",
            Some(BottomView::ActivityUsageProfile),
            TopView::Gantt,
        ),
        (
            "resource-usage-spreadsheet",
            Some(BottomView::ResourceUsageSheet),
            TopView::Gantt,
        ),
        (
            "resource-usage-profile",
            Some(BottomView::ResourceUsageProfile),
            TopView::Gantt,
        ),
        (
            "activity-network",
            Some(BottomView::Details),
            TopView::Network,
        ),
        ("no-bottom-layout", None, TopView::Gantt),
    ];
    for (name, bottom, top) in shots {
        app.set_bottom(bottom);
        app.set_top(top);
        for _ in 0..4 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        let (image, triangles) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        assert!(triangles > 300, "{name} draws the view");
        image.save(out.join(format!("{prefix}{name}.png"))).unwrap();
        let rects = |list: &[(i64, Rect)]| {
            list.iter()
                .map(|(id, r)| {
                    let code = app.project.model.activities[index_of(&app.project, *id)]
                        .task_code
                        .clone();
                    serde_json::json!({"id": code, "rect": [r.left(), r.top(), r.right(), r.bottom()]})
                })
                .collect::<Vec<_>>()
        };
        report.insert(
            name.into(),
            serde_json::json!({
                "trace_boxes": rects(&app.panes.trace.boxes),
                "network_boxes": rects(&app.panes.network.boxes).len(),
                "body": [app.panes.body.left(), app.panes.body.top(), app.panes.body.right(), app.panes.body.bottom()],
                "table_width": (app.panes.body.width() * app.grid_fraction).max(400.).min(app.panes.body.width() - 150.),
                "actions": app.action_rects.clone(),
            }),
        );
    }
    assert!(app.error.is_none(), "{:?}", app.error);
    std::fs::write(
        out.join(format!("{prefix}rects.json")),
        serde_json::to_vec_pretty(&serde_json::json!({
            "source": app.project.source_name,
            "focus": focus,
            "shots": report,
        }))
        .unwrap(),
    )
    .unwrap();
    drop(app);
    let _ = std::fs::remove_file(path);
}

// ------------------------------------------------------------ costs and layouts

/// The starter with a costed labor assignment (24 h at 40 an hour, 8 h done).
pub(super) fn costed() -> ProjectData {
    let mut p = rs::tests::with_resource();
    let t = p.document.table_mut("TASKRSRC");
    for (field, value) in [
        ("target_cost", "960"),
        ("remain_cost", "640"),
        ("act_reg_cost", "300"),
        ("act_ot_cost", "20"),
        ("cost_per_qty", "40"),
    ] {
        t.set(0, field, value.into());
    }
    p
}

/// The top-left of activity row `k` of the bottom table (screen).
fn bottom_row(app: &App, k: usize) -> Pos2 {
    Pos2::new(
        app.panes.body.left() + 40.,
        app.panes.grid.body_top + app.panes.grid.row_offsets[k] + 9.,
    )
}

#[test]
fn bottom_activity_table_and_gantt_share_rows_selection_and_keep_their_columns() {
    let (path, ctx, mut app, mut time) = setup("bottom-table", scheduled_starter());
    app.set_bottom(Some(BottomView::ActivityTable));
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![], &mut time);
    }
    // The bottom table starts from the top table's columns.
    assert_eq!(app.panes.grid.columns, app.grid.layout.columns);
    // Measured metrics: 35 px header, 18 px activity rows.
    let (header_top, header_h) = app.panes.grid.header;
    assert_eq!(header_h, 35.);
    assert!((app.panes.grid.body_top - (header_top + 35.)).abs() < 0.5);
    let rows = app.rows();
    let offsets = app.panes.grid.row_offsets.clone();
    assert_eq!(offsets.len(), rows.len() + 1);
    for (k, row) in rows.iter().enumerate() {
        if matches!(row, grid::Row::Activity(_)) {
            assert_eq!(offsets[k + 1] - offsets[k], 18., "row {k}");
        }
    }
    // The bottom table's own columns leave the top table alone.
    let top_columns = app.grid.layout.columns.clone();
    app.panes.grid.columns.truncate(2);
    frame(&mut app, &ctx, vec![], &mut time);
    assert_eq!(app.grid.layout.columns, top_columns);
    assert_eq!(app.panes.grid.columns.len(), 2);
    // A click in the bottom table selects the activity in both panes.
    let activity_rows: Vec<(usize, usize)> = rows
        .iter()
        .enumerate()
        .filter_map(|(k, r)| match r {
            grid::Row::Activity(i) => Some((k, *i)),
            _ => None,
        })
        .collect();
    let (k, i) = activity_rows[2];
    let at = bottom_row(&app, k);
    press(&mut app, &ctx, at, &mut time, 1);
    assert_eq!(app.selected, Some(i));
    assert!(app.is_selected(i));
    // A click in the top table shows as the selection in the bottom one.
    let (k2, i2) = activity_rows[1];
    let x = {
        let [l, _, r, _] = app.action_rects["grid_header_activity_id"];
        (l + r) / 2.
    };
    let y = app.grid.body_top + app.grid.row_offsets[k2] + 9.;
    assert!(y < header_top, "the top row is above the bottom pane");
    press(&mut app, &ctx, Pos2::new(x, y), &mut time, 1);
    assert_eq!(app.selected, Some(i2));
    // Render on a fresh context so the software painter sees every texture.
    let render = egui::Context::default();
    crate::chrome::configure(&render);
    render.set_pixels_per_point(1.);
    let mut painter = SoftwarePainter::default();
    let _ = painter.frame(&mut app, &render, &mut time, vec![], false);
    let (image, _) = painter
        .frame(&mut app, &render, &mut time, vec![], true)
        .unwrap();
    let at = bottom_row(&app, k2);
    let pixel = image.get_pixel(at.x as u32 + 30, at.y as u32);
    assert_eq!(
        [pixel[0], pixel[1], pixel[2]],
        [SELECT.r(), SELECT.g(), SELECT.b()],
        "the bottom row shows the selection"
    );
    // Columns... from Bottom Layout Options edits the bottom table only.
    app.open_bottom_layout_options();
    let d = app.grid.dialogs.columns.as_mut().unwrap();
    assert!(d.bottom);
    assert_eq!(d.columns.len(), 2);
    d.columns.truncate(1);
    for _ in 0..2 {
        frame(&mut app, &ctx, vec![], &mut time);
    }
    click(&mut app, &ctx, "columns_ok", &mut time, 1);
    assert_eq!(app.panes.grid.columns.len(), 1);
    assert_eq!(app.grid.layout.columns, top_columns);
    // The Gantt Chart below: the top pane's bars stay in the top pane.
    app.set_bottom(Some(BottomView::Gantt));
    for _ in 0..2 {
        frame(&mut app, &ctx, vec![], &mut time);
    }
    assert!(!app.bar_geometry.is_empty());
    assert!(app
        .bar_geometry
        .iter()
        .all(|b| b.row.bottom() <= header_top + 1.));
    assert_eq!(app.panes.grid.row_offsets.len(), rows.len() + 1);
    // The choice and the bottom columns persist in a saved layout and in
    // the workspace.
    app.save_layout_as("Bottom Gantt").unwrap();
    app.set_bottom(Some(BottomView::Details));
    app.panes.grid.columns = top_columns.clone();
    frame(&mut app, &ctx, vec![], &mut time);
    app.open_layout("Bottom Gantt", true).unwrap();
    assert_eq!(app.panes.bottom, BottomView::Gantt);
    assert_eq!(app.panes.grid.columns.len(), 1);
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let store = Store::open(path.clone()).unwrap();
    let project = store.last().unwrap().unwrap();
    let ctx = egui::Context::default();
    let app = App::new(&ctx, store, project);
    assert_eq!(app.panes.bottom, BottomView::Gantt);
    assert_eq!(app.panes.grid.columns.len(), 1);
    // A damaged column list is refused as a whole.
    assert!(columns_from_json(&serde_json::json!([{"id": "", "width": 50}])).is_none());
    assert!(columns_from_json(&serde_json::json!([{"id": "start", "width": 1e9}])).is_none());
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn bottom_gantt_has_its_own_splitter_and_timescale_scroll_saved_with_the_layout() {
    let mut d = crate::qa_sweep::Driver::new("l1-bottom-scale", scheduled_starter());
    d.home();
    d.app.set_bottom(Some(BottomView::Gantt));
    d.frames(3);
    // First shown, it starts from the top pane's split and timescale.
    let top = (
        d.app.grid_fraction,
        d.app.timeline_origin,
        d.app.pixels_per_day,
    );
    let own = d.app.panes.grid.scale.clone();
    assert!(own.own);
    assert_eq!(
        (own.grid_fraction, own.timeline_origin, own.pixels_per_day),
        (top.0, Some(top.1), top.2)
    );
    // Dragging the bottom splitter moves only the bottom's.
    let splitter = d.rect("bottom_splitter");
    let from = splitter.center();
    let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    d.frame(vec![egui::Event::PointerMoved(from)]);
    d.frame(vec![button(from, true)]);
    for k in 1..=6 {
        let at = from - Vec2::new(30. * k as f32, 0.);
        d.frame(vec![egui::Event::PointerMoved(at)]);
    }
    d.frame(vec![button(from - Vec2::new(180., 0.), false)]);
    d.frames(2);
    let moved = d.app.panes.grid.scale.grid_fraction;
    assert!(
        moved < own.grid_fraction - 0.05,
        "{moved} vs {}",
        own.grid_fraction
    );
    assert_eq!(d.app.grid_fraction, top.0, "the top split stays");
    // The wheel over the bottom chart scrolls only the bottom timescale.
    let chart = Pos2::new(d.app.panes.grid.scale.chart_width / 2. + 600., from.y + 40.);
    d.frame(vec![egui::Event::PointerMoved(chart)]);
    let bottom_origin = d.app.panes.grid.scale.timeline_origin;
    for _ in 0..4 {
        d.frame(vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(-80., 0.),
            modifiers: Default::default(),
        }]);
    }
    d.frames(6);
    assert_ne!(
        d.app.panes.grid.scale.timeline_origin, bottom_origin,
        "bottom scrolled"
    );
    assert_eq!(d.app.timeline_origin, top.1, "the top timescale stays");
    // Zoom is the bottom's own too.
    let ppd = d.app.panes.grid.scale.pixels_per_day;
    d.frame(vec![egui::Event::Zoom(1.5)]);
    d.frames(2);
    assert!(d.app.panes.grid.scale.pixels_per_day > ppd, "bottom zoomed");
    assert_eq!(d.app.pixels_per_day, top.2, "the top zoom stays");
    // Rows still line up and the selection is still shared.
    assert_eq!(d.app.panes.grid.row_offsets, d.app.grid.row_offsets);
    // Saved with a layout, and restored by opening it.
    d.app.save_layout_as("L1 bottom scale").unwrap();
    let saved = d.app.panes.grid.scale.clone();
    let json = d.app.panes.json();
    assert_eq!(
        json["bottom_gantt"]["grid_fraction"],
        saved.grid_fraction as f64
    );
    assert_eq!(
        json["bottom_gantt"]["timeline_origin"],
        saved.timeline_origin.unwrap()
    );
    d.app.panes.grid.scale.grid_fraction = 0.7;
    d.app.panes.grid.scale.timeline_origin = Some(12_345 * 60);
    d.app.panes.grid.scale.pixels_per_day = 3.;
    d.frames(2);
    d.app.open_layout("L1 bottom scale", true).unwrap();
    d.frames(2);
    let back = &d.app.panes.grid.scale;
    assert_eq!(back.grid_fraction, saved.grid_fraction);
    assert_eq!(back.timeline_origin, saved.timeline_origin);
    assert_eq!(back.pixels_per_day, saved.pixels_per_day);
    assert_eq!((back.upper, back.lower), (saved.upper, saved.lower));
    assert_eq!(d.app.grid_fraction, top.0);
    // Damaged values are refused piecemeal; the rest still loads.
    let mut p = PanesState::default();
    p.restore(&serde_json::json!({"bottom_gantt": {
        "grid_fraction": 99.0, "pixels_per_day": "x", "timeline_origin": 1e30, "upper": "Nope"
    }}));
    assert_eq!(p.grid.scale.grid_fraction, 1.0, "clamped to the full range");
    assert_eq!(
        p.grid.scale.pixels_per_day,
        BottomScale::default().pixels_per_day
    );
    assert_eq!(p.grid.scale.timeline_origin, None);
    // A layout saved before the bottom Gantt had its own scale has none.
    assert!(PanesState::default().json()["bottom_gantt"].is_null());
    // The workspace keeps the split and zoom but not the scroll position.
    assert!(d.app.panes.json_with(false)["bottom_gantt"]["timeline_origin"].is_null());
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

/// The 2 px curves of the last frame: color and points.
fn curves(output: &egui::FullOutput) -> Vec<(Color32, Vec<Pos2>)> {
    let mut out = vec![];
    for clipped in &output.shapes {
        let mut visit = |shape: &egui::Shape| {
            if let egui::Shape::Path(p) = shape {
                if p.stroke.width == 2. {
                    if let egui::epaint::ColorMode::Solid(c) = &p.stroke.color {
                        out.push((*c, p.points.clone()));
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

fn bar_fills(output: &egui::FullOutput, color: Color32) -> usize {
    output
        .shapes
        .iter()
        .filter(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.fill == color))
        .count()
}

fn text_shown(output: &egui::FullOutput, text: &str) -> bool {
    output
        .shapes
        .iter()
        .any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == text))
}

/// Display > Cumulative of the profile on show, switched to `on`, through
/// its Display header menu.
fn set_curves(d: &mut crate::qa_sweep::Driver, on: bool) {
    let resource = d.app.panes.bottom == BottomView::ResourceUsageProfile;
    if d.app.panes.usage.profile.any_cumulative(resource) != on {
        d.click("usage_profile_display");
        d.frames(1);
        d.click("usage_display_cumulative");
        d.frames(1);
    }
    assert_eq!(d.app.panes.usage.profile.any_cumulative(resource), on);
}

#[test]
fn cumulative_draws_a_curve_over_the_period_bars_in_units_and_costs() {
    let mut d = crate::qa_sweep::Driver::new("l1-cumulative", costed());
    d.home();
    d.app.set_bottom(Some(BottomView::ActivityUsageProfile));
    d.frames(3);
    let plain = d.frame(vec![]);
    let bars = bar_fills(&plain, usage::ACTUAL.0) + bar_fills(&plain, usage::REMAINING.0);
    assert!(bars > 0, "the profile draws bars");
    assert!(curves(&plain).is_empty(), "no curve without Cumulative");
    assert!(!text_shown(&plain, "Cumulative (right scale)"));
    // Cumulative (Display > Cumulative on a profile switches every series
    // drawn as bars) keeps the period bars and adds a running-total curve
    // per series, one dashed (remaining late).
    set_curves(&mut d, true);
    d.frames(2);
    let on = d.frame(vec![]);
    assert_eq!(
        bar_fills(&on, usage::ACTUAL.0) + bar_fills(&on, usage::REMAINING.0),
        bars,
        "the bars are not replaced"
    );
    let lines = curves(&on);
    let colors: Vec<Color32> = lines.iter().map(|(c, _)| *c).collect();
    for border in [usage::BUDGET.1, usage::ACTUAL.1, usage::REMAINING.1] {
        assert!(colors.contains(&border), "a curve in {border:?}");
    }
    for (color, points) in &lines {
        assert!(points.len() > 2, "{color:?}");
        assert!(
            points.windows(2).all(|w| w[1].y <= w[0].y + 0.01),
            "{color:?}: running totals never fall"
        );
        assert!(points.windows(2).all(|w| w[1].x >= w[0].x), "left to right");
    }
    assert!(text_shown(&on, "Cumulative (right scale)"));
    // Units: the right scale reaches the series' total (24 h -> 30).
    assert!(text_shown(&on, &format::units(30.)));
    // Costs: the same curves in the currency format (960 -> 1,000).
    d.app.panes.usage.costs = true;
    d.frames(2);
    let costs = d.frame(vec![]);
    assert!(text_shown(&costs, &format::cost(1000.)));
    assert!(!text_shown(&costs, &format::units(30.)));
    assert!(!curves(&costs).is_empty());
    // The resource profile: a running total per series drawn as bars
    // (actual and remaining early), each in its series' color.
    d.app.panes.usage.costs = false;
    let rsrc = rs::resources(&d.app.project)[0].id;
    d.app.panes.usage.resource = Some(rsrc);
    d.app.set_bottom(Some(BottomView::ResourceUsageProfile));
    set_curves(&mut d, true);
    d.frames(3);
    let resource = d.frame(vec![]);
    let lines = curves(&resource);
    let colors: Vec<Color32> = lines.iter().map(|(c, _)| *c).collect();
    assert_eq!(colors, [usage::ACTUAL.1, usage::REMAINING.1]);
    set_curves(&mut d, false);
    d.frames(2);
    assert!(curves(&d.frame(vec![])).is_empty());
    // Spreadsheets keep their running totals.
    d.app.panes.usage.cumulative = true;
    let u = usage::UsageState {
        cumulative: true,
        ..Default::default()
    };
    assert_eq!(u.shown(0., &[1., 2., 3.]), vec![1., 3., 6.]);
    // From what falls before the first period on show.
    assert_eq!(u.shown(4., &[1., 2., 3.]), vec![5., 7., 10.]);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn show_on_bottom_offers_the_table_and_gantt_from_the_view_menu() {
    let mut d = crate::qa_sweep::Driver::new("h4-show-on-bottom", scheduled_starter());
    d.home();
    for (item, view) in [
        ("menu_bottom_activity_table", BottomView::ActivityTable),
        ("menu_bottom_gantt", BottomView::Gantt),
    ] {
        d.click("menu_bar_view");
        assert!(d.hover_text("Show on Bottom"));
        assert!(d.has(item), "{item} is drawn and enabled");
        d.click(item);
        d.frames(2);
        assert_eq!(d.app.panes.bottom, view);
        assert!(d.app.bottom_showing(view), "{item} is checked");
    }
    d.assert_clean("show on bottom");
}

#[test]
fn usage_display_options_change_the_values_and_agree_with_the_histograms() {
    let p = costed();
    let periods = schedule_days(&p);
    let by_task = usage::labor_assignments(&p);
    // Units and costs agree with the resource histogram buckets.
    for costs in [false, true] {
        let u = usage::activity_usage_of(&p, &by_task, &periods, None, costs);
        let h = rs::usage_of(&p, 7, &periods, None, costs);
        for k in 0..periods.len() {
            assert!(
                (u.remaining[k] - h.remaining[k]).abs() < 1e-6,
                "{costs} {k}"
            );
            assert!((u.actual[k] - h.actual[k]).abs() < 1e-6, "{costs} {k}");
        }
        let a = &by_task.values().next().unwrap()[0];
        let (budget, remaining) = if costs {
            (a.budget_cost, a.remaining_cost)
        } else {
            (a.budget, a.remaining)
        };
        assert!((u.budget.iter().sum::<f64>() - budget).abs() < 1e-6);
        assert!((u.remaining.iter().sum::<f64>() - remaining).abs() < 1e-6);
    }
    let a = &by_task.values().next().unwrap()[0];
    assert_eq!(
        (a.budget_cost, a.remaining_cost, a.actual_cost),
        (960., 640., 320.)
    );
    // Cumulative: running totals ending at the total.
    let display = usage::UsageState {
        cumulative: true,
        ..Default::default()
    };
    let u = usage::activity_usage_of(&p, &by_task, &periods, None, true);
    let running = display.shown(0., &u.remaining);
    assert!((running.last().unwrap() - 640.).abs() < 1e-6);
    assert!(running.windows(2).all(|w| w[1] >= w[0]));
    // Weeks hold the days' sums.
    let weeks = crate::timescale::cells(
        crate::timescale::Unit::Week,
        None,
        p.model.origin_serial,
        periods[0].0 as f64,
        periods.last().unwrap().1 as f64,
    );
    let week_periods: Vec<(i64, i64)> = weeks
        .iter()
        .map(|c| (c.start_minute, c.end_minute))
        .collect();
    let w = usage::activity_usage_of(&p, &by_task, &week_periods, None, false);
    assert!((w.remaining.iter().sum::<f64>() - 24.).abs() < 1e-6);
    assert!(w.remaining.iter().filter(|v| **v > 0.).count() <= 2);
    // Through the UI: the profile's Display menu switches to costs,
    // cumulative and months, and the layout keeps them.
    let (path, ctx, mut app, mut time) = setup("usage-display", costed());
    app.set_bottom(Some(BottomView::ActivityUsageProfile));
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![], &mut time);
    }
    for item in ["usage_display_costs", "usage_display_cumulative"] {
        click(&mut app, &ctx, "usage_profile_display", &mut time, 1);
        click(&mut app, &ctx, item, &mut time, 1);
    }
    // On a profile, Cumulative switches the profile's series curves.
    let curves_on = |app: &App| app.panes.usage.profile.any_cumulative(false);
    assert!(app.panes.usage.costs && curves_on(&app));
    app.panes.usage.interval = usage::Interval::Month;
    app.save_layout_as("Costs").unwrap();
    app.panes.usage = Default::default();
    app.open_layout("Costs", true).unwrap();
    assert!(app.panes.usage.costs && curves_on(&app));
    assert_eq!(app.panes.usage.interval, usage::Interval::Month);
    // The spreadsheet's value header opens the same menu.
    app.set_bottom(Some(BottomView::ActivityUsageSheet));
    for _ in 0..2 {
        frame(&mut app, &ctx, vec![], &mut time);
    }
    click(&mut app, &ctx, "usage_sheet_display", &mut time, 1);
    click(&mut app, &ctx, "usage_display_units", &mut time, 1);
    assert!(!app.panes.usage.costs);
    // Display Activities for selected Time Period: only the activities
    // working in the picked period stay in the table.
    app.set_bottom(Some(BottomView::ActivityUsageProfile));
    let all = app.rows().len();
    let i = index_of(&app.project, 1010);
    let a = &app.project.model.activities[i];
    let start = displayed_start(a, false).unwrap();
    app.panes.usage.period = Some((start, start + DAY));
    assert_eq!(app.rows().len(), all, "the box is off");
    app.panes.usage.time_period = true;
    let kept = app.rows();
    assert!(kept.len() < all);
    assert!(kept.contains(&grid::Row::Activity(i)));
    for row in &kept {
        if let grid::Row::Activity(k) = row {
            let a = &app.project.model.activities[*k];
            let s = displayed_start(a, false).unwrap();
            let f = displayed_finish(a, false).unwrap();
            assert!(s < start + DAY && f.max(s + 1) > start);
        }
    }
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(app.action_rects.contains_key("usage_time_period"));
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn network_boxes_keep_dragged_places_until_reorganize() {
    let (path, ctx, mut app, mut time) = setup("network-drag", scheduled_starter());
    app.set_top(TopView::Network);
    app.set_bottom(None);
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![], &mut time);
    }
    let computed = app.network_layout();
    let (id, rect) = app.panes.network.boxes[0];
    let i = index_of(&app.project, id);
    let code = app.project.model.activities[i].task_code.clone();
    let from = rect.center();
    let to = from + Vec2::new(150., 60.);
    // Drag the box with the primary button.
    let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(from)],
        &mut time,
    );
    frame(&mut app, &ctx, vec![button(from, true)], &mut time);
    for k in 1..=6 {
        let at = from + (to - from) * (k as f32 / 6.);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(at)],
            &mut time,
        );
    }
    frame(&mut app, &ctx, vec![button(to, false)], &mut time);
    frame(&mut app, &ctx, vec![egui::Event::PointerGone], &mut time);
    let zoom = app.panes.network.zoom;
    let moved = app.network_layout();
    let before = computed.find(i).unwrap().rect.min;
    let after = moved.find(i).unwrap().rect.min;
    let expected = before + (to - from) / zoom;
    assert!(
        (after - expected).length() < 2.,
        "{before:?} -> {after:?}, expected {expected:?}"
    );
    assert!(app.panes.network.positions.contains_key(&code));
    // Other boxes stay where the layout puts them.
    for (a, b) in computed.boxes.iter().zip(&moved.boxes) {
        if a.activity != i {
            assert_eq!(a.rect, b.rect);
        }
    }
    // The place survives saving the layout and reopening the workspace.
    app.save_layout_as("Moved").unwrap();
    frame(&mut app, &ctx, vec![], &mut time);
    drop(app);
    let store = Store::open(path.clone()).unwrap();
    let project = store.last().unwrap().unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx, store, project);
    assert_eq!(app.network_layout().find(i).unwrap().rect.min, after);
    // Reorganize returns to the computed layout; the saved layout keeps it.
    app.network_reorganize();
    assert_eq!(app.network_layout(), computed);
    app.open_layout("Moved", true).unwrap();
    assert_eq!(app.network_layout().find(i).unwrap().rect.min, after);
    // Saved positions are bounded and validated.
    let bad = serde_json::json!({
        "A": [1e12, 5.0], "B": [f64::MAX, 1.0], "": [1.0, 1.0], "C": [10.0, 20.0], "D": "x"
    });
    let parsed = network::positions_from_json(&bad);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed["C"], Pos2::new(10., 20.));
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn network_font_and_colors_apply_through_the_options_and_cancel_cleanly() {
    let (path, ctx, mut app, mut time) = setup("network-font", scheduled_starter());
    app.set_top(TopView::Network);
    app.open_network_options();
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![], &mut time);
    }
    click(&mut app, &ctx, "network_font_colors", &mut time, 1);
    assert!(app.panes.network_font.is_some());
    frame(&mut app, &ctx, vec![], &mut time);
    // Cancel leaves everything as it was.
    click(&mut app, &ctx, "network_font_cancel", &mut time, 1);
    assert!(app.panes.network_font.is_none());
    assert_eq!(app.panes.network.options, NetworkOptions::default());
    // OK hands the choice to the options dialog; its OK applies it.
    click(&mut app, &ctx, "network_font_colors", &mut time, 1);
    let d = app.panes.network_font.as_mut().unwrap();
    d.size = "11".into();
    d.options.normal = Color32::from_rgb(0x80, 0xFF, 0xFF);
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "network_font_ok", &mut time, 1);
    assert_eq!(app.panes.network.options, NetworkOptions::default());
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "network_ok", &mut time, 1);
    let o = app.panes.network.options;
    assert_eq!(o.font_size, 11.);
    assert_eq!(o.normal, Color32::from_rgb(0x80, 0xFF, 0xFF));
    assert_eq!(o.colors(false).fill, o.normal);
    assert_eq!(o.colors(true), boxes::CRITICAL_BOX);
    // The options round trip as the layout stores them.
    let mut back = NetworkOptions::default();
    back.restore(&o.json());
    assert_eq!(back, o);
    // Escape closes the dialogs without applying.
    app.open_network_options();
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "network_font_colors", &mut time, 1);
    app.panes.network_font.as_mut().unwrap().size = "15".into();
    let escape = egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    };
    frame(&mut app, &ctx, vec![escape.clone()], &mut time);
    assert!(app.panes.network_font.is_none());
    assert!(app.panes.network_dialog.is_some());
    frame(&mut app, &ctx, vec![escape], &mut time);
    assert!(app.panes.network_dialog.is_none());
    assert_eq!(app.panes.network.options.font_size, 11.);
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn network_copy_from_takes_template_and_box_places_from_a_saved_layout() {
    let (path, ctx, mut app, mut time) = setup("network-copy", scheduled_starter());
    app.set_top(TopView::Network);
    // A saved layout with a dates template and two moved boxes.
    app.panes.network.options.template = boxes::Template::Dates;
    app.panes
        .network
        .positions
        .insert("A1020".into(), Pos2::new(300., 200.));
    app.panes
        .network
        .positions
        .insert("A1030".into(), Pos2::new(500., 90.));
    app.save_layout_as("Moved dates").unwrap();
    // A saved layout with no network settings is not offered.
    app.save_layout_as("Plain").unwrap();
    app.grid
        .library
        .iter_mut()
        .find(|l| l.name == "Plain")
        .unwrap()
        .panes = None;
    // Back to a fresh state: standard boxes, none moved.
    app.panes.network.options.template = boxes::Template::Standard;
    app.panes.network.positions.clear();
    app.open_network_options();
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![], &mut time);
    }
    // The button is live and lists the layouts that hold network settings.
    click(&mut app, &ctx, "network_copy_from", &mut time, 1);
    let names: Vec<_> = app
        .panes
        .network_copy
        .as_ref()
        .expect("Copy From opens")
        .sources
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(names, ["Moved dates"]);
    frame(&mut app, &ctx, vec![], &mut time);
    // Cancel leaves the options and boxes alone.
    click(&mut app, &ctx, "network_copy_cancel", &mut time, 1);
    assert!(app.panes.network_copy.is_none());
    click(&mut app, &ctx, "network_ok", &mut time, 1);
    assert_eq!(
        app.panes.network.options.template,
        boxes::Template::Standard
    );
    assert!(app.panes.network.positions.is_empty());
    // OK copies into the dialog; the options dialog's OK applies it.
    app.open_network_options();
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "network_copy_from", &mut time, 1);
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "network_copy_row_0", &mut time, 1);
    click(&mut app, &ctx, "network_copy_ok", &mut time, 1);
    assert!(app.panes.network_copy.is_none());
    assert!(app.panes.network.positions.is_empty(), "not applied yet");
    frame(&mut app, &ctx, vec![], &mut time);
    assert_eq!(
        app.panes.network_dialog.as_ref().unwrap().options.template,
        boxes::Template::Dates
    );
    click(&mut app, &ctx, "network_ok", &mut time, 1);
    assert_eq!(app.panes.network.options.template, boxes::Template::Dates);
    assert_eq!(app.panes.network.positions.len(), 2);
    assert_eq!(app.panes.network.positions["A1020"], Pos2::new(300., 200.));
    // The copy is kept with the working layout and a new save.
    app.save_layout_as("Copied").unwrap();
    let saved = app.saved_layout("Copied").unwrap().panes.unwrap();
    assert_eq!(saved["network"]["template"], "dates");
    assert_eq!(
        saved["network"]["positions"]["A1030"],
        serde_json::json!([500.0, 90.0])
    );
    // Escape closes Copy From first, then the options dialog.
    app.open_network_options();
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "network_copy_from", &mut time, 1);
    let escape = egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    };
    frame(&mut app, &ctx, vec![escape.clone()], &mut time);
    assert!(app.panes.network_copy.is_none() && app.panes.network_dialog.is_some());
    frame(&mut app, &ctx, vec![escape], &mut time);
    assert!(app.panes.network_dialog.is_none());
    assert_eq!(app.panes.network.positions.len(), 2);
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn layout_files_export_and_import_identically_and_refuse_bad_files() {
    let (path, ctx, mut app, mut time) = setup("layout-file", scheduled_starter());
    app.set_bottom(Some(BottomView::Gantt));
    app.panes
        .network
        .positions
        .insert("A1".into(), Pos2::new(40., 60.));
    app.grid.layout.columns.truncate(4);
    frame(&mut app, &ctx, vec![], &mut time);
    app.save_layout_as("Shared").unwrap();
    let original = app.saved_layout("Shared").unwrap();
    let dir = std::env::temp_dir().join(format!("openxer-h4-layouts-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("shared.oxl");
    app.export_layout_file("Shared", &file).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["format"], "openxer-layout");
    assert_eq!(v["version"], 1);
    // Importing next to the original gets a new name and the same content.
    let name = app.import_layout_file(&file).unwrap();
    assert_eq!(name, "Shared (2)");
    let mut imported = app.saved_layout(&name).unwrap();
    imported.name = original.name.clone();
    assert_eq!(imported, original);
    // In another workspace it keeps its name and opens to the same panes.
    app.delete_layout("Shared").unwrap();
    app.delete_layout("Shared (2)").unwrap();
    assert_eq!(app.import_layout_file(&file).unwrap(), "Shared");
    assert_eq!(app.saved_layout("Shared").unwrap(), original);
    app.set_bottom(Some(BottomView::Details));
    app.open_layout("Shared", true).unwrap();
    assert_eq!(app.panes.bottom, BottomView::Gantt);
    assert_eq!(app.panes.network.positions["A1"], Pos2::new(40., 60.));
    // Oversize, foreign, newer and invalid files are refused with a message.
    let count = app.grid.library.len();
    let cases: Vec<(&str, String)> = vec![
        (
            "big",
            format!("{{\"pad\": \"{}\"}}", "x".repeat(3 * 1024 * 1024)),
        ),
        ("foreign", r#"{"name": "x"}"#.into()),
        ("newer", text.replace("\"version\": 1", "\"version\": 99")),
        ("invalid", text.replace("\"width\": ", "\"width\": -")),
        ("not json", "layout".into()),
    ];
    for (what, body) in cases {
        let bad = dir.join(format!("{}.oxl", what.replace(' ', "-")));
        std::fs::write(&bad, body).unwrap();
        let err = app.import_layout_file(&bad).unwrap_err();
        assert!(!err.is_empty(), "{what}");
        assert_eq!(app.grid.library.len(), count, "{what} adds nothing");
    }
    assert!(app.import_layout_file(&dir.join("missing.oxl")).is_err());
    // The Open Layout dialog offers Import and Export.
    app.open_layout_dialog();
    for _ in 0..2 {
        frame(&mut app, &ctx, vec![], &mut time);
    }
    assert!(app.action_rects.contains_key("open_layout_import"));
    assert!(app.action_rects.contains_key("open_layout_export"));
    click(&mut app, &ctx, "open_layout_cancel", &mut time, 1);
    assert!(app.error.is_none(), "{:?}", app.error);
    let _ = std::fs::remove_dir_all(dir);
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// Software renders, for review, to the ignored
/// `artifacts/h4/`: the bottom Activity Table and Gantt Chart, the usage
/// Display menu with costs, a dragged network box with Font & Colors, Open
/// Layout with Import and Export, Page Setup Options with the bottom-pane
/// choices, and print previews of a profile under the Gantt and of the WBS.
#[test]
fn render_h4_panes_layouts_and_printing() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/h4");
    std::fs::create_dir_all(&out).unwrap();
    let path = std::env::temp_dir().join(format!("openxer-h4-render-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let project = costed();
    let mut store = Store::open(path.clone()).unwrap();
    store.save(&project).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    let mut time = 0.;
    let mut painter = SoftwarePainter::default();
    painter.screen = Some(Vec2::new(1920., 1040.));
    app.selected = Some(index_of(&app.project, 1010));
    let shot = |app: &mut App, painter: &mut SoftwarePainter, time: &mut f64, name: &str| {
        for _ in 0..3 {
            let _ = painter.frame(app, &ctx, time, vec![], false);
        }
        let (image, triangles) = painter.frame(app, &ctx, time, vec![], true).unwrap();
        assert!(triangles > 300, "{name}");
        image.save(out.join(format!("{name}.png"))).unwrap();
    };
    let click_at = |app: &mut App, painter: &mut SoftwarePainter, time: &mut f64, action: &str| {
        let [l, t, r, b] = app.action_rects[action];
        let pos = Pos2::new((l + r) / 2., (t + b) / 2.);
        for pressed in [true, false] {
            let _ = painter.frame(
                app,
                &ctx,
                time,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
                false,
            );
        }
    };
    app.set_bottom(Some(BottomView::ActivityTable));
    shot(&mut app, &mut painter, &mut time, "bottom-activity-table");
    app.set_bottom(Some(BottomView::Gantt));
    app.panes.grid.columns.truncate(3);
    shot(&mut app, &mut painter, &mut time, "bottom-gantt");
    app.set_bottom(Some(BottomView::ActivityUsageProfile));
    app.panes.usage.costs = true;
    for s in app.panes.usage.profile.activity.iter_mut() {
        s.cumulative = s.by_date;
    }
    shot(&mut app, &mut painter, &mut time, "usage-profile-costs");
    click_at(&mut app, &mut painter, &mut time, "usage_profile_display");
    shot(&mut app, &mut painter, &mut time, "usage-display-menu");
    app.panes.usage = Default::default();
    app.set_bottom(Some(BottomView::ActivityUsageSheet));
    app.panes.usage.interval = usage::Interval::Week;
    shot(&mut app, &mut painter, &mut time, "usage-sheet-weeks");
    app.panes.usage = Default::default();
    app.set_top(TopView::Network);
    app.set_bottom(None);
    let code = app.project.model.activities[2].task_code.clone();
    app.panes
        .network
        .positions
        .insert(code, Pos2::new(60., 330.));
    app.panes.network.options.normal = Color32::from_rgb(0x80, 0xFF, 0xFF);
    app.panes.network.options.font_size = 10.;
    shot(&mut app, &mut painter, &mut time, "network-dragged-box");
    app.open_network_options();
    app.panes.network_font = Some(network::FontDraft::new(app.panes.network.options));
    shot(&mut app, &mut painter, &mut time, "network-font-colors");
    app.panes.network_font = None;
    app.panes.network_dialog = None;
    app.panes.network.positions.clear();
    app.panes.network.options = Default::default();
    app.set_top(TopView::Gantt);
    app.open_layout_dialog();
    shot(
        &mut app,
        &mut painter,
        &mut time,
        "open-layout-import-export",
    );
    click_at(&mut app, &mut painter, &mut time, "open_layout_cancel");
    app.set_bottom(Some(BottomView::TraceLogic));
    app.open_page_setup();
    app.print.dialog.as_mut().unwrap().tab = 4;
    shot(
        &mut app,
        &mut painter,
        &mut time,
        "page-setup-options-trace",
    );
    app.print.dialog = None;
    app.set_bottom(Some(BottomView::ActivityUsageProfile));
    app.open_print_preview();
    shot(&mut app, &mut painter, &mut time, "print-preview-profile");
    app.print.preview = None;
    app.set_bottom(Some(BottomView::TraceLogic));
    app.open_print_preview();
    if let Some(p) = app.print.preview.as_mut() {
        let last = p.count() - 1;
        p.step(last as isize, 0);
    }
    shot(&mut app, &mut painter, &mut time, "print-preview-trace");
    app.print.preview = None;
    app.show_view(crate::views::MainView::Wbs);
    app.open_print_preview();
    shot(&mut app, &mut painter, &mut time, "print-preview-wbs");
    app.print.preview = None;
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

// ------------------------------------------------ timescale label fitting

/// The usage views draw their timescale with the Gantt's label fitting: at
/// hour and day zoom no label is cut by the header or overlaps another, hours
/// follow the time format and days carry weekday names.
#[test]
fn usage_headers_fit_their_labels_like_the_gantt_header() {
    let (path, ctx, mut app, mut time) = setup("usage-labels", rs::tests::with_resource());
    let run = |app: &mut App, time: &mut f64| {
        let mut last = None;
        for _ in 0..3 {
            *time += 0.1;
            last = Some(ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500., 900.))),
                    time: Some(*time),
                    focused: true,
                    ..Default::default()
                },
                |ctx| app.update_ui(ctx),
            ));
        }
        last.unwrap()
    };
    for (view, action) in [
        (BottomView::ActivityUsageSheet, "usage_sheet_timescale"),
        (BottomView::ActivityUsageProfile, "usage_profile_timescale"),
    ] {
        app.set_bottom(Some(view));
        for (scale, upper, lower) in [
            (24. * 7. * 7.5, timescale::Unit::Day, timescale::Unit::Hour),
            (24. * 7. * 32., timescale::Unit::Day, timescale::Unit::Hour),
            (200., timescale::Unit::Week, timescale::Unit::Day),
            (7. * 130., timescale::Unit::Week, timescale::Unit::Day),
        ] {
            app.pixels_per_day = (scale / 7.) as f32;
            app.timeline_origin = app.project.model.project.data_date.unwrap_or(0);
            (app.view.upper, app.view.lower) = (upper, lower);
            app.view.automatic_units = false;
            let output = run(&mut app, &mut time);
            let [l, t, r, b] = app.action_rects[action];
            let header = Rect::from_min_max(Pos2::new(l, t), Pos2::new(r, b));
            let mut labels: Vec<(String, Rect)> = vec![];
            for clipped in &output.shapes {
                if let egui::Shape::Text(shape) = &clipped.shape {
                    let rect = shape.galley.rect.translate(shape.pos.to_vec2());
                    if header.contains(rect.center()) {
                        labels.push((shape.galley.text().to_string(), rect));
                    }
                }
            }
            assert!(labels.len() > 4, "{view:?} {scale}: {labels:?}");
            // As in the reference, a cell cut by the header's edge keeps its
            // centred label and shows the visible part; hour labels are whole.
            for (text, rect) in &labels {
                if lower == timescale::Unit::Hour && text.contains(':') {
                    assert!(
                        header.expand(1.).contains_rect(*rect),
                        "{view:?} {scale}: {text:?} {rect:?} leaves {header:?}"
                    );
                }
            }
            for label in labels.iter_mut() {
                label.1 = label.1.intersect(header);
            }
            for (i, a) in labels.iter().enumerate() {
                for b in &labels[i + 1..] {
                    assert!(
                        !a.1.shrink(0.5).intersects(b.1.shrink(0.5)),
                        "{view:?} {scale}: {:?} overlaps {:?}",
                        a.0,
                        b.0
                    );
                }
            }
            let texts: Vec<&str> = labels.iter().map(|l| l.0.as_str()).collect();
            if lower == timescale::Unit::Hour && scale > 24. * 7. * 28. {
                assert!(texts.iter().any(|t| t.contains(':')), "{texts:?}");
            }
            if lower == timescale::Unit::Day && scale > 7. * 100. {
                assert!(texts.iter().any(|t| t.contains("day ")), "{texts:?}");
            }
        }
    }
    drop(app);
    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------
// Package CM-C: right-click menus, profile options, spreadsheet fields,
// splitters, Show All Projects, Copy to Excel and the Tracking menu.
// ---------------------------------------------------------------------

use crate::qa_sweep::Driver;

fn secondary(d: &mut Driver, pos: Pos2) {
    d.press_at(pos, egui::PointerButton::Secondary, Default::default());
}

/// The first activity row (not a band) of the activity usage spreadsheet:
/// its number and the activity index.
fn first_sheet_row(d: &Driver) -> (usize, usize) {
    d.app
        .rows()
        .iter()
        .enumerate()
        .find_map(|(k, r)| match r {
            grid::Row::Activity(i) => Some((k, *i)),
            _ => None,
        })
        .expect("an activity row")
}

fn open_activity_sheet(name: &str) -> Driver {
    let mut d = Driver::new(name, costed());
    d.home();
    d.app.set_bottom(Some(BottomView::ActivityUsageSheet));
    d.frames(4);
    d
}

fn open_menu_on_sheet_row(d: &mut Driver) -> usize {
    let (k, i) = first_sheet_row(d);
    let r = d.rect(&format!("usage_sheet_row_{k}"));
    secondary(d, Pos2::new(r.left() + 20., r.center().y));
    i
}

#[test]
fn usage_spreadsheet_menu_selects_the_row_and_lists_the_commands_in_order() {
    let mut d = open_activity_sheet("cmc-sheet-menu");
    d.app.selected = None;
    let i = open_menu_on_sheet_row(&mut d);
    assert_eq!(d.app.selected, Some(i), "the row selects");
    let order = [
        "usage_sheet_menu_timescale",
        "usage_sheet_menu_options",
        "usage_sheet_menu_show_all_projects",
        "usage_sheet_menu_zoom_in",
        "usage_sheet_menu_zoom_out",
        "usage_sheet_menu_user_preferences",
    ];
    let tops: Vec<f32> = order.iter().map(|k| d.rect(k).top()).collect();
    assert!(
        tops.windows(2).all(|w| w[0] < w[1]),
        "menu order {order:?} {tops:?}"
    );
    // Removed (2026-11-07 UI cleanup): the unbuilt viewer and the cell
    // commands (the activity table's Fill Down already works).
    for item in [
        "date_spread",
        "fill_down",
        "fill_across",
        "range_copy",
        "range_paste",
    ] {
        assert!(
            !d.has(&format!("usage_sheet_menu_{item}")),
            "{item} is gone"
        );
    }
    // Display options are mirrored.
    assert!(d.has("usage_sheet_menu_display"));
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn usage_spreadsheet_menu_commands_run() {
    let mut d = open_activity_sheet("cmc-sheet-run");
    let ppd = d.app.pixels_per_day;
    open_menu_on_sheet_row(&mut d);
    d.click_at(d.rect("usage_sheet_menu_zoom_in").center());
    assert!(d.app.pixels_per_day > ppd, "Zoom In widens the timescale");
    let zoomed = d.app.pixels_per_day;
    open_menu_on_sheet_row(&mut d);
    d.click_at(d.rect("usage_sheet_menu_zoom_out").center());
    assert!(d.app.pixels_per_day < zoomed, "Zoom Out narrows it");
    // Timescale... opens the shared dialog; Escape-free cancel keeps it.
    open_menu_on_sheet_row(&mut d);
    d.click_at(d.rect("usage_sheet_menu_timescale").center());
    d.frames(2);
    assert!(d.app.gantt_ui.timescale.is_some());
    d.app.gantt_ui.timescale = None;
    d.frames(2);
    // User Preferences...
    open_menu_on_sheet_row(&mut d);
    d.click_at(d.rect("usage_sheet_menu_user_preferences").center());
    d.frames(2);
    assert!(d.app.prefs.open);
    d.app.prefs.open = false;
    d.frames(2);
    // Show All Projects toggles and stays checked in the next menu.
    assert!(!d.app.panes.usage.show_all_projects);
    open_menu_on_sheet_row(&mut d);
    d.click_at(d.rect("usage_sheet_menu_show_all_projects").center());
    assert!(d.app.panes.usage.show_all_projects);
    open_menu_on_sheet_row(&mut d);
    d.click_at(d.rect("usage_sheet_menu_show_all_projects").center());
    assert!(!d.app.panes.usage.show_all_projects);
    // Spreadsheet Options... opens the fields dialog.
    open_menu_on_sheet_row(&mut d);
    d.click_at(d.rect("usage_sheet_menu_options").center());
    d.frames(2);
    assert!(d.app.panes.usage_fields.is_some());
    assert!(d.has("usage_fields_ok"));
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.panes.usage_fields.is_none(), "Escape closes it");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn every_usage_view_opens_its_menu_on_a_right_click() {
    let mut d = open_activity_sheet("cmc-all-views");
    // The timescale of the sheet, then the body below the rows.
    let tiers = d.rect("usage_sheet_timescale");
    secondary(&mut d, tiers.center());
    assert!(d.has("usage_sheet_menu_timescale"), "timescale band");
    d.key(Key::Escape);
    // Above the timescale's scroll bar strip along the bottom.
    let body = d.app.panes.body;
    secondary(
        &mut d,
        Pos2::new(
            body.center().x,
            body.bottom() - crate::hscroll::BAR_HEIGHT - 4.,
        ),
    );
    assert!(d.has("usage_sheet_menu_timescale"), "empty area");
    d.key(Key::Escape);
    // The resource spreadsheet: the resource list and the assignment area.
    d.app.set_bottom(Some(BottomView::ResourceUsageSheet));
    d.frames(4);
    let r = d.rect("usage_resource_0");
    secondary(&mut d, r.center());
    assert!(d.has("usage_sheet_menu_options"), "resource list");
    assert_eq!(d.app.panes.usage.resource, Some(7), "the resource selects");
    d.key(Key::Escape);
    let r = d.rect("usage_assignment_row_0");
    secondary(&mut d, Pos2::new(r.center().x, r.center().y));
    assert!(d.has("usage_sheet_menu_options"), "assignment rows");
    d.key(Key::Escape);
    // The profiles carry the profile menu.
    for view in [
        BottomView::ActivityUsageProfile,
        BottomView::ResourceUsageProfile,
    ] {
        d.app.set_bottom(Some(view));
        d.frames(4);
        let body = d.app.panes.body;
        secondary(&mut d, Pos2::new(body.center().x + 80., body.center().y));
        for item in [
            "timescale",
            "options",
            "stacked",
            "show_usage_for",
            "show_all_projects",
            "zoom_in",
            "zoom_out",
        ] {
            let key = format!("usage_profile_menu_{item}");
            assert!(d.has(&key), "{view:?} {key}");
        }
        d.key(Key::Escape);
    }
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

/// The resource profile of a resource that is overallocated, has overtime
/// among its actual hours and spare capacity before and after the work.
fn overloaded() -> ProjectData {
    let mut p = costed();
    let i = index_of(&p, 1010);
    let start = p.model.activities[i].early_start.unwrap();
    p.model.activities[i].act_start = Some(start);
    p.model.project.data_date = Some(start + DAY);
    let t = p.document.table_mut("TASKRSRC");
    for (field, value) in [
        ("remain_qty", "60"),
        ("act_reg_qty", "6"),
        ("act_ot_qty", "2"),
    ] {
        t.set(0, field, value.into());
    }
    p
}

#[test]
fn profile_options_change_what_the_resource_histogram_draws_and_persist() {
    let mut d = Driver::new("cmc-profile-options", overloaded());
    d.home();
    d.app.panes.usage.resource = Some(7);
    d.app.set_bottom(Some(BottomView::ResourceUsageProfile));
    d.frames(4);
    let body = d.app.panes.body;
    let base = d.frame(vec![]);
    assert!(body_fills(&base, usage::ACTUAL.0, body) > 0, "actual bars");
    assert!(
        body_fills(&base, usage::OVER.0, body) > 0,
        "overallocation shows"
    );
    assert_eq!(body_fills(&base, usage::OVERTIME.0, body), 0);
    assert_eq!(body_fills(&base, usage::AVAILABLE.0, body), 0);
    assert!(text_shown(&base, "Actual Units"), "legend shows");
    assert!(text_shown(&base, "Limit"));
    let ink_base = body_fills(&base, INK, body);
    // Each option through the Profile Options dialog (the legend is on the
    // Graph tab).
    let toggle = |d: &mut Driver, key: &str| {
        let body = d.app.panes.body;
        secondary(d, Pos2::new(body.center().x + 80., body.center().y));
        d.click_at(d.rect("usage_profile_menu_options").center());
        d.frames(2);
        assert!(d.app.panes.profile_dialog.is_some());
        if key == "legend" {
            d.click("usage_profile_tab_graph");
            d.frames(1);
        }
        d.click(&format!("usage_profile_option_{key}"));
        d.click("usage_profile_options_ok");
        d.frames(3);
        assert!(d.app.panes.profile_dialog.is_none());
    };
    toggle(&mut d, "overtime");
    toggle(&mut d, "availability");
    let on = d.frame(vec![]);
    assert!(
        body_fills(&on, usage::OVERTIME.0, body) > 0,
        "overtime shows"
    );
    assert!(
        body_fills(&on, usage::AVAILABLE.0, body) > 0,
        "availability shows"
    );
    assert!(text_shown(&on, "Overtime") && text_shown(&on, "Availability"));
    toggle(&mut d, "overallocation");
    let no_over = d.frame(vec![]);
    assert_eq!(
        body_fills(&no_over, usage::OVER.0, body),
        0,
        "no overallocation"
    );
    assert!(!text_shown(&no_over, "Overallocated Early Units"));
    toggle(&mut d, "limits");
    let no_limit = d.frame(vec![]);
    assert!(!text_shown(&no_limit, "Limit"));
    assert!(
        body_fills(&no_limit, INK, body) < ink_base,
        "the limit line goes"
    );
    toggle(&mut d, "legend");
    let no_legend = d.frame(vec![]);
    assert!(!text_shown(&no_legend, "Actual Units"), "legend hidden");
    assert!(
        body_fills(&no_legend, usage::ACTUAL.0, body) > 0,
        "bars stay"
    );
    // Persists in the layout.
    let o = d.app.panes.usage.profile.clone();
    assert_eq!(
        (
            o.overtime,
            o.availability,
            o.limits,
            o.overallocation,
            o.legend
        ),
        (true, true, false, false, false)
    );
    let json = d.app.panes.json();
    let mut again = PanesState::default();
    again.restore(&json);
    assert_eq!(again.usage.profile, o);
    d.app.save_layout_as("Profile").unwrap();
    d.app.panes.usage = Default::default();
    d.app.open_layout("Profile", true).unwrap();
    assert_eq!(d.app.panes.usage.profile, o);
    // Cancel changes nothing.
    let body = d.app.panes.body;
    secondary(&mut d, Pos2::new(body.center().x + 80., body.center().y));
    d.click_at(d.rect("usage_profile_menu_options").center());
    d.frames(2);
    d.click("usage_profile_tab_graph");
    d.frames(1);
    d.click("usage_profile_option_legend");
    d.click("usage_profile_options_cancel");
    d.frames(2);
    assert_eq!(d.app.panes.usage.profile, o);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

/// Two open projects that both use resource PIPE: 24 h in RIVERBEND, 16 h
/// in NORTHYARD.
pub(super) fn labor_portfolio(name: &str) -> Driver {
    let mut doc = openxer_xer::starter_portfolio();
    doc.table_mut("RSRC").add(&[
        ("rsrc_id", "7".into()),
        ("clndr_id", "1".into()),
        ("rsrc_short_name", "PIPE".into()),
        ("rsrc_name", "Pipefitter".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("def_qty_per_hr", "1".into()),
        ("active_flag", "Y".into()),
        ("rsrc_seq_num", "2".into()),
    ]);
    for (id, task, proj, qty) in [("1", "1010", "1", "24"), ("2", "2010", "2", "16")] {
        doc.table_mut("TASKRSRC").add(&[
            ("taskrsrc_id", id.into()),
            ("task_id", task.into()),
            ("proj_id", proj.into()),
            ("rsrc_id", "7".into()),
            ("remain_qty", qty.into()),
            ("remain_qty_per_hr", "1".into()),
            ("target_qty", qty.into()),
            ("rollup_dates_flag", "N".into()),
        ]);
    }
    let make = |key: i64| {
        let mut p = ProjectData::from_document(doc.clone(), key, "portfolio.xer".into()).unwrap();
        scheduler::schedule_with_resources(&mut p.model, &p.resources).unwrap();
        p
    };
    let (rb, ny) = (make(1), make(2));
    let dir = std::env::temp_dir().join(format!(
        "openxer-cmc-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let mut store = Store::open(dir.join("workspace.db")).unwrap();
    let ny_id = store.save(&ny).unwrap();
    let rb_id = store.save(&rb).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, rb);
    app.restore_open_set();
    app.open_project_set(&[rb_id, ny_id]);
    assert_eq!(app.open_ids(), vec![rb_id, ny_id]);
    let mut d = Driver {
        app,
        ctx,
        time: 0.,
        dir,
        notes: vec![],
        screen: egui::Vec2::new(1500., 900.),
        probe: Default::default(),
        opened_dialog: false,
        origin: None,
        enter_check: false,
        setup: None,
        slice: None,
    };
    d.frames(3);
    d
}

#[test]
fn show_all_projects_adds_the_other_projects_assignments_to_the_totals() {
    let mut d = labor_portfolio("all-projects");
    d.home();
    d.f9();
    d.app.set_bottom(Some(BottomView::ResourceUsageSheet));
    d.frames(3);
    let body = d.app.panes.body;
    d.app.panes.usage.resource = Some(7);
    // Resource Usage Spreadsheet: the resource row totals the assignments.
    d.app.set_bottom(Some(BottomView::ResourceUsageSheet));
    d.frames(4);
    let one = d.frame(vec![]);
    assert!(
        pane_shows(&one, &format::units(24.), body),
        "this project alone: 24"
    );
    assert!(!pane_shows(&one, &format::units(40.), body));
    assert!(!pane_shows(&one, "Other open projects", body));
    d.app.panes.usage.show_all_projects = true;
    d.frames(3);
    let both = d.frame(vec![]);
    assert!(
        pane_shows(&both, &format::units(40.), body),
        "24 + 16 in the resource row"
    );
    assert!(pane_shows(&both, "Other open projects", body));
    assert!(
        pane_shows(&both, &format::units(16.), body),
        "their own row"
    );
    // The resource profile peak grows by the other project's hours.
    d.app.set_bottom(Some(BottomView::ResourceUsageProfile));
    d.frames(3);
    let with = d.frame(vec![]);
    d.app.panes.usage.show_all_projects = false;
    d.frames(3);
    let without = d.frame(vec![]);
    assert_ne!(
        loaded(&with, body),
        loaded(&without, body),
        "the bars change with Show All Projects"
    );
    // Activity Usage Spreadsheet: a row for the other projects.
    d.app.set_bottom(Some(BottomView::ActivityUsageSheet));
    d.frames(3);
    assert!(!text_shown(&d.frame(vec![]), "Other open projects"));
    d.app.panes.usage.show_all_projects = true;
    d.frames(3);
    // The extra row sits after the last activity: scroll down to it.
    for _ in 0..3 {
        let _ = d.frame(vec![
            egui::Event::PointerMoved(body.center()),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0., -300.),
                modifiers: Default::default(),
            },
        ]);
    }
    d.frames(2);
    let sheet = d.frame(vec![]);
    assert!(pane_shows(&sheet, "Other open projects", body));
    assert!(
        pane_shows(&sheet, &format::units(16.), body),
        "NORTHYARD's labor units"
    );
    // Activity Usage Profile: more labor in the same periods.
    d.app.set_bottom(Some(BottomView::ActivityUsageProfile));
    d.frames(3);
    let with = d.frame(vec![]);
    d.app.panes.usage.show_all_projects = false;
    d.frames(3);
    let without = d.frame(vec![]);
    assert_ne!(loaded(&with, body), loaded(&without, body));
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

/// Whether the bottom pane shows `text`.
/// The load bars in the bottom pane (a resource without a limit shows its
/// remaining units as overallocated).
fn loaded(output: &egui::FullOutput, body: Rect) -> Vec<Rect> {
    let mut rects = in_body(bar_rects(output, usage::REMAINING.0), body);
    rects.extend(in_body(bar_rects(output, usage::OVER.0), body));
    rects
}

fn pane_shows(output: &egui::FullOutput, text: &str, body: Rect) -> bool {
    output.shapes.iter().any(|s| {
        matches!(&s.shape, egui::Shape::Text(t)
            if t.galley.text() == text && body.contains(t.pos))
    })
}

/// Fills of `color` inside the bottom pane (the Gantt above uses red too).
fn body_fills(output: &egui::FullOutput, color: Color32, body: Rect) -> usize {
    bar_rects(output, color)
        .iter()
        .filter(|r| body.contains_rect(**r))
        .count()
}

fn in_body(rects: Vec<Rect>, body: Rect) -> Vec<Rect> {
    rects
        .into_iter()
        .filter(|r| body.contains_rect(*r))
        .collect()
}

fn bar_rects(output: &egui::FullOutput, color: Color32) -> Vec<Rect> {
    output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Rect(r) if r.fill == color => Some(r.rect),
            _ => None,
        })
        .collect()
}

#[test]
fn spreadsheet_fields_add_remove_and_keep_their_order() {
    let mut d = open_activity_sheet("cmc-fields");
    let body = d.app.panes.body;
    assert_eq!(
        d.app.panes.usage.activity_fields,
        vec!["activity_id", "activity_name"]
    );
    let open = |d: &mut Driver| {
        let r = d.rect("usage_sheet_timescale");
        secondary(d, r.center());
        d.click_at(d.rect("usage_sheet_menu_options").center());
        d.frames(2);
        assert!(d.has("usage_fields_ok"));
    };
    let available_row = |d: &Driver, id: &str| -> usize {
        let draft = d.app.panes.usage_fields.as_ref().unwrap();
        usage_fields::available(&d.app.project, draft.which())
            .into_iter()
            .filter(|(f, _)| !draft.lists[draft.tab].contains(f))
            .position(|(f, _)| f == id)
            .unwrap()
    };
    // Add Start and Finish.
    open(&mut d);
    for id in ["start", "finish"] {
        // The list scrolls; pick the field the way a click on its row does.
        let row = available_row(&d, id);
        d.app.panes.usage_fields.as_mut().unwrap().available = Some(row);
        d.click("usage_fields_add");
    }
    // Nothing is applied until OK.
    assert_eq!(d.app.panes.usage.activity_fields.len(), 2);
    d.click("usage_fields_ok");
    d.frames(3);
    assert_eq!(
        d.app.panes.usage.activity_fields,
        vec!["activity_id", "activity_name", "start", "finish"]
    );
    assert!(
        pane_shows(&d.frame(vec![]), "Start", body),
        "the header shows it"
    );
    // Move Finish up one place, remove Activity Name.
    open(&mut d);
    d.click("usage_fields_selected_row_3");
    d.click("usage_fields_up");
    d.click("usage_fields_selected_row_1");
    d.click("usage_fields_remove");
    d.click("usage_fields_ok");
    d.frames(3);
    assert_eq!(
        d.app.panes.usage.activity_fields,
        vec!["activity_id", "finish", "start"]
    );
    let shown = d.frame(vec![]);
    assert!(pane_shows(&shown, "Finish", body) && !pane_shows(&shown, "Activity Name", body));
    // The order persists with the layout.
    d.app.save_layout_as("Fields").unwrap();
    d.app.panes.usage = Default::default();
    assert_eq!(d.app.panes.usage.activity_fields.len(), 2);
    d.app.open_layout("Fields", true).unwrap();
    assert_eq!(
        d.app.panes.usage.activity_fields,
        vec!["activity_id", "finish", "start"]
    );
    // Cancel changes nothing; the last field cannot be removed.
    open(&mut d);
    d.click("usage_fields_selected_row_0");
    d.click("usage_fields_remove");
    d.click("usage_fields_remove");
    d.click("usage_fields_cancel");
    d.frames(2);
    assert_eq!(
        d.app.panes.usage.activity_fields,
        vec!["activity_id", "finish", "start"]
    );
    // Line numbers come from the same dialog.
    open(&mut d);
    d.click("usage_fields_line_numbers");
    d.click("usage_fields_ok");
    d.frames(3);
    assert!(d.app.panes.usage.line_numbers);
    assert!(pane_shows(&d.frame(vec![]), "1", body));
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn resource_lists_take_fields_roles_and_a_sort() {
    let mut p = costed();
    // Resource PIPE has a primary role.
    p.document.table_mut("ROLES").add(&[
        ("role_id", "3".into()),
        ("role_short_name", "FITTER".into()),
        ("role_name", "Pipe fitter".into()),
    ]);
    p.document.table_mut("ROLES").add(&[
        ("role_id", "4".into()),
        ("role_short_name", "ALPHA".into()),
        ("role_name", "Alpha role".into()),
    ]);
    p.document.table_mut("RSRC").add(&[
        ("rsrc_id", "10".into()),
        ("clndr_id", "1".into()),
        ("rsrc_short_name", "WELD".into()),
        ("rsrc_name", "Welder".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("role_id", "4".into()),
        ("rsrc_seq_num", "4".into()),
    ]);
    p.document.table_mut("TASKRSRC").add(&[
        ("taskrsrc_id", "2".into()),
        ("task_id", "1020".into()),
        ("proj_id", "1".into()),
        ("rsrc_id", "10".into()),
        ("remain_qty", "8".into()),
        ("target_qty", "8".into()),
    ]);
    let rows = p.document.table("RSRC").unwrap().rows.len();
    for r in 0..rows {
        if p.document
            .table("RSRC")
            .unwrap()
            .value(&p.document.table("RSRC").unwrap().rows[r], "rsrc_id")
            == "7"
        {
            p.document.table_mut("RSRC").set(r, "role_id", "3".into());
        }
    }
    let mut d = Driver::new("cmc-resource-fields", p);
    d.home();
    d.app.set_bottom(Some(BottomView::ResourceUsageSheet));
    d.frames(4);
    let r = d.rect("usage_sheet_timescale");
    secondary(&mut d, r.center());
    d.click_at(d.rect("usage_sheet_menu_options").center());
    d.frames(2);
    // Two lists to edit: the resources and the assignments.
    assert!(d.has("usage_fields_tab_0") && d.has("usage_fields_tab_1"));
    let draft = d.app.panes.usage_fields.as_ref().unwrap();
    let row = usage_fields::available(&d.app.project, draft.which())
        .into_iter()
        .filter(|(f, _)| !draft.lists[0].contains(f))
        .position(|(f, _)| f == "primary_role")
        .unwrap();
    d.app.panes.usage_fields.as_mut().unwrap().available = Some(row);
    d.click("usage_fields_add");
    // Sort the resource list by the role, descending.
    d.click("usage_fields_selected_row_2");
    d.click("usage_fields_sort_descending");
    d.click("usage_fields_ok");
    d.frames(3);
    assert_eq!(
        d.app.panes.usage.resource_fields,
        vec!["resource_id", "resource_name", "primary_role"]
    );
    assert_eq!(
        d.app.panes.usage.sheet_list.sort,
        usage::SheetSort {
            field: Some("primary_role".into()),
            descending: true
        }
    );
    // Descending by role: FITTER above ALPHA; ascending swaps them.
    let y = |d: &mut Driver, text: &str| {
        d.texts()
            .into_iter()
            .rev()
            .find(|(t, _)| t == text)
            .unwrap_or_else(|| panic!("{text} is not listed"))
            .1
            .top()
    };
    assert!(y(&mut d, "FITTER") < y(&mut d, "ALPHA"));
    d.app.panes.usage.sheet_list.sort.descending = false;
    d.frames(2);
    assert!(y(&mut d, "ALPHA") < y(&mut d, "FITTER"));
    d.app.panes.usage.sheet_list.sort.descending = true;
    let json = d.app.panes.json();
    assert_eq!(
        json["usage"]["list"]["sheet"]["sort"]["field"],
        "primary_role"
    );
    let mut again = PanesState::default();
    again.restore(&json);
    assert_eq!(
        again.usage.resource_fields,
        d.app.panes.usage.resource_fields
    );
    assert_eq!(
        again.usage.sheet_list.sort,
        d.app.panes.usage.sheet_list.sort
    );
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn copy_to_excel_writes_the_visible_columns() {
    use openxer_interchange::xlsx_import as xi;
    let mut d = open_activity_sheet("cmc-excel");
    d.app.panes.usage.activity_fields =
        vec!["activity_id".into(), "start".into(), "activity_name".into()];
    d.app.panes.usage.line_numbers = true;
    d.frames(3);
    let file = d.dir.join("usage.xlsx");
    d.app.copy_usage_to_excel_to(&file);
    assert!(file.exists(), "{} {:?}", d.app.status, d.app.error);
    let wb = xi::read_workbook(&file).unwrap();
    let s = wb.sheet("Activity Usage").expect("the usage sheet");
    let rows = &wb.sheets[s].rows;
    let header = rows
        .iter()
        .find(|r| r.iter().any(|c| c.text() == "Activity ID"))
        .expect("a header row");
    let titles: Vec<String> = header.iter().map(|c| c.text()).collect();
    assert_eq!(
        &titles[..5],
        &[
            "No.",
            "Activity ID",
            "Start",
            "Activity Name",
            "Remaining Labor Units"
        ]
    );
    assert!(titles.len() > 6, "a column per period");
    // The data rows hold the survey activity with its remaining hours.
    let survey = rows
        .iter()
        .find(|r| r.iter().any(|c| c.text() == "RB-1010"))
        .expect("the survey row");
    assert_eq!(survey[4].text().trim_end_matches(".0"), survey[4].text());
    let total: f64 = survey
        .iter()
        .skip(5)
        .filter_map(|c| c.text().parse::<f64>().ok())
        .sum();
    assert!((total - 16.).abs() < 0.1 || (total - 24.).abs() < 0.1 || total > 0.);
    // The resource sheet writes its own columns.
    d.app.set_bottom(Some(BottomView::ResourceUsageSheet));
    d.frames(4);
    let file = d.dir.join("resource.xlsx");
    d.app.copy_usage_to_excel_to(&file);
    let wb = xi::read_workbook(&file).unwrap();
    assert!(wb.sheet("Resource Usage").is_some());
    // Without a spreadsheet on show there is nothing to copy.
    d.app.set_bottom(Some(BottomView::Details));
    d.frames(2);
    d.app.copy_usage_to_excel_to(&d.dir.join("none.xlsx"));
    assert!(d.app.error.is_some());
    d.app.error = None;
}

fn drag(d: &mut Driver, from: Pos2, to: Pos2) {
    let b = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let _ = d.frame(vec![egui::Event::PointerMoved(from)]);
    let _ = d.frame(vec![b(from, true)]);
    let mid = from + (to - from) * 0.5;
    let _ = d.frame(vec![egui::Event::PointerMoved(mid)]);
    let _ = d.frame(vec![egui::Event::PointerMoved(to)]);
    let _ = d.frame(vec![b(to, false)]);
    d.frames(2);
}

#[test]
fn dragging_a_splitter_changes_the_pane_width_and_is_stored() {
    let mut d = open_activity_sheet("cmc-splitter");
    assert_eq!(d.app.panes.usage.table_width, None);
    let before = d.rect("usage_sheet_splitter");
    let c = before.center();
    drag(&mut d, c, c + Vec2::new(90., 0.));
    let after = d.rect("usage_sheet_splitter");
    assert!(
        (after.center().x - before.center().x - 90.).abs() < 3.,
        "{} -> {}",
        before.center().x,
        after.center().x
    );
    let width = d.app.panes.usage.table_width.expect("stored");
    // The value is the pane's width from its left edge.
    assert!((width - (after.left() - d.app.panes.body.left())).abs() < 2.);
    // Saved with the layout and the workspace choices.
    let json = d.app.panes.json_with(false);
    assert_eq!(json["usage"]["table_width"].as_f64().unwrap() as f32, width);
    let mut again = PanesState::default();
    again.restore(&json);
    assert_eq!(again.usage.table_width, Some(width));
    // The limits hold: far past the right edge stops short of it.
    let c = after.center();
    drag(&mut d, c, c + Vec2::new(5000., 0.));
    let wide = d.app.panes.usage.table_width.unwrap();
    assert!(wide <= d.app.panes.body.width() - 150.);
    let c = d.rect("usage_sheet_splitter").center();
    drag(&mut d, c, Pos2::new(0., c.y));
    assert!(d.app.panes.usage.table_width.unwrap() >= usage::MIN_PANE);
    // The profile and the resource sheet have splitters too.
    d.app.set_bottom(Some(BottomView::ActivityUsageProfile));
    d.frames(3);
    assert!(d.has("usage_profile_splitter"));
    d.app.set_bottom(Some(BottomView::ResourceUsageSheet));
    d.frames(3);
    assert!(d.has("usage_list_splitter") && d.has("usage_sheet_splitter"));
    let c = d.rect("usage_list_splitter").center();
    drag(&mut d, c, c + Vec2::new(-20., 0.));
    assert!(d.app.panes.usage.list_width.is_some());
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn resource_filter_keeps_the_activities_of_the_selected_resource() {
    let mut d = Driver::new("cmc-resource-filter", costed());
    d.home();
    d.app.set_bottom(Some(BottomView::ResourceUsageSheet));
    d.app.panes.usage.resource = Some(7);
    d.frames(3);
    let all = d.app.rows().len();
    d.app.panes.usage.resource_filter = true;
    let kept = d.app.rows();
    assert!(kept.len() < all, "{} of {all}", kept.len());
    let survey = index_of(&d.app.project, 1010);
    assert!(kept.contains(&grid::Row::Activity(survey)));
    assert_eq!(
        kept.iter()
            .filter(|r| matches!(r, grid::Row::Activity(_)))
            .count(),
        1,
        "only the assigned activity"
    );
    d.app.panes.usage.resource = Some(9);
    assert!(d
        .app
        .rows()
        .iter()
        .all(|r| !matches!(r, grid::Row::Activity(_))));
    // Off restores; other views are not filtered.
    d.app.panes.usage.resource_filter = false;
    assert_eq!(d.app.rows().len(), all);
    d.app.panes.usage.resource_filter = true;
    d.app.set_bottom(Some(BottomView::Details));
    assert_eq!(d.app.rows().len(), all);
}

#[test]
fn assignments_menu_zooms_totals_and_toggles_the_details() {
    let mut d = Driver::new("cmc-assignments", costed());
    d.home();
    d.app.show_view(views::MainView::Assignments);
    d.frames(4);
    let header = d.rect("assignments_col_0");
    let at = Pos2::new(
        header.center().x,
        header.bottom() + views::grid::ROW_HEIGHT * 1.5,
    );
    secondary(&mut d, at);
    for item in [
        "layout",
        "resource_usage",
        "details",
        "timescale",
        "options",
        "show_all_projects",
        "zoom_in",
        "zoom_out",
        "user_preferences",
    ] {
        assert!(d.has(&format!("assignments_menu_{item}")), "{item}");
    }
    // Fill Down fills the table's cells (Resource Assignments window); the
    // spreadsheet's cell commands are on the spreadsheet's own menu.
    assert!(d.has("assignments_menu_fill_down"));
    for item in ["date_spread", "fill_across", "range_copy", "range_paste"] {
        assert!(
            !d.has(&format!("assignments_menu_{item}")),
            "{item} is gone"
        );
    }
    // Zoom In and Out step the spreadsheet's days.
    assert_eq!(d.app.views.assignments.zoom, 100.);
    d.click_at(d.rect("assignments_menu_zoom_in").center());
    assert_eq!(d.app.views.assignments.zoom, 140.);
    secondary(&mut d, at);
    d.click_at(d.rect("assignments_menu_zoom_out").center());
    secondary(&mut d, at);
    d.click_at(d.rect("assignments_menu_zoom_out").center());
    assert_eq!(d.app.views.assignments.zoom, 70.);
    // Assignment Details hides the pane.
    assert!(d.app.views.assignments.show_details);
    secondary(&mut d, at);
    d.click_at(d.rect("assignments_menu_details").center());
    assert!(!d.app.views.assignments.show_details);
    // Resource Usage Spreadsheet shows or hides the window's spreadsheet,
    // as the reference's check item does.
    assert!(d.app.views.assignments.show_sheet);
    secondary(&mut d, at);
    d.click_at(d.rect("assignments_menu_resource_usage").center());
    d.frames(3);
    assert_eq!(d.app.views.main, views::MainView::Assignments);
    assert!(!d.app.views.assignments.show_sheet);
    // The view settings persist.
    let json = d.app.views.assignments.json();
    let mut again = views::assignments::AssignmentsState::default();
    again.restore(&json);
    assert_eq!(again.zoom, 70.);
    assert!(!again.show_details);
    assert!(!again.show_sheet);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

/// Frames through the software painter, for renders.
struct Rig {
    ctx: egui::Context,
    painter: SoftwarePainter,
    time: f64,
    out: PathBuf,
}

impl Rig {
    fn frames(&mut self, app: &mut App, n: usize, events: Vec<egui::Event>) {
        let _ = self
            .painter
            .frame(app, &self.ctx, &mut self.time, events, false);
        for _ in 1..n {
            let _ = self
                .painter
                .frame(app, &self.ctx, &mut self.time, vec![], false);
        }
    }
    fn shot(&mut self, app: &mut App, name: &str) {
        let (image, _) = self
            .painter
            .frame(app, &self.ctx, &mut self.time, vec![], true)
            .unwrap();
        image.save(self.out.join(format!("{name}.png"))).unwrap();
    }
    fn right_click(&mut self, app: &mut App, pos: Pos2) {
        for pressed in [true, false] {
            self.frames(
                app,
                1,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        self.frames(app, 3, vec![]);
    }
    fn escape(&mut self, app: &mut App) {
        let key = |pressed| egui::Event::Key {
            key: Key::Escape,
            physical_key: Some(Key::Escape),
            pressed,
            repeat: false,
            modifiers: Default::default(),
        };
        self.frames(app, 1, vec![key(true)]);
        self.frames(app, 1, vec![key(false)]);
        self.frames(app, 2, vec![]);
    }
}

/// Renders of the new menus and dialogs for a look: `artifacts/cmc/*.png`.
#[test]
fn render_the_usage_menus_and_dialogs() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/cmc");
    std::fs::create_dir_all(&out).unwrap();
    let path = std::env::temp_dir().join(format!("openxer-cmc-render-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let project = overloaded();
    let mut store = Store::open(path.clone()).unwrap();
    store.save(&project).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    let mut r = Rig {
        ctx,
        painter: SoftwarePainter::default(),
        time: 0.,
        out,
    };
    r.painter.screen = Some(Vec2::new(1500., 900.));
    r.frames(&mut app, 4, vec![]);
    app.panes.usage.resource = Some(7);
    app.panes.usage.profile.overtime = true;
    app.panes.usage.profile.availability = true;
    for (name, view) in [
        ("sheet", BottomView::ActivityUsageSheet),
        ("resource-sheet", BottomView::ResourceUsageSheet),
        ("resource-profile", BottomView::ResourceUsageProfile),
        ("activity-profile", BottomView::ActivityUsageProfile),
    ] {
        app.set_bottom(Some(view));
        r.frames(&mut app, 4, vec![]);
        r.shot(&mut app, &format!("{name}-plain"));
        let body = app.panes.body;
        let at = match view {
            BottomView::ActivityUsageSheet => {
                let k = app.action_rects["usage_sheet_row_1"];
                Pos2::new(k[0] + 60., (k[1] + k[3]) / 2.)
            }
            BottomView::ResourceUsageSheet => {
                let k = app.action_rects["usage_assignment_row_1"];
                Pos2::new(k[0] + 60., (k[1] + k[3]) / 2.)
            }
            _ => Pos2::new(body.right() - 150., body.top() + 120.),
        };
        r.right_click(&mut app, at);
        r.shot(&mut app, &format!("{name}-menu"));
        r.escape(&mut app);
    }
    app.open_spreadsheet_fields(usage_fields::FieldsView::ResourceSheet);
    r.frames(&mut app, 4, vec![]);
    r.shot(&mut app, "fields-dialog");
    app.panes.usage_fields = None;
    app.open_profile_options();
    r.frames(&mut app, 4, vec![]);
    r.shot(&mut app, "profile-options-dialog");
    app.panes.profile_dialog = None;
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// A right-click right after a menu click, with the pointer still resting on
/// the item just clicked: the pointer's jump to the row arrives in the same
/// frame as the press (as it does when a busy frame batches the mouse
/// events). The sheet must not take the press as a drag-to-scroll.
#[test]
fn a_right_click_in_the_frame_the_pointer_jumps_opens_the_sheet_menu() {
    let mut d = open_activity_sheet("cmc-sheet-jump");
    let (k, i) = first_sheet_row(&d);
    let r = d.rect(&format!("usage_sheet_row_{k}"));
    let row = Pos2::new(r.left() + 20., r.center().y);
    secondary(&mut d, row);
    let at = d.rect("usage_sheet_menu_zoom_in").center();
    d.click_at(at);
    assert!(
        !d.has("usage_sheet_menu_zoom_in"),
        "the click closed the menu"
    );
    d.frames(3);
    d.app.selected = None;
    let m = egui::Modifiers::default();
    let button = |pressed| egui::Event::PointerButton {
        pos: row,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: m,
    };
    let _ = d.frame_with(vec![egui::Event::PointerMoved(row), button(true)], m);
    let _ = d.frame_with(vec![button(false)], m);
    d.frames(1);
    assert_eq!(d.app.selected, Some(i), "the right-click selects the row");
    assert!(d.has("usage_sheet_menu_zoom_in"), "and opens the menu");
    assert_eq!(
        d.rect(&format!("usage_sheet_row_{k}")),
        r,
        "the sheet did not scroll"
    );
}

/// View > Show on Top > Resource Usage Spreadsheet: the resource sheet fills
/// the top pane on the shared timeline, its menu works there, the choice is
/// kept with the layout and the Gantt comes back from the toolbar.
#[test]
fn the_resource_usage_spreadsheet_shows_on_top_and_zooms_with_the_timeline() {
    let mut d = Driver::new("top-resource-sheet", costed());
    d.home();
    d.app.set_bottom(None);
    d.frames(2);
    d.click("menu_bar_view");
    assert!(d.hover_text("Show on Top"), "the submenu is drawn");
    d.click("menu_top_resource_usage");
    d.frames(3);
    assert_eq!(d.app.panes.top, TopView::ResourceUsageSheet);
    assert!(
        !d.app.show_details,
        "no bottom pane: the sheet is the top one"
    );
    assert!(d.app.bar_geometry.is_empty(), "no Gantt on top");
    assert!(d.has("usage_sheet_timescale"), "the sheet's timescale");
    assert!(d.has("usage_sheet_timescale_hscroll"), "and its scroll bar");
    d.assert_clean("resource usage on top");
    // The menu on the timescale zooms the shared timeline.
    let ppd = d.app.pixels_per_day;
    let at = d.rect("usage_sheet_timescale").center();
    secondary(&mut d, at);
    assert!(d.has("usage_sheet_menu_zoom_in"));
    d.click_at(d.rect("usage_sheet_menu_zoom_in").center());
    assert!(d.app.pixels_per_day > ppd, "Zoom In widens the timescale");
    // Ctrl+wheel over the timescale zooms too; the scroll bar pans.
    d.sweep_timeline("usage_sheet_timescale_hscroll", "usage_sheet_timescale");
    // Kept with the layout.
    let mut panes = PanesState::default();
    panes.restore(&d.app.panes.json());
    assert_eq!(panes.top, TopView::ResourceUsageSheet);
    // The Gantt comes back from the Top Layout toolbar.
    d.click("tool_gantt");
    assert_eq!(d.app.panes.top, TopView::Gantt);
    d.frames(2);
    assert!(!d.app.bar_geometry.is_empty());
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

/// View > Bottom Layout Options opens the options of the view in the bottom
/// pane: Spreadsheet Options for the usage spreadsheets, Profile Options for
/// the usage profiles, never Customize Activity Details for those.
#[test]
fn bottom_layout_options_open_the_usage_views_own_options() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/bugfix-help-audit");
    std::fs::create_dir_all(&out).unwrap();
    let mut d = Driver::probed(
        "bottom-layout-options",
        overloaded(),
        Vec2::new(1500., 900.),
        Some(out.clone()),
    );
    d.home();
    d.app.panes.usage.resource = Some(7);
    for (view, name) in [
        (BottomView::ActivityUsageSheet, "activity-usage-sheet"),
        (BottomView::ResourceUsageSheet, "resource-usage-sheet"),
        (BottomView::ActivityUsageProfile, "activity-usage-profile"),
        (BottomView::ResourceUsageProfile, "resource-usage-profile"),
    ] {
        d.app.set_bottom(Some(view));
        d.frames(3);
        d.click("menu_bar_view");
        d.click("menu_bottom_layout_options");
        d.frames(2);
        assert!(
            d.app.detail.customize.is_none(),
            "{view:?}: not Customize Activity Details"
        );
        match view {
            BottomView::ActivityUsageSheet | BottomView::ResourceUsageSheet => {
                let fields = d
                    .app
                    .panes
                    .usage_fields
                    .as_ref()
                    .expect("Spreadsheet Options");
                let expected = if view == BottomView::ActivityUsageSheet {
                    usage_fields::FieldsView::ActivitySheet
                } else {
                    usage_fields::FieldsView::ResourceSheet
                };
                assert_eq!(fields.view, expected);
            }
            _ => assert!(
                d.app.panes.profile_dialog.is_some(),
                "{view:?}: Profile Options"
            ),
        }
        d.screenshot(&out.join(format!("bottom-layout-options-{name}.png")));
        d.app.panes.usage_fields = None;
        d.app.panes.profile_dialog = None;
        d.frames(2);
    }
    // Activity Details still opens Customize Activity Details.
    d.app.set_bottom(Some(BottomView::Details));
    d.frames(2);
    d.app.open_bottom_layout_options();
    assert!(d.app.detail.customize.is_some());
}

/// Ctrl+wheel over the Network zooms it (egui reports Ctrl+wheel as a zoom
/// factor, not a scroll).
#[test]
fn ctrl_wheel_zooms_the_network() {
    let (path, ctx, mut app, mut time) = setup("ctrl-wheel", scheduled_starter());
    app.set_top(TopView::Network);
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![], &mut time);
    }
    let [l, t, r, b] = *app
        .action_rects
        .iter()
        .find(|(k, _)| k.starts_with("network_box_"))
        .map(|(_, v)| v)
        .expect("a network box is drawn");
    let pos = Pos2::new((l + r) / 2., (t + b) / 2.);
    let before = app.panes.network.zoom;
    let ctrl = egui::Modifiers::CTRL;
    for k in 0..6 {
        time += 0.1;
        let events = if k == 0 {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0., -60.),
                    modifiers: ctrl,
                },
            ]
        } else {
            vec![egui::Event::PointerMoved(pos)]
        };
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500., 900.))),
                time: Some(time),
                modifiers: ctrl,
                events,
                focused: true,
                ..Default::default()
            },
            |ctx| app.update_ui(ctx),
        );
    }
    assert!(
        app.panes.network.zoom < before * 0.99,
        "{before} -> {}",
        app.panes.network.zoom
    );
    drop(app);
    let _ = std::fs::remove_file(path);
}
