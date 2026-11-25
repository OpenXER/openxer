//! Test helpers shared by the Schedule Comparison, Project Checker, Report
//! Wizard and Spell Check tests.
use crate::dialog_kit as kit;
use crate::store::Store;
use crate::App;
use eframe::egui;
use openxer_xer::ProjectData;
use std::path::{Path, PathBuf};

pub(crate) fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("openxer-g3-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub(crate) fn scheduled_starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    crate::scheduling::schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

/// An app on a fresh workspace in `dir` with the scheduled starter saved.
pub(crate) fn app(dir: &Path) -> (App, egui::Context) {
    let store = Store::open(dir.join("workspace.db")).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, scheduled_starter());
    app.save();
    (app, ctx)
}

/// Crop a dialog drawn this frame: its client area plus the title bar.
pub(crate) fn crop_dialog(
    image: &image::RgbaImage,
    ctx: &egui::Context,
    salt: &str,
) -> image::RgbaImage {
    let client = kit::noted_client(ctx, salt).unwrap_or_else(|| panic!("{salt} was not drawn"));
    let left = (client.left() - 1.).max(0.) as u32;
    let top = (client.top() - 32.).max(0.) as u32;
    let w = (client.width() + 2.) as u32;
    let h = (client.height() + 33.) as u32;
    image::imageops::crop_imm(
        image,
        left,
        top,
        w.min(image.width() - left),
        h.min(image.height() - top),
    )
    .to_image()
}

/// The gitignored folder for the package's renders.
pub(crate) fn artifacts() -> PathBuf {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/g3");
    std::fs::create_dir_all(&out).unwrap();
    out
}

/// Removes the starter library (its reports and its group) so a test sees
/// only the reports it makes.
pub(crate) fn without_library(app: &mut App) {
    use crate::reports::rpt::store;
    let db = app.store.connection();
    for s in store::list(db).unwrap() {
        if s.lib_id.is_some() {
            store::delete(db, s.id).unwrap();
        }
    }
    let mut groups = app.report_groups();
    groups.retain(|g| g.name != store::LIBRARY_GROUP);
    app.save_report_groups(&groups);
}
