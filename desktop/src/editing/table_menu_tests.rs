//! The table context menu through the production UI: right-click a row or
//! a column header, click an item, check that its command ran. egui
//! `RawInput` only; no native window or system pointer.
use super::*;
use openxer_engine::model::LinkType;
use std::path::PathBuf;

fn starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn index_of(p: &ProjectData, code: &str) -> usize {
    p.model
        .activities
        .iter()
        .position(|a| a.task_code == code)
        .unwrap_or_else(|| panic!("no activity {code}"))
}

fn frame(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>, time: &mut f64) {
    frame_mod(app, ctx, events, time, Default::default());
}

fn frame_mod(
    app: &mut App,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    time: &mut f64,
    modifiers: egui::Modifiers,
) {
    *time += 0.1;
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500., 900.))),
            time: Some(*time),
            events,
            modifiers,
            focused: true,
            ..Default::default()
        },
        |ctx| app.update_ui(ctx),
    );
}

fn idle(app: &mut App, ctx: &egui::Context, time: &mut f64) {
    frame(app, ctx, vec![], time);
}

fn setup(name: &str) -> (PathBuf, egui::Context, App, f64) {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "openxer-table-menu-{name}-{}-{unique}.db",
        std::process::id()
    ));
    let store = Store::open(path.clone()).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, starter());
    let mut time = 0.;
    for _ in 0..3 {
        idle(&mut app, &ctx, &mut time);
    }
    (path, ctx, app, time)
}

fn press(
    app: &mut App,
    ctx: &egui::Context,
    pos: Pos2,
    button: egui::PointerButton,
    modifiers: egui::Modifiers,
    time: &mut f64,
) {
    idle(app, ctx, time);
    for pressed in [true, false] {
        frame_mod(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers,
                },
            ],
            time,
            modifiers,
        );
    }
    idle(app, ctx, time);
}

fn rect(app: &App, action: &str) -> Rect {
    let [l, t, r, b] = *app
        .action_rects
        .get(action)
        .unwrap_or_else(|| panic!("No rendered control {action:?}"));
    Rect::from_min_max(Pos2::new(l, t), Pos2::new(r, b))
}

/// Click a menu item; the pointer stays over it so submenus stay open.
fn click(app: &mut App, ctx: &egui::Context, action: &str, time: &mut f64) {
    idle(app, ctx, time);
    let pos = rect(app, action).center();
    press(
        app,
        ctx,
        pos,
        egui::PointerButton::Primary,
        Default::default(),
        time,
    );
}

fn hover(app: &mut App, ctx: &egui::Context, action: &str, time: &mut f64) {
    let pos = rect(app, action).center();
    for _ in 0..4 {
        frame(app, ctx, vec![egui::Event::PointerMoved(pos)], time);
    }
}

/// A point in activity `code`'s Activity Name cell.
fn row(app: &App, code: &str) -> Pos2 {
    let task_id = app.project.model.activities[index_of(&app.project, code)].task_id;
    let r = app
        .bar_geometry
        .iter()
        .find(|b| b.task_id == task_id)
        .unwrap()
        .row;
    Pos2::new(
        rect(app, "grid_header_activity_name").center().x,
        r.center().y,
    )
}

fn select(app: &mut App, ctx: &egui::Context, codes: &[&str], time: &mut f64) {
    let shift = egui::Modifiers {
        shift: true,
        ..Default::default()
    };
    for (n, code) in codes.iter().enumerate() {
        let pos = row(app, code);
        let modifiers = if n == 0 { Default::default() } else { shift };
        press(app, ctx, pos, egui::PointerButton::Primary, modifiers, time);
    }
}

fn open_row_menu(app: &mut App, ctx: &egui::Context, code: &str, time: &mut f64) {
    let pos = row(app, code);
    press(
        app,
        ctx,
        pos,
        egui::PointerButton::Secondary,
        Default::default(),
        time,
    );
    assert!(
        app.action_rects.contains_key("ctx_activity_details"),
        "the row menu is open"
    );
}

/// Every enabled item of the 27-entry menu, by action key, with the command
/// it must run.
fn enabled_items() -> Vec<(&'static str, Command)> {
    vec![
        ("activity_details", Command::ActivityDetails),
        ("cut", Command::Cut),
        ("copy", Command::Copy),
        ("paste", Command::Paste),
        ("fill_down", Command::FillDown),
        ("add", Command::Add),
        ("delete", Command::Delete),
        // Dissolve is greyed with two rows selected, as in the reference;
        // qa_workflows::context_menus runs it from a one-row menu.
        ("renumber", Command::Renumber),
        ("export_excel", Command::ExportExcel),
        ("link", Command::Link),
        ("unlink", Command::Unlink),
        ("columns", Command::Columns),
        ("filters", Command::Filters),
        ("group_sort", Command::GroupSort),
        ("collapse_all", Command::CollapseAll),
        ("line_numbers", Command::LineNumbers),
        ("table_font", Command::TableFont),
        ("collapse_to", Command::CollapseTo),
    ]
}

#[test]
fn every_enabled_row_menu_item_runs_its_command() {
    for (key, command) in enabled_items() {
        let (path, ctx, mut app, mut time) = setup(key);
        // Two rows selected and something on the clipboard enable every
        // selection-dependent item.
        select(&mut app, &ctx, &["RB-1010", "RB-1020"], &mut time);
        app.editing.pending = Some(Command::Copy);
        idle(&mut app, &ctx, &mut time);
        app.editing.last = None;
        let activities = app.project.model.activities.len();
        open_row_menu(&mut app, &ctx, "RB-1020", &mut time);
        click(&mut app, &ctx, &format!("ctx_{key}"), &mut time);
        assert_eq!(app.editing.last, Some(command), "ctx_{key}");
        assert!(
            !app.action_rects.contains_key("ctx_activity_details"),
            "ctx_{key} closes the menu"
        );
        match command {
            Command::ActivityDetails => {
                assert!(app.bottom_showing(panes::BottomView::Details))
            }
            Command::Add => assert_eq!(app.project.model.activities.len(), activities + 1),
            // Cut asks nothing and removes nothing: it marks the rows for
            // a move on Paste.
            Command::Cut => {
                assert_eq!(app.project.model.activities.len(), activities);
                assert!(app.delete_confirm.is_none());
                assert!(app.editing.clipboard.as_ref().is_some_and(|c| c.cut));
            }
            Command::Delete => assert_eq!(app.delete_confirm.as_ref().map(Vec::len), Some(2)),
            Command::Columns => assert!(app.grid.dialogs.columns.is_some()),
            Command::Filters => assert!(app.grid.dialogs.filters.is_some()),
            Command::GroupSort => assert!(app.grid.dialogs.group_sort.is_some()),
            Command::CollapseAll => assert!(!app.collapsed.is_empty()),
            Command::LineNumbers => assert!(app.grid.layout.table.line_numbers),
            Command::TableFont => assert!(app.layout_opts.drafts.table.is_some()),
            Command::CollapseTo => assert!(app.layout_opts.drafts.collapse.is_some()),
            _ => {}
        }
        assert!(app.error.is_none(), "ctx_{key}: {:?}", app.error);
        drop(app);
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn header_menu_is_the_same_menu_and_assign_opens_detail_tabs() {
    let (path, ctx, mut app, mut time) = setup("header");
    select(&mut app, &ctx, &["RB-1010"], &mut time);
    // One row: Fill Down, Link and Unlink are disabled; a click does nothing.
    let pos = rect(&app, "grid_header_finish").center();
    press(
        &mut app,
        &ctx,
        pos,
        egui::PointerButton::Secondary,
        Default::default(),
        &mut time,
    );
    assert!(app.action_rects.contains_key("header_menu_cut"));
    click(&mut app, &ctx, "header_menu_link", &mut time);
    assert_eq!(app.editing.last, None, "Link needs two activities");
    // Collapse All, then Expand All (enabled once a band is collapsed).
    click(&mut app, &ctx, "header_menu_collapse_all", &mut time);
    assert_eq!(app.editing.last, Some(Command::CollapseAll));
    assert!(!app.collapsed.is_empty());
    let pos = rect(&app, "grid_header_finish").center();
    press(
        &mut app,
        &ctx,
        pos,
        egui::PointerButton::Secondary,
        Default::default(),
        &mut time,
    );
    click(&mut app, &ctx, "header_menu_expand_all", &mut time);
    assert_eq!(app.editing.last, Some(Command::ExpandAll));
    assert!(app.collapsed.is_empty());
    // Assign > Activity Codes... and Predecessors... open their tabs, and
    // Predecessors... its assign picker at once; Resources... opens the
    // resource picker on the Resources tab.
    for (key, command, tab) in [
        ("codes", Command::Assign(DetailTab::Codes), DetailTab::Codes),
        (
            "predecessors",
            Command::Assign(DetailTab::Predecessors),
            DetailTab::Predecessors,
        ),
        (
            "resources",
            Command::AssignResource(details::assignments::ResourceAssign::Resources),
            DetailTab::Resources,
        ),
    ] {
        open_row_menu(&mut app, &ctx, "RB-1010", &mut time);
        hover(&mut app, &ctx, "ctx_assign", &mut time);
        click(&mut app, &ctx, &format!("ctx_assign_{key}"), &mut time);
        assert_eq!(app.editing.last, Some(command));
        assert_eq!(app.detail.tab, tab);
        assert!(app.bottom_showing(panes::BottomView::Details));
        assert_eq!(
            matches!(
                app.detail.picker,
                Some(details::Picker::Relationship {
                    predecessor: true,
                    ..
                })
            ),
            key == "predecessors"
        );
        app.detail.picker = None;
        app.detail.assign = None;
        idle(&mut app, &ctx, &mut time);
    }
    drop(app);
    let _ = std::fs::remove_file(path);
}

fn fs_ties(app: &App) -> Vec<(String, String)> {
    let a = &app.project.model.activities;
    app.project
        .model
        .links
        .iter()
        .filter(|l| l.link_type == LinkType::FS && l.lag == 0)
        .map(|l| (a[l.pred].task_code.clone(), a[l.succ].task_code.clone()))
        .collect()
}

#[test]
fn link_and_unlink_from_the_menu_are_one_undo_step_each() {
    let (path, ctx, mut app, mut time) = setup("link");
    // Three rows in table order, without ties among them first.
    let order: Vec<String> = app
        .table_activities()
        .iter()
        .map(|&i| app.project.model.activities[i].task_code.clone())
        .collect();
    let picked: Vec<&str> = order[0..3].iter().map(String::as_str).collect();
    let rows: Vec<usize> = picked.iter().map(|c| index_of(&app.project, c)).collect();
    links::unlink_among(&mut app.project.model, &rows).unwrap();
    idle(&mut app, &ctx, &mut time);
    let base = app.project.model.links.clone();
    let (undo_before, _) = app.history.counts();
    // Select the first and third rows with Shift: the range selects all three.
    select(&mut app, &ctx, &[picked[0], picked[2]], &mut time);
    open_row_menu(&mut app, &ctx, picked[1], &mut time);
    click(&mut app, &ctx, "ctx_link", &mut time);
    assert_eq!(app.editing.last, Some(Command::Link));
    let ties = fs_ties(&app);
    for pair in picked.windows(2) {
        assert!(
            ties.contains(&(pair[0].to_string(), pair[1].to_string())),
            "{pair:?} tied finish to start: {ties:?}"
        );
    }
    assert_eq!(app.project.model.links.len(), base.len() + 2);
    let (undo_after_link, _) = app.history.counts();
    assert_eq!(undo_after_link, undo_before + 1, "Link is one undo step");
    app.history_move(&ctx, false);
    assert_eq!(app.project.model.links, base, "undo removes both ties");
    app.history_move(&ctx, true);
    assert_eq!(app.project.model.links.len(), base.len() + 2);
    idle(&mut app, &ctx, &mut time);
    // Unlink removes the ties among the selection only.
    let linked = app.project.model.links.clone();
    open_row_menu(&mut app, &ctx, picked[1], &mut time);
    click(&mut app, &ctx, "ctx_unlink", &mut time);
    assert_eq!(app.editing.last, Some(Command::Unlink));
    assert_eq!(app.project.model.links, base);
    app.history_move(&ctx, false);
    assert_eq!(app.project.model.links, linked, "undo restores the ties");
    // The Edit menu runs the same command.
    app.history_move(&ctx, true);
    idle(&mut app, &ctx, &mut time);
    click(&mut app, &ctx, "menu_bar_edit", &mut time);
    click(&mut app, &ctx, "edit_link", &mut time);
    assert_eq!(app.project.model.links.len(), base.len() + 2);
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// Review render of the table context menu: one row selected, nothing
/// copied, every band expanded. Writes
/// `artifacts/review/chrome/openxer-table-menu.png` (the whole
/// window) and `openxer-table-menu.json` (the menu window's top-left).
#[test]
fn render_table_context_menu_for_review() {
    use crate::headless_render::SoftwarePainter;
    let path = std::env::temp_dir().join(format!(
        "openxer-table-menu-render-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/review/chrome");
    std::fs::create_dir_all(&output).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, Store::open(path.clone()).unwrap(), starter());
    // Every frame goes through the software painter so it sees every
    // texture upload.
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let mut press = |app: &mut App, pos: Pos2, button: egui::PointerButton| {
        for pressed in [true, false] {
            let events = vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers: Default::default(),
                },
            ];
            let _ = painter.frame(app, &ctx, &mut time, events, false);
        }
        let _ = painter.frame(app, &ctx, &mut time, vec![], false);
    };
    let pos = row(&app, "RB-1010");
    press(&mut app, pos, egui::PointerButton::Primary);
    press(&mut app, pos, egui::PointerButton::Secondary);
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
    // The first item starts 3 px inside the menu window (border and frame).
    let first = rect(&app, "ctx_activity_details");
    let last = rect(&app, "ctx_collapse_all");
    let origin = first.min - Vec2::splat(3.);
    // Collapse To... (no action) follows Collapse All.
    let size = Vec2::new(first.width() + 6., last.bottom() + 22. + 3. - origin.y);
    assert_eq!(first.height(), 22.);
    image.save(output.join("openxer-table-menu.png")).unwrap();
    std::fs::write(
        output.join("openxer-table-menu.json"),
        format!(
            "{{\"x\": {}, \"y\": {}, \"w\": {}, \"h\": {}}}",
            origin.x, origin.y, size.x, size.y
        ),
    )
    .unwrap();
    println!("Table menu at {origin:?}, {size:?}");
    // The menu window is 504 px tall: 21 items and 6 separators.
    assert_eq!(size.y, 504.);
    drop(app);
    let _ = std::fs::remove_file(path);
}
