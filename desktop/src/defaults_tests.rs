//! The out-of-the-box values on a fresh workspace, one test per default,
//! and a workspace holding the earlier values explicitly, which must read
//! back exactly as stored.
use crate::details::DetailTab;
use crate::format::{Formats, TimeDisplay};
use crate::gantt::settings::DateInterval;
use crate::grid::columns;
use crate::grid::group::BandTarget;
use crate::grid::layout::{self, Layout};
use crate::print::setup::{Orientation, PageSetup};
use crate::qa_sweep::Driver;
use crate::*;

/// The first-run sample project: it carries a project WBS node, as every
/// file the reference writes does.
fn sample() -> ProjectData {
    openxer_xer::sample_project::sample_project().current
}

/// A fresh workspace on the sample project, with the reference defaults.
fn fresh(name: &str) -> Driver {
    crate::defaults::use_reference();
    Driver::new(name, sample())
}

/// The text the table shows for activity `i` in column `id`.
fn shown(d: &Driver, i: usize, id: &str) -> String {
    let p = &d.app.project;
    let catalog = columns::catalog(p);
    let ix = columns::Index::new((false, 0.));
    let column = columns::find(&catalog, id).unwrap_or_else(|| panic!("column {id}"));
    columns::display(p, Some(i), &columns::value(p, &ix, column, i))
}

/// The first activity with a start date and a nonzero remaining duration.
fn a_task(d: &Driver) -> usize {
    let m = &d.app.project.model;
    (0..m.activities.len())
        .find(|&i| m.activities[i].rem > 0 && m.activities[i].early_start.is_some())
        .expect("a task with dates")
}

#[test]
fn dates_show_no_time() {
    let d = fresh("defaults-dates");
    assert_eq!(Formats::default().dates.time, TimeDisplay::Hidden);
    assert_eq!(d.app.formats.dates.time, TimeDisplay::Hidden);
    let i = a_task(&d);
    for id in ["start", "finish"] {
        let text = shown(&d, i, id);
        assert!(!text.is_empty(), "{id} has a date");
        assert!(!text.contains(':'), "{id} shows no time: {text:?}");
    }
    assert_eq!(d.app.formats.date_example(), "30-Oct-26");
}

#[test]
fn durations_and_units_show_no_unit_label() {
    let d = fresh("defaults-durations");
    let f = &d.app.formats;
    assert!(!f.durations.show_label && !f.units.show_label);
    let i = a_task(&d);
    for id in ["original_duration", "remaining_duration", "total_float"] {
        let text = shown(&d, i, id);
        assert!(
            !text.is_empty() && text.chars().all(|c| c.is_ascii_digit() || c == '-'),
            "{id} is a bare number: {text:?}"
        );
    }
    assert_eq!(f.units(8.), "8");
}

#[test]
fn activities_columns_are_the_reference_set() {
    let d = fresh("defaults-columns");
    let ids: Vec<&str> = d
        .app
        .grid
        .layout
        .columns
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(
        ids,
        [
            "activity_id",
            "activity_name",
            "original_duration",
            "remaining_duration",
            "schedule_percent_complete",
            "start",
            "finish",
            "total_float"
        ]
    );
    assert_eq!(d.app.grid.layout.columns, layout::default_columns());
}

#[test]
fn activities_group_by_wbs_under_a_project_band() {
    let d = fresh("defaults-grouping");
    let levels = &d.app.grid.layout.group_sort.levels;
    assert_eq!(levels.len(), 1);
    assert_eq!(levels[0].column, "wbs");
    let _ = d.app.rows();
    let bands = d.app.grid.bands.borrow();
    let project = views::wbs::project_node(&d.app.project).expect("a project node");
    assert_eq!(bands[0].level, 0, "the project is the top band");
    assert_eq!(bands[0].target, BandTarget::Wbs(project));
    assert!(
        bands[0].label.contains(&d.app.project.model.project.name),
        "the top band names the project: {:?}",
        bands[0].label
    );
    assert!(bands.len() > 1);
    assert!(
        bands[1..].iter().all(|b| b.level >= 1),
        "every WBS band sits inside the project band"
    );
}

#[test]
fn activity_icons_are_shown() {
    let d = fresh("defaults-icons");
    assert!(d.app.grid.layout.table.show_icons);
}

#[test]
fn details_tabs_follow_the_reference() {
    let d = fresh("defaults-details");
    let captions: Vec<&str> = d.app.detail.tabs.iter().map(|t| t.caption()).collect();
    assert_eq!(
        captions,
        [
            "General",
            "Status",
            "Resources",
            "Predecessors",
            "Successors"
        ]
    );
    let rest: Vec<&str> = DetailTab::ordered()[5..]
        .iter()
        .map(|t| t.caption())
        .collect();
    assert_eq!(
        rest,
        [
            "Codes",
            "Expenses",
            "Notes to Resources",
            "Notebook",
            "Relationships",
            "Risks",
            "Steps",
            "Summary",
            "User Defined Fields",
            "Documents"
        ]
    );
    // Every tab is listed once.
    let mut all = DetailTab::REFERENCE.to_vec();
    all.sort_by_key(|t| t.key());
    all.dedup();
    assert_eq!(all.len(), DetailTab::ALL.len());
}

#[test]
fn timescale_is_quarter_over_month() {
    let d = fresh("defaults-timescale");
    assert_eq!(d.app.gantt.timescale.interval, DateInterval::QuarterMonth);
    assert!(!d.app.view.automatic_units);
    assert_eq!(d.app.view.upper, timescale::Unit::Quarter);
    assert_eq!(d.app.view.lower, timescale::Unit::Month);
}

#[test]
fn smaller_defaults() {
    let mut d = fresh("defaults-smaller");
    assert!(!d.app.grid.layout.group_sort.hide_if_empty, "Hide if empty");
    assert!(
        d.app.gantt.bars.iter().all(|b| !b.show_when_collapsed),
        "Show bar when collapsed"
    );
    assert_eq!(PageSetup::default().orientation, Orientation::Landscape);
    assert_eq!(d.app.page_setup().orientation, Orientation::Landscape);
    assert_eq!(openxer_xer::edit::wbs::separator(&d.app.project), ".");
    d.app.open_apply_actuals();
    let draft = d.app.sched.progress.actuals.clone().expect("Apply Actuals");
    let m = &d.app.project.model;
    assert_eq!(
        draft.new_date,
        format::date(m.origin_serial, m.project.data_date),
        "Apply Actuals proposes the current data date"
    );
    d.app.sched.progress.actuals = None;
}

/// A workspace saved with the earlier defaults holds every one of them as
/// an explicit value. Opened with the reference defaults, it shows exactly
/// what it stored.
#[test]
fn stored_values_survive_the_new_defaults() {
    // The earlier build: legacy defaults (the test thread's start), every
    // one of them saved.
    let mut d = Driver::new("defaults-stored", sample());
    let page = d.app.page_setup();
    assert_eq!(page.orientation, Orientation::Automatic);
    d.app.save_formats();
    d.app.save_detail_layout();
    d.app.save_gantt();
    d.app.save_page_setup(&page).unwrap();
    eframe::App::on_exit(&mut d.app, None);
    let formats = d.app.formats.clone();
    assert_eq!(formats.dates.time, TimeDisplay::Hour24);
    assert!(formats.durations.show_label);
    let layout = d.app.grid.layout.clone();
    assert!(layout.group_sort.hide_if_empty && !layout.table.show_icons);
    assert_eq!(layout.columns, layout::earlier_columns());
    let tabs = d.app.detail.tabs.clone();
    assert_eq!(tabs.len(), 12);
    let gantt = d.app.gantt.clone();
    assert_eq!(gantt.timescale.interval, DateInterval::Automatic);
    assert!(gantt.bars.iter().all(|b| b.show_when_collapsed));
    let view = d.app.view.clone();
    assert!(view.automatic_units);
    let stored_formats = d.app.store.preference(format::PREFERENCE_KEY).unwrap();
    // A saved layout from before the table options and Hide if empty were
    // kept: those keys are missing and read as they did then.
    let mut old = layout.to_json();
    old["name"] = "Old layout".into();
    old.as_object_mut().unwrap().remove("table");
    old["group_sort"]
        .as_object_mut()
        .unwrap()
        .remove("hide_if_empty");
    let old_layout = Layout::from_json(&old).unwrap();
    assert!(old_layout.group_sort.hide_if_empty && !old_layout.table.show_icons);

    // The new build opens the same workspace.
    crate::defaults::use_reference();
    assert_eq!(Formats::default().dates.time, TimeDisplay::Hidden);
    let store = Store::open(d.app.store.path.clone()).unwrap();
    let ctx = egui::Context::default();
    let app = App::new(&ctx, store, sample());
    assert_eq!(app.formats, formats, "user preferences");
    assert_eq!(app.grid.layout, layout, "the working layout");
    assert_eq!(app.detail.tabs, tabs, "Activity Details tabs");
    assert!(app.gantt == gantt, "Gantt settings");
    assert_eq!(app.gantt.timescale.interval, DateInterval::Automatic);
    assert!(
        app.view.automatic_units && app.view.upper == view.upper && app.view.lower == view.lower,
        "the timescale bands"
    );
    assert_eq!(app.page_setup(), page, "Page Setup");
    assert_eq!(
        app.store.preference(format::PREFERENCE_KEY).unwrap(),
        stored_formats,
        "the stored preferences are not rewritten"
    );
    // Read again under the new defaults, the old layout record is unchanged.
    assert_eq!(Layout::from_json(&old).unwrap(), old_layout);
}

/// A workspace with a stored view but no stored Gantt settings (an older
/// build) keeps the bands its view implied, not the new default interval.
#[test]
fn a_stored_view_keeps_its_bands() {
    let mut d = Driver::new("defaults-view-only", sample());
    eframe::App::on_exit(&mut d.app, None);
    d.app
        .store
        .connection()
        .execute(
            "DELETE FROM preferences WHERE key=?1",
            ["gantt_settings_v1"],
        )
        .unwrap();
    assert!(d
        .app
        .store
        .preference("activities_view_v1")
        .unwrap()
        .is_some());
    crate::defaults::use_reference();
    let store = Store::open(d.app.store.path.clone()).unwrap();
    let app = App::new(&egui::Context::default(), store, sample());
    assert_eq!(app.gantt.timescale.interval, DateInterval::Automatic);
    assert!(app.view.automatic_units);
}

/// The Activities window with the new defaults, for comparison with the
/// reference's first launch, written to the ignored `artifacts/` folder.
#[test]
fn render_activities_with_the_reference_defaults() {
    use crate::headless_render::SoftwarePainter;
    let output = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/review/defaults");
    std::fs::create_dir_all(&output).unwrap();
    // A fresh workspace, built on the painter's own context so the painter
    // sees every texture from the first frame.
    crate::defaults::use_reference();
    let database = std::env::temp_dir().join(format!(
        "openxer-defaults-render-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let project = sample();
    let mut store = Store::open(database.clone()).unwrap();
    store.save(&project).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    let app = &mut app;
    let mut painter = SoftwarePainter::default();
    // The reference capture's size.
    painter.screen = Some(Vec2::new(1552., 928.));
    let mut time = 0.;
    for _ in 0..4 {
        let _ = painter.frame(app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter.frame(app, &ctx, &mut time, vec![], true).unwrap();
    image
        .save(output.join("openxer-activities-defaults.png"))
        .unwrap();
    assert!(app.error.is_none());
    let _ = std::fs::remove_file(database);
}
