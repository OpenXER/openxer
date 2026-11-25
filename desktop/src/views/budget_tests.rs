//! Budget Log, Spending Plan and WBS Milestones: the rules over the tables,
//! the XER round trip of a file that carries them, and the tabs through the
//! production UI (add, edit, delete, undo; empty tabs say so).
use super::budget::*;
use super::*;
use crate::qa_sweep::Driver;
use chrono::{Datelike, NaiveDate};
use openxer_xer::{Document, ProjectData};

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn minutes(y: i32, m: u32, d: u32) -> i64 {
    (date(y, m, d) - date(1899, 12, 30)).num_days() * 1440
}

fn doc() -> Document {
    openxer_xer::starter().document
}

#[test]
fn a_budget_log_is_made_on_the_first_add_and_totals_follow_the_status() {
    let mut d = doc();
    assert!(changes(&d, 1, 5).is_empty());
    let row = add_change(&mut d, 1, 5, minutes(2026, 11, 2));
    assert!(d.table(LOG).is_some(), "the table is made");
    set_change(&mut d, row, LogField::Amount, "$25,000.50").unwrap();
    set_change(&mut d, row, LogField::Responsible, "Plant Manager").unwrap();
    set_change(&mut d, row, LogField::Reason, "Scope added").unwrap();
    set_change(&mut d, row, LogField::Number, "CO-1").unwrap();
    set_change(&mut d, row, LogField::Date, "05-Nov-26").unwrap();
    let second = add_change(&mut d, 1, 5, minutes(2026, 11, 9));
    set_change(&mut d, second, LogField::Amount, "-(1,000)").unwrap();
    set_change(&mut d, second, LogField::Status, "approved").unwrap();
    // Another element's change is not listed.
    add_change(&mut d, 1, 6, minutes(2026, 11, 2));
    let log = changes(&d, 1, 5);
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].amount, 25000.5);
    assert_eq!(log[0].status, "Pending");
    assert_eq!(log[0].number, "CO-1");
    assert_eq!(log[1].amount, -1000.);
    assert_eq!(log[1].status, "Approved");
    let (orig, current, proposed) = budget_totals(100_000., &log);
    assert_eq!((orig, current, proposed), (100_000., 99_000., 124_000.5));
    // Bad entries change nothing.
    for (field, text) in [
        (LogField::Amount, "lots"),
        (LogField::Date, "never"),
        (LogField::Status, "Maybe"),
    ] {
        assert!(set_change(&mut d, row, field, text).is_err(), "{text}");
    }
    assert_eq!(changes(&d, 1, 5)[0].amount, 25000.5);
    delete_change(&mut d, row);
    assert_eq!(changes(&d, 1, 5).len(), 1);
}

#[test]
fn a_table_with_other_column_names_is_read_and_only_its_columns_are_set() {
    let mut d = doc();
    let t = d.table_mut(LOG);
    t.fields = [
        "budg_chng_id",
        "proj_id",
        "wbs_id",
        "change_date",
        "change_amount",
        "status",
    ]
    .map(String::from)
    .to_vec();
    t.add(&[
        ("budg_chng_id", "4".into()),
        ("proj_id", "1".into()),
        ("wbs_id", "5".into()),
        ("change_date", "2026-10-01 00:00".into()),
        ("change_amount", "500".into()),
        ("status", "Approved".into()),
    ]);
    let log = changes(&d, 1, 5);
    assert_eq!(log[0].amount, 500.);
    assert_eq!(log[0].status, "Approved");
    let row = add_change(&mut d, 1, 5, minutes(2026, 11, 2));
    // No reason column: the edit is refused, no column is invented.
    assert!(set_change(&mut d, row, LogField::Reason, "x").is_err());
    assert_eq!(d.table(LOG).unwrap().fields.len(), 6);
    set_change(&mut d, row, LogField::Amount, "7").unwrap();
    assert_eq!(changes(&d, 1, 5).len(), 2);
}

#[test]
fn the_spending_plan_keeps_months_tallies_and_drops_empty_rows() {
    let mut d = doc();
    set_month(&mut d, 1, 5, date(2026, 11, 1), PlanField::Spend, 1000.).unwrap();
    set_month(&mut d, 1, 5, date(2026, 12, 1), PlanField::Spend, 500.).unwrap();
    set_month(&mut d, 1, 5, date(2026, 12, 1), PlanField::Benefit, 250.).unwrap();
    let plan = months(&d, 1, 5, date(2026, 10, 15), date(2027, 1, 20), 0);
    let firsts: Vec<NaiveDate> = plan.iter().map(|m| m.first).collect();
    assert_eq!(
        firsts,
        [
            date(2026, 10, 1),
            date(2026, 11, 1),
            date(2026, 12, 1),
            date(2027, 1, 1)
        ]
    );
    assert_eq!(plan[2].last, date(2026, 12, 31));
    assert_eq!(plan[2].spend, 500.);
    let t = tallies(&plan, 5000.);
    assert_eq!(t.spend_tally, [0., 1000., 1500., 1500.]);
    assert_eq!(t.benefit_tally, [0., 0., 250., 250.]);
    assert_eq!(t.undistributed, [5000., 4000., 3500., 3500.]);
    assert_eq!(t.benefit_variance, [0., -1000., -1250., -1250.]);
    assert_eq!((t.total_spend, t.total_benefit), (1500., 250.));
    // A month set back to nothing loses its row; none is made for a zero.
    set_month(&mut d, 1, 5, date(2026, 11, 1), PlanField::Spend, 0.).unwrap();
    set_month(&mut d, 1, 5, date(2027, 3, 1), PlanField::Spend, 0.).unwrap();
    assert_eq!(d.table(PLAN).unwrap().rows.len(), 1);
    assert!(set_month(&mut d, 1, 5, date(2027, 3, 1), PlanField::Spend, f64::NAN).is_err());
    // The window is padded to the least number of months.
    assert_eq!(
        months(&d, 1, 5, date(2026, 1, 1), date(2026, 1, 1), 12).len(),
        12
    );
}

#[test]
fn milestones_are_added_edited_ordered_and_counted_by_weight() {
    let mut d = doc();
    assert!(milestones(&d, 1, 10).is_empty());
    let a = add_milestone(&mut d, 1, 10);
    let b = add_milestone(&mut d, 1, 10);
    set_milestone(&mut d, a, StepField::Name, "Foundations poured").unwrap();
    set_milestone(&mut d, a, StepField::Weight, "3").unwrap();
    set_milestone(&mut d, b, StepField::Name, "Steel erected").unwrap();
    set_milestone(&mut d, a, StepField::Done, "Yes").unwrap();
    let list = milestones(&d, 1, 10);
    assert_eq!(list[0].name, "Foundations poured");
    assert_eq!(milestone_percent(&list), 75.);
    assert!(set_milestone(&mut d, a, StepField::Name, "  ").is_err());
    assert!(set_milestone(&mut d, a, StepField::Weight, "-2").is_err());
    assert!(set_milestone(&mut d, a, StepField::Done, "maybe").is_err());
    assert_eq!(move_milestone(&mut d, 1, 10, 0, true), None);
    assert_eq!(move_milestone(&mut d, 1, 10, 0, false), Some(1));
    let list = milestones(&d, 1, 10);
    assert_eq!(list[0].name, "Steel erected");
    assert_eq!(list[1].name, "Foundations poured");
    // Another element's milestones are separate.
    assert!(milestones(&d, 1, 20).is_empty());
    delete_milestone(&mut d, list[0].row);
    assert_eq!(milestones(&d, 1, 10).len(), 1);
}

/// A project file that carries the three tables, as a synthetic fixture.
fn with_tables() -> ProjectData {
    let mut d = doc();
    for (table, rows) in [
        (
            LOG,
            vec![vec![
                "1",
                "1",
                "10",
                "2026-10-01 00:00",
                "2500",
                "A. Lead",
                "Approved",
                "Added pump",
                "CO-7",
            ]],
        ),
        (
            PLAN,
            vec![
                vec![
                    "1",
                    "1",
                    "10",
                    "2026-11-01 00:00",
                    "2026-11-30 23:59",
                    "1200",
                    "300",
                ],
                vec![
                    "2",
                    "1",
                    "10",
                    "2026-12-01 00:00",
                    "2026-12-31 23:59",
                    "800",
                    "0",
                ],
            ],
        ),
        (
            STEPS,
            vec![
                vec!["1", "1", "10", "10", "Design done", "2", "", "Y"],
                vec![
                    "2",
                    "1",
                    "10",
                    "20",
                    "Issued for construction",
                    "1",
                    "",
                    "N",
                ],
            ],
        ),
    ] {
        let t = crate::records::ensure_table(&mut d, table);
        for r in rows {
            t.rows.push(r.iter().map(|s| s.to_string()).collect());
        }
    }
    ProjectData::from_document(d, 1, "budget".into()).unwrap()
}

#[test]
fn the_tables_round_trip_through_xer_save_and_reload_and_other_tables_pass_through() {
    let p = with_tables();
    let rows = |p: &ProjectData, t: &str| p.document.table(t).unwrap().rows.clone();
    let text = p.updated_document().text();
    let back = ProjectData::from_document(Document::parse(&text).unwrap(), 1, "rt".into()).unwrap();
    for t in [LOG, PLAN, STEPS] {
        assert_eq!(rows(&back, t), rows(&p, t), "{t} survives the export");
        assert_eq!(
            back.document.table(t).unwrap().fields,
            p.document.table(t).unwrap().fields
        );
    }
    assert_eq!(changes(&back.document, 1, 10)[0].number, "CO-7");
    assert_eq!(
        milestone_percent(&milestones(&back.document, 1, 10)),
        2. / 3. * 100.
    );
    // An edit survives the workspace save and reopen.
    let mut edited = back.clone();
    set_month(
        &mut edited.document,
        1,
        10,
        date(2026, 11, 1),
        PlanField::Spend,
        1500.,
    )
    .unwrap();
    let dir = std::env::temp_dir().join(format!("openxer-budget-{}.db", std::process::id()));
    let mut store = crate::store::Store::open(dir.clone()).unwrap();
    store.save(&edited).unwrap();
    let reopened = store.last().unwrap().unwrap();
    assert_eq!(
        months(
            &reopened.document,
            1,
            10,
            date(2026, 11, 1),
            date(2026, 11, 1),
            0
        )[0]
        .spend,
        1500.
    );
    drop(store);
    let _ = std::fs::remove_file(dir);
    // Tables this build does not know pass through untouched.
    let mut with_unknown = with_tables();
    let t = with_unknown.document.table_mut("MYSTERY");
    t.fields = vec!["a".into()];
    t.rows.push(vec!["kept".into()]);
    let text = with_unknown.updated_document().text();
    let again = Document::parse(&text).unwrap();
    assert_eq!(
        again.table("MYSTERY").unwrap().rows,
        vec![vec!["kept".to_string()]]
    );
}

fn open_projects_tab(d: &mut Driver, tab: &str) {
    d.click("directory_projects");
    let p = &d.app.project;
    d.app.views.projects.selected = Some((p.source_name.clone(), p.model.project.proj_id));
    d.app.views.projects.show_details = true;
    d.app.views.projects.tab = projects::TABS.iter().position(|t| *t == tab).unwrap();
    d.frames(3);
}

fn has_text(d: &mut Driver, text: &str) -> bool {
    d.texts().iter().any(|(t, _)| t == text)
}

fn node_project() -> ProjectData {
    let mut p = wbs::tests::with_project_node();
    crate::scheduling::schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

#[test]
fn the_budget_log_tab_adds_edits_deletes_and_undoes_one_step_each() {
    let mut d = Driver::new("budget-log", node_project());
    open_projects_tab(&mut d, "Budget Log");
    assert!(
        has_text(&mut d, "No budget changes are recorded."),
        "an empty log says so"
    );
    let steps = d.app.history.steps();
    d.click("project_budget_buttons_0");
    d.frames(2);
    let node = wbs::project_node(&d.app.project).unwrap();
    assert_eq!(changes(&d.app.project.document, 1, node).len(), 1);
    assert_eq!(d.app.history.steps(), steps + 1, "Add is one undo step");
    // Edit the amount in place.
    let row = changes(&d.app.project.document, 1, node)[0].row;
    set_change(&mut d.app.project.document, row, LogField::Amount, "100").unwrap();
    d.app.touch_records();
    d.frames(3);
    assert!(has_text(&mut d, &format::cost(100.)));
    // Undo removes the table rows again.
    d.app.history_move(&d.ctx.clone(), false);
    d.app.history_move(&d.ctx.clone(), false);
    d.frames(2);
    assert!(changes(&d.app.project.document, 1, node).is_empty());
    d.app.history_move(&d.ctx.clone(), true);
    d.app.history_move(&d.ctx.clone(), true);
    d.frames(2);
    assert_eq!(changes(&d.app.project.document, 1, node).len(), 1);
    d.click("project_budget_buttons_1");
    d.frames(2);
    assert!(changes(&d.app.project.document, 1, node).is_empty());
    assert_eq!(d.app.error, None);
}

#[test]
fn the_spending_plan_tab_lists_months_with_a_total_row_and_scrolls() {
    let mut d = Driver::new("budget-plan", node_project());
    let node = wbs::project_node(&d.app.project).unwrap();
    set_month(
        &mut d.app.project.document,
        1,
        node,
        date(2026, 11, 1),
        PlanField::Spend,
        1200.,
    )
    .unwrap();
    d.app.touch_records();
    open_projects_tab(&mut d, "Spending Plan");
    assert!(has_text(&mut d, "Total"));
    assert!(has_text(&mut d, &format::cost(1200.)));
    assert!(has_text(&mut d, "Undistributed Current Variance"));
    assert!(d.has("project_spending_row_0"));
    d.click("project_spending_row_1");
    assert_eq!(d.app.views.budget.plan_row, Some(0));
    assert_eq!(d.app.error, None);
}

#[test]
fn the_wbs_milestones_tab_adds_edits_orders_and_undoes() {
    let mut d = Driver::new("budget-steps", node_project());
    d.click("directory_wbs");
    d.app.views.wbs.selected = Some(10);
    d.app.views.wbs.show_details = true;
    d.app.views.wbs.tab = wbs::TABS
        .iter()
        .position(|t| *t == "WBS Milestones")
        .unwrap();
    d.frames(3);
    assert!(has_text(&mut d, "No WBS milestones are recorded."));
    let steps = d.app.history.steps();
    d.click("wbs_milestones_buttons_0");
    d.click("wbs_milestones_buttons_0");
    d.frames(2);
    assert_eq!(milestones(&d.app.project.document, 1, 10).len(), 2);
    assert_eq!(d.app.history.steps(), steps + 2, "one undo step each");
    let list = milestones(&d.app.project.document, 1, 10);
    set_milestone(
        &mut d.app.project.document,
        list[0].row,
        StepField::Done,
        "Yes",
    )
    .unwrap();
    d.app.touch_records();
    d.frames(3);
    assert!(has_text(&mut d, "50%"));
    // Down moves the selected (second added) milestone: it is selected, so
    // Up is the live one.
    d.click("wbs_milestones_up");
    d.frames(2);
    let list = milestones(&d.app.project.document, 1, 10);
    assert!(list[0].name == "New WBS Milestone" && !list[0].done);
    d.click("wbs_milestones_buttons_1");
    assert_eq!(milestones(&d.app.project.document, 1, 10).len(), 1);
    d.app.history_move(&d.ctx.clone(), false);
    d.frames(2);
    assert_eq!(milestones(&d.app.project.document, 1, 10).len(), 2);
    assert_eq!(d.app.error, None);
}

#[test]
fn extreme_dates_in_a_file_cannot_make_the_plan_huge_or_slow() {
    // Processor time, so a busy machine cannot fail the bound.
    let started = crate::util::CpuTimer::start();
    let mut d = doc();
    let t = crate::records::ensure_table(&mut d, PLAN);
    for (start, end) in [
        ("0001-01-01 00:00", "9999-12-31 23:59"),
        ("9999-12-01 00:00", "9999-12-31 23:59"),
        ("1000-01-01 00:00", "1000-01-31 23:59"),
        ("2026-11-01 00:00", "2026-11-30 23:59"),
        ("2026-12-01 00:00", "0001-01-01 00:00"),
        ("nonsense", "also nonsense"),
    ] {
        t.add(&[
            ("proj_id", "1".into()),
            ("wbs_id", "5".into()),
            ("start_date", start.into()),
            ("end_date", end.into()),
            ("spend_cost", "1e308".into()),
            ("benefit_cost", "5".into()),
        ]);
    }
    let far = (date(1, 1, 1), date(9999, 12, 31));
    for (from, to) in [far, (far.1, far.0), (date(2026, 1, 1), date(9999, 12, 31))] {
        let (plan, skipped) = months_checked(&d, 1, 5, from, to, 24);
        assert!(
            plan.len() <= MAX_MONTHS + MAX_ROWS.min(40),
            "{} months",
            plan.len()
        );
        assert_eq!(
            skipped, 3,
            "year 1 and year 9999 and 1000 rows are left out"
        );
        assert!(plan
            .iter()
            .all(|m| m.first.year() >= 1900 && m.first.year() <= 2200));
        let t = tallies(&plan, 1.);
        assert_eq!(t.spend_tally.len(), plan.len());
    }
    // Setting a month outside the years is refused.
    assert!(set_month(&mut d, 1, 5, date(9999, 12, 1), PlanField::Spend, 1.).is_err());
    assert!(set_month(&mut d, 1, 5, date(1, 1, 1), PlanField::Spend, 1.).is_err());
    // Minutes at the extremes of i64 are not a calendar day.
    for m in [i64::MIN, i64::MAX, i64::MIN / 2, i64::MAX / 2] {
        assert_eq!(safe_day(m), None);
    }
    assert_eq!(safe_day(0), Some(date(1899, 12, 30)));
    // A budget log and milestones with a million-row look are bounded.
    let t = crate::records::ensure_table(&mut d, LOG);
    for _ in 0..(MAX_ROWS + 50) {
        t.add(&[
            ("proj_id", "1".into()),
            ("wbs_id", "5".into()),
            ("chng_date", "9999-12-31 00:00".into()),
            ("chng_cost", "1".into()),
        ]);
    }
    assert_eq!(changes(&d, 1, 5).len(), MAX_ROWS);
    let t = crate::records::ensure_table(&mut d, STEPS);
    for _ in 0..(MAX_ROWS + 50) {
        t.add(&[
            ("proj_id", "1".into()),
            ("wbs_id", "5".into()),
            ("seq_num", "9223372036854775807".into()),
            ("step_wt", "1e308".into()),
        ]);
    }
    let list = milestones(&d, 1, 5);
    assert_eq!(list.len(), MAX_ROWS);
    let _ = milestone_percent(&list);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn a_spending_plan_tab_on_a_crafted_project_still_draws() {
    let mut p = node_project();
    let node = wbs::project_node(&p).unwrap();
    let t = crate::records::ensure_table(&mut p.document, PLAN);
    t.add(&[
        ("proj_id", "1".into()),
        ("wbs_id", node.to_string()),
        ("start_date", "0001-01-01 00:00".into()),
        ("end_date", "9999-12-31 00:00".into()),
        ("spend_cost", "5".into()),
    ]);
    let started = std::time::Instant::now();
    let mut d = Driver::new("budget-crafted", p);
    open_projects_tab(&mut d, "Spending Plan");
    assert!(has_text(&mut d, "Total"));
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
}

#[test]
fn a_long_budget_log_scrolls_and_the_new_row_edits_inside_the_table() {
    let mut d = Driver::new("budget-log-scroll", node_project());
    open_projects_tab(&mut d, "Budget Log");
    for _ in 0..30 {
        d.click("project_budget_buttons_0");
        d.frames(1);
    }
    d.frames(2);
    let node = wbs::project_node(&d.app.project).unwrap();
    let n = changes(&d.app.project.document, 1, node).len();
    assert_eq!(n, 30);
    // The new (last) row is selected, scrolled into view above the buttons,
    // and its edit boxes sit on it.
    let last = d.rect(&format!("project_budget_log_row_{}", n - 1));
    let buttons = d.rect("project_budget_buttons_0");
    assert!(
        last.bottom() <= buttons.top(),
        "the last row {last:?} is above the buttons {buttons:?}"
    );
    assert!(d.has("project_budget_log_scroll_thumb"), "a scroll bar");
    let cell = d.rect("project_budget_cell_1");
    assert!(
        (cell.center().y - last.center().y).abs() < 2.,
        "the edit box {cell:?} sits on the row {last:?}"
    );
    // The scroll bar's up arrow moves the rows back down.
    d.click("project_budget_log_scroll_up");
    d.frames(1);
    let moved = d.rect(&format!("project_budget_log_row_{}", n - 2));
    assert!((moved.top() - last.top()).abs() < 1., "one row up");
    assert_eq!(d.app.error, None);
}

/// Two spending plan rows in one month (a file can carry them) total both,
/// and a typed month total replaces both without counting either twice.
#[test]
fn two_plan_rows_in_one_month_total_both() {
    let mut d = doc();
    set_month(&mut d, 1, 5, date(2026, 11, 1), PlanField::Spend, 1000.).unwrap();
    set_month(&mut d, 1, 5, date(2026, 11, 1), PlanField::Benefit, 100.).unwrap();
    let t = d.table_mut(PLAN);
    let mut second = t.rows[0].clone();
    let start = t.field("start_date").unwrap();
    let spend = t.field("spend_cost").unwrap();
    let benefit = t.field("benefit_cost").unwrap();
    let id = t.field("wbs_budg_id").unwrap();
    second[id] = "99".into();
    second[start] = "2026-11-15 00:00".into();
    second[spend] = "250".into();
    second[benefit] = "0".into();
    t.rows.push(second);
    let nov = |d: &Document| {
        months(d, 1, 5, date(2026, 11, 1), date(2026, 11, 1), 0)
            .into_iter()
            .find(|m| m.first == date(2026, 11, 1))
            .unwrap()
    };
    assert_eq!((nov(&d).spend, nov(&d).benefit), (1250., 100.));
    set_month(&mut d, 1, 5, date(2026, 11, 1), PlanField::Spend, 900.).unwrap();
    assert_eq!((nov(&d).spend, nov(&d).benefit), (900., 100.));
    // The second row held only spending, so it is gone.
    assert_eq!(d.table(PLAN).unwrap().rows.len(), 1);
}
