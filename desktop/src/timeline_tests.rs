//! Every timeline scrolls and zooms like the Activities Gantt: a scroll bar
//! that moves the visible range, Ctrl+wheel zoom around the pointer, the zoom commands acting on the focused timeline,
//! header drags (upper band pans, lowest band zooms) and rows in step with
//! the table. Everything runs through `App::update_ui` with synthetic egui
//! input; the renders go to the ignored `artifacts/timelines/`.
use super::*;
use crate::headless_render::SoftwarePainter;
use crate::panes::BottomView;
use crate::qa_sweep::Driver;
use crate::timeline::Span;
use crate::views::MainView;

fn scheduled_starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn wheel(d: &mut Driver, pos: Pos2, delta: Vec2, modifiers: egui::Modifiers) {
    let _ = d.frame_with(
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta,
                modifiers,
            },
        ],
        modifiers,
    );
    d.frames(2);
}

/// Press at `from`, move to `to` in steps, release; then let the UI settle.
fn drag(d: &mut Driver, from: Pos2, to: Pos2) {
    let _ = d.frame(vec![egui::Event::PointerMoved(from)]);
    let _ = d.frame(vec![
        egui::Event::PointerMoved(from),
        egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        },
    ]);
    for step in [0.25, 0.5, 0.75, 1.] {
        let _ = d.frame(vec![egui::Event::PointerMoved(from.lerp(to, step))]);
    }
    let _ = d.frame(vec![egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    }]);
    let _ = d.frame(vec![egui::Event::PointerGone]);
    d.frames(2);
}

/// A view's timeline as the tests read it: first minute and scale.
#[derive(Clone, Copy)]
enum Which {
    Wbs,
    Projects,
    Assignments,
}

impl Which {
    fn id(self) -> &'static str {
        match self {
            Self::Wbs => "wbs",
            Self::Projects => "projects",
            Self::Assignments => "assignments",
        }
    }

    fn view(self) -> MainView {
        match self {
            Self::Wbs => MainView::Wbs,
            Self::Projects => MainView::Projects,
            Self::Assignments => MainView::Assignments,
        }
    }

    /// First minute shown and scale in pixels a week.
    fn span(self, app: &App) -> Span {
        let v = &app.views;
        let (first, default, zoom) = match self {
            Self::Wbs => (v.wbs.timeline_first, v.wbs.timeline_default, v.wbs.zoom),
            Self::Projects => (
                v.projects.timeline_first,
                v.projects.timeline_default,
                v.projects.zoom,
            ),
            Self::Assignments => (
                v.assignments.first_minute,
                v.assignments.timeline_default,
                v.assignments.zoom,
            ),
        };
        Span::new(first.unwrap_or(default), zoom)
    }

    fn place(self, app: &App) -> crate::timeline::Place {
        let v = &app.views;
        match self {
            Self::Wbs => v.wbs.timeline_place,
            Self::Projects => v.projects.timeline_place,
            Self::Assignments => v.assignments.timeline_place,
        }
    }
}

fn open(which: Which) -> Driver {
    let mut d = Driver::new(&format!("timeline-{}", which.id()), scheduled_starter());
    d.app.show_view(which.view());
    // The details panes leave the chart a few rows; hide them.
    d.app.views.wbs.show_details = false;
    d.app.views.projects.show_details = false;
    d.app.views.assignments.show_details = false;
    d.frames(4);
    assert_eq!(d.app.views.main, which.view());
    d
}

/// The x of `minute` on the view's chart.
/// Zoom a view's chart in to 200 px a week, so its few weeks of bars are
/// wider than the chart and the bar can scroll both ways (at the default
/// zoom the whole project fits and, as on the Gantt, the view may only move
/// back toward it).
fn zoom_in(d: &mut Driver, which: Which) {
    let v = &mut d.app.views;
    match which {
        Which::Wbs => v.wbs.zoom = 200.,
        Which::Projects => v.projects.zoom = 200.,
        Which::Assignments => v.assignments.zoom = 200.,
    }
    d.frames(3);
}

fn x_of(d: &Driver, which: Which, minute: f64) -> f32 {
    which.span(&d.app).axis(which.place(&d.app).left).x(minute)
}

fn minute_at(d: &Driver, which: Which, x: f32) -> f64 {
    which.span(&d.app).axis(which.place(&d.app).left).x_date(x)
}

const ALL: [Which; 3] = [Which::Wbs, Which::Projects, Which::Assignments];

#[test]
fn every_view_chart_has_a_scroll_bar_that_moves_the_visible_range() {
    for which in ALL {
        let mut d = open(which);
        zoom_in(&mut d, which);
        let id = which.id();
        let bar = format!("{id}_hscroll");
        assert!(d.has(&bar), "{id}: the chart has a scroll bar");
        assert_eq!(d.rect(&bar).height(), crate::hscroll::BAR_HEIGHT);
        // The bar runs under the chart, from the time axis's left edge.
        let chart = d.rect(&format!("{id}_chart"));
        assert!((d.rect(&bar).left() - chart.left()).abs() < 0.5, "{id}");
        assert!(d.rect(&bar).top() >= chart.bottom() - 0.5, "{id}");
        // An arrow steps one cell of the lowest band.
        let before = which.span(&d.app).first;
        d.click(&format!("{bar}_right"));
        let after = which.span(&d.app).first;
        assert!(after > before, "{id}: arrow right {before} -> {after}");
        d.click(&format!("{bar}_left"));
        assert!(
            which.span(&d.app).first < after,
            "{id}: arrow left came back"
        );
        // Dragging the thumb moves the range, and the zoom stays.
        let zoom = which.span(&d.app).pixels_per_week;
        let thumb = d.rect(&format!("{bar}_thumb"));
        let before = which.span(&d.app).first;
        drag(&mut d, thumb.center(), thumb.center() + Vec2::new(60., 2.));
        assert!(
            which.span(&d.app).first > before,
            "{id}: thumb drag {before} -> {}",
            which.span(&d.app).first
        );
        assert_eq!(which.span(&d.app).pixels_per_week, zoom, "{id}");
        // A click on the track pages one view.
        let thumb = d.rect(&format!("{bar}_thumb"));
        let start = which.span(&d.app).first;
        let back = Pos2::new(
            (thumb.left() + d.rect(&bar).left() + 17.) / 2.,
            thumb.center().y,
        );
        if back.x < thumb.left() - 2. {
            d.click_at(back);
            d.frames(2);
            assert!(which.span(&d.app).first < start, "{id}: page back");
        }
        d.assert_clean(id);
    }
}

#[test]
fn ctrl_wheel_zooms_every_view_chart_around_the_pointer() {
    for which in ALL {
        let mut d = open(which);
        let id = which.id();
        let chart = d.rect(&format!("{id}_chart"));
        let pointer = Pos2::new(chart.left() + chart.width() * 0.4, chart.top() + 30.);
        let date = minute_at(&d, which, pointer.x);
        let before = which.span(&d.app).pixels_per_week;
        let ppp = d.ctx.pixels_per_point();
        wheel(&mut d, pointer, Vec2::new(0., 120.), egui::Modifiers::CTRL);
        let after = which.span(&d.app).pixels_per_week;
        assert!(
            after > before,
            "{id}: Ctrl+wheel zooms in {before} -> {after}"
        );
        assert!(
            (x_of(&d, which, date) - pointer.x).abs() <= 1.1,
            "{id}: the date under the pointer stays put"
        );
        assert_eq!(d.ctx.pixels_per_point(), ppp, "{id}: the UI scale stays");
        // Shift+wheel and a sideways wheel pan.
        let first = which.span(&d.app).first;
        wheel(
            &mut d,
            pointer,
            Vec2::new(0., -120.),
            egui::Modifiers::SHIFT,
        );
        assert!(which.span(&d.app).first > first, "{id}: Shift+wheel pans");
        let first = which.span(&d.app).first;
        let header = d.rect(&format!("{id}_timescale_pan")).center();
        wheel(&mut d, header, Vec2::new(120., 0.), Default::default());
        assert!(
            which.span(&d.app).first < first,
            "{id}: a sideways wheel over the timescale pans"
        );
        d.assert_clean(id);
    }
}

#[test]
fn header_drags_pan_and_zoom_every_view_chart_like_the_gantt() {
    for which in ALL {
        let mut d = open(which);
        let id = which.id();
        let pan = d.rect(&format!("{id}_timescale_pan"));
        let zoom = d.rect(&format!("{id}_timescale_zoom"));
        assert_eq!(pan.bottom(), zoom.top(), "{id}: the bands meet");
        let left = which.place(&d.app).left;
        let date = minute_at(&d, which, left);
        let scale = which.span(&d.app).pixels_per_week;
        drag(&mut d, pan.center(), pan.center() + Vec2::new(72., 0.));
        assert_eq!(which.span(&d.app).pixels_per_week, scale, "{id}");
        assert!(
            (x_of(&d, which, date) - left - 72.).abs() <= 1.1,
            "{id}: a 72 px pan moves the left-edge date 72 px"
        );
        let first = which.span(&d.app).first;
        let zoom = d.rect(&format!("{id}_timescale_zoom"));
        drag(&mut d, zoom.center(), zoom.center() + Vec2::new(20., 0.));
        let after = which.span(&d.app).pixels_per_week;
        assert!(
            after > scale * 1.15,
            "{id}: lower band drag zooms {scale} -> {after}"
        );
        assert!(
            (which.span(&d.app).first - first).abs() < 1.,
            "{id}: the zoom keeps its left-edge anchor"
        );
        d.assert_clean(id);
    }
}

#[test]
fn zoom_commands_act_on_the_view_that_shows() {
    for which in ALL {
        let mut d = open(which);
        let id = which.id();
        // The Display toolbar's and the View menu's Zoom are on for the view.
        assert!(d.app.timeline_on_screen(), "{id}");
        let place = which.place(&d.app);
        let centre = place.left + place.width / 2.;
        let date = minute_at(&d, which, centre);
        let gantt = d.app.pixels_per_day;
        let before = which.span(&d.app).pixels_per_week;
        d.app.zoom_step(true);
        d.frames(2);
        let after = which.span(&d.app).pixels_per_week;
        assert!(after > before, "{id}: Zoom In {before} -> {after}");
        assert_eq!(
            d.app.pixels_per_day, gantt,
            "{id}: the Gantt keeps its zoom"
        );
        if !matches!(which, Which::Assignments) {
            assert!(
                (x_of(&d, which, date) - centre).abs() <= 1.1,
                "{id}: Zoom In keeps the chart's centre date"
            );
        }
        d.app.zoom_step(false);
        d.frames(2);
        assert!(which.span(&d.app).pixels_per_week < after, "{id}: Zoom Out");
        // Zoom To Best Fit shows every bar.
        d.app.zoom_to_best_fit();
        d.frames(2);
        let place = which.place(&d.app);
        let (start, finish) = place.extent.unwrap();
        assert!(
            x_of(&d, which, start as f64) >= place.left - 0.5,
            "{id} fit start"
        );
        assert!(
            x_of(&d, which, finish as f64) <= place.left + place.width + 0.5,
            "{id} fit finish"
        );
        d.assert_clean(id);
    }
}

#[test]
fn the_view_charts_scroll_with_their_table_rows() {
    for which in [Which::Wbs, Which::Projects] {
        let d = open(which);
        let id = which.id();
        let bars: Vec<(usize, Rect)> = d
            .app
            .action_rects
            .iter()
            .filter_map(|(k, r)| {
                let row = k.strip_prefix(&format!("{id}_bar_"))?.parse().ok()?;
                Some((
                    row,
                    Rect::from_min_max(Pos2::new(r[0], r[1]), Pos2::new(r[2], r[3])),
                ))
            })
            .collect();
        assert!(!bars.is_empty(), "{id} draws bars");
        for (row, bar) in bars {
            let line = d.rect(&format!("{id}_row_{row}"));
            assert!(
                bar.top() >= line.top() && bar.bottom() <= line.bottom() + 1.,
                "{id}: bar of row {row} {bar:?} sits in its row {line:?}"
            );
        }
    }
}

#[test]
fn the_view_zoom_is_saved_with_the_view_and_old_layouts_still_load() {
    let mut d = open(Which::Wbs);
    d.app.views.wbs.zoom = 100.;
    let json = d.app.views.wbs.json();
    let mut other = crate::views::wbs::WbsState::default();
    other.restore(&json);
    assert_eq!(other.zoom, 100.);
    // A layout saved before the chart could zoom keeps the default.
    let mut old = crate::views::wbs::WbsState::default();
    old.restore(&serde_json::json!({"table_width": 500.}));
    assert_eq!(old.zoom, crate::views::chart::WEEK_PITCH);
    assert_eq!(old.table_width, 500.);
    let mut projects = crate::views::projects::ProjectsState::default();
    projects.restore(&serde_json::json!({"zoom": 13.}));
    assert_eq!(projects.zoom, 13.);
    projects.restore(&serde_json::json!({"zoom": -4.}));
    assert_eq!(projects.zoom, 13., "an unusable zoom is ignored");
    // The assignments spreadsheet keeps any zoom inside its range.
    let mut sheet = crate::views::assignments::AssignmentsState::default();
    sheet.restore(&serde_json::json!({"zoom": 123.}));
    assert_eq!(sheet.zoom, 123.);
}

/// The Activities view with a bottom pane, scheduled starter.
fn activities_with(bottom: BottomView) -> Driver {
    let mut d = Driver::new(
        "timeline-usage",
        crate::views::resources::tests::with_resource(),
    );
    schedule_model(&mut d.app.project.model, &d.app.project.resources).unwrap();
    d.app.set_bottom(Some(bottom));
    d.frames(4);
    d
}

#[test]
fn usage_views_scroll_and_zoom_with_the_gantt_above_them() {
    for (bottom, name) in [
        (BottomView::ActivityUsageProfile, "usage_profile_timescale"),
        (BottomView::ResourceUsageProfile, "usage_profile_timescale"),
        (BottomView::ActivityUsageSheet, "usage_sheet_timescale"),
        (BottomView::ResourceUsageSheet, "usage_sheet_timescale"),
    ] {
        let mut d = activities_with(bottom);
        let bar = format!("{name}_hscroll");
        assert!(d.has(&bar), "{bottom:?}: a scroll bar");
        assert!(
            d.has("hscroll_gantt"),
            "{bottom:?}: the Gantt keeps its own"
        );
        // The usage view shares the Gantt's timescale: its bar moves both.
        let before = d.app.timeline_origin;
        d.click(&format!("{bar}_right"));
        assert!(d.app.timeline_origin > before, "{bottom:?}: arrow");
        // Ctrl+wheel over the chart zooms the shared timescale at the pointer.
        let body = d.rect(&format!("{name}_body"));
        let pointer = Pos2::new(body.left() + body.width() * 0.5, body.center().y);
        let axis = d.app.usage_axis_for_tests(body.left());
        let date = axis.x_date(pointer.x);
        let scale = d.app.pixels_per_day;
        wheel(&mut d, pointer, Vec2::new(0., 120.), egui::Modifiers::CTRL);
        assert!(d.app.pixels_per_day > scale, "{bottom:?}: Ctrl+wheel zooms");
        let axis = d.app.usage_axis_for_tests(body.left());
        assert!(
            (axis.x(date) - pointer.x).abs() <= 1.1,
            "{bottom:?}: around the pointer"
        );
        // The lowest band's drag zooms, the other band's pans.
        let scale = d.app.pixels_per_day;
        let zoom = d.rect(&format!("{name}_zoom"));
        drag(&mut d, zoom.center(), zoom.center() + Vec2::new(20., 0.));
        assert!(d.app.pixels_per_day > scale, "{bottom:?}: band drag zooms");
        let origin = d.app.timeline_origin;
        let pan = d.rect(&format!("{name}_pan"));
        drag(&mut d, pan.center(), pan.center() + Vec2::new(-40., 0.));
        assert!(d.app.timeline_origin > origin, "{bottom:?}: band drag pans");
        // View > Zoom acts on the shared timescale.
        let scale = d.app.pixels_per_day;
        d.app.zoom_step(true);
        d.frames(2);
        assert!(d.app.pixels_per_day > scale, "{bottom:?}: Zoom In");
        d.assert_clean(name);
    }
}

#[test]
fn zoom_commands_follow_the_gantt_last_used_top_or_bottom() {
    let mut d = activities_with(BottomView::Gantt);
    assert!(
        d.has("bottom_hscroll_gantt"),
        "the bottom Gantt has its bar"
    );
    let top = d.app.pixels_per_day;
    let bottom = d.app.panes.grid.scale.pixels_per_day;
    // A wheel on the bottom Gantt's timescale focuses it.
    let bar = d.rect("bottom_hscroll_gantt").center();
    wheel(&mut d, bar, Vec2::new(-60., 0.), Default::default());
    d.app.zoom_step(true);
    d.frames(3);
    assert_eq!(d.app.pixels_per_day, top, "the top Gantt keeps its zoom");
    assert!(
        d.app.panes.grid.scale.pixels_per_day > bottom,
        "Zoom In acts on the bottom Gantt: {bottom} -> {}",
        d.app.panes.grid.scale.pixels_per_day
    );
    // Back on the top Gantt's bar: the top zooms.
    let bar = d.rect("hscroll_gantt").center();
    wheel(&mut d, bar, Vec2::new(-60., 0.), Default::default());
    let bottom = d.app.panes.grid.scale.pixels_per_day;
    d.app.zoom_step(true);
    d.frames(3);
    assert!(d.app.pixels_per_day > top, "Zoom In acts on the top Gantt");
    assert_eq!(d.app.panes.grid.scale.pixels_per_day, bottom);
    d.assert_clean("bottom gantt focus");
}

/// Renders of each timeline next to the Activities Gantt at the same zoom,
/// to `artifacts/timelines/` for review.
#[test]
fn render_every_timeline_next_to_the_gantt() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/timelines");
    std::fs::create_dir_all(&out).unwrap();
    let path = std::env::temp_dir().join(format!("openxer-timelines-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut project = crate::views::resources::tests::with_resource();
    schedule_model(&mut project.model, &project.resources).unwrap();
    let mut store = Store::open(path.clone()).unwrap();
    store.save(&project).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    let mut time = 0.;
    let mut painter = SoftwarePainter::default();
    let mut shot = |app: &mut App, name: &str| {
        for _ in 0..4 {
            let _ = painter.frame(app, &ctx, &mut time, vec![], false);
        }
        let (image, triangles) = painter.frame(app, &ctx, &mut time, vec![], true).unwrap();
        assert!(triangles > 300, "{name}");
        image.save(out.join(format!("{name}.png"))).unwrap();
        assert!(app.error.is_none(), "{name}: {:?}", app.error);
    };
    // The reference: the Activities Gantt at 100 px a week (Week over Day).
    app.set_bottom(None);
    app.pixels_per_day = 100. / 7.;
    app.update_timescale_units();
    shot(&mut app, "00-activities-gantt-week-day");
    app.pixels_per_day = 28. / 7.;
    app.update_timescale_units();
    shot(&mut app, "01-activities-gantt-month-week");
    // The WBS view at the same two zooms, scrolled by a week.
    app.show_view(MainView::Wbs);
    app.views.wbs.show_details = false;
    shot(&mut app, "02-wbs-default");
    app.views.wbs.zoom = 100.;
    app.views.wbs.timeline_first = Some(app.views.wbs.timeline_default + 7. * DAY as f64);
    shot(&mut app, "03-wbs-week-day-scrolled");
    app.views.wbs.zoom = 13.;
    shot(&mut app, "04-wbs-quarter-month");
    app.show_view(MainView::Projects);
    app.views.projects.show_details = false;
    app.views.projects.zoom = 100.;
    shot(&mut app, "05-projects-week-day");
    app.show_view(MainView::Assignments);
    shot(&mut app, "07-resource-assignments");
    app.pixels_per_day = 100. / 7.;
    app.update_timescale_units();
    app.show_view(MainView::Activities);
    for (bottom, name) in [
        (
            BottomView::ResourceUsageProfile,
            "09-gantt-over-resource-usage-profile",
        ),
        (
            BottomView::ActivityUsageSheet,
            "10-gantt-over-activity-usage-sheet",
        ),
        (BottomView::Gantt, "11-gantt-over-bottom-gantt"),
    ] {
        app.set_bottom(Some(bottom));
        shot(&mut app, name);
    }
    drop(app);
    let _ = std::fs::remove_file(path);
}
