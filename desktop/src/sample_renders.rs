//! Renders of the first-run sample project, through the software painter
//! with no window, to the ignored `artifacts/sample/`:
//! `cargo test -p openxer-desktop --bin OpenXER sample_renders -- --ignored`.
//! They show the workspace a first start makes: the Activities view, the
//! Resource Usage Profile and the WBS view.
use super::*;
use crate::headless_render::SoftwarePainter;
use crate::panes::BottomView;

/// The app as a first start opens it, in a 1600 x 900 headless painter.
fn first_start(name: &str) -> (egui::Context, App, SoftwarePainter, f64, PathBuf) {
    let dir = std::env::temp_dir().join(format!("openxer-sample-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let args = vec![
        std::ffi::OsString::from("--database"),
        dir.join("openxer.db").into_os_string(),
    ];
    let started = launch(&args, |m| panic!("{m}")).unwrap();
    assert_eq!(started.error, None);
    std::env::set_var("USERNAME", "scheduler");
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, started.store, started.project);
    app.adopt_file_baselines();
    app.baselines_changed();
    drop(started.lock);
    let mut painter = SoftwarePainter::default();
    painter.screen = Some(Vec2::new(1600., 900.));
    let mut time = 0.;
    for _ in 0..4 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    (ctx, app, painter, time, dir)
}

fn save(
    app: &mut App,
    ctx: &egui::Context,
    painter: &mut SoftwarePainter,
    time: &mut f64,
    name: &str,
) {
    for _ in 0..3 {
        let _ = painter.frame(app, ctx, time, vec![], false);
    }
    let (image, triangles) = painter.frame(app, ctx, time, vec![], true).unwrap();
    assert!(triangles > 1000, "{name}: {triangles} triangles");
    assert!(app.error.is_none(), "{name}: {:?}", app.error);
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/sample");
    std::fs::create_dir_all(&out).unwrap();
    image.save(out.join(name)).unwrap();
    println!("Rendered {}", out.join(name).display());
}

#[test]
#[ignore = "renders the first-run sample to artifacts/sample"]
fn render_the_first_run_sample() {
    let (ctx, mut app, mut painter, mut time, dir) = first_start("views");
    save(&mut app, &ctx, &mut painter, &mut time, "activities.png");
    // The pipefitters' usage profile under the table.
    let pf = app
        .project
        .resource_names
        .iter()
        .find(|(_, n)| n.as_str() == "Pipefitter")
        .map(|(id, _)| *id);
    app.panes.usage.resource = pf;
    app.set_bottom(Some(BottomView::ResourceUsageProfile));
    save(&mut app, &ctx, &mut painter, &mut time, "usage-profile.png");
    // The electricians by week from the data date: the winter peak is over
    // their limit of ten.
    app.panes.usage.resource = app
        .project
        .resource_names
        .iter()
        .find(|(_, n)| n.as_str() == "Electrician")
        .map(|(id, _)| *id);
    // The day the electricians' remaining work peaks, from the early dates.
    let elec = app.panes.usage.resource.unwrap();
    let mut load: std::collections::BTreeMap<i64, f64> = Default::default();
    let t = app.project.document.table("TASKRSRC").unwrap();
    for r in &t.rows {
        if t.value(r, "rsrc_id").parse::<i64>() != Ok(elec) {
            continue;
        }
        let task: i64 = t.value(r, "task_id").parse().unwrap();
        let a = app
            .project
            .model
            .activities
            .iter()
            .find(|a| a.task_id == task)
            .unwrap();
        if let (false, Some(s), Some(f)) = (a.is_completed(), a.early_start, a.early_finish) {
            let units: f64 = t.value(r, "remain_qty_per_hr").parse().unwrap_or(0.);
            for d in s / DAY..=(f - 1) / DAY {
                *load.entry(d).or_default() += units;
            }
        }
    }
    let peak = load
        .iter()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(d, _)| *d)
        .unwrap();
    assert!(
        load[&peak] > 10.,
        "the electricians' peak passes their limit"
    );
    app.pixels_per_day = 16.;
    app.timeline_origin = (peak - 14) * DAY;
    app.update_timescale_units();
    save(
        &mut app,
        &ctx,
        &mut painter,
        &mut time,
        "usage-electricians-weekly.png",
    );
    app.set_bottom(None);
    painter.click_action(&mut app, &ctx, &mut time, "directory_wbs");
    save(&mut app, &ctx, &mut painter, &mut time, "wbs.png");
    painter.click_action(&mut app, &ctx, &mut time, "directory_projects");
    save(&mut app, &ctx, &mut painter, &mut time, "projects.png");
    drop(app);
    let _ = std::fs::remove_dir_all(&dir);
}
