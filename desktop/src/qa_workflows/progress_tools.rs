//! Workflow: the progress tools on a small project that mirrors the observed
//! reference case (five 5-day activities planned Monday 08:00 to Friday
//! 16:00 on an eight-hour Monday to Friday calendar, data date the Sunday
//! before). Each tool is opened from the Tools menu, run, its numbers
//! checked against the recorded ones, then undone in one step.
//!
//! - Duration % with "Recalculate actual units and cost when duration %
//!   complete changes" on: every assignment's actual units are budget x %,
//!   costs follow at the price, remaining is budget minus actual; off: only
//!   the remaining duration follows.
//! - Apply Actuals: the activity flag progresses the activity and every
//!   assignment; a resource flag on every assignment progresses the
//!   activity; a resource flag on some assignments gives only those actuals;
//!   a flagged expense of a started activity accrues uniformly; nothing
//!   flagged, nothing changes. Actual this period accumulates.
//! - Update Progress: the proposed date is the data date, a flagged expense
//!   of an activity it completes is fully spent, and a notice follows.
use super::*;
use openxer_xer::edit::progress::RemainingRule;

const PROJECT: [&str; 9] = [
    "proj_id",
    "proj_short_name",
    "last_recalc_date",
    "plan_start_date",
    "clndr_id",
    "act_pct_link_flag",
    "act_this_per_link_flag",
    "add_act_remain_flag",
    "rem_target_link_flag",
];
const CALENDAR: [&str; 5] = [
    "clndr_id",
    "clndr_name",
    "clndr_data",
    "day_hr_cnt",
    "week_hr_cnt",
];
const PROJWBS: [&str; 6] = [
    "wbs_id",
    "proj_id",
    "wbs_short_name",
    "wbs_name",
    "seq_num",
    "proj_node_flag",
];
const RSRC: [&str; 6] = [
    "rsrc_id",
    "rsrc_short_name",
    "rsrc_name",
    "rsrc_type",
    "active_flag",
    "auto_compute_act_flag",
];
const RSRCRATE: [&str; 5] = [
    "rsrc_rate_id",
    "rsrc_id",
    "max_qty_per_hr",
    "cost_per_qty",
    "start_date",
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
    "auto_compute_act_flag",
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
const PROJCOST: [&str; 11] = [
    "cost_item_id",
    "proj_id",
    "task_id",
    "cost_name",
    "target_cost",
    "act_cost",
    "remain_cost",
    "target_qty",
    "cost_per_qty",
    "cost_load_type",
    "auto_compute_act_flag",
];

/// The flagged labor resource as observed: 14.545 units at 0.3636/h, $100/h.
/// After the first run 2 of 5 days are spent, after the second 4 of 5.
const A1010_FIRST: [f64; 5] = [5.818, 8.727, 581.82, 872.73, 5.818];
const A1010_SECOND: [f64; 5] = [11.636, 2.909, 1163.64, 290.91, 11.636];

fn week() -> String {
    // Monday to Friday 08:00 to 16:00, as the reference case.
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

fn line(fields: &[&str]) -> String {
    fields.join("\t")
}

/// One activity row: id, code, name, activity Auto Compute Actuals.
fn task(id: &str, code: &str, name: &str, auto: &str) -> String {
    line(&[
        id,
        "4",
        "40",
        "1",
        code,
        name,
        "TT_Task",
        "TK_NotStart",
        "DT_FixedDUR2",
        "CP_Drtn",
        "40",
        "40",
        "2026-11-23 08:00",
        "2026-11-27 16:00",
        auto,
    ])
}

/// One assignment row: id, task, resource, budget units, units/hour, price.
fn assignment(id: &str, task: &str, rsrc: &str, units: &str, rate: &str, price: f64) -> String {
    let cost = format!("{:.4}", units.parse::<f64>().unwrap() * price);
    let kind = if rsrc == "21" { "RT_Equip" } else { "RT_Labor" };
    let price = format!("{price}");
    line(&[
        id,
        task,
        "4",
        rsrc,
        units,
        units,
        rate,
        rate,
        &price,
        &cost,
        &cost,
        "0",
        "0",
        "0",
        "0",
        "0",
        "0",
        "COST_PER_QTY",
        kind,
        "ST_Rsrc",
        "Y",
        "Y",
    ])
}

/// The probe project: A1000 labor (unflagged, $120/h) and nonlabor (flagged,
/// $50/h); A1010 a flagged labor resource; A1020 no resource and a flagged
/// $1,000 expense, uniform accrual; A1030 the activity flag with the
/// unflagged labor; A1040 nothing flagged.
fn probe_project() -> ProjectData {
    let mut s = "ERMHDR\t25.12\t2026-11-22\tProject\tOpenXER\tOpenXER\tdbxDatabaseNoName\tProject Management\tUSD\n".to_string();
    let mut table = |name: &str, fields: &[&str], rows: &[String]| {
        s += &format!("%T\t{name}\n%F\t{}\n", fields.join("\t"));
        for r in rows {
            s += &format!("%R\t{r}\n");
        }
    };
    let project = line(&[
        "4",
        "ZP4",
        "2026-11-22 00:00",
        "2026-11-22 00:00",
        "1",
        "Y",
        "Y",
        "N",
        "Y",
    ]);
    table("PROJECT", &PROJECT, &[project]);
    let calendar = line(&["1", "Day shift", &week(), "8", "40"]);
    table("CALENDAR", &CALENDAR, &[calendar]);
    let wbs = line(&["40", "4", "ZP4", "Progress probe", "1", "Y"]);
    table("PROJWBS", &PROJWBS, &[wbs]);
    let resources = [
        line(&["21", "ZNL", "Probe nonlabor", "RT_Equip", "Y", "Y"]),
        line(&["22", "ZHU", "Probe labor", "RT_Labor", "Y", "N"]),
        line(&["23", "ZLAB", "Probe crew", "RT_Labor", "Y", "Y"]),
    ];
    table("RSRC", &RSRC, &resources);
    let rates = [
        line(&["1", "21", "1", "50", "2000-01-01 00:00"]),
        line(&["2", "22", "1", "120", "2000-01-01 00:00"]),
        line(&["3", "23", "1", "100", "2000-01-01 00:00"]),
    ];
    table("RSRCRATE", &RSRCRATE, &rates);
    let tasks = [
        task("100", "A1000", "Set pump base, labor and crane", "N"),
        task("101", "A1010", "Grout pump base", "N"),
        task("102", "A1020", "Rent light tower", "N"),
        task("103", "A1030", "Align pump and motor", "Y"),
        task("104", "A1040", "Connect suction piping", "N"),
    ];
    table("TASK", &TASK, &tasks);
    let assignments = [
        assignment("1", "100", "21", "40", "1", 50.),
        assignment("2", "100", "22", "40", "1", 120.),
        assignment(
            "3",
            "101",
            "23",
            "14.5454545454545",
            "0.363636363636364",
            100.,
        ),
        assignment("4", "103", "22", "40", "1", 120.),
        assignment("5", "104", "22", "40", "1", 120.),
    ];
    table("TASKRSRC", &TASKRSRC, &assignments);
    let expense = line(&[
        "1",
        "4",
        "102",
        "Light tower rental",
        "1000",
        "0",
        "1000",
        "1",
        "1000",
        "CL_Uniform",
        "Y",
    ]);
    table("PROJCOST", &PROJCOST, &[expense]);
    s += "%E\n";
    let doc = Document::parse(&s).unwrap();
    let mut p = ProjectData::from_document(doc, 4, "probe".into()).unwrap();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn at(p: &ProjectData, text: &str) -> i64 {
    openxer_xer::parse_relative(p.model.origin_serial, text)
        .unwrap()
        .unwrap()
}

/// A date as the dialogs show and read it.
fn day(p: &ProjectData, text: &str) -> String {
    crate::format::date(p.model.origin_serial, Some(at(p, text)))
}

/// (actual units, remaining units, actual cost, remaining cost, actual
/// units this period) of the assignment of resource `rsrc` on `code`.
fn work(p: &ProjectData, code: &str, rsrc: &str) -> [f64; 5] {
    let id = p.model.activities[index_of(p, code)].task_id.to_string();
    let rsrc_id = match rsrc {
        "ZNL" => "21",
        "ZHU" => "22",
        _ => "23",
    };
    let t = p.document.table("TASKRSRC").unwrap();
    let r = t
        .rows
        .iter()
        .find(|r| t.value(r, "task_id") == id && t.value(r, "rsrc_id") == rsrc_id)
        .unwrap_or_else(|| panic!("{code} has {rsrc}"));
    let v = |k: &str| t.value(r, k).parse::<f64>().unwrap_or(0.);
    [
        v("act_reg_qty") + v("act_ot_qty"),
        v("remain_qty"),
        v("act_reg_cost") + v("act_ot_cost"),
        v("remain_cost"),
        v("act_this_per_qty"),
    ]
}

#[track_caller]
fn check(p: &ProjectData, code: &str, rsrc: &str, want: [f64; 5]) {
    let got = work(p, code, rsrc);
    for (n, (g, w)) in got.iter().zip(want).enumerate() {
        assert!(
            (g - w).abs() < 0.006,
            "{code} {rsrc}: figure {n} is {g}, observed {w} (all: {got:?})"
        );
    }
}

fn expense(p: &ProjectData) -> [f64; 2] {
    let t = p.document.table("PROJCOST").unwrap();
    let v = |k: &str| t.value(&t.rows[0], k).parse::<f64>().unwrap_or(0.);
    [v("act_cost"), v("remain_cost")]
}

fn rem(p: &ProjectData, code: &str) -> i64 {
    p.model.activities[index_of(p, code)].rem
}

fn status_of(p: &ProjectData, code: &str) -> StatusType {
    p.model.activities[index_of(p, code)].status
}

fn column(d: &Driver, id: &str) -> grid::columns::Column {
    let catalog = grid::columns::catalog(&d.app.project);
    grid::columns::find(&catalog, id).unwrap().clone()
}

/// Start activity `code` by hand, as its Activity Status cell does.
fn start(d: &mut Driver, code: &str) {
    let i = index_of(&d.app.project, code);
    assert!(d.app.apply_cell(i, "activity_status", "In Progress"));
    d.frames(1);
    assert_eq!(status_of(&d.app.project, code), StatusType::InProgress);
}

/// Tools > `item`, from the menu bar, as things stand (no Escape first, so
/// a selection stays).
fn tools(d: &mut Driver, item: &str) {
    d.click("menu_bar_tools");
    d.frames(1);
    d.click(item);
    d.frames(2);
}

/// Answer the future data date question Yes when it is asked (it is while
/// the probe dates lie after today).
fn answer_future(d: &mut Driver) {
    d.frames(2);
    if d.app.sched.progress.confirm_future.is_some() {
        assert!(d.has("future_date_yes"), "the question offers Yes");
        d.key(Key::Enter);
        d.frames(2);
    }
    d.assert_clean("progress run");
}

/// The notice the tool shows after a run, closed with OK.
fn close_notice(d: &mut Driver) {
    assert!(
        d.has("progress_done_ok"),
        "a notice follows the run: {} {:?}",
        d.app.status,
        d.app.error
    );
    d.key(Key::Enter);
    d.frames(2);
    assert!(!d.has("progress_done_ok"), "OK closes it");
}

/// Every cell of table `name`.
fn rows(p: &ProjectData, name: &str) -> Vec<Vec<String>> {
    let Some(t) = p.document.table(name) else {
        return vec![];
    };
    let mut out = vec![];
    for r in &t.rows {
        let cell = |f: &String| t.value(r, f).to_string();
        out.push(t.fields.iter().map(cell).collect());
    }
    out
}

/// The project values a progress tool may change, for the undo check.
fn snapshot(p: &ProjectData) -> (Model, Vec<Vec<String>>, Vec<Vec<String>>) {
    (p.model.clone(), rows(p, "TASKRSRC"), rows(p, "PROJCOST"))
}

/// Show column `id` in the activity table, second from the left so the
/// table pane shows it.
fn add_column(d: &mut Driver, id: &str) {
    let setting = grid::layout::ColumnSetting::new(id, 80.);
    d.app.grid.layout.columns.insert(1, setting);
    d.frames(2);
}

#[test]
fn workflow_duration_percent_follows_the_recalculate_switch() {
    let mut d = Driver::new("qa_progress_duration_pct", probe_project());
    show(&mut d, "activities");
    add_column(&mut d, "duration_percent_complete");
    let i = index_of(&d.app.project, "A1000");
    // Duration % is closed until the activity has started.
    let pct = column(&d, "duration_percent_complete");
    assert!(d.app.cell_editable(i, &pct).is_err());
    start(&mut d, "A1000");

    // Switch on (as stored): 40% gives every assignment 16 of 40 units.
    let before = snapshot(&d.app.project);
    let steps = d.app.history.counts().0;
    cell_edit(&mut d, i, "duration_percent_complete", "40");
    let p = &d.app.project;
    assert_eq!(rem(p, "A1000"), 24 * 60, "remaining 3d of 5d");
    check(p, "A1000", "ZHU", [16., 24., 1920., 2880., 16.]);
    check(p, "A1000", "ZNL", [16., 24., 800., 1200., 16.]);
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    d.home();
    d.click("undo");
    d.frames(2);
    assert_eq!(snapshot(&d.app.project), before, "undo");
    d.click("redo");
    d.frames(2);
    let p = &d.app.project;
    check(p, "A1000", "ZHU", [16., 24., 1920., 2880., 16.]);

    // Switch off: 70% moves only the remaining duration.
    let project = d.app.project.document.table_mut("PROJECT");
    project.set(0, "act_pct_link_flag", "N".into());
    cell_edit(&mut d, i, "duration_percent_complete", "70");
    let p = &d.app.project;
    assert_eq!(rem(p, "A1000"), 12 * 60, "remaining 1.5d");
    check(p, "A1000", "ZHU", [16., 24., 1920., 2880., 16.]);
    check(p, "A1000", "ZNL", [16., 24., 800., 1200., 16.]);
}

#[test]
fn workflow_auto_compute_actuals_is_a_check_box_column() {
    let mut d = Driver::new("qa_progress_flag_column", probe_project());
    show(&mut d, "activities");
    // The activity flag is a General column, a check box set in place.
    let catalog = grid::columns::catalog(&d.app.project);
    let flag = grid::columns::find(&catalog, "auto_compute_actuals");
    let flag = flag.expect("an Auto Compute Actuals column").clone();
    assert_eq!(flag.group, grid::columns::GENERAL);
    assert_eq!(flag.kind, grid::columns::Kind::Flag);
    add_column(&mut d, "auto_compute_actuals");
    let a1040 = index_of(&d.app.project, "A1040");
    select_row(&mut d, a1040);
    let cell = row_cell(&mut d, a1040, "auto_compute_actuals");
    // Two single clicks (focus the cell, then tick it), each after the
    // double-click time of the click before has passed.
    for _ in 0..2 {
        d.frames(5);
        d.click_at(cell);
    }
    d.frames(2);
    let flagged = |d: &Driver| {
        let p = &d.app.project;
        let id = p.model.activities[a1040].task_id;
        records::task_value(&p.document, id, 4, "auto_compute_act_flag")
    };
    assert_eq!(flagged(&d), "Y", "a click ticks it");
    d.click("undo");
    d.frames(2);
    assert_eq!(flagged(&d), "N", "one undo step");
}

#[test]
fn workflow_apply_actuals_follows_the_auto_compute_flags() {
    let mut d = Driver::new("qa_progress_apply_actuals", probe_project());
    show(&mut d, "activities");
    // A1000 and A1020 under way, as started by hand.
    start(&mut d, "A1000");
    start(&mut d, "A1020");
    let p = d.app.project.clone();
    let start = at(&p, "2026-11-23 08:00");

    // First run: the data date to Wednesday 25-Nov-26.
    d.home();
    tools(&mut d, "menu_apply_actuals");
    assert!(d.app.sched.progress.actuals.is_some(), "it opens");
    let draft = d.app.sched.progress.actuals.as_mut().unwrap();
    assert!(draft.own_date, "each project its own date");
    draft.new_date = day(&p, "2026-11-25 00:00");
    let before = snapshot(&d.app.project);
    let steps = d.app.history.counts().0;
    d.click("actuals_apply");
    answer_future(&mut d);
    let p = d.app.project.clone();
    let dd = at(&p, "2026-11-25 00:00");
    assert_eq!(p.model.project.data_date, Some(dd));
    // Resource flag on every assignment: the activity starts as planned.
    let a1010 = &p.model.activities[index_of(&p, "A1010")];
    assert_eq!(a1010.status, StatusType::InProgress);
    assert_eq!(a1010.act_start, Some(start));
    assert_eq!(a1010.rem, 24 * 60);
    check(&p, "A1010", "ZLAB", A1010_FIRST);
    // Activity flag: every assignment, whatever its resource's flag.
    assert_eq!(status_of(&p, "A1030"), StatusType::InProgress);
    assert_eq!(rem(&p, "A1030"), 24 * 60);
    check(&p, "A1030", "ZHU", [16., 24., 1920., 2880., 16.]);
    // Some assignments flagged: only those, the duration stays.
    assert_eq!(rem(&p, "A1000"), 40 * 60);
    check(&p, "A1000", "ZNL", [16., 24., 800., 1200., 16.]);
    check(&p, "A1000", "ZHU", [0., 40., 0., 4800., 0.]);
    // A flagged expense of a started activity: two of five days.
    assert_eq!(expense(&p), [400., 600.]);
    assert_eq!(rem(&p, "A1020"), 40 * 60, "duration kept");
    // Nothing flagged: nothing progressed.
    assert_eq!(status_of(&p, "A1040"), StatusType::NotStarted);
    check(&p, "A1040", "ZHU", [0., 40., 0., 4800., 0.]);
    close_notice(&mut d);
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    let after_first = snapshot(&p);

    // Second run to Friday 27-Nov-26: the actuals accumulate.
    tools(&mut d, "menu_apply_actuals");
    let draft = d.app.sched.progress.actuals.as_mut().unwrap();
    draft.new_date = day(&p, "2026-11-27 00:00");
    d.click("actuals_apply");
    answer_future(&mut d);
    let p = d.app.project.clone();
    check(&p, "A1010", "ZLAB", A1010_SECOND);
    assert_eq!(rem(&p, "A1010"), 8 * 60);
    check(&p, "A1030", "ZHU", [32., 8., 3840., 960., 32.]);
    assert_eq!(rem(&p, "A1030"), 8 * 60);
    check(&p, "A1000", "ZNL", [32., 8., 1600., 400., 32.]);
    check(&p, "A1000", "ZHU", [0., 40., 0., 4800., 0.]);
    assert_eq!(rem(&p, "A1000"), 40 * 60);
    assert_eq!(expense(&p), [800., 200.]);
    assert_eq!(status_of(&p, "A1040"), StatusType::NotStarted);
    close_notice(&mut d);

    // Undo once: back to the first run's result, then to the start.
    d.home();
    d.click("undo");
    d.frames(2);
    assert_eq!(snapshot(&d.app.project), after_first, "undo");
    d.click("undo");
    d.frames(2);
    assert_eq!(snapshot(&d.app.project), before, "undo again");
}

#[test]
fn workflow_update_progress_spends_the_expense_of_what_it_completes() {
    let mut d = Driver::new("qa_progress_update", probe_project());
    show(&mut d, "activities");
    let i = index_of(&d.app.project, "A1020");
    start(&mut d, "A1020");
    d.home();
    select_row(&mut d, i);
    tools(&mut d, "menu_update_progress");
    let p = d.app.project.clone();
    d.click("update_selected");
    d.click("update_by_type");
    let draft = d.app.sched.progress.update.as_mut().expect("it opens");
    assert!(draft.selected_only, "Selected activities only is chosen");
    draft.new_date = day(&p, "2026-12-03 00:00");
    let before = snapshot(&d.app.project);
    let steps = d.app.history.counts().0;
    d.click("update_apply");
    answer_future(&mut d);
    let p = d.app.project.clone();
    assert_eq!(status_of(&p, "A1020"), StatusType::Completed);
    let finish = at(&p, "2026-11-27 16:00");
    assert_eq!(p.model.activities[i].act_finish, Some(finish));
    assert_eq!(expense(&p), [1000., 0.], "the expense is spent");
    for code in ["A1000", "A1010", "A1030", "A1040"] {
        assert_eq!(status_of(&p, code), StatusType::NotStarted, "{code}");
    }
    close_notice(&mut d);
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    // Next time: the remaining duration choice is kept, and with the
    // spotlight on the proposal is still the data date.
    d.app.gantt.options.spotlight = true;
    d.home();
    tools(&mut d, "menu_update_progress");
    let draft = d.app.sched.progress.update.clone().expect("it opens");
    assert_eq!(draft.rule, RemainingRule::DurationType, "kept");
    let proposed = crate::format::date(p.model.origin_serial, p.model.project.data_date);
    assert_eq!(draft.new_date, proposed, "the data date is proposed");
    d.click("update_cancel");
    d.frames(1);
    d.home();
    d.click("undo");
    d.frames(2);
    assert_eq!(snapshot(&d.app.project), before, "undo");
}
