//! Multi-project (EPS) tracking through the production UI: Add opens the New
//! Project wizard in the selected folder, the rollup columns on a synthetic
//! two-project workspace (values worked out by hand in
//! `openxer_xer::workspace::tests`), Customize Columns, sort, group and
//! filter, the project Gantt with its baseline bar, and Open Project opening
//! a whole branch. Renders go to the gitignored `artifacts/eps-tracking/`.
use super::project_rollups as rollups;
use super::projects::{ProjectSummary, ProjectsState};
use super::*;
use crate::baselines::store::{self as bstore, BaselineRow, Choice, UserBaselines};
use crate::qa_sweep::Driver;
use openxer_xer::workspace::rollup::Rollup;
use openxer_xer::workspace::sample;
use std::path::PathBuf;

fn starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    crate::scheduling::schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn artifacts() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/eps-tracking");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn render(d: &mut Driver, name: &str) {
    d.frames(2);
    d.screenshot(&artifacts().join(name));
}

fn driver(name: &str) -> Driver {
    Driver::probed(
        name,
        starter(),
        egui::Vec2::new(1500., 900.),
        Some(artifacts()),
    )
}

/// Store an XER text as a closed workspace project; returns its row id.
fn insert(d: &Driver, source: &str, key: i64, title: &str, doc: &Document) -> i64 {
    let c = d.app.store.connection();
    c.execute(
        "INSERT INTO projects(source_name,project_key,title,xer_content) VALUES(?1,?2,?3,?4)",
        rusqlite::params![source, key, title, doc.text()],
    )
    .unwrap();
    c.last_insert_rowid()
}

fn add_folder(d: &mut Driver, parent: Option<i64>, name: &str, code: &str) -> i64 {
    let mut id = 0;
    let (name, code) = (name.to_string(), code.to_string());
    let out = std::rc::Rc::new(std::cell::Cell::new(0));
    let keep = out.clone();
    d.app
        .run_workspace(
            Box::new(move |ws| {
                keep.set(ws.add_folder(parent, &name, Some(&code))?);
                Ok(crate::enterprise::Done::Changed(None))
            }),
            0.,
        )
        .unwrap();
    id += out.get();
    id
}

fn place(d: &mut Driver, project: i64, folder: i64) {
    d.app
        .run_workspace(
            Box::new(move |ws| {
                ws.place(project, folder)?;
                Ok(crate::enterprise::Done::Changed(None))
            }),
            0.,
        )
        .unwrap();
}

/// The Driver's starter plus ALPHA and BRAVO (closed) in the top folder
/// EAST, ALPHA with a stored Primary baseline finishing Thu 21-Jan-27 17:00.
fn sample_workspace(name: &str) -> (Driver, i64, i64, i64) {
    let mut d = driver(name);
    let a = insert(&d, "alpha.xer", 1, "ALPHA", &sample::project_a());
    let b = insert(&d, "bravo.xer", 7, "BRAVO", &sample::project_b());
    let east = add_folder(&mut d, None, "East Region", "EAST");
    place(&mut d, a, east);
    place(&mut d, b, east);
    let mut bl = sample::project(50, "ALPHA-BL", "2027-01-04 08:00", "");
    sample::task(
        &mut bl,
        50,
        1,
        &[
            ("status_code", "TK_NotStart"),
            ("early_start_date", "2027-01-04 08:00"),
            ("early_end_date", "2027-01-21 17:00"),
        ],
    );
    let c = d.app.store.connection();
    let id = bstore::put(
        c,
        &BaselineRow {
            parent: a,
            project_key: 50,
            short_name: "ALPHA-BL".into(),
            name: "Alpha baseline".into(),
            xer: bl.text(),
            ..Default::default()
        },
    )
    .unwrap();
    bstore::set_user_baselines(
        c,
        a,
        UserBaselines([Choice::Baseline(id), Choice::None, Choice::None]),
    )
    .unwrap();
    d.click("directory_projects");
    d.frames(3);
    (d, east, a, b)
}

fn summary(d: &mut Driver, id: &str) -> ProjectSummary {
    d.app
        .workspace_projects()
        .into_iter()
        .find(|s| s.id == id)
        .unwrap()
}

#[test]
fn rollup_columns_show_the_hand_computed_values_for_projects_and_folders() {
    let (mut d, _east, _, _) = sample_workspace("eps-rollups");
    d.app.views.projects.show_details = false;
    d.app.views.projects.table_width = 1000.;
    let a = summary(&mut d, "ALPHA");
    let b = summary(&mut d, "BRAVO");
    let day = format::Periods {
        day: 8 * 60,
        ..format::Periods::DEFAULT
    };
    let cells = rollups::cells(&a, false);
    assert_eq!(cells[1], "Alpha Unit Revamp");
    assert_eq!(cells[4], "Active");
    assert_eq!(
        cells[5],
        format::date(0, openxer_xer::parse_absolute("2027-01-04 08:00"))
    );
    assert_eq!(
        cells[6],
        format::date(0, openxer_xer::parse_absolute("2027-01-22 17:00"))
    );
    assert_eq!(
        cells[7],
        format::date(0, openxer_xer::parse_absolute("2027-01-13 08:00"))
    );
    // 56 of 120 working hours: 46.7 %. Variance: one working day late.
    assert_eq!(cells[14], "46.7%");
    assert_eq!(cells[12], format::float(Some(-8 * 60), day));
    assert_eq!(cells[13], format::float(Some(8 * 60), day));
    assert_eq!(cells[18], format::cost(5500.));
    assert_eq!(cells[15], format::units(40.));
    assert_eq!(
        (&cells[21][..], &cells[22][..], &cells[23][..]),
        ("1", "1", "1")
    );
    // The folder: 28.0 %, the latest data date, the lowest float.
    let east = rollups::folder_summary("EAST".into(), "East Region".into(), &[&a, &b]);
    let f = rollups::cells(&east, true);
    assert_eq!(f[14], "28.0%");
    assert_eq!(f[7], cells[7], "the latest data date is ALPHA's");
    assert_eq!(f[13], format::float(Some(-8 * 60), day));
    assert_eq!(f[2], "4");
    // The rows the view draws carry the same text (the default columns).
    let texts: Vec<String> = d.texts().into_iter().map(|(t, _)| t).collect();
    for want in ["46.7%", "28.0%", "Alpha Unit Revamp", "Active", "Planned"] {
        assert!(texts.iter().any(|t| t == want), "{want} is drawn");
    }
    render(&mut d, "projects-rollups.png");
    d.app.views.projects.table_width = 520.;
    render(&mut d, "projects-gantt.png");
    d.app.views.projects.show_details = true;
    // The chart: a bar per project and folder, a baseline bar for ALPHA.
    let lines: Vec<usize> = (0..12)
        .filter(|r| d.has(&format!("projects_bar_{r}")))
        .collect();
    assert!(lines.len() >= 4, "{lines:?}");
    assert_eq!(
        (0..12)
            .filter(|r| d.has(&format!("projects_baseline_{r}")))
            .count(),
        2,
        "ALPHA's baseline bar and its folder's"
    );
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn closed_projects_show_their_stored_values_without_opening() {
    let (mut d, _, a, _) = sample_workspace("eps-closed");
    assert!(!d.app.is_open(a));
    let s = summary(&mut d, "ALPHA");
    assert!(!s.current);
    assert_eq!(s.rollup.completed, 1);
    // A change to the stored text shows on the next look.
    let mut doc = sample::project_a();
    let t = doc.table_mut("TASK");
    t.set(2, "status_code", "TK_Complete".into());
    d.app
        .store
        .connection()
        .execute(
            "UPDATE projects SET xer_content=?1,updated_at='2099-01-01 00:00:00' WHERE id=?2",
            rusqlite::params![doc.text(), a],
        )
        .unwrap();
    assert_eq!(summary(&mut d, "ALPHA").rollup.completed, 2);
}

#[test]
fn customize_sort_group_and_filter_are_kept_with_the_view() {
    let (mut d, _, _, _) = sample_workspace("eps-columns");
    // Customize Columns from the layout menu.
    d.click("projects_layout");
    d.click("projects_layout_customize");
    d.frames(2);
    let dialog = d
        .app
        .detail
        .column_dialog
        .clone()
        .expect("the customizer opens");
    let want: Vec<String> = rollups::default_order()
        .iter()
        .map(|&c| rollups::TITLES[c].to_string())
        .collect();
    assert_eq!(dialog.selected, want);
    assert_eq!(dialog.available.len(), rollups::TITLES.len());
    d.click("detail_columns_selected_2");
    d.click("detail_columns_remove");
    d.click("detail_columns_ok");
    d.frames(2);
    let order = d.app.views.projects.columns.order.clone();
    assert!(!order.contains(&4), "Status was removed: {order:?}");
    // Escape and Cancel change nothing; right-click also offers it.
    let at = d.rect("projects_row_1").center();
    d.press_at(at, egui::PointerButton::Secondary, Default::default());
    // Columns > Customize..., as in the reference.
    let hover = d.rect("projects_menu_columns").center();
    let _ = d.frame(vec![egui::Event::PointerMoved(hover)]);
    d.frames(3);
    d.click("projects_menu_customize");
    assert!(d.app.detail.column_dialog.is_some());
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.detail.column_dialog.is_none());
    assert_eq!(d.app.views.projects.columns.order, order);
    // A header click sorts (ascending, descending, off).
    d.click("projects_col_0");
    assert_eq!(
        d.app.views.projects.sort,
        Some(rollups::Sort {
            field: 0,
            descending: false
        })
    );
    d.click("projects_col_0");
    assert!(d.app.views.projects.sort.unwrap().descending);
    // Projects in EAST in descending Project ID order: BRAVO before ALPHA.
    let texts: Vec<(String, egui::Rect)> = d.texts();
    let y = |t: &str| texts.iter().find(|(x, _)| x == t).unwrap().1.top();
    assert!(y("BRAVO") < y("ALPHA"));
    d.click("projects_col_0");
    assert_eq!(d.app.views.projects.sort, None);
    // Group by status, then hide Planned projects.
    d.app.views.projects.group = rollups::Group::Status;
    d.frames(2);
    let texts: Vec<String> = d.texts().into_iter().map(|(t, _)| t).collect();
    assert!(texts.iter().any(|t| t == "Planned"));
    d.app.views.projects.status_filter = ["WS_Open".to_string()].into();
    d.frames(2);
    let texts: Vec<String> = d.texts().into_iter().map(|(t, _)| t).collect();
    assert!(
        !texts.iter().any(|t| t == "BRAVO"),
        "a Planned project is hidden"
    );
    assert!(texts.iter().any(|t| t == "ALPHA"));
    render(&mut d, "projects-group-by-status.png");
    // All of it is saved with the view.
    d.app.views.projects.sort = Some(rollups::Sort {
        field: 14,
        descending: true,
    });
    let json = d.app.views.json();
    let mut back = Views::default();
    back.restore(&json);
    assert_eq!(back.projects.columns.order, order);
    assert_eq!(back.projects.group, rollups::Group::Status);
    assert_eq!(
        back.projects.status_filter,
        d.app.views.projects.status_filter
    );
    assert_eq!(back.projects.sort, d.app.views.projects.sort);
    // A layout saved with the old four columns keeps them.
    let mut old = ProjectsState::default();
    old.restore(
        &serde_json::json!({"columns": {"order": [0, 1, 2, 3], "widths": [150., 150., 75., 100.]}}),
    );
    assert_eq!(old.columns.order, [0, 1, 2, 3]);
}

#[test]
fn add_opens_the_wizard_in_the_selected_folder_or_the_projects_folder() {
    let mut d = driver("eps-add");
    let east = add_folder(&mut d, None, "East Region", "EAST");
    let before = d.app.store.list().unwrap().len();
    d.click("directory_projects");
    d.frames(2);
    d.app.views.projects.selected = None;
    d.app.views.projects.selected_eps = Some(east);
    d.frames(2);
    d.click("add");
    d.frames(2);
    let w = d
        .app
        .new_project
        .clone()
        .expect("Add opens the New Project dialog");
    assert_eq!(w.eps, east);
    assert_eq!(
        d.app.store.list().unwrap().len(),
        before,
        "nothing added yet"
    );
    assert!(d.has("new_project_rate_type"));
    render(&mut d, "new-project-from-add.png");
    d.click("new_project_cancel");
    assert_eq!(d.app.store.list().unwrap().len(), before);
    // With a project selected, its folder is chosen.
    let open = d.app.workspace_row().unwrap();
    let top = d.app.workspace().node_of(Some(open));
    d.click("projects_row_1");
    d.click("add");
    assert_eq!(d.app.new_project.as_ref().unwrap().eps, top);
    // Finish files the new project in the chosen folder as one undo step.
    {
        let w = d.app.new_project.as_mut().unwrap();
        w.eps = east;
        w.project_id = "EAST-1".into();
        w.planned_start = format::date(
            openxer_xer::workspace::new_project::origin(),
            Some(10 * DAY),
        );
        w.rate_type = "COST_PER_QTY3".into();
    }
    let spec = d
        .app
        .new_project
        .clone()
        .unwrap()
        .check(&d.app.taken_project_ids(), &[top, east])
        .unwrap();
    let steps = d.app.history.steps();
    let id = d.app.create_wizard_project(&spec, 5.).unwrap();
    assert_eq!(d.app.history.steps(), steps + 1, "one undo step");
    assert_eq!(d.app.workspace().node_of(Some(id)), east);
    let p = d.app.store.load(id).unwrap();
    let pid = p.model.project.proj_id;
    assert_eq!(
        crate::records::get(&p.document, "PROJECT", 0, "def_rate_type"),
        "COST_PER_QTY3",
        "project {pid} takes the chosen rate type"
    );
    let ctx = d.ctx.clone();
    d.app.history_move(&ctx, false);
    assert!(d.app.store.load(id).is_err(), "undo removes it");
    d.app.new_project = None;
}

#[test]
fn open_project_opens_a_whole_branch_of_the_tree() {
    let mut d = driver("eps-open");
    let mut ids = vec![];
    for (source, name) in [("one", "ONE"), ("two", "TWO"), ("three", "THREE")] {
        let mut p = openxer_xer::starter();
        p.source_name = source.into();
        p.model.project.name = name.into();
        ids.push(d.app.store.save(&p).unwrap());
    }
    let east = add_folder(&mut d, None, "East Region", "EAST");
    let plant = add_folder(&mut d, Some(east), "Plant 4", "P4");
    place(&mut d, ids[0], east);
    place(&mut d, ids[1], plant);
    d.app.open_project_dialog();
    d.frames(2);
    let dialog = d.app.open_set.open_dialog.clone().unwrap();
    let shown = dialog.visible();
    let row = |code: &str| {
        shown
            .iter()
            .position(|&i| dialog.rows[i].project_id == code)
            .unwrap()
    };
    // Every folder row and project row is there, nested.
    let r_east = row("East Region");
    assert_eq!(dialog.rows[shown[row("Plant 4")]].depth, 1);
    assert_eq!(dialog.rows[shown[row("TWO")]].depth, 2);
    render(&mut d, "open-project-tree.png");
    // Collapse and expand with the +/- box.
    d.click(&format!("open_project_expand_{r_east}"));
    assert!(d
        .app
        .open_set
        .open_dialog
        .as_ref()
        .unwrap()
        .collapsed
        .contains(&east));
    d.click(&format!("open_project_expand_{r_east}"));
    assert!(d
        .app
        .open_set
        .open_dialog
        .as_ref()
        .unwrap()
        .collapsed
        .is_empty());
    // Select the EAST folder: ONE and TWO (in its child folder).
    d.click(&format!("open_project_grid_row_{r_east}"));
    let mut chosen = d.app.open_set.open_dialog.as_ref().unwrap().chosen();
    chosen.sort();
    assert_eq!(chosen, [ids[0], ids[1]]);
    d.click("open_project_open");
    let mut open = d.app.open_ids();
    open.sort();
    assert_eq!(open, [ids[0], ids[1]], "the branch is the open set");
    assert!(d.app.open_set.default.is_some_and(|x| open.contains(&x)));
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn a_schema_2_workspace_rolls_closed_projects_up_from_the_dictionary() {
    let (mut d, _, _, _) = sample_workspace("eps-schema2");
    let before = summary(&mut d, "ALPHA").rollup;
    d.app.store.migrate_dictionary(false).unwrap();
    assert_eq!(d.app.store.schema().unwrap(), 3);
    d.app.views.projects.invalidate();
    let mut after = summary(&mut d, "ALPHA").rollup;
    // The migration may rename a calendar to keep names unique; the work
    // times are what count.
    let cal = |r: &Rollup| r.calendar.clone().map(|c| (c.data, c.day_hours_x100));
    assert_eq!(cal(&after), cal(&before));
    after.calendar = before.calendar.clone();
    assert_eq!(after, before);
    assert_eq!(rollups::percent_text(after.schedule_percent()), "46.7%");
}
