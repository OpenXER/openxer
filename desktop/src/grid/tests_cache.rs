//! Values kept between frames (column lookups, band totals, earned value
//! parts, row builds, the view memo, the network layout and the workspace
//! listings) must give exactly what computing them afresh gives: after
//! edits, F9, undo and redo, document edits, filter and layout changes, and
//! in every pane and view that keeps them.
use super::columns::{self, Value};
use super::filters::{Criterion, Op, UserFilter, GROUP_ANY};
use super::layout::{ColumnSetting, GroupLevel};
use super::tests_h3::progressed_starter;
use super::*;
use crate::qa_sweep::Driver;
use std::rc::Rc;

/// Every added column group, so earned value, percents, steps, counts and
/// feedback all draw with band totals.
fn add_all_columns(d: &mut Driver) {
    let catalog = columns::catalog(d.app.shown());
    for c in catalog.iter().filter(|c| {
        [
            columns_ext::EARNED_VALUE,
            columns::PERCENTS,
            columns_ext::STEPS,
            columns_ext::NUMBER_OF_ACTIVITIES,
            columns_ext::FEEDBACK,
        ]
        .contains(&c.group.as_str())
    }) {
        if !d.app.grid.layout.columns.iter().any(|s| s.id == c.id) {
            d.app
                .grid
                .layout
                .columns
                .push(ColumnSetting::new(&c.id, c.width));
        }
    }
    d.app.drop_frame_cache();
}

fn shapes(output: egui::FullOutput) -> String {
    format!("{:?}", output.shapes)
}

/// Draw a frame with the kept values, then the same frame with all of them
/// dropped, and require identical shapes.
fn same_frame(d: &mut Driver, step: &str) {
    d.frames(3);
    let kept = shapes(d.frame(vec![]));
    d.app.drop_kept_values();
    let fresh = shapes(d.frame(vec![]));
    if kept != fresh {
        let at = kept
            .bytes()
            .zip(fresh.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or(kept.len().min(fresh.len()));
        let from = at.saturating_sub(300);
        panic!(
            "{step}: kept values draw differently\nkept:  {}\nfresh: {}",
            &kept[from..(at + 300).min(kept.len())],
            &fresh[from..(at + 300).min(fresh.len())]
        );
    }
    assert!(d.app.error.is_none(), "{step}: {:?}", d.app.error);
}

/// The kept lookups give every column value and band total of the table
/// that fresh lookups give.
fn same_values(d: &Driver, step: &str) {
    let app = &d.app;
    let p = app.shown();
    let kept = app.shared_index();
    let fresh = app.grid_index();
    let catalog = columns::catalog(p);
    let cols = app.visible_columns(&catalog);
    for i in 0..p.model.activities.len() {
        for c in &cols {
            assert_eq!(
                columns::value(p, &kept, c, i),
                columns::value(p, &fresh, c, i),
                "{step}: {} of activity {i}",
                c.id
            );
        }
    }
    let rows = app.rows();
    let built = app.build_rows(&fresh, &catalog);
    assert_eq!(rows, built.rows, "{step}: kept rows");
    let bands = app.grid.bands.borrow().clone();
    assert_eq!(bands, built.bands, "{step}: kept bands");
    for b in &bands {
        for c in &cols {
            // Twice: the first may compute, the second must come back kept.
            for _ in 0..2 {
                assert_eq!(
                    group::band_total_cached(p, &kept, c, &b.members),
                    group::band_total(p, &fresh, c, &b.members),
                    "{step}: {} total of band {}",
                    c.id,
                    b.label
                );
            }
        }
    }
}

fn check(d: &mut Driver, step: &str) {
    same_frame(d, step);
    same_values(d, step);
}

fn lookahead() -> UserFilter {
    let row = |column: &str, op: Op, value: &str, high: &str, level: usize| Criterion {
        column: column.into(),
        op,
        value: value.into(),
        high: high.into(),
        level,
    };
    UserFilter {
        name: "Lookahead".into(),
        match_all: true,
        rows: vec![
            row("start", Op::Within, "DD", "DD + 3W", 0),
            row(GROUP_ANY, Op::Equals, "", "", 0),
            row("activity_status", Op::Equals, "In Progress", "", 1),
            row("activity_status", Op::Equals, "Not Started", "", 1),
        ],
    }
}

#[test]
fn kept_table_values_match_fresh_ones_after_edits_undo_filters_and_layouts() {
    let mut d = Driver::new("kept-table", progressed_starter());
    d.home();
    add_all_columns(&mut d);
    assert!(d.app.grid.layout.group_sort.show_group_totals);
    check(&mut d, "added columns");
    let ev_column = columns::catalog(d.app.shown())
        .into_iter()
        .find(|c| c.id == "budgeted_total_cost" || c.group == columns_ext::EARNED_VALUE)
        .unwrap();
    let ev_of = |d: &Driver| -> Vec<Value> {
        let ix = d.app.shared_index();
        (0..d.app.project.model.activities.len())
            .map(|i| columns::value(&d.app.project, &ix, &ev_column, i))
            .collect()
    };
    let schedule_of = |d: &Driver| -> Vec<Value> {
        let catalog = columns::catalog(d.app.shown());
        let c = columns::find(&catalog, "schedule_percent_complete").unwrap();
        let ix = d.app.shared_index();
        (0..d.app.project.model.activities.len())
            .map(|i| columns::value(&d.app.project, &ix, c, i))
            .collect()
    };
    let _ = ev_of(&d);

    // An edit, recorded, then F9 moves dates under the kept lookups.
    let before = schedule_of(&d);
    let ix_before = d.app.shared_index();
    let i = d.visible_activities()[2];
    assert!(d.app.apply_cell(i, "original_duration", "9d"));
    d.frames(2);
    assert!(
        !Rc::ptr_eq(&ix_before, &d.app.shared_index()),
        "an edit replaces the kept lookups"
    );
    check(&mut d, "after an edit");
    assert!(d.f9());
    check(&mut d, "after F9");
    let after = schedule_of(&d);
    assert_ne!(before, after, "the edit and F9 change schedule percents");

    // Undo and redo.
    let ctx = d.ctx.clone();
    d.app.history_move(&ctx, false);
    check(&mut d, "after undo");
    d.app.history_move(&ctx, false);
    check(&mut d, "after a second undo");
    assert_eq!(schedule_of(&d), before, "undo restores the values");
    d.app.history_move(&ctx, true);
    check(&mut d, "after redo");

    // A document edit: a step completed, an assignment's remaining units.
    let t = d.app.project.document.table_mut("TASKPROC");
    let row = (0..t.rows.len())
        .find(|&r| t.value(&t.rows[r], "complete_flag") == "N")
        .unwrap();
    t.set(row, "complete_flag", "Y".into());
    d.app.touch_records();
    d.frames(2);
    check(&mut d, "after a step edit");
    let t = d.app.project.document.table_mut("TASKRSRC");
    t.set(0, "remain_qty", "4".into());
    t.set(0, "remain_cost", "200".into());
    d.app.touch_records();
    d.frames(2);
    check(&mut d, "after an assignment edit");

    // Filters: standard, nested lookahead, highlight.
    d.app.grid.layout.filter.all_activities = false;
    d.app.grid.layout.filter.selected = vec![filters::Standard::NotStarted.key()];
    check(&mut d, "Not Started filter");
    let f = lookahead();
    d.app.grid.user_filters.push(f.clone());
    d.app.grid.layout.filter.selected = vec![f.key()];
    check(&mut d, "lookahead filter");
    d.app.grid.layout.filter.highlight = true;
    check(&mut d, "lookahead filter, highlighted");
    d.app.grid.layout.filter = Default::default();

    // Layouts: group by start week under WBS, sort, collapse a band.
    d.app.grid.layout.group_sort.levels.push(GroupLevel {
        column: "start".into(),
        to_level: None,
        indent: true,
        interval: "week".into(),
    });
    d.app.grid.layout.group_sort.sort = vec![layout::SortKey {
        column: "activity_name".into(),
        descending: true,
    }];
    check(&mut d, "grouped by start week");
    let key = d.app.grid.bands.borrow()[1].key.clone();
    d.app.collapsed.insert(key);
    check(&mut d, "a collapsed band");
    d.app.collapsed.clear();
    d.app.grid.layout.table.line_numbers = true;
    d.app.grid.layout.table.optimize_rows = Some(0);
    d.app.gantt.options.spotlight = true;
    check(&mut d, "line numbers, optimized rows and spotlight");
    d.app.grid.layout.columns.truncate(3);
    d.app.drop_frame_cache();
    check(&mut d, "fewer columns");
    d.assert_clean("kept table values");
}

#[test]
fn kept_pane_and_view_values_match_fresh_ones() {
    let mut d = Driver::new("kept-panes", progressed_starter());
    d.home();
    // Bottom panes, the usage display options among them.
    for view in [
        panes::BottomView::ActivityTable,
        panes::BottomView::Gantt,
        panes::BottomView::ActivityUsageSheet,
        panes::BottomView::ActivityUsageProfile,
        panes::BottomView::ResourceUsageSheet,
        panes::BottomView::ResourceUsageProfile,
        panes::BottomView::TraceLogic,
    ] {
        d.app.set_bottom(Some(view));
        for (costs, cumulative, interval) in [
            (false, false, panes::usage::Interval::Timescale),
            (true, true, panes::usage::Interval::Week),
        ] {
            let u = &mut d.app.panes.usage;
            (u.costs, u.cumulative, u.interval) = (costs, cumulative, interval);
            check(&mut d, &format!("{view:?} costs {costs}"));
        }
    }
    d.app
        .set_bottom(Some(panes::BottomView::ActivityUsageSheet));
    d.frames(2);
    let hits = d.app.views.memo.borrow().hits;
    d.frames(2);
    assert!(
        d.app.views.memo.borrow().hits > hits,
        "the usage panes reuse kept values"
    );
    // An assignment edit reaches every pane.
    let t = d.app.project.document.table_mut("TASKRSRC");
    t.set(1, "remain_qty", "12".into());
    t.set(1, "remain_cost", "600".into());
    d.app.touch_records();
    d.frames(2);
    check(&mut d, "usage after an assignment edit");
    d.app
        .set_bottom(Some(panes::BottomView::ActivityUsageProfile));
    d.app.panes.usage.selected_only = true;
    check(&mut d, "activity profile of the selection");
    d.app.panes.usage = Default::default();
    d.home();

    // The network, before and after a box drag and an edit.
    d.app.set_top(panes::TopView::Network);
    d.app.set_bottom(None);
    check(&mut d, "network");
    let (_, rect) = d.app.panes.network.boxes[0];
    let from = rect.center();
    let to = from + Vec2::new(120., 40.);
    let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let _ = d.frame(vec![egui::Event::PointerMoved(from)]);
    let _ = d.frame(vec![button(from, true)]);
    for k in 1..=4 {
        let at = from + (to - from) * (k as f32 / 4.);
        let _ = d.frame(vec![egui::Event::PointerMoved(at)]);
    }
    let _ = d.frame(vec![button(to, false), egui::Event::PointerGone]);
    assert!(!d.app.panes.network.positions.is_empty());
    check(&mut d, "network after a drag");
    let i = d.visible_activities()[1];
    assert!(d.app.apply_cell(i, "original_duration", "7d"));
    assert!(d.f9());
    check(&mut d, "network after an edit and F9");
    d.app.network_reorganize();
    d.home();

    // Views and dialogs.
    for view in ["directory_wbs", "directory_projects", "directory_resources"] {
        d.click(view);
        check(&mut d, view);
    }
    d.click("directory_wbs");
    for tab in 0..crate::views::wbs::TABS.len() {
        d.app.views.wbs.tab = tab;
        check(&mut d, &format!("WBS tab {tab}"));
    }
    let collapsed = d.app.project.wbs[1].id;
    d.app.views.wbs.collapsed.insert(collapsed);
    check(&mut d, "WBS with a collapsed element");
    d.click("directory_activities");
    d.click("menu_bar_project");
    d.click("menu_resource_assignments");
    check(&mut d, "Resource Assignments");
    d.app.views.assignments.first_minute =
        Some(d.app.project.model.project.data_date.unwrap() as f64 + 3. * DAY as f64);
    check(&mut d, "Resource Assignments, later dates");
    let t = d.app.project.document.table_mut("TASKRSRC");
    t.set(2, "remain_qty", "3".into());
    d.app.touch_records();
    d.frames(2);
    check(&mut d, "Resource Assignments after an edit");
    d.home();
    d.app.open_enterprise(crate::enterprise::Dialog::Eps);
    check(&mut d, crate::enterprise::Dialog::Eps.title());
    d.key(Key::Escape);
    d.frames(2);
    d.assert_clean("kept pane values");
}

#[test]
fn kept_lookups_are_reused_until_the_project_or_their_inputs_change() {
    let mut d = Driver::new("kept-reuse", progressed_starter());
    d.home();
    add_all_columns(&mut d);
    d.frames(2);
    let first = d.app.shared_index();
    d.frames(3);
    assert!(
        Rc::ptr_eq(&first, &d.app.shared_index()),
        "kept across frames"
    );
    // Band totals drawn by the table are kept in the lookups.
    assert!(!first.band_totals.borrow().is_empty());
    // A changed critical definition or check threshold builds new lookups.
    d.app.sched.check_params.large_duration_hours += 1.;
    let second = d.app.shared_index();
    assert!(!Rc::ptr_eq(&first, &second));
    assert!(Rc::ptr_eq(&second, &d.app.shared_index()));
    // An unrecorded edit is never kept; once recorded the new snapshot is.
    let i = d.visible_activities()[0];
    d.app.project.model.activities[i].rem += 60;
    let edited = d.app.shared_index();
    assert!(!Rc::ptr_eq(&second, &edited));
    assert!(!Rc::ptr_eq(&edited, &d.app.shared_index()), "not kept");
    d.frames(2);
    let recorded = d.app.shared_index();
    assert!(Rc::ptr_eq(&recorded, &d.app.shared_index()));
    // The memo keeps nothing for an unrecorded edit and drops all of a
    // snapshot when the next one comes.
    let serial = d.app.project_serial();
    assert!(serial.is_some());
    let calls = std::cell::Cell::new(0);
    let value = |app: &App| {
        app.memo(app.project_serial(), "test", 1, || {
            calls.set(calls.get() + 1);
            calls.get()
        })
    };
    assert_eq!(*value(&d.app), 1);
    assert_eq!(*value(&d.app), 1, "kept");
    d.app.project.model.activities[i].rem += 60;
    assert_eq!(*value(&d.app), 2, "an unrecorded edit computes");
    assert_eq!(*value(&d.app), 3, "and keeps nothing");
    d.frames(2);
    assert_eq!(*value(&d.app), 4, "a new snapshot computes");
    assert_eq!(*value(&d.app), 4);
    // One name with another value type starts that kind afresh.
    let other = d.app.memo(d.app.project_serial(), "test", 1, || "text");
    assert_eq!(*other, "text");
}

#[test]
fn workspace_listings_follow_saves_and_direct_writes() {
    let mut d = Driver::new("kept-listings", progressed_starter());
    let first = d.app.store.list().unwrap();
    assert_eq!(first, d.app.store.list().unwrap());
    let stamps = d.app.store.stamps().unwrap();
    // Another project saved.
    let mut other = openxer_xer::starter();
    other.source_name = "other.xer".into();
    other.model.project.proj_id = 77;
    other.model.project.name = "Other".into();
    d.app.store.save(&other).unwrap();
    let listed = d.app.store.list().unwrap();
    assert_eq!(listed.len(), first.len() + 1);
    assert_eq!(d.app.store.stamps().unwrap().len(), stamps.len() + 1);
    // A direct write through the connection (as workspace undo does).
    d.app
        .store
        .connection()
        .execute(
            "UPDATE projects SET title='Renamed' WHERE project_key=77",
            [],
        )
        .unwrap();
    assert!(d
        .app
        .store
        .list()
        .unwrap()
        .iter()
        .any(|(_, title)| title == "Renamed"));
    // Inside a transaction nothing is kept, and a rollback is seen.
    d.app.store.connection().execute_batch("BEGIN").unwrap();
    d.app
        .store
        .connection()
        .execute("DELETE FROM projects WHERE project_key=77", [])
        .unwrap();
    assert_eq!(d.app.store.list().unwrap().len(), first.len());
    d.app.store.connection().execute_batch("ROLLBACK").unwrap();
    assert_eq!(d.app.store.list().unwrap().len(), first.len() + 1);
    // A second connection's commit.
    let path = d.app.store.path.clone();
    let other = rusqlite::Connection::open(&path).unwrap();
    other
        .execute("DELETE FROM projects WHERE project_key=77", [])
        .unwrap();
    assert_eq!(d.app.store.list().unwrap(), first);
    assert_eq!(d.app.store.stamps().unwrap(), stamps);
}
