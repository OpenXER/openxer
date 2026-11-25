//! Software renders, for review, to the ignored
//! `artifacts/l1/`: text attachments with fonts and band anchors, the text
//! dialog and its font picker, Activity Network Copy From, the bottom Gantt
//! with its own split and timescale, the usage profile's cumulative curve,
//! the layout bar naming a path filter, and a submenu sliding up to the
//! screen edge.
use super::*;
use crate::gantt::settings::{Rgb, TextAttachment, TextFont};
use crate::headless_render::SoftwarePainter;
use crate::panes::{BottomView, TopView};
use crate::views::resources as rs;

/// The starter with a costed labor assignment (24 h at 40 an hour, 8 h done).
fn costed() -> ProjectData {
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

#[test]
fn render_l1_surfaces() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/l1");
    std::fs::create_dir_all(&out).unwrap();
    let path = std::env::temp_dir().join(format!("openxer-l1-render-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let project = costed();
    let mut store = Store::open(path.clone()).unwrap();
    store.save(&project).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    let mut time = 0.;
    let mut painter = SoftwarePainter::default();
    let shot = |app: &mut App, painter: &mut SoftwarePainter, time: &mut f64, name: &str| {
        for _ in 0..3 {
            let _ = painter.frame(app, &ctx, time, vec![], false);
        }
        let (image, triangles) = painter.frame(app, &ctx, time, vec![], true).unwrap();
        assert!(triangles > 300, "{name}");
        image.save(out.join(format!("{name}.png"))).unwrap();
        assert!(app.error.is_none(), "{name}: {:?}", app.error);
    };
    // Text attachments: an activity row, a band row and a styled one.
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let dd = app.project.model.project.data_date.unwrap_or(0);
    let band = app
        .row_geometry
        .iter()
        .find_map(|(a, _)| match a {
            RowAnchor::Band(key) => Some(key.clone()),
            RowAnchor::Activity(_) => None,
        })
        .unwrap();
    app.gantt.texts.push(TextAttachment {
        text: "Crane on site".into(),
        date: dd + 9 * DAY,
        task_id: 1030,
        band: None,
        font: TextFont::default(),
    });
    app.gantt.texts.push(TextAttachment {
        text: "Phase 1 shutdown".into(),
        date: dd + 3 * DAY,
        task_id: 0,
        band: Some(band),
        font: TextFont {
            face: crate::chrome::Face::SegoeUi,
            points: 10.,
            bold: true,
            italic: false,
            color: Rgb(0xC0, 0x00, 0x00),
        },
    });
    shot(&mut app, &mut painter, &mut time, "text-attachments");
    app.edit_text_attachment(1);
    shot(&mut app, &mut painter, &mut time, "text-dialog");
    app.open_text_font_picker(app.gantt.texts[1].font);
    shot(&mut app, &mut painter, &mut time, "text-font-picker");
    app.layout_opts.drafts.picker = None;
    app.gantt_ui.text = None;
    // Activity Network Copy From.
    app.panes.network.options.template = crate::panes::boxes::Template::Dates;
    app.panes
        .network
        .positions
        .insert("RB-1020".into(), Pos2::new(300., 200.));
    app.save_layout_as("Dates moved").unwrap();
    app.panes.network.options = Default::default();
    app.panes.network.positions.clear();
    app.set_top(TopView::Network);
    app.set_bottom(None);
    app.open_network_options();
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "network_copy_from");
    shot(&mut app, &mut painter, &mut time, "network-copy-from");
    painter.click_action(&mut app, &ctx, &mut time, "network_copy_ok");
    shot(&mut app, &mut painter, &mut time, "network-after-copy");
    app.panes.network_dialog = None;
    app.set_top(TopView::Gantt);
    // The bottom Gantt with its own split and timescale.
    app.gantt.texts.clear();
    app.set_bottom(Some(BottomView::Gantt));
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    app.panes.grid.scale.grid_fraction = 0.35;
    app.panes.grid.scale.timeline_origin = Some(app.timeline_origin + 14 * DAY);
    app.panes.grid.scale.pixels_per_day *= 1.8;
    shot(&mut app, &mut painter, &mut time, "bottom-gantt-own-scale");
    // The usage profile: bars with the cumulative curve, units and costs.
    app.set_bottom(Some(BottomView::ActivityUsageProfile));
    for s in app.panes.usage.profile.activity.iter_mut() {
        s.cumulative = s.by_date;
    }
    shot(&mut app, &mut painter, &mut time, "usage-cumulative-units");
    app.panes.usage.costs = true;
    shot(&mut app, &mut painter, &mut time, "usage-cumulative-costs");
    app.panes.usage = Default::default();
    app.set_bottom(Some(BottomView::Details));
    // The layout bar while a critical path filter shows.
    let last = (0..app.project.model.activities.len())
        .max_by_key(|&i| app.project.model.activities[i].early_finish)
        .unwrap();
    app.selected = Some(last);
    app.toggle_critical_paths(crate::critical_path::Direction::Backward);
    shot(
        &mut app,
        &mut painter,
        &mut time,
        "filter-bar-critical-path",
    );
    app.hide_critical_paths();
    // A submenu taller than the space under its item slides up to the
    // screen edge (1920 x 1009, the reference client size).
    painter.screen = Some(Vec2::new(1920., 1009.));
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "menu_bar_view");
    assert!(painter.hover_text(&mut app, &ctx, &mut time, "Toolbars"));
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("submenu-slide.png")).unwrap();
    drop(app);
    let _ = std::fs::remove_file(path);
}
