//! Headless click tests of the Available Options header menu in the Columns
//! dialog and in a Customize ... Columns dialog.
use super::*;
use crate::details::grid_columns::PREDECESSORS;
use crate::grid::columns;
use crate::qa_sweep::Driver;
use std::path::PathBuf;

fn driver(name: &str) -> Driver {
    Driver::new(name, crate::reports::test_support::scheduled_starter())
}

/// Open the header menu, pick List or Category through the submenu.
fn pick(d: &mut Driver, header: &str, view: &str) {
    d.click(header);
    assert!(d.has("available_view_group_sort"), "the menu is open");
    assert!(
        d.hover_text("Group and Sort By"),
        "Group and Sort By is drawn"
    );
    assert!(d.has("available_view_list") && d.has("available_view_category"));
    d.click(view);
    assert!(
        !d.has("available_view_group_sort"),
        "choosing closes the menu"
    );
}

/// Ids of the Available Options rows drawn in the Columns dialog, top down.
fn rows(d: &Driver) -> Vec<String> {
    let mut found: Vec<(f32, String)> = d
        .app
        .action_rects
        .iter()
        .filter_map(|(k, r)| {
            let id = k.strip_prefix("columns_available_")?;
            (id != "header").then(|| (r[1], id.to_string()))
        })
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|f| f.1).collect()
}

fn groups(d: &Driver) -> Vec<String> {
    d.app
        .action_rects
        .keys()
        .filter_map(|k| k.strip_prefix("columns_group_").map(str::to_string))
        .collect()
}

fn draft_ids(d: &Driver) -> Vec<String> {
    let draft = d.app.grid.dialogs.columns.as_ref().unwrap();
    draft.columns.iter().map(|c| c.id.clone()).collect()
}

#[test]
fn the_columns_dialog_lists_every_available_column_alphabetically() {
    let mut d = driver("available-list");
    let model = d.app.project.model.clone();
    let undo = d.app.history.counts().0;
    d.app.open_columns_dialog();
    d.frames(3);
    assert!(!groups(&d).is_empty(), "Category is the default");
    assert_eq!(d.app.available_view, AvailableView::Category);
    // Open a group, then switch to the list.
    d.click("columns_group_Durations");
    assert!(d
        .app
        .grid
        .dialogs
        .columns
        .as_ref()
        .unwrap()
        .open_groups
        .contains("Durations"));
    pick(&mut d, "columns_available_header", "available_view_list");
    assert_eq!(d.app.available_view, AvailableView::List);
    d.frames(2);
    assert!(groups(&d).is_empty(), "the list has no group rows");
    // The first rows are the alphabetical start of every column not selected.
    let catalog = columns::catalog(&d.app.project);
    let chosen = draft_ids(&d);
    let mut expected: Vec<&columns::Column> =
        catalog.iter().filter(|c| !chosen.contains(&c.id)).collect();
    expected.sort_by_key(|c| c.title.to_lowercase());
    let drawn = rows(&d);
    assert!(drawn.len() >= 10, "{drawn:?}");
    let want: Vec<&str> = expected.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(drawn[..], want[..drawn.len()]);
    // > moves the picked column, a double-click moves another.
    let (first, second) = (drawn[0].clone(), drawn[1].clone());
    d.click(&format!("columns_available_{first}"));
    d.click("columns_add");
    assert!(draft_ids(&d).contains(&first));
    assert!(!rows(&d).contains(&first), "it left the list");
    let at = d.rect(&format!("columns_available_{second}")).center();
    d.double_click_at(at);
    assert!(draft_ids(&d).contains(&second));
    // >> moves everything left in the list.
    d.click("columns_add_all");
    let all: Vec<String> = catalog.iter().map(|c| c.id.clone()).collect();
    let now = draft_ids(&d);
    assert!(all.iter().all(|id| now.contains(id)));
    assert!(rows(&d).is_empty());
    // Back to Category: the group rows return, Durations still open.
    d.click("columns_remove_all");
    pick(
        &mut d,
        "columns_available_header",
        "available_view_category",
    );
    d.frames(2);
    assert!(groups(&d).contains(&"Durations".to_string()));
    assert!(rows(&d).iter().any(|id| catalog
        .iter()
        .any(|c| &c.id == id && c.group == "Durations")));
    // Cancel leaves the project and the layout alone.
    d.click("columns_cancel");
    assert!(d.app.grid.dialogs.columns.is_none());
    assert!(d.app.project.model == model);
    assert_eq!(d.app.history.counts().0, undo);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn the_choice_is_remembered_between_openings() {
    let mut d = driver("available-remember");
    d.app.open_columns_dialog();
    d.frames(3);
    pick(&mut d, "columns_available_header", "available_view_list");
    d.click("columns_cancel");
    // A fresh read of the workspace gives the same choice.
    d.app.available_view = AvailableView::Category;
    d.app.restore_available_view();
    assert_eq!(d.app.available_view, AvailableView::List);
    d.app.open_columns_dialog();
    d.frames(3);
    assert!(groups(&d).is_empty() && !rows(&d).is_empty());
    // The menu opens again and Escape closes it.
    d.click("columns_available_header");
    assert!(d.hover_text("Group and Sort By"));
    d.frames(2);
    d.key(Key::Escape);
    pick(
        &mut d,
        "columns_available_header",
        "available_view_category",
    );
    assert_eq!(d.app.available_view, AvailableView::Category);
    d.app.available_view = AvailableView::List;
    d.app.restore_available_view();
    assert_eq!(d.app.available_view, AvailableView::Category);
}

#[test]
fn escape_closes_the_menu_before_the_dialog() {
    let mut d = driver("available-escape");
    d.app.open_columns_dialog();
    d.frames(3);
    d.click("columns_available_header");
    assert!(d.has("available_view_group_sort"));
    d.key(Key::Escape);
    assert!(!d.has("available_view_group_sort"), "the menu closed");
    assert!(d.app.grid.dialogs.columns.is_some(), "the dialog stays");
    d.key(Key::Escape);
    assert!(
        d.app.grid.dialogs.columns.is_none(),
        "the next Escape closes it"
    );
    // Closing the dialog with the menu open leaves no menu behind.
    d.app.open_columns_dialog();
    d.frames(3);
    d.click("columns_available_header");
    assert!(menu_open(&d.ctx));
    d.click("columns_cancel");
    assert!(d.app.grid.dialogs.columns.is_none());
    assert!(!menu_open(&d.ctx));
}

#[test]
fn a_customize_columns_dialog_has_the_same_menu() {
    let mut d = driver("available-customize");
    let model = d.app.project.model.clone();
    d.app.open_column_customize(&PREDECESSORS);
    d.frames(3);
    let titles = |d: &Driver| -> Vec<usize> {
        let mut found: Vec<usize> = d
            .app
            .action_rects
            .keys()
            .filter_map(|k| k.strip_prefix("detail_columns_available_")?.parse().ok())
            .collect();
        found.sort();
        found
    };
    // Category keeps the grid's own order: Start first.
    assert_eq!(titles(&d), [0, 1, 2, 3]);
    d.click("detail_columns_available_0");
    assert_eq!(
        d.app.detail.column_dialog.as_ref().unwrap().pick_available,
        Some(0)
    );
    pick(
        &mut d,
        "detail_columns_available_header",
        "available_view_list",
    );
    d.frames(2);
    assert_eq!(d.app.available_view, AvailableView::List);
    // The list is alphabetical: Critical, Driving, Finish, Start.
    let texts = d.texts();
    let drawn: Vec<String> = (0..4)
        .map(|n| {
            let at = d.rect(&format!("detail_columns_available_{n}")).center();
            let (name, _) = texts
                .iter()
                .rev()
                .find(|(_, r)| r.contains(at))
                .expect("a row caption");
            name.clone()
        })
        .collect();
    assert_eq!(drawn, ["Critical", "Driving", "Finish", "Start"]);
    d.click("detail_columns_available_0");
    d.click("detail_columns_add");
    d.click("detail_columns_ok");
    let shown = d.app.detail_columns(&PREDECESSORS, false);
    assert_eq!(shown.last().map(|c| c.0), Some("Critical"));
    assert!(d.app.project.model == model);
    // Escape closes the menu first, the dialog second.
    d.app.open_column_customize(&PREDECESSORS);
    d.frames(3);
    d.click("detail_columns_available_header");
    assert!(d.has("available_view_group_sort"));
    d.key(Key::Escape);
    assert!(!d.has("available_view_group_sort"));
    assert!(d.app.detail.column_dialog.is_some());
    d.key(Key::Escape);
    assert!(d.app.detail.column_dialog.is_none());
    // Back to Category: the grid's own order again.
    d.app.open_column_customize(&PREDECESSORS);
    d.frames(3);
    pick(
        &mut d,
        "detail_columns_available_header",
        "available_view_category",
    );
    assert_eq!(d.app.available_view, AvailableView::Category);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn the_bottom_tables_columns_dialog_follows_the_choice() {
    let mut d = driver("available-bottom");
    d.app.set_available_view(AvailableView::List);
    d.app.open_bottom_columns();
    d.frames(3);
    assert!(d.app.grid.dialogs.columns.as_ref().unwrap().bottom);
    assert!(groups(&d).is_empty() && !rows(&d).is_empty());
    pick(
        &mut d,
        "columns_available_header",
        "available_view_category",
    );
    d.frames(2);
    assert!(!groups(&d).is_empty());
    d.click("columns_cancel");
}

/// Software renders of the menu and of both dialogs in List mode, written
/// to the ignored `artifacts/colopts/`.
#[test]
fn render_available_options() {
    use crate::headless_render::SoftwarePainter;
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/colopts");
    std::fs::create_dir_all(&out).unwrap();
    let dir = std::env::temp_dir().join(format!("openxer-colopts-render-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let project = crate::reports::test_support::scheduled_starter();
    let mut store = Store::open(dir.join("workspace.db")).unwrap();
    store.save(&project).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    struct Rig {
        ctx: egui::Context,
        painter: SoftwarePainter,
        time: f64,
        out: PathBuf,
    }
    impl Rig {
        fn step(&mut self, app: &mut App, events: Vec<egui::Event>) {
            let _ = self
                .painter
                .frame(app, &self.ctx, &mut self.time, events, false);
        }
        fn center(app: &App, action: &str) -> Pos2 {
            let [l, t, r, b] = app.action_rects[action];
            Pos2::new((l + r) / 2., (t + b) / 2.)
        }
        fn click(&mut self, app: &mut App, action: &str) {
            self.step(app, vec![]);
            self.step(app, vec![]);
            let pos = Self::center(app, action);
            for pressed in [true, false] {
                self.step(
                    app,
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
            self.step(app, vec![]);
        }
        fn hover(&mut self, app: &mut App, action: &str) {
            let pos = Self::center(app, action);
            self.step(app, vec![egui::Event::PointerMoved(pos)]);
            self.step(app, vec![]);
            self.step(app, vec![]);
        }
        fn shot(&mut self, app: &mut App, name: &str) {
            self.step(app, vec![]);
            self.step(app, vec![]);
            let (image, triangles) = self
                .painter
                .frame(app, &self.ctx, &mut self.time, vec![], true)
                .unwrap();
            assert!(triangles > 100, "{name}");
            image.save(self.out.join(format!("{name}.png"))).unwrap();
        }
    }
    let mut rig = Rig {
        ctx: ctx.clone(),
        painter: SoftwarePainter::default(),
        time: 0.,
        out,
    };
    // Columns dialog: the open menu with its submenu, then the flat list.
    app.open_columns_dialog();
    rig.click(&mut app, "columns_group_Dates");
    rig.click(&mut app, "columns_available_header");
    rig.hover(&mut app, "available_view_group_sort");
    rig.shot(&mut app, "columns-menu");
    rig.click(&mut app, "available_view_list");
    rig.shot(&mut app, "columns-list");
    app.grid.dialogs.columns = None;
    // Customize Predecessor Columns in List mode with the menu open.
    app.open_column_customize(&PREDECESSORS);
    rig.click(&mut app, "detail_columns_available_header");
    rig.hover(&mut app, "available_view_group_sort");
    rig.shot(&mut app, "customize-menu");
    rig.click(&mut app, "available_view_list");
    rig.shot(&mut app, "customize-list");
    assert!(app.error.is_none(), "{:?}", app.error);
    let _ = std::fs::remove_dir_all(dir);
}
