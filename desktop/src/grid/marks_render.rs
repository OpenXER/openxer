//! Software render of the constraint marker to the ignored
//! `artifacts/constraint-mark/`: the activity table with marked Start and
//! Finish cells, the hover text of a marked cell and of the Start header,
//! the same text in copied cells, on Gantt bar labels and in Activity
//! Network boxes.
use super::marks::{self, End};
use crate::format::{self, DateMark};
use crate::gantt::settings::{BarLabel, LabelField, LabelPosition};
use crate::headless_render::SoftwarePainter;
use crate::store::Store;
use crate::App;
use eframe::egui;
use openxer_engine::model::CstrType;
use std::path::PathBuf;

/// Moves the pointer to `pos` and runs frames until a text shape matching
/// `want` paints (the hover delay), returning that text and the frame's
/// render; `None` when it never shows.
fn hover_until(
    app: &mut App,
    painter: &mut SoftwarePainter,
    ctx: &egui::Context,
    time: &mut f64,
    pos: egui::Pos2,
    want: impl Fn(&str) -> bool,
) -> Option<(String, image::RgbaImage)> {
    for n in 0..16 {
        *time += 0.1;
        let events = if n == 0 {
            vec![egui::Event::PointerMoved(pos)]
        } else {
            vec![]
        };
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1500., 900.),
                )),
                time: Some(*time),
                focused: true,
                events,
                ..Default::default()
            },
            |ctx| app.update_ui(ctx),
        );
        painter.ingest(&output);
        let found = output.shapes.iter().find_map(|c| match &c.shape {
            egui::Shape::Text(t) if want(t.galley.text()) => Some(t.galley.text().to_string()),
            _ => None,
        });
        if let Some(text) = found {
            // Let the tooltip finish fading in before the render.
            let mut output = output;
            for _ in 0..4 {
                *time += 0.1;
                output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1500., 900.),
                        )),
                        time: Some(*time),
                        focused: true,
                        ..Default::default()
                    },
                    |ctx| app.update_ui(ctx),
                );
                painter.ingest(&output);
            }
            let (image, _) = painter.rasterize(
                ctx,
                output.shapes,
                output.pixels_per_point,
                egui::vec2(1500., 900.),
            );
            return Some((text, image));
        }
    }
    None
}

/// One painted frame: every text shape with its clipped rect, and the
/// render.
fn paint(
    app: &mut App,
    painter: &mut SoftwarePainter,
    ctx: &egui::Context,
    time: &mut f64,
) -> (Vec<(String, egui::Rect)>, image::RgbaImage) {
    for _ in 0..3 {
        let _ = painter.frame(app, ctx, time, vec![egui::Event::PointerGone], false);
    }
    *time += 0.1;
    let screen = egui::vec2(1500., 900.);
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen)),
            time: Some(*time),
            focused: true,
            ..Default::default()
        },
        |ctx| app.update_ui(ctx),
    );
    painter.ingest(&output);
    let texts = output
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => {
                let r = t.visual_bounding_rect().intersect(c.clip_rect);
                r.is_positive().then(|| (t.galley.text().to_string(), r))
            }
            _ => None,
        })
        .collect();
    let (image, _) = painter.rasterize(ctx, output.shapes, output.pixels_per_point, screen);
    (texts, image)
}

/// Where the last painted copy of `text` sits.
fn find_text(
    app: &mut App,
    painter: &mut SoftwarePainter,
    ctx: &egui::Context,
    time: &mut f64,
    text: &str,
) -> Option<egui::Pos2> {
    let _ = painter.frame(app, ctx, time, vec![egui::Event::PointerGone], false);
    *time += 0.1;
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1500., 900.),
            )),
            time: Some(*time),
            focused: true,
            ..Default::default()
        },
        |ctx| app.update_ui(ctx),
    );
    painter.ingest(&output);
    output.shapes.iter().rev().find_map(|c| match &c.shape {
        egui::Shape::Text(t) if t.galley.text() == text => {
            let r = t.visual_bounding_rect().intersect(c.clip_rect);
            r.is_positive().then_some(r.center())
        }
        _ => None,
    })
}

#[test]
fn render_constraint_marks() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/constraint-mark");
    std::fs::create_dir_all(&out).unwrap();
    let path =
        std::env::temp_dir().join(format!("openxer-constraint-mark-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut project = openxer_xer::starter();
    crate::scheduling::schedule_model(&mut project.model, &project.resources).unwrap();
    // Three activities not started: a Start On or After; a Start On or
    // After with a Finish On or Before secondary; a Finish On or After.
    let open: Vec<usize> = (0..project.model.activities.len())
        .filter(|&i| {
            let a = &project.model.activities[i];
            a.act_start.is_none() && a.early_start.is_some() && a.early_finish.is_some()
        })
        .take(3)
        .collect();
    assert_eq!(open.len(), 3, "the starter has open activities");
    {
        let acts = &mut project.model.activities;
        let a = &mut acts[open[0]];
        a.cstr = CstrType::StartOnAfter;
        a.cstr_date = a.early_start;
        let a = &mut acts[open[1]];
        a.cstr = CstrType::StartOnAfter;
        a.cstr_date = a.early_start;
        a.cstr2 = CstrType::FinishOnBefore;
        a.cstr_date2 = a.early_finish;
        let a = &mut acts[open[2]];
        a.cstr = CstrType::FinishOnAfter;
        a.cstr_date = a.early_finish;
    }
    let mut store = Store::open(path.clone()).unwrap();
    store.save(&project).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    app.selected = None;
    let mut time = 0.;
    let mut painter = SoftwarePainter::default();
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("table.png")).unwrap();

    let origin = app.project.model.origin_serial;
    let shown = |app: &App, i: usize, end: End| {
        let a = &app.project.model.activities[i];
        let (minute, mark) = match end {
            End::Start => (crate::util::displayed_start(a, false), marks::mark(a, end)),
            End::Finish => (crate::util::displayed_finish(a, false), marks::mark(a, end)),
        };
        format::date_marked(origin, minute, mark)
    };
    let start0 = shown(&app, open[0], End::Start);
    assert!(start0.ends_with(" *"), "{start0}");
    assert!(!shown(&app, open[0], End::Finish).ends_with('*'));
    assert!(shown(&app, open[1], End::Finish).ends_with(" *"));
    assert!(shown(&app, open[2], End::Finish).ends_with(" *"));
    assert_eq!(
        marks::mark(&app.project.model.activities[open[2]], End::Start),
        DateMark::Plain
    );

    // The marked Start cell on the table names its constraints on hover.
    let start1 = shown(&app, open[1], End::Start);
    let pos = find_text(&mut app, &mut painter, &ctx, &mut time, &start1)
        .unwrap_or_else(|| panic!("{start1:?} is painted on the table"));
    let want =
        marks::tooltip(origin, &app.project.model.activities[open[1]], End::Start).expect("marked");
    assert!(want.starts_with("Constraint: Start On or After "), "{want}");
    let (_, image) = hover_until(&mut app, &mut painter, &ctx, &mut time, pos, |t| t == want)
        .unwrap_or_else(|| panic!("tooltip {want:?} shows on hover"));
    image.save(out.join("tooltip-start.png")).unwrap();
    // The secondary on the same activity's Finish.
    let finish1 = shown(&app, open[1], End::Finish);
    let pos = find_text(&mut app, &mut painter, &ctx, &mut time, &finish1)
        .unwrap_or_else(|| panic!("{finish1:?} is painted on the table"));
    let (_, image) = hover_until(&mut app, &mut painter, &ctx, &mut time, pos, |t| {
        t.starts_with("Secondary constraint: Finish On or Before ")
    })
    .expect("the secondary constraint shows on the Finish cell");
    image.save(out.join("tooltip-finish.png")).unwrap();

    // The Start header explains the markers.
    let pos = find_text(&mut app, &mut painter, &ctx, &mut time, "Start")
        .expect("the Start header is painted");
    let (_, image) = hover_until(&mut app, &mut painter, &ctx, &mut time, pos, |t| {
        t.ends_with(marks::HEADER_HINT)
    })
    .expect("the Start header explains the markers");
    image.save(out.join("tooltip-header.png")).unwrap();

    // Copied cells carry the same text as the table.
    let copied = app.selection_text(&[open[0]]);
    assert!(copied.contains(&start0), "{copied}");

    // Gantt bar labels: Start left of the bar, Finish right of it.
    let index = app
        .gantt
        .bars
        .iter()
        .position(|b| b.name == "Current Bar Labels")
        .unwrap();
    app.gantt.bars[index].labels = vec![
        BarLabel {
            position: LabelPosition::Left,
            field: LabelField::Start,
        },
        BarLabel {
            position: LabelPosition::Right,
            field: LabelField::Finish,
        },
    ];
    app.drop_frame_cache();
    let (texts, image) = paint(&mut app, &mut painter, &ctx, &mut time);
    image.save(out.join("gantt-labels.png")).unwrap();
    let left = app.gantt_left;
    let on_chart = |t: &str| texts.iter().any(|(s, r)| s == t && r.left() >= left);
    let chart: Vec<&(String, egui::Rect)> =
        texts.iter().filter(|(_, r)| r.left() >= left).collect();
    // open[1]: both ends constrained; open[2]: only its Finish. (open[0]
    // may be a milestone, whose bar labels show the name only.)
    for (i, end, marked) in [
        (open[1], End::Start, true),
        (open[1], End::Finish, true),
        (open[2], End::Start, false),
        (open[2], End::Finish, true),
    ] {
        let text = shown(&app, i, end);
        assert_eq!(text.ends_with(" *"), marked, "{text}");
        assert!(on_chart(&text), "{text} on a bar label in {chart:?}");
    }
    // An actual date on a bar label shows no marker, as before.
    let started = (0..app.project.model.activities.len())
        .find(|&i| app.project.model.activities[i].act_start.is_some());
    if let Some(i) = started {
        let a = &app.project.model.activities[i];
        let text = crate::gantt::bar_label_date(
            origin,
            crate::util::displayed_start(a, false),
            a,
            End::Start,
        );
        assert!(!text.ends_with(" A") && !text.ends_with('*'), "{text}");
    }

    // Activity Network boxes carry the same marks as the table.
    assert_eq!(app.box_text(open[0]).start, start0);
    assert_eq!(
        app.box_text(open[0]).finish,
        shown(&app, open[0], End::Finish)
    );
    assert!(app.box_text(open[2]).finish.ends_with(" *"));
    app.set_top(crate::panes::TopView::Network);
    app.selected = Some(open[0]);
    let (texts, image) = paint(&mut app, &mut painter, &ctx, &mut time);
    image.save(out.join("network.png")).unwrap();
    assert!(
        texts.iter().any(|(s, _)| *s == start0),
        "{start0} in a network box"
    );
    drop(app);
    let _ = std::fs::remove_file(path);
}
