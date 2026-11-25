//! Layout display options through the production UI: menu, toolbar and
//! context-menu clicks with synthetic input (no window, no system pointer),
//! plus renders of Line Numbers, Progress Spotlight and Table Font and Row
//! to the ignored `artifacts/g2a/` folder.
use super::checks::CheckFilter;
use super::*;
use crate::grid::layout::TableDisplay;
use crate::headless_render::SoftwarePainter;
use crate::qa_sweep::Driver;

fn scheduled() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn driver(name: &str) -> Driver {
    let mut d = Driver::new(&format!("g2a-{name}"), scheduled());
    d.home();
    d
}

/// Open a main menu (and a submenu) and click one of its items.
fn menu(d: &mut Driver, bar: &str, sub: Option<&str>, item: &str) {
    d.click(&format!("menu_bar_{bar}"));
    if let Some(sub) = sub {
        assert!(d.hover_text(sub), "submenu {sub} is drawn");
    }
    d.click(item);
    d.frames(2);
}

/// Open the `Layout:` options popup and click one of its items.
fn layout_menu(d: &mut Driver, item: &str) {
    d.click("layout_button");
    d.click(item);
    d.frames(2);
}

fn codes(d: &Driver) -> Vec<String> {
    d.visible_activities()
        .into_iter()
        .map(|i| d.app.project.model.activities[i].task_code.clone())
        .collect()
}

fn alt() -> egui::Modifiers {
    egui::Modifiers {
        alt: true,
        ..Default::default()
    }
}

fn shift() -> egui::Modifiers {
    egui::Modifiers {
        shift: true,
        ..Default::default()
    }
}

// ------------------------------------------------------------ Line Numbers

#[test]
fn line_numbers_toggle_from_every_entry_and_shift_the_table() {
    let mut d = driver("line-numbers");
    let before = d.rect("grid_header_activity_id");
    menu(&mut d, "view", None, "menu_line_numbers");
    assert!(d.app.grid.layout.table.line_numbers);
    d.frames(2);
    let after = d.rect("grid_header_activity_id");
    let gutter = d.app.line_number_width(d.app.rows().len());
    assert!(gutter >= 28.);
    assert_eq!(
        after.left(),
        before.left() + gutter,
        "the gutter comes first"
    );
    // Every visible row, bands included, gets its number.
    let rows = d.app.rows().len();
    let texts: Vec<String> = d.texts().into_iter().map(|(t, _)| t).collect();
    for n in 1..=rows.min(10) {
        assert!(texts.contains(&n.to_string()), "row number {n} drawn");
    }
    // The Layout toolbar button turns it off, the table menu on again.
    d.click("tool_line_numbers");
    assert!(!d.app.grid.layout.table.line_numbers);
    let i = d.visible_activities()[0];
    let pos = d.cell(i, "activity_name").unwrap();
    d.press_at(pos, egui::PointerButton::Secondary, Default::default());
    d.click("ctx_line_numbers");
    assert!(d.app.grid.layout.table.line_numbers);
    assert_eq!(d.app.editing.last, Some(editing::Command::LineNumbers));
}

#[test]
fn table_options_are_saved_with_the_layout_and_survive_reopening() {
    let mut d = driver("persist");
    d.app.toggle_line_numbers();
    d.app.toggle_hint_help();
    d.app.grid.layout.table.font.points = 10.;
    d.app.grid.layout.table.font.italic = true;
    d.app.grid.layout.table.cell_color = [255, 255, 224];
    d.app.grid.layout.table.optimize_rows = Some(3);
    d.app.gantt.options.spotlight = true;
    d.app.gantt.options.background = gantt::settings::Rgb(250, 250, 240);
    d.app.save_layout_as("Options").unwrap();
    // Change everything, then open the saved layout again.
    d.app.grid.layout.table = TableDisplay::default();
    d.app.gantt.options.spotlight = false;
    d.app.gantt.options.background = gantt::settings::Rgb::WHITE;
    d.app.open_layout("Options", true).unwrap();
    let t = &d.app.grid.layout.table;
    assert!(t.line_numbers && t.hint_help && t.font.italic);
    assert_eq!(t.font.points, 10.);
    assert_eq!(t.cell_color, [255, 255, 224]);
    assert_eq!(t.optimize_rows, Some(3));
    assert!(d.app.gantt.options.spotlight);
    assert_eq!(
        d.app.gantt.options.background,
        gantt::settings::Rgb(250, 250, 240)
    );
    // The working layout is kept in the workspace: a new session has it.
    d.frames(8);
    d.app.save_layout();
    let path = d.app.store.path.clone();
    let mut again = App::new(
        &egui::Context::default(),
        Store::open(path).unwrap(),
        scheduled(),
    );
    again.restore_layout();
    assert_eq!(again.grid.layout.table, d.app.grid.layout.table);
}

#[test]
fn table_display_json_round_trips_and_rejects_bad_values() {
    let mut l = layout::Layout::default();
    l.table.line_numbers = true;
    l.table.font.face = chrome::Face::Tahoma;
    l.table.font.bold = true;
    l.table.optimize_rows = Some(0);
    l.table.show_icons = true;
    let back = layout::Layout::from_json(&l.to_json()).unwrap();
    assert_eq!(back, l);
    // Older layouts without the table part load with the defaults.
    let mut old = l.to_json();
    old.as_object_mut().unwrap().remove("table");
    assert_eq!(
        layout::Layout::from_json(&old).unwrap().table,
        TableDisplay::default()
    );
    let mut bad = l.to_json();
    bad["table"]["font"]["face"] = "Comic".into();
    assert!(layout::Layout::from_json(&bad).is_err());
    let mut bad = l.to_json();
    bad["table"]["font"]["points"] = 99.into();
    assert!(layout::Layout::from_json(&bad).is_err());
}

// ------------------------------------------------------- Progress Spotlight

#[test]
fn spotlight_highlights_the_next_period_and_follows_the_data_date_drag() {
    let mut d = driver("spotlight");
    d.app.view.automatic_units = false;
    d.app.view.lower = timescale::Unit::Week;
    d.click("tool_spotlight");
    assert!(d.app.gantt.options.spotlight);
    let dd = d.app.project.model.project.data_date.unwrap();
    assert_eq!(d.app.spotlight_window(), Some((dd, dd + 7 * DAY)));
    let leveled = d.app.project.model.options.do_level;
    let expected: BTreeSet<usize> = d
        .app
        .project
        .model
        .activities
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            !a.is_completed() && displayed_start(a, leveled).is_some_and(|s| s < dd + 7 * DAY)
        })
        .map(|(i, _)| i)
        .collect();
    assert!(!expected.is_empty());
    d.frames(2);
    assert_eq!(*d.app.grid.highlighted.borrow(), expected);
    // Drag the spotlight's edge far right: more activities come into it.
    let handle = d.rect("spotlight_end").center();
    let to = handle + Vec2::new(300., 0.);
    let _ = d.frame(vec![egui::Event::PointerMoved(handle)]);
    let press = |pressed, pos| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let _ = d.frame(vec![press(true, handle)]);
    for k in 1..=6 {
        let _ = d.frame(vec![egui::Event::PointerMoved(
            handle + Vec2::new(50. * k as f32, 0.),
        )]);
    }
    let _ = d.frame(vec![press(false, to)]);
    d.frames(2);
    let (_, end) = d.app.spotlight_window().unwrap();
    assert!(end > dd + 7 * DAY, "the drag moved the spotlight's end");
    assert_eq!(end % DAY, 0, "snapped to a day");
    assert!(d.app.grid.highlighted.borrow().len() > expected.len());
    // Update Progress proposes the spotlight's end.
    d.app.open_update_progress();
    let proposed = d
        .app
        .sched
        .progress
        .update
        .as_ref()
        .unwrap()
        .new_date
        .clone();
    assert_eq!(
        proposed,
        schedule_dialogs::display_date(d.app.project.model.origin_serial, Some(end))
    );
    d.app.sched.progress.update = None;
    // The View menu turns it off again.
    menu(&mut d, "view", None, "menu_spotlight");
    assert!(!d.app.gantt.options.spotlight);
    d.frames(2);
    assert!(d.app.grid.highlighted.borrow().is_empty());
}

#[test]
fn spotlight_default_period_follows_the_timescale() {
    let mut d = driver("spotlight-units");
    d.app.gantt.options.spotlight = true;
    let dd = d.app.project.model.project.data_date.unwrap();
    for (unit, length) in [
        (timescale::Unit::Hour, 60),
        (timescale::Unit::Day, DAY),
        (timescale::Unit::Week, 7 * DAY),
    ] {
        d.app.view.lower = unit;
        assert_eq!(d.app.spotlight_window(), Some((dd, dd + length)));
    }
    d.app.view.lower = timescale::Unit::Month;
    let (_, end) = d.app.spotlight_window().unwrap();
    assert!((28 * DAY..=31 * DAY).contains(&(end - dd)));
}

// --------------------------------------------------------------- Collapse To

#[test]
fn collapse_to_lists_levels_and_collapses_the_chosen_level() {
    let mut d = driver("collapse-to");
    let all = d.app.rows().len();
    menu(&mut d, "view", None, "menu_collapse_to");
    let levels = d
        .app
        .layout_opts
        .drafts
        .collapse
        .as_ref()
        .unwrap()
        .levels
        .clone();
    assert!(!levels.is_empty());
    assert!(levels[0].starts_with("WBS Level 1"), "{levels:?}");
    // Escape closes without a change.
    d.key(Key::Escape);
    assert!(d.app.layout_opts.drafts.collapse.is_none());
    assert!(d.app.collapsed.is_empty());
    // Cancel leaves the bands alone too.
    layout_menu(&mut d, "layout_collapse_to");
    d.click("collapse_to_cancel");
    assert!(d.app.collapsed.is_empty());
    // Level 1: only the top level bands remain.
    layout_menu(&mut d, "layout_collapse_to");
    d.click("collapse_to_level");
    d.click("collapse_to_level_item_0");
    d.click("collapse_to_ok");
    assert!(d.app.layout_opts.drafts.collapse.is_none());
    let rows = d.app.rows();
    assert!(rows.len() < all);
    let bands = d.app.grid.bands.borrow().clone();
    assert!(rows
        .iter()
        .all(|r| matches!(r, Row::Group(b) if bands[*b].level == 0)));
}

// ------------------------------------------------------ Table Font and Row

#[test]
fn table_font_dialog_sets_font_rows_and_cancels_cleanly() {
    let mut d = driver("table-font");
    let height = d.app.activity_row_height();
    menu(&mut d, "view", None, "menu_table_font");
    assert!(d.app.layout_opts.drafts.table.is_some());
    // Cancel drops the draft.
    d.click("table_font_cancel");
    assert!(d.app.layout_opts.drafts.table.is_none());
    assert_eq!(d.app.grid.layout.table, TableDisplay::default());
    // The font sample opens the picker: Tahoma, Bold Italic, 12 pt.
    menu(&mut d, "view", None, "menu_table_font");
    d.click("table_font_font");
    assert!(d.app.layout_opts.drafts.picker.is_some());
    d.click("font_picker_face_row_2");
    d.click("font_picker_style_row_3");
    d.click("font_picker_size_row_6");
    d.click("font_picker_ok");
    assert!(d.app.layout_opts.drafts.picker.is_none());
    d.click("table_font_icons");
    d.click("table_font_ok");
    let t = d.app.grid.layout.table.clone();
    assert_eq!(t.font.face, chrome::Face::Tahoma);
    assert!(t.font.bold && t.font.italic);
    assert_eq!(t.font.points, 12.);
    assert!(t.show_icons);
    // Bigger text makes taller rows, in the table and the Gantt alike.
    assert!(d.app.activity_row_height() >= 24.);
    d.frames(2);
    let bars = &d.app.bar_geometry;
    assert!(bars.iter().all(|b| b.row.height() >= 24.));
    // Select height for all rows sets a fixed height.
    layout_menu(&mut d, "layout_table_font");
    d.click("table_font_keep");
    d.click("table_font_fixed");
    d.app.layout_opts.drafts.table.as_mut().unwrap().height = "30".into();
    d.click("table_font_ok");
    assert_eq!(d.app.view.row_height, 30.);
    assert!(d.app.activity_row_height() >= 30.);
    // Default restores Arial 8 and 18 px rows.
    layout_menu(&mut d, "layout_table_font");
    d.click("table_font_default");
    d.click("table_font_ok");
    assert!(d.app.grid.layout.table.font.is_default());
    assert_eq!(d.app.activity_row_height(), height);
    // Escape closes without applying.
    layout_menu(&mut d, "layout_table_font");
    d.click("table_font_icons");
    d.key(Key::Escape);
    assert!(d.app.layout_opts.drafts.table.is_none());
    assert!(!d.app.grid.layout.table.show_icons);
}

#[test]
fn optimized_rows_grow_to_fit_wrapped_names() {
    let mut d = driver("optimize");
    let i = d.visible_activities()[0];
    let long = "A long activity name that cannot fit in a narrow column at all";
    assert!(d.app.apply_cell(i, "activity_name", long));
    d.app.set_column_width("activity_name", 80.);
    let base = d.app.activity_row_height();
    d.app.open_table_font();
    d.frames(2);
    d.click("table_font_keep");
    d.click("table_font_optimize");
    d.click("table_font_ok");
    assert_eq!(d.app.grid.layout.table.optimize_rows, Some(0));
    d.frames(2);
    assert!(d.app.activity_height(i) > base * 2., "the long name wraps");
    let other = d.visible_activities()[1];
    assert!(d.app.activity_height(other) < d.app.activity_height(i));
    // Do not exceed 2 lines.
    d.app.open_table_font();
    d.frames(2);
    d.click("table_font_keep");
    d.click("table_font_limit");
    d.app.layout_opts.drafts.table.as_mut().unwrap().lines = "2".into();
    d.click("table_font_ok");
    assert_eq!(d.app.grid.layout.table.optimize_rows, Some(2));
    d.frames(2);
    let two = d.app.activity_height(i);
    assert!(two > base && two < base * 2. + 4., "{two}");
}

// --------------------------------------------------- Chart Font and Colors

#[test]
fn chart_font_and_colors_apply_to_the_gantt_and_cancel_cleanly() {
    let mut d = driver("chart-font");
    let before = d.app.gantt.options;
    menu(&mut d, "view", None, "menu_chart_font");
    let draft = &mut d.app.layout_opts.drafts.chart.as_mut().unwrap().options;
    draft.background = gantt::settings::Rgb(0xF0, 0xF8, 0xFF);
    draft.sight_lines.horizontal_minor.color = gantt::settings::Rgb::BLUE;
    d.click("chart_font_cancel");
    assert_eq!(d.app.gantt.options, before);
    menu(&mut d, "view", None, "menu_chart_font");
    d.click("chart_font_font");
    d.click("font_picker_style_row_1");
    d.click("font_picker_size_row_3");
    d.click("font_picker_ok");
    let draft = &mut d.app.layout_opts.drafts.chart.as_mut().unwrap().options;
    draft.background = gantt::settings::Rgb(0xF0, 0xF8, 0xFF);
    draft.nonwork.color = gantt::settings::Rgb(0xC0, 0xC0, 0xFF);
    draft.sight_lines.vertical_minor.color = gantt::settings::Rgb::RED;
    d.click("chart_font_ok");
    let o = d.app.gantt.options;
    assert_eq!(o.background, gantt::settings::Rgb(0xF0, 0xF8, 0xFF));
    assert_eq!(o.nonwork.color, gantt::settings::Rgb(0xC0, 0xC0, 0xFF));
    assert_eq!(
        o.sight_lines.vertical_minor.color,
        gantt::settings::Rgb::RED
    );
    assert!(o.font.bold);
    assert_eq!(o.font.points, 9.);
    // The settings validate and round trip as the layout stores them.
    let back = gantt::settings::GanttSettings::from_json(&d.app.gantt.to_json()).unwrap();
    assert_eq!(back.options, o);
    // Escape closes the dialog.
    menu(&mut d, "view", None, "menu_chart_font");
    d.key(Key::Escape);
    assert!(d.app.layout_opts.drafts.chart.is_none());
}

// ---------------------------------------------------------------- New Layout

#[test]
fn new_layout_starts_from_the_default_or_the_current_layout() {
    let mut d = driver("new-layout");
    d.app.grid.layout.columns.truncate(3);
    d.app.toggle_line_numbers();
    menu(&mut d, "view", Some("Layout"), "menu_new_layout");
    assert!(d.app.layout_opts.drafts.new_layout.is_some());
    d.click("new_layout_cancel");
    // Only the starter samples are saved.
    assert!(d
        .app
        .grid
        .library
        .iter()
        .all(|l| l.name.ends_with("(OpenXER sample)")));
    // From the default layout.
    menu(&mut d, "view", Some("Layout"), "menu_new_layout");
    d.app.layout_opts.drafts.new_layout.as_mut().unwrap().name = "Fresh".into();
    d.click("new_layout_ok");
    assert_eq!(d.app.grid.layout.name, "Fresh");
    assert_eq!(d.app.grid.layout.columns, layout::default_columns());
    assert!(!d.app.grid.layout.table.line_numbers);
    assert!(d.app.grid.library.iter().any(|l| l.name == "Fresh"));
    // From the current layout.
    d.app.grid.layout.columns.truncate(2);
    d.app.open_new_layout();
    d.frames(2);
    d.click("new_layout_current");
    d.app.layout_opts.drafts.new_layout.as_mut().unwrap().name = "Copy".into();
    d.click("new_layout_ok");
    assert_eq!(d.app.grid.layout.name, "Copy");
    assert_eq!(d.app.grid.layout.columns.len(), 2);
    // A taken name is refused and the dialog stays.
    d.app.open_new_layout();
    d.app.layout_opts.drafts.new_layout.as_mut().unwrap().name = "fresh".into();
    d.frames(2);
    d.click("new_layout_ok");
    assert!(d.app.error.take().is_some());
    assert!(d.app.layout_opts.drafts.new_layout.is_some());
    // The error box leaves the screen first; while it shows, Escape is its.
    d.frames(2);
    d.key(Key::Escape);
    assert!(d.app.layout_opts.drafts.new_layout.is_none());
}

// ---------------------------------------------------------------- Show on Top

#[test]
fn show_on_top_table_and_activity_usage_spreadsheet() {
    let mut d = driver("top");
    menu(&mut d, "view", Some("Show on Top"), "menu_top_table");
    assert_eq!(d.app.panes.top, panes::TopView::Table);
    d.frames(2);
    assert!(d.app.bar_geometry.is_empty(), "no Gantt beside the table");
    assert!(!d.has("splitter"));
    assert!(d.has("grid_header_activity_id"));
    assert_eq!(d.app.panes.json()["top"], "table");
    // The Top Layout toolbar brings the Gantt back.
    d.click("tool_gantt");
    assert_eq!(d.app.panes.top, panes::TopView::Gantt);
    d.frames(2);
    assert!(!d.app.bar_geometry.is_empty());
    d.click("tool_table");
    assert_eq!(d.app.panes.top, panes::TopView::Table);
    menu(
        &mut d,
        "view",
        Some("Show on Top"),
        "menu_top_activity_usage",
    );
    assert_eq!(d.app.panes.top, panes::TopView::ActivityUsageSheet);
    d.frames(2);
    d.assert_clean("activity usage on top");
    let mut panes = panes::PanesState::default();
    panes.restore(&d.app.panes.json());
    assert_eq!(panes.top, panes::TopView::ActivityUsageSheet);
}

// ------------------------------------------------- Disable Auto-Reorganization

#[test]
fn disabled_reorganization_keeps_rows_in_place_until_f5() {
    let mut d = driver("reorganize");
    let start = codes(&d);
    let last = *d.visible_activities().last().unwrap();
    let code = d.app.project.model.activities[last].task_code.clone();
    d.key_with(Key::F12, shift());
    assert!(d.app.layout_opts.reorganize_off);
    // Renaming the last activity's ID to sort first does not move it.
    assert!(d.app.apply_cell(last, "activity_id", "AAA-0001"));
    d.frames(2);
    let kept = codes(&d);
    assert_eq!(kept.last().map(String::as_str), Some("AAA-0001"));
    assert_eq!(kept.len(), start.len());
    // A new activity appears; nothing else moves.
    d.app.new_activity();
    d.frames(2);
    assert_eq!(codes(&d).len(), start.len() + 1);
    // Collapsing a band takes a new order from a regular build; the band
    // rows still name their bands.
    let labels = |d: &Driver| -> Vec<String> {
        d.app
            .rows()
            .into_iter()
            .filter_map(|r| match r {
                Row::Group(b) => Some(d.app.band_label(b)),
                Row::Activity(_) => None,
            })
            .collect()
    };
    let bands = labels(&d);
    let first = d.app.grid.bands.borrow()[0].key.clone();
    d.app.collapsed.insert(first.clone());
    d.frames(2);
    assert_eq!(labels(&d), bands);
    assert!(codes(&d).len() < start.len());
    d.app.collapsed.remove(&first);
    d.frames(2);
    assert_eq!(labels(&d), bands);
    // F5 regroups and resorts.
    d.key(Key::F5);
    let resorted = codes(&d);
    assert_ne!(resorted.last().map(String::as_str), Some("AAA-0001"));
    assert!(resorted.contains(&"AAA-0001".to_string()));
    assert!(!resorted.contains(&code));
    // Turned back on from the Tools menu.
    menu(&mut d, "tools", None, "menu_reorganize");
    assert!(!d.app.layout_opts.reorganize_off);
    assert!(d.app.layout_opts.frozen.borrow().is_none());
}

// ----------------------------------------------------------------- Hint Help

#[test]
fn hint_help_shows_the_whole_value_of_a_truncated_cell() {
    let mut d = driver("hints");
    let i = d.visible_activities()[0];
    let name = d.app.project.name(i).to_string();
    d.app.set_column_width("activity_name", 40.);
    d.frames(2);
    let pos = d.cell(i, "activity_name").unwrap();
    let _ = d.frame(vec![egui::Event::PointerMoved(pos)]);
    d.frames(1);
    assert_eq!(d.app.layout_opts.hint, None, "hints are off");
    d.key_with(Key::F1, alt());
    assert!(d.app.grid.layout.table.hint_help);
    let _ = d.frame(vec![egui::Event::PointerMoved(pos)]);
    d.frames(1);
    assert_eq!(d.app.layout_opts.hint.as_deref(), Some(name.as_str()));
    // A cell whose value fits gives no hint.
    let id = d.cell(i, "activity_id").unwrap();
    let _ = d.frame(vec![egui::Event::PointerMoved(id)]);
    d.frames(1);
    assert_eq!(d.app.layout_opts.hint, None);
    // The Display toolbar turns it off.
    d.click("tool_hints");
    assert!(!d.app.grid.layout.table.hint_help);
}

// ------------------------------------------------------- Check Schedule filters

#[test]
fn check_schedule_filters_show_what_each_check_finds() {
    let mut p = scheduled();
    // Progress the data date problems into the project: one activity with
    // an actual start after the data date.
    let dd = p.model.project.data_date.unwrap();
    let late_actual = p
        .model
        .activities
        .iter()
        .position(|a| a.status == StatusType::NotStarted && a.task_type == TaskType::Task)
        .unwrap();
    p.model.activities[late_actual].act_start = Some(dd + 2 * DAY);
    let mut d = Driver::new("g2a-checks", p);
    d.home();
    d.app.sched.check_params.large_float_hours = 8.;
    let all = codes(&d);
    for filter in CheckFilter::ALL {
        let action = format!("menu_{}", filter.key().replace(':', "_"));
        menu(&mut d, "view", Some("Check Schedule"), &action);
        assert!(d.app.check_filter_on(filter), "{filter:?}");
        let expected = checks::matching(
            &d.app.project,
            &d.app.sched.check_params,
            Some(&d.app.baseline_compare()),
            filter,
        );
        let shown: BTreeSet<usize> = d.visible_activities().into_iter().collect();
        assert_eq!(shown, expected, "{filter:?}");
        assert_eq!(
            d.app.grid.layout.filter.summary(&d.app.grid.user_filters),
            filter.caption()
        );
        // The same item again shows every activity.
        menu(&mut d, "view", Some("Check Schedule"), &action);
        assert!(!d.app.grid.layout.filter.active());
        assert_eq!(codes(&d), all);
    }
    // The checks use the report's rules and thresholds.
    let ix = d.app.grid_index();
    let after = ix.check_set(&d.app.project, CheckFilter::InvalidAfter);
    assert!(after.contains(&late_actual));
    let large = ix.check_set(&d.app.project, CheckFilter::LargeFloat);
    let limit = 8 * 60;
    for (i, a) in d.app.project.model.activities.iter().enumerate() {
        let open = !a.is_completed() && !matches!(a.task_type, TaskType::Loe | TaskType::Wbs);
        assert_eq!(
            large.contains(&i),
            open && a.total_float.is_some_and(|f| f > limit),
            "{}",
            a.task_code
        );
    }
    let missing = ix.check_set(&d.app.project, CheckFilter::MissingInBaseline);
    assert!(missing.is_empty(), "no baseline is assigned");
}

// -------------------------------------------------------------------- renders

fn artifacts() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/g2a");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn render_line_numbers_spotlight_and_table_font() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let db = std::env::temp_dir().join(format!("openxer-g2a-render-{unique}.db"));
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, Store::open(db.clone()).unwrap(), scheduled());
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let out = artifacts();
    let mut shoot = |app: &mut App, name: &str| {
        for _ in 0..2 {
            let _ = painter.frame(app, &ctx, &mut time, vec![], false);
        }
        let (image, _) = painter.frame(app, &ctx, &mut time, vec![], true).unwrap();
        image.save(out.join(name)).unwrap();
        image
    };
    app.toggle_line_numbers();
    shoot(&mut app, "line-numbers.png");
    app.toggle_line_numbers();
    app.view.automatic_units = false;
    app.view.lower = timescale::Unit::Week;
    app.toggle_spotlight();
    let image = shoot(&mut app, "spotlight.png");
    let highlighted = image
        .pixels()
        .filter(|p| {
            let [r, g, b] = crate::theme::bytes(crate::theme::HIGHLIGHT);
            p.0 == [r, g, b, 255]
        })
        .count();
    assert!(highlighted > 100, "spotlighted rows are highlighted");
    app.toggle_spotlight();
    app.grid.layout.table.font = layout::TableFont {
        face: chrome::Face::Tahoma,
        points: 11.,
        bold: true,
        italic: true,
        color: [0, 0, 128],
    };
    app.grid.layout.table.cell_color = [255, 255, 224];
    app.grid.layout.table.show_icons = true;
    shoot(&mut app, "table-font.png");
    app.grid.layout.table = TableDisplay::default();
    app.open_table_font();
    shoot(&mut app, "table-font-dialog.png");
    app.layout_opts.drafts.table = None;
    app.open_chart_font();
    shoot(&mut app, "chart-font-dialog.png");
    app.layout_opts.drafts.chart = None;
    app.open_collapse_to();
    shoot(&mut app, "collapse-to-dialog.png");
    app.layout_opts.drafts.collapse = None;
    app.open_table_font();
    app.layout_opts.drafts.picker = Some(dialogs::FontPicker {
        target: dialogs::PickerFor::Table,
        face: chrome::Face::SegoeUi,
        points: 10.,
        bold: false,
        italic: true,
        color: gantt::settings::Rgb::BLACK,
    });
    shoot(&mut app, "font-picker.png");
    app.layout_opts.drafts.picker = None;
    app.layout_opts.drafts.table = None;
    drop(app);
    let _ = std::fs::remove_file(db);
}

#[test]
fn dialog_lists_draw_no_text_below_their_bottom_edge() {
    // The font picker's size list shows six of its eleven sizes; the shared
    // list also lays out the row cut by its bottom edge, whose text must
    // stay clipped to the list.
    let mut d = driver("list-clip");
    menu(&mut d, "view", None, "menu_table_font");
    d.click("table_font_font");
    assert!(d.app.layout_opts.drafts.picker.is_some());
    let sizes = ["6", "7", "8", "9", "10", "11", "12", "14", "16", "18", "20"];
    let rows: Vec<Rect> = (0..sizes.len())
        .filter_map(|r| {
            let key = format!("font_picker_size_row_{r}");
            d.has(&key).then(|| d.rect(&key))
        })
        .filter(|r| r.is_positive())
        .collect();
    assert!(
        rows.len() >= 2 && rows.len() < sizes.len(),
        "the size list scrolls: {} rows shown",
        rows.len()
    );
    let bottom = rows.iter().map(|r| r.bottom()).fold(f32::MIN, f32::max);
    let (left, right) = (rows[0].left(), rows[0].right());
    let mut seen = 0;
    for (text, rect) in d.texts() {
        if sizes.contains(&text.as_str()) && rect.center().x > left && rect.center().x < right {
            seen += 1;
            assert!(
                rect.bottom() <= bottom + 0.5,
                "{text:?} is drawn at {rect:?}, below the list's bottom edge {bottom}"
            );
        }
    }
    assert_eq!(seen, rows.len(), "one size per visible row");
    d.click("font_picker_cancel");
    d.click("table_font_cancel");
}
