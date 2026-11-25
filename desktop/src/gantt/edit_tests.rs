//! Gantt editing: relationship line hit testing, Edit Relationship, the
//! chart menu, text attachments, bar dragging, Edit > Assign and automatic
//! scheduling. Real frames and clicks on the rendered controls (egui
//! `RawInput` only, no native window or system pointer).
use super::*;
use crate::headless_render::SoftwarePainter;

// ------------------------------------------------------------ hit testing

/// Hit-test points of one relationship drawn from `from` to `to`.
fn route(from: (f32, f32), kind: LinkType, to: (f32, f32), succ_top: f32) -> Vec<Pos2> {
    let from_start = matches!(kind, LinkType::SS | LinkType::SF);
    let to_start = matches!(kind, LinkType::FS | LinkType::SS);
    link_points(&link_path(
        Pos2::new(from.0, from.1),
        from_start,
        Pos2::new(to.0, to.1),
        to_start,
        succ_top,
    ))
}

#[test]
fn line_hit_testing_picks_every_route_and_misses_just_outside() {
    // Rows 18 px apart; bars' middle row 7.5 px below the row top.
    let pred_y = 100. + 7.5;
    for kind in [LinkType::FS, LinkType::SS, LinkType::FF, LinkType::SF] {
        for (label, succ_top, succ_x) in [
            ("cross-row direct", 136., (300., 360.)),
            ("cross-row behind", 136., (120., 150.)),
            ("same row", 100., (300., 360.)),
        ] {
            let succ_y = succ_top + 7.5;
            let to = match kind {
                LinkType::FS | LinkType::SS => succ_x.0,
                _ => succ_x.1,
            };
            let from = match kind {
                LinkType::SS | LinkType::SF => 200.,
                _ => 260.,
            };
            let points = route((from, pred_y), kind, (to, succ_y), succ_top);
            let lines = vec![(7, points.clone())];
            for pair in points.windows(2) {
                let mid = pair[0].lerp(pair[1], 0.5);
                assert_eq!(
                    pick_link(mid, &lines),
                    Some(7),
                    "{kind:?} {label}: a click on the line picks it"
                );
                // Just outside the tolerance, square to the segment.
                let along = (pair[1] - pair[0]).normalized();
                let normal = Vec2::new(-along.y, along.x);
                let outside = mid + normal * (LINK_HIT + 1.);
                let nearest = points
                    .windows(2)
                    .map(|p| segment_distance(outside, p[0], p[1]))
                    .fold(f32::INFINITY, f32::min);
                if nearest > LINK_HIT {
                    assert_eq!(
                        pick_link(outside, &lines),
                        None,
                        "{kind:?} {label}: a click {} px off misses",
                        LINK_HIT + 1.
                    );
                }
                let inside = mid + normal * (LINK_HIT - 0.5);
                assert_eq!(pick_link(inside, &lines), Some(7));
            }
        }
    }
}

#[test]
fn nearest_line_wins_and_a_tie_goes_to_the_topmost() {
    let a = vec![Pos2::new(0., 10.), Pos2::new(100., 10.)];
    let b = vec![Pos2::new(0., 13.), Pos2::new(100., 13.)];
    let lines = vec![(1, a.clone()), (2, b.clone())];
    assert_eq!(pick_link(Pos2::new(50., 11.), &lines), Some(1));
    assert_eq!(pick_link(Pos2::new(50., 12.5), &lines), Some(2));
    // Equally near: the line drawn later (on top) wins.
    assert_eq!(pick_link(Pos2::new(50., 11.5), &lines), Some(2));
    let lines = vec![(2, b), (1, a)];
    assert_eq!(pick_link(Pos2::new(50., 11.5), &lines), Some(1));
    assert_eq!(pick_link(Pos2::new(50., 30.), &lines), None);
}

#[test]
fn drag_dates_snap_to_whole_days_on_the_calendar() {
    let (_, _, app) = setup("dragmath");
    let i = index_of(&app, 1010);
    let a = &app.project.model.activities[i];
    let cal = app.project.model.calendars.get(a.cal);
    let (start, finish) = (a.early_start.unwrap(), a.early_finish.unwrap());
    let rem = a.rem;
    // RB-1010 starts Monday 08:00: two days later is Wednesday 08:00.
    let (s, f, d) = drag_dates(cal, BarPart::Body, (start, finish, rem), 480, 2 * DAY);
    assert_eq!(s, start + 2 * DAY, "two work days later, same time");
    assert_eq!(d, rem);
    assert_eq!(f, cal.unwrap().add(s, rem));
    // A start pushed onto a weekend moves to Monday morning.
    let (s, _, _) = drag_dates(cal, BarPart::Body, (start, finish, rem), 480, 5 * DAY);
    assert_eq!(s, start + 7 * DAY);
    // The finish end: whole days of work, at least one.
    let (_, f, d) = drag_dates(cal, BarPart::Finish, (start, finish, rem), 480, DAY);
    assert_eq!(d, rem + 480);
    assert_eq!(f, cal.unwrap().add(start, d));
    let (_, _, d) = drag_dates(cal, BarPart::Finish, (start, finish, rem), 480, -10 * DAY);
    assert_eq!(d, 480);
    // The start end: three days earlier is the Friday before, a day more of
    // work, the same finish.
    assert_eq!(finish, cal.unwrap().add(start, rem));
    let (s, f, d) = drag_dates(cal, BarPart::Start, (start, finish, rem), 480, -3 * DAY);
    assert_eq!((s, f, d), (start - 3 * DAY, finish, rem + 480));
    // Pushed past the finish: one day of work, still ending at the finish.
    let (s, f, d) = drag_dates(cal, BarPart::Start, (start, finish, rem), 480, 40 * DAY);
    assert_eq!((f, d), (finish, 480));
    assert!(s > start && s < finish);
    // The body keeps the time of day it was dropped at: two hours later is
    // Monday 10:00; the ends take the nearest whole day.
    let (s, _, d) = drag_dates(cal, BarPart::Body, (start, finish, rem), 480, 2 * 60);
    assert_eq!((s, d), (start + 2 * 60, rem));
    let (_, f, _) = drag_dates(cal, BarPart::Finish, (start, finish, rem), 480, 2 * 60);
    assert_eq!(f, finish, "under half a day: the finish end stays");
}

#[test]
fn bar_end_zones_nest_and_short_bars_drop_start_stretch_then_body_then_finish() {
    let y = 107.5;
    let zones = |width: f32, movable: bool, milestone: bool| {
        end_zones(
            Pos2::new(200., y),
            Pos2::new(200. + width, y),
            100.,
            movable,
            milestone,
            (true, true),
        )
    };
    // A long bar: each stretch zone sits inside its relationship zone with
    // a ring of relationship area above, below and on every side.
    let z = zones(100., true, false);
    for (stretch, link, x) in [
        (z.start_stretch.unwrap(), z.start_link, 200.),
        (z.finish_stretch.unwrap(), z.finish_link, 300.),
    ] {
        assert_eq!(stretch.center().x, x);
        assert_eq!(link.center(), Pos2::new(x, y));
        assert_eq!(stretch.width(), STRETCH_WIDTH);
        assert_eq!(link.size(), LINK_ZONE);
        // The bar's own height.
        assert_eq!(
            (stretch.top(), stretch.bottom()),
            (103., 103. + bars::BOX_HEIGHT)
        );
        assert!(link.top() < stretch.top() && link.bottom() > stretch.bottom());
        assert!(link.left() < stretch.left() && link.right() > stretch.right());
    }
    let body = z.body.unwrap();
    assert_eq!((body.left(), body.right()), (208., 292.));
    // Shortening the bar drops the start stretch zone first, then the
    // body, then the finish stretch zone.
    let parts = |w| {
        let z = zones(w, true, false);
        (
            z.start_stretch.is_some(),
            z.body.is_some(),
            z.finish_stretch.is_some(),
        )
    };
    assert_eq!(parts(START_STRETCH_MIN), (true, true, true));
    assert_eq!(parts(START_STRETCH_MIN - 1.), (false, true, true));
    assert_eq!(parts(17.), (false, false, true));
    assert_eq!(parts(FINISH_STRETCH_MIN), (false, false, true));
    assert_eq!(parts(FINISH_STRETCH_MIN - 1.), (false, false, false));
    // No zone swallows another: the relationship zones meet halfway, and a
    // stretch zone never reaches into the other end's relationship zone.
    for w in [10., 11., 12., 16., 18., 24., 40.] {
        let z = zones(w, true, false);
        assert!(z.start_link.right() <= z.finish_link.left(), "{w}");
        if let Some(f) = z.finish_stretch {
            assert!(f.left() >= z.start_link.right(), "{w}");
        }
        if let Some(s) = z.start_stretch {
            assert!(s.right() <= z.finish_link.left(), "{w}");
            assert!(s.right() <= z.body.unwrap().left(), "{w}");
        }
    }
    // A bar that cannot be stretched is relationship area end to end.
    let fixed = zones(100., false, false);
    assert_eq!(
        (fixed.start_stretch, fixed.finish_stretch, fixed.body),
        (None, None, None)
    );
    assert_eq!(fixed.start_link, z.start_link);
    // A milestone: no stretch zones, a 2 px body at its middle.
    let m = zones(10., true, true);
    assert_eq!((m.start_stretch, m.finish_stretch), (None, None));
    assert_eq!(m.body.unwrap().center().x, 205.);
    assert_eq!(m.body.unwrap().width(), 2.);
    // An end pulled in to the chart's edge does not stretch.
    let clipped = end_zones(
        Pos2::new(200., y),
        Pos2::new(300., y),
        100.,
        true,
        false,
        (false, true),
    );
    assert!(clipped.start_stretch.is_none() && clipped.finish_stretch.is_some());
}

// ------------------------------------------------------------ headless UI

fn frame(
    app: &mut App,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    time: &mut f64,
) -> egui::FullOutput {
    *time += 0.1;
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500., 900.))),
            time: Some(*time),
            events,
            focused: true,
            ..Default::default()
        },
        |ctx| app.update_ui(ctx),
    )
}

fn idle(app: &mut App, ctx: &egui::Context, time: &mut f64) {
    let _ = frame(app, ctx, vec![], time);
}

fn pointer(
    app: &mut App,
    ctx: &egui::Context,
    pos: Pos2,
    button: egui::PointerButton,
    pressed: bool,
    time: &mut f64,
) {
    let _ = frame(
        app,
        ctx,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: Default::default(),
            },
        ],
        time,
    );
}

fn press(
    app: &mut App,
    ctx: &egui::Context,
    pos: Pos2,
    button: egui::PointerButton,
    time: &mut f64,
) {
    pointer(app, ctx, pos, button, true, time);
    pointer(app, ctx, pos, button, false, time);
    idle(app, ctx, time);
}

fn key(app: &mut App, ctx: &egui::Context, key: Key, time: &mut f64) {
    for pressed in [true, false] {
        let _ = frame(
            app,
            ctx,
            vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers: Default::default(),
            }],
            time,
        );
    }
}

/// Ctrl+Z or Ctrl+Y.
fn ctrl(app: &mut App, ctx: &egui::Context, key: Key, time: &mut f64) {
    let modifiers = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    for pressed in [true, false] {
        *time += 0.1;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500., 900.))),
                time: Some(*time),
                events: vec![egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed,
                    repeat: false,
                    modifiers,
                }],
                modifiers,
                focused: true,
                ..Default::default()
            },
            |ctx| app.update_ui(ctx),
        );
    }
    idle(app, ctx, time);
}

/// Two clicks at `pos`, after a pause so they are not a triple click.
fn double_click(app: &mut App, ctx: &egui::Context, pos: Pos2, time: &mut f64) {
    for _ in 0..8 {
        idle(app, ctx, time);
    }
    for _ in 0..2 {
        pointer(app, ctx, pos, egui::PointerButton::Primary, true, time);
        pointer(app, ctx, pos, egui::PointerButton::Primary, false, time);
    }
    idle(app, ctx, time);
}

fn type_text(app: &mut App, ctx: &egui::Context, text: &str, time: &mut f64) {
    let _ = frame(app, ctx, vec![egui::Event::Text(text.into())], time);
}

fn rect_of(app: &App, action: &str) -> Rect {
    let [l, t, r, b] = *app
        .action_rects
        .get(action)
        .unwrap_or_else(|| panic!("No rendered control {action:?}"));
    Rect::from_min_max(Pos2::new(l, t), Pos2::new(r, b))
}

fn click(app: &mut App, ctx: &egui::Context, action: &str, time: &mut f64) {
    idle(app, ctx, time);
    let pos = rect_of(app, action).center();
    press(app, ctx, pos, egui::PointerButton::Primary, time);
}

fn hover(app: &mut App, ctx: &egui::Context, action: &str, time: &mut f64) {
    let pos = rect_of(app, action).center();
    for _ in 0..4 {
        let _ = frame(app, ctx, vec![egui::Event::PointerMoved(pos)], time);
    }
}

fn texts(output: &egui::FullOutput) -> Vec<String> {
    output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(shape) => Some(shape.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

fn unique(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "openxer-gantt-edit-{name}-{}-{nanos}.db",
        std::process::id()
    ))
}

fn starter() -> ProjectData {
    let mut project = openxer_xer::starter();
    schedule_model(&mut project.model, &project.resources).unwrap();
    project
}

fn setup(name: &str) -> (PathBuf, egui::Context, App) {
    let path = unique(name);
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let app = App::new(&ctx, Store::open(path.clone()).unwrap(), starter());
    (path, ctx, app)
}

fn start(name: &str) -> (PathBuf, egui::Context, App, f64) {
    let (path, ctx, mut app) = setup(name);
    let mut time = 0.;
    for _ in 0..3 {
        idle(&mut app, &ctx, &mut time);
    }
    (path, ctx, app, time)
}

fn index_of(app: &App, task_id: i64) -> usize {
    app.project
        .model
        .activities
        .iter()
        .position(|a| a.task_id == task_id)
        .unwrap()
}

fn link_index(app: &App, pred: i64, succ: i64) -> usize {
    let (p, s) = (index_of(app, pred), index_of(app, succ));
    app.project
        .model
        .links
        .iter()
        .position(|l| l.pred == p && l.succ == s)
        .unwrap()
}

/// A point on relationship `index`'s drawn line, as far from the other
/// lines as possible.
fn on_line(app: &App, index: usize) -> Pos2 {
    let points = &app
        .link_geometry
        .iter()
        .find(|(i, _)| *i == index)
        .expect("the relationship is drawn")
        .1;
    points
        .windows(2)
        .flat_map(|pair| [0.25, 0.5, 0.75].map(|t| pair[0].lerp(pair[1], t)))
        .max_by(|a, b| {
            let clearance = |pos| {
                app.link_geometry
                    .iter()
                    .filter(|(i, _)| *i != index)
                    .flat_map(|(_, points)| points.windows(2))
                    .map(|pair| segment_distance(pos, pair[0], pair[1]))
                    .fold(f32::INFINITY, f32::min)
            };
            clearance(*a).total_cmp(&clearance(*b))
        })
        .unwrap()
}

#[test]
fn edit_relationship_opens_from_a_line_and_edits_deletes_or_cancels() {
    let (path, ctx, mut app, mut time) = start("relationship");
    let index = link_index(&app, 1020, 1030);
    let original = app.project.model.links.clone();
    // Right-click on the drawn line: as observed in the reference, the line
    // is selected and the chart menu opens; closing the menu (Escape) opens
    // Edit Relationship.
    let pos = on_line(&app, index);
    press(
        &mut app,
        &ctx,
        pos,
        egui::PointerButton::Secondary,
        &mut time,
    );
    assert!(
        app.gantt_ui.menu.is_some(),
        "the chart menu opens on a line"
    );
    assert_eq!(
        app.gantt_ui.picked_link,
        Some(index),
        "the line is selected"
    );
    assert!(app.link_editor.is_none(), "not before the menu closes");
    key(&mut app, &ctx, Key::Escape, &mut time);
    assert!(app.gantt_ui.menu.is_none());
    assert_eq!(app.link_editor.as_ref().map(|e| e.index), Some(index));
    let output = frame(&mut app, &ctx, vec![], &mut time);
    let shown = texts(&output);
    for task_id in [1020, 1030] {
        let i = index_of(&app, task_id);
        let label = format!(
            "{} - {}",
            app.project.model.activities[i].task_code,
            app.project.name(i)
        );
        assert!(shown.contains(&label), "the dialog names {label}");
    }
    let client = crate::dialog_kit::noted_client(&ctx, "edit_relationship").unwrap();
    assert_eq!(client.size(), Vec2::new(456., 146.));

    // Cancel and Escape leave everything as it was.
    let undo = app.history.counts().0;
    click(&mut app, &ctx, "link_type", &mut time);
    click(&mut app, &ctx, "link_type_item_1", &mut time);
    assert_eq!(app.link_editor.as_ref().unwrap().kind, LinkType::SS);
    click(&mut app, &ctx, "link_cancel", &mut time);
    assert!(app.link_editor.is_none());
    assert_eq!(app.project.model.links, original);
    // Double-click opens it too.
    let pos = on_line(&app, index);
    double_click(&mut app, &ctx, pos, &mut time);
    assert_eq!(app.link_editor.as_ref().map(|e| e.index), Some(index));
    click(&mut app, &ctx, "link_type", &mut time);
    click(&mut app, &ctx, "link_type_item_3", &mut time);
    key(&mut app, &ctx, Key::Escape, &mut time);
    assert!(app.link_editor.is_none());
    assert_eq!(app.project.model.links, original);
    assert_eq!(app.history.counts().0, undo, "cancelling adds no undo step");

    // OK: type and lag change as one undo step.
    let pos = on_line(&app, index);
    press(
        &mut app,
        &ctx,
        pos,
        egui::PointerButton::Secondary,
        &mut time,
    );
    key(&mut app, &ctx, Key::Escape, &mut time);
    click(&mut app, &ctx, "link_type", &mut time);
    click(&mut app, &ctx, "link_type_item_2", &mut time);
    click(&mut app, &ctx, "link_lag", &mut time);
    key(&mut app, &ctx, Key::End, &mut time);
    for _ in 0..3 {
        key(&mut app, &ctx, Key::Backspace, &mut time);
    }
    type_text(&mut app, &ctx, "2d", &mut time);
    click(&mut app, &ctx, "link_ok", &mut time);
    assert!(app.link_editor.is_none());
    let link = &app.project.model.links[index];
    assert_eq!((link.link_type, link.lag), (LinkType::FF, 2 * 8 * 60));
    assert_eq!(app.history.counts().0, undo + 1, "one undo step");
    ctrl(&mut app, &ctx, Key::Z, &mut time);
    assert_eq!(app.project.model.links, original);
    ctrl(&mut app, &ctx, Key::Y, &mut time);
    assert_eq!(app.project.model.links[index].link_type, LinkType::FF);

    // A bad lag keeps the dialog open and changes nothing.
    let edited = app.project.model.links.clone();
    let pos = on_line(&app, index);
    press(
        &mut app,
        &ctx,
        pos,
        egui::PointerButton::Secondary,
        &mut time,
    );
    key(&mut app, &ctx, Key::Escape, &mut time);
    app.link_editor.as_mut().unwrap().lag_text = "soon".into();
    click(&mut app, &ctx, "link_ok", &mut time);
    assert!(app.link_editor.is_some());
    assert_eq!(app.project.model.links, edited);

    // Delete removes it, one undo step.
    let undo = app.history.counts().0;
    click(&mut app, &ctx, "link_delete", &mut time);
    click(&mut app, &ctx, "delete_relationship_yes", &mut time);
    assert!(app.link_editor.is_none());
    assert_eq!(app.project.model.links.len(), edited.len() - 1);
    assert_eq!(app.history.counts().0, undo + 1);
    ctrl(&mut app, &ctx, Key::Z, &mut time);
    assert_eq!(app.project.model.links, edited);
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// A point of the chart body on activity `task_id`'s row, at least 10 px
/// from every relationship line.
fn empty_chart_point(app: &App, task_id: i64) -> Pos2 {
    let bar = app
        .bar_geometry
        .iter()
        .find(|b| b.task_id == task_id)
        .unwrap();
    let y = bar.row.center().y;
    let mut x = app.gantt_left + 20.;
    while x < 1480. {
        let pos = Pos2::new(x, y);
        let clear = app
            .link_geometry
            .iter()
            .flat_map(|(_, p)| p.windows(2))
            .all(|p| segment_distance(pos, p[0], p[1]) > 10.);
        if clear {
            return pos;
        }
        x += 7.;
    }
    panic!("no empty chart point on {task_id}'s row");
}

#[test]
fn gantt_menu_has_the_reference_items_and_text_attachments_persist_and_print() {
    let (path, ctx, mut app, mut time) = start("text");
    let pos = empty_chart_point(&app, 1020);
    press(
        &mut app,
        &ctx,
        pos,
        egui::PointerButton::Secondary,
        &mut time,
    );
    assert!(app.gantt_ui.menu.is_some());
    idle(&mut app, &ctx, &mut time);
    let first = rect_of(&app, "gantt_menu_details");
    let last = rect_of(&app, "gantt_menu_attachments");
    assert_eq!(first.height(), 22.);
    for (above, below, gap) in [
        ("gantt_menu_details", "gantt_menu_bars", 6.),
        ("gantt_menu_bars", "gantt_menu_options", 0.),
        ("gantt_menu_options", "gantt_menu_timescale", 0.),
        ("gantt_menu_timescale", "gantt_menu_attachments", 0.),
    ] {
        assert_eq!(
            rect_of(&app, below).top() - rect_of(&app, above).bottom(),
            gap,
            "{above} to {below}"
        );
    }
    assert_eq!(last.bottom() + 3. - (first.top() - 3.), 122., "menu height");
    // To the pixel only with Arial itself; Liberation Sans draws the
    // widest item 2 px wider.
    if crate::chrome::arial_is_real() {
        assert_eq!(first.width() + 6., 161., "menu width");
    } else {
        assert!((first.width() + 6. - 161.).abs() <= 3., "menu width");
    }
    // Attachments > Text waits for a click on the chart, which places the
    // text on the clicked row.
    hover(&mut app, &ctx, "gantt_menu_attachments", &mut time);
    click(&mut app, &ctx, "gantt_menu_text", &mut time);
    assert!(app.gantt_ui.menu.is_none());
    assert!(app.gantt_ui.placing_text.is_some() && app.gantt_ui.text.is_none());
    idle(&mut app, &ctx, &mut time);
    press(&mut app, &ctx, pos, egui::PointerButton::Primary, &mut time);
    assert!(app.gantt_ui.placing_text.is_none());
    let draft = app
        .gantt_ui
        .text
        .as_ref()
        .expect("The click opens the text dialog");
    assert_eq!(draft.task_id, 1020);
    assert_eq!(draft.band, None);
    // Cancel adds nothing.
    click(&mut app, &ctx, "text_attachment_cancel", &mut time);
    assert!(app.gantt_ui.text.is_none() && app.gantt.texts.is_empty());
    press(
        &mut app,
        &ctx,
        pos,
        egui::PointerButton::Secondary,
        &mut time,
    );
    hover(&mut app, &ctx, "gantt_menu_attachments", &mut time);
    click(&mut app, &ctx, "gantt_menu_text", &mut time);
    idle(&mut app, &ctx, &mut time);
    press(&mut app, &ctx, pos, egui::PointerButton::Primary, &mut time);
    click(&mut app, &ctx, "text_attachment_text", &mut time);
    type_text(&mut app, &ctx, "Crane on site", &mut time);
    click(&mut app, &ctx, "text_attachment_ok", &mut time);
    assert!(app.gantt_ui.text.is_none());
    assert_eq!(app.gantt.texts.len(), 1);
    assert_eq!(app.gantt.texts[0].text, "Crane on site");
    let output = frame(&mut app, &ctx, vec![], &mut time);
    assert!(
        texts(&output).contains(&"Crane on site".to_string()),
        "drawn on the chart"
    );
    let box_rect = rect_of(&app, "text_attachment_0");
    let row = app
        .bar_geometry
        .iter()
        .find(|b| b.task_id == 1020)
        .unwrap()
        .row;
    assert_eq!(box_rect.top(), row.top() + 1., "anchored to the row");

    // Double-click edits it; Delete in the dialog removes it.
    double_click(&mut app, &ctx, box_rect.center(), &mut time);
    assert_eq!(app.gantt_ui.text.as_ref().and_then(|d| d.index), Some(0));
    click(&mut app, &ctx, "text_attachment_cancel", &mut time);
    assert_eq!(app.gantt.texts.len(), 1);

    // Kept with the layout: saved, cleared, reopened.
    app.save_layout_as("H2 text").unwrap();
    let saved = app.gantt.texts.clone();
    let mut cleared = app.gantt.clone();
    cleared.texts.clear();
    app.apply_gantt(cleared).unwrap();
    assert!(app.gantt.texts.is_empty());
    app.open_layout("H2 text", true).unwrap();
    assert_eq!(app.gantt.texts, saved);
    // And in the workspace: a new session shows it.
    drop(app);
    let mut app = App::new(&ctx, Store::open(path.clone()).unwrap(), starter());
    assert_eq!(app.gantt.texts, saved);
    // Printed: a text item on the page and in the PDF.
    let printed = app.print_with(&crate::print::PageSetup::default()).unwrap();
    assert!(printed.pages[0].texts().contains(&"Crane on site"));
    let pdf = crate::print::pdf::write(&printed.pages, "Test");
    let content: Vec<u8> = pdf_streams(&pdf).concat();
    assert!(
        content.windows(17).any(|w| w == b"(Crane on site) T"),
        "the text is in the PDF"
    );
    // The dialog's Delete.
    for _ in 0..3 {
        idle(&mut app, &ctx, &mut time);
    }
    app.edit_text_attachment(0);
    click(&mut app, &ctx, "text_attachment_delete", &mut time);
    assert!(app.gantt.texts.is_empty());
    drop(app);
    let _ = std::fs::remove_file(path);
}

fn pdf_streams(pdf: &[u8]) -> Vec<Vec<u8>> {
    let find = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).position(|w| w == needle);
    let mut out = vec![];
    let mut rest = pdf;
    while let Some(start) = find(rest, b"stream\n") {
        let body = &rest[start + 7..];
        let end = find(body, b"\nendstream").expect("stream ends");
        let data = &body[..end];
        out.push(
            miniz_oxide::inflate::decompress_to_vec_zlib(data).unwrap_or_else(|_| data.to_vec()),
        );
        rest = &body[end + 10..];
    }
    out
}

/// Drags from `from` by `dx` pixels in small steps, checking the live hint
/// on the way, then releases.
fn drag_by(app: &mut App, ctx: &egui::Context, from: Pos2, dx: f32, time: &mut f64) -> String {
    pointer(app, ctx, from, egui::PointerButton::Primary, true, time);
    let mut hint = String::new();
    for step in 1..=6 {
        let pos = from + Vec2::new(dx * step as f32 / 6., 0.);
        let _ = frame(app, ctx, vec![egui::Event::PointerMoved(pos)], time);
        hint = app.status.clone();
    }
    pointer(
        app,
        ctx,
        from + Vec2::new(dx, 0.),
        egui::PointerButton::Primary,
        false,
        time,
    );
    idle(app, ctx, time);
    hint
}

#[test]
fn bar_drags_move_or_stretch_not_started_bars_only() {
    let (path, ctx, mut app, mut time) = start("bardrag");
    let i = index_of(&app, 1020);
    let before = app.project.model.activities[i].clone();
    let constraint = (before.cstr, before.cstr_date);
    let origin = app.project.model.origin_serial;
    let ppd = app.pixels_per_day;
    let undo = app.history.counts().0;
    // The body: two days later.
    let body = rect_of(&app, "bar_body_1020");
    let hint = drag_by(&mut app, &ctx, body.center(), 2. * ppd + 1., &mut time);
    // The drop asks, as a typed Start does; Yes holds it.
    assert!(app.sched.start_hold.is_some(), "the drop asks first");
    assert_eq!(app.project.model.activities[i], before, "nothing yet");
    click(&mut app, &ctx, "start_hold_yes", &mut time);
    let a = app.project.model.activities[i].clone();
    // Thursday plus two days is Saturday: the bar starts Monday morning.
    let cal = app.project.model.calendars.get(a.cal).unwrap();
    let start = cal.snap_start(before.early_start.unwrap() + 2 * DAY);
    assert_eq!(start, before.early_start.unwrap() + 4 * DAY);
    assert_eq!(a.early_start, Some(start));
    assert!(
        hint.contains(&format!("Start: {}", format::date(origin, Some(start)))),
        "the hint showed the new start: {hint}"
    );
    assert!(
        hint.contains(&format::date(origin, a.early_finish)),
        "{hint}"
    );
    assert_eq!(a.target_start, Some(start), "the planned start follows");
    assert_eq!(a.rem, before.rem, "the duration stays");
    // As a start typed into the Start cell: a Start On or After constraint
    // holds it there (it had none).
    assert_eq!(constraint.0, CstrType::None);
    assert_eq!((a.cstr, a.cstr_date), (CstrType::StartOnAfter, Some(start)));
    assert_eq!(app.history.counts().0, undo + 1, "one undo step");
    // The finish end: three more days of duration.
    let finish = rect_of(&app, "bar_finish_1020");
    let hint = drag_by(&mut app, &ctx, finish.center(), 3. * ppd, &mut time);
    let a = app.project.model.activities[i].clone();
    let periods = format::activity_periods(&app.project, i);
    assert_eq!(a.rem, before.rem + 3 * periods.day);
    let original = app.project.details[&a.task_id].original_duration;
    assert_eq!(original, a.rem, "original and remaining stay equal");
    assert!(
        hint.contains(&format!(
            "Original Duration: {}",
            format::duration(a.rem, periods)
        )),
        "{hint}"
    );
    assert_eq!(app.history.counts().0, undo + 2);
    ctrl(&mut app, &ctx, Key::Z, &mut time);
    ctrl(&mut app, &ctx, Key::Z, &mut time);
    assert_eq!(app.project.model.activities[i], before);

    // The move survives the next schedule run.
    let body = rect_of(&app, "bar_body_1020");
    let _ = drag_by(&mut app, &ctx, body.center(), 2. * ppd + 1., &mut time);
    click(&mut app, &ctx, "start_hold_yes", &mut time);
    app.schedule();
    idle(&mut app, &ctx, &mut time);
    assert_eq!(app.project.model.activities[i].early_start, Some(start));

    // A started bar does not move.
    app.edit_activity(i, details::setters::Edit::Status(StatusType::InProgress))
        .unwrap();
    for _ in 0..2 {
        idle(&mut app, &ctx, &mut time);
    }
    assert!(!app.action_rects.contains_key("bar_body_1020"));
    let started = app.project.model.activities[i].clone();
    let bar = app.bar_geometry.iter().find(|b| b.task_id == 1020).unwrap();
    let middle = Pos2::new((bar.start.x + bar.finish.x) / 2., bar.start.y);
    let _ = drag_by(&mut app, &ctx, middle, 3. * ppd, &mut time);
    assert_eq!(app.project.model.activities[i], started);
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// What a press at `pos` and a short drag start: the bar drag part, the
/// relationship drag edge, and the cursor shown on hover there. Cancelled
/// with Escape before release, so nothing changes.
fn grab(
    app: &mut App,
    ctx: &egui::Context,
    pos: Pos2,
    time: &mut f64,
) -> (Option<BarPart>, Option<Edge>, egui::CursorIcon) {
    let mut cursor = egui::CursorIcon::Default;
    for _ in 0..3 {
        cursor = frame(app, ctx, vec![egui::Event::PointerMoved(pos)], time)
            .platform_output
            .cursor_icon;
    }
    pointer(app, ctx, pos, egui::PointerButton::Primary, true, time);
    for dx in [3., 6.] {
        let _ = frame(
            app,
            ctx,
            vec![egui::Event::PointerMoved(pos + Vec2::new(dx, 0.))],
            time,
        );
    }
    let state = (
        app.gantt_ui.bar_drag.map(|d| d.part),
        app.link_drag.map(|d| d.edge),
    );
    key(app, ctx, Key::Escape, time);
    pointer(
        app,
        ctx,
        pos + Vec2::new(6., 0.),
        egui::PointerButton::Primary,
        false,
        time,
    );
    idle(app, ctx, time);
    (state.0, state.1, cursor)
}

#[test]
fn bar_ends_stretch_at_the_middle_and_link_in_the_ring_around_it() {
    let (path, ctx, mut app, mut time) = start("bullseye");
    let i = index_of(&app, 1020);
    let before = app.project.model.activities[i].clone();
    let bar = *app.bar_geometry.iter().find(|b| b.task_id == 1020).unwrap();
    assert!(
        bar.finish.x - bar.start.x >= START_STRETCH_MIN,
        "the bar is long enough to stretch at both ends"
    );
    // In the ring the system cursor is hidden for the painted link pointer.
    use egui::CursorIcon::{None as LinkPointer, ResizeHorizontal};
    for (edge, part, at, outward) in [
        (Edge::Start, BarPart::Start, bar.start, -6.),
        (Edge::Finish, BarPart::Finish, bar.finish, 6.),
    ] {
        // Exactly on the bar end: stretch.
        assert_eq!(
            grab(&mut app, &ctx, at, &mut time),
            (Some(part), None, ResizeHorizontal),
            "{edge:?} end"
        );
        // Above the bar, and just outside its end: a relationship.
        for ring in [bar.link_grip(edge), at + Vec2::new(outward, 0.)] {
            assert_eq!(
                grab(&mut app, &ctx, ring, &mut time),
                (None, Some(edge), LinkPointer),
                "{edge:?} ring at {ring:?}"
            );
        }
    }
    assert_eq!(app.project.model.activities[i], before, "all cancelled");

    // A started bar's ends are relationship area only. Started in the model
    // directly, so no out-of-sequence prompt is open over the chart.
    let a = &mut app.project.model.activities[i];
    a.status = StatusType::InProgress;
    a.act_start = a.early_start;
    for _ in 0..2 {
        idle(&mut app, &ctx, &mut time);
    }
    assert!(!app.action_rects.contains_key("bar_start_1020"));
    assert!(!app.action_rects.contains_key("bar_finish_1020"));
    let bar = *app.bar_geometry.iter().find(|b| b.task_id == 1020).unwrap();
    for (edge, at) in [(Edge::Start, bar.start), (Edge::Finish, bar.finish)] {
        assert_eq!(
            grab(&mut app, &ctx, at, &mut time),
            (None, Some(edge), LinkPointer),
            "started bar, {edge:?} end"
        );
    }
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// Whether the rendered frame shows the Relationship Lines icon as the
/// pointer at `pos`: every opaque pixel of the authored 16 px drawing at
/// `POINTER_ICON_OFFSET` below and right of the pointer, and the black hot
/// spot cross exactly on it.
fn link_pointer_painted(
    painter: &mut SoftwarePainter,
    ctx: &egui::Context,
    output: &egui::FullOutput,
    pos: Pos2,
) -> bool {
    let (image, _) = painter.rasterize(
        ctx,
        output.shapes.clone(),
        output.pixels_per_point,
        Vec2::new(1500., 900.),
    );
    let icon = image::load_from_memory(include_bytes!("../../../assets/icons/16/SHOWRELBTN16.png"))
        .unwrap()
        .into_rgba8();
    let hot = (pos.x.round() as u32, pos.y.round() as u32);
    let near = |a: &image::Rgba<u8>, b: &image::Rgba<u8>| (0..3).all(|c| a[c].abs_diff(b[c]) <= 8);
    let black = image::Rgba([0, 0, 0, 255]);
    let origin = (
        hot.0 + chrome::POINTER_ICON_OFFSET.x as u32,
        hot.1 + chrome::POINTER_ICON_OFFSET.y as u32,
    );
    let mut opaque = 0;
    let icon_ok = icon.enumerate_pixels().all(|(x, y, px)| {
        if px[3] < 255 {
            return true;
        }
        opaque += 1;
        near(image.get_pixel(origin.0 + x, origin.1 + y), px)
    });
    icon_ok && opaque > 10 && near(image.get_pixel(hot.0, hot.1), &black)
}

#[test]
fn the_link_ring_and_a_link_drag_show_the_relationship_lines_icon_as_the_pointer() {
    let (path, ctx, mut app) = setup("linkpointer");
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    let mut run = |app: &mut App, events: Vec<egui::Event>, painter: &mut SoftwarePainter| {
        let output = frame(app, &ctx, events, &mut time);
        painter.ingest(&output);
        output
    };
    for _ in 0..3 {
        let _ = run(&mut app, vec![], &mut painter);
    }
    let bar = *app.bar_geometry.iter().find(|b| b.task_id == 1020).unwrap();
    let ring = bar.link_grip(Edge::Finish).round();
    let mut output = run(
        &mut app,
        vec![egui::Event::PointerMoved(ring)],
        &mut painter,
    );
    for _ in 0..2 {
        output = run(&mut app, vec![], &mut painter);
    }
    assert_eq!(output.platform_output.cursor_icon, egui::CursorIcon::None);
    assert!(
        link_pointer_painted(&mut painter, &ctx, &output, ring),
        "hovering the ring paints the Relationship Lines icon at the pointer"
    );

    // While a relationship is dragged, away from any bar end.
    let away = ring + Vec2::new(40., 30.);
    let _ = run(
        &mut app,
        vec![egui::Event::PointerButton {
            pos: ring,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }],
        &mut painter,
    );
    for dx in [3., 6.] {
        let _ = run(
            &mut app,
            vec![egui::Event::PointerMoved(ring + Vec2::new(dx, 0.))],
            &mut painter,
        );
    }
    let output = run(
        &mut app,
        vec![egui::Event::PointerMoved(away)],
        &mut painter,
    );
    assert!(app.link_drag.is_some(), "a relationship drag is under way");
    assert_eq!(output.platform_output.cursor_icon, egui::CursorIcon::None);
    assert!(
        link_pointer_painted(&mut painter, &ctx, &output, away),
        "the dragging pointer is the Relationship Lines icon"
    );
    let _ = run(
        &mut app,
        vec![egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
        &mut painter,
    );
    let _ = run(
        &mut app,
        vec![egui::Event::PointerButton {
            pos: away,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
        &mut painter,
    );
    assert!(app.link_drag.is_none());

    // The stretch spot keeps the resize cursor and no painted pointer.
    let mut output = run(
        &mut app,
        vec![egui::Event::PointerMoved(bar.finish)],
        &mut painter,
    );
    for _ in 0..2 {
        output = run(&mut app, vec![], &mut painter);
    }
    assert_eq!(
        output.platform_output.cursor_icon,
        egui::CursorIcon::ResizeHorizontal
    );
    assert!(!link_pointer_painted(
        &mut painter,
        &ctx,
        &output,
        bar.finish.round()
    ));

    // Back in the ring, then off the window: the hidden cursor does not stick.
    for _ in 0..2 {
        let _ = run(
            &mut app,
            vec![egui::Event::PointerMoved(ring)],
            &mut painter,
        );
    }
    let _ = run(&mut app, vec![egui::Event::PointerGone], &mut painter);
    let output = run(&mut app, vec![], &mut painter);
    assert_ne!(
        output.platform_output.cursor_icon,
        egui::CursorIcon::None,
        "the system cursor returns off the window"
    );
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn dragging_the_start_end_moves_the_start_and_keeps_the_finish_in_one_undo_step() {
    let (path, ctx, mut app, mut time) = start("startstretch");
    let i = index_of(&app, 1020);
    let before = app.project.model.activities[i].clone();
    let origin = app.project.model.origin_serial;
    let ppd = app.pixels_per_day;
    let undo = app.history.counts().0;
    let periods = format::activity_periods(&app.project, i);
    let (start, finish) = (before.early_start.unwrap(), before.early_finish.unwrap());
    let new_start = app
        .project
        .model
        .calendars
        .get(before.cal)
        .unwrap()
        .snap_start(start - 7 * DAY);
    // One week earlier: five more work days, the same finish.
    let grip = rect_of(&app, "bar_start_1020");
    let hint = drag_by(&mut app, &ctx, grip.center(), -7. * ppd, &mut time);
    let a = app.project.model.activities[i].clone();
    assert_eq!(a.early_start, Some(new_start));
    assert_eq!(a.early_finish, Some(finish), "the finish stays");
    assert_eq!(a.rem, before.rem + 5 * periods.day, "the duration grows");
    let original = app.project.details[&a.task_id].original_duration;
    assert_eq!(original, a.rem, "original and remaining stay equal");
    assert_eq!(a.target_start, Some(new_start), "the planned start follows");
    // Held at its new start, as a moved bar is.
    assert_eq!(
        (a.cstr, a.cstr_date),
        (CstrType::StartOnAfter, Some(new_start))
    );
    assert!(
        hint.contains(&format!("Start: {}", format::date(origin, Some(new_start))))
            && hint.contains(&format!(
                "Original Duration: {}",
                format::duration(a.rem, periods)
            )),
        "{hint}"
    );
    assert!(app.status.contains("Start On or After"), "{}", app.status);
    assert_eq!(app.history.counts().0, undo + 1, "one undo step");
    ctrl(&mut app, &ctx, Key::Z, &mut time);
    assert_eq!(
        app.project.model.activities[i], before,
        "one undo puts it back"
    );
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn edit_assign_opens_the_assign_pickers() {
    let (path, ctx, mut app, mut time) = start("assign");
    app.selected = Some(index_of(&app, 1020));
    for (tab, predecessor) in [
        (DetailTab::Predecessors, Some(true)),
        (DetailTab::Successors, Some(false)),
    ] {
        // What Edit > Assign > Predecessors... and Successors... queue.
        app.editing.pending = Some(editing::Command::Assign(tab));
        idle(&mut app, &ctx, &mut time);
        assert_eq!(app.detail.tab, tab);
        match &app.detail.picker {
            Some(details::Picker::Relationship { predecessor: p, .. }) => {
                assert_eq!(Some(*p), predecessor)
            }
            _ => panic!("Assign {tab:?} opens the relationship picker"),
        }
        idle(&mut app, &ctx, &mut time);
        click(&mut app, &ctx, "picker_close", &mut time);
        assert!(app.detail.picker.is_none());
    }
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn automatic_scheduling_after_an_edit_matches_f9_and_is_off_by_default() {
    let (path, ctx, mut auto, mut time) = start("auto");
    assert!(!auto.sched.auto_schedule, "off by default");
    // Turned on in Schedule Options.
    auto.open_options();
    for _ in 0..2 {
        idle(&mut auto, &ctx, &mut time);
    }
    click(&mut auto, &ctx, "options_auto_schedule", &mut time);
    assert!(!auto.sched.auto_schedule, "a draft until the dialog closes");
    use crate::qa_workflows::schedule_options_close::{close_schedule_options, Frames};
    close_schedule_options(&mut Frames(&mut auto, &ctx, &mut time), Some("Yes"));
    assert!(auto.sched.auto_schedule);
    let (path2, ctx2, mut manual, mut time2) = start("manual");
    let i = index_of(&auto, 1020);
    let undo = auto.history.counts().0;
    let runs = auto.schedule_runs;
    for (app, ctx, time) in [
        (&mut auto, &ctx, &mut time),
        (&mut manual, &ctx2, &mut time2),
    ] {
        app.edit_activity(i, details::setters::Edit::OriginalDuration(10 * 8 * 60))
            .unwrap();
        idle(app, ctx, time);
    }
    assert_eq!(auto.schedule_runs, runs + 1, "scheduled once");
    assert_eq!(
        auto.history.counts().0,
        undo + 1,
        "the edit and its dates are one step"
    );
    let moved = auto.project.model.activities[index_of(&auto, 1030)].early_start;
    assert_ne!(
        moved,
        manual.project.model.activities[index_of(&manual, 1030)].early_start,
        "without automatic scheduling the successor waits for F9"
    );
    manual.schedule();
    let dates = |app: &App| -> Vec<_> {
        app.project
            .model
            .activities
            .iter()
            .map(|a| {
                (
                    a.task_id,
                    a.early_start,
                    a.early_finish,
                    a.late_start,
                    a.late_finish,
                    a.total_float,
                )
            })
            .collect()
    };
    assert_eq!(
        dates(&auto),
        dates(&manual),
        "same dates as editing and pressing F9"
    );
    // Undo does not schedule again; nothing else does either.
    ctrl(&mut auto, &ctx, Key::Z, &mut time);
    let runs = auto.schedule_runs;
    for _ in 0..3 {
        idle(&mut auto, &ctx, &mut time);
    }
    assert_eq!(auto.schedule_runs, runs);
    // The choice is kept for the project.
    drop(auto);
    let mut again = App::new(&ctx, Store::open(path.clone()).unwrap(), starter());
    again.refresh_auto_schedule();
    assert!(again.sched.auto_schedule);
    drop(again);
    drop(manual);
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path2);
}

/// Review render of the Gantt context menu. Writes `artifacts/review/chrome/openxer-gantt-menu.png`
/// (the whole window) and `openxer-gantt-menu.json` (the menu window).
#[test]
fn render_gantt_context_menu_for_review() {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/review/chrome");
    std::fs::create_dir_all(&output).unwrap();
    let (path, ctx, mut app) = setup("menu-render");
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let pos = empty_chart_point(&app, 1020);
    for pressed in [true, false] {
        let events = vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: Default::default(),
            },
        ];
        let _ = painter.frame(&mut app, &ctx, &mut time, events, false);
    }
    let _ = painter.frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::PointerGone],
        false,
    );
    let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    let first = rect_of(&app, "gantt_menu_details");
    let last = rect_of(&app, "gantt_menu_attachments");
    let origin = first.min - Vec2::splat(3.);
    let size = Vec2::new(first.width() + 6., last.bottom() + 3. - origin.y);
    image.save(output.join("openxer-gantt-menu.png")).unwrap();
    std::fs::write(
        output.join("openxer-gantt-menu.json"),
        format!(
            "{{\"x\": {}, \"y\": {}, \"w\": {}, \"h\": {}}}",
            origin.x, origin.y, size.x, size.y
        ),
    )
    .unwrap();
    if crate::chrome::arial_is_real() {
        assert_eq!(size, Vec2::new(161., 122.), "the menu window is 161 x 122");
    } else {
        assert_eq!(size.y, 122., "the menu window is 122 high");
        assert!(
            (size.x - 161.).abs() <= 3.,
            "the menu window is about 161 wide"
        );
    }
    // Edit Relationship and the Text Attachment dialog, for review.
    app.gantt_ui.menu = None;
    app.edit_link(link_index(&app, 1020, 1030));
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image
        .save(output.join("openxer-edit-relationship.png"))
        .unwrap();
    // A text attachment on the chart, then its dialog.
    app.link_editor = None;
    let dd = app.project.model.project.data_date.unwrap();
    app.gantt.texts.push(super::settings::TextAttachment {
        text: "Crane on site\nweek 46".into(),
        date: dd + 9 * DAY,
        task_id: 1030,
        band: None,
        font: Default::default(),
    });
    for (name, open) in [
        ("openxer-text-attachment.png", false),
        ("openxer-text-dialog.png", true),
    ] {
        if open {
            app.edit_text_attachment(0);
        }
        for _ in 0..2 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        let (image, _) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        image.save(output.join(name)).unwrap();
    }
    drop(app);
    let _ = std::fs::remove_file(path);
}
