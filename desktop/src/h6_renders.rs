//! Software renders, for review, to the ignored
//! `artifacts/h6/`: the Activity Critical Path toolbar with grouped paths
//! and Select Paths to Show, the Projects view's Copy Project Options and
//! delete confirmation, the WBS Delete All confirmation, the Assignment
//! Codes dictionary and the Codes tab picker, Activity Step Templates with
//! Add from Template, and Customize Steps Columns.
use super::*;
use crate::headless_render::SoftwarePainter;

#[test]
fn render_h6_surfaces() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/h6");
    std::fs::create_dir_all(&out).unwrap();
    let path = std::env::temp_dir().join(format!("openxer-h6-render-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut project = crate::views::resources::tests::with_resource();
    {
        use crate::dictionaries::{ops, CodeKind};
        let doc = &mut project.document;
        let kind = CodeKind::Assignment;
        let t = ops::add_code_type(doc, kind, ops::Scope::Global, 0, Some("Crew Shift")).unwrap();
        let v = ops::add_code_value(doc, kind, t, None).unwrap();
        ops::set_code_value(doc, kind, t, v, ops::ValueField::Short, "DAY").unwrap();
        ops::set_code_value(doc, kind, t, v, ops::ValueField::Name, "Day shift").unwrap();
        let g = crate::dictionaries::steps::add_template(doc, Some("Pipe spool")).unwrap();
        crate::dictionaries::steps::add_template_step(doc, g, "Fit up", 2., "");
        crate::dictionaries::steps::add_template_step(doc, g, "Weld out", 3., "");
    }
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
    // Activity Critical Path: toolbar on, both directions, grouped.
    let last = (0..app.project.model.activities.len())
        .max_by_key(|&i| app.project.model.activities[i].early_finish)
        .unwrap();
    app.selected = Some(last);
    app.views
        .shown_toolbars
        .insert("Activity Critical Path".into());
    app.critical_path.count = 2;
    app.critical_path.group = true;
    app.toggle_critical_paths(crate::critical_path::Direction::Both);
    shot(&mut app, &mut painter, &mut time, "acp-grouped");
    app.open_select_paths();
    shot(&mut app, &mut painter, &mut time, "acp-select-paths");
    app.critical_path.select = None;
    app.hide_critical_paths();
    // Customize Steps Columns.
    app.selected = Some(0);
    app.show_detail_tab(DetailTab::Steps);
    app.open_column_customize(&crate::details::grid_columns::STEPS);
    shot(&mut app, &mut painter, &mut time, "customize-steps-columns");
    app.detail.column_dialog = None;
    // Add from Template on the Steps tab.
    app.dictionaries.template_pick = Some(crate::dictionaries::steps::TemplatePick {
        task_id: app.project.model.activities[0].task_id,
        selected: None,
        ..Default::default()
    });
    shot(&mut app, &mut painter, &mut time, "steps-add-from-template");
    app.dictionaries.template_pick = None;
    // Dictionaries.
    app.open_dictionary(crate::dictionaries::Dictionary::StepTemplates);
    shot(&mut app, &mut painter, &mut time, "step-templates");
    app.open_dictionary(crate::dictionaries::Dictionary::ASSIGNMENT_CODES);
    shot(&mut app, &mut painter, &mut time, "assignment-codes");
    app.dictionaries = Default::default();
    // Resource Assignments > Codes with the picker.
    app.show_view(views::MainView::Assignments);
    let a = crate::views::assignments::assignments(&app.project)[0].id;
    app.views.assignments.selected = Some(crate::views::assignments::Key::Assignment(a));
    app.views.assignments.tab = 3;
    app.dictionaries.assignment_pick = Some(crate::views::assignment_codes::AssignPick {
        assignment: a,
        type_id: None,
        value: None,
    });
    shot(&mut app, &mut painter, &mut time, "assignment-codes-tab");
    app.dictionaries.assignment_pick = None;
    // WBS Delete All confirmation.
    app.show_view(views::MainView::Wbs);
    app.views.wbs.pending = Some(crate::views::wbs::PendingDelete {
        id: 10,
        activities: 2,
        children: 0,
        delete_all: true,
    });
    shot(&mut app, &mut painter, &mut time, "wbs-delete-all");
    app.views.wbs.pending = None;
    // Projects view: a copy and the delete confirmation.
    app.show_view(views::MainView::Projects);
    let source = app.workspace_row().unwrap();
    let eps = app.workspace().node_of(Some(source));
    app.open_copy_options(source, eps);
    shot(&mut app, &mut painter, &mut time, "projects-copy-options");
    app.views.projects.copy_dialog = None;
    let copy = app
        .paste_project_copy(
            source,
            Some(eps),
            crate::views::project_ops::CopyChoice::default(),
            time,
        )
        .unwrap();
    app.request_project_delete(copy);
    shot(&mut app, &mut painter, &mut time, "projects-delete-confirm");
    drop(app);
    let _ = std::fs::remove_file(path);
}
