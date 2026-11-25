//! Workflow: the second look pass over Activity Details and Project
//! Details, on a small tank job built here (an eight-hour Monday to Friday
//! calendar, a PROJECT row with no defaults of its own, three activities
//! where Paint tank shell carries one day of total float, no risks, no
//! documents, no roles and no step templates).
//!
//! - Project Details shows the values in use where the project stores
//!   none: the new activity defaults, the WBS separator, the summarize
//!   level and the assignment defaults; the number boxes are right-aligned.
//! - Settings tab: Define Critical Activities is edited there, writing the
//!   columns the table, bars and filters read; each change is one undo step.
//! - The add buttons of the details tabs stay enabled with nothing to pick
//!   and open their pickers empty.
//! - The General tab's WBS `...` opens Select WBS; Assign moves the
//!   activity, one undo step.
//! - The Predecessors and Successors tabs start with their own columns; the
//!   Relationships tab keeps its list.
//! - The notes panes name what is not selected yet.
use super::*;
use crate::details::{DetailTab, Picker};
use crate::views::record_view::PickTarget;
use openxer_xer::critical::{critical_definition, is_critical};

fn line(fields: &[&str]) -> String {
    fields.join("\t")
}

fn week() -> String {
    let days: String = (1..=7)
        .map(|d| {
            if (2..=6).contains(&d) {
                format!("(0||{d}()((0||0(s|08:00|f|16:00)())))")
            } else {
                format!("(0||{d}()())")
            }
        })
        .collect();
    format!("(0||CalendarData()((0||DaysOfWeek()({days}))(0||Exceptions()())))")
}

/// A1000 Set tank base (5 days) drives A1010 Erect tank shell (4 days) and
/// A1020 Paint tank shell (3 days, one day of float).
fn tank_job() -> ProjectData {
    let mut s = "ERMHDR\t25.12\t2026-11-23\tProject\tOpenXER\tOpenXER\tdbxDatabaseNoName\tProject Management\tUSD\n".to_string();
    let mut table = |name: &str, fields: &[&str], rows: &[String]| {
        s += &format!("%T\t{name}\n%F\t{}\n", fields.join("\t"));
        for r in rows {
            s += &format!("%R\t{r}\n");
        }
    };
    let fields = [
        "proj_id",
        "proj_short_name",
        "last_recalc_date",
        "plan_start_date",
        "clndr_id",
        "critical_path_type",
        "critical_drtn_hr_cnt",
    ];
    let project = line(&[
        "7",
        "TNK",
        "2026-11-23 08:00",
        "2026-11-23 00:00",
        "1",
        "CT_TotFloat",
        "0",
    ]);
    table("PROJECT", &fields, &[project]);
    let calendar = line(&["1", "Day shift", &week(), "8", "40"]);
    let fields = [
        "clndr_id",
        "clndr_name",
        "clndr_data",
        "day_hr_cnt",
        "week_hr_cnt",
    ];
    table("CALENDAR", &fields, &[calendar]);
    let fields = [
        "wbs_id",
        "proj_id",
        "parent_wbs_id",
        "wbs_short_name",
        "wbs_name",
        "seq_num",
        "proj_node_flag",
    ];
    let wbs = [
        line(&["70", "7", "", "TNK", "Tank farm", "1", "Y"]),
        line(&["71", "7", "70", "CIV", "Civil", "1", "N"]),
        line(&["72", "7", "70", "MEC", "Mechanical", "2", "N"]),
    ];
    table("PROJWBS", &fields, &wbs);
    let fields = [
        "task_id",
        "proj_id",
        "wbs_id",
        "clndr_id",
        "task_code",
        "task_name",
        "task_type",
        "status_code",
        "duration_type",
        "complete_pct_type",
        "remain_drtn_hr_cnt",
        "target_drtn_hr_cnt",
    ];
    let task = |id: &str, code: &str, name: &str, hours: &str| {
        line(&[
            id,
            "7",
            "71",
            "1",
            code,
            name,
            "TT_Task",
            "TK_NotStart",
            "DT_FixedDUR2",
            "CP_Drtn",
            hours,
            hours,
        ])
    };
    let tasks = [
        task("700", "A1000", "Set tank base", "40"),
        task("701", "A1010", "Erect tank shell", "32"),
        task("702", "A1020", "Paint tank shell", "24"),
    ];
    table("TASK", &fields, &tasks);
    let fields = [
        "task_pred_id",
        "task_id",
        "pred_task_id",
        "proj_id",
        "pred_proj_id",
        "pred_type",
        "lag_hr_cnt",
    ];
    let links = [
        line(&["1", "701", "700", "7", "7", "PR_FS", "0"]),
        line(&["2", "702", "700", "7", "7", "PR_FS", "0"]),
    ];
    table("TASKPRED", &fields, &links);
    s += "%E\n";
    let doc = Document::parse(&s).unwrap();
    let mut p = ProjectData::from_document(doc, 7, "tank job".into()).unwrap();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

/// A PROJECT column of the open project.
fn project_column(d: &Driver, column: &str) -> String {
    let t = d.app.project.document.table("PROJECT").unwrap();
    t.value(&t.rows[0], column).to_string()
}

/// Projects view, tab `name` of the open project.
fn project_tab(d: &mut Driver, name: &str) {
    let id = d.app.open_set.active.expect("the open project has a row");
    projects_select(d, id);
    let tab = views::projects::TABS
        .iter()
        .position(|t| *t == name)
        .unwrap();
    d.click(&format!("projects_tab_{tab}"));
    assert_eq!(d.app.views.projects.tab, tab);
    d.frames(2);
}

/// The texts drawn inside the rendered control `key`, with their rects.
fn texts_in(d: &mut Driver, key: &str) -> Vec<(String, egui::Rect)> {
    d.frames(2);
    let r = d.rect(key);
    d.texts()
        .into_iter()
        .filter(|(_, t)| r.contains(t.center()))
        .collect()
}

fn text_in(d: &mut Driver, key: &str) -> String {
    texts_in(d, key)
        .into_iter()
        .map(|(s, _)| s)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Click a Details tab by its strip action, adding the tab when the layout
/// hides it.
fn open_tab(d: &mut Driver, tab: DetailTab, action: &str) {
    d.frames(2);
    if !d.has(action) {
        d.app.show_detail_tab(tab);
        d.frames(2);
    }
    d.click(action);
    d.frames(2);
    assert_eq!(d.app.detail.tab, tab, "{action} shows its tab");
}

fn shown(d: &mut Driver, text: &str) -> bool {
    d.texts().iter().any(|(t, _)| t == text)
}

#[test]
fn workflow_project_details_show_the_values_in_use() {
    let mut d = Driver::new("wf-look-project-values", tank_job());
    // Defaults: what a new activity takes, though the project stores none.
    project_tab(&mut d, "Defaults");
    assert_eq!(project_column(&d, "def_duration_type"), "");
    for (key, text) in [
        ("project_def_duration_type", "Fixed Duration & Units"),
        ("project_def_complete_pct_type", "Duration"),
        ("project_def_task_type", "Task Dependent"),
        ("project_field_task_code_prefix", "A"),
        ("project_field_task_code_base", "1000"),
        ("project_field_task_code_step", "10"),
    ] {
        assert_eq!(text_in(&mut d, key), text, "{key} shows the value in use");
    }
    // Numbers sit at the right of their boxes, the prefix at the left.
    for (key, right) in [
        ("project_field_task_code_base", true),
        ("project_field_task_code_step", true),
        ("project_field_task_code_prefix", false),
    ] {
        let field = d.rect(key);
        let (_, text) = texts_in(&mut d, key).into_iter().next().unwrap();
        assert_eq!(text.center().x > field.center().x, right, "{key} alignment");
    }
    // Settings: the WBS separator and the summarize level.
    project_tab(&mut d, "Settings");
    assert_eq!(text_in(&mut d, "project_field_name_sep_char"), ".");
    assert!(
        text_in(&mut d, "project_sum_level").contains('2'),
        "summarize level"
    );
    // Resources: the rate type new assignments take.
    project_tab(&mut d, "Resources");
    assert_eq!(text_in(&mut d, "project_rate_type"), "Price / Unit");
    // Looking stores nothing.
    assert_eq!(project_column(&d, "def_duration_type"), "");
    assert_eq!(project_column(&d, "name_sep_char"), "");
    assert_eq!(d.app.history.counts().0, 0, "nothing to undo");
    d.assert_clean("project values");
}

#[test]
fn workflow_critical_definition_is_edited_on_the_settings_tab() {
    let mut d = Driver::new("wf-look-critical", tank_job());
    let paint = index_of(&d.app.project, "A1020");
    let critical = |d: &Driver| {
        let p = &d.app.project;
        is_critical(&p.model.activities[paint], critical_definition(p))
    };
    assert_eq!(critical_definition(&d.app.project), (false, 0.));
    assert!(!critical(&d), "one day of float is not critical at zero");
    project_tab(&mut d, "Settings");
    let steps = d.app.history.counts().0;
    // Total float of one day or less.
    let one_day = crate::format::duration(8 * 60, crate::format::project_periods(&d.app.project));
    type_into(&mut d, "project_critical_float_value", &one_day);
    d.key(Key::Enter);
    d.frames(2);
    d.assert_clean("critical float");
    assert_eq!(project_column(&d, "critical_drtn_hr_cnt"), "8");
    assert_eq!(critical_definition(&d.app.project), (false, 8.));
    assert!(critical(&d), "the table, bars and filters follow");
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    // Longest Path.
    d.click("project_critical_longest");
    d.frames(2);
    assert_eq!(project_column(&d, "critical_path_type"), "CT_DrivPath");
    assert_eq!(critical_definition(&d.app.project), (true, 8.));
    assert_eq!(d.app.history.counts().0, steps + 2, "one undo step");
    // Schedule Options shows the same setting.
    d.app.open_options_drafts();
    assert!(d.app.sched.critical_longest, "Schedule Options agrees");
    // Undo once: the float radio is back; once more: the float is back.
    d.home();
    d.click("undo");
    d.frames(2);
    d.assert_clean("undo longest path");
    assert_eq!(critical_definition(&d.app.project), (false, 8.));
    d.click("undo");
    d.frames(2);
    assert_eq!(critical_definition(&d.app.project), (false, 0.));
    assert_eq!(project_column(&d, "critical_path_type"), "CT_TotFloat");
    assert!(!critical(&d));
}

#[test]
fn workflow_add_buttons_open_their_pickers_empty() {
    let mut d = Driver::new("wf-look-add-buttons", tank_job());
    d.home();
    let base = index_of(&d.app.project, "A1000");
    select_row(&mut d, base);
    let model = d.app.project.model.clone();
    let steps = d.app.history.counts().0;
    // Risks: no project risks, Assign still opens the risk picker.
    open_tab(&mut d, DetailTab::Risks, "detail_tab_risks");
    d.click("detail_risks_assign");
    d.frames(2);
    assert!(
        matches!(d.app.detail.picker, Some(Picker::Risk { .. })),
        "Assign opens the risk picker with nothing to pick"
    );
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.detail.picker.is_none(), "Escape closes it");
    // WPs & Docs: no documents, Assign opens the document picker.
    open_tab(&mut d, DetailTab::Documents, "detail_tab_documents");
    d.click("detail_documents_assign");
    d.frames(2);
    assert!(
        matches!(d.app.detail.picker, Some(Picker::Document { .. })),
        "Assign opens the document picker with nothing to pick"
    );
    d.key(Key::Escape);
    d.frames(2);
    // Steps: no step templates, Add from Template opens the template list.
    open_tab(&mut d, DetailTab::Steps, "detail_tab_steps");
    d.click("detail_steps_from_template");
    d.frames(2);
    assert!(
        d.app.dictionaries.template_pick.is_some(),
        "Add from Template opens the template list with no templates"
    );
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.dictionaries.template_pick.is_none());
    // Resources: no roles, Add Role opens the role picker.
    open_tab(&mut d, DetailTab::Resources, "detail_tab_resources");
    d.click("detail_rsrc_add_role");
    d.frames(2);
    assert!(
        matches!(
            d.app.detail.assign,
            Some(crate::details::assignments::AssignPicker::Role { .. })
        ),
        "Add Role opens the role picker with no roles"
    );
    d.key(Key::Escape);
    d.frames(2);
    d.assert_clean("add buttons");
    assert!(d.app.project.model == model, "nothing changed");
    assert_eq!(d.app.history.counts().0, steps);
}

#[test]
fn workflow_select_wbs_from_the_general_tab() {
    let mut d = Driver::new("wf-look-select-wbs", tank_job());
    d.home();
    let paint = index_of(&d.app.project, "A1020");
    let task_id = d.app.project.model.activities[paint].task_id;
    let wbs = |d: &Driver| d.app.project.details[&task_id].wbs;
    assert_eq!(wbs(&d), 71);
    select_row(&mut d, paint);
    open_tab(&mut d, DetailTab::General, "detail_tab_general");
    d.click("detail_activity_wbs");
    d.frames(2);
    let picker = d.app.views.records.picker.clone();
    let picker = picker.expect("Select WBS opens");
    assert_eq!(picker.target, PickTarget::ActivityWbs(task_id));
    assert!(shown(&mut d, "Select WBS"), "the window title");
    let steps = d.app.history.counts().0;
    assert!(d.click_text("Mechanical"), "Mechanical is listed");
    d.click("pick_assign");
    d.frames(2);
    d.assert_clean("assign WBS");
    assert!(d.app.views.records.picker.is_none(), "Assign closes it");
    assert_eq!(wbs(&d), 72, "the activity moves to Mechanical");
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    d.home();
    d.click("undo");
    d.frames(2);
    assert_eq!(wbs(&d), 71, "undo puts it back");
}

#[test]
fn workflow_link_tabs_start_with_their_own_columns() {
    crate::defaults::use_reference();
    let mut d = Driver::new("wf-look-link-columns", tank_job());
    d.home();
    let shell = index_of(&d.app.project, "A1010");
    select_row(&mut d, shell);
    open_tab(&mut d, DetailTab::Predecessors, "detail_tab_predecessors");
    assert!(shown(&mut d, "Activity Name"), "Predecessors columns");
    assert!(!shown(&mut d, "Comments"), "no Comments on Predecessors");
    assert!(!shown(&mut d, "Project ID"), "no Project ID there");
    open_tab(&mut d, DetailTab::Successors, "detail_tab_successors");
    assert!(!shown(&mut d, "Comments"), "no Comments on Successors");
    open_tab(&mut d, DetailTab::Relationships, "detail_tab_relationships");
    // The halves are narrow: their first column is the one on screen.
    assert!(shown(&mut d, "Project ID"), "Relationships keeps its list");
    // A stored choice loads as stored.
    d.app.detail.grid_columns.insert(
        "predecessors".into(),
        vec!["Activity ID".into(), "Comments".into()],
    );
    open_tab(&mut d, DetailTab::Predecessors, "detail_tab_predecessors");
    assert!(shown(&mut d, "Comments"), "the stored choice is drawn");
    d.assert_clean("link columns");
}

#[test]
fn workflow_notes_panes_name_what_is_not_selected() {
    let mut d = Driver::new("wf-look-notes", tank_job());
    d.home();
    let base = index_of(&d.app.project, "A1000");
    select_row(&mut d, base);
    open_tab(&mut d, DetailTab::Notebook, "detail_tab_notebook");
    let none = "(No Notebook Topic selected)";
    assert!(shown(&mut d, none), "Activity Notebook");
    open_tab(&mut d, DetailTab::Steps, "detail_tab_steps");
    assert!(shown(&mut d, "(No Step Name selected)"), "Steps");
    project_tab(&mut d, "Notebook");
    assert!(shown(&mut d, none), "Project Notebook");
    d.assert_clean("notes panes");
}
