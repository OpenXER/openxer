//! The starter filters, layouts and report: what a fresh workspace holds,
//! that existing items are kept, that the lookahead follows the data date,
//! and that the lookahead report runs to HTML, PDF and Excel.
use super::filters::{FilterSelection, UserFilter};
use super::starters::*;
use super::*;
use crate::qa_sweep::Driver;
use crate::reports::doc::Output;
use crate::store::Store;

fn scheduled() -> ProjectData {
    let mut p = openxer_xer::starter();
    crate::scheduling::schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn names(filters: &[UserFilter]) -> Vec<String> {
    filters.iter().map(|f| f.name.clone()).collect()
}

fn select(d: &mut Driver, name: &str) {
    d.app.grid.layout.filter = FilterSelection {
        all_activities: false,
        selected: vec![format!("user:{name}")],
        match_all: true,
        highlight: false,
    };
    d.app.drop_frame_cache();
    d.frames(2);
}

fn shown(d: &Driver) -> Vec<String> {
    d.visible_activities()
        .into_iter()
        .map(|i| d.app.project.name(i).to_string())
        .collect()
}

#[test]
fn a_fresh_workspace_has_the_starter_filters_and_layouts_labeled_as_samples() {
    let d = Driver::new("starters-fresh", scheduled());
    let got = names(&d.app.grid.user_filters);
    for n in [THREE_WEEK_LOOKAHEAD, NEAR_CRITICAL, STARTED_LAST_PERIOD] {
        assert!(got.iter().any(|g| g == n), "{n} in {got:?}");
        assert!(n.ends_with("(OpenXER sample)"));
    }
    let layouts: Vec<&str> = d.app.grid.library.iter().map(|l| l.name.as_str()).collect();
    assert!(layouts.contains(&LOOKAHEAD_LAYOUT) && layouts.contains(&WBS_SUMMARY_LAYOUT));
    // Saved, so a restart reads them back.
    let stored = d.app.store.preference(FILTERS_KEY).unwrap().unwrap();
    assert_eq!(layout::filters_from_json(&stored).unwrap().len(), 3);
    let lookahead = d
        .app
        .grid
        .library
        .iter()
        .find(|l| l.name == LOOKAHEAD_LAYOUT)
        .unwrap();
    assert_eq!(lookahead.group_sort.levels[0].column, "wbs");
    assert_eq!(
        lookahead.filter.selected,
        vec![format!("user:{THREE_WEEK_LOOKAHEAD}")]
    );
    let summary = d
        .app
        .grid
        .library
        .iter()
        .find(|l| l.name == WBS_SUMMARY_LAYOUT)
        .unwrap();
    assert!(summary.group_sort.show_summaries_only);
    // Their colors come from the theme, not from the layout code.
    assert_eq!(summary.group_sort.bands, layout::default_bands());
}

#[test]
fn the_lookahead_layout_carries_its_page_setup_timescale() {
    let mut d = Driver::new("starters-page", scheduled());
    d.app.open_layout(LOOKAHEAD_LAYOUT, true).unwrap();
    let page = d.app.page_setup();
    assert!(!page.options.same_as_screen);
    assert_eq!(page.options.timescale_start, "DD-1W");
    assert_eq!(page.options.timescale_finish, "DD+3W");
    assert!(d.app.grid.layout.filter.active());
    d.app.open_layout(WBS_SUMMARY_LAYOUT, true).unwrap();
    d.frames(2);
    assert!(
        d.visible_activities().is_empty(),
        "summaries only lists no activity rows"
    );
}

#[test]
fn the_lookahead_filter_follows_the_data_date() {
    let mut d = Driver::new("starters-lookahead", scheduled());
    select(&mut d, THREE_WEEK_LOOKAHEAD);
    let near = shown(&d);
    assert!(!near.is_empty());
    let all = d.app.project.model.activities.len();
    assert!(near.len() < all, "the end of the project is past 3 weeks");
    // Moving the data date forward moves the window with it.
    let dd = d.app.project.model.project.data_date.unwrap();
    d.app.project.model.project.data_date = Some(dd + 70 * DAY);
    d.app.drop_frame_cache();
    d.frames(2);
    let later = shown(&d);
    assert_ne!(near, later);
    assert!(!later.contains(&"Notice to proceed".to_string()));
}

#[test]
fn near_critical_lists_open_activities_under_ten_days_of_float() {
    let mut d = Driver::new("starters-near", scheduled());
    select(&mut d, NEAR_CRITICAL);
    let rows = d.visible_activities();
    assert!(!rows.is_empty());
    for i in rows {
        let a = &d.app.project.model.activities[i];
        assert!(
            a.total_float.is_some_and(|f| f < 10 * 480),
            "{}",
            a.task_code
        );
    }
}

#[test]
fn started_last_period_lists_activities_started_in_the_week_before_the_data_date() {
    let mut d = Driver::new("starters-started", scheduled());
    let dd = d.app.project.model.project.data_date.unwrap();
    d.app.project.model.activities[1].act_start = Some(dd - 2 * DAY);
    d.app.project.model.activities[2].act_start = Some(dd - 20 * DAY);
    d.app.drop_frame_cache();
    select(&mut d, STARTED_LAST_PERIOD);
    let got = shown(&d);
    assert_eq!(got, vec![d.app.project.name(1).to_string()]);
}

/// The same workspace file opened by a new application, as after a restart.
fn restart(d: &Driver) -> App {
    let store = Store::open(d.dir.join("workspace.db")).unwrap();
    let ctx = egui::Context::default();
    App::new(&ctx, store, scheduled())
}

#[test]
fn existing_workspaces_gain_the_samples_once_and_keep_their_own_items() {
    let mut d = Driver::new("starters-existing", scheduled());
    // The workspace before this feature: its own filter and layout, no
    // record of any sample.
    d.app.grid.user_filters = vec![UserFilter {
        name: "near critical (openxer sample)".into(),
        match_all: true,
        rows: vec![],
    }];
    d.app.grid.library = vec![layout::Layout {
        name: "My layout".into(),
        ..layout::Layout::default()
    }];
    d.app.save_user_filters();
    d.app.save_library();
    d.app.store.set_preference(LOADED_KEY, "[]").unwrap();
    let mut app = restart(&d);
    let got = names(&app.grid.user_filters);
    assert_eq!(got.len(), 3, "{got:?}");
    assert!(got.contains(&"near critical (openxer sample)".to_string()));
    assert!(!got.contains(&NEAR_CRITICAL.to_string()), "never replaces");
    assert!(app.grid.library.iter().any(|l| l.name == "My layout"));
    assert!(app.grid.library.iter().any(|l| l.name == LOOKAHEAD_LAYOUT));
    // A sample the user deletes stays deleted after another start.
    app.grid
        .user_filters
        .retain(|f| f.name != THREE_WEEK_LOOKAHEAD);
    app.grid.library.retain(|l| l.name != WBS_SUMMARY_LAYOUT);
    app.save_user_filters();
    app.save_library();
    drop(app);
    let again = restart(&d);
    assert!(!names(&again.grid.user_filters).contains(&THREE_WEEK_LOOKAHEAD.to_string()));
    assert!(!again
        .grid
        .library
        .iter()
        .any(|l| l.name == WBS_SUMMARY_LAYOUT));
    assert!(again
        .grid
        .library
        .iter()
        .any(|l| l.name == LOOKAHEAD_LAYOUT));
}

#[test]
fn view_menus_list_the_saved_column_sets_and_groupings_after_customize() {
    let mut d = Driver::new("starters-menus", scheduled());
    let columns = d.app.column_presets();
    let groups = d.app.group_presets();
    // The standard layout and the two samples differ in columns; the
    // lookahead and summary differ in grouping.
    let col_names: Vec<&str> = columns.iter().map(|(n, _)| n.as_str()).collect();
    assert!(col_names.contains(&"Standard Layout"));
    assert!(col_names.contains(&LOOKAHEAD_LAYOUT));
    assert!(col_names.contains(&WBS_SUMMARY_LAYOUT));
    assert!(groups.iter().any(|(n, _)| n == WBS_SUMMARY_LAYOUT));
    d.click("menu_bar_view");
    assert!(d.hover_text("Columns"));
    assert!(d.has("view_columns"));
    let customize = d.rect("view_columns");
    let first = d.rect("menu_columns_set_0");
    assert!(customize.top() < first.top(), "Customize stays first");
    // Choosing a set changes only the columns.
    let i = columns
        .iter()
        .position(|(n, _)| n == WBS_SUMMARY_LAYOUT)
        .unwrap();
    let group_before = d.app.grid.layout.group_sort.clone();
    d.click(&format!("menu_columns_set_{i}"));
    assert_eq!(d.app.grid.layout.columns, columns[i].1);
    assert_eq!(d.app.grid.layout.group_sort, group_before);
    // The same for Group and Sort by.
    let j = groups
        .iter()
        .position(|(n, _)| n == WBS_SUMMARY_LAYOUT)
        .unwrap();
    d.click("menu_bar_view");
    assert!(d.hover_text("Group and Sort by"));
    assert!(d.rect("view_group_sort").top() < d.rect("menu_group_set_0").top());
    d.click(&format!("menu_group_set_{j}"));
    assert!(d.app.grid.layout.group_sort.show_summaries_only);
    assert_eq!(d.app.grid.layout.columns, columns[i].1);
}

#[test]
fn the_lookahead_report_runs_to_html_pdf_and_excel() {
    let d = Driver::new("starters-report", scheduled());
    let stored = d.app.stored_reports();
    let find = |name: &str| stored.iter().find(|s| s.name == name).map(|s| s.id);
    let id = find("3-Week Lookahead by WBS").expect("the lookahead report");
    assert!(find("Critical and Near Critical").is_some());
    for name in ["Milestones", "Progress This Period", "WBS Summary"] {
        assert!(find(name).is_some(), "{name}");
    }
    let html = d.dir.join("lookahead.html");
    let pdf = d.dir.join("lookahead.pdf");
    let xlsx = d.dir.join("lookahead.xlsx");
    for (format, path) in [
        (Output::Html, &html),
        (Output::Pdf, &pdf),
        (Output::Excel, &xlsx),
    ] {
        d.app.write_stored_as(id, format, path, "").unwrap();
        assert!(path.metadata().unwrap().len() > 500, "{path:?}");
    }
    assert!(std::fs::read_to_string(&html)
        .unwrap()
        .contains("3-Week Lookahead"));
    assert!(crate::print::pdf::check(&std::fs::read(&pdf).unwrap()).unwrap() >= 1);
    assert_eq!(&std::fs::read(&xlsx).unwrap()[..2], b"PK");
    // The sheet holds the activities in the window, one row each.
    let g = d.app.generate_stored(id, "").unwrap();
    let sheet = crate::reports::rpt::writers::excel_sheet(&g);
    let text: Vec<String> = sheet
        .rows
        .iter()
        .flat_map(|r| r.cells.iter())
        .filter_map(|c| match c {
            openxer_interchange::report_xlsx::ReportCell::Text(s) => Some(s.clone()),
            _ => None,
        })
        .collect();
    assert!(text.iter().any(|t| t == "Activity ID"));
    assert!(text.iter().any(|t| t.starts_with("RB-")), "{text:?}");
    // Dates and durations are typed cells, so they sort and sum; each
    // activity row has its start and finish as dates and its remaining
    // duration and float as numbers.
    use openxer_interchange::report_xlsx::ReportCell;
    let activity_rows: Vec<_> = sheet
        .rows
        .iter()
        .filter(|r| {
            r.cells
                .iter()
                .any(|c| matches!(c, ReportCell::Text(s) if s.starts_with("RB-")))
        })
        .collect();
    assert!(!activity_rows.is_empty());
    for row in activity_rows {
        let dates = row
            .cells
            .iter()
            .filter(|c| matches!(c, ReportCell::Date(_)))
            .count();
        let numbers = row
            .cells
            .iter()
            .filter(|c| matches!(c, ReportCell::Number(_)))
            .count();
        assert_eq!((dates, numbers), (2, 2), "{:?}", row.cells);
    }
}

/// Every caption and fixed text of the starter reports fits its cell, so
/// none prints cut short with "..." (Baseline Variance's "Finish Variance"
/// did).
#[test]
fn starter_report_captions_fit_their_cells() {
    let d = Driver::new("starters-captions", scheduled());
    let mut cut = vec![];
    for s in d.app.stored_reports() {
        let g = d.app.generate_stored(s.id, "").unwrap();
        for line in g.all_lines() {
            for c in &line.cells {
                let t = c.text.trim();
                if c.data
                    || c.kind != crate::reports::rpt::engine::Kind::Text
                    || t.is_empty()
                    || t.starts_with('[')
                {
                    continue;
                }
                let width = crate::reports::pdf::text_width(t, c.font.px, c.font.bold);
                if width > c.w - 4. {
                    cut.push(format!(
                        "{}: {t:?} needs {width:.0} of {:.0}",
                        s.name,
                        c.w - 4.
                    ));
                }
            }
        }
    }
    assert!(cut.is_empty(), "{cut:#?}");
}

#[test]
fn the_near_critical_report_lists_critical_or_low_float_activities() {
    let d = Driver::new("starters-near-report", scheduled());
    let id = d
        .app
        .stored_reports()
        .iter()
        .find(|s| s.name == "Critical and Near Critical")
        .unwrap()
        .id;
    let g = d.app.generate_stored(id, "").unwrap();
    let lines = crate::reports::rpt::writers::excel_sheet(&g).rows.len();
    assert!(lines > 4, "title block and rows");
    assert!(g.warnings.is_empty(), "{:?}", g.warnings);
}
