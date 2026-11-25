//! Software renders of the activity table's date alignment to the ignored
//! `artifacts/date-align/`: `before.png` with every date column set Right
//! (how the old default drew them), `after.png` with the defaults (dates
//! Left, numbers Right), and `detail-summary-dates.png` with the Activity
//! Details Summary tab showing dates.
use super::columns::{self, Align, Kind};
use super::layout::ColumnSetting;
use crate::headless_render::SoftwarePainter;
use crate::store::Store;
use crate::App;
use eframe::egui;
use std::path::PathBuf;

/// Where `text` paints inside the horizontal range `x_range`.
fn text_rect(output: &egui::FullOutput, text: &str, x_range: (f32, f32)) -> Option<egui::Rect> {
    output.shapes.iter().find_map(|c| match &c.shape {
        egui::Shape::Text(t) if t.galley.text() == text => {
            let r = t.visual_bounding_rect().intersect(c.clip_rect);
            (r.is_positive() && r.left() >= x_range.0 && r.right() <= x_range.1).then_some(r)
        }
        _ => None,
    })
}

fn run(app: &mut App, ctx: &egui::Context, time: &mut f64) -> egui::FullOutput {
    *time += 0.1;
    ctx.run(
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
    )
}

#[test]
fn render_date_alignment_before_and_after() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/date-align");
    std::fs::create_dir_all(&out).unwrap();
    let path = std::env::temp_dir().join(format!("openxer-date-align-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut project = openxer_xer::starter();
    crate::scheduling::schedule_model(&mut project.model, &project.resources).unwrap();
    let mut store = Store::open(path.clone()).unwrap();
    store.save(&project).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    app.selected = None;
    app.grid
        .layout
        .columns
        .insert(6, ColumnSetting::new("late_finish", 100.));
    let catalog = columns::catalog(&app.project);
    let is_date = |id: &str| columns::find(&catalog, id).is_some_and(|c| c.kind == Kind::Date);
    let mut time = 0.;
    let mut painter = SoftwarePainter::default();

    // Before: dates drawn Right, as the old default did.
    let after_layout = app.grid.layout.clone();
    for c in &mut app.grid.layout.columns {
        if is_date(&c.id) {
            c.align = Some(Align::Right);
        }
    }
    app.drop_frame_cache();
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("before.png")).unwrap();

    // After: the defaults.
    app.grid.layout = after_layout;
    app.drop_frame_cache();
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("after.png")).unwrap();

    // Date text starts at the column's left edge; float ends at its right.
    let output = run(&mut app, &ctx, &mut time);
    painter.ingest(&output);
    let header = |id: &str| {
        // The header's action rect sits a few pixels inside the column.
        let [l, _, r, _] = app.action_rects[&format!("grid_header_{id}")];
        (l - 6., r + 6.)
    };
    let i = (0..app.project.model.activities.len())
        .find(|&i| {
            let a = &app.project.model.activities[i];
            a.act_start.is_none() && a.early_start.is_some()
        })
        .unwrap();
    let visible = app.visible_columns(&catalog);
    let start_col = visible.iter().find(|c| c.id == "start").unwrap();
    let start_text = columns::display(
        &app.project,
        None,
        &super::marks::start_value(&app.project.model.activities[i], false),
    );
    let (l, r) = header("start");
    let rect = text_rect(&output, &start_text, (l, r)).unwrap_or_else(|| {
        let all: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Text(t) if t.galley.text() == start_text => {
                    Some((t.visual_bounding_rect(), c.clip_rect))
                }
                _ => None,
            })
            .collect();
        panic!("{start_text:?} painted in the Start column {l}..{r}: {all:?}")
    });
    assert_eq!(start_col.align, Align::Left);
    assert!(
        rect.left() - l < 8.,
        "Start sits left: {rect:?} in {l}..{r}"
    );
    assert!(
        r - rect.right() > 8.,
        "Start is not right aligned: {rect:?}"
    );
    let (l, r) = header("total_float");
    let floats: Vec<egui::Rect> = output
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => {
                let b = t.visual_bounding_rect().intersect(c.clip_rect);
                (b.is_positive() && b.left() >= l && b.right() <= r && b.top() > 80.).then_some(b)
            }
            _ => None,
        })
        .collect();
    assert!(!floats.is_empty(), "Total Float cells paint");
    for b in floats {
        assert!(
            r - b.right() < 8.,
            "Total Float stays right: {b:?} in {l}..{r}"
        );
    }

    // Activity Details Summary tab with dates.
    app.selected = Some(i);
    app.detail.summary_mode = crate::details::SummaryMode::Dates;
    app.show_detail_tab(crate::details::DetailTab::Summary);
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("detail-summary-dates.png")).unwrap();
    drop(app);
    let _ = std::fs::remove_file(path);
}
