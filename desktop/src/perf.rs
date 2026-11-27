//! Release-build performance measurements of the production UI on corpus
//! project 369 (3,126 activities, 5,532 relationships).
//!
//! ```text
//! cargo.exe test -p openxer-desktop --release perf_369 -- --ignored --nocapture
//! ```
//!
//! A frame is `App::update_ui` plus tessellation of its shapes, the CPU work
//! the desktop app does each frame before the GPU upload; GPU painting is not
//! included. Results are printed and written to `artifacts/perf-369.json`.
//! The test is ignored because debug builds are an order of magnitude slower
//! and it needs the private corpus.
use super::*;
use crate::qa_sweep::Driver;
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
struct Stats {
    median: f64,
    p95: f64,
    max: f64,
    frames: usize,
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.
}

fn stats(mut samples: Vec<f64>) -> Stats {
    samples.sort_by(f64::total_cmp);
    let at = |q: f64| samples[((samples.len() - 1) as f64 * q).round() as usize];
    Stats {
        median: at(0.5),
        p95: at(0.95),
        max: *samples.last().unwrap(),
        frames: samples.len(),
    }
}

/// One timed frame: update and tessellate.
fn timed(d: &mut Driver, events: Vec<egui::Event>) -> f64 {
    let started = Instant::now();
    let output = d.frame(events);
    let _ = d.ctx.tessellate(output.shapes, output.pixels_per_point);
    ms(started.elapsed())
}

/// Idle frames with the pointer resting over the table (hover effects on).
fn idle(d: &mut Driver, n: usize) -> Stats {
    let hover = Pos2::new(300., 400.);
    let _ = d.frame(vec![egui::Event::PointerMoved(hover)]);
    d.frames(2);
    stats((0..n).map(|_| timed(d, vec![])).collect())
}

/// Frames that each scroll the table by one wheel notch.
fn scrolling(d: &mut Driver, n: usize, down: bool) -> Stats {
    let hover = Pos2::new(300., 400.);
    let _ = d.frame(vec![egui::Event::PointerMoved(hover)]);
    let delta = if down { -3. } else { 3. };
    stats(
        (0..n)
            .map(|_| {
                timed(
                    d,
                    vec![egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Line,
                        delta: Vec2::new(0., delta),
                        modifiers: Default::default(),
                    }],
                )
            })
            .collect(),
    )
}

fn scroll_to(d: &mut Driver, row: usize) {
    let rows = d.visible_activities();
    d.app.selected = Some(rows[row.min(rows.len() - 1)]);
    d.app.scroll_to_selected = true;
    d.frames(3);
}

/// Click a control with every frame timed; the slowest frame.
fn click_max(d: &mut Driver, action: &str) -> f64 {
    d.frames(2);
    let pos = d.rect(action).center();
    let mut worst: f64 = timed(d, vec![egui::Event::PointerMoved(pos)]);
    for pressed in [true, false] {
        worst = worst.max(timed(
            d,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            }],
        ));
    }
    for _ in 0..4 {
        worst = worst.max(timed(d, vec![]));
    }
    worst
}

fn once(f: impl FnOnce()) -> f64 {
    let started = Instant::now();
    f();
    ms(started.elapsed())
}

/// Process memory from the Windows process counters (no extra crate).
#[cfg(windows)]
fn memory_mb() -> (f64, f64) {
    #[repr(C)]
    #[derive(Default)]
    struct Counters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set: usize,
        working_set: usize,
        quota_peak_paged_pool: usize,
        quota_paged_pool: usize,
        quota_peak_non_paged_pool: usize,
        quota_non_paged_pool: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
        private_usage: usize,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut Counters, cb: u32) -> i32;
    }
    let mut c = Counters {
        cb: std::mem::size_of::<Counters>() as u32,
        ..Default::default()
    };
    // SAFETY: the counters struct is the documented PROCESS_MEMORY_COUNTERS_EX
    // layout with its size in `cb`, and the pseudo handle needs no closing.
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    if ok == 0 {
        return (0., 0.);
    }
    let mb = |v: usize| v as f64 / 1_048_576.;
    (mb(c.working_set), mb(c.private_usage))
}

#[cfg(not(windows))]
fn memory_mb() -> (f64, f64) {
    (0., 0.)
}

/// Change some state, then time the frames that follow; the slowest.
fn after(d: &mut Driver, change: impl FnOnce(&mut Driver)) -> f64 {
    d.frames(2);
    change(d);
    (0..7).map(|_| timed(d, vec![])).fold(0., f64::max)
}

/// Add the catalog columns of `groups` the layout does not show yet.
fn add_groups(d: &mut Driver, groups: &[&str]) -> usize {
    let catalog = grid::columns::catalog(d.app.shown());
    let mut added = 0;
    for c in catalog
        .iter()
        .filter(|c| groups.contains(&c.group.as_str()))
    {
        if !d.app.grid.layout.columns.iter().any(|s| s.id == c.id) {
            d.app
                .grid
                .layout
                .columns
                .push(grid::layout::ColumnSetting::new(&c.id, c.width));
            added += 1;
        }
    }
    d.app.drop_frame_cache();
    added
}

/// Scenarios for the later features. Each starts
/// from the Activities view on top of the Gantt with Activity Details below
/// and leaves it that way.
fn features(
    d: &mut Driver,
    n: usize,
    add: &mut impl FnMut(&str, Stats),
    ops: &mut Vec<(String, f64)>,
) {
    use grid::filters::{Criterion, Op, UserFilter, GROUP_ANY};
    let columns = d.app.grid.layout.columns.clone();
    // Earned value and percent complete columns, grouped by WBS with band
    // totals.
    let first = after(d, |d| {
        let added = add_groups(
            d,
            &[grid::columns_ext::EARNED_VALUE, grid::columns::PERCENTS],
        );
        assert!(added > 20, "{added} columns");
    });
    ops.push((
        "Earned Value columns: slowest frame while adding".into(),
        first,
    ));
    add("Earned Value and % Complete columns (idle)", idle(d, n));
    add(
        "Earned Value columns, wheel scrolling down",
        scrolling(d, n, true),
    );
    scroll_to(d, 0);
    d.app.grid.layout.columns = columns.clone();
    let first = after(d, |d| {
        let added = add_groups(
            d,
            &[
                grid::columns_ext::STEPS,
                grid::columns_ext::NUMBER_OF_ACTIVITIES,
            ],
        );
        assert_eq!(added, 7);
    });
    ops.push((
        "Steps and count columns: slowest frame while adding".into(),
        first,
    ));
    add("Steps and Number of Activities columns (idle)", idle(d, n));
    d.app.grid.layout.columns = columns.clone();
    d.app.drop_frame_cache();

    // A nested lookahead filter: starts within DD to DD + 3W and any of in
    // progress or not started.
    let row = |column: &str, op: Op, value: &str, high: &str, level: usize| Criterion {
        column: column.into(),
        op,
        value: value.into(),
        high: high.into(),
        level,
    };
    let lookahead = UserFilter {
        name: "Perf lookahead".into(),
        match_all: true,
        rows: vec![
            row("start", Op::Within, "DD", "DD + 3W", 0),
            row(GROUP_ANY, Op::Equals, "", "", 0),
            row("activity_status", Op::Equals, "In Progress", "", 1),
            row("activity_status", Op::Equals, "Not Started", "", 1),
        ],
    };
    let all = d.visible_activities().len();
    let first = after(d, |d| {
        d.app.grid.user_filters.push(lookahead.clone());
        d.app.grid.layout.filter.all_activities = false;
        d.app.grid.layout.filter.selected = vec![lookahead.key()];
        d.app.drop_frame_cache();
    });
    let kept = d.visible_activities().len();
    assert!(kept > 0 && kept < all, "lookahead keeps {kept} of {all}");
    ops.push((
        "Lookahead filter: slowest frame while applying".into(),
        first,
    ));
    add("Nested lookahead filter (idle)", idle(d, n));
    d.app.grid.layout.filter.all_activities = true;
    d.app.grid.layout.filter.selected.clear();
    d.app.grid.user_filters.retain(|f| f.name != lookahead.name);
    d.app.drop_frame_cache();

    // Group by WBS, then by start date in weeks.
    let group_sort = d.app.grid.layout.group_sort.clone();
    let first = after(d, |d| {
        let levels = &mut d.app.grid.layout.group_sort.levels;
        assert_eq!(levels.first().map(|l| l.column.as_str()), Some("wbs"));
        levels.push(grid::layout::GroupLevel {
            column: "start".into(),
            to_level: None,
            indent: true,
            interval: "week".into(),
        });
        d.app.drop_frame_cache();
    });
    ops.push((
        "WBS and start by week: slowest frame while grouping".into(),
        first,
    ));
    add("Grouped by WBS and start week (idle)", idle(d, n));
    add(
        "Grouped by WBS and start week, scrolling down",
        scrolling(d, n, true),
    );
    scroll_to(d, 0);
    d.app.grid.layout.group_sort = group_sort;
    d.app.drop_frame_cache();

    // Line numbers, Progress Spotlight and optimized row heights.
    let first = click_max(d, "tool_line_numbers");
    assert!(d.app.grid.layout.table.line_numbers);
    ops.push(("Line Numbers: slowest frame while turning on".into(), first));
    add("Line numbers on (idle)", idle(d, n));
    d.click("tool_line_numbers");
    let first = click_max(d, "tool_spotlight");
    assert!(d.app.gantt.options.spotlight);
    ops.push((
        "Progress Spotlight: slowest frame while turning on".into(),
        first,
    ));
    add("Progress Spotlight on (idle)", idle(d, n));
    d.click("tool_spotlight");
    let first = after(d, |d| {
        d.app.grid.layout.table.optimize_rows = Some(0);
        d.app.drop_frame_cache();
    });
    ops.push((
        "Optimize row height: slowest frame while turning on".into(),
        first,
    ));
    add("Optimize row height on (idle)", idle(d, n));
    add(
        "Optimize row height, wheel scrolling down",
        scrolling(d, n, true),
    );
    scroll_to(d, 0);
    d.app.grid.layout.table.optimize_rows = None;
    d.app.drop_frame_cache();
    d.frames(2);

    // Two tab groups: the Activities view beside the Additional Activities
    // View.
    d.click("menu_bar_project");
    let first = click_max(d, "menu_additional_activities");
    ops.push((
        "Additional Activities View: slowest frame while opening".into(),
        first,
    ));
    let first = click_max(d, "tool_tab_vertical");
    ops.push((
        "Second tab group: slowest frame while splitting".into(),
        first,
    ));
    assert_eq!(d.app.views.groups.groups.len(), 2);
    if d.has("dock_tab_activities@g0") {
        d.click("dock_tab_activities@g0");
    }
    add("Two tab groups, both Activities views (idle)", idle(d, n));
    d.home();

    // Bottom Activity Table and bottom Gantt.
    for (view, label) in [
        (panes::BottomView::ActivityTable, "Bottom: Activity Table"),
        (panes::BottomView::Gantt, "Bottom: Gantt"),
    ] {
        let first = after(d, |d| d.app.set_bottom(Some(view)));
        ops.push((format!("{label}: slowest frame while opening"), first));
        add(label, idle(d, n));
    }
    // The usage spreadsheet by week with costs, cumulative.
    let first = after(d, |d| {
        d.app
            .set_bottom(Some(panes::BottomView::ActivityUsageSheet));
        let u = &mut d.app.panes.usage;
        u.interval = panes::usage::Interval::Week;
        u.costs = true;
        u.cumulative = true;
    });
    ops.push((
        "Usage Spreadsheet, weekly cumulative costs: slowest frame while opening".into(),
        first,
    ));
    add(
        "Bottom: Usage Spreadsheet, weekly cumulative costs",
        idle(d, n),
    );
    d.app.panes.usage = Default::default();
    d.home();

    // The Activity Network after dragging a box.
    d.app.set_top(panes::TopView::Network);
    d.app.set_bottom(None);
    d.frames(3);
    let (_, rect) = d.app.panes.network.boxes[0];
    let from = rect.center();
    let to = from + Vec2::new(150., 60.);
    let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let mut drag = vec![timed(d, vec![egui::Event::PointerMoved(from)])];
    drag.push(timed(d, vec![button(from, true)]));
    for k in 1..=12 {
        let at = from + (to - from) * (k as f32 / 12.);
        drag.push(timed(d, vec![egui::Event::PointerMoved(at)]));
    }
    drag.push(timed(d, vec![button(to, false)]));
    assert!(!d.app.panes.network.positions.is_empty(), "the box moved");
    add("Activity Network: dragging a box", stats(drag));
    add("Activity Network after a box drag (idle)", idle(d, n));
    d.app.network_reorganize();
    d.home();

    // Every enterprise dictionary dialog.
    for which in enterprise::Dialog::ALL {
        let first = after(d, |d| d.app.open_enterprise(which));
        assert!(d.app.enterprise_open(), "{which:?}");
        ops.push((
            format!("{}: slowest frame while opening", which.title()),
            first,
        ));
        add(&format!("Dialog: {}", which.title()), idle(d, 10));
        d.key(Key::Escape);
        d.frames(2);
        assert!(!d.app.enterprise_open(), "{which:?} closes on Escape");
    }
    // The dictionaries: codes, user defined fields, cost accounts and admin
    // categories.
    use dictionaries::{CodeKind, Dictionary};
    for (which, label) in [
        (Dictionary::Codes(CodeKind::Activity), "Activity Codes"),
        (Dictionary::Codes(CodeKind::Project), "Project Codes"),
        (Dictionary::Codes(CodeKind::Resource), "Resource Codes"),
        (Dictionary::Udf, "User Defined Fields"),
        (Dictionary::CostAccounts, "Cost Accounts"),
        (Dictionary::AdminCategories, "Admin Categories"),
    ] {
        let first = after(d, |d| d.app.open_dictionary(which));
        ops.push((format!("{label}: slowest frame while opening"), first));
        add(&format!("Dialog: {label}"), idle(d, 10));
        let open = |d: &Driver| {
            let st = &d.app.dictionaries;
            st.codes.is_some()
                || st.udf.is_some()
                || st.accounts.is_some()
                || st.admin.is_some()
                || st.notice.is_some()
        };
        for _ in 0..3 {
            if open(d) {
                d.key(Key::Escape);
                d.frames(2);
            }
        }
        assert!(!open(d), "{label} closes on Escape");
    }
    d.home();
}

#[test]
#[ignore = "release-build measurement on the private corpus"]
fn perf_369() {
    let path = crate::test_corpus::file("369.xer");
    if !path.exists() {
        println!("SKIPPED: {} not found (set OPENXER_CORPUS)", path.display());
        return;
    }
    if cfg!(debug_assertions) {
        println!("NOTE: debug build; numbers are not representative (use --release)");
    }
    let mut report = serde_json::Map::new();
    let mut frame_rows: Vec<(String, Stats)> = vec![];
    let mut ops: Vec<(String, f64)> = vec![];
    let mut memory: Vec<(String, (f64, f64))> = vec![("start".into(), memory_mb())];

    let mut d = Driver::new("perf", openxer_xer::starter());
    // A large shared dictionary first: every other corpus project's
    // calendars, resources and codes are in the workspace.
    let others: Vec<ProjectData> = std::fs::read_dir(path.parent().unwrap())
        .map(|entries| {
            entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xer")) && *p != path
                })
                .filter_map(|p| {
                    let doc = Document::read(&p).ok()?;
                    let pid = doc.projects().first()?.0;
                    ProjectData::from_document(doc, pid, p.display().to_string()).ok()
                })
                .collect()
        })
        .unwrap_or_default();
    let count = others.len();
    let writes: Vec<_> = others
        .into_iter()
        .map(crate::interchange::workspace::Write::Insert)
        .collect();
    let fill = once(|| {
        crate::interchange::workspace::commit(&mut d.app.store, writes).unwrap();
    });
    ops.push((
        format!("Store {count} other corpus projects (their dictionaries)"),
        fill,
    ));
    let (dict, _) = d.app.store.dictionary().unwrap();
    let dictionary_rows: usize = dict.tables.iter().map(|t| t.rows.len()).sum();
    println!("Workspace dictionary: {dictionary_rows} rows from {count} other projects");
    report.insert("dictionary_rows".into(), serde_json::json!(dictionary_rows));
    let import = once(|| d.import(&path));
    ops.push(("File > Import wizard (369.xer, Create New)".into(), import));
    memory.push(("after import".into(), memory_mb()));
    // The shared dictionary's own costs on 369.
    let id = d.app.open_set.active.unwrap();
    let hydrate = once(|| {
        d.app.store.load(id).unwrap();
    });
    ops.push((
        "Store load 369 (hydrate from the dictionary)".into(),
        hydrate,
    ));
    let project = d.app.project.clone();
    let store = once(|| {
        d.app.store.save(&project).unwrap();
    });
    ops.push(("Store save 369 (merge into the dictionary)".into(), store));
    let fingerprint = once(|| {
        openxer_xer::workspace_scope::delta::fingerprint(&d.app.project.document);
    });
    ops.push((
        "Dictionary fingerprint of 369 (per edit step)".into(),
        fingerprint,
    ));
    d.home();
    let n = 40;
    let mut add = |name: &str, s: Stats| {
        println!(
            "{name:<48} median {:>7.2} ms  p95 {:>7.2} ms  max {:>7.2} ms",
            s.median, s.p95, s.max
        );
        frame_rows.push((name.into(), s));
    };
    add("Activities, top, grouped by WBS (idle)", idle(&mut d, n));
    add(
        "Activities, wheel scrolling down",
        scrolling(&mut d, n, true),
    );
    let rows = d.visible_activities().len();
    scroll_to(&mut d, rows / 2);
    add("Activities, middle (idle)", idle(&mut d, n));
    scroll_to(&mut d, rows - 1);
    add("Activities, bottom (idle)", idle(&mut d, n));
    add(
        "Activities, wheel scrolling up",
        scrolling(&mut d, n, false),
    );
    scroll_to(&mut d, 0);
    d.app.gantt.options.show_relationships = false;
    add("Activities, relationship lines off", idle(&mut d, n));
    d.app.gantt.options.show_relationships = true;
    let groups = std::mem::take(&mut d.app.grid.layout.group_sort.levels);
    d.app.drop_frame_cache();
    add("Activities, no grouping", idle(&mut d, n));
    d.app.grid.layout.group_sort.levels = groups;
    d.app.grid.layout.filter.all_activities = false;
    d.app.grid.layout.filter.selected = vec![grid::filters::Standard::NotStarted.key()];
    d.app.drop_frame_cache();
    add("Activities, filtered (Not Started)", idle(&mut d, n));
    d.app.grid.layout.filter.all_activities = true;
    d.app.drop_frame_cache();
    // Interaction: click rows and switch detail tabs, every frame timed.
    let mut clicks = vec![];
    for k in 0..12 {
        let rows = d.visible_activities();
        let i = rows[k * 2];
        if let Some(pos) = d.cell(i, "activity_name") {
            let _ = d.frame(vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                clicks.push(timed(
                    &mut d,
                    vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    }],
                ));
            }
        }
    }
    // The Relationships tab replaced Predecessors and Successors; Steps
    // keeps the count of tabs at five.
    for tab in ["status", "relationships", "steps", "resources", "general"] {
        let pos = d.rect(&format!("detail_tab_{tab}")).center();
        let _ = d.frame(vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            clicks.push(timed(
                &mut d,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                }],
            ));
        }
        clicks.push(timed(&mut d, vec![]));
    }
    add("Interaction: row clicks and details tabs", stats(clicks));
    for (pane, label) in [
        (
            "tool_activity_usage_sheet",
            "Bottom: Activity Usage Spreadsheet",
        ),
        (
            "tool_activity_usage_profile",
            "Bottom: Activity Usage Profile",
        ),
        (
            "tool_resource_usage_sheet",
            "Bottom: Resource Usage Spreadsheet",
        ),
        (
            "tool_resource_usage_profile",
            "Bottom: Resource Usage Profile",
        ),
        ("trace", "Bottom: Trace Logic"),
    ] {
        let first = click_max(&mut d, pane);
        let s = idle(&mut d, n);
        add(label, s);
        ops.push((format!("{label}: slowest frame while opening"), first));
    }
    d.home();
    let first = click_max(&mut d, "tool_network");
    ops.push((
        "Activity Network: slowest frame while opening".into(),
        first,
    ));
    add("Top: Activity Network", idle(&mut d, n));
    d.click("tool_gantt");
    for (view, label) in [
        ("directory_wbs", "WBS view"),
        ("directory_projects", "Projects view"),
        ("directory_resources", "Resources view"),
    ] {
        let first = click_max(&mut d, view);
        ops.push((format!("{label}: slowest frame while opening"), first));
        add(label, idle(&mut d, n));
    }
    d.click("directory_activities");
    d.click("menu_bar_project");
    let first = click_max(&mut d, "menu_resource_assignments");
    ops.push((
        "Resource Assignments view: slowest frame while opening".into(),
        first,
    ));
    add("Resource Assignments view", idle(&mut d, n));
    d.home();
    memory.push(("after views".into(), memory_mb()));

    // The later features.
    features(&mut d, n, &mut add, &mut ops);
    memory.push(("after the added features".into(), memory_mb()));

    // Operations.
    ops.push((
        "F9 (schedule, write back, save)".into(),
        once(|| d.app.schedule()),
    ));
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    d.frames(2);
    memory.push(("after F9".into(), memory_mb()));
    let mut model = d.app.project.model.clone();
    let inputs = d.app.project.resources.clone();
    ops.push((
        "Engine schedule only".into(),
        once(|| schedule_model(&mut model, &inputs).unwrap()),
    ));
    // The UI thread's part of a recovery copy: cloning the changed project
    // (the worker writes the XER text).
    d.app.dirty = true;
    ops.push((
        "Recovery copy, UI thread (snapshot)".into(),
        once(|| d.app.write_recovery_copies()),
    ));
    ops.push(("Save (Commit Changes)".into(), once(|| d.app.save())));
    let t = d.time;
    d.app.open_level_resources();
    ops.push((
        "Level Resources (level, write back, save)".into(),
        once(|| d.app.run_level(t)),
    ));
    if let Some(e) = d.app.error.take() {
        println!("Level: {e}");
    }
    d.app.sched.progress.level = None;
    d.frames(2);
    memory.push((
        "after Level (one whole-project undo step)".into(),
        memory_mb(),
    ));
    for k in 2..=3 {
        d.app.open_level_resources();
        let t = d.time;
        d.app.run_level(t);
        d.app.error = None;
        d.app.sched.progress.level = None;
        d.frames(2);
        memory.push((format!("after Level x{k}"), memory_mb()));
    }
    let dir = d.dir.clone();
    ops.push((
        "Export XER (build and write)".into(),
        once(|| {
            openxer_interchange::xer_export::export_document(&[&d.app.project], "25.12")
                .unwrap()
                .write(&dir.join("perf.xer"))
                .unwrap();
        }),
    ));
    ops.push((
        "Export XLSX, modern layout".into(),
        once(|| d.app.export_excel_to(&dir.join("perf.xlsx"))),
    ));
    ops.push((
        "Export XLSX, tables layout".into(),
        once(|| {
            openxer_interchange::xlsx_export::write_tables(
                &d.app.project,
                openxer_interchange::columns::DurationUnit::Days,
                &dir.join("perf-tables.xlsx"),
            )
            .unwrap();
        }),
    ));
    memory.push(("after exports".into(), memory_mb()));
    // Undo snapshots: the per-frame check with nothing changed, then the
    // frame that records an edit.
    let revision = d.app.records_revision;
    ops.push((
        "Undo: per-frame change check (no edit)".into(),
        once(|| d.app.history.record(&d.app.project, revision, None, 0.)),
    ));
    let mut record_ms = vec![];
    for k in 0..10 {
        let i = d.visible_activities()[k];
        assert!(d
            .app
            .apply_cell(i, "activity_name", &format!("Perf edit {k}")));
        let started = Instant::now();
        d.app
            .history
            .record(&d.app.project, revision, None, 100. + k as f64);
        record_ms.push(ms(started.elapsed()));
    }
    let s = stats(record_ms);
    ops.push(("Undo: record one edit (median of 10)".into(), s.median));
    ops.push((
        "Undo: one step back".into(),
        once(|| {
            let ctx = d.ctx.clone();
            d.app.history_move(&ctx, false)
        }),
    ));
    add("Activities after 10 edits (idle)", idle(&mut d, n));
    // Back through the remaining name edits to the last leveling step.
    let ctx = d.ctx.clone();
    for _ in 0..9 {
        d.app.history_move(&ctx, false);
    }
    ops.push((
        "Undo: one leveling step back (whole project)".into(),
        once(|| d.app.history_move(&ctx, false)),
    ));
    ops.push((
        "Redo: the leveling step".into(),
        once(|| d.app.history_move(&ctx, true)),
    ));
    memory.push(("after 10 name edits (10 undo steps)".into(), memory_mb()));

    println!();
    for (name, v) in &ops {
        println!("{name:<48} {v:>9.1} ms");
    }
    for (name, (ws, private)) in &memory {
        println!("memory {name:<30} working set {ws:>7.1} MB  private {private:>7.1} MB");
    }
    report.insert(
        "frames".into(),
        serde_json::json!(frame_rows
            .iter()
            .map(|(n, s)| serde_json::json!({"name": n, "median_ms": s.median, "p95_ms": s.p95, "max_ms": s.max, "frames": s.frames}))
            .collect::<Vec<_>>()),
    );
    report.insert(
        "operations".into(),
        serde_json::json!(ops
            .iter()
            .map(|(n, v)| serde_json::json!({"name": n, "ms": v}))
            .collect::<Vec<_>>()),
    );
    report.insert(
        "memory".into(),
        serde_json::json!(memory
            .iter()
            .map(
                |(n, (w, p))| serde_json::json!({"point": n, "working_set_mb": w, "private_mb": p})
            )
            .collect::<Vec<_>>()),
    );
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts");
    std::fs::create_dir_all(&out).unwrap();
    let name = std::env::var("OPENXER_PERF_OUT").unwrap_or_else(|_| "perf-369.json".into());
    std::fs::write(
        out.join(name),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
}

// ---------------------------------------------------------------- printing

/// Click a control; the frame that handles the click (the button release)
/// and the slowest of the six frames after it.
fn click_split(d: &mut Driver, action: &str) -> (f64, f64) {
    d.frames(2);
    let pos = d.rect(action).center();
    let button = |pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let _ = timed(d, vec![egui::Event::PointerMoved(pos)]);
    let _ = timed(d, vec![button(true)]);
    let click = timed(d, vec![button(false)]);
    let later = (0..6).map(|_| timed(d, vec![])).fold(0., f64::max);
    (click, later)
}

/// Header and footer text edit applied through the open Page Setup dialog
/// (the text differs each time, so the dialog always differs from what is
/// saved).
fn edit_header_text(d: &mut Driver, k: usize) {
    let dialog = d.app.print.dialog.as_mut().expect("Page Setup is open");
    dialog.draft.header.sections[0].text = format!("Plan [project_name] rev {k}");
    dialog.draft.footer.sections[2].text = format!("[page_number] of [total_pages] {k}");
}

/// Frames pumped until the open preview is laid out; the number of pumped
/// frames and the slowest of them.
fn settle_preview(d: &mut Driver) -> (usize, f64) {
    let mut worst: f64 = 0.;
    for n in 0..4000 {
        if d.app.print_preview_ready() {
            return (n, worst);
        }
        worst = worst.max(timed(d, vec![]));
    }
    panic!("the preview never finished");
}

fn print_profile(
    d: &mut Driver,
    label: &str,
    bottom: panes::BottomView,
    rows: &mut Vec<(String, f64)>,
) {
    let mut add = |what: &str, v: f64| {
        println!("{label}: {what:<62} {v:>9.1} ms");
        rows.push((format!("{label}: {what}"), v));
    };
    d.home();
    d.app.show_details = true;
    d.app.set_bottom(Some(bottom));
    d.frames(3);
    let setup = d.app.page_setup();
    // Composing every page: what Print and Publish do, and what Apply and
    // Print Preview used to do before the first page showed.
    add(
        "compose all pages (first)",
        once(|| {
            let _ = d.app.print_with(&setup).unwrap();
        }),
    );
    let pages = d.app.print_with(&setup).unwrap().pages.len();
    println!("{label}: {pages} page(s)");
    // The result line's page plan: a layout pass, nothing painted.
    let mut auto = setup.clone();
    auto.orientation = crate::print::setup::Orientation::Automatic;
    let plans: Vec<f64> = (0..5)
        .map(|_| {
            once(|| {
                let _ = d.app.plan_summary(&auto).unwrap().unwrap();
            })
        })
        .collect();
    add(
        "page plan for the result line, Automatic (median)",
        stats(plans).median,
    );
    let mut fixed = setup.clone();
    fixed.orientation = crate::print::setup::Orientation::Landscape;
    let plans: Vec<f64> = (0..5)
        .map(|_| {
            once(|| {
                let _ = d.app.plan_summary(&fixed).unwrap().unwrap();
            })
        })
        .collect();
    add(
        "page plan for the result line, fixed (median)",
        stats(plans).median,
    );
    // Page Setup open, Apply with no preview. The dialog is shown once
    // first, so the first-use cost of its fonts and layout is not counted.
    d.app.open_page_setup();
    d.frames(3);
    d.key(Key::Escape);
    d.frames(3);
    d.app.open_page_setup();
    d.frames(3);
    edit_header_text(d, 1);
    let (click, later) = click_split(d, "page_setup_apply");
    add("Apply, no preview: the click frame", click);
    add("Apply, no preview: slowest frame after", later);
    assert!(d.app.print.preview.is_none());
    d.key(Key::Escape);
    d.frames(3);
    // Print Preview from the toolbar.
    let (click, later) = click_split(d, "tool_print_preview");
    add("open Print Preview: the click frame", click);
    add("open Print Preview: slowest frame after", later);
    let (n, worst) = settle_preview(d);
    add(
        &format!("  then {n} more frame(s) until laid out: slowest"),
        worst,
    );
    let count = d.app.print.preview.as_ref().unwrap().count();
    let s = idle(d, 12);
    add("preview showing a page, idle frame (median)", s.median);
    let split: Vec<(f64, f64)> = (0..8)
        .map(|_| {
            let started = Instant::now();
            let output = d.frame(vec![]);
            let update = ms(started.elapsed());
            let started = Instant::now();
            let _ = d.ctx.tessellate(output.shapes, output.pixels_per_point);
            (update, ms(started.elapsed()))
        })
        .collect();
    let mean = |f: fn(&(f64, f64)) -> f64| split.iter().map(f).sum::<f64>() / split.len() as f64;
    add("  of which update_ui (mean)", mean(|v| v.0));
    add("  of which tessellation (mean)", mean(|v| v.1));
    // Page through it.
    let mut paging = vec![];
    for _ in 0..count.saturating_sub(1).min(12) {
        let (click, later) = click_split(d, "preview_right");
        paging.push(click.max(later));
    }
    if !paging.is_empty() {
        let s = stats(paging);
        add("page right: slowest frame, median of clicks", s.median);
        add("page right: slowest frame, worst of clicks", s.max);
    }
    d.click("preview_first");
    d.frames(2);
    // Scroll the pages with the mouse wheel, one notch a frame, through the
    // whole printout: every frame, the ones that build a page included.
    let hover = Pos2::new(750., 450.);
    let _ = d.frame(vec![egui::Event::PointerMoved(hover)]);
    let wheel = |delta: f32| egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: Vec2::new(0., delta),
        modifiers: Default::default(),
    };
    let scrolls: Vec<f64> = (0..120).map(|_| timed(d, vec![wheel(-1.)])).collect();
    let built = d.app.print.preview.as_ref().unwrap().page;
    let s = stats(scrolls);
    add("wheel scroll through the pages: frame (median)", s.median);
    add(
        "wheel scroll through the pages: frame (95th percentile)",
        s.p95,
    );
    add("wheel scroll through the pages: slowest frame", s.max);
    println!("{label}: wheel scrolling reached page {}", built + 1);
    let zooms: Vec<f64> = (0..8)
        .map(|k| {
            let m = egui::Modifiers {
                ctrl: true,
                command: true,
                ..Default::default()
            };
            let delta = if k % 2 == 0 { -2. } else { 2. };
            timed(
                d,
                vec![egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: Vec2::new(0., delta),
                    modifiers: m,
                }],
            )
        })
        .collect();
    add("Ctrl+wheel zoom: slowest frame", stats(zooms).max);
    d.click("preview_fit");
    d.click("preview_first");
    d.frames(2);
    // The same with the page strip open (its thumbnails are built when the
    // view rests), and Find over every page.
    d.app.print.preview.as_mut().unwrap().view.thumbs = true;
    d.frames(12);
    let _ = d.frame(vec![egui::Event::PointerMoved(hover)]);
    let scrolls: Vec<f64> = (0..60).map(|_| timed(d, vec![wheel(-1.)])).collect();
    let s = stats(scrolls);
    add("wheel scroll, page strip open: frame (median)", s.median);
    add(
        "wheel scroll, page strip open: frame (95th percentile)",
        s.p95,
    );
    add("wheel scroll, page strip open: slowest frame", s.max);
    let rest: Vec<f64> = (0..12).map(|_| timed(d, vec![])).collect();
    add(
        "page strip open, view at rest: slowest frame",
        stats(rest).max,
    );
    {
        let p = d.app.print.preview.as_mut().unwrap();
        p.view.find.open = true;
        p.view.find.query = "install".into();
    }
    let search: Vec<f64> = (0..count + 4).map(|_| timed(d, vec![])).collect();
    add(
        "Find: slowest frame while searching every page",
        stats(search).max,
    );
    {
        let p = d.app.print.preview.as_mut().unwrap();
        p.view.find.open = false;
        p.view.thumbs = false;
    }
    d.click("preview_first");
    d.frames(2);
    // Apply with the preview open: text only, a margin change, then a
    // print option (which needs the canvas painted again).
    d.click("preview_page_setup");
    d.frames(3);
    edit_header_text(d, 2);
    let (click, later) = click_split(d, "page_setup_apply");
    add("Apply, preview open, header text: the click frame", click);
    add(
        "Apply, preview open, header text: slowest frame after",
        later,
    );
    let (n, worst) = settle_preview(d);
    add(
        &format!("  then {n} more frame(s) until laid out: slowest"),
        worst,
    );
    edit_header_text(d, 3);
    d.app.print.dialog.as_mut().unwrap().draft.margins.left = 0.75;
    d.frames(2);
    let (click, later) = click_split(d, "page_setup_apply");
    add("Apply, preview open, margin: the click frame", click);
    add("Apply, preview open, margin: slowest frame after", later);
    let (n, worst) = settle_preview(d);
    add(
        &format!("  then {n} more frame(s) until laid out: slowest"),
        worst,
    );
    edit_header_text(d, 4);
    let draft = &mut d.app.print.dialog.as_mut().unwrap().draft;
    draft.options.grid_lines = !draft.options.grid_lines;
    d.frames(2);
    let (click, later) = click_split(d, "page_setup_apply");
    add("Apply, preview open, grid lines: the click frame", click);
    add(
        "Apply, preview open, grid lines: slowest frame after",
        later,
    );
    let (n, worst) = settle_preview(d);
    add(
        &format!("  then {n} more frame(s) until laid out: slowest"),
        worst,
    );
    d.key(Key::Escape);
    d.frames(2);
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.print.preview.is_none());
    // Publish to PDF.
    let setup = d.app.page_setup();
    let path = d.dir.join("perf-print.pdf");
    let v = once(|| {
        d.app.write_pdf(&path, &setup).unwrap();
    });
    add("Publish to PDF (compose, write)", v);
    let bytes = std::fs::metadata(&path).unwrap().len();
    println!("{label}: PDF {bytes} bytes");
}

#[test]
#[ignore = "release-build measurement; the 369 part needs the private corpus"]
fn perf_print() {
    use panes::BottomView as B;
    if cfg!(debug_assertions) {
        println!("NOTE: debug build; numbers are not representative (use --release)");
    }
    let mut rows: Vec<(String, f64)> = vec![];
    let mut projects: Vec<(&str, Driver)> = vec![];
    let mut starter = openxer_xer::starter();
    schedule_model(&mut starter.model, &starter.resources).unwrap();
    projects.push(("starter", Driver::new("print-starter", starter)));
    let path = crate::test_corpus::file("369.xer");
    if path.exists() {
        let mut d = Driver::new("print-369", openxer_xer::starter());
        d.import(&path);
        projects.push(("369", d));
    } else {
        println!("SKIPPED 369: {} not found", path.display());
    }
    let quick = std::env::var_os("OPENXER_PERF_PRINT_QUICK").is_some();
    for (name, d) in projects.iter_mut() {
        if quick && *name != "369" {
            continue;
        }
        let first = d.visible_activities()[0];
        d.app.selected = Some(first);
        for (view, what) in [
            (B::Details, "default layout"),
            (B::ActivityUsageProfile, "bottom profile"),
            (B::TraceLogic, "Trace Logic"),
        ] {
            print_profile(d, &format!("{name}, {what}"), view, &mut rows);
            if quick {
                break;
            }
        }
    }
    let report: Vec<_> = rows
        .iter()
        .map(|(n, v)| serde_json::json!({"name": n, "ms": v}))
        .collect();
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts");
    std::fs::create_dir_all(&out).unwrap();
    let file = std::env::var("OPENXER_PERF_PRINT_OUT").unwrap_or_else(|_| "perf-print.json".into());
    std::fs::write(
        out.join(file),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
}

/// The printout of corpus project 369 is what the first version printed:
/// every dash painted on its own (the screen's way), every page built with
/// the bands of all pages in one pass. Slow (about 15 s in release).
#[test]
#[ignore = "release-build check on the private corpus"]
fn print_369_pages_are_unchanged() {
    use panes::BottomView as B;
    let path = crate::test_corpus::file("369.xer");
    if !path.exists() {
        println!("SKIPPED: {} not found", path.display());
        return;
    }
    let mut d = Driver::new("print-369-same", openxer_xer::starter());
    d.import(&path);
    for (view, what) in [(B::Details, "default"), (B::TraceLogic, "Trace Logic")] {
        d.home();
        d.app.set_bottom(Some(view));
        d.frames(3);
        let setup = d.app.page_setup();
        let ctx = d.app.print_context();
        let job = d.app.compose_job(&ctx, &setup, true).unwrap().unwrap();
        let reference = d.app.reference_pages(&ctx, &job);
        let eager = d.app.print_with(&setup).unwrap();
        assert_eq!(eager.pages, reference, "{what}: pages against the one pass");
        gantt::paint::PLAIN_DASHES.set(true);
        let plain = d.app.print_with(&setup).unwrap();
        gantt::paint::PLAIN_DASHES.set(false);
        assert_eq!(eager.pages.len(), plain.pages.len(), "{what}");
        for (k, page) in plain.pages.iter().enumerate() {
            assert_eq!(
                *page, eager.pages[k],
                "{what}: page {k} against plain dashes"
            );
        }
        println!("{what}: {} pages identical", plain.pages.len());
    }
}

fn synthetic_35k_xer() -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(16 * 1024 * 1024);
    out.push_str(
        "ERMHDR\t25.12\t2026-11-02\tProject\tOpenXER\tOpenXER\tdbxDatabaseNoName\tProject Management\tUSD\n",
    );
    out.push_str("%T\tPROJECT\n");
    out.push_str(
        "%F\tproj_id\tproj_short_name\tlast_recalc_date\tplan_start_date\tclndr_id\trem_target_link_flag\n",
    );
    out.push_str("%R\t1\tPERF35K\t2026-11-02 08:00\t2026-11-02 08:00\t1\tY\n");

    out.push_str("%T\tCALENDAR\n");
    out.push_str("%F\tclndr_id\tclndr_name\tclndr_data\n");
    let base_cal =
        openxer_xer::calendar_data::CalendarData::parse(&openxer_xer::standard_calendar_data())
            .unwrap();
    for c in 1..=10 {
        let mut cal = base_cal.clone();
        for e in 0..200_i64 {
            cal.exceptions.insert(46328 + e * 7 + (c - 1), vec![]);
        }
        let _ = writeln!(out, "%R\t{c}\tCal-{c}\t{}", cal.serialize());
    }

    out.push_str("%T\tPROJWBS\n");
    out.push_str("%F\twbs_id\tproj_id\tparent_wbs_id\twbs_short_name\twbs_name\tseq_num\n");
    out.push_str("%R\t1\t1\t\tP35K\tSynthetic 35k Root\t1\n");
    for id in 2..=10 {
        let _ = writeln!(out, "%R\t{id}\t1\t1\tL1.{id}\tArea {id}\t{id}");
    }
    for id in 11..=100 {
        let parent = 2 + ((id - 11) % 9);
        let _ = writeln!(out, "%R\t{id}\t1\t{parent}\tL2.{id}\tZone {id}\t{id}");
    }
    for id in 101..=500 {
        let parent = 11 + ((id - 101) % 90);
        let _ = writeln!(
            out,
            "%R\t{id}\t1\t{parent}\tWBS-{id}\tWork Package {id}\t{id}"
        );
    }

    out.push_str("%T\tRSRC\n");
    out.push_str("%F\trsrc_id\tclndr_id\trsrc_short_name\trsrc_name\trsrc_type\tdef_qty_per_hr\n");
    for r in 1..=20 {
        let cal = 1 + ((r - 1) % 10);
        let _ = writeln!(out, "%R\t{r}\t{cal}\tR-{r:02}\tResource {r}\tRT_Labor\t8");
    }

    out.push_str("%T\tTASK\n");
    out.push_str(
        "%F\ttask_id\tproj_id\twbs_id\tclndr_id\ttask_code\ttask_name\ttask_type\tstatus_code\tduration_type\tremain_drtn_hr_cnt\ttarget_drtn_hr_cnt\trsrc_id\n",
    );
    const N_ACT: usize = 35_000;
    for i in 0..N_ACT {
        let task_id = 1000 + i;
        let wbs_id = 101 + (i * 400 / N_ACT);
        let clndr_id = 1 + (i % 10);
        let rsrc_id = 1 + (i % 20);
        let (ttype, hrs) = if i % 40 == 0 {
            ("TT_Mile", 0)
        } else if i % 40 == 39 {
            ("TT_FinMile", 0)
        } else {
            ("TT_Task", ((i % 10) + 1) * 8)
        };
        let _ = writeln!(
            out,
            "%R\t{task_id}\t1\t{wbs_id}\t{clndr_id}\tA{i:05}\tSynthetic activity {i} scale item\t{ttype}\tTK_NotStart\tDT_FixedDUR2\t{hrs}\t{hrs}\t{rsrc_id}"
        );
    }

    out.push_str("%T\tTASKPRED\n");
    out.push_str(
        "%F\ttask_pred_id\tproj_id\tpred_proj_id\tpred_task_id\ttask_id\tpred_type\tlag_hr_cnt\n",
    );
    let mut link_id = 1usize;
    for i in 0..N_ACT {
        if i + 1 < N_ACT {
            let p = 1000 + i;
            let s = 1001 + i;
            let _ = writeln!(out, "%R\t{link_id}\t1\t1\t{p}\t{s}\tPR_FS\t0");
            link_id += 1;
        }
        if i % 2 == 0 && i + 3 < N_ACT {
            let p = 1000 + i;
            let s = 1003 + i;
            let _ = writeln!(out, "%R\t{link_id}\t1\t1\t{p}\t{s}\tPR_SS\t8");
            link_id += 1;
        }
        if i % 3 == 0 && i + 5 < N_ACT {
            let p = 1000 + i;
            let s = 1005 + i;
            let _ = writeln!(out, "%R\t{link_id}\t1\t1\t{p}\t{s}\tPR_FS\t0");
            link_id += 1;
        }
        if i % 15 == 0 && i + 5_000 < N_ACT {
            let p = 1000 + i;
            let s = 1000 + i + 5_000;
            let _ = writeln!(out, "%R\t{link_id}\t1\t1\t{p}\t{s}\tPR_FS\t0");
            link_id += 1;
        }
    }
    for hub in [1_000usize, 8_000, 15_000, 22_000, 29_000] {
        let hub_id = 1000 + hub;
        for off in 6..506 {
            let pred_id = 1000 + hub - off;
            let _ = writeln!(out, "%R\t{link_id}\t1\t1\t{pred_id}\t{hub_id}\tPR_FS\t0");
            link_id += 1;
            let succ_id = 1000 + hub + off;
            let _ = writeln!(out, "%R\t{link_id}\t1\t1\t{hub_id}\t{succ_id}\tPR_FS\t0");
            link_id += 1;
        }
    }

    out.push_str("%T\tTASKRSRC\n");
    out.push_str("%F\ttaskrsrc_id\ttask_id\tproj_id\trsrc_id\tremain_qty\ttarget_qty\tremain_qty_per_hr\ttarget_qty_per_hr\trollup_dates_flag\trsrc_type\n");
    let mut tr_id = 1usize;
    for i in 0..N_ACT {
        let task_id = 1000 + i;
        let r1 = 1 + (i % 20);
        let _ = writeln!(
            out,
            "%R\t{tr_id}\t{task_id}\t1\t{r1}\t40\t40\t1\t1\tY\tRT_Labor"
        );
        tr_id += 1;
        if i % 2 == 0 {
            let r2 = 1 + ((i + 7) % 20);
            let _ = writeln!(
                out,
                "%R\t{tr_id}\t{task_id}\t1\t{r2}\t24\t24\t1\t1\tY\tRT_Labor"
            );
            tr_id += 1;
        }
    }
    out.push_str("%E\n");
    out
}

#[test]
#[ignore = "release-build 35k-activity benchmark"]
fn perf_35k() {
    if cfg!(debug_assertions) {
        println!("NOTE: debug build; numbers are not representative (use --release)");
    }
    let mut report = serde_json::Map::new();
    let mut frame_rows: Vec<(String, Stats)> = vec![];
    let mut ops: Vec<(String, f64)> = vec![];
    let mut memory: Vec<(String, (f64, f64))> = vec![("start".into(), memory_mb())];

    let xer_text = synthetic_35k_xer();
    let mut parsed: Option<ProjectData> = None;
    let import_ms = once(|| {
        let doc = Document::parse(&xer_text).unwrap();
        let pid = doc.projects()[0].0;
        parsed = Some(ProjectData::from_document(doc, pid, "Synthetic 35k".into()).unwrap());
    });
    let mut project = parsed.unwrap();
    assert_eq!(project.model.activities.len(), 35_000);
    assert!(project.model.links.len() >= 60_000);
    ops.push(("Import XER (parse + ProjectData, 35k)".into(), import_ms));
    memory.push(("after import".into(), memory_mb()));

    let cpm_ms = once(|| {
        schedule_model(&mut project.model, &project.resources).unwrap();
    });
    ops.push(("Engine CPM only (35k)".into(), cpm_ms));

    let mut d = Driver::new("perf-35k", project);
    d.home();
    let n = 30;
    let mut add = |name: &str, s: Stats| {
        println!(
            "{name:<48} median {:>7.2} ms  p95 {:>7.2} ms  max {:>7.2} ms",
            s.median, s.p95, s.max
        );
        frame_rows.push((name.into(), s));
    };

    d.app.gantt.options.show_relationships = true;
    d.frames(3);
    add("35k idle (links ON)", idle(&mut d, n));
    add("35k scroll down (links ON)", scrolling(&mut d, n, true));
    add("35k scroll up (links ON)", scrolling(&mut d, n, false));

    d.app.gantt.options.show_relationships = false;
    d.frames(3);
    add("35k idle (links OFF)", idle(&mut d, n));
    add("35k scroll down (links OFF)", scrolling(&mut d, n, true));
    d.app.gantt.options.show_relationships = true;
    scroll_to(&mut d, 0);

    let wbs_levels = d.app.grid.layout.group_sort.levels.clone();
    let group_status = after(&mut d, |d| {
        d.app.grid.layout.group_sort.levels = vec![grid::layout::GroupLevel {
            column: "activity_status".into(),
            to_level: None,
            indent: true,
            interval: String::new(),
        }];
        d.app.drop_frame_cache();
    });
    ops.push((
        "Group by Activity Status (slowest frame)".into(),
        group_status,
    ));

    let group_start = after(&mut d, |d| {
        d.app.grid.layout.group_sort.levels = vec![grid::layout::GroupLevel {
            column: "start".into(),
            to_level: None,
            indent: true,
            interval: "Month".into(),
        }];
        d.app.drop_frame_cache();
    });
    ops.push(("Group by Start Month (slowest frame)".into(), group_start));

    d.app.grid.layout.group_sort.levels = wbs_levels;
    d.app.drop_frame_cache();
    d.frames(2);
    memory.push(("after grouping".into(), memory_mb()));

    let mut edit_ms = vec![];
    for k in 0..10 {
        let i = d.visible_activities()[k];
        let started = Instant::now();
        assert!(d
            .app
            .apply_cell(i, "activity_name", &format!("Edit 35k {k}")));
        d.app.history.record(
            &d.app.project,
            d.app.records_revision,
            None,
            200. + k as f64,
        );
        edit_ms.push(ms(started.elapsed()));
    }
    ops.push(("Single edit + undo record (first)".into(), edit_ms[0]));
    ops.push((
        "Edit + undo record (median of 10)".into(),
        stats(edit_ms).median,
    ));
    let ctx = d.ctx.clone();
    ops.push((
        "Undo 10 steps".into(),
        once(|| {
            for _ in 0..10 {
                d.app.history_move(&ctx, false);
            }
        }),
    ));
    ops.push((
        "Redo 10 steps".into(),
        once(|| {
            for _ in 0..10 {
                d.app.history_move(&ctx, true);
            }
        }),
    ));
    memory.push(("after 10 edits + undo/redo".into(), memory_mb()));

    ops.push(("F9 schedule_now (35k)".into(), once(|| d.app.schedule())));
    assert!(d.app.error.is_none(), "{:?}", d.app.error);

    ops.push((
        "updated_document().text() (35k)".into(),
        once(|| {
            let text = d.app.project.updated_document().text();
            assert!(!text.is_empty());
        }),
    ));
    memory.push(("end".into(), memory_mb()));

    println!();
    for (name, v) in &ops {
        println!("{name:<48} {v:>9.1} ms");
    }
    for (name, (ws, private)) in &memory {
        println!("memory {name:<30} working set {ws:>7.1} MB  private {private:>7.1} MB");
    }

    report.insert(
        "frames".into(),
        serde_json::json!(frame_rows
            .iter()
            .map(|(n, s)| serde_json::json!({
                "name": n,
                "median_ms": s.median,
                "p95_ms": s.p95,
                "max_ms": s.max,
                "frames": s.frames
            }))
            .collect::<Vec<_>>()),
    );
    report.insert(
        "operations".into(),
        serde_json::json!(ops
            .iter()
            .map(|(n, v)| serde_json::json!({"name": n, "ms": v}))
            .collect::<Vec<_>>()),
    );
    report.insert(
        "memory".into(),
        serde_json::json!(memory
            .iter()
            .map(|(n, (w, p))| serde_json::json!({
                "point": n,
                "working_set_mb": w,
                "private_mb": p
            }))
            .collect::<Vec<_>>()),
    );
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts");
    std::fs::create_dir_all(&out).unwrap();
    let name = std::env::var("OPENXER_PERF_35K_OUT").unwrap_or_else(|_| "perf-35k.json".into());
    std::fs::write(
        out.join(name),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
}
