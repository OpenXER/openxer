//! Production-UI interaction tests for view switching and the WBS, Projects
//! and Resources views. Like `headless_ui`, these drive `App::update_ui`
//! through egui `RawInput` only: no native window, no system pointer.
use super::*;

pub(super) fn frame(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>, time: &mut f64) {
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

pub(super) fn press(app: &mut App, ctx: &egui::Context, pos: Pos2, time: &mut f64, count: usize) {
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

pub(super) fn center(app: &App, action: &str) -> Pos2 {
    let [l, t, r, b] = *app
        .action_rects
        .get(action)
        .unwrap_or_else(|| panic!("No rendered control {action:?}"));
    Pos2::new((l + r) / 2., (t + b) / 2.)
}

pub(super) fn click(app: &mut App, ctx: &egui::Context, action: &str, time: &mut f64) {
    if !app.action_rects.contains_key(action) {
        if let Some(menu) = crate::menu::menu_holding(action) {
            click_text(app, ctx, menu, time);
        }
    }
    let pos = center(app, action);
    press(app, ctx, pos, time, 1);
}

/// Click a row away from its expander and icon, in the name column.
pub(super) fn click_row(
    app: &mut App,
    ctx: &egui::Context,
    grid: &str,
    row: usize,
    time: &mut f64,
) {
    let [l, t, _, b] = app.action_rects[&format!("{grid}_row_{row}")];
    let name = app.action_rects[&format!("{grid}_col_1")];
    let pos = Pos2::new(name[0].max(l) + 30., (t + b) / 2.);
    press(app, ctx, pos, time, 1);
}

/// Menus have no stable action IDs until open; locate their rendered title.
pub(super) fn click_text(app: &mut App, ctx: &egui::Context, text: &str, time: &mut f64) {
    *time += 0.1;
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500., 900.))),
            time: Some(*time),
            focused: true,
            ..Default::default()
        },
        |ctx| app.update_ui(ctx),
    );
    let pos = output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::Shape::Text(shape) if shape.galley.text() == text => {
                Some(shape.visual_bounding_rect().center())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("No visible UI text {text:?}"));
    press(app, ctx, pos, time, 1);
}

/// View > Show on Bottom > `item`.
pub(super) fn show_on_bottom(app: &mut App, ctx: &egui::Context, item: &str, time: &mut f64) {
    click_text(app, ctx, "View", time);
    click_text(app, ctx, "Show on Bottom", time);
    click(app, ctx, item, time);
}

pub(super) fn setup(name: &str, project: ProjectData) -> (PathBuf, egui::Context, App, f64) {
    setup_with(name, project, true)
}

/// `schedule` runs F9 first when the project has no dates (the starter).
pub(super) fn setup_with(
    name: &str,
    mut project: ProjectData,
    schedule: bool,
) -> (PathBuf, egui::Context, App, f64) {
    if schedule
        && project
            .model
            .activities
            .iter()
            .all(|a| a.early_start.is_none())
    {
        schedule_model(&mut project.model, &project.resources).unwrap();
    }
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "openxer-views-{name}-{}-{unique}.db",
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

#[test]
fn directory_menus_and_back_forward_switch_every_view_and_return() {
    let (path, ctx, mut app, mut time) = setup("switch", resources::tests::with_resource());
    assert_eq!(app.views.main, MainView::Activities);
    assert_eq!(app.bar_geometry.len(), 14);
    for (action, view, probe) in [
        ("directory_wbs", MainView::Wbs, "wbs_row_0"),
        ("directory_projects", MainView::Projects, "projects_row_1"),
        (
            "directory_resources",
            MainView::Resources,
            "resources_row_0",
        ),
    ] {
        click(&mut app, &ctx, action, &mut time);
        assert_eq!(app.views.main, view, "{action}");
        assert!(
            app.action_rects.contains_key(probe),
            "{action} renders {probe}"
        );
        assert!(
            app.bar_geometry.is_empty(),
            "the Gantt is not drawn under {view:?}"
        );
    }
    // Every opened view has a dock tab; the tabs switch views.
    assert_eq!(
        app.views.opened(),
        vec![
            MainView::Activities,
            MainView::Projects,
            MainView::Wbs,
            MainView::Resources
        ]
    );
    click(&mut app, &ctx, "dock_tab_wbs", &mut time);
    assert_eq!(app.views.main, MainView::Wbs);
    click(&mut app, &ctx, "dock_tab_projects", &mut time);
    assert_eq!(app.views.main, MainView::Projects);
    // The caption's close button closes the view's tab.
    click(&mut app, &ctx, "dock_tab_resources", &mut time);
    click(&mut app, &ctx, "view_close", &mut time);
    assert!(!app.views.opened().contains(&MainView::Resources));
    assert_eq!(app.views.main, MainView::Wbs);
    click(&mut app, &ctx, "directory_activities", &mut time);
    assert_eq!(app.views.main, MainView::Activities);
    assert_eq!(
        app.bar_geometry.len(),
        14,
        "Activities draws its Gantt again"
    );
    // Enterprise > Resources and Project > WBS reach the same views.
    click_text(&mut app, &ctx, "Enterprise", &mut time);
    click(&mut app, &ctx, "menu_resources", &mut time);
    assert_eq!(app.views.main, MainView::Resources);
    click_text(&mut app, &ctx, "Project", &mut time);
    click(&mut app, &ctx, "menu_wbs", &mut time);
    assert_eq!(app.views.main, MainView::Wbs);
    click(&mut app, &ctx, "dock_tab_resources", &mut time);
    assert_eq!(app.views.main, MainView::Resources);
    show_on_bottom(&mut app, &ctx, "menu_resources_bottom_none", &mut time);
    assert!(!app.views.resources.show_bottom);
    show_on_bottom(&mut app, &ctx, "menu_resources_bottom_details", &mut time);
    assert!(app.views.resources.show_bottom);
    assert!(app.error.is_none());
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn wbs_commands_merge_and_persist_through_save_and_reopen() {
    let (path, ctx, mut app, mut time) = setup("wbs", openxer_xer::starter());
    click(&mut app, &ctx, "directory_wbs", &mut time);
    // Row 0 is the project row; rows 1-4 are RB.1-RB.4.
    click_row(&mut app, &ctx, "wbs", 3, &mut time);
    assert_eq!(app.views.wbs.selected, Some(30));
    click(&mut app, &ctx, "dock_move_right", &mut time);
    assert_eq!(
        app.project.wbs.iter().find(|w| w.id == 30).unwrap().parent,
        Some(20)
    );
    assert!(app.dirty);
    click(&mut app, &ctx, "dock_move_up", &mut time);
    assert!(app.status.contains("already first"), "{}", app.status);
    let moved = app.project.details.values().filter(|d| d.wbs == 30).count();
    let before = app.project.details.values().filter(|d| d.wbs == 20).count();
    click(&mut app, &ctx, "dock_delete", &mut time);
    assert!(app.views.wbs.pending.is_some());
    click(&mut app, &ctx, "wbs_confirm_merge", &mut time);
    assert!(app.views.wbs.pending.is_none());
    assert!(app.project.wbs.iter().all(|w| w.id != 30));
    assert_eq!(
        app.project.details.values().filter(|d| d.wbs == 20).count(),
        before + moved
    );
    assert_eq!(app.views.wbs.selected, Some(20));
    // Structural WBS edits are undoable: undo the merge, then redo it.
    assert_eq!(
        app.history.counts(),
        (2, 0),
        "shift and merge are undo steps"
    );
    app.history_move(&ctx, false);
    assert!(
        app.project.wbs.iter().any(|w| w.id == 30),
        "undo restores the element"
    );
    assert_eq!(
        app.project.details.values().filter(|d| d.wbs == 30).count(),
        moved,
        "undo moves its activities back"
    );
    app.history_move(&ctx, true);
    assert!(
        app.project.wbs.iter().all(|w| w.id != 30),
        "redo merges again"
    );
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "add", &mut time);
    let added = app.views.wbs.selected.unwrap();
    assert_eq!(
        app.project.wbs.iter().find(|w| w.id == added).unwrap().code,
        "RB.3"
    );
    app.save();
    assert!(!app.dirty);
    let reopened = app.store.last().unwrap().unwrap();
    assert!(reopened.wbs.iter().all(|w| w.id != 30));
    assert!(reopened
        .wbs
        .iter()
        .any(|w| w.id == added && w.code == "RB.3"));
    assert_eq!(
        reopened.details.values().filter(|d| d.wbs == 20).count(),
        before + moved
    );
    // The Activities grouping follows the edited tree.
    click(&mut app, &ctx, "directory_activities", &mut time);
    assert_eq!(app.bar_geometry.len(), 14);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn view_layouts_persist_across_app_reopen() {
    let (path, ctx, mut app, mut time) = setup("layout", openxer_xer::starter());
    click(&mut app, &ctx, "directory_wbs", &mut time);
    let edge = center(&app, "wbs_resize_1");
    let before = app.views.wbs.columns.widths[1];
    let pointer = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(edge), pointer(edge, true)],
        &mut time,
    );
    for step in 1..=4 {
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(
                edge + Vec2::new(15. * step as f32, 0.),
            )],
            &mut time,
        );
    }
    let end = edge + Vec2::new(60., 0.);
    frame(&mut app, &ctx, vec![pointer(end, false)], &mut time);
    let widened = app.views.wbs.columns.widths[1];
    assert!(widened > before + 40., "{before} -> {widened}");
    click(&mut app, &ctx, "wbs_tab_2", &mut time);
    assert_eq!(app.views.wbs.tab, 2);
    click(&mut app, &ctx, "directory_resources", &mut time);
    show_on_bottom(&mut app, &ctx, "menu_resources_bottom_none", &mut time);
    for _ in 0..8 {
        frame(&mut app, &ctx, vec![], &mut time);
    }
    assert!(app.views.save_at.is_none(), "idle save ran");
    let store = Store::open(path.clone()).unwrap();
    let reopened = App::new(&ctx, store, openxer_xer::starter());
    assert_eq!(reopened.views.wbs.columns.widths[1], widened);
    assert_eq!(reopened.views.wbs.tab, 2);
    assert!(!reopened.views.resources.show_bottom);
    drop(reopened);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn projects_view_opens_another_workspace_project() {
    let mut second = openxer_xer::starter();
    second.source_name = "Second".into();
    second.model.project.name = "AAA-SECOND".into();
    let (path, ctx, mut app, mut time) = setup("projects", second);
    app.load(openxer_xer::starter());
    frame(&mut app, &ctx, vec![], &mut time);
    assert_eq!(app.project.model.project.name, "RIVERBEND");
    click(&mut app, &ctx, "directory_projects", &mut time);
    // Rows: EPS, AAA-SECOND, RIVERBEND (sorted by Project ID).
    click_row(&mut app, &ctx, "projects", 1, &mut time);
    assert_eq!(app.views.projects.selected, Some(("Second".to_string(), 1)));
    click(&mut app, &ctx, "projects_cmd_open", &mut time);
    assert_eq!(app.project.model.project.name, "AAA-SECOND");
    assert_eq!(app.views.main, MainView::Activities);
    assert!(app.error.is_none());
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// Real exported XERs (project nodes, OBS, resources, rates, assignments).
/// Skipped when the local corpus is not present.
#[test]
fn corpus_projects_render_every_view_and_keep_wbs_consistent() {
    let corpus = crate::test_corpus::dir();
    let Ok(entries) = std::fs::read_dir(&corpus) else {
        return;
    };
    let mut checked = 0;
    for entry in entries {
        let path = entry.unwrap().path();
        // Saving very large files is slow in debug builds (XER export scans
        // relationships repeatedly), so the routine run covers files up to 1 MB.
        let large = std::fs::metadata(&path).map_or(true, |m| m.len() > 1_000_000);
        if path.extension().is_none_or(|x| x != "xer") || large {
            continue;
        }
        let doc = Document::read(&path).unwrap();
        let Some((key, _)) = doc.projects().first().cloned() else {
            continue;
        };
        let Ok(project) = ProjectData::from_document(doc, key, path.display().to_string()) else {
            continue;
        };
        let tree = wbs::Tree::build(&project);
        assert_eq!(
            tree.preorder().len(),
            project.wbs.len(),
            "{}",
            path.display()
        );
        let totals = wbs::rollups(&project, &tree);
        assert_eq!(totals[&None].activities, project.model.activities.len());
        let started = std::time::Instant::now();
        let (db, ctx, mut app, mut time) = setup_with("corpus", project, false);
        for action in ["directory_wbs", "directory_projects", "directory_resources"] {
            click(&mut app, &ctx, action, &mut time);
        }
        show_on_bottom(&mut app, &ctx, "menu_resources_bottom_none", &mut time);
        click(&mut app, &ctx, "directory_wbs", &mut time);
        assert!(app.error.is_none(), "{}: {:?}", path.display(), app.error);
        // Shift the last top-level element up, then check the export.
        let tree = wbs::Tree::build(&app.project);
        let top = wbs::project_node(&app.project);
        if let [.., before, last] = tree.kids(top) {
            let (before, last) = (*before, *last);
            wbs::shift_up(&mut app.project, last).unwrap();
            let text = app.project.updated_document().text();
            let q = ProjectData::from_document(Document::parse(&text).unwrap(), key, "rt".into())
                .unwrap();
            let t = wbs::Tree::build(&q);
            let kids = t.kids(top);
            let at = |id| kids.iter().position(|&k| k == id).unwrap();
            assert!(at(last) < at(before), "{}", path.display());
            assert_eq!(q.wbs.len(), app.project.wbs.len());
        }
        println!(
            "{}: {} activities, {} WBS, {} resources in {:.2}s",
            path.file_name().unwrap().to_string_lossy(),
            app.project.model.activities.len(),
            app.project.wbs.len(),
            resources::resources(&app.project).len(),
            started.elapsed().as_secs_f64()
        );
        drop(app);
        let _ = std::fs::remove_file(db);
        checked += 1;
    }
    assert!(checked > 0);
}

/// Software renders of every view and every details tab in a 1080p window,
/// for review. Images and the rectangles of rendered controls go to the
/// ignored `artifacts/review/views/`. With `OPENXER_VIEWS_REFERENCE=1` it
/// renders corpus project 369 (slow in debug builds); otherwise the
/// starter project with resources.
#[test]
fn render_views_and_details_tabs_for_review() {
    use crate::headless_render::SoftwarePainter;
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/review/views");
    std::fs::create_dir_all(&out).unwrap();
    let corpus = crate::test_corpus::file("369.xer");
    let reference = std::env::var_os("OPENXER_VIEWS_REFERENCE").is_some();
    let project = reference
        .then(|| Document::read(&corpus).ok())
        .flatten()
        .and_then(|doc| {
            let key = doc.projects().first()?.0;
            ProjectData::from_document(doc, key, "369".into()).ok()
        })
        .unwrap_or_else(resources::tests::with_resource);
    let mut project = project;
    if project
        .model
        .activities
        .iter()
        .all(|a| a.early_start.is_none())
    {
        schedule_model(&mut project.model, &project.resources).unwrap();
    }
    let source = project.source_name.clone();
    let path = std::env::temp_dir().join(format!("openxer-views-review-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut store = Store::open(path.clone()).unwrap();
    store.save(&project).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    let mut time = 0.;
    let mut painter = SoftwarePainter::default();
    painter.screen = Some(Vec2::new(1920., 1040.));
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let mut report = serde_json::Map::new();
    let mut shot = |app: &mut App, painter: &mut SoftwarePainter, time: &mut f64, name: String| {
        for _ in 0..2 {
            let _ = painter.frame(app, &ctx, time, vec![], false);
        }
        let (image, triangles) = painter.frame(app, &ctx, time, vec![], true).unwrap();
        assert!(triangles > 300, "{name} draws the view");
        let prefix = if reference { "" } else { "starter-" };
        image.save(out.join(format!("{prefix}{name}.png"))).unwrap();
        report.insert(name, serde_json::json!(app.action_rects.clone()));
    };
    let views: [(&str, &str, &[&str]); 4] = [
        ("projects", "directory_projects", &projects::TABS),
        ("wbs", "directory_wbs", &wbs::TABS),
        ("resources", "directory_resources", &resources::TABS),
        ("assignments", "", &assignments::TABS),
    ];
    for (view, action, tabs) in views {
        if action.is_empty() {
            app.show_view(MainView::Assignments);
        } else {
            painter.click_action(&mut app, &ctx, &mut time, action);
        }
        if view == "projects" {
            // Select the open project's row.
            let row = app
                .workspace_projects()
                .iter()
                .position(|p| p.current)
                .unwrap()
                + 1;
            painter.click_action(&mut app, &ctx, &mut time, &format!("projects_row_{row}"));
        }
        if view == "wbs" {
            painter.click_action(&mut app, &ctx, &mut time, "wbs_row_0");
        }
        shot(&mut app, &mut painter, &mut time, format!("{view}-view"));
        for (i, tab) in tabs.iter().enumerate() {
            painter.click_action(&mut app, &ctx, &mut time, &format!("{view}_tab_{i}"));
            let slug: String = tab
                .to_lowercase()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect();
            shot(
                &mut app,
                &mut painter,
                &mut time,
                format!("{view}-tab-{slug}"),
            );
        }
    }
    assert!(app.error.is_none(), "{:?}", app.error);
    std::fs::write(
        out.join(if reference {
            "rects.json"
        } else {
            "starter-rects.json"
        }),
        serde_json::to_vec_pretty(&serde_json::json!({"source": source, "shots": report})).unwrap(),
    )
    .unwrap();
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// The Projects details tabs paint through the shared tab strip (the
/// Activities page look): a `#646464` divider on both sides of every tab,
/// the tabs abutting divider to divider, and each `projects_tab_{i}`
/// switches to its tab.
#[test]
fn projects_details_tabs_show_dividers_and_switch() {
    use crate::headless_render::SoftwarePainter;
    // Every frame goes through the software painter so it sees the font
    // texture upload.
    let mut project = wbs::tests::with_project_node();
    schedule_model(&mut project.model, &project.resources).unwrap();
    let path = std::env::temp_dir().join(format!("openxer-tab-strip-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut store = Store::open(path.clone()).unwrap();
    store.save(&project).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    let mut time = 0.;
    let mut painter = SoftwarePainter::default();
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "directory_projects");
    let row = app
        .workspace_projects()
        .iter()
        .position(|p| p.current)
        .unwrap()
        + 1;
    painter.click_action(&mut app, &ctx, &mut time, &format!("projects_row_{row}"));
    let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    let frame_color = crate::details::form::FRAME;
    let pixel = |x: f32, y: f32| {
        let p = image.get_pixel(x as u32, y as u32).0;
        Color32::from_rgb(p[0], p[1], p[2])
    };
    let rects: Vec<[f32; 4]> = (0..projects::TABS.len())
        .map(|i| app.action_rects[&format!("projects_tab_{i}")])
        .collect();
    for (i, [l, t, r, b]) in rects.iter().copied().enumerate() {
        // The hit rect runs from the left divider through the right one.
        for x in [l, r - 1.] {
            for y in [t + 3., (t + b) / 2., b - 2.] {
                assert_eq!(
                    pixel(x, y),
                    frame_color,
                    "divider of tab {i} ({}) at ({x}, {y})",
                    projects::TABS[i]
                );
            }
        }
        assert_eq!(pixel((l + r) / 2., t), frame_color, "top line of tab {i}");
        assert_ne!(pixel(l + 2., b - 3.), frame_color, "inside tab {i}");
        if let Some(next) = rects.get(i + 1) {
            assert_eq!(next[0], r - 1., "tab {} starts on tab {i}'s divider", i + 1);
        }
    }
    for i in (0..projects::TABS.len()).rev() {
        painter.click_action(&mut app, &ctx, &mut time, &format!("projects_tab_{i}"));
        assert_eq!(app.views.projects.tab, i, "{}", projects::TABS[i]);
    }
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// Click a field, replace its text and press Enter.
pub(super) fn type_into(
    app: &mut App,
    ctx: &egui::Context,
    action: &str,
    text: &str,
    time: &mut f64,
) {
    click(app, ctx, action, time);
    let key = |key, modifiers| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    frame(
        app,
        ctx,
        vec![
            key(Key::A, egui::Modifiers::COMMAND),
            egui::Event::Text(text.into()),
        ],
        time,
    );
    frame(app, ctx, vec![key(Key::Enter, Default::default())], time);
    frame(app, ctx, vec![], time);
}

fn open_project_tab(app: &mut App, ctx: &egui::Context, tab: &str, time: &mut f64) {
    if app.views.main != MainView::Projects {
        click(app, ctx, "directory_projects", time);
    }
    let row = app
        .workspace_projects()
        .iter()
        .position(|p| p.current)
        .unwrap()
        + 1;
    click_row(app, ctx, "projects", row, time);
    let index = projects::TABS.iter().position(|t| *t == tab).unwrap();
    click(app, ctx, &format!("projects_tab_{index}"), time);
    assert_eq!(app.views.projects.tab, index);
}

#[test]
fn must_finish_by_edits_in_project_dates_feed_f9_save_export_and_undo() {
    let (path, ctx, mut app, mut time) = setup("must-finish", wbs::tests::with_project_node());
    open_project_tab(&mut app, &ctx, "Dates", &mut time);
    let origin = app.project.model.origin_serial;
    let finish = app
        .project
        .model
        .activities
        .iter()
        .filter_map(|a| a.early_finish)
        .max()
        .unwrap();
    let target = finish + 7 * DAY;
    assert_eq!(app.project.model.project.plan_end, None);
    type_into(
        &mut app,
        &ctx,
        "project_must_finish",
        &format::date(origin, Some(target)),
        &mut time,
    );
    assert_eq!(
        app.project.model.project.plan_end,
        Some(target),
        "{}",
        app.status
    );
    assert!(app.dirty);
    // F9 uses it: late dates match a direct run with the same constraint.
    let mut expected = app.project.clone();
    schedule_model(&mut expected.model, &expected.resources).unwrap();
    app.schedule();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(app.project.model.project.plan_end, Some(target));
    let late = |p: &ProjectData| -> Vec<Option<i64>> {
        p.model.activities.iter().map(|a| a.late_finish).collect()
    };
    assert_eq!(late(&app.project), late(&expected));
    let mut unconstrained = app.project.clone();
    unconstrained.model.project.plan_end = None;
    schedule_model(&mut unconstrained.model, &unconstrained.resources).unwrap();
    assert_ne!(
        late(&app.project),
        late(&unconstrained),
        "Must Finish By moves the late dates"
    );
    assert!(app
        .project
        .model
        .activities
        .iter()
        .all(|a| a.total_float.unwrap_or(0) > 0));
    // Save, reopen and export carry it.
    app.save();
    let reopened = app.store.last().unwrap().unwrap();
    assert_eq!(reopened.model.project.plan_end, Some(target));
    let text = reopened.updated_document().text();
    let doc = Document::parse(&text).unwrap();
    let table = doc.table("PROJECT").unwrap();
    assert_eq!(
        openxer_xer::parse_relative(origin, table.value(&table.rows[0], "plan_end_date")).unwrap(),
        Some(target)
    );
    // Undo walks back through the schedule run to the edit.
    app.history_move(&ctx, false);
    app.history_move(&ctx, false);
    assert_eq!(app.project.model.project.plan_end, None);
    app.history_move(&ctx, true);
    assert_eq!(app.project.model.project.plan_end, Some(target));
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn project_settings_edit_project_table_round_trip_and_undo() {
    let (path, ctx, mut app, mut time) = setup("project-settings", wbs::tests::with_project_node());
    let value = |app: &App, field: &str| {
        let t = app.project.document.table("PROJECT").unwrap();
        t.value(&t.rows[0], field).to_string()
    };
    open_project_tab(&mut app, &ctx, "Calculations", &mut time);
    let before = value(&app, "step_complete_flag");
    click(&mut app, &ctx, "project_step_complete", &mut time);
    let after = value(&app, "step_complete_flag");
    assert_ne!(before == "Y", after == "Y", "the check box toggles");
    click(&mut app, &ctx, "project_add_actual", &mut time);
    assert_eq!(value(&app, "add_act_remain_flag"), "Y");
    // The critical definition is edited here as in Schedule Options.
    open_project_tab(&mut app, &ctx, "Settings", &mut time);
    click(&mut app, &ctx, "project_critical_longest", &mut time);
    assert_eq!(value(&app, "critical_path_type"), "CT_DrivPath");
    open_project_tab(&mut app, &ctx, "General", &mut time);
    type_into(
        &mut app,
        &ctx,
        "project_field_proj_url",
        "https://example.test/plan",
        &mut time,
    );
    assert_eq!(value(&app, "proj_url"), "https://example.test/plan");
    open_project_tab(&mut app, &ctx, "Defaults", &mut time);
    type_into(
        &mut app,
        &ctx,
        "project_field_task_code_step",
        "25",
        &mut time,
    );
    assert_eq!(value(&app, "task_code_step"), "25");
    type_into(
        &mut app,
        &ctx,
        "project_field_task_code_step",
        "x",
        &mut time,
    );
    assert_eq!(
        value(&app, "task_code_step"),
        "25",
        "non-numbers are refused"
    );
    app.save();
    let reopened = app.store.last().unwrap().unwrap();
    let doc = Document::parse(&reopened.updated_document().text()).unwrap();
    let t = doc.table("PROJECT").unwrap();
    assert_eq!(t.value(&t.rows[0], "step_complete_flag"), after);
    assert_eq!(t.value(&t.rows[0], "add_act_remain_flag"), "Y");
    assert_eq!(t.value(&t.rows[0], "proj_url"), "https://example.test/plan");
    assert_eq!(t.value(&t.rows[0], "task_code_step"), "25");
    // Undo restores the earlier values, step by step.
    while value(&app, "step_complete_flag") != before && app.history.counts().0 > 0 {
        app.history_move(&ctx, false);
    }
    assert_eq!(value(&app, "step_complete_flag"), before);
    assert_ne!(value(&app, "proj_url"), "https://example.test/plan");
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn every_view_opens_its_reference_default_tab() {
    let (path, ctx, mut app, mut time) = setup("defaults", resources::tests::with_resource());
    click(&mut app, &ctx, "directory_wbs", &mut time);
    assert_eq!(wbs::TABS[app.views.wbs.tab], "Notebook");
    click(&mut app, &ctx, "directory_resources", &mut time);
    assert_eq!(resources::TABS[app.views.resources.tab], "Units & Prices");
    click(&mut app, &ctx, "directory_projects", &mut time);
    assert_eq!(projects::TABS[app.views.projects.tab], "General");
    click_text(&mut app, &ctx, "Project", &mut time);
    click(&mut app, &ctx, "menu_resource_assignments", &mut time);
    assert_eq!(app.views.main, MainView::Assignments);
    assert!(app.action_rects.contains_key("assignments_row_1"));
    assert!(app.action_rects.contains_key("assignments_tab_3"));
    // The Gantt panes draw one bar per dated row.
    click(&mut app, &ctx, "directory_wbs", &mut time);
    assert!(app.action_rects.contains_key("wbs_bar_1"));
    click(&mut app, &ctx, "directory_projects", &mut time);
    assert!(app.action_rects.contains_key("projects_bar_0"));
    assert!(app.error.is_none());
    drop(app);
    let _ = std::fs::remove_file(path);
}
