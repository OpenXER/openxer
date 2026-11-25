//! Workflow scenarios: whole jobs a scheduler does, driven through the
//! production UI with the headless `qa_sweep::Driver` (synthetic input only,
//! no native window), checking what the views show and what the workspace
//! holds after every step, and that it survives save, reload and an XER
//! round trip.
//!
//! A scenario that found a bug is marked `#[ignore = "BUG n: ..."]`; the
//! numbers match the QA report on branch `feature/qa-workflows`. Run them
//! with `cargo test -p openxer-desktop qa_workflows -- --include-ignored`.
use super::*;
use crate::qa_sweep::Driver;

// Scenarios kept in their own files under `qa_workflows/`.
mod activity_wizard;
mod assignments_window;
mod bar_drag_hold;
mod chat;
mod confirm_defaults;
mod context_menus;
mod cut_paste;
mod data_date_edit;
mod delete_confirms;
mod details_gaps;
mod dialog_look;
mod dissolve;
mod eps_delete;
mod global_change_cli;
mod global_change_report;
mod look_details_project;
mod look_layouts_records;
mod notebooks_project_codes;
mod out_of_sequence;
mod owner_ui_fixes_1;
mod period_delete;
mod polish_1;
mod profiles;
mod progress_tools;
mod project_numbers;
mod reference_defaults;
mod resource_wizard;
mod round2_corrections;
mod round3_corrections;
mod schedule_freeze;
pub(crate) mod schedule_options_close;
mod shared_controls;
mod start_constraint;
mod table_editing;
mod udf_columns;
mod usage_cells;
mod usage_spreadsheet;
mod wbs_copy_calendars;
mod wbs_projects_tables;
mod xml_interchange;

fn ctrl() -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    }
}

/// A first start: the app on its scheduled sample project, nothing else in
/// the workspace.
fn fresh(name: &str) -> Driver {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    Driver::new(name, p)
}

/// Click a text field, select what it holds and type `text` over it.
fn type_into(d: &mut Driver, action: &str, text: &str) {
    d.click(action);
    d.key_with(Key::A, ctrl());
    d.type_text(text);
    d.frames(1);
}

/// File > New through the wizard; returns the new project's workspace id.
fn new_project(d: &mut Driver, id: &str, name: &str, start: &str, finish: &str) -> i64 {
    new_project_in(d, None, id, name, start, finish)
}

/// File > New with the EPS folder `eps` (its name as the drop-down shows
/// it) picked in the wizard.
fn new_project_in(
    d: &mut Driver,
    eps: Option<&str>,
    id: &str,
    name: &str,
    start: &str,
    finish: &str,
) -> i64 {
    d.home();
    d.key_with(Key::N, ctrl());
    d.frames(3);
    assert!(d.app.new_project.is_some(), "Ctrl+N opens New Project");
    if let Some(eps) = eps {
        d.click("new_project_eps");
        assert!(d.click_text(eps), "EPS {eps:?} is offered");
    }
    type_into(d, "new_project_id", id);
    type_into(d, "new_project_name", name);
    type_into(d, "new_project_start", start);
    type_into(d, "new_project_finish_by", finish);
    d.click("new_project_finish");
    d.frames(3);
    if let Some(w) = &d.app.new_project {
        panic!("New Project did not finish: {:?}", w.error);
    }
    d.assert_clean("new project");
    assert_eq!(
        d.app.project.model.project.name, id,
        "the new project is open"
    );
    d.app
        .open_set
        .active
        .expect("the new project has a workspace row")
}

/// Undo every step made in a project File > New created, back to the bare
/// new project: all but the first step, which would remove the project
/// (see `bug_5_new_project_can_be_undone`). Returns the steps undone.
fn undo_to_new_project(d: &mut Driver) -> usize {
    let mut n = 0;
    while d.app.history.counts().0 > 1 {
        d.click("undo");
        n += 1;
        assert!(n <= 70, "undo did not empty the history");
    }
    n
}

/// Where undo left the app, for a failure message: the open project, its
/// workspace row, the undo and redo counts, the steps the history has seen,
/// the open set and the status line.
fn undo_state(d: &Driver) -> String {
    format!(
        "project {} (row {:?}), undo/redo {:?}, steps {}, open {:?}, status {:?}, error {:?}",
        d.app.project.model.project.name,
        d.app.open_set.active,
        d.app.history.counts(),
        d.app.history.steps(),
        d.app.open_ids(),
        d.app.status,
        d.app.error,
    )
}

fn show(d: &mut Driver, view: &str) {
    d.key(Key::Escape);
    d.click(&format!("directory_{view}"));
    d.frames(3);
}

/// The WBS view's rows as (code, name), as the table lists them.
fn wbs_rows(d: &Driver) -> Vec<(String, String)> {
    let p = &d.app.project;
    let codes = views::wbs::full_codes(p, &views::wbs::Tree::build(p));
    let project = views::wbs::project_node(p);
    let mut out = vec![];
    if project.is_none() {
        out.push((p.model.project.name.clone(), views::wbs::project_name(p)));
    }
    for w in &p.wbs {
        let code = if Some(w.id) == project {
            p.model.project.name.clone()
        } else {
            codes.get(&w.id).cloned().unwrap_or_default()
        };
        out.push((code, w.name.clone()));
    }
    out
}

/// The table row of WBS element `id` (the WBS list is kept in tree order).
fn wbs_row(d: &Driver, id: i64) -> usize {
    let offset = usize::from(views::wbs::project_node(&d.app.project).is_none());
    offset
        + d.app
            .project
            .wbs
            .iter()
            .position(|w| w.id == id)
            .unwrap_or_else(|| panic!("WBS {id} is listed"))
}

fn wbs_click(d: &mut Driver, id: i64) {
    let row = wbs_row(d, id);
    d.click(&format!("wbs_row_{row}"));
    assert_eq!(
        d.app.views.wbs.selected,
        Some(id),
        "row {row} selects WBS {id}"
    );
}

/// WBS Details, General tab: type into a field and press Enter.
fn wbs_field(d: &mut Driver, action: &str, text: &str) {
    d.click("wbs_tab_0");
    type_into(d, action, text);
    d.key(Key::Enter);
    d.frames(2);
}

fn index_of(p: &ProjectData, code: &str) -> usize {
    p.model
        .activities
        .iter()
        .position(|a| a.task_code == code)
        .unwrap_or_else(|| panic!("activity {code}"))
}

/// The center of activity `i`'s cell in column `id`, from the rendered
/// header and the table's own row offsets (a new activity has no bar yet).
fn row_cell(d: &mut Driver, i: usize, id: &str) -> Pos2 {
    d.frames(1);
    let rows = d.app.rows();
    let k = rows
        .iter()
        .position(|r| *r == grid::Row::Activity(i))
        .expect("the activity is listed");
    let (top, height) = d.app.grid_row_tops(&rows)[k];
    Pos2::new(
        d.rect(&format!("grid_header_{id}")).center().x,
        top + height / 2.,
    )
}

/// Click activity row `i` in the table.
fn select_row(d: &mut Driver, i: usize) {
    let pos = row_cell(d, i, "activity_id");
    d.click_at(pos);
    assert_eq!(d.app.selected, Some(i), "row click selects");
    if d.app.edit_cell.is_some() {
        d.key(Key::Escape);
    }
}

/// Edit a cell of the activity table the way a user does: double-click,
/// select all, type, Enter.
fn cell_edit(d: &mut Driver, i: usize, column: &str, text: &str) {
    select_row(d, i);
    let pos = row_cell(d, i, column);
    d.double_click_at(pos);
    assert_eq!(
        d.app.edit_cell,
        Some((i, column.to_string())),
        "{column} editor opens: {}",
        d.app.status
    );
    d.key_with(Key::A, ctrl());
    d.type_text(text);
    d.key(Key::Enter);
    d.frames(1);
    d.assert_clean(column);
}

/// Edit > Add: a new activity, named and given a duration in the table.
fn add_activity(d: &mut Driver, name: &str, duration: &str) -> usize {
    d.home();
    let before = d.app.project.model.activities.len();
    // The Edit toolbar's Add.
    d.click("add");
    d.frames(2);
    assert_eq!(
        d.app.project.model.activities.len(),
        before + 1,
        "Add adds one"
    );
    let i = d.app.selected.expect("the new activity is selected");
    cell_edit(d, i, "activity_name", name);
    cell_edit(d, i, "remaining_duration", duration);
    i
}

/// Select two activity rows (click, then Ctrl+click) and Edit > Link
/// Activities: a finish-to-start relationship from the first to the second.
fn link(d: &mut Driver, pred: usize, succ: usize) {
    d.home();
    select_row(d, pred);
    let pos = row_cell(d, succ, "activity_id");
    d.press_at(pos, egui::PointerButton::Primary, ctrl());
    d.frames(1);
    let links = d.app.project.model.links.len();
    d.click("menu_bar_edit");
    d.click("edit_link");
    d.frames(2);
    d.assert_clean("link");
    assert_eq!(
        d.app.project.model.links.len(),
        links + 1,
        "Link Activities adds one relationship: {}",
        d.app.status
    );
}

fn date(p: &ProjectData, minutes: Option<i64>) -> String {
    date_string(p.model.origin_serial, minutes)
}

/// The workspace copy of project `id` as the store holds it.
fn stored(d: &Driver, id: i64) -> ProjectData {
    d.app.store.load(id).unwrap()
}

/// Export the open project through File > Export and read it back.
fn xer_round_trip(d: &mut Driver, name: &str) -> ProjectData {
    let path = d.dir.join(name);
    d.export(interchange::ExportFormat::Xer, &path);
    let doc = Document::read(&path).unwrap();
    let key = d.app.project.model.project.proj_id;
    ProjectData::from_document(doc, key, "round trip".into()).unwrap()
}

/// In the WBS view: select `parent`, Add, then give the new element its code
/// and name in WBS Details. Returns its id.
fn add_wbs(d: &mut Driver, parent: i64, code: &str, name: &str) -> i64 {
    show(d, "wbs");
    wbs_click(d, parent);
    d.click("add");
    d.frames(2);
    let id = d
        .app
        .views
        .wbs
        .selected
        .expect("the new element is selected");
    wbs_field(d, "wbs_code_edit", code);
    wbs_field(d, "wbs_name_edit", name);
    d.assert_clean("add wbs");
    assert_eq!(
        d.app
            .project
            .wbs
            .iter()
            .find(|w| w.id == id)
            .map(|w| w.code.as_str()),
        Some(code)
    );
    id
}

/// Quit and start again on the same workspace, the way `main` starts: the
/// last saved project, the open set and the file baselines restored.
fn restart(d: &mut Driver) {
    eframe::App::on_exit(&mut d.app, None);
    let path = d.app.store.path.clone();
    let store = Store::open(path).unwrap();
    let project = store.last().unwrap().expect("a saved project");
    let mut app = App::new(&d.ctx, store, project);
    app.adopt_file_baselines();
    app.restore_open_set();
    app.note_loops_on_open();
    app.ai_agents.env = Some(crate::ai_agents::temp_profile(&d.dir));
    app.sched.check_report_path = d.dir.join("check.html").display().to_string();
    app.sched.log_path = d.dir.join("SCHEDLOG.TXT").display().to_string();
    d.app = app;
    d.frames(3);
    d.assert_clean("restart");
}

/// The project node of the open project.
fn root(d: &Driver) -> i64 {
    views::wbs::project_node(&d.app.project).expect("a project node")
}

/// Click the Projects view row of workspace project `id`.
fn projects_select(d: &mut Driver, id: i64) {
    show(d, "projects");
    let (source, key, _) = d.app.store.source(id).unwrap();
    for k in 0..40 {
        let row = format!("projects_row_{k}");
        if !d.has(&row) {
            break;
        }
        d.click(&row);
        if d.app.views.projects.selected == Some((source.clone(), key)) {
            return;
        }
    }
    panic!("project {id} is not listed in the Projects view");
}

/// File > Open with exactly these workspace projects picked (click, then
/// Ctrl+click), then Open.
fn open_projects(d: &mut Driver, ids: &[i64]) {
    d.home();
    d.click("menu_bar_file");
    d.click("menu_open");
    d.frames(2);
    let rows = d
        .app
        .open_set
        .open_dialog
        .as_ref()
        .expect("Open Project")
        .rows
        .clone();
    for (n, id) in ids.iter().enumerate() {
        let r = rows
            .iter()
            .position(|r| r.id == Some(*id))
            .unwrap_or_else(|| panic!("project {id} is offered"));
        let pos = d.rect(&format!("open_project_grid_row_{r}")).center();
        let modifiers = if n == 0 { Default::default() } else { ctrl() };
        d.press_at(pos, egui::PointerButton::Primary, modifiers);
    }
    let chosen = d.app.open_set.open_dialog.as_ref().unwrap().chosen();
    assert_eq!(chosen.len(), ids.len(), "picked {chosen:?}");
    d.click("open_project_open");
    d.frames(3);
    d.assert_clean("open projects");
    let mut open = d.app.open_ids();
    open.sort();
    let mut want = ids.to_vec();
    want.sort();
    assert_eq!(open, want, "exactly the picked projects are open");
}

/// Every open project, by workspace id.
fn open_project(d: &Driver, id: i64) -> &ProjectData {
    d.app
        .open_projects()
        .into_iter()
        .zip(d.app.open_ids())
        .find(|(_, k)| *k == id)
        .map(|(p, _)| p)
        .unwrap_or_else(|| panic!("project {id} is open"))
}

fn wbs_codes(p: &ProjectData) -> Vec<String> {
    p.wbs.iter().map(|w| w.code.clone()).collect()
}

/// WBS elements, activities, calendars, codes and resources of one project.
#[derive(Debug, PartialEq)]
struct Contents {
    wbs: Vec<String>,
    activities: Vec<String>,
    calendars: Vec<String>,
    codes: Vec<String>,
    resources: Vec<String>,
}

fn contents(p: &ProjectData) -> Contents {
    let column = |table: &str, field: &str| -> Vec<String> {
        let mut v: Vec<String> = p
            .document
            .table(table)
            .map(|t| {
                t.rows
                    .iter()
                    .map(|r| t.value(r, field).to_string())
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    };
    let mut activities: Vec<String> = p
        .model
        .activities
        .iter()
        .map(|a| a.task_code.clone())
        .collect();
    activities.sort();
    Contents {
        wbs: wbs_codes(p),
        activities,
        calendars: p.calendar_names.clone(),
        codes: column("ACTVTYPE", "actv_code_type"),
        resources: column("RSRC", "rsrc_short_name"),
    }
}

// ------------------------------------------------------------------ 1

/// Workflow 1: from nothing to a scheduled project. A project folder, a new
/// project, a calendar, a WBS built, renamed, moved and pruned, activities
/// with durations and logic, F9, and the dates checked.
#[test]
fn workflow_1_set_up_a_project_from_nothing() {
    let mut d = fresh("wf1");
    // Enterprise > Project Folders (EPS): add a folder under the root.
    d.home();
    d.click("menu_bar_enterprise");
    d.click("menu_eps");
    d.frames(2);
    d.click("eps_add");
    d.frames(2);
    d.click("eps_list_cell_1_1");
    d.frames(1);
    for _ in 0..30 {
        d.key(Key::Backspace);
    }
    d.type_text("North Plant");
    d.key(Key::Enter);
    d.frames(2);
    d.click("eps_close");
    let nodes = d.app.workspace().eps();
    assert_eq!(nodes.len(), 2, "the folder is added");
    let folder = nodes[1].id;
    assert_eq!(
        d.app.workspace().eps_value(folder, "eps_name"),
        "North Plant"
    );

    // File > New into that folder.
    let id = new_project_in(
        &mut d,
        Some("    North Plant"),
        "NP-100",
        "North Plant Tie-ins",
        "04-Jan-27",
        "",
    );
    assert_eq!(d.app.workspace().node_of(Some(id)), folder);
    assert!(d.app.project.model.activities.is_empty());
    show(&mut d, "wbs");
    assert_eq!(
        wbs_rows(&d),
        vec![("NP-100".to_string(), "North Plant Tie-ins".to_string())],
        "a new project's WBS is its project node alone"
    );

    // WBS: three elements under the project node.
    let root = d.app.project.wbs[0].id;
    let undo = d.app.history.counts().0;
    for _ in 0..3 {
        wbs_click(&mut d, root);
        d.click("add");
        d.frames(2);
    }
    assert_eq!(d.app.project.wbs.len(), 4, "{:?}", wbs_rows(&d));
    assert_eq!(d.app.history.counts().0, undo + 3, "each Add is one step");
    // Rename them in WBS Details.
    let added: Vec<i64> = d.app.project.wbs[1..].iter().map(|w| w.id).collect();
    for (id, (code, name)) in added.iter().zip([
        ("DEMO", "Demolition"),
        ("PIPE", "Piping"),
        ("ELEC", "Electrical"),
    ]) {
        wbs_click(&mut d, *id);
        wbs_field(&mut d, "wbs_code_edit", code);
        wbs_field(&mut d, "wbs_name_edit", name);
        d.assert_clean("wbs rename");
    }
    let names: Vec<_> = wbs_rows(&d).into_iter().skip(1).collect();
    assert_eq!(
        names,
        [
            ("NP-100.DEMO".to_string(), "Demolition".to_string()),
            ("NP-100.PIPE".into(), "Piping".into()),
            ("NP-100.ELEC".into(), "Electrical".into())
        ],
        "renamed in the table"
    );
    // Indent ELEC under PIPE, then move it back out and up above PIPE.
    let (demo, pipe, elec) = (added[0], added[1], added[2]);
    wbs_click(&mut d, elec);
    d.click("dock_move_right");
    let parent = |d: &Driver, id: i64| {
        d.app
            .project
            .wbs
            .iter()
            .find(|w| w.id == id)
            .unwrap()
            .parent
    };
    assert_eq!(parent(&d, elec), Some(pipe), "Shift right indents");
    assert_eq!(wbs_rows(&d)[3].0, "NP-100.PIPE.ELEC", "{:?}", wbs_rows(&d));
    d.click("dock_move_left");
    assert_eq!(parent(&d, elec), Some(root), "Shift left outdents");
    d.click("dock_move_up");
    assert_eq!(
        wbs_rows(&d)
            .iter()
            .skip(1)
            .map(|r| r.1.as_str())
            .collect::<Vec<_>>(),
        ["Demolition", "Electrical", "Piping"]
    );
    // A spare element added and deleted.
    wbs_click(&mut d, root);
    d.click("add");
    let spare = d.app.views.wbs.selected.unwrap();
    d.click("dock_delete");
    d.click("wbs_confirm_merge");
    d.frames(2);
    assert!(d.app.project.wbs.iter().all(|w| w.id != spare), "deleted");
    assert_eq!(d.app.project.wbs.len(), 4);

    // Activities: demo before install, with logic, in their WBS.
    show(&mut d, "activities");
    d.app.wbs_filter = None;
    let mut made = vec![];
    for (wbs, name, dur) in [
        (demo, "Demo existing 6in line", "3d"),
        (pipe, "Install new 6in line", "5d"),
        (pipe, "Hydrotest 6in line", "1d"),
        (elec, "Pull power cable to pump", "2d"),
    ] {
        // A selected activity in the element gives the new one its WBS;
        // the first goes in through Activity Details.
        let i = add_activity(&mut d, name, dur);
        let task = d.app.project.model.activities[i].task_id;
        d.app
            .edit_activity(i, crate::details::setters::Edit::Wbs(wbs))
            .unwrap();
        d.frames(2);
        made.push(task);
    }
    let at = |d: &Driver, task: i64| {
        d.app
            .project
            .model
            .activities
            .iter()
            .position(|a| a.task_id == task)
            .unwrap()
    };
    for (p, s) in [(0, 1), (1, 2), (0, 3)] {
        let (p, s) = (at(&d, made[p]), at(&d, made[s]));
        link(&mut d, p, s);
    }
    // Enterprise > Calendars: a new calendar with a holiday on Tuesday 5
    // January, given to the demolition activity.
    d.home();
    d.click("tool_calendars");
    d.frames(2);
    assert!(d.app.calendars.is_some(), "Calendars opens");
    d.click("calendars_add");
    d.click("calendars_copy_select");
    d.frames(2);
    assert_eq!(
        d.app.project.model.calendars.len(),
        2,
        "a calendar is added"
    );
    d.click("calendars_modify");
    assert!(d.app.calendars.as_ref().unwrap().editor.is_some());
    d.click("calendar_day_2027-01-05");
    d.click("calendar_nonwork");
    d.click("calendar_ok");
    d.click("calendars_close");
    d.assert_clean("calendars");
    let added_cal = d.app.project.model.calendars.len() - 1;
    let demo_at = at(&d, made[0]);
    d.app
        .edit_activity(demo_at, crate::details::setters::Edit::Calendar(added_cal))
        .unwrap();
    // F9.
    d.f9();
    let p = &d.app.project.clone();
    let a = |task| {
        &p.model.activities[p
            .model
            .activities
            .iter()
            .position(|a| a.task_id == task)
            .unwrap()]
    };
    assert_eq!(date(p, a(made[0]).early_start), "2027-01-04 08:00");
    // The holiday holds demolition one day.
    assert_eq!(date(p, a(made[0]).early_finish), "2027-01-07 17:00");
    assert_eq!(date(p, a(made[1]).early_start), "2027-01-08 08:00");
    assert_eq!(date(p, a(made[2]).early_finish), "2027-01-15 17:00");
    assert!(a(made[3]).total_float.unwrap() > 0, "the cable has float");
    // The table shows those dates.
    let finish = d.app.project.model.activities.len();
    assert_eq!(finish, 4);
    let texts: Vec<String> = d.texts().into_iter().map(|(t, _)| t).collect();
    assert!(
        texts.iter().any(|t| t.contains("15-Jan-27")),
        "the hydrotest finish is drawn: {:?}",
        texts
            .iter()
            .filter(|t| t.contains("Jan"))
            .collect::<Vec<_>>()
    );

    // Save, reload, and the XER round trip keep it all.
    d.key_with(Key::S, ctrl());
    let back = stored(&d, id);
    assert_eq!(back.model.activities.len(), 4);
    assert_eq!(back.model.links.len(), 3);
    assert_eq!(back.wbs.len(), 4, "{:?}", back.wbs);
    let xer = xer_round_trip(&mut d, "wf1.xer");
    assert_eq!(xer.model.activities.len(), 4);
    assert_eq!(xer.wbs.len(), 4, "{:?}", xer.wbs);
    assert_eq!(xer.model.calendars.len(), 2, "the new calendar is exported");
    for task in &made {
        let x = &xer.model.activities[index_of(&xer, &a(*task).task_code)];
        assert_eq!(x.early_finish, a(*task).early_finish);
        assert_eq!(xer.details[&x.task_id].wbs, p.details[task].wbs);
    }
    let mut again = xer.clone();
    schedule_model(&mut again.model, &again.resources).unwrap();
    for (x, y) in again.model.activities.iter().zip(&xer.model.activities) {
        assert_eq!(x.early_finish, y.early_finish, "F9 after import agrees");
    }

    // Delete a WBS element that holds activities: Delete All takes them
    // along, and one undo brings both back.
    show(&mut d, "wbs");
    let before = d.app.project.clone();
    wbs_click(&mut d, pipe);
    d.click("dock_delete");
    d.click("wbs_confirm_delete_all");
    d.click("wbs_confirm_delete_all_yes");
    d.frames(2);
    assert_eq!(
        d.app.project.model.activities.len(),
        2,
        "two piping activities go"
    );
    assert_eq!(d.app.project.model.links.len(), 1);
    d.key_with(Key::Z, ctrl());
    d.frames(2);
    assert!(
        d.app.project.model == before.model,
        "undo restores the network"
    );
    assert_eq!(d.app.project.wbs, before.wbs, "undo restores the WBS");
}

// ------------------------------------------------------------------ 2

/// Two new projects next to the sample one: build each, open one, then
/// both, switch the default project, add to each, close one and delete it.
/// Each project's WBS, activities, calendars, codes and resources stay its
/// own.
#[test]
fn workflow_2_two_projects_stay_apart() {
    let mut d = fresh("wf2");
    let sample = d.app.open_set.active.unwrap();
    let sample_contents = contents(&d.app.project);

    let a = new_project(&mut d, "AA-100", "Alpha Unit", "04-Jan-27", "");
    assert_eq!(
        wbs_codes(&d.app.project),
        ["AA-100"],
        "A starts with its node"
    );
    let node = root(&d);
    let area_a = add_wbs(&mut d, node, "AREA-A", "Alpha Area");
    show(&mut d, "activities");
    let i = add_activity(&mut d, "Alpha first job", "2d");
    d.app
        .edit_activity(i, crate::details::setters::Edit::Wbs(area_a))
        .unwrap();
    add_activity(&mut d, "Alpha second job", "3d");
    d.key_with(Key::S, ctrl());
    let a_contents = contents(&d.app.project);

    // B is made while A is open: it must not start with A's WBS.
    let b = new_project(&mut d, "BB-200", "Bravo Unit", "11-Jan-27", "");
    assert_eq!(
        wbs_codes(&d.app.project),
        ["BB-200"],
        "B starts with its own node only"
    );
    let node = root(&d);
    let area_b = add_wbs(&mut d, node, "AREA-B", "Bravo Area");
    show(&mut d, "activities");
    let i = add_activity(&mut d, "Bravo only job", "4d");
    d.app
        .edit_activity(i, crate::details::setters::Edit::Wbs(area_b))
        .unwrap();
    // A calendar of B's own: a project calendar (a global one is shared by
    // every project, workflow_5b).
    d.home();
    d.click("tool_calendars");
    d.click("calendars_project");
    d.click("calendars_add");
    d.click("calendars_copy_select");
    d.click("calendars_close");
    d.key_with(Key::S, ctrl());
    let b_contents = contents(&d.app.project);
    assert_eq!(b_contents.calendars.len(), a_contents.calendars.len() + 1);
    assert_eq!(
        contents(&stored(&d, a)),
        a_contents,
        "A is untouched by B's work"
    );
    assert_eq!(contents(&stored(&d, sample)), sample_contents);

    // Open A alone, then A and B together.
    open_projects(&mut d, &[a]);
    assert_eq!(d.app.project.model.project.name, "AA-100");
    assert_eq!(contents(&d.app.project), a_contents);
    open_projects(&mut d, &[a, b]);
    assert_eq!(
        d.app.shown().model.activities.len(),
        3,
        "both projects listed"
    );
    assert_eq!(contents(open_project(&d, a)), a_contents);
    assert_eq!(contents(open_project(&d, b)), b_contents);
    // The WBS view shows the current project's WBS.
    show(&mut d, "wbs");
    let current = d.app.open_set.active.unwrap();
    let want = if current == a {
        &a_contents
    } else {
        &b_contents
    };
    assert_eq!(wbs_codes(&d.app.project), want.wbs);

    // Project > Set Default Project: B. A new activity goes to B, in B's WBS.
    d.home();
    d.click("menu_bar_project");
    d.click("menu_set_default_project");
    d.frames(2);
    let rows = d.app.open_set.default_dialog.as_ref().unwrap().rows.clone();
    let r = rows.iter().position(|r| r.0 == b).unwrap();
    d.click(&format!("default_project_grid_row_{r}"));
    d.click("default_project_ok");
    assert_eq!(d.app.open_set.default, Some(b));
    show(&mut d, "activities");
    // Start from a row of A, as a user looking at A would.
    if d.app.open_set.active != Some(a) {
        d.app.activate_id(a);
    }
    d.home();
    d.click("add");
    d.frames(2);
    assert_eq!(
        d.app.open_set.active,
        Some(b),
        "the default project gets it"
    );
    let added = d.app.selected.unwrap();
    let task = d.app.project.model.activities[added].task_id;
    let wbs = d.app.project.details[&task].wbs;
    assert!(
        d.app.project.wbs.iter().any(|w| w.id == wbs),
        "the new activity's WBS {wbs} is one of B's: {:?}",
        wbs_codes(&d.app.project)
    );
    assert_eq!(open_project(&d, a).model.activities.len(), 2, "A unchanged");
    assert_eq!(open_project(&d, b).model.activities.len(), 2);

    // Undo the add, redo it; A never moves. Escape first closes the new
    // activity's name editor, which holds the keys.
    d.key(Key::Escape);
    d.key_with(Key::Z, ctrl());
    assert_eq!(open_project(&d, b).model.activities.len(), 1);
    d.key_with(Key::Y, ctrl());
    assert_eq!(open_project(&d, b).model.activities.len(), 2);
    assert_eq!(contents(open_project(&d, a)), a_contents);

    // Save; each project reloads as itself.
    d.key_with(Key::S, ctrl());
    assert!(!d.app.any_dirty());
    let sa = stored(&d, a);
    let sb = stored(&d, b);
    assert_eq!(contents(&sa), a_contents);
    assert_eq!(sb.model.activities.len(), 2);
    assert_eq!(wbs_codes(&sb), b_contents.wbs);
    assert_eq!(sb.calendar_names, b_contents.calendars);

    // Quit and start again: both projects come back open, each as itself.
    let b_now = contents(open_project(&d, b));
    restart(&mut d);
    let mut open = d.app.open_ids();
    open.sort();
    assert_eq!(open, vec![a, b], "the open set is restored");
    assert_eq!(contents(open_project(&d, a)), a_contents);
    assert_eq!(contents(open_project(&d, b)), b_now);
    assert_eq!(
        d.app.open_set.default,
        Some(b),
        "the default project is kept"
    );

    // Close A from the Projects view; B stays.
    projects_select(&mut d, a);
    d.click("projects_cmd_close");
    d.frames(2);
    assert_eq!(d.app.open_ids(), vec![b], "only B is open");
    assert_eq!(d.app.project.model.project.name, "BB-200");
    // Delete A: it asks, then goes; undo brings it back whole.
    projects_select(&mut d, a);
    d.click("dock_delete");
    assert!(d.app.views.projects.delete.is_some(), "Delete asks first");
    d.click("delete_project_yes");
    d.frames(2);
    assert!(d.app.store.load(a).is_err(), "A is deleted");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    d.key_with(Key::Z, ctrl());
    d.frames(2);
    assert_eq!(contents(&stored(&d, a)), a_contents, "undo restores A");
    assert_eq!(contents(&stored(&d, sample)), sample_contents);
    println!("new project A holds {a_contents:?}");
}

/// The known bug (fixed on feature/fix-new-project-wbs): Projects view Add
/// makes a project that shows the sample project's WBS.
#[test]
fn workflow_2b_projects_view_add_has_only_its_own_wbs() {
    let mut d = fresh("wf2b");
    let sample = d.app.open_set.active.unwrap();
    let a = new_project(&mut d, "AA-100", "Alpha Unit", "04-Jan-27", "");
    let b = new_project(&mut d, "BB-200", "Bravo Unit", "11-Jan-27", "");
    // Projects view Add: a third empty project, opened on its own, has only
    // its own node in the WBS view, before and after a restart.
    show(&mut d, "projects");
    d.click("projects_row_0");
    // Add opens the New Project dialog; Finish makes the project.
    d.click("add");
    d.frames(2);
    d.click("new_project_finish");
    d.frames(2);
    let ids = d.app.workspace_ids();
    let c = *ids
        .iter()
        .find(|id| ![a, b, sample].contains(id))
        .expect("Add makes a project");
    open_projects(&mut d, &[c]);
    show(&mut d, "wbs");
    // What a user then tries: delete a stray element.
    if let Some(stray) = d.app.project.wbs.get(1).map(|w| w.id) {
        let n = d.app.project.wbs.len();
        wbs_click(&mut d, stray);
        d.click("dock_delete");
        let asked = d.app.views.wbs.pending.is_some();
        if d.has("wbs_confirm_merge") {
            d.click("wbs_confirm_merge");
        }
        println!(
            "delete stray: asked {asked}, {} -> {} elements, status {:?}",
            n,
            d.app.project.wbs.len(),
            d.app.status
        );
    }
    assert_eq!(wbs_rows(&d).len(), 1, "C's WBS: {:?}", wbs_rows(&d));
    assert!(d.app.project.model.activities.is_empty());
    d.key_with(Key::S, ctrl());
    restart(&mut d);
    assert_eq!(d.app.open_ids(), vec![c]);
    assert_eq!(
        wbs_rows(&d).len(),
        1,
        "C's WBS after restart: {:?}",
        wbs_rows(&d)
    );
}

// ------------------------------------------------------- new activity defaults

/// Projects view, Defaults tab of the open project.
fn defaults_tab(d: &mut Driver) {
    let id = d.app.open_set.active.expect("the open project has a row");
    projects_select(d, id);
    let tab = views::projects::TABS
        .iter()
        .position(|t| *t == "Defaults")
        .unwrap();
    d.click(&format!("projects_tab_{tab}"));
    assert_eq!(d.app.views.projects.tab, tab);
}

/// A PROJECT column of the open project.
fn project_value(d: &Driver, column: &str) -> String {
    let p = &d.app.project;
    let t = p.document.table("PROJECT").unwrap();
    t.rows
        .iter()
        .find(|r| records::num(t.value(r, "proj_id")) == p.model.project.proj_id)
        .map(|r| t.value(r, column).to_string())
        .unwrap_or_default()
}

/// Edit > Add with the project's Defaults tab values: the types, calendar,
/// duration and Activity ID numbering a scheduler set there, the row in the
/// selected activity's WBS with its name ready to type over, and one undo
/// step per add.
#[test]
fn workflow_new_activity_takes_the_project_defaults() {
    let mut d = fresh("wf-new-activity");
    // Defaults tab: Fixed Units, Physical, Resource Dependent, IDs RB-
    // from 1000 by 10, incremented from the selected activity.
    defaults_tab(&mut d);
    for (combo, item) in [
        ("project_def_duration_type", 2),
        ("project_def_complete_pct_type", 0),
        ("project_def_task_type", 1),
    ] {
        d.click(combo);
        d.click(&format!("{combo}_item_{item}"));
    }
    for (field, text) in [
        ("project_field_task_code_prefix", "RB-"),
        ("project_field_task_code_base", "1000"),
        ("project_field_task_code_step", "10"),
    ] {
        type_into(&mut d, field, text);
        d.key(Key::Enter);
    }
    if project_value(&d, "task_code_prefix_flag") != "Y" {
        d.click("project_increment_flag");
    }
    d.assert_clean("defaults tab");
    for (column, value) in [
        ("def_duration_type", "DT_FixedQty"),
        ("def_complete_pct_type", "CP_Phys"),
        ("def_task_type", "TT_Rsrc"),
        ("task_code_prefix", "RB-"),
        ("task_code_base", "1000"),
        ("task_code_step", "10"),
        ("task_code_prefix_flag", "Y"),
    ] {
        assert_eq!(project_value(&d, column), value, "Defaults tab {column}");
    }

    // Leave a gap after RB-1010: RB-1020 becomes RB-1500.
    d.home();
    let moved = index_of(&d.app.project, "RB-1020");
    cell_edit(&mut d, moved, "activity_id", "RB-1500");
    let before = d.app.project.model.activities.len();
    let steps = d.app.history.counts().0;

    // Edit > Add with RB-1010 selected: RB-1020 fills the gap (not the
    // highest plus 10), in RB-1010's WBS, with every default.
    let anchor = index_of(&d.app.project, "RB-1010");
    let anchor_wbs = d.app.project.details[&d.app.project.model.activities[anchor].task_id].wbs;
    select_row(&mut d, anchor);
    d.click("menu_bar_edit");
    d.click("menu_add");
    d.frames(2);
    assert_eq!(
        d.app.project.model.activities.len(),
        before + 1,
        "Add adds one"
    );
    let i = d.app.selected.expect("the new activity is selected");
    let p = &d.app.project;
    let a = &p.model.activities[i];
    let (task_id, pid) = (a.task_id, p.model.project.proj_id);
    assert_eq!(a.task_code, "RB-1020", "selected ID plus the increment");
    assert_eq!(a.task_type, TaskType::Resource);
    assert_eq!(a.rem, 5 * 8 * 60, "five days remaining");
    assert_eq!(p.details[&task_id].original_duration, 5 * 8 * 60);
    assert_eq!(
        p.details[&task_id].wbs, anchor_wbs,
        "the selected activity's WBS"
    );
    let calendar = project_value(&d, "clndr_id");
    assert_eq!(
        p.model.calendars[a.cal].id, calendar,
        "the default calendar"
    );
    let task = |p: &ProjectData, c: &str| {
        openxer_xer::edit::records::task_value(&p.document, task_id, pid, c)
    };
    assert_eq!(task(p, "duration_type"), "DT_FixedQty");
    assert_eq!(task(p, "complete_pct_type"), "CP_Phys");
    assert_eq!(d.app.history.counts().0, steps + 1, "Add is one undo step");
    // The name cell is open holding "New Activity", all selected: typing
    // replaces it and Enter keeps it.
    assert_eq!(d.app.edit_cell, Some((i, "activity_name".to_string())));
    assert_eq!(d.app.edit_text, "New Activity");
    d.frames(2);
    d.type_text("Set pump skid");
    d.key(Key::Enter);
    d.frames(2);
    d.assert_clean("name the new activity");
    assert_eq!(d.app.project.name(i), "Set pump skid");

    // Switch off: the highest ID plus the increment, whatever is selected.
    defaults_tab(&mut d);
    d.click("project_increment_flag");
    assert_eq!(project_value(&d, "task_code_prefix_flag"), "N");
    d.home();
    let anchor = index_of(&d.app.project, "RB-1010");
    select_row(&mut d, anchor);
    let steps = d.app.history.counts().0;
    d.click("menu_bar_edit");
    d.click("menu_add");
    d.frames(2);
    let j = d.app.selected.expect("the second new activity is selected");
    assert_ne!(i, j);
    assert_eq!(d.app.project.model.activities[j].task_code, "RB-1510");
    assert_eq!(d.app.project.name(j), "New Activity");
    assert_eq!(d.app.history.counts().0, steps + 1, "Add is one undo step");

    // One undo takes the second add back and nothing else.
    d.key(Key::Escape);
    d.click("undo");
    d.frames(2);
    d.assert_clean("undo add");
    let codes: Vec<&str> = d
        .app
        .project
        .model
        .activities
        .iter()
        .map(|a| a.task_code.as_str())
        .collect();
    assert!(
        !codes.contains(&"RB-1510"),
        "undo removes the add: {codes:?}"
    );
    assert!(codes.contains(&"RB-1020") && codes.contains(&"RB-1500"));
    assert_eq!(d.app.project.model.activities.len(), before + 1);
    assert_eq!(project_value(&d, "task_code_prefix_flag"), "N");
}
// ------------------------------------------------------------------ 3

/// Click a row of the open picker by its text, then Assign and Close.
fn pick_and_assign(d: &mut Driver, text: &str) {
    assert!(d.click_text(text), "{text:?} is offered in the picker");
    d.click("rsrc_picker_assign");
    if d.has("assignment_question_ok") {
        d.click("assignment_question_ok");
    }
    if d.has("rsrc_picker_close") {
        d.click("rsrc_picker_close");
    }
    d.assert_clean("assign");
}

fn rsrc_ids(p: &ProjectData) -> Vec<i64> {
    p.document
        .table("RSRC")
        .map(|t| {
            t.rows
                .iter()
                .filter_map(|r| t.value(r, "rsrc_id").parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Workflow 3: resources and roles made in a new project, assigned, priced,
/// looked at in the usage spreadsheet and profile, and leveled.
#[test]
fn workflow_3_resources_roles_assign_rate_and_level() {
    let mut d = fresh("wf3");
    new_project(&mut d, "RS-300", "Resource Trial", "04-Jan-27", "");
    show(&mut d, "activities");
    let weld = add_activity(&mut d, "Weld tie-in TP-1", "5d");
    let fit = add_activity(&mut d, "Fit up tie-in TP-2", "5d");
    d.f9();

    // Enterprise > Roles: one role.
    d.home();
    d.click("menu_bar_enterprise");
    d.click("menu_roles");
    d.frames(2);
    d.click("roles_add");
    d.frames(2);
    d.click("roles_list_cell_0_1");
    for _ in 0..30 {
        d.key(Key::Backspace);
    }
    d.type_text("Pipe Welder");
    d.key(Key::Enter);
    d.frames(2);
    d.click("roles_close");
    d.assert_clean("roles");
    let roles = d
        .app
        .project
        .document
        .table("ROLES")
        .map_or(0, |t| t.rows.len());
    assert_eq!(roles, 1, "a role is added");

    // Resources view: Add, name it, max units 8h/d and a price of 100/h.
    show(&mut d, "resources");
    let before = rsrc_ids(&d.app.project);
    d.click("add");
    d.frames(2);
    let rsrc = *rsrc_ids(&d.app.project)
        .iter()
        .find(|id| !before.contains(id))
        .expect("Add makes a resource");
    assert_eq!(d.app.views.resources.selected, Some(rsrc));
    d.click("resources_tab_0");
    type_into(&mut d, "resource_field_rsrc_short_name", "WELDER");
    d.key(Key::Enter);
    d.frames(2);
    let value =
        |d: &Driver, field: &str| views::resource_edit::value(&d.app.project.document, rsrc, field);
    assert_eq!(value(&d, "rsrc_short_name"), "WELDER");
    d.click("resources_tab_3");
    d.click("resource_rates_row_0");
    type_into(&mut d, "resource_rate_cell_1", "8h/d");
    d.key(Key::Enter);
    type_into(&mut d, "resource_rate_cell_2", "100");
    d.key(Key::Enter);
    d.frames(2);
    d.assert_clean("rates");
    let rate = views::resource_edit::rate_rows(&d.app.project.document, rsrc)[0];
    let get = |d: &Driver, k: &str| records::get(&d.app.project.document, "RSRCRATE", rate, k);
    assert_eq!(
        records::float(&get(&d, "max_qty_per_hr")),
        1.0,
        "8h/d is 1/h"
    );
    assert_eq!(records::float(&get(&d, "cost_per_qty")), 100.0);

    // Assign WELDER to both activities through Activity Details.
    for i in [weld, fit] {
        show(&mut d, "activities");
        select_row(&mut d, i);
        d.click("detail_tab_resources");
        d.click("detail_rsrc_add");
        pick_and_assign(&mut d, "WELDER");
        let task = d.app.project.model.activities[i].task_id;
        let rows = d.app.project.assignment_rows(task);
        assert_eq!(rows.len(), 1, "one assignment");
        let view = d.app.project.assignment_view(rows[0]).unwrap();
        assert_eq!(view.rsrc_id, Some(rsrc));
        assert_eq!(view.remaining.units, 40., "5 days at 8h/d");
        let cost = records::float(&records::get(
            &d.app.project.document,
            "TASKRSRC",
            rows[0],
            "target_cost",
        ));
        assert!((cost - 4000.).abs() < 0.01, "40h at 100/h: {cost}");
    }
    // A role assignment on the weld.
    select_row(&mut d, weld);
    d.click("detail_tab_resources");
    d.click("detail_rsrc_add_role");
    pick_and_assign(&mut d, "Pipe Welder");
    let task = d.app.project.model.activities[weld].task_id;
    assert_eq!(
        d.app.project.assignment_rows(task).len(),
        2,
        "role row added"
    );

    // Usage spreadsheet and profile show the overload: 80h in the first week.
    d.home();
    d.click("tool_resource_usage_sheet");
    d.frames(3);
    d.assert_clean("usage sheet");
    let texts: Vec<String> = d.texts().into_iter().map(|(t, _)| t).collect();
    assert!(
        texts.iter().any(|t| t == "80h"),
        "the usage spreadsheet shows the 80h week: {:?}",
        texts
            .iter()
            .filter(|t| t.chars().next().is_some_and(|c| c.is_ascii_digit()))
            .take(30)
            .collect::<Vec<_>>()
    );
    d.click("tool_resource_usage_profile");
    d.frames(3);
    d.assert_clean("usage profile");

    // Level: one welder cannot do both at once.
    d.f9();
    d.level();
    d.assert_clean("level");
    let span = |d: &Driver, i: usize| {
        let a = &d.app.project.model.activities[i];
        (
            a.restart.or(a.early_start).unwrap(),
            a.reend.or(a.early_finish).unwrap(),
        )
    };
    let (w, f) = (span(&d, weld), span(&d, fit));
    assert!(w.1 <= f.0 || f.1 <= w.0, "leveled apart: {w:?} {f:?}");

    // Save, reload and the XER round trip keep resources, rates and
    // assignments; leveling the import gives the same dates.
    d.key_with(Key::S, ctrl());
    let id = d.app.open_set.active.unwrap();
    let back = stored(&d, id);
    assert!(rsrc_ids(&back).contains(&rsrc));
    assert_eq!(
        back.document.table("TASKRSRC").map_or(0, |t| t.rows.len()),
        3
    );
    let xer = xer_round_trip(&mut d, "wf3.xer");
    assert!(rsrc_ids(&xer).contains(&rsrc), "the resource is exported");
    assert_eq!(xer.document.table("ROLES").map_or(0, |t| t.rows.len()), 1);
    let mut again = xer.clone();
    again.model.options.do_level = true;
    again.refresh_derived_durations();
    schedule_model(&mut again.model, &again.resources).unwrap();
    let leveled = |p: &ProjectData, i: usize| {
        let a = &p.model.activities[i];
        (a.restart, a.reend)
    };
    for i in [weld, fit] {
        assert_eq!(
            leveled(&again, i),
            leveled(&d.app.project, i),
            "same leveled dates"
        );
    }

    // Undo back past the resource: it and its assignments are gone, and F9
    // no longer sees them.
    let steps = undo_to_new_project(&mut d);
    assert!(steps > 0);
    assert!(
        !rsrc_ids(&d.app.project).contains(&rsrc),
        "undo removes the resource: {steps} undone, {}",
        undo_state(&d)
    );
    assert!(
        !d.app.project.resources.limits.contains_key(&rsrc),
        "the engine's resource limits follow undo"
    );
    d.redo_all();
    assert!(
        rsrc_ids(&d.app.project).contains(&rsrc),
        "redo brings it back"
    );
    assert_eq!(d.app.project.assignment_rows(task).len(), 2);
}
// ------------------------------------------------------------------ 4

/// Show these columns in the activity table and read activity `i`'s cells
/// as drawn.
fn drawn_cells(d: &mut Driver, i: usize, columns: &[&str]) -> Vec<String> {
    d.home();
    d.app.grid.layout.columns = std::iter::once("activity_id")
        .chain(columns.iter().copied())
        .map(|c| grid::layout::ColumnSetting::new(c, 110.))
        .collect();
    d.app.drop_frame_cache();
    d.frames(3);
    let texts = d.texts();
    columns
        .iter()
        .map(|c| {
            let at = row_cell(d, i, c);
            texts
                .iter()
                .filter(|(_, r)| {
                    r.contains(at)
                        || (r.top() <= at.y
                            && r.bottom() >= at.y
                            && (r.center().x - at.x).abs() < 55.)
                })
                .map(|(t, _)| t.clone())
                .next()
                .unwrap_or_default()
        })
        .collect()
}

fn status(p: &ProjectData, i: usize) -> StatusType {
    p.model.activities[i].status
}

/// Workflow 4: a baseline, the data date moved by Update Progress, actual
/// dates and remaining durations entered, F9, and the schedule compared with
/// the baseline.
#[test]
fn workflow_4_progress_against_a_baseline() {
    let mut d = fresh("wf4");
    let id = new_project(&mut d, "PR-400", "Progress Trial", "04-Jan-27", "");
    show(&mut d, "activities");
    let a = add_activity(&mut d, "Demo old pump base", "5d");
    let b = add_activity(&mut d, "Pour new pump base", "5d");
    let c = add_activity(&mut d, "Set and grout new pump", "5d");
    link(&mut d, a, b);
    link(&mut d, b, c);
    d.f9();
    let p = d.app.project.clone();
    assert_eq!(
        date(&p, p.model.activities[c].early_finish),
        "2027-01-22 17:00"
    );

    // Project > Maintain Baselines > Add: a copy of the project.
    d.home();
    d.click("menu_bar_project");
    d.click("menu_maintain_baselines");
    d.frames(2);
    d.click("baselines_add");
    d.frames(2);
    d.click("add_ok");
    d.frames(2);
    d.click("baselines_close");
    d.assert_clean("add baseline");
    // Project > Assign Baselines: the copy as the project baseline.
    d.click("menu_bar_project");
    d.click("menu_assign_baselines");
    d.frames(2);
    d.click("assign_lookup_0_button");
    d.frames(1);
    d.click("lookup_row_0");
    d.click("lookup_select");
    d.click("assign_ok");
    d.assert_clean("assign baseline");

    // Update Progress (Tools toolbar) to 13 January: A is done, B is under way.
    d.click("tool_update_progress");

    d.frames(2);
    type_into(&mut d, "update_new_date", "13-Jan-27 08:00");
    d.click("update_apply");
    d.frames(2);
    // The notice that follows a run, as the reference shows one.
    d.click("progress_done_ok");
    d.assert_clean("update progress");
    let p = d.app.project.clone();
    assert_eq!(date(&p, p.model.project.data_date), "2027-01-13 08:00");
    assert_eq!(
        status(&p, a),
        StatusType::Completed,
        "A finished by the data date"
    );
    assert_eq!(status(&p, b), StatusType::InProgress, "B started");
    assert_eq!(status(&p, c), StatusType::NotStarted);
    assert_eq!(
        date(&p, p.model.activities[a].act_finish),
        "2027-01-08 17:00"
    );

    // The field says B really started on the 12th with 4 days to go.
    select_row(&mut d, b);
    d.click("detail_tab_status");
    type_into(
        &mut d,
        &format!("detail_date_{b}-act_start"),
        "12-Jan-27 08:00",
    );
    d.key(Key::Enter);
    type_into(&mut d, "detail_remaining", "4d");
    d.key(Key::Enter);
    d.frames(2);
    d.assert_clean("actuals");
    if let Some(prompt) = d.app.sched.sequence.take() {
        panic!("unexpected out-of-sequence prompt for {}", prompt.activity);
    }
    let p = d.app.project.clone();
    assert_eq!(
        date(&p, p.model.activities[b].act_start),
        "2027-01-12 08:00"
    );
    assert_eq!(p.model.activities[b].rem, 4 * 480);

    // F9 and the dates against the baseline.
    d.f9();
    let p = d.app.project.clone();
    assert_eq!(
        date(&p, p.model.activities[b].early_finish),
        "2027-01-18 17:00"
    );
    assert_eq!(
        date(&p, p.model.activities[c].early_finish),
        "2027-01-25 17:00"
    );
    let cells = drawn_cells(
        &mut d,
        c,
        &["finish", "bl_project_finish", "variance_bl_project_finish"],
    );
    assert_eq!(cells[0], "25-Jan-27 17:00", "{cells:?}");
    assert_eq!(
        cells[1], "22-Jan-27 17:00",
        "the baseline keeps the plan: {cells:?}"
    );
    assert_eq!(cells[2], "-1d", "one day late: {cells:?}");
    let cells = drawn_cells(&mut d, b, &["duration_percent_complete", "actual_start"]);
    assert_eq!(cells, ["20%", "12-Jan-27 08:00"], "1 of 5 days done");

    // Save, restart and the XER round trip keep progress and the baseline.
    d.key_with(Key::S, ctrl());
    restart(&mut d);
    assert_eq!(d.app.open_set.active, Some(id));
    let cells = drawn_cells(&mut d, c, &["bl_project_finish"]);
    assert_eq!(
        cells[0], "22-Jan-27 17:00",
        "the baseline survives a restart: {cells:?}"
    );
    let xer = xer_round_trip(&mut d, "wf4.xer");
    let x = &xer.model.activities[index_of(&xer, &p.model.activities[b].task_code)];
    assert_eq!(date(&xer, x.act_start), "2027-01-12 08:00");
    assert_eq!(x.rem, 4 * 480);
    assert_eq!(date(&xer, xer.model.project.data_date), "2027-01-13 08:00");
    let mut again = xer.clone();
    schedule_model(&mut again.model, &again.resources).unwrap();
    let y = &again.model.activities[index_of(&again, &p.model.activities[c].task_code)];
    assert_eq!(
        date(&again, y.early_finish),
        "2027-01-25 17:00",
        "F9 after import agrees"
    );
}

// ------------------------------------------------- 4b Global Change scope

/// Every activity's Original Duration as the table shows it.
fn original_durations(d: &Driver) -> Vec<String> {
    let p = &d.app.project;
    let catalog = grid::columns::catalog(p);
    let column = grid::columns::find(&catalog, "original_duration").unwrap();
    let ix = d.app.grid_index();
    (0..p.model.activities.len())
        .map(|i| grid::columns::display(p, Some(i), &grid::columns::value(p, &ix, column, i)))
        .collect()
}

/// The Activities view's layout bar Filter button (Filters...): tick `key`
/// (or All Activities with `None`), OK.
fn set_filter(d: &mut Driver, key: Option<&str>) {
    d.home();
    d.click("filter_button");
    assert!(d.app.grid.dialogs.filters.is_some(), "Filters opens");
    match key {
        Some(key) => d.click(&format!("filter_select_{key}")),
        None => d.click("filters_all_activities"),
    }
    d.click("filters_ok");
    assert!(d.app.grid.dialogs.filters.is_none(), "OK closes Filters");
    d.assert_clean("filters");
}

/// A global change with no If row acts only on the activities the
/// Activities view's filter shows: the report lists only those, Commit
/// changes only those (one undo step), and the hidden ones keep their
/// values once the filter is removed.
#[test]
fn workflow_4b_global_change_acts_on_the_filtered_activities_only() {
    let mut d = fresh("qa_gc_scope");
    let all = d.app.project.model.activities.len();
    set_filter(&mut d, Some(&grid::filters::Standard::Critical.key()));
    let shown: BTreeSet<i64> = d
        .visible_activities()
        .into_iter()
        .map(|i| d.app.project.model.activities[i].task_id)
        .collect();
    assert!(
        !shown.is_empty() && shown.len() < all,
        "the filter shows a subset: {} of {all}",
        shown.len()
    );
    let before = original_durations(&d);
    // Tools > Global Change, New: drop the If row, Then Original Duration =
    // Original Duration x 2.
    d.home();
    d.click("menu_bar_tools");
    d.click("tools_global_change");
    assert!(
        d.app.editing.dialogs.gc_list.is_some(),
        "Global Change opens"
    );
    d.click("gc_new");
    assert!(d.app.editing.dialogs.gc_edit.is_some(), "New opens Modify");
    type_into(&mut d, "gc_edit_name", "Double durations");
    d.click("gc_if_0");
    d.click("gc_edit_delete");
    d.click("gc_delete_line_yes");
    {
        // The parameter and operator are drop-downs; pick them as a click
        // in their lists would.
        let e = d.app.editing.dialogs.gc_edit.as_mut().unwrap();
        assert!(e.gc.conditions.is_empty(), "the If row is deleted");
        e.gc.then[0].column = "original_duration".into();
        e.gc.then[0].operator = "x".into();
        e.gc.then[0].value2 = "2".into();
    }
    type_into(&mut d, "gc_Then_value_0", "Original Duration");
    d.click("gc_edit_ok");
    assert!(d.app.editing.dialogs.gc_edit.is_none(), "{:?}", d.app.error);
    let k = d
        .app
        .editing
        .global_changes
        .iter()
        .position(|g| g.name == "Double durations")
        .expect("the definition is saved");
    assert!(d.app.editing.global_changes[k].conditions.is_empty());
    assert_eq!(
        d.app.editing.dialogs.gc_list.as_ref().unwrap().selected,
        Some(k)
    );
    d.click("gc_apply");
    let report = d
        .app
        .editing
        .dialogs
        .gc_report
        .as_ref()
        .expect("Apply Change opens the report")
        .changes
        .clone();
    // The report lists every shown activity whose duration doubles (a
    // milestone's zero stays zero) and nothing the filter hides.
    let listed: BTreeSet<i64> = report.iter().map(|c| c.task_id).collect();
    let doubling: BTreeSet<i64> = d
        .app
        .project
        .model
        .activities
        .iter()
        .filter(|a| shown.contains(&a.task_id))
        .filter(|a| !matches!(a.task_type, TaskType::Mile | TaskType::FinMile))
        .map(|a| a.task_id)
        .collect();
    assert!(!doubling.is_empty(), "some shown activity has a duration");
    assert_eq!(
        listed, doubling,
        "the report lists only the shown activities: {report:#?}"
    );
    assert!(report.iter().all(|c| c.error.is_none() && c.apply));
    let undo = d.app.history.counts().0;
    d.click("gc_commit");
    d.assert_clean("commit");
    assert!(
        d.app.editing.dialogs.gc_report.is_none(),
        "Commit closes the report"
    );
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");
    // All Activities again: only the shown activities changed.
    set_filter(&mut d, None);
    assert_eq!(d.visible_activities().len(), all, "the filter is off");
    let after = original_durations(&d);
    for (i, a) in d.app.project.model.activities.iter().enumerate() {
        match report.iter().find(|c| c.task_id == a.task_id) {
            Some(c) => assert_eq!(after[i], c.new, "{} doubled", a.task_code),
            None => assert_eq!(after[i], before[i], "{} unchanged", a.task_code),
        }
    }
    assert_ne!(after, before);
    d.click("undo");
    assert_eq!(
        original_durations(&d),
        before,
        "one undo puts every value back"
    );
    assert_eq!(d.app.history.counts().0, undo);
}

// ------------------------------------------------------------------ 4c

/// The stored baselines of the open project, as Maintain Baselines lists
/// them.
fn baseline_rows(d: &Driver) -> Vec<crate::baselines::store::BaselineRow> {
    let parent = d.app.workspace_row().expect("a workspace row");
    crate::baselines::store::list(d.app.store.connection(), parent).unwrap()
}

/// The Maintain Baselines list row of baseline `id`: baselines are counted
/// across the bands of the open projects, in band order.
fn baseline_line(d: &Driver, id: i64) -> String {
    let k = d
        .app
        .baseline_bands()
        .iter()
        .flat_map(|b| b.rows.iter())
        .position(|r| r.id == id)
        .unwrap_or_else(|| panic!("baseline {id} is listed"));
    format!("baselines_row_{k}")
}

/// Project > Maintain Baselines.
fn open_maintain(d: &mut Driver) {
    d.home();
    d.click("menu_bar_project");
    d.click("menu_maintain_baselines");
    d.frames(2);
    assert!(
        d.app.baselines.maintain.is_some(),
        "Maintain Baselines opens"
    );
}

/// Maintain Baselines > Add > Save a copy (no further question). Returns
/// the new baseline.
fn add_baseline_copy(d: &mut Driver) -> i64 {
    let before = baseline_rows(d).len();
    d.click("baselines_add");
    d.frames(2);
    d.click("add_ok");
    d.frames(2);
    d.assert_clean("add baseline");
    assert!(d.app.baselines.add.is_none() && d.app.baselines.select.is_none());
    let rows = baseline_rows(d);
    assert_eq!(rows.len(), before + 1, "Add saves one copy");
    let id = rows.last().unwrap().id;
    assert_eq!(
        d.app.baselines.maintain.as_ref().unwrap().selected,
        Some(id),
        "the new baseline is selected"
    );
    id
}

/// The OK-only message up now, then OK.
fn notice_ok(d: &mut Driver, text: &str) {
    d.frames(2);
    assert_eq!(d.app.baselines.notice.as_deref(), Some(text));
    assert!(d.app.baselines.confirm.is_none(), "no question first");
    d.click("baseline_notice_ok");
    assert!(d.app.baselines.notice.is_none());
}

/// Assign Baselines: field `slot` (0 project, 1 primary, 2 secondary, 3
/// tertiary) set from its lookup: row `row` selected, or Remove.
fn assign_slot(d: &mut Driver, slot: usize, row: Option<usize>) {
    d.click(&format!("assign_lookup_{slot}_button"));
    d.frames(2);
    assert!(d.app.baselines.lookup.is_some(), "the lookup opens");
    match row {
        Some(k) => {
            d.click(&format!("lookup_row_{k}"));
            d.click("lookup_select");
        }
        None => d.click("lookup_remove"),
    }
    assert!(d.app.baselines.lookup.is_none());
}

/// Project > Assign Baselines, the slots set, OK.
fn assign(d: &mut Driver, slots: &[(usize, Option<usize>)]) {
    d.home();
    d.click("menu_bar_project");
    d.click("menu_assign_baselines");
    d.frames(2);
    for (slot, row) in slots {
        assign_slot(d, *slot, *row);
    }
    d.click("assign_ok");
    d.assert_clean("assign baselines");
    assert!(d.app.baselines.assign.is_none());
}

/// Workflow 4b: baselines kept and managed through Project > Maintain
/// Baselines, Assign Baselines and Revert Project to Baseline, with the
/// behaviour of the scheduler users know: copies named "<ID> - B<n>", Copy
/// as another baseline, assigned baselines refused for Delete and Restore,
/// Restore into a separate project in the owner's folder, Convert of a
/// closed project only, and undo.
#[test]
fn workflow_4c_maintain_and_assign_baselines() {
    use crate::baselines::store::Choice;
    use openxer_automation::baseline_ops as ops;
    let mut d = fresh("wf4b");
    let id = new_project(&mut d, "BL-100", "Baseline Trial", "04-Jan-27", "");
    let project_count = d.app.store.list().unwrap().len();
    show(&mut d, "activities");
    let a = add_activity(&mut d, "Demo old pump base", "5d");
    let b = add_activity(&mut d, "Pour new pump base", "5d");
    link(&mut d, a, b);
    d.f9();
    // Two baseline types, added in reverse order and reordered in the
    // dictionary: the Type drop-down lists <None>, then by name.
    let zeta = dictionary(&mut d, Cmd::AddCategory(dict::Category::Baseline, None)).unwrap();
    dictionary(
        &mut d,
        Cmd::SetCategory(
            dict::Category::Baseline,
            zeta,
            "base_type",
            "Zeta Plan".into(),
        ),
    );
    let alpha = dictionary(
        &mut d,
        Cmd::AddCategory(dict::Category::Baseline, Some(zeta)),
    )
    .unwrap();
    dictionary(
        &mut d,
        Cmd::SetCategory(
            dict::Category::Baseline,
            alpha,
            "base_type",
            "Alpha Plan".into(),
        ),
    );
    let spec = dict::category_tree(dict::Category::Baseline);
    dictionary(
        &mut d,
        Cmd::Move(spec, None, alpha, crate::dialog_kit::Move::Up),
    );
    let order: Vec<String> = dict::categories(&d.app.project.document, dict::Category::Baseline)
        .into_iter()
        .map(|r| r.cells[0].clone())
        .collect();
    assert_eq!(order, ["Alpha Plan", "Zeta Plan"], "Shift up reorders");
    let names: Vec<String> = d
        .app
        .baseline_type_list()
        .into_iter()
        .map(|t| t.1)
        .collect();
    assert_eq!(names, ["<None>", "Alpha Plan", "Zeta Plan"]);

    // The band selected: Delete does nothing.
    open_maintain(&mut d);
    assert!(d.has("baselines_row_project"), "the project band");
    d.click("baselines_row_project");
    d.click("baselines_delete");
    assert!(d.app.baselines.confirm.is_none() && d.app.baselines.notice.is_none());

    // Add twice: B1 and B2, named after the project's ID and name.
    let b1 = add_baseline_copy(&mut d);
    let b2 = add_baseline_copy(&mut d);
    let rows = baseline_rows(&d);
    let ids: Vec<(&str, &str)> = rows
        .iter()
        .map(|r| (r.short_name.as_str(), r.name.as_str()))
        .collect();
    assert_eq!(
        ids,
        [
            ("BL-100 - B1", "Baseline Trial - B1"),
            ("BL-100 - B2", "Baseline Trial - B2")
        ]
    );
    let p = d.app.project.clone();
    assert_eq!(
        rows[0].data_date,
        date(&p, p.model.project.data_date),
        "the copy keeps the data date"
    );
    assert_eq!(
        rows[0].updated, rows[0].added,
        "last update is when it was taken"
    );

    // Copy B1: B3, a baseline of the same project, with B1's last update.
    d.click(&baseline_line(&d, b1));
    d.click("baselines_copy");
    d.frames(2);
    assert!(d.app.baselines.confirm.is_none(), "Copy asks nothing");
    let rows = baseline_rows(&d);
    assert_eq!(rows.len(), 3);
    let b3 = rows[2].id;
    assert_eq!(rows[2].short_name, "BL-100 - B3");
    assert_eq!(rows[2].name, "Baseline Trial - B3");
    assert_eq!(rows[2].updated, rows[0].updated);
    assert_eq!(
        d.app.store.list().unwrap().len(),
        project_count,
        "baselines are no projects"
    );

    // Edit B3's ID, name and type under the list.
    d.click(&baseline_line(&d, b3));
    type_into(&mut d, "baselines_id", "BL-100 - CTR");
    type_into(&mut d, "baselines_name", "Contract plan");
    d.click("baselines_type");
    d.click("baselines_type_item_1");
    d.frames(2);
    let row = baseline_rows(&d).into_iter().find(|r| r.id == b3).unwrap();
    assert_eq!(row.short_name, "BL-100 - CTR");
    assert_eq!(row.name, "Contract plan");
    assert_eq!(row.baseline_type, alpha.to_string());
    d.click("baselines_close");
    d.assert_clean("edit baseline");
    // Undo takes the type back, then the ID and name; redo puts them back.
    d.click("undo");
    let row = baseline_rows(&d).into_iter().find(|r| r.id == b3).unwrap();
    assert_eq!(row.baseline_type, "", "undo of the type");
    for _ in 0..4 {
        if !baseline_rows(&d)
            .iter()
            .any(|r| r.short_name == "BL-100 - CTR")
        {
            break;
        }
        d.click("undo");
    }
    let row = baseline_rows(&d).into_iter().find(|r| r.id == b3).unwrap();
    assert_eq!(row.short_name, "BL-100 - B3");
    for _ in 0..6 {
        if !baseline_rows(&d)
            .iter()
            .any(|r| r.id == b3 && r.baseline_type.is_empty())
        {
            break;
        }
        d.click("redo");
    }
    let row = baseline_rows(&d).into_iter().find(|r| r.id == b3).unwrap();
    assert_eq!(
        (row.short_name.as_str(), row.name.as_str()),
        ("BL-100 - CTR", "Contract plan")
    );

    // Assign Baselines: Project Baseline starts on <Current Project>; B1
    // as the project baseline, B2 as the secondary baseline.
    assert_eq!(d.app.baseline_choices().0, Choice::Current);
    assert_eq!(d.app.project_baseline_name(), "Current Project");
    assign(&mut d, &[(0, Some(0)), (2, Some(1))]);
    let (project, users) = d.app.baseline_choices();
    assert_eq!(project, Choice::Baseline(b1));
    assert_eq!(
        users.0,
        [Choice::Current, Choice::Baseline(b2), Choice::None]
    );
    assert_eq!(
        d.app.project_baseline_name(),
        "Baseline Trial - B1",
        "the status bar names the project baseline"
    );

    // Delete and Restore of an assigned baseline: refused, OK only, no
    // question first; in any role.
    open_maintain(&mut d);
    for (line, button, text) in [
        (b1, "baselines_delete", ops::DELETE_ASSIGNED),
        (b1, "baselines_restore", ops::RESTORE_ASSIGNED),
        (b2, "baselines_delete", ops::DELETE_ASSIGNED),
        (b2, "baselines_restore", ops::RESTORE_ASSIGNED),
    ] {
        d.click(&baseline_line(&d, line));
        d.click(button);
        notice_ok(&mut d, text);
    }
    assert_eq!(baseline_rows(&d).len(), 3);
    d.click("baselines_close");
    // Free B2 again: Remove in the secondary lookup gives <None>.
    assign(&mut d, &[(2, None)]);
    assert_eq!(d.app.baseline_choices().1 .0[1], Choice::None);

    // Delete B2 (free): Yes, then the done message.
    open_maintain(&mut d);
    d.click(&baseline_line(&d, b2));
    d.click("baselines_delete");
    assert_eq!(
        d.app.baselines.confirm,
        Some(crate::baselines::dialogs::Confirm::Delete(b2))
    );
    d.click("confirm_yes");
    notice_ok(&mut d, ops::DELETED);
    assert!(baseline_rows(&d).iter().all(|r| r.id != b2));
    d.click("baselines_close");
    d.click("undo");
    assert!(
        baseline_rows(&d).iter().any(|r| r.id == b2),
        "undo puts it back"
    );
    d.click("redo");
    assert!(
        baseline_rows(&d).iter().all(|r| r.id != b2),
        "redo deletes it"
    );

    // Restore the contract baseline (free): Yes; it leaves the list and is a
    // separate project with its ID and name, in BL-100's folder.
    open_maintain(&mut d);
    d.click(&baseline_line(&d, b3));
    d.click("baselines_restore");
    assert_eq!(
        d.app.baselines.confirm,
        Some(crate::baselines::dialogs::Confirm::Restore(b3))
    );
    d.click("confirm_yes");
    d.frames(2);
    assert!(d.app.baselines.notice.is_none(), "no message after Restore");
    assert!(baseline_rows(&d).iter().all(|r| r.id != b3));
    let restored = d
        .app
        .store
        .list()
        .unwrap()
        .into_iter()
        .find(|(_, title)| title == "BL-100 - CTR")
        .map(|(w, _)| w)
        .expect("the restored project is in the workspace");
    let r = stored(&d, restored);
    assert_eq!(views::project_title(&r), "Contract plan");
    assert_eq!(r.model.activities.len(), 2);
    let folder = |d: &mut Driver, w: i64| d.app.workspace().node_of(Some(w));
    assert_eq!(
        folder(&mut d, restored),
        folder(&mut d, id),
        "same EPS folder"
    );
    d.click("baselines_close");
    // The Projects window lists it.
    projects_select(&mut d, restored);
    d.home();
    d.click("undo");
    assert!(d
        .app
        .store
        .list()
        .unwrap()
        .iter()
        .all(|(w, _)| *w != restored));
    assert!(
        baseline_rows(&d).iter().any(|r| r.id == b3),
        "undo brings it back"
    );
    d.click("redo");
    assert!(d
        .app
        .store
        .list()
        .unwrap()
        .iter()
        .any(|(w, _)| *w == restored));
    // Baselines are not offered in File > Open; the restored project is.
    d.home();
    d.click("menu_bar_file");
    d.click("menu_open");
    d.frames(2);
    let offered: Vec<i64> = d
        .app
        .open_set
        .open_dialog
        .as_ref()
        .expect("Open Project")
        .rows
        .iter()
        .filter_map(|r| r.id)
        .collect();
    let mut projects: Vec<i64> = d
        .app
        .store
        .list()
        .unwrap()
        .into_iter()
        .map(|p| p.0)
        .collect();
    let mut sorted = offered.clone();
    sorted.sort();
    projects.sort();
    assert_eq!(sorted, projects, "File > Open lists projects only");
    assert!(offered.contains(&restored));
    d.key(Key::Escape);
    d.frames(2);

    // Convert: the open project is refused (OK only), the closed restored
    // project becomes a baseline keeping its ID and name, last update blank.
    open_maintain(&mut d);
    d.click("baselines_add");
    d.click("add_convert");
    d.click("add_ok");
    d.frames(2);
    let list = d
        .app
        .baselines
        .select
        .as_ref()
        .and_then(|s| s.projects.clone())
        .expect("Select Project lists the projects");
    let at = |w: i64| list.iter().position(|p| p.0 == w).unwrap();
    d.click(&format!("select_project_{}", at(id)));
    d.click("select_ok");
    notice_ok(&mut d, ops::OPEN_REFUSED);
    assert!(
        d.app.baselines.select.is_some(),
        "Select Project stays open"
    );
    d.click(&format!("select_project_{}", at(restored)));
    d.click("select_ok");
    d.frames(2);
    d.assert_clean("convert");
    assert!(d.app.baselines.select.is_none());
    let converted = baseline_rows(&d).into_iter().last().unwrap();
    assert_eq!(converted.short_name, "BL-100 - CTR");
    assert_eq!(converted.name, "Contract plan");
    assert_eq!(converted.updated, "", "last update date blank");
    assert!(!converted.added.is_empty());
    assert!(d
        .app
        .store
        .list()
        .unwrap()
        .iter()
        .all(|(w, _)| *w != restored));
    d.click("baselines_close");
    d.click("undo");
    assert!(
        d.app
            .store
            .list()
            .unwrap()
            .iter()
            .any(|(w, _)| *w == restored),
        "undo of convert"
    );
    d.click("redo");

    // Revert Project to Baseline (Project menu): the project takes B1's data
    // and keeps its ID and name; the baseline stays; undo brings it back.
    let c = add_activity(&mut d, "Set and grout new pump", "5d");
    assert_eq!(d.app.project.model.activities.len(), 3);
    let code_c = d.app.project.model.activities[c].task_code.clone();
    let kept = baseline_rows(&d).len();
    d.home();
    d.click("menu_bar_project");
    d.click("menu_revert_baseline");
    d.frames(2);
    d.click("revert_row_0");
    d.click("revert_ok");
    assert_eq!(
        d.app.baselines.confirm,
        Some(crate::baselines::dialogs::Confirm::Revert(b1))
    );
    d.click("confirm_yes");
    d.frames(2);
    d.assert_clean("revert");
    assert!(d.app.baselines.revert.is_none());
    assert_eq!(d.app.project.model.activities.len(), 2);
    assert_eq!(d.app.project.model.project.name, "BL-100");
    assert_eq!(views::project_title(&d.app.project), "Baseline Trial");
    assert_eq!(baseline_rows(&d).len(), kept, "the baseline stays");
    assert_eq!(
        d.app.baseline_choices().0,
        Choice::Baseline(b1),
        "still assigned"
    );
    d.click("undo");
    assert_eq!(d.app.project.model.activities.len(), 3);
    assert!(d
        .app
        .project
        .model
        .activities
        .iter()
        .any(|x| x.task_code == code_c));

    // Saved and reopened, the baselines and choices are still there.
    d.key_with(Key::S, ctrl());
    restart(&mut d);
    assert_eq!(baseline_rows(&d).len(), kept);
    assert_eq!(d.app.baseline_choices().0, Choice::Baseline(b1));
}
// ------------------------------------------------------------------ 5

use crate::dictionaries::{ops as dict, Cmd};

/// A dictionary command as the Activity Codes or User Defined Fields
/// dialog's buttons dispatch it, then a frame.
fn dictionary(d: &mut Driver, cmd: Cmd) -> Option<i64> {
    let shown = format!("{cmd:?}");
    let out = d.app.run_dictionary(cmd).unwrap_or_else(|e| panic!("{e}"));
    d.frames(2);
    if let Some(notice) = &d.app.dictionaries.notice {
        panic!("{shown}: the dictionary says {notice:?}");
    }
    out
}

/// Activity Codes: a code type of `scope` named `name` with these values.
fn code_type(d: &mut Driver, scope: dict::Scope, name: &str, values: &[&str]) -> (i64, Vec<i64>) {
    let kind = dict::CodeKind::Activity;
    let t = dictionary(d, Cmd::AddType(kind, scope)).expect("a type id");
    dictionary(d, Cmd::RenameType(kind, t, name.into()));
    let mut ids = vec![];
    for v in values {
        let id = dictionary(d, Cmd::AddValue(kind, t, None)).expect("a value id");
        dictionary(
            d,
            Cmd::SetValue(kind, t, id, dict::ValueField::Short, v.to_string()),
        );
        ids.push(id);
    }
    (t, ids)
}

/// Codes tab: Assign, pick the value, Assign, Close.
fn assign_code(d: &mut Driver, i: usize, type_id: i64, value: i64) {
    select_row(d, i);
    d.click("detail_tab_codes");
    d.click("detail_codes_assign");
    d.frames(2);
    assert!(d.app.detail.picker.is_some(), "the code picker opens");
    d.app.detail.picker = Some(crate::details::Picker::Code {
        type_id: Some(type_id),
        selected: Some(value),
    });
    d.frames(1);
    d.click("picker_assign");
    d.click("picker_close");
    d.assert_clean("assign code");
}

fn code_names(d: &Driver) -> Vec<String> {
    let pid = d.app.project.model.project.proj_id;
    d.app.code_types(pid).into_iter().map(|(_, n)| n).collect()
}

fn band_labels(d: &Driver) -> Vec<String> {
    d.app
        .rows()
        .iter()
        .filter_map(|r| match r {
            grid::Row::Group(b) => Some(d.app.band_label(*b)),
            _ => None,
        })
        .collect()
}

/// Workflow 5: global, EPS and project activity codes and a user defined
/// field made, assigned, grouped by, filtered on, saved in a layout and
/// reopened, then exported; and the global codes seen from another project.
#[test]
fn workflow_5_codes_udfs_group_filter_and_layouts() {
    let mut d = fresh("wf5");
    let other = new_project(&mut d, "CD-600", "Code Neighbor", "04-Jan-27", "");
    let id = new_project(&mut d, "CD-500", "Code Trial", "04-Jan-27", "");
    show(&mut d, "activities");
    let acts: Vec<usize> = ["Demo north line", "Install north line", "Demo south line"]
        .iter()
        .map(|n| add_activity(&mut d, n, "2d"))
        .collect();

    d.home();
    d.click("tool_activity_codes");
    d.frames(2);
    assert!(d.app.dictionaries.codes.is_some(), "Activity Codes opens");
    // With no codes yet it opens with a notice to click Modify.
    if d.has("dictionary_notice_ok") {
        d.click("dictionary_notice_ok");
    }
    let (area, area_v) = code_type(&mut d, dict::Scope::Global, "Area", &["NORTH", "SOUTH"]);
    let (unit, unit_v) = code_type(&mut d, dict::Scope::Eps, "Unit", &["U1"]);
    let (phase, phase_v) = code_type(&mut d, dict::Scope::Project, "Phase", &["DEMO", "INST"]);
    d.click("codes_close");
    let names = code_names(&d);
    for n in ["Area", "Unit", "Phase"] {
        assert!(names.contains(&n.to_string()), "{n} in {names:?}");
    }
    for (i, a, p) in [(0, 0, 0), (1, 0, 1), (2, 1, 0)] {
        assign_code(&mut d, acts[i], area, area_v[a]);
        assign_code(&mut d, acts[i], phase, phase_v[p]);
    }
    assign_code(&mut d, acts[0], unit, unit_v[0]);

    // Enterprise > User Defined Fields: an activity text field, filled in.
    d.home();
    d.click("tool_udf");
    d.frames(2);
    if d.has("dictionary_notice_ok") {
        d.click("dictionary_notice_ok");
    }
    let udf = dictionary(&mut d, Cmd::AddUdf("TASK")).expect("a udf id");
    dictionary(&mut d, Cmd::UdfTitle(udf, "Permit No".into()));
    d.click("udf_close");
    d.app
        .edit_activity(
            acts[0],
            crate::details::setters::Edit::Udf {
                type_id: udf,
                value: crate::details::setters::UdfValue::Text("HW-17".into()),
            },
        )
        .unwrap();
    d.frames(2);
    let cells = drawn_cells(
        &mut d,
        acts[0],
        &["code:Area", "code:Phase", "udf:Permit No"],
    );
    assert_eq!(cells[0], "NORTH", "{cells:?}");
    assert_eq!(cells[1], "DEMO", "{cells:?}");

    // Group by Area: two bands, NORTH with two activities.
    d.app.grid.layout.group_sort.levels = vec![grid::layout::GroupLevel {
        column: "code:Area".into(),
        to_level: None,
        indent: true,
        interval: String::new(),
    }];
    d.app.drop_frame_cache();
    d.frames(2);
    let bands = band_labels(&d);
    assert!(bands.iter().any(|b| b.contains("NORTH")), "{bands:?}");
    assert!(bands.iter().any(|b| b.contains("SOUTH")), "{bands:?}");
    // A filter on Phase = DEMO leaves the two demolition rows.
    d.app.grid.user_filters.push(grid::filters::UserFilter {
        name: "Demo only".into(),
        match_all: true,
        rows: vec![grid::filters::Criterion {
            column: "code:Phase".into(),
            op: grid::filters::Op::Equals,
            value: "DEMO".into(),
            high: String::new(),
            level: 0,
        }],
    });
    d.app.grid.layout.filter.all_activities = false;
    d.app.grid.layout.filter.selected = vec!["user:Demo only".into()];
    d.app.drop_frame_cache();
    d.frames(2);
    assert_eq!(d.visible_activities().len(), 2, "two demolition activities");
    // Save the layout, change it, reopen it.
    d.app.save_layout_as("By Area").unwrap();
    d.app.grid.layout.group_sort.levels.clear();
    d.app.grid.layout.filter.all_activities = true;
    d.app.drop_frame_cache();
    d.frames(2);
    assert_eq!(d.visible_activities().len(), 3);
    d.app.open_layout("By Area", true).unwrap();
    d.frames(2);
    assert_eq!(
        d.visible_activities().len(),
        2,
        "the layout's filter is back"
    );
    assert!(
        band_labels(&d).iter().any(|b| b.contains("NORTH")),
        "grouping is back"
    );

    // Save and the XER round trip carry the codes and the field value.
    d.key_with(Key::S, ctrl());
    let xer = xer_round_trip(&mut d, "wf5.xer");
    let rows = |p: &ProjectData, t: &str| p.document.table(t).map_or(0, |t| t.rows.len());
    assert_eq!(rows(&xer, "ACTVTYPE"), 3);
    assert_eq!(rows(&xer, "TASKACTV"), 7);
    assert_eq!(rows(&xer, "UDFVALUE"), 1);
    // Undo walks it all back to the bare project.
    let steps = undo_to_new_project(&mut d);
    assert!(
        code_names(&d).is_empty(),
        "undo removes the codes: {:?}, {steps} undone, {}",
        code_names(&d),
        undo_state(&d)
    );
    assert_eq!(d.app.project.model.activities.len(), 0);
    d.redo_all();
    assert_eq!(code_names(&d).len(), 3);
    assert_eq!(d.app.open_set.active, Some(id));
    let _ = other;
}

/// Global and EPS dictionaries made while one project is open: the other
/// project in the same EPS folder should offer them too.
#[test]
fn workflow_5b_global_dictionaries_reach_every_project() {
    let mut d = fresh("wf5b");
    let first = new_project(&mut d, "GL-100", "First", "04-Jan-27", "");
    let second = new_project(&mut d, "GL-200", "Second", "04-Jan-27", "");
    assert_eq!(
        d.app.workspace().node_of(Some(first)),
        d.app.workspace().node_of(Some(second))
    );
    // In GL-200: a global and an EPS code, a calendar and a resource.
    d.home();
    d.click("tool_activity_codes");
    d.frames(2);
    if d.has("dictionary_notice_ok") {
        d.click("dictionary_notice_ok");
    }
    code_type(&mut d, dict::Scope::Global, "Area", &["NORTH"]);
    code_type(&mut d, dict::Scope::Eps, "Unit", &["U1"]);
    d.click("codes_close");
    d.click("tool_calendars");
    d.click("calendars_add");
    d.click("calendars_copy_select");
    d.click("calendars_close");
    let calendar = d.app.project.calendar_names.last().cloned().unwrap();
    show(&mut d, "resources");
    d.click("add");
    d.frames(2);
    d.key_with(Key::S, ctrl());
    // GL-100 opened afterwards.
    open_projects(&mut d, &[first]);
    let mut missing = vec![];
    let names = code_names(&d);
    for code in ["Area", "Unit"] {
        if !names.contains(&code.to_string()) {
            missing.push(format!("activity code {code} (codes here: {names:?})"));
        }
    }
    if !d.app.project.calendar_names.contains(&calendar) {
        missing.push(format!(
            "calendar {calendar:?} (calendars here: {:?})",
            d.app.project.calendar_names
        ));
    }
    if rsrc_ids(&d.app.project).is_empty() {
        missing.push("resource New Resource (no resources here)".into());
    }
    assert!(missing.is_empty(), "GL-100 does not offer: {missing:#?}");
}
// ------------------------------------------------------------------ 6

/// Activity dates by Activity ID, as text.
fn dates_by_code(p: &ProjectData) -> BTreeMap<String, (String, String)> {
    p.model
        .activities
        .iter()
        .map(|a| {
            (
                a.task_code.clone(),
                (date(p, a.early_start), date(p, a.early_finish)),
            )
        })
        .collect()
}

/// Contents without the project's own node (a copy renames it).
fn below_node(mut c: Contents) -> Contents {
    if !c.wbs.is_empty() {
        c.wbs.remove(0);
    }
    c
}

/// One edit, then undo and redo it: undo gives back exactly the state
/// before, redo exactly the state after.
fn undo_redo_check(d: &mut Driver, what: &str, edit: impl FnOnce(&mut Driver)) {
    let before = (d.app.project.model.clone(), d.app.project.details.clone());
    let doc_before = d.app.project.updated_document().text();
    edit(d);
    d.frames(2);
    let after = (d.app.project.model.clone(), d.app.project.details.clone());
    let doc_after = d.app.project.updated_document().text();
    assert!(after != before, "{what} changed something");
    d.key_with(Key::Z, ctrl());
    d.frames(2);
    assert!(
        (d.app.project.model.clone(), d.app.project.details.clone()) == before,
        "undo {what}: {}",
        crate::qa_sweep::model_diff(&before.0, &d.app.project.model)
    );
    assert_eq!(
        d.app.project.updated_document().text(),
        doc_before,
        "undo {what}: the XER text"
    );
    d.key_with(Key::Y, ctrl());
    d.frames(2);
    assert!(
        (d.app.project.model.clone(), d.app.project.details.clone()) == after,
        "redo {what}"
    );
    assert_eq!(
        d.app.project.updated_document().text(),
        doc_after,
        "redo {what}: the XER text"
    );
}

/// Workflow 6: an XER exported by OpenXER imported as a new project,
/// changed (with undo and redo of each change), exported and imported
/// again; then the project copied and pasted in the Projects view.
#[test]
fn workflow_6_import_change_export_reimport_and_copy() {
    let mut d = fresh("wf6");
    let sample_id = d.app.open_set.active.unwrap();
    let sample = d.app.project.clone();
    let first = d.dir.join("sample.xer");
    d.export(interchange::ExportFormat::Xer, &first);
    let before: Vec<i64> = d.app.workspace_ids();
    d.import(&first);
    let imported = d.app.open_set.active.unwrap();
    assert_ne!(
        imported, sample_id,
        "Create New makes a new workspace project"
    );
    assert!(!before.contains(&imported));
    assert_eq!(d.app.workspace_ids().len(), before.len() + 1);
    assert_eq!(
        contents(&d.app.project).activities,
        contents(&sample).activities
    );
    assert_eq!(d.app.project.model.links.len(), sample.model.links.len());
    assert_eq!(
        below_node(contents(&d.app.project)),
        below_node(contents(&sample))
    );
    assert_eq!(
        dates_by_code(&d.app.project),
        dates_by_code(&sample),
        "stored dates come in"
    );
    // The sample is untouched.
    assert_eq!(contents(&stored(&d, sample_id)), contents(&sample));

    // Changes, each undone and redone.
    show(&mut d, "activities");
    let i = d.visible_activities()[0];
    undo_redo_check(&mut d, "rename", |d| {
        cell_edit(d, i, "activity_name", "Imported and renamed")
    });
    let j = d
        .visible_activities()
        .into_iter()
        .find(|&k| {
            let a = &d.app.project.model.activities[k];
            !a.is_completed() && a.task_type == TaskType::Task && a.act_start.is_none()
        })
        .expect("a not-started task");
    undo_redo_check(&mut d, "duration", |d| {
        cell_edit(d, j, "remaining_duration", "9d")
    });
    let (p, s) = {
        let m = &d.app.project.model;
        let n = m.activities.len();
        (0..n)
            .flat_map(|p| (0..n).map(move |s| (p, s)))
            .find(|&(p, s)| {
                p < s
                    && !m
                        .links
                        .iter()
                        .any(|l| (l.pred == p && l.succ == s) || (l.pred == s && l.succ == p))
                    && m.activities[p].act_start.is_none()
                    && m.activities[s].act_start.is_none()
            })
            .expect("two unlinked activities")
    };
    undo_redo_check(&mut d, "link", |d| link(d, p, s));
    undo_redo_check(&mut d, "F9", |d| {
        d.f9();
    });
    let changed = d.app.project.clone();

    // Export and import again: the changes come along.
    let second = d.dir.join("changed.xer");
    d.export(interchange::ExportFormat::Xer, &second);
    d.import(&second);
    let again = d.app.open_set.active.unwrap();
    assert!(![sample_id, imported].contains(&again));
    assert_eq!(
        d.app.project.details[&d.app.project.model.activities[i].task_id].name,
        "Imported and renamed"
    );
    assert_eq!(d.app.project.model.links.len(), changed.model.links.len());
    assert_eq!(dates_by_code(&d.app.project), dates_by_code(&changed));
    assert_eq!(
        below_node(contents(&d.app.project)),
        below_node(contents(&changed))
    );

    // Projects view: copy the imported project and paste it under the root.
    projects_select(&mut d, imported);
    d.click("dock_copy");
    d.click("projects_row_0");
    d.click("dock_paste");
    d.click("copy_project_ok");
    d.frames(2);
    d.assert_clean("paste project");
    let ids = d.app.workspace_ids();
    let copy = *ids
        .iter()
        .find(|id| ![sample_id, imported, again].contains(id))
        .expect("the copy is in the workspace");
    let source = stored(&d, imported);
    let pasted = stored(&d, copy);
    assert_eq!(below_node(contents(&pasted)), below_node(contents(&source)));
    assert_eq!(pasted.model.links.len(), source.model.links.len());
    assert_ne!(
        pasted.model.project.name, source.model.project.name,
        "the copy gets its own ID"
    );
    // Opened, the copy is itself: its WBS has its own node only once.
    open_projects(&mut d, &[copy]);
    assert_eq!(
        below_node(contents(&d.app.project)),
        below_node(contents(&source))
    );
    assert_eq!(dates_by_code(&d.app.project), dates_by_code(&source));
    // Undo the paste and redo it, back in the project that was open when it
    // was made (its history is kept while another project is open).
    open_projects(&mut d, &[again]);
    show(&mut d, "projects");
    let n = d.app.workspace_ids().len();
    let ctx = d.ctx.clone();
    d.app.history_move(&ctx, false);
    d.frames(2);
    assert_eq!(
        d.app.workspace_ids().len(),
        n - 1,
        "undo in the source project removes the pasted copy"
    );
    d.app.history_move(&ctx, true);
    d.frames(2);
    assert_eq!(d.app.workspace_ids().len(), n, "redo pastes it again");
}
// ------------------------------------------------------------------ 7

/// Answer Yes to the Resources tab Remove question, and to the second one
/// when the assignment is the activity's primary resource.
fn answer_remove_assignment(d: &mut Driver) {
    d.click("remove_assignment_yes");
    d.frames(1);
    if d.has("remove_primary_yes") {
        d.click("remove_primary_yes");
    }
}

/// Delete something through the UI, check it is gone, undo (everything
/// comes back exactly, down to the XER text), redo (gone again), and undo
/// once more so the next check starts from the full project. The XER text
/// is compared byte for byte, GUIDs and create dates included.
fn delete_check(
    d: &mut Driver,
    what: &str,
    delete: impl FnOnce(&mut Driver),
    gone: impl Fn(&Driver) -> bool,
) {
    let before = d.app.project.updated_document().text();
    let model = d.app.project.model.clone();
    assert!(!gone(d), "{what} is there to delete");
    delete(d);
    d.frames(2);
    d.assert_clean(what);
    assert!(gone(d), "{what} is deleted: {}", d.app.status);
    d.key_with(Key::Z, ctrl());
    d.frames(2);
    assert!(!gone(d), "undo brings {what} back");
    assert!(
        d.app.project.model == model,
        "undo {what}: {}",
        crate::qa_sweep::model_diff(&model, &d.app.project.model)
    );
    let after = d.app.project.updated_document().text();
    if after != before {
        let (a, b): (Vec<&str>, Vec<&str>) = (before.lines().collect(), after.lines().collect());
        let diff: Vec<_> = a
            .iter()
            .filter(|l| !b.contains(l))
            .map(|l| format!("- {l}"))
            .chain(
                b.iter()
                    .filter(|l| !a.contains(l))
                    .map(|l| format!("+ {l}")),
            )
            .take(8)
            .collect();
        panic!("undo {what} does not restore the XER text exactly: {diff:#?}");
    }
    d.key_with(Key::Y, ctrl());
    d.frames(2);
    assert!(
        gone(d),
        "redo deletes {what} again: {:?} {}",
        d.app.error,
        d.app.status
    );
    d.key_with(Key::Z, ctrl());
    d.frames(2);
    assert!(!gone(d));
}

fn table_has(p: &ProjectData, table: &str, field: &str, value: &str) -> bool {
    p.document
        .table(table)
        .is_some_and(|t| t.rows.iter().any(|r| t.value(r, field) == value))
}

/// Workflow 7: everything workflows 1 to 5 make can be deleted where a user
/// looks for Delete, and undo restores it exactly.
#[test]
fn workflow_7_delete_and_undo_everything() {
    let mut d = fresh("wf7");
    let sample = d.app.open_set.active.unwrap();
    new_project(&mut d, "DL-700", "Delete Trial", "04-Jan-27", "");
    let node = root(&d);
    let area = add_wbs(&mut d, node, "AREA", "Area");
    show(&mut d, "activities");
    let a = add_activity(&mut d, "Keep me", "2d");
    let b = add_activity(&mut d, "Delete me", "3d");
    for i in [a, b] {
        d.app
            .edit_activity(i, crate::details::setters::Edit::Wbs(area))
            .unwrap();
    }
    link(&mut d, a, b);
    // A resource on activity b, a role, a code, a field and a calendar.
    show(&mut d, "resources");
    d.click("add");
    d.frames(2);
    let rsrc = d.app.views.resources.selected.unwrap();
    show(&mut d, "activities");
    select_row(&mut d, b);
    d.click("detail_tab_resources");
    d.click("detail_rsrc_add");
    pick_and_assign(&mut d, "(New Resource)");
    d.home();
    d.click("menu_bar_enterprise");
    d.click("menu_roles");
    d.click("roles_add");
    d.click("roles_close");
    d.home();
    d.click("tool_activity_codes");
    d.frames(2);
    if d.has("dictionary_notice_ok") {
        d.click("dictionary_notice_ok");
    }
    let (code, values) = code_type(&mut d, dict::Scope::Project, "Phase", &["DEMO"]);
    d.click("codes_close");
    assign_code(&mut d, a, code, values[0]);
    d.home();
    d.click("tool_udf");
    d.frames(2);
    if d.has("dictionary_notice_ok") {
        d.click("dictionary_notice_ok");
    }
    let udf = dictionary(&mut d, Cmd::AddUdf("TASK")).unwrap();
    d.click("udf_close");
    d.f9();
    d.key_with(Key::S, ctrl());
    let task_b = d.app.project.model.activities[b].task_id.to_string();

    // An activity: Edit > Delete, confirmed. Its link and assignment go too.
    delete_check(
        &mut d,
        "activity",
        |d| {
            show(d, "activities");
            select_row(d, b);
            d.click("menu_bar_edit");
            d.click("edit_delete");
            if d.has("delete_yes") {
                d.click("delete_yes");
            }
        },
        |d| !table_has(&d.app.project, "TASK", "task_id", &task_b),
    );
    // A resource assignment: Remove on the Resources tab.
    delete_check(
        &mut d,
        "assignment",
        |d| {
            show(d, "activities");
            select_row(d, b);
            d.click("detail_tab_resources");
            d.app.detail.rows[0] = Some(0);
            d.frames(1);
            d.click("detail_rsrc_remove");
            answer_remove_assignment(d);
        },
        |d| !table_has(&d.app.project, "TASKRSRC", "task_id", &task_b),
    );
    // A relationship: the Relationships tab is not needed; Unlink.
    delete_check(
        &mut d,
        "relationship",
        |d| {
            show(d, "activities");
            select_row(d, a);
            let pos = row_cell(d, b, "activity_id");
            d.press_at(pos, egui::PointerButton::Primary, ctrl());
            d.click("menu_bar_edit");
            d.click("edit_unlink");
        },
        |d| d.app.project.model.links.is_empty(),
    );
    // A code assignment, the code value's type, the field.
    delete_check(
        &mut d,
        "code type",
        |d| {
            d.home();
            d.click("tool_activity_codes");
            d.frames(2);
            let _ = d
                .app
                .run_dictionary(Cmd::DeleteType(dict::CodeKind::Activity, code));
            d.frames(2);
            if d.has("dictionary_confirm_yes") {
                d.click("dictionary_confirm_yes");
            }
            d.click("codes_close");
        },
        |d| {
            !table_has(
                &d.app.project,
                "ACTVTYPE",
                "actv_code_type_id",
                &code.to_string(),
            )
        },
    );
    delete_check(
        &mut d,
        "user defined field",
        |d| {
            d.home();
            d.click("tool_udf");
            d.frames(2);
            let _ = d.app.run_dictionary(Cmd::DeleteUdf(udf));
            d.frames(2);
            if d.has("dictionary_confirm_yes") {
                d.click("dictionary_confirm_yes");
            }
            d.click("udf_close");
        },
        |d| !table_has(&d.app.project, "UDFTYPE", "udf_type_id", &udf.to_string()),
    );
    // A role: Enterprise > Roles > Delete, Yes.
    delete_check(
        &mut d,
        "role",
        |d| {
            d.home();
            d.click("menu_bar_enterprise");
            d.click("menu_roles");
            d.click("roles_list_row_0");
            d.click("roles_delete");
            if d.has("enterprise_msg_yes") {
                d.click("enterprise_msg_yes");
            }
            d.click("roles_close");
        },
        |d| {
            d.app
                .project
                .document
                .table("ROLES")
                .is_none_or(|t| t.rows.is_empty())
        },
    );
    // A WBS element with its activities: Delete All.
    delete_check(
        &mut d,
        "WBS element",
        |d| {
            show(d, "wbs");
            wbs_click(d, area);
            d.click("dock_delete");
            d.click("wbs_confirm_delete_all");
            d.click("wbs_confirm_delete_all_yes");
        },
        |d| d.app.project.wbs.iter().all(|w| w.id != area),
    );
    // A calendar: Calendars > Add, then Delete, Yes.
    d.home();
    d.click("tool_calendars");
    d.click("calendars_add");
    d.click("calendars_copy_select");
    d.click("calendars_close");
    let calendars = d.app.project.model.calendars.len();
    delete_check(
        &mut d,
        "calendar",
        |d| {
            d.home();
            d.click("tool_calendars");
            d.frames(2);
            let name = d.app.project.calendar_names.last().cloned().unwrap();
            assert!(d.click_text(&name), "{name} is listed");
            d.click("calendars_delete");
            d.click("calendars_delete_yes");
            d.click("calendars_close");
        },
        |d| d.app.project.model.calendars.len() < calendars,
    );
    // The resource, once nothing uses it: Resources view, Delete.
    d.home();
    select_row(&mut d, b);
    d.click("detail_tab_resources");
    d.app.detail.rows[0] = Some(0);
    d.frames(1);
    d.click("detail_rsrc_remove");
    answer_remove_assignment(&mut d);
    delete_check(
        &mut d,
        "resource",
        |d| {
            show(d, "resources");
            d.app.views.resources.selected = Some(rsrc);
            d.frames(1);
            d.click("dock_delete");
        },
        |d| !rsrc_ids(&d.app.project).contains(&rsrc),
    );
    // The project itself: close it, delete it in the Projects view, undo.
    d.key_with(Key::S, ctrl());
    let id = d.app.open_set.active.unwrap();
    let kept = stored(&d, d.app.open_set.active.unwrap())
        .updated_document()
        .text();
    open_projects(&mut d, &[sample]);
    projects_select(&mut d, id);
    d.click("dock_delete");
    d.click("delete_project_yes");
    d.frames(2);
    assert!(d.app.store.load(id).is_err(), "the project is deleted");
    d.key_with(Key::Z, ctrl());
    d.frames(2);
    assert_eq!(
        stored(&d, id).updated_document().text(),
        kept,
        "undo restores the project exactly"
    );
}
// ------------------------------------------------------------------ bugs

/// An activity added in the app has no TASK row in the project's document
/// until the project is reloaded, so every export and every save writes it
/// with a new GUID (and a new create date).
#[test]
fn bug_3_a_new_activity_keeps_its_guid_across_exports() {
    let mut d = fresh("bug3");
    new_project(&mut d, "GU-100", "Guid Trial", "04-Jan-27", "");
    show(&mut d, "activities");
    add_activity(&mut d, "Hold my GUID", "2d");
    let one = d.app.project.updated_document().text();
    let two = d.app.project.updated_document().text();
    assert_eq!(one, two, "two exports of an unchanged project differ");
    d.key_with(Key::S, ctrl());
    let id = d.app.open_set.active.unwrap();
    let saved = |d: &Driver| d.app.store.source(id).unwrap().2;
    let first = saved(&d);
    d.app.dirty = true;
    d.key_with(Key::S, ctrl());
    assert_eq!(saved(&d), first, "two saves of an unchanged project differ");
}

/// A new activity shows no Start, Finish or bar until F9.
#[test]
fn bug_4_a_new_activity_is_dated_at_the_data_date() {
    let mut d = fresh("bug4");
    new_project(&mut d, "ND-100", "No Dates", "04-Jan-27", "");
    show(&mut d, "activities");
    let i = add_activity(&mut d, "Where is my bar", "3d");
    d.frames(2);
    let p = &d.app.project;
    let a = &p.model.activities[i];
    assert_eq!(
        date(p, a.early_start),
        "2027-01-04 08:00",
        "starts on the data date"
    );
    assert_eq!(date(p, a.early_finish), "2027-01-06 17:00");
    assert!(
        d.app.bar_geometry.iter().any(|b| b.task_id == a.task_id),
        "the Gantt draws its bar"
    );
}

/// File > New's step heads the new project's history: Ctrl+Z there removes
/// the new project and opens the one File > New was started from, whose
/// Redo adds the new project again (`session_history`).
#[test]
fn bug_5_new_project_can_be_undone() {
    let mut d = fresh("bug5");
    let sample = d.app.open_set.active.unwrap();
    let before = d.app.workspace_ids().len();
    let id = new_project(&mut d, "UN-100", "Undo Me", "04-Jan-27", "");
    assert_eq!(d.app.workspace_ids().len(), before + 1);
    // The first Ctrl+Z after Finish removes the project.
    assert!(
        d.app.history.counts().0 > 0,
        "Edit > Undo is offered after File > New"
    );
    d.key_with(Key::Z, ctrl());
    d.frames(2);
    assert_eq!(
        d.app.workspace_ids().len(),
        before,
        "undo removes the new project"
    );
    assert_eq!(d.app.open_set.active, Some(sample), "back in the sample");
    d.assert_clean("undo new project");
    assert!(d.app.history.counts().1 > 0, "Redo is offered");
    d.key_with(Key::Y, ctrl());
    d.frames(2);
    assert_eq!(
        d.app.workspace_ids().len(),
        before + 1,
        "redo adds it again"
    );
    assert!(
        d.app.taken_project_ids().iter().any(|t| t == "UN-100"),
        "{:?}",
        d.app.taken_project_ids()
    );
    assert_eq!(d.app.open_set.active, Some(id), "redo opens it again");
    // Work in it, undo everything (the project goes), redo everything (the
    // project and the work come back).
    show(&mut d, "activities");
    add_activity(&mut d, "Survive undo", "2d");
    let n = d.undo_all();
    assert!(n >= 2);
    assert_eq!(d.app.open_set.active, Some(sample));
    assert_eq!(d.app.workspace_ids().len(), before);
    d.redo_all();
    assert_eq!(d.app.open_set.active, Some(id));
    assert!(
        d.app
            .project
            .details
            .values()
            .any(|a| a.name == "Survive undo"),
        "the work is redone"
    );
    d.assert_clean("redo all");
    // Projects opened and closed keep their own history: back in the
    // sample, its step that added UN-100 is still there to undo.
    open_projects(&mut d, &[sample]);
    assert!(d.app.history.undo_external().is_some());
}

/// One of every record the app makes (activity, WBS element, relationship,
/// resource, assignment, role, code type and value, code assignment, UDF and
/// UDF value, calendar, notebook, document, expense, risk, issue): two
/// exports, two saves, an undo and redo, an activity delete undone and a
/// reopen all write the same XER text, byte for byte. A record made this
/// session gets its GUID and create date once, when it is made.
#[test]
fn records_made_in_the_app_export_the_same_twice() {
    let mut d = fresh("same_twice");
    new_project(&mut d, "ST-100", "Same Twice", "04-Jan-27", "");
    let node = root(&d);
    let area = add_wbs(&mut d, node, "AREA", "Area");
    show(&mut d, "activities");
    let a = add_activity(&mut d, "First", "2d");
    let b = add_activity(&mut d, "Second", "3d");
    for i in [a, b] {
        d.app
            .edit_activity(i, crate::details::setters::Edit::Wbs(area))
            .unwrap();
    }
    link(&mut d, a, b);
    show(&mut d, "resources");
    d.click("add");
    d.frames(2);
    show(&mut d, "activities");
    select_row(&mut d, b);
    d.click("detail_tab_resources");
    d.click("detail_rsrc_add");
    pick_and_assign(&mut d, "(New Resource)");
    d.home();
    d.click("menu_bar_enterprise");
    d.click("menu_roles");
    d.click("roles_add");
    d.click("roles_close");
    d.home();
    d.click("tool_activity_codes");
    d.frames(2);
    if d.has("dictionary_notice_ok") {
        d.click("dictionary_notice_ok");
    }
    let (code, values) = code_type(&mut d, dict::Scope::Project, "Phase", &["DEMO"]);
    d.click("codes_close");
    assign_code(&mut d, a, code, values[0]);
    d.home();
    d.click("tool_udf");
    d.frames(2);
    if d.has("dictionary_notice_ok") {
        d.click("dictionary_notice_ok");
    }
    let udf = dictionary(&mut d, Cmd::AddUdf("TASK")).unwrap();
    d.click("udf_close");
    d.app
        .edit_activity(
            a,
            crate::details::setters::Edit::Udf {
                type_id: udf,
                value: crate::details::setters::UdfValue::Text("HW-17".into()),
            },
        )
        .unwrap();
    d.home();
    d.click("tool_calendars");
    d.click("calendars_add");
    d.click("calendars_copy_select");
    d.click("calendars_close");
    // A pasted copy of activity a.
    show(&mut d, "activities");
    select_row(&mut d, a);
    let count = d.app.project.model.activities.len();
    let output = d.frame_with(vec![egui::Event::Copy], ctrl());
    let copied = output
        .platform_output
        .commands
        .iter()
        .find_map(|c| match c {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
        .expect("Copy puts the rows on the clipboard");
    let _ = d.frame_with(vec![egui::Event::Paste(copied)], ctrl());
    if d.has("copy_options_ok") {
        d.click("copy_options_ok");
    }
    d.frames(2);
    assert_eq!(d.app.project.model.activities.len(), count + 1, "pasted");
    // The record views' and tabs' Add commands.
    let task = d.app.project.model.activities[a].task_id;
    let pid = d.app.project.model.project.proj_id;
    let p = &mut d.app.project;
    let memo = records::next_id(&p.document, "TASKMEMO", "memo_id");
    records::add_row(
        &mut p.document,
        "TASKMEMO",
        &[
            ("memo_id", memo.to_string()),
            ("task_id", task.to_string()),
            ("memo_type_id", "1".into()),
            ("proj_id", pid.to_string()),
            ("task_memo", records::text_to_memo("Note")),
        ],
    );
    views::documents::add_document(p, None);
    views::expenses::add_expense(p, task);
    views::risks::add_risk(p, "2027-01-04");
    views::issues::add_issue(p, "2027-01-04");
    d.app.touch_records();
    d.frames(2);
    d.assert_clean("records");
    let text = d.app.project.updated_document().text();
    for table in [
        "TASK", "PROJWBS", "TASKPRED", "RSRC", "TASKRSRC", "ROLES", "ACTVTYPE", "ACTVCODE",
        "TASKACTV", "UDFTYPE", "UDFVALUE", "CALENDAR", "TASKMEMO", "DOCUMENT", "PROJCOST",
        "PROJISSU",
    ] {
        assert!(
            text.lines().any(|l| l.trim_end() == format!("%T\t{table}")),
            "the export carries {table}"
        );
    }

    let export = |d: &Driver| d.app.project.updated_document().text();
    let first = export(&d);
    assert_eq!(export(&d), first, "two exports differ");
    let path = d.dir.join("same_twice.xer");
    d.export(interchange::ExportFormat::Xer, &path);
    let file = std::fs::read(&path).unwrap();
    d.export(interchange::ExportFormat::Xer, &path);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        file,
        "two File > Export differ"
    );
    d.key_with(Key::S, ctrl());
    let id = d.app.open_set.active.unwrap();
    let saved = |d: &Driver| d.app.store.source(id).unwrap().2;
    let stored_text = saved(&d);
    d.app.dirty = true;
    d.key_with(Key::S, ctrl());
    assert_eq!(saved(&d), stored_text, "two saves differ");
    assert_eq!(export(&d), first, "a save changed the export");

    // Undo and redo an edit, and undo a delete of a new activity.
    cell_edit(&mut d, a, "activity_name", "First renamed");
    d.key_with(Key::Z, ctrl());
    d.frames(2);
    assert_eq!(export(&d), first, "undo of a rename");
    d.key_with(Key::Y, ctrl());
    d.frames(2);
    d.key_with(Key::Z, ctrl());
    d.frames(2);
    assert_eq!(export(&d), first, "undo after redo of a rename");
    show(&mut d, "activities");
    let count = d.app.project.model.activities.len();
    select_row(&mut d, b);
    d.click("menu_bar_edit");
    d.click("edit_delete");
    if d.has("delete_yes") {
        d.click("delete_yes");
    }
    d.frames(2);
    assert_eq!(d.app.project.model.activities.len(), count - 1, "deleted");
    d.key_with(Key::Z, ctrl());
    d.frames(2);
    assert_eq!(export(&d), first, "undo of an activity delete");

    // Reopened from the workspace, the project writes the same text.
    d.key_with(Key::S, ctrl());
    let reopened = stored(&d, id).updated_document().text();
    assert_eq!(reopened, first, "the reopened project");
}

// ------------------------------------------------------------------ 8

/// Tools > Schedule..., Options...: on the General tab make open-ended
/// activities critical and define critical activities through the dialog's
/// own controls (Longest Path, or Total Float less than or equal to
/// `threshold` typed into its field); Close and Yes, then Schedule.
fn schedule_with_critical(d: &mut Driver, threshold: Option<&str>) {
    d.home();
    d.open_menu("menu_bar_tools");
    d.click("menu_schedule");
    assert!(
        d.app.sched.schedule_open,
        "Tools > Schedule... opens the Schedule dialog"
    );
    d.click("schedule_options");
    assert!(d.app.show_schedule, "Options... opens Schedule Options");
    d.click("options_tab_0");
    let open_ends = |d: &Driver| {
        d.app
            .draft_options
            .as_ref()
            .expect("Schedule Options has a draft")
            .open_end_uses_early
    };
    if !open_ends(d) {
        d.click("options_open_critical");
    }
    assert!(open_ends(d), "open-ended activities are made critical");
    match threshold {
        None => {
            d.click("options_critical_longest");
            assert!(d.app.sched.critical_longest, "Longest Path is picked");
        }
        Some(text) => {
            d.click("options_critical_float");
            assert!(!d.app.sched.critical_longest, "Total Float is picked");
            type_into(d, "options_critical_hours", text);
        }
    }
    schedule_options_close::close_schedule_options(d, Some("Yes"));
    assert!(
        !d.app.show_schedule,
        "Close and Yes close Schedule Options: {:?}",
        d.app.error
    );
    assert!(d.app.sched.schedule_open, "the Schedule dialog stays up");
    let runs = d.app.schedule_runs;
    d.click("schedule_run");
    if let Some(e) = d.app.error.take() {
        panic!("Schedule failed: {e}");
    }
    assert_eq!(d.app.schedule_runs, runs + 1, "Schedule ran once");
    assert!(!d.app.sched.schedule_open);
    d.assert_clean("schedule");
}

/// The activity table showing Activity ID, Total Float and Critical, read as
/// drawn: each activity row's Critical cell text.
fn drawn_critical(d: &mut Driver) -> BTreeMap<usize, String> {
    d.home();
    d.app.grid.layout.columns = ["activity_id", "total_float", "critical"]
        .into_iter()
        .map(|c| grid::layout::ColumnSetting::new(c, 110.))
        .collect();
    d.app.grid.layout.filter.all_activities = true;
    // Room for every row under the taller (12 and 11 pt) band rows.
    let before = d.screen;
    d.screen.y = before.y.max(1400.);
    d.app.drop_frame_cache();
    d.frames(3);
    let texts = d.texts();
    let screen = d.screen;
    let rows = d.visible_activities();
    assert_eq!(
        rows.len(),
        d.app.project.model.activities.len(),
        "every activity is listed"
    );
    let drawn: BTreeMap<usize, String> = rows
        .into_iter()
        .map(|i| {
            let at = row_cell(d, i, "critical");
            assert!(
                at.y > 0. && at.y < screen.y,
                "activity {i}'s row is on screen ({at:?})"
            );
            let text = texts
                .iter()
                .filter(|(_, r)| {
                    r.contains(at)
                        || (r.top() <= at.y
                            && r.bottom() >= at.y
                            && (r.center().x - at.x).abs() < 55.)
                })
                .map(|(t, _)| t.clone())
                .next()
                .unwrap_or_default();
            (i, text)
        })
        .collect();
    d.screen = before;
    drawn
}

/// The table's Critical column, the Critical filter and the Gantt bars'
/// critical flag agree for every activity; returns the table's answer.
fn table_filter_gantt_agree(d: &mut Driver, step: &str) -> BTreeMap<usize, bool> {
    let drawn = drawn_critical(d);
    let table: BTreeMap<usize, bool> = drawn
        .iter()
        .map(|(&i, text)| {
            assert!(
                text == "Yes" || text == "No",
                "{step}: activity {i}'s Critical cell reads {text:?}"
            );
            (i, text == "Yes")
        })
        .collect();
    // The Critical filter leaves exactly the rows the column marks.
    d.app.grid.layout.filter.all_activities = false;
    d.app.grid.layout.filter.selected = vec![grid::filters::Standard::Critical.key()];
    d.app.drop_frame_cache();
    d.frames(2);
    let mut filtered = d.visible_activities();
    filtered.sort();
    let marked: Vec<usize> = table.iter().filter(|(_, c)| **c).map(|(i, _)| *i).collect();
    assert_eq!(filtered, marked, "{step}: the Critical filter");
    d.app.grid.layout.filter.all_activities = true;
    d.app.drop_frame_cache();
    d.frames(2);
    // The Gantt chart picks the critical bar from the same fact.
    let ix = d.app.shared_index();
    for (k, a) in d.app.project.model.activities.iter().enumerate() {
        assert_eq!(
            d.app.subject(&ix, k, a).critical,
            table[&k],
            "{step}: Gantt critical flag of {}",
            a.task_code
        );
    }
    table
}

/// File > Export, Spreadsheet in the OpenXER layout, then the workbook read
/// back: each activity's Critical cell and hidden critical flag, by
/// Activity ID.
fn exported_critical(d: &mut Driver, name: &str) -> BTreeMap<String, (bool, bool)> {
    use openxer_interchange::xlsx_export::{ACTIVITY_SHEET, CRITICAL_HEADER};
    use openxer_interchange::xlsx_import::{read_workbook, Cell};
    let path = d.dir.join(name);
    d.export(interchange::ExportFormat::Modern, &path);
    let wb = read_workbook(&path).unwrap();
    let sheet = &wb.sheets[wb.sheet(ACTIVITY_SHEET).expect("an Activities sheet")];
    let column = |title: &str| {
        sheet.rows[0]
            .iter()
            .position(|c| c.text() == title)
            .unwrap_or_else(|| panic!("the workbook has a {title} column"))
    };
    let (id, critical, hidden) = (
        column("Activity ID"),
        column("Critical"),
        column(CRITICAL_HEADER),
    );
    let flag = |c: Option<&Cell>| match c {
        Some(Cell::Bool(b)) => *b,
        other => panic!("a critical cell holds {other:?}"),
    };
    let codes: BTreeSet<String> = d
        .app
        .project
        .model
        .activities
        .iter()
        .map(|a| a.task_code.clone())
        .collect();
    sheet.rows[1..]
        .iter()
        .filter_map(|r| {
            let code = r.get(id)?.text();
            codes.contains(&code).then(|| {
                let pair = (flag(r.get(critical)), flag(r.get(hidden)));
                (code, pair)
            })
        })
        .collect()
}

/// The workbook's Critical column equals the table's, activity by activity.
fn export_matches_table(d: &mut Driver, table: &BTreeMap<usize, bool>, name: &str) {
    let exported = exported_critical(d, name);
    let p = &d.app.project;
    assert_eq!(exported.len(), p.model.activities.len(), "every activity");
    for (&i, &critical) in table {
        let code = &p.model.activities[i].task_code;
        assert_eq!(
            exported[code],
            (critical, critical),
            "{name}: {code} is {} in the table",
            if critical { "critical" } else { "not critical" }
        );
    }
}

/// Tools > Report Wizard, Relationships with the Critical column added, run
/// to the screen: each relationship's (predecessor, successor, Critical).
fn relationships_report(d: &mut Driver) -> Vec<(String, String, String)> {
    d.home();
    d.open_menu("menu_bar_tools");
    d.click("menu_report_wizard");
    assert!(d.app.reports.wizard.is_some(), "the Report Wizard opens");
    d.click("wizard_subject_relationships");
    d.click("wizard_next");
    // Critical is the first column the subject does not show by default.
    d.click("wizard_available_row_0");
    d.click("wizard_add");
    let w = d.app.reports.wizard.as_ref().unwrap();
    assert_eq!(
        w.def.columns.last().map(String::as_str),
        Some("critical"),
        "Critical is added"
    );
    d.click("wizard_next");
    d.click("wizard_next");
    d.click("wizard_output_0");
    let before = d.app.reports.tabs.ids();
    d.click("wizard_run");
    d.assert_clean("run report");
    let id = d
        .app
        .reports
        .tabs
        .ids()
        .into_iter()
        .find(|id| !before.contains(id))
        .expect("Run Report opens a report tab");
    if d.app.reports.wizard.is_some() {
        d.click("wizard_cancel");
    }
    let tab = d.app.reports.tabs.get(id).unwrap();
    let crate::reports::report_tab::Doc::Sheet(report) = &tab.doc else {
        panic!("a wizard report is a sheet");
    };
    let (_, table) = report.tables().next().expect("a table");
    let column = |title: &str| {
        table
            .columns
            .iter()
            .position(|c| c.title == title)
            .unwrap_or_else(|| panic!("the report has a {title} column"))
    };
    let (pred, succ, critical) = (
        column("Predecessor"),
        column("Successor"),
        column("Critical"),
    );
    table
        .rows
        .iter()
        .map(|r| {
            (
                r.cells[pred].text().to_string(),
                r.cells[succ].text().to_string(),
                r.cells[critical].text().to_string(),
            )
        })
        .collect()
}

/// Workflow 8: the project's critical definition, set in Schedule Options,
/// holds the same everywhere a user sees "critical": the table's Critical
/// column, the Critical filter, the Gantt bars, the spreadsheet export and
/// the Relationships report. First Longest Path, with an activity that has
/// zero float but is off the longest path; then Total Float less than or
/// equal to 16 hours, with activities at 8 hours (critical) and 17 hours
/// (not).
#[test]
fn workflow_8_critical_definition_reaches_table_export_and_reports() {
    let mut d = fresh("critical_definition");
    show(&mut d, "activities");
    // Three activities that start at the data date feed one open-ended
    // activity, which Schedule Options makes critical: the 24h one has no
    // float, the 16h one 8h and the 7h one 17h. None of them finishes the
    // project, so none is on the longest path.
    let a = add_activity(&mut d, "Set pump skid", "24h");
    let b = add_activity(&mut d, "Grout pump base", "16h");
    let c = add_activity(&mut d, "Pull pump power cable", "7h");
    let e = add_activity(&mut d, "Connect pump suction and discharge", "8h");
    for pred in [a, b, c] {
        link(&mut d, pred, e);
    }

    // Longest Path.
    schedule_with_critical(&mut d, None);
    assert_eq!(d.app.critical_definition(), (true, 0.));
    let act = |d: &Driver, i: usize| d.app.project.model.activities[i].clone();
    let off_path = act(&d, a);
    assert!(
        off_path.total_float.is_some_and(|f| f <= 0) && !off_path.driving_path,
        "{} has no float and is off the longest path: {:?} {}",
        off_path.task_code,
        off_path.total_float,
        off_path.driving_path
    );
    let table = table_filter_gantt_agree(&mut d, "longest path");
    assert!(
        !table[&a],
        "zero float off the longest path is not critical by this project's rule"
    );
    assert!(
        table.values().any(|c| *c),
        "the longest path is marked critical"
    );
    for (&i, &critical) in &table {
        let x = act(&d, i);
        assert_eq!(
            critical,
            !x.is_completed() && x.driving_path,
            "{}",
            x.task_code
        );
    }
    export_matches_table(&mut d, &table, "critical-longest-path.xlsx");
    // The Relationships report marks a relationship critical when both ends
    // are critical in the table.
    let code_row: BTreeMap<String, usize> = d
        .app
        .project
        .model
        .activities
        .iter()
        .enumerate()
        .map(|(i, x)| (x.task_code.clone(), i))
        .collect();
    let rows = relationships_report(&mut d);
    assert_eq!(
        rows.len(),
        d.app.project.model.links.len(),
        "every relationship"
    );
    for (pred, succ, critical) in &rows {
        let want = table[&code_row[pred]] && table[&code_row[succ]];
        assert_eq!(
            critical,
            if want { "Yes" } else { "No" },
            "relationship {pred} -> {succ}"
        );
    }
    let (new_pred, new_succ) = (act(&d, a).task_code, act(&d, e).task_code);
    assert!(
        rows.iter()
            .any(|(p, s, _)| *p == new_pred && *s == new_succ),
        "the new relationships are listed"
    );

    // Total Float less than or equal to 16 hours.
    schedule_with_critical(&mut d, Some("16h"));
    assert_eq!(d.app.critical_definition(), (false, 16.));
    assert_eq!(act(&d, b).total_float, Some(8 * 60), "8 hours of float");
    assert_eq!(act(&d, c).total_float, Some(17 * 60), "17 hours of float");
    let table = table_filter_gantt_agree(&mut d, "16 hour threshold");
    assert!(table[&b], "8 hours of float is within 16 hours");
    assert!(!table[&c], "17 hours of float is over 16 hours");
    assert!(table[&a] && table[&e], "no float is critical");
    export_matches_table(&mut d, &table, "critical-threshold.xlsx");
}

#[path = "qa_workflows_update.rs"]
mod baseline_update;
