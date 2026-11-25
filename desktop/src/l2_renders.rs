//! Software renders, for review, to the ignored
//! `artifacts/l2/`: assignment dates on the Resources tab, the Resource
//! Assignments window and the view's Planning tab, Customize Resource
//! Columns, the dictionary dialogs in select mode, Copy Project Options,
//! and the details grids that now show relationship comments, expense cost
//! accounts and risk columns.
use super::*;
use crate::headless_render::SoftwarePainter;
use openxer_engine::model::StatusType;

pub(crate) fn project() -> ProjectData {
    let mut project = crate::views::resources::tests::with_resource();
    let doc = &mut project.document;
    for (id, abbrev, name) in [(1, "ea", "Each"), (2, "hr", "Hour")] {
        doc.table_mut("UMEASURE").add(&[
            ("unit_id", id.to_string()),
            ("unit_abbrev", abbrev.into()),
            ("unit_name", name.into()),
        ]);
    }
    for (id, short, name, rate) in [(1, "USD", "US Dollar", "1"), (2, "EUR", "Euro", "0.92")] {
        doc.table_mut("CURRTYPE").add(&[
            ("curr_id", id.to_string()),
            ("curr_short_name", short.into()),
            ("curr_type", name.into()),
            ("base_exch_rate", rate.into()),
        ]);
    }
    doc.table_mut("ROLES").add(&[
        ("role_id", "4".into()),
        ("role_short_name", "PF".into()),
        ("role_name", "Pipefitter".into()),
    ]);
    doc.table_mut("ACCOUNT").add(&[
        ("acct_id", "3".into()),
        ("acct_short_name", "2100".into()),
        ("acct_name", "Scaffolding".into()),
    ]);
    doc.table_mut("RISKTYPE")
        .add(&[("risk_type_id", "1".into()), ("risk_type", "Safety".into())]);
    // A relationship comment, an expense and a risk on RB-1020.
    let comments = doc.table_mut("TASKPRED");
    if comments.field("comments").is_none() {
        comments.fields.push("comments".into());
        for row in comments.rows.iter_mut() {
            row.push(String::new());
        }
    }
    comments.set(0, "comments", "Crane booked for the lift".into());
    let pid = project.model.project.proj_id.to_string();
    let task = project.model.activities[1].task_id.to_string();
    crate::records::add_row(
        &mut project.document,
        "PROJCOST",
        &[
            ("cost_item_id", "1".into()),
            ("proj_id", pid.clone()),
            ("task_id", task.clone()),
            ("cost_name", "Crane rental".into()),
            ("acct_id", "3".into()),
            ("auto_compute_act_flag", "Y".into()),
            ("target_cost", "1500".into()),
            ("remain_cost", "1500".into()),
        ],
    );
    crate::records::add_row(
        &mut project.document,
        "PROJRISK",
        &[
            ("risk_id", "1".into()),
            ("proj_id", pid.clone()),
            ("risk_code", "R-1".into()),
            ("risk_name", "Crane wind limit".into()),
            ("risk_type_id", "1".into()),
            ("risk_to_type", "RT_Threat".into()),
            ("status_code", "RS_Open".into()),
            ("rsrc_id", "7".into()),
        ],
    );
    crate::records::add_row(
        &mut project.document,
        "TASKRISK",
        &[("task_id", task), ("risk_id", "1".into()), ("proj_id", pid)],
    );
    project
}

#[test]
fn render_l2_surfaces() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/l2");
    std::fs::create_dir_all(&out).unwrap();
    let path = std::env::temp_dir().join(format!("openxer-l2-render-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let project = project();
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
    // Activity 1010 started a day before the data date.
    let i = app
        .project
        .model
        .activities
        .iter()
        .position(|a| a.task_id == 1010)
        .unwrap();
    let dd = app.project.model.project.data_date.unwrap();
    {
        let a = &mut app.project.model.activities[i];
        a.status = StatusType::InProgress;
        a.act_start = Some(dd - 1440);
    }
    crate::scheduling::schedule_model(&mut app.project.model, &app.project.resources).unwrap();
    app.project.write_assignment_schedule();
    app.touch_records();
    app.selected = Some(i);
    // The Resources tab with the date columns and the selected row's editors.
    app.show_detail_tab(DetailTab::Resources);
    app.detail.rows[0] = Some(0);
    app.detail.grid_columns.insert(
        "resources".into(),
        [
            "Resource ID Name",
            "Budgeted Units",
            "Actual Units",
            "Actual Start",
            "Actual Finish",
            "Remaining Start",
            "Remaining Finish",
        ]
        .map(String::from)
        .to_vec(),
    );
    shot(&mut app, &mut painter, &mut time, "resources-tab-dates");
    app.detail.grid_columns.clear();
    app.open_column_customize(&crate::details::grid_columns::RESOURCES);
    shot(
        &mut app,
        &mut painter,
        &mut time,
        "customize-resources-columns",
    );
    app.detail.column_dialog = None;
    // Resource Assignments window.
    app.open_assignment_window();
    let row = app.project.assignment_rows(1010)[0];
    app.detail.assignment_window.as_mut().unwrap().selected = Some(row);
    shot(&mut app, &mut painter, &mut time, "assignment-window");
    app.detail.assignment_window = None;
    // Resource Assignments view, Planning tab.
    app.show_view(views::MainView::Assignments);
    let a = crate::views::assignments::assignments(&app.project)[0].id;
    app.views.assignments.selected = Some(crate::views::assignments::Key::Assignment(a));
    app.views.assignments.tab = 1;
    shot(&mut app, &mut painter, &mut time, "assignments-planning");
    // Dictionary dialogs in select mode, from the Resources view.
    app.show_view(views::MainView::Resources);
    app.views.resources.selected = Some(7);
    app.views.resources.tab = 2;
    for (pick, name) in [
        (crate::views::resources::Pick::Unit, "select-unit"),
        (crate::views::resources::Pick::Currency, "select-currency"),
        (crate::views::resources::Pick::Calendar, "select-calendar"),
        (crate::views::resources::Pick::Role, "select-role"),
    ] {
        app.views.resources.pick = Some((pick, 7));
        shot(&mut app, &mut painter, &mut time, name);
        app.dictionaries = Default::default();
        app.enterprise.ui = Default::default();
        app.calendars = None;
    }
    // Copy Project Options with the new choices.
    app.show_view(views::MainView::Projects);
    let source = app.workspace_row().unwrap();
    let eps = app.workspace().node_of(Some(source));
    app.open_copy_options(source, eps);
    shot(&mut app, &mut painter, &mut time, "projects-copy-options");
    app.views.projects.copy_dialog = None;
    // Details grids: relationship comments, expenses, risks.
    app.show_view(views::MainView::Activities);
    app.selected = Some(1);
    for (tab, name) in [
        (DetailTab::Predecessors, "details-predecessors-comments"),
        (DetailTab::Expenses, "details-expenses-account"),
        (DetailTab::Risks, "details-risks-columns"),
    ] {
        app.show_detail_tab(tab);
        app.detail.rows[0] = Some(0);
        shot(&mut app, &mut painter, &mut time, name);
    }
    drop(app);
    let _ = std::fs::remove_file(path);
}
