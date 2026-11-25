//! Layout checks for the Status tab of Activity Details: the date fields'
//! calendar drop-down stays inside its `...` face, and the three group boxes
//! (Duration, Status/Constraints, Units) never overlap or run past the pane.
use super::headless_tests::{cleanup, temp_db};
use super::*;
use crate::headless_render::SoftwarePainter;
use crate::store::Store;
use std::path::PathBuf;

struct Status {
    ctx: egui::Context,
    app: App,
    time: f64,
    path: PathBuf,
    painter: SoftwarePainter,
}

/// The starter project with a started activity selected on the Status tab.
fn status_app(name: &str, width: f32) -> Status {
    let path = temp_db(name);
    let store = Store::open(path.clone()).unwrap();
    let mut project = openxer_xer::starter();
    schedule_model(&mut project.model, &project.resources).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    // A started activity with no predecessor, so no out-of-sequence prompt.
    let model = &app.project.model;
    let i = (0..model.activities.len())
        .find(|&i| !model.links.iter().any(|l| l.succ == i))
        .unwrap();
    app.selected = Some(i);
    app.edit_activity(i, setters::Edit::Status(StatusType::InProgress))
        .unwrap();
    let mut time = 0.;
    let mut painter = SoftwarePainter::default();
    for _ in 0..3 {
        let _ = step(&mut painter, &ctx, &mut app, &mut time, width, vec![]);
    }
    let [l, t, r, b] = app.action_rects["detail_tab_status"];
    let pos = Pos2::new((l + r) / 2., (t + b) / 2.);
    for pressed in [true, false] {
        let _ = step(
            &mut painter,
            &ctx,
            &mut app,
            &mut time,
            width,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
    let _ = step(
        &mut painter,
        &ctx,
        &mut app,
        &mut time,
        width,
        vec![egui::Event::PointerGone],
    );
    let _ = step(&mut painter, &ctx, &mut app, &mut time, width, vec![]);
    Status {
        ctx,
        app,
        time,
        path,
        painter,
    }
}

/// One frame of a `width` x 900 window.
fn step(
    painter: &mut SoftwarePainter,
    ctx: &egui::Context,
    app: &mut App,
    time: &mut f64,
    width: f32,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    *time += 0.1;
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 900.))),
            time: Some(*time),
            focused: true,
            events,
            ..Default::default()
        },
        |ctx| app.update_ui(ctx),
    );
    painter.ingest(&output);
    output
}

fn rect_of(app: &App, action: &str) -> Rect {
    let [l, t, r, b] = app
        .action_rects
        .get(action)
        .copied()
        .unwrap_or_else(|| panic!("No action {action}"));
    Rect::from_min_max(Pos2::new(l, t), Pos2::new(r, b))
}

/// Every visible shape that touches a date field's `...` face lies inside
/// the field, and no calendar glyph is drawn anywhere on the tab.
#[test]
fn date_picker_glyph_stays_inside_the_ellipsis_button() {
    for width in [1366., 1600., 1920.] {
        let Status {
            ctx,
            mut app,
            mut time,
            path,
            mut painter,
        } = status_app("status-glyph", width);
        let output = step(&mut painter, &ctx, &mut app, &mut time, width, vec![]);
        let fields: Vec<(String, Rect)> = app
            .action_rects
            .keys()
            .filter(|k| k.starts_with("detail_date_"))
            .map(|k| (k.clone(), rect_of(&app, k)))
            .collect();
        assert!(fields.len() >= 6, "{width}: date fields {fields:?}");
        for clipped in &output.shapes {
            let egui::Shape::Text(text) = &clipped.shape else {
                continue;
            };
            let shown = text.visual_bounding_rect().intersect(clipped.clip_rect);
            // Empty text (an empty field or button caption) draws nothing.
            if !shown.is_positive() || text.galley.text().trim().is_empty() {
                continue;
            }
            assert!(
                !text.galley.text().contains('\u{1F4C6}'),
                "{width}: calendar glyph drawn at {shown:?}"
            );
            for (key, field) in &fields {
                // The action rect is the text area; the `...` face docks in
                // the 20 px right of it, inside the sunken box.
                let face = Rect::from_min_max(
                    Pos2::new(field.right(), field.top()),
                    Pos2::new(field.right() + 20., field.bottom()),
                );
                let boxed =
                    Rect::from_min_max(field.min - Vec2::splat(3.), face.max + Vec2::splat(3.));
                if shown.intersects(face.expand(6.)) && !boxed.contains_rect(shown) {
                    panic!(
                        "{width}: {key}: text {:?} at {shown:?} spills out of {field:?}",
                        text.galley.text()
                    );
                }
            }
        }
        // The face still opens the calendar drop-down.
        let i = app.selected.unwrap();
        let field = rect_of(&app, &format!("detail_date_{i}-act_start"));
        let pos = Pos2::new(field.right() + 10., field.center().y);
        for pressed in [true, false] {
            let _ = step(
                &mut painter,
                &ctx,
                &mut app,
                &mut time,
                width,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        let output = step(&mut painter, &ctx, &mut app, &mut time, width, vec![]);
        let shown = super::headless_tests::texts(&output);
        assert!(
            shown.iter().any(|t| t == "Today") && shown.iter().any(|t| t == "Mo"),
            "{width}: no calendar drop-down"
        );
        assert!(
            !shown.iter().any(|t| t == "Save"),
            "{width}: calendar drop-down should select on click without Save"
        );
        drop(app);
        cleanup(path);
    }
}

/// The Units group keeps clear of the Status and Constraints group and its
/// fields, and stays inside the pane at common widths.
#[test]
fn status_tab_groups_do_not_overlap_or_clip() {
    for width in [1024., 1366., 1600., 1920.] {
        let Status {
            ctx,
            mut app,
            mut time,
            path,
            mut painter,
        } = status_app("status-groups", width);
        let _ = step(&mut painter, &ctx, &mut app, &mut time, width, vec![]);
        let bar = rect_of(&app, "detail_units_mode");
        let units_left = bar.left() - 1.;
        // Every Status and Constraints control ends left of the Units group.
        for key in [
            "detail_date_{i}-suspend",
            "detail_date_{i}-resume",
            "detail_date_{i}-secondary",
            "detail_constraint_secondary",
        ] {
            let i = app.selected.unwrap();
            let key = key.replace("{i}", &i.to_string());
            let r = rect_of(&app, &key);
            assert!(
                r.right() <= units_left - 5.,
                "{width}: {key} {r:?} runs into the Units group at {units_left}"
            );
        }
        // The Units fields keep their full width inside the pane.
        let budget = rect_of(&app, "detail_units_budgeted");
        let pane = ctx.content_rect();
        assert!(budget.width() >= 100., "{width}: {budget:?}");
        assert!(
            bar.right() <= pane.right(),
            "{width}: Units group {bar:?} past the window {pane:?}"
        );
        drop(app);
        cleanup(path);
    }
}

/// Writes `artifacts/status-tab/status-<width>.png` for visual review.
/// `cargo.exe test -p openxer-desktop render_status_tab -- --ignored`.
#[test]
#[ignore = "writes review renders to artifacts/status-tab; run on demand"]
fn render_status_tab_widths_to_png() {
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts")
        .join("status-tab");
    std::fs::create_dir_all(&artifacts).unwrap();
    for width in [1024., 1366., 1600., 1920.] {
        let Status {
            ctx,
            mut app,
            mut time,
            path,
            mut painter,
        } = status_app("status-render", width);
        let output = step(&mut painter, &ctx, &mut app, &mut time, width, vec![]);
        let (image, _) = painter.rasterize(
            &ctx,
            output.shapes,
            output.pixels_per_point,
            Vec2::new(width, 900.),
        );
        image
            .save(artifacts.join(format!("status-{width}.png")))
            .unwrap();
        drop(app);
        cleanup(path);
    }
}
