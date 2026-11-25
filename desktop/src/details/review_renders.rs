//! Review render of the Activity Details pane: the starter project in a
//! 1080p window with the pane at a fixed height, one image per default
//! tab, written to the ignored `artifacts/review/details/` folder.
use super::*;
use crate::headless_render::SoftwarePainter;
use crate::store::Store;
use std::path::PathBuf;

/// Client y of the tab strip's top line in the render.
const STRIP_TOP: f32 = 425.;

/// The default tabs in strip order, with the image each one is in.
pub const DEFAULT_TABS: [(DetailTab, &str); 12] = [
    (DetailTab::General, "general"),
    (DetailTab::Status, "status"),
    (DetailTab::Resources, "resources"),
    (DetailTab::Codes, "codes"),
    (DetailTab::Relationships, "relationships"),
    (DetailTab::Notebook, "notebook"),
    (DetailTab::Steps, "steps"),
    (DetailTab::Feedback, "feedback"),
    (DetailTab::Documents, "wpsdocs"),
    (DetailTab::Risks, "risks"),
    (DetailTab::Expenses, "expenses"),
    (DetailTab::Summary, "summary"),
];

#[test]
fn render_activity_details_for_review() {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/review/details");
    std::fs::create_dir_all(&output).unwrap();
    let database = std::env::temp_dir().join(format!(
        "openxer-details-review-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let mut project = openxer_xer::starter();
        schedule_model(&mut project.model, &project.resources).unwrap();
        let mut store = Store::open(database.clone()).unwrap();
        store.save(&project).unwrap();
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let mut app = App::new(&ctx, store, project);
        let mut painter = SoftwarePainter::default();
        painter.screen = Some(Vec2::new(1920., 1009.));
        let mut time = 0.;
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        // The pane at a fixed height: move its top edge until the tab strip
        // starts at STRIP_TOP.
        for _ in 0..3 {
            let id = egui::Id::new("activity_details");
            let state = egui::containers::panel::PanelState::load(&ctx, id).unwrap();
            let shift = app.action_rects["detail_tab_general"][1] - STRIP_TOP;
            let mut rect = state.rect;
            rect.min.y -= shift;
            ctx.data_mut(|d| d.insert_persisted(id, egui::containers::panel::PanelState { rect }));
            for _ in 0..2 {
                let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
            }
        }
        // A completed activity, so the status fields are filled.
        let i = app
            .project
            .model
            .activities
            .iter()
            .position(|a| a.task_id == 1080)
            .unwrap();
        app.selected = Some(i);
        {
            let a = &mut app.project.model.activities[i];
            let start = a.early_start.unwrap();
            a.status = StatusType::Completed;
            a.act_start = Some(start);
            a.act_finish = Some(start + 2 * 600);
            a.rem = 0;
        }
        app.detail.tabs = DEFAULT_TABS.iter().map(|(t, _)| *t).collect();
        let mut geometry = serde_json::Map::new();
        for (tab, name) in DEFAULT_TABS {
            app.detail.tab = tab;
            for _ in 0..3 {
                let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
            }
            let (image, _) = painter
                .frame(&mut app, &ctx, &mut time, vec![], true)
                .unwrap();
            assert_eq!(image.dimensions(), (1920, 1009));
            image
                .save(output.join(format!("openxer-details-{name}.png")))
                .unwrap();
            let rects: serde_json::Map<_, _> = app
                .action_rects
                .iter()
                .filter(|(k, _)| k.starts_with("detail"))
                .map(|(k, v)| (k.clone(), serde_json::json!(v)))
                .collect();
            geometry.insert(name.into(), serde_json::Value::Object(rects));
            assert!(app.error.is_none(), "{name}: {:?}", app.error);
        }
        // The tab strip's context menu, opened with a real right click.
        let [_, t, r, b] = app.action_rects["detail_tab_summary"];
        let pos = Pos2::new(r + 40., (t + b) / 2.);
        for pressed in [true, false] {
            let _ = painter.frame(
                &mut app,
                &ctx,
                &mut time,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
                false,
            );
        }
        let _ = painter.frame(
            &mut app,
            &ctx,
            &mut time,
            vec![egui::Event::PointerGone],
            false,
        );
        let (image, _) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        image.save(output.join("openxer-details-menu.png")).unwrap();
        let hide = app.action_rects["detail_menu_hide"];
        let customize = app.action_rects["detail_menu_customize"];
        assert!(hide[1] < customize[1], "Hide Detail Window comes first");
        geometry.insert(
            "menu".into(),
            serde_json::json!({"hide": hide, "customize": customize}),
        );
        let _ = painter.frame(
            &mut app,
            &ctx,
            &mut time,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
            false,
        );
        // Customize Activity Details.
        app.open_customize_details();
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        let (image, _) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        image
            .save(output.join("openxer-details-customize.png"))
            .unwrap();
        geometry.insert(
            "customize".into(),
            serde_json::json!({"ok": app.action_rects["customize_tabs_ok"]}),
        );
        std::fs::write(
            output.join("openxer-details.json"),
            serde_json::to_string_pretty(&geometry).unwrap(),
        )
        .unwrap();
    }
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", database.display()));
    }
}
