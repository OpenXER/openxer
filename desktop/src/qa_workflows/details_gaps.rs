//! Workflow: Activity Details gaps on a small pump job built here (an
//! eight-hour Monday to Friday calendar, data date Friday 27-Nov-26 08:00,
//! one labor resource at $120/h).
//!
//! - Status tab: the units At Complete box is typed. On the not started
//!   activity (budget and remaining linked) budget and remaining both take
//!   it and the units/time follows with the duration kept; on the started
//!   one the remaining takes At Complete less Actual and the budget stays,
//!   as observed in the reference.
//! - Resources tab: cost, at completion, this period and Cost Units Linked
//!   columns added through Customize Resource Columns, a budgeted cost typed
//!   (the units follow at the price) and the linked tick box cleared.
//! - Steps tab: the Step Weight Percent column (weight over the total, one
//!   decimal, read only), and Assign > Steps giving two selected activities a
//!   template's steps at once, after the selected step on the activity shown.
//!
//! Each change is undone once and checked back.
use super::*;
use crate::details::grid_columns::RESOURCES;
use crate::details::DetailTab;

const PROJECT: [&str; 8] = [
    "proj_id",
    "proj_short_name",
    "last_recalc_date",
    "plan_start_date",
    "clndr_id",
    "rem_target_link_flag",
    "add_act_remain_flag",
    "act_this_per_link_flag",
];
const TASK: [&str; 15] = [
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
    "target_start_date",
    "target_end_date",
    "act_start_date",
];
const TASKRSRC: [&str; 22] = [
    "taskrsrc_id",
    "task_id",
    "proj_id",
    "rsrc_id",
    "remain_qty",
    "target_qty",
    "remain_qty_per_hr",
    "target_qty_per_hr",
    "cost_per_qty",
    "target_cost",
    "remain_cost",
    "act_reg_qty",
    "act_ot_qty",
    "act_reg_cost",
    "act_ot_cost",
    "act_this_per_qty",
    "act_this_per_cost",
    "rate_type",
    "rsrc_type",
    "cost_per_qty_source_type",
    "rollup_dates_flag",
    "cost_qty_link_flag",
];
const TASKPROC: [&str; 9] = [
    "proc_id",
    "task_id",
    "proj_id",
    "seq_num",
    "proc_name",
    "complete_flag",
    "proc_wt",
    "complete_pct",
    "proc_descr",
];

/// The template the job's step lists come from.
const TEMPLATE_STEPS: [&str; 3] = ["Check bolts", "Fill oil", "Run test"];

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

/// One activity row, Fixed Duration and Units: remaining and original hours,
/// planned start, planned finish and actual start.
fn task(
    id: &str,
    code: &str,
    name: &str,
    status: &str,
    hours: [&str; 2],
    dates: [&str; 3],
) -> String {
    line(&[
        id,
        "4",
        "40",
        "1",
        code,
        name,
        "TT_Task",
        status,
        "DT_FixedDUR2",
        "CP_Drtn",
        hours[0],
        hours[1],
        dates[0],
        dates[1],
        dates[2],
    ])
}

/// A1030 in progress (32 of 40 units spent, one day left), A1040 not started
/// (40 units over six days, two steps), A1050 not started with no steps.
fn pump_job() -> ProjectData {
    let mut s = "ERMHDR\t25.12\t2026-11-22\tProject\tOpenXER\tOpenXER\tdbxDatabaseNoName\tProject Management\tUSD\n".to_string();
    let mut table = |name: &str, fields: &[&str], rows: &[String]| {
        s += &format!("%T\t{name}\n%F\t{}\n", fields.join("\t"));
        for r in rows {
            s += &format!("%R\t{r}\n");
        }
    };
    let project = line(&[
        "4",
        "PMP",
        "2026-11-27 08:00",
        "2026-11-23 00:00",
        "1",
        "Y",
        "N",
        "Y",
    ]);
    table("PROJECT", &PROJECT, &[project]);
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
        "wbs_short_name",
        "wbs_name",
        "seq_num",
        "proj_node_flag",
    ];
    let wbs = line(&["40", "4", "PMP", "Pump set", "1", "Y"]);
    table("PROJWBS", &fields, &[wbs]);
    let fields = [
        "rsrc_id",
        "rsrc_short_name",
        "rsrc_name",
        "rsrc_type",
        "active_flag",
    ];
    let crew = line(&["22", "MILL", "Millwright", "RT_Labor", "Y"]);
    table("RSRC", &fields, &[crew]);
    let fields = [
        "rsrc_rate_id",
        "rsrc_id",
        "max_qty_per_hr",
        "cost_per_qty",
        "start_date",
    ];
    let rate = line(&["1", "22", "1", "120", "2000-01-01 00:00"]);
    table("RSRCRATE", &fields, &[rate]);
    let tasks = [
        task(
            "103",
            "A1030",
            "Align pump and motor",
            "TK_Active",
            ["8", "40"],
            ["2026-11-23 08:00", "2026-11-27 16:00", "2026-11-23 08:00"],
        ),
        task(
            "104",
            "A1040",
            "Connect suction piping",
            "TK_NotStart",
            ["48", "48"],
            ["2026-11-30 08:00", "2026-12-07 16:00", ""],
        ),
        task(
            "105",
            "A1050",
            "Flush lube oil system",
            "TK_NotStart",
            ["40", "40"],
            ["2026-11-30 08:00", "2026-12-04 16:00", ""],
        ),
    ];
    table("TASK", &TASK, &tasks);
    let assignment = |id: &str, task: &str, units: [&str; 3], rate: &str, costs: [&str; 4]| {
        line(&[
            id,
            task,
            "4",
            "22",
            units[1],
            units[0],
            rate,
            rate,
            "120",
            costs[0],
            costs[1],
            units[2],
            "0",
            costs[2],
            "0",
            units[2],
            costs[3],
            "COST_PER_QTY",
            "RT_Labor",
            "ST_Rsrc",
            "Y",
            "Y",
        ])
    };
    let assignments = [
        // Budget 40, remaining 8, actual 32 (and 32 this period).
        assignment(
            "1",
            "103",
            ["40", "8", "32"],
            "1",
            ["4800.0000", "960.0000", "3840.0000", "3840.0000"],
        ),
        assignment(
            "2",
            "104",
            ["40", "40", "0"],
            "0.833333333333333",
            ["4800.0000", "4800.0000", "0", "0"],
        ),
    ];
    table("TASKRSRC", &TASKRSRC, &assignments);
    let steps = [
        line(&["1", "104", "4", "1", "(New step)", "N", "1", "0", ""]),
        line(&["2", "104", "4", "2", "(New step)1", "N", "1", "0", ""]),
    ];
    table("TASKPROC", &TASKPROC, &steps);
    let fields = ["proc_group_id", "seq_num", "proc_group_name"];
    let group = line(&["1", "1", "Pump start-up"]);
    table("PROCGROUP", &fields, &[group]);
    let items: Vec<String> = TEMPLATE_STEPS
        .iter()
        .enumerate()
        .map(|(k, name)| {
            let n = (k + 1).to_string();
            line(&[&n, "1", &n, name, "1", ""])
        })
        .collect();
    let fields = [
        "proc_item_id",
        "proc_group_id",
        "seq_num",
        "proc_name",
        "proc_wt",
        "proc_descr",
    ];
    table("PROCITEM", &fields, &items);
    s += "%E\n";
    let doc = Document::parse(&s).unwrap();
    let mut p = ProjectData::from_document(doc, 4, "pump job".into()).unwrap();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

/// The assignment row of activity `code`, as the file holds it.
fn row_of(d: &Driver, code: &str) -> Vec<String> {
    let p = &d.app.project;
    let id = p.model.activities[index_of(p, code)].task_id.to_string();
    let t = p.document.table("TASKRSRC").unwrap();
    t.rows
        .iter()
        .find(|r| t.value(r, "task_id") == id)
        .unwrap_or_else(|| panic!("{code} has an assignment"))
        .clone()
}

fn cell(d: &Driver, code: &str, column: &str) -> String {
    let t = d.app.project.document.table("TASKRSRC").unwrap();
    t.value(&row_of(d, code), column).to_string()
}

/// The activity's step names in step order.
fn steps_of(d: &Driver, code: &str) -> Vec<String> {
    let p = &d.app.project;
    let task = p.model.activities[index_of(p, code)].task_id;
    let pid = p.model.project.proj_id;
    let get = |r: usize, k: &str| crate::records::get(&p.document, "TASKPROC", r, k);
    let mut rows = crate::records::rows_for(&p.document, "TASKPROC", task, pid);
    rows.sort_by_key(|&r| crate::records::num(&get(r, "seq_num")));
    rows.iter().map(|&r| get(r, "proc_name")).collect()
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

/// The texts drawn inside the rendered control `key`.
fn text_in(d: &mut Driver, key: &str) -> String {
    d.frames(2);
    let r = d.rect(key);
    d.texts()
        .into_iter()
        .filter(|(_, t)| r.contains(t.center()))
        .map(|(s, _)| s)
        .collect::<Vec<_>>()
        .join(" ")
}

/// A number cell of the selected row: click the row, click its editor, type
/// `text` over the value and press Enter.
fn type_in_cell(d: &mut Driver, key: &str, text: &str) {
    d.frames(2);
    let r = d.rect(key);
    d.click_at(r.center());
    d.frames(2);
    d.click_at(Pos2::new(r.left() + 20., r.center().y));
    d.frames(2);
    d.key_with(Key::A, ctrl());
    d.type_text(text);
    d.key(Key::Enter);
    d.frames(2);
    d.assert_clean(key);
}

/// Click a cell of the selected row as `type_in_cell` does and say whether
/// an editor took the keyboard; Escape then leaves it unchanged.
fn opens_editor(d: &mut Driver, key: &str) -> bool {
    d.frames(2);
    let r = d.rect(key);
    d.click_at(r.center());
    d.frames(2);
    d.click_at(Pos2::new(r.left() + 20., r.center().y));
    d.frames(2);
    let typing = d.ctx.wants_keyboard_input();
    if typing {
        d.key(Key::Escape);
        d.frames(2);
    }
    typing
}

/// Click a field of the Status tab and say whether it took the keyboard;
/// Escape then leaves it unchanged.
fn field_takes_keys(d: &mut Driver, key: &str) -> bool {
    d.click(key);
    d.frames(2);
    let typing = d.ctx.wants_keyboard_input();
    if typing {
        d.key(Key::Escape);
        d.frames(2);
    }
    typing
}

/// Right-click the details grid above `button` and pick `item`.
fn grid_menu(d: &mut Driver, button: &str, item: &str) {
    d.frames(2);
    let b = d.rect(button);
    d.press_at(
        Pos2::new(b.left() + 60., b.top() - 30.),
        egui::PointerButton::Secondary,
        Default::default(),
    );
    assert!(d.click_text(item), "{item} is on the grid menu");
    d.frames(2);
    assert!(d.app.detail.column_dialog.is_some(), "{item} opens");
}

/// In Customize ... Columns: pick `title` in Available Options (scrolling
/// the list down until it shows) and move it across.
fn pick_available(d: &mut Driver, title: &str) {
    for _ in 0..40 {
        if d.click_text(title) {
            d.click("detail_columns_add");
            return;
        }
        let add = d.rect("detail_columns_add");
        let over = Pos2::new(add.left() - 90., add.top() + 10.);
        d.frame(vec![egui::Event::PointerMoved(over)]);
        d.frame(vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::Vec2::new(0., -54.),
            modifiers: Default::default(),
        }]);
        d.frames(2);
    }
    panic!("{title} is not in Available Options");
}

fn undo(d: &mut Driver) {
    d.click("undo");
    d.frames(2);
    d.assert_clean("undo");
}

#[test]
fn workflow_units_at_complete_is_typed_on_the_status_tab() {
    let mut d = Driver::new("wf-details-at-complete", pump_job());
    d.home();
    // Not started A1040: 40 becomes 60 on budget and remaining, the units
    // per hour follow (60 over 48 hours) and the six days stay.
    let a1040 = index_of(&d.app.project, "A1040");
    let rem = d.app.project.model.activities[a1040].rem;
    let before = row_of(&d, "A1040");
    let undo_count = d.app.history.counts().0;
    select_row(&mut d, a1040);
    open_tab(&mut d, DetailTab::Status, "detail_tab_status");
    type_into(&mut d, "detail_units_at_complete", "60");
    d.key(Key::Enter);
    d.frames(2);
    d.assert_clean("at complete 60");
    assert_eq!(cell(&d, "A1040", "target_qty"), "60", "budget follows");
    assert_eq!(cell(&d, "A1040", "remain_qty"), "60");
    assert_eq!(cell(&d, "A1040", "remain_qty_per_hr"), "1.25");
    assert_eq!(cell(&d, "A1040", "target_cost"), "7200.0000");
    let now = d.app.project.model.activities[a1040].rem;
    assert_eq!(now, rem, "duration kept");
    assert_eq!(d.app.history.counts().0, undo_count + 1, "one undo step");
    undo(&mut d);
    assert_eq!(row_of(&d, "A1040"), before, "undo puts it back");

    // In progress A1030 (32 actual, 8 remaining): At Complete 50 leaves 18
    // remaining over the same 8 hours, the budget stays 40.
    let a1030 = index_of(&d.app.project, "A1030");
    let before = row_of(&d, "A1030");
    select_row(&mut d, a1030);
    open_tab(&mut d, DetailTab::Status, "detail_tab_status");
    type_into(&mut d, "detail_units_at_complete", "50");
    d.key(Key::Enter);
    d.frames(2);
    d.assert_clean("at complete 50");
    assert_eq!(cell(&d, "A1030", "remain_qty"), "18");
    assert_eq!(cell(&d, "A1030", "target_qty"), "40", "budget stays");
    assert_eq!(cell(&d, "A1030", "act_reg_qty"), "32");
    assert_eq!(cell(&d, "A1030", "remain_qty_per_hr"), "2.25");
    assert_eq!(cell(&d, "A1030", "remain_cost"), "2160.0000");
    assert_eq!(d.app.project.model.activities[a1030].rem, 480);
    undo(&mut d);
    assert_eq!(row_of(&d, "A1030"), before);

    // Not started A1040: Actual is read only, as observed (a click gives
    // it no keyboard), Budgeted is typed.
    let before = row_of(&d, "A1040");
    select_row(&mut d, a1040);
    open_tab(&mut d, DetailTab::Status, "detail_tab_status");
    assert!(field_takes_keys(&mut d, "detail_units_budgeted"));
    assert!(
        !field_takes_keys(&mut d, "detail_units_actual"),
        "Actual is read only before the start"
    );
    assert_eq!(row_of(&d, "A1040"), before);
    // Labor Cost: At Complete cost is typed too. 6000 is 50 units at $120
    // on budget and remaining (editable observed, result not observed: the
    // same rule as the units, through the price).
    select_row(&mut d, a1040);
    open_tab(&mut d, DetailTab::Status, "detail_tab_status");
    d.click("detail_units_mode");
    assert!(d.click_text("Labor Cost"), "the chooser lists Labor Cost");
    d.frames(2);
    let undo_count = d.app.history.counts().0;
    type_into(&mut d, "detail_units_at_complete", "6000");
    d.key(Key::Enter);
    d.frames(2);
    d.assert_clean("at complete cost 6000");
    assert_eq!(cell(&d, "A1040", "remain_cost"), "6000.0000");
    assert_eq!(cell(&d, "A1040", "target_qty"), "50");
    assert_eq!(cell(&d, "A1040", "remain_qty"), "50");
    assert_eq!(d.app.history.counts().0, undo_count + 1, "one undo step");
    undo(&mut d);
    assert_eq!(row_of(&d, "A1040"), before, "undo puts it back");
}

#[test]
fn workflow_resources_tab_cost_and_period_columns() {
    let mut d = Driver::new("wf-details-resource-columns", pump_job());
    d.home();
    let a1040 = index_of(&d.app.project, "A1040");
    select_row(&mut d, a1040);
    open_tab(&mut d, DetailTab::Resources, "detail_tab_resources");
    // Customize Resource Columns: four columns right after Resource ID Name.
    grid_menu(&mut d, "detail_rsrc_add", "Customize Resource Columns...");
    d.click("detail_columns_selected_0");
    let added = [
        "Budgeted Cost",
        "At Completion Units",
        "Cost Units Linked",
        "Units This Period",
        "At Completion Cost",
    ];
    for title in added {
        pick_available(&mut d, title);
    }
    d.click("detail_columns_ok");
    d.frames(2);
    let columns = d.app.detail_columns(&RESOURCES, false);
    let shown: Vec<&str> = columns.iter().map(|c| c.0).collect();
    assert_eq!(shown[1..6], added, "added in place: {shown:?}");
    // The cells show the assignment's figures.
    let budget = crate::format::cost(4800.);
    assert_eq!(text_in(&mut d, "assignment_cell_0_27"), budget);
    let total = crate::format::units(40.);
    assert_eq!(text_in(&mut d, "assignment_cell_0_31"), total);
    // A budgeted cost of 6000 is 50 units at $120: budget and remaining.
    let before = row_of(&d, "A1040");
    let undo_count = d.app.history.counts().0;
    type_in_cell(&mut d, "assignment_cell_0_27", "6000");
    assert_eq!(cell(&d, "A1040", "target_qty"), "50");
    assert_eq!(cell(&d, "A1040", "remain_qty"), "50");
    assert_eq!(cell(&d, "A1040", "target_cost"), "6000.0000");
    assert_eq!(d.app.history.counts().0, undo_count + 1);
    undo(&mut d);
    assert_eq!(row_of(&d, "A1040"), before);
    // Cost Units Linked is a tick box; clearing it asks nothing.
    let r = d.rect("assignment_cell_0_36");
    let tick = Pos2::new(r.left() + 10., r.center().y);
    d.click_at(tick);
    d.frames(2);
    assert!(!d.has("relink_costs_yes"), "clearing the link asks nothing");
    assert_eq!(cell(&d, "A1040", "cost_qty_link_flag"), "N");
    // Ticking it again asks first: No leaves it cleared, Yes ticks it.
    d.click_at(tick);
    d.frames(2);
    assert!(
        d.has("relink_costs_yes"),
        "ticking the link again asks before recalculating the costs"
    );
    d.click("relink_costs_no");
    d.frames(2);
    assert_eq!(
        cell(&d, "A1040", "cost_qty_link_flag"),
        "N",
        "No: still off"
    );
    d.click_at(tick);
    d.frames(2);
    d.click("relink_costs_yes");
    d.frames(2);
    assert_eq!(cell(&d, "A1040", "cost_qty_link_flag"), "Y", "Yes ticks it");
    // The costs are worked out again from 40 units at $120: as before.
    assert_eq!(row_of(&d, "A1040"), before);

    // Not started A1040: the price from the resource opens no editor, as
    // observed; typing changes nothing.
    assert!(
        opens_editor(&mut d, "assignment_cell_0_5"),
        "Budgeted Units opens an editor"
    );
    assert!(
        !opens_editor(&mut d, "assignment_cell_0_1"),
        "a price from the resource opens no editor"
    );
    assert_eq!(row_of(&d, "A1040"), before, "the price stays");
    // At Completion Units is typed: budget and remaining take it.
    let undo_count = d.app.history.counts().0;
    type_in_cell(&mut d, "assignment_cell_0_31", "60");
    assert_eq!(cell(&d, "A1040", "target_qty"), "60");
    assert_eq!(cell(&d, "A1040", "remain_qty"), "60");
    assert_eq!(d.app.history.counts().0, undo_count + 1, "one undo step");
    undo(&mut d);
    assert_eq!(row_of(&d, "A1040"), before);
    // At Completion Cost is typed: 7200 is 60 units at $120 (editable
    // observed, result not observed: the same rule as the units).
    type_in_cell(&mut d, "assignment_cell_0_30", "7200");
    assert_eq!(cell(&d, "A1040", "remain_qty"), "60");
    assert_eq!(cell(&d, "A1040", "target_qty"), "60");
    assert_eq!(cell(&d, "A1040", "target_cost"), "7200.0000");
    assert_eq!(d.app.history.counts().0, undo_count + 1, "one undo step");
    undo(&mut d);
    assert_eq!(row_of(&d, "A1040"), before);
    // Units This Period opens no editor before the start.
    assert!(
        !opens_editor(&mut d, "assignment_cell_0_37"),
        "not started: Units This Period opens no editor"
    );
    assert_eq!(row_of(&d, "A1040"), before);

    // A1030: actual this period as the file holds it, at completion 32 + 8.
    let a1030 = index_of(&d.app.project, "A1030");
    select_row(&mut d, a1030);
    d.frames(2);
    let period = crate::format::units(32.);
    assert_eq!(text_in(&mut d, "assignment_cell_0_37"), period);
    assert_eq!(text_in(&mut d, "assignment_cell_0_31"), total);
    // In progress: Units This Period is typed; the project links actual to
    // date and this period, so the actual units move by the change.
    let before = row_of(&d, "A1030");
    type_in_cell(&mut d, "assignment_cell_0_37", "30");
    assert_eq!(cell(&d, "A1030", "act_reg_qty"), "30");
    undo(&mut d);
    assert_eq!(row_of(&d, "A1030"), before);
}

#[test]
fn workflow_step_weight_percent_and_steps_for_several_activities() {
    let mut d = Driver::new("wf-details-steps", pump_job());
    d.home();
    let a1040 = index_of(&d.app.project, "A1040");
    let a1050 = index_of(&d.app.project, "A1050");
    select_row(&mut d, a1040);
    open_tab(&mut d, DetailTab::Steps, "detail_tab_steps");
    // Step Weight Percent through Customize Steps Columns.
    grid_menu(&mut d, "detail_steps_add", "Customize Steps Columns...");
    pick_available(&mut d, "Step Weight Percent");
    pick_available(&mut d, "Step Weight");
    d.click("detail_columns_ok");
    d.frames(2);
    assert_eq!(text_in(&mut d, "step_cell_0_4"), "50.0");
    assert_eq!(text_in(&mut d, "step_cell_1_4"), "50.0");
    // Weight 3 on the first step: 75.0 and 25.0, one undo step.
    let undo_count = d.app.history.counts().0;
    type_in_cell(&mut d, "step_cell_0_3", "3");
    assert_eq!(text_in(&mut d, "step_cell_0_4"), "75.0");
    assert_eq!(text_in(&mut d, "step_cell_1_4"), "25.0");
    assert_eq!(d.app.history.counts().0, undo_count + 1);
    undo(&mut d);
    assert_eq!(text_in(&mut d, "step_cell_0_4"), "50.0");

    // A1050 then Ctrl+click A1040, select A1040's first step, Assign > Steps.
    select_row(&mut d, a1050);
    let pos = row_cell(&mut d, a1040, "activity_id");
    d.press_at(pos, egui::PointerButton::Primary, ctrl());
    d.frames(2);
    assert_eq!(d.app.selected, Some(a1040));
    assert_eq!(d.app.assign_targets().len(), 2, "two activities selected");
    let r = d.rect("step_cell_0_0");
    d.click_at(r.center());
    d.frames(2);
    assert_eq!(d.app.detail.rows[0], Some(0), "first step selected");
    let before = (steps_of(&d, "A1040"), steps_of(&d, "A1050"));
    let undo_count = d.app.history.counts().0;
    d.click("dock_assign_steps");
    d.frames(2);
    assert!(
        d.app.dictionaries.template_pick.is_some(),
        "Assign > Steps opens the template picker"
    );
    d.click("template_pick_add");
    d.frames(2);
    d.assert_clean("assign steps");
    assert!(d.app.dictionaries.template_pick.is_none());
    assert_eq!(
        steps_of(&d, "A1040"),
        [
            "(New step)",
            TEMPLATE_STEPS[0],
            TEMPLATE_STEPS[1],
            TEMPLATE_STEPS[2],
            "(New step)1",
        ],
        "after the selected step"
    );
    assert_eq!(steps_of(&d, "A1050"), TEMPLATE_STEPS, "appended");
    assert_eq!(d.app.history.counts().0, undo_count + 1, "one undo step");
    undo(&mut d);
    assert_eq!((steps_of(&d, "A1040"), steps_of(&d, "A1050")), before);
}
