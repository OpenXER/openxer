//! Headless tests of the Reports view: navigation, bands, columns, the
//! Reports toolbar, the right-click menu, Cut and Paste between groups, Add
//! through the wizard, Run with Last Run Date, report scope, batches and
//! Import of a definition file by content. They drive the production UI
//! through `qa_sweep::Driver` (synthetic egui input only).
use super::groups;
use super::reports_view::{build_lines, Filter, GroupBy, Line, RowKey, SortBy};
use super::rpt::store::{self, ReportScope};
use super::rpt_entries::Which;
use super::test_support::{scheduled_starter, temp};
use crate::qa_sweep::Driver;
use crate::views::MainView;
use eframe::egui::{self, Key, Pos2};

pub(super) fn driver(name: &str) -> Driver {
    let mut d = Driver::new(name, scheduled_starter());
    d.home();
    d
}

/// The rows the view draws: the saved reports and the built-in ones.
fn lines(d: &Driver) -> Vec<Line> {
    build_lines(
        &d.app.report_list(),
        &d.app.report_groups(),
        &d.app.views.reports,
    )
}

/// The captions of the rows, indented by depth, for readable asserts.
fn outline(d: &Driver) -> Vec<String> {
    lines(d)
        .iter()
        .map(|l| format!("{}{}", "  ".repeat(l.row.depth), l.row.cells[0]))
        .collect()
}

/// Run Report to File... for the selected report.
fn open_run_to_file(d: &mut Driver) {
    d.app.reports_command("run_to_file");
    d.frames(2);
    assert!(d.app.reports.run.is_some(), "the dialog opens");
}

pub(super) fn open_view(d: &mut Driver) {
    d.click("dock_reports");
    assert_eq!(d.app.views.main, MainView::Reports);
    d.frames(2);
}

pub(super) fn show_toolbar(d: &mut Driver) {
    d.app.views.shown_toolbars.insert("Reports".into());
    d.frames(2);
}

pub(super) fn row_of(d: &Driver, key: &RowKey) -> usize {
    lines(d)
        .iter()
        .position(|l| &l.key == key)
        .unwrap_or_else(|| panic!("{key:?} is not listed: {:?}", outline(d)))
}

pub(super) fn report_row(d: &Driver, name: &str) -> usize {
    let entries = d.app.report_entries();
    let which = entries
        .iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("no report {name}"))
        .which
        .clone();
    row_of(d, &RowKey::Report(which))
}

/// A point inside the row, clear of the expander and the icon.
pub(super) fn row_point(d: &Driver, row: usize) -> Pos2 {
    let r = d.rect(&format!("reports_row_{row}"));
    Pos2::new(r.left() + 150., r.center().y)
}

fn select(d: &mut Driver, key: &RowKey) {
    let row = row_of(d, key);
    d.click_at(row_point(d, row));
    assert_eq!(d.app.views.reports.selected.as_ref(), Some(key));
}

pub(super) fn select_report(d: &mut Driver, name: &str) {
    let row = report_row(d, name);
    d.click_at(row_point(d, row));
}

pub(super) fn right_click(d: &mut Driver, key: &RowKey) {
    let row = row_of(d, key);
    d.press_at(
        row_point(d, row),
        egui::PointerButton::Secondary,
        Default::default(),
    );
    d.frames(2);
}

/// Clicks a context-menu item: it closes the menu when it is enabled.
pub(super) fn menu_item(d: &mut Driver, item: &str) -> bool {
    let pos = d.rect(item).center();
    d.click_at(pos);
    d.frames(2);
    !d.has(item)
}

pub(super) fn type_into(d: &mut Driver, action: &str, text: &str) {
    d.click(action);
    d.key_with(
        Key::A,
        egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        },
    );
    d.type_text(text);
    d.frames(2);
}

fn group_named(d: &Driver, name: &str) -> u32 {
    d.app
        .report_groups()
        .iter()
        .find(|g| g.name == name)
        .unwrap_or_else(|| panic!("no group {name}"))
        .id
}

/// Adds report groups through the app (the Report Groups dialog does the
/// same with its Add button).
fn add_group(d: &mut Driver, parent: Option<u32>, name: &str) -> u32 {
    let mut list = d.app.report_groups();
    let id = groups::add_group(&mut list, parent, name);
    d.app.save_report_groups(&list);
    d.frames(2);
    id
}

#[test]
fn the_navigation_button_and_the_tools_menu_open_the_reports_view_in_order() {
    let mut d = driver("rv-nav");
    // Projects, Resources, Reports, top to bottom in one bar.
    let top = |d: &Driver, a: &str| d.rect(a).top();
    assert!(top(&d, "directory_projects") < top(&d, "directory_resources"));
    assert!(top(&d, "directory_resources") < top(&d, "dock_reports"));
    assert_eq!(
        d.rect("dock_reports").left(),
        d.rect("directory_projects").left(),
        "in the same bar"
    );
    assert!(!d.has("dock_tracking"), "Tracking was removed");
    assert!(!d.has("dock_tab_reports"), "no tab before it is opened");
    d.click("dock_reports");
    assert_eq!(d.app.views.main, MainView::Reports);
    assert!(d.has("dock_tab_reports") && d.has("reports_display"));
    assert_eq!(d.app.views.opened().last(), Some(&MainView::Reports));
    // It stays open when another view is shown, and its tab brings it back.
    d.click("directory_activities");
    assert_eq!(d.app.views.main, MainView::Activities);
    assert!(d.has("dock_tab_reports"));
    d.click("dock_tab_reports");
    assert_eq!(d.app.views.main, MainView::Reports);
    // Closing the tab goes back to the tab on its left.
    d.click("view_close");
    assert_ne!(d.app.views.main, MainView::Reports);
    assert!(!d.has("dock_tab_reports"));
    // Tools > Reports > Reports opens the same view.
    d.home();
    d.click("menu_bar_tools");
    assert!(d.hover_text("Reports"), "the Reports submenu");
    let pos = d.rect("menu_reports").center();
    d.click_at(pos);
    d.frames(2);
    assert_eq!(d.app.views.main, MainView::Reports);
    assert!(d.has("dock_tab_reports"));
    assert!(d.app.reports.run.is_none(), "no dialog, a view");
    d.assert_clean("navigation");
}

#[test]
fn bands_nest_and_loose_reports_sit_under_no_report_group() {
    let mut d = driver("rv-bands");
    let lib = group_named(&d, store::LIBRARY_GROUP);
    let weekly = add_group(&mut d, Some(lib), "Weekly");
    let deep = add_group(&mut d, Some(weekly), "Deep");
    let _empty = add_group(&mut d, None, "Empty");
    let ids: Vec<i64> = d.app.stored_reports().iter().map(|s| s.id).collect();
    store::set_group(d.app.store.connection(), ids[0], Some(weekly)).unwrap();
    store::set_group(d.app.store.connection(), ids[1], Some(deep)).unwrap();
    // A wizard report without a group.
    let mut def = super::wizard::Definition::new(super::wizard::Subject::Resources);
    def.name = "Crew list".into();
    d.app.save_report_definitions(&[def]);
    open_view(&mut d);
    let rows = lines(&d);
    let text = outline(&d);
    // Band depth follows the group tree; reports sit one level inside.
    assert_eq!(text[0], "Scheduling Reports");
    assert_eq!(text[1], "  Weekly");
    assert_eq!(text[2], "    Deep");
    assert!(rows[0].row.band && rows[1].row.band && rows[2].row.band);
    assert_eq!(rows[1].row.depth, 1);
    assert_eq!(rows[2].row.depth, 2);
    // Reports of Deep are at depth 3, of Weekly 2, of the library group 1.
    assert_eq!(rows[3].row.depth, 3);
    assert!(!rows[3].row.band);
    let bands: Vec<&str> = text
        .iter()
        .filter(|t| !t.starts_with(' ') || t.trim() == "Weekly" || t.trim() == "Deep")
        .map(String::as_str)
        .collect();
    assert!(
        bands.contains(&"Empty"),
        "a group without reports is listed"
    );
    // The loose wizard report is last, under No Report Group.
    let n = text.len();
    assert_eq!(text[n - 2], "No Report Group");
    assert!(rows[n - 2].row.band);
    assert_eq!(text[n - 1].trim(), "Crew list");
    assert_eq!(rows[n - 1].row.depth, 1);
    assert_eq!(rows[n - 1].row.cells[1], "Global");
    // Every drawn row has its action rectangle.
    for i in 0..n {
        assert!(d.has(&format!("reports_row_{i}")), "row {i} is drawn");
    }
    // No loose report: no No Report Group band.
    d.app.save_report_definitions(&[]);
    d.frames(2);
    assert!(!outline(&d).iter().any(|t| t == "No Report Group"));
    d.assert_clean("bands");
}

#[test]
fn bands_expand_and_collapse_and_the_columns_are_the_documented_ones() {
    let mut d = driver("rv-cols");
    open_view(&mut d);
    let full = lines(&d).len();
    // The band, the library reports and the built-in reports listed with
    // them (S-Curve, Earned Hours).
    assert_eq!(
        full,
        1 + super::rpt::library::entries().len() + super::rpt_entries::Builtin::ALL.len()
    );
    // Columns: Report Name, Report Scope, Last Run Date; no Report Group.
    for (i, title) in ["Report Name", "Report Scope", "Last Run Date"]
        .iter()
        .enumerate()
    {
        assert!(d.has(&format!("reports_col_{i}")), "{title} column");
    }
    assert!(!d.has("reports_col_3"));
    assert_eq!(
        super::reports_view::TITLES[..3],
        ["Report Name", "Report Scope", "Last Run Date"]
    );
    let rows = lines(&d);
    assert_eq!(rows[1].row.cells[1], "Global");
    assert_eq!(rows[1].row.cells[2], "", "a report that never ran");
    // The band's - box collapses it, the + box opens it again.
    assert!(d.has("reports_expand_0"));
    d.click("reports_expand_0");
    assert_eq!(lines(&d).len(), 1, "collapsed to its band");
    assert!(!d.has("reports_row_1"));
    d.click("reports_expand_0");
    assert_eq!(lines(&d).len(), full);
    // Collapse All and Expand All from the Display options.
    d.click("reports_display");
    d.click("reports_display_collapse_all");
    assert_eq!(lines(&d).len(), 1);
    d.click("reports_display");
    d.click("reports_display_expand_all");
    assert_eq!(lines(&d).len(), full);
    // The Report Group column is a choice in the Display options.
    d.click("reports_display");
    d.click("reports_display_show_group_column");
    assert!(d.has("reports_col_3"));
    assert_eq!(lines(&d)[1].row.cells[3], store::LIBRARY_GROUP);
    // A double click on a band opens or closes it too.
    let p = row_point(&d, 0);
    d.double_click_at(p);
    assert_eq!(lines(&d).len(), 1);
    d.double_click_at(p);
    assert_eq!(lines(&d).len(), full);
    d.assert_clean("columns");
}

#[test]
fn display_options_group_sort_and_filter() {
    let mut d = driver("rv-display");
    let mut def = super::wizard::Definition::new(super::wizard::Subject::Resources);
    def.name = "Crew list".into();
    d.app.save_report_definitions(&[def]);
    // One library report becomes a project report.
    let id = d.app.stored_reports()[0].id;
    let open = d.app.open_project_id();
    store::set_scope(d.app.store.connection(), id, &ReportScope::Project(open)).unwrap();
    open_view(&mut d);
    assert!(d.has("reports_display"));
    // Filter: Project Reports lists the one report under its group; the
    // wizard report and its band are gone.
    d.click("reports_display");
    d.click_text("Filter By");
    d.click("reports_display_filter_project");
    assert_eq!(d.app.views.reports.filter, Filter::Project);
    let text = outline(&d);
    assert_eq!(text.len(), 2, "{text:?}");
    assert_eq!(text[0], store::LIBRARY_GROUP);
    // Group By Report Scope: bands Global and Project.
    d.app.views.reports.filter = Filter::All;
    d.click("reports_display");
    d.click_text("Group By");
    d.click("reports_display_group_by_scope");
    assert_eq!(d.app.views.reports.group_by, GroupBy::Scope);
    let text = outline(&d);
    assert_eq!(text[0], "Global");
    assert!(text.iter().any(|t| t == "Project"));
    // Group By None: a flat list with no bands.
    d.app.views.reports.group_by = GroupBy::Nothing;
    assert!(lines(&d).iter().all(|l| !l.row.band && l.row.depth == 0));
    // Sort By Last Run Date puts the report that ran first.
    d.app.views.reports.group_by = GroupBy::ReportGroup;
    d.app.mark_run(&Which::Stored(d.app.stored_reports()[3].id));
    d.app.views.reports.sort = SortBy::LastRun;
    assert_eq!(
        lines(&d)[1].row.cells[0],
        d.app.stored_reports()[3].name,
        "newest run first"
    );
    // The choices are kept with the layout.
    let json = d.app.views.json();
    let mut again = crate::views::Views::default();
    again.restore(&json);
    assert_eq!(again.reports.sort, SortBy::LastRun);
    assert_eq!(again.reports.group_by, GroupBy::ReportGroup);
    assert_eq!(again.reports.columns, d.app.views.reports.columns);
    d.assert_clean("display");
}

#[test]
fn every_reports_toolbar_button_opens_its_command_and_cancel_changes_nothing() {
    let mut d = driver("rv-toolbar");
    open_view(&mut d);
    show_toolbar(&mut d);
    let stored = d.app.stored_reports();
    let model = d.app.project.model.clone();
    // Eight buttons, in the documented order.
    let order = [
        "tool_run_report",
        "tool_run_batch",
        "tool_modify_report",
        "tool_import_report",
        "tool_export_report",
        "tool_report_wizard",
        "tool_report_groups",
        "tool_batch_reports",
    ];
    let lefts: Vec<f32> = order.iter().map(|a| d.rect(a).left()).collect();
    assert!(lefts.windows(2).all(|w| w[0] < w[1]), "{lefts:?}");
    // Without a selected report the report commands are off.
    let baseline = d.keys();
    for off in [
        "tool_run_report",
        "tool_modify_report",
        "tool_export_report",
    ] {
        d.click(off);
        assert_eq!(d.keys(), baseline, "{off} needs a selected report");
    }
    select_report(&mut d, "Critical Activities");
    let baseline = d.keys();
    for action in order {
        d.click(action);
        if action == "tool_run_report" {
            let ids = d.app.reports.tabs.ids();
            assert_eq!(ids.len(), 1, "Run Report opens a tab, not a dialog");
            d.app.close_tab(MainView::Report(ids[0]));
            d.frames(2);
            assert_eq!(d.app.views.main, MainView::Reports);
            continue;
        }
        let opened = match action {
            "tool_run_batch" => d.app.reports.run_batch.is_some(),
            "tool_modify_report" => d.app.reports.editor.is_some(),
            "tool_import_report" => d.app.reports.import.is_some(),
            "tool_export_report" => d.app.reports.export.is_some(),
            "tool_report_wizard" => d.app.reports.wizard.is_some(),
            "tool_report_groups" => d.app.reports.groups.is_some(),
            _ => d.app.reports.batch.is_some(),
        };
        assert!(opened, "{action} opens its dialog");
        let pressed = d.close_dialogs(&baseline, action);
        assert!(!pressed.is_empty(), "{action} closes with a button");
        d.assert_clean(action);
        // And again with Escape.
        d.click(action);
        d.key(Key::Escape);
        d.frames(2);
        assert!(d.app.reports.run.is_none() && d.app.reports.editor.is_none());
        assert!(d.app.reports.wizard.is_none() && d.app.reports.import.is_none());
    }
    // Running a report records its run; nothing else in the list changed.
    let unrun = |list: Vec<store::Stored>| -> Vec<store::Stored> {
        list.into_iter()
            .map(|mut s| {
                s.last_run = None;
                s
            })
            .collect()
    };
    assert_eq!(unrun(d.app.stored_reports()), unrun(stored));
    assert!(d.app.project.model == model);
    assert!(!d.app.dirty);
    // Modify on a wizard report opens the wizard on its page of columns.
    let mut def = super::wizard::Definition::new(super::wizard::Subject::Resources);
    def.name = "Crew list".into();
    d.app.save_report_definitions(&[def]);
    d.frames(2);
    select_report(&mut d, "Crew list");
    d.click("tool_modify_report");
    let w = d.app.reports.wizard.as_ref().expect("the wizard opens");
    assert_eq!(w.editing, Some(0));
    d.key(Key::Escape);
    d.frames(2);
    // Export is for definition reports only.
    let baseline = d.keys();
    d.click("tool_export_report");
    assert!(d.app.reports.export.is_none());
    assert_eq!(d.keys(), baseline);
    d.assert_clean("toolbar");
}

#[test]
fn the_right_click_menu_lists_the_reference_commands() {
    let mut d = driver("rv-menu");
    open_view(&mut d);
    let key = RowKey::Report(d.app.report_entries()[2].which.clone());
    right_click(&mut d, &key);
    for item in [
        "reports_menu_run",
        "reports_menu_modify",
        "reports_menu_import",
        "reports_menu_export",
        "reports_menu_cut",
        "reports_menu_copy",
        "reports_menu_paste",
        "reports_menu_display",
    ] {
        assert!(d.has(item), "{item} is on the menu");
    }
    // Paste waits for a Cut or Copy.
    assert!(!menu_item(&mut d, "reports_menu_paste"));
    // Run > Report and Run > Batch.
    assert!(d.hover_text("Run"));
    assert!(d.has("reports_menu_run_report") && d.has("reports_menu_run_batch"));
    assert!(d.has("reports_menu_run_to_file"));
    assert!(menu_item(&mut d, "reports_menu_run_to_file"));
    assert!(d.app.reports.run.is_some(), "Run > Report to File");
    d.click("run_report_cancel");
    right_click(&mut d, &key);
    assert!(d.hover_text("Run"));
    assert!(menu_item(&mut d, "reports_menu_run_report"));
    assert_eq!(
        d.app.reports.tabs.ids().len(),
        1,
        "Run > Report opens a tab"
    );
    assert!(d.app.reports.run.is_none());
    d.app
        .close_tab(MainView::Report(d.app.reports.tabs.ids()[0]));
    d.frames(2);
    right_click(&mut d, &key);
    assert!(d.hover_text("Run"));
    assert!(menu_item(&mut d, "reports_menu_run_batch"));
    assert!(d.app.reports.run_batch.is_some(), "Run > Batch");
    d.click("run_batch_cancel");
    // Modify, Export and Import.
    right_click(&mut d, &key);
    assert!(menu_item(&mut d, "reports_menu_modify"));
    assert!(d.app.reports.editor.is_some());
    d.click("report_editor_cancel");
    right_click(&mut d, &key);
    assert!(menu_item(&mut d, "reports_menu_export"));
    assert!(d.app.reports.export.is_some());
    d.click("export_report_cancel");
    right_click(&mut d, &key);
    assert!(menu_item(&mut d, "reports_menu_import"));
    assert!(d.app.reports.import.is_some());
    d.click("import_reports_cancel");
    // Copy then Paste puts a copy in the group.
    let count = d.app.stored_reports().len();
    right_click(&mut d, &key);
    assert!(menu_item(&mut d, "reports_menu_copy"));
    right_click(&mut d, &key);
    assert!(menu_item(&mut d, "reports_menu_paste"));
    assert_eq!(d.app.stored_reports().len(), count + 1);
    // A band row: the report commands are off.
    let band = RowKey::Group(group_named(&d, store::LIBRARY_GROUP));
    right_click(&mut d, &band);
    for off in [
        "reports_menu_modify",
        "reports_menu_export",
        "reports_menu_cut",
        "reports_menu_copy",
        "reports_menu_delete",
    ] {
        assert!(!menu_item(&mut d, off), "{off} needs a report");
    }
    assert!(menu_item(&mut d, "reports_menu_import"));
    d.click("import_reports_cancel");
    d.assert_clean("menu");
}

#[test]
fn cut_and_paste_moves_a_report_between_groups() {
    let mut d = driver("rv-cut");
    let lib = group_named(&d, store::LIBRARY_GROUP);
    let site = add_group(&mut d, None, "Site Reports");
    let mut def = super::wizard::Definition::new(super::wizard::Subject::Resources);
    def.name = "Crew list".into();
    d.app.save_report_definitions(&[def]);
    open_view(&mut d);
    let group_of = |d: &Driver, name: &str| {
        d.app
            .report_entries()
            .into_iter()
            .find(|e| e.name == name)
            .unwrap()
            .group
    };
    let count = d.app.report_entries().len();
    assert_eq!(group_of(&d, "Milestones"), Some(lib));
    // Edit toolbar: Cut, select the group band, Paste.
    select_report(&mut d, "Milestones");
    d.click("dock_cut");
    assert!(d.app.reports.clipboard.is_some());
    select(&mut d, &RowKey::Group(site));
    d.click("dock_paste");
    assert_eq!(group_of(&d, "Milestones"), Some(site));
    assert_eq!(d.app.report_entries().len(), count, "moved, not copied");
    assert!(d.app.reports.clipboard.is_none(), "a Cut pastes once");
    let text = outline(&d);
    let at = text.iter().position(|t| t == "Site Reports").unwrap();
    assert_eq!(text[at + 1].trim(), "Milestones");
    // Right-click menu: Cut on the report, Paste on the library band.
    let key = RowKey::Report(report_which(&d, "Milestones"));
    right_click(&mut d, &key);
    assert!(menu_item(&mut d, "reports_menu_cut"));
    right_click(&mut d, &RowKey::Group(lib));
    assert!(menu_item(&mut d, "reports_menu_paste"));
    assert_eq!(group_of(&d, "Milestones"), Some(lib));
    // Paste on the No Report Group band leaves the report without a group.
    select_report(&mut d, "Milestones");
    d.click("dock_cut");
    select(&mut d, &RowKey::NoGroup);
    d.click("dock_paste");
    assert_eq!(group_of(&d, "Milestones"), None);
    // A wizard report can be cut and pasted too.
    select_report(&mut d, "Crew list");
    d.click("dock_cut");
    select(&mut d, &RowKey::Group(site));
    d.click("dock_paste");
    assert_eq!(group_of(&d, "Crew list"), Some(site));
    // Copy and Paste adds a copy next to the original, with a free name.
    select_report(&mut d, "Crew list");
    d.click("dock_copy");
    d.click("dock_paste");
    assert_eq!(d.app.report_entries().len(), count + 1);
    assert!(d
        .app
        .report_entries()
        .iter()
        .any(|e| e.name == "Crew list (2)" && e.group == Some(site)));
    // The Edit menu does the same in this view.
    d.click("menu_bar_edit");
    assert!(d.has("edit_cut") && d.has("edit_copy") && d.has("edit_paste"));
    d.key(Key::Escape);
    d.assert_clean("cut");
}

pub(super) fn report_which(d: &Driver, name: &str) -> Which {
    d.app
        .report_entries()
        .into_iter()
        .find(|e| e.name == name)
        .unwrap()
        .which
}

#[test]
fn delete_removes_the_selected_report() {
    let mut d = driver("rv-delete");
    open_view(&mut d);
    let count = d.app.report_entries().len();
    select_report(&mut d, "Milestones");
    d.click("dock_delete");
    assert_eq!(d.app.report_entries().len(), count - 1);
    assert!(d
        .app
        .report_entries()
        .iter()
        .all(|e| e.name != "Milestones"));
    assert!(d.app.views.reports.selected.is_none());
    d.assert_clean("delete");
}

#[test]
fn add_starts_the_wizard_in_the_selected_group() {
    let mut d = driver("rv-add");
    let site = add_group(&mut d, None, "Site Reports");
    open_view(&mut d);
    select(&mut d, &RowKey::Group(site));
    d.click("add");
    let w = d.app.reports.wizard.as_ref().expect("Add opens the wizard");
    assert_eq!(w.group, Some(site));
    d.click("wizard_next");
    d.click("wizard_next");
    d.click("wizard_next");
    d.app.reports.wizard.as_mut().unwrap().save_name = "Site lookahead".into();
    d.click("wizard_finish");
    assert!(d.app.reports.wizard.is_none(), "{:?}", d.app.status);
    // Finish ran the report into a tab; back to the list.
    assert_eq!(d.app.reports.tabs.ids().len(), 1);
    d.app.close_report_tabs();
    d.frames(2);
    let saved = d.app.report_definitions();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].group, Some(site), "lands in the selected group");
    d.frames(2);
    let text = outline(&d);
    let at = text.iter().position(|t| t == "Site Reports").unwrap();
    assert_eq!(text[at + 1].trim(), "Site lookahead");
    // With a report selected, the new one goes in that report's group.
    select_report(&mut d, "Critical Activities");
    d.click("add");
    let lib = group_named(&d, store::LIBRARY_GROUP);
    assert_eq!(d.app.reports.wizard.as_ref().unwrap().group, Some(lib));
    d.key(Key::Escape);
    d.frames(2);
    // With nothing selected it has no group.
    d.app.views.reports.selected = None;
    d.click("add");
    assert_eq!(d.app.reports.wizard.as_ref().unwrap().group, None);
    d.key(Key::Escape);
    d.assert_clean("add");
}

#[test]
fn run_report_to_a_tab_html_and_text_updates_last_run_date() {
    let mut d = driver("rv-run");
    open_view(&mut d);
    show_toolbar(&mut d);
    let model = d.app.project.model.clone();
    select_report(&mut d, "Critical Activities");
    let which = report_which(&d, "Critical Activities");
    let last = |d: &Driver| {
        d.app
            .report_entries()
            .into_iter()
            .find(|e| e.which == which)
            .unwrap()
            .last_run
    };
    assert_eq!(last(&d), None);
    // Run Report goes straight to a tab.
    d.click("tool_run_report");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    assert!(d.app.reports.run.is_none(), "no dialog");
    let ids = d.app.reports.tabs.ids();
    assert_eq!(ids.len(), 1);
    assert_eq!(d.app.views.main, MainView::Report(ids[0]));
    let first = last(&d).expect("the run is recorded");
    d.app.close_tab(MainView::Report(ids[0]));
    d.frames(2);
    assert_eq!(d.app.views.main, MainView::Reports);
    // Run to File lists the file targets and the notes.
    open_run_to_file(&mut d);
    for t in 0..3 {
        assert!(d.has(&format!("run_report_target_{t}")));
    }
    assert!(!d.has("run_report_target_3"));
    assert!(d.has("run_report_notes") && d.has("run_report_open"));
    assert_eq!(
        d.app.reports.run.as_ref().unwrap().target,
        super::rpt_run::Target::Html
    );
    d.click("run_report_cancel");
    // The Last Run Date column shows it.
    let row = report_row(&d, "Critical Activities");
    assert_ne!(lines(&d)[row].row.cells[2], "");
    assert_eq!(
        lines(&d)[row].row.cells[2],
        super::rpt_entries::last_run_text(Some(&first))
    );
    // HTML file.
    open_run_to_file(&mut d);
    d.click("run_report_target_0");
    let html = d.dir.join("critical.html");
    type_into(&mut d, "run_report_path", &html.display().to_string());
    d.click("run_report_run");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    assert!(std::fs::read_to_string(&html)
        .unwrap()
        .contains("Critical Activities"));
    assert!(last(&d).unwrap() >= first);
    // ASCII text file with a semicolon delimiter and a single quote.
    open_run_to_file(&mut d);
    d.click("run_report_target_1");
    let txt = d.dir.join("critical.txt");
    type_into(&mut d, "run_report_path", &txt.display().to_string());
    d.click("run_report_delimiter");
    assert!(d.click_text("Semicolon"));
    d.click("run_report_qualifier");
    assert!(d.click_text("Single quote"));
    d.click("run_report_notes");
    d.type_text("my notes");
    d.click("run_report_run");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    let text = std::fs::read_to_string(&txt).unwrap();
    assert!(
        text.lines()
            .any(|l| l.starts_with("'RB-") && l.contains("';'")),
        "{text}"
    );
    // A run that fails does not record a run time.
    let before = last(&d);
    open_run_to_file(&mut d);
    d.click("run_report_target_0");
    let missing = d.dir.join("no such folder").join("critical.html");
    type_into(&mut d, "run_report_path", &missing.display().to_string());
    d.click("run_report_run");
    assert!(d.app.reports.run.as_ref().unwrap().error.is_some());
    assert_eq!(last(&d), before);
    d.click("run_report_cancel");
    // A wizard report runs through the same dialog and records its run.
    let mut def = super::wizard::Definition::new(super::wizard::Subject::Resources);
    def.name = "Crew list".into();
    d.app.save_report_definitions(&[def]);
    d.frames(2);
    select_report(&mut d, "Crew list");
    open_run_to_file(&mut d);
    d.click("run_report_target_0");
    let crew = d.dir.join("crew.html");
    type_into(&mut d, "run_report_path", &crew.display().to_string());
    d.click("run_report_run");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    assert!(crew.exists());
    assert!(d
        .app
        .report_entries()
        .iter()
        .any(|e| e.name == "Crew list" && e.last_run.is_some()));
    // The project is untouched and nothing needs saving.
    assert!(d.app.project.model == model);
    assert!(!d.app.dirty);
}

#[test]
fn global_reports_run_anywhere_and_project_reports_only_on_their_project() {
    let mut d = driver("rv-scope");
    open_view(&mut d);
    show_toolbar(&mut d);
    let id = |d: &Driver, name: &str| match report_which(d, name) {
        Which::Stored(id) => id,
        Which::Wizard(_) | Which::Builtin(_) => unreachable!(),
    };
    // Report Scope in the right-click menu: make it a project report of the
    // open project.
    let key = RowKey::Report(report_which(&d, "Milestones"));
    right_click(&mut d, &key);
    assert!(d.hover_text("Report Scope"));
    assert!(menu_item(&mut d, "reports_menu_scope_project"));
    let open = d.app.open_project_id();
    let entry = d
        .app
        .report_entries()
        .into_iter()
        .find(|e| e.name == "Milestones")
        .unwrap();
    assert_eq!(entry.scope, ReportScope::Project(open.clone()));
    let row = report_row(&d, "Milestones");
    assert_eq!(lines(&d)[row].row.cells[1], "Project");
    // It runs on its own project.
    select_report(&mut d, "Milestones");
    d.click("tool_run_report");
    assert_eq!(d.app.reports.tabs.ids().len(), 1);
    d.app
        .close_tab(MainView::Report(d.app.reports.tabs.ids()[0]));
    // Another project's report does not run here.
    let milestones = id(&d, "Milestones");
    store::set_scope(
        d.app.store.connection(),
        milestones,
        &ReportScope::Project("OTHER".into()),
    )
    .unwrap();
    d.frames(2);
    let baseline = d.keys();
    d.click("tool_run_report");
    assert!(d.app.reports.tabs.ids().is_empty(), "the button is off");
    assert_eq!(d.keys(), baseline);
    d.click_at(row_point(&d, report_row(&d, "Milestones")));
    d.double_click_at(row_point(&d, report_row(&d, "Milestones")));
    assert!(d.app.reports.tabs.ids().is_empty());
    assert!(
        d.app.status.contains("OTHER"),
        "the status line says why: {}",
        d.app.status
    );
    right_click(&mut d, &key);
    assert!(d.hover_text("Run"));
    menu_item(&mut d, "reports_menu_run_report");
    assert!(d.app.reports.tabs.ids().is_empty(), "the menu item is off");
    d.key(Key::Escape);
    d.key(Key::Escape);
    let e = d
        .app
        .report_entries()
        .into_iter()
        .find(|e| e.name == "Milestones")
        .unwrap();
    assert!(d
        .app
        .run_report_entry(
            &e.which,
            super::rpt_run::Target::Html,
            &d.dir.join("refused.html"),
            &Default::default(),
            ""
        )
        .is_err());
    let after = d
        .app
        .report_entries()
        .into_iter()
        .find(|x| x.name == "Milestones")
        .unwrap();
    assert_eq!(after.last_run, e.last_run, "a refused run is not a run");
    // A global report runs on any project.
    select_report(&mut d, "Critical Activities");
    d.click("tool_run_report");
    assert_eq!(d.app.reports.tabs.ids().len(), 1);
    assert!(!d.dir.join("refused.html").exists());
    d.app.close_report_tabs();
    d.frames(2);
    // Back to global in the menu.
    right_click(&mut d, &key);
    assert!(d.hover_text("Report Scope"));
    assert!(menu_item(&mut d, "reports_menu_scope_global"));
    assert!(d
        .app
        .report_entries()
        .iter()
        .any(|e| e.name == "Milestones" && e.scope.is_global()));
    d.assert_clean("scope");
}

#[test]
fn the_group_lookup_sets_or_clears_a_reports_group() {
    let mut d = driver("rv-lookup");
    let lib = group_named(&d, store::LIBRARY_GROUP);
    let site = add_group(&mut d, None, "Site Reports");
    open_view(&mut d);
    let key = RowKey::Report(report_which(&d, "Milestones"));
    right_click(&mut d, &key);
    assert!(menu_item(&mut d, "reports_menu_report_group"));
    assert!(d.app.reports.pick_group.is_some());
    assert_eq!(
        d.app.reports.pick_group.as_ref().unwrap().selected,
        Some(lib)
    );
    // Cancel changes nothing.
    d.click("pick_group_cancel");
    assert!(d.app.reports.pick_group.is_none());
    right_click(&mut d, &key);
    menu_item(&mut d, "reports_menu_report_group");
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.reports.pick_group.is_none(), "Escape closes it");
    let group = |d: &Driver| {
        d.app
            .report_entries()
            .into_iter()
            .find(|e| e.name == "Milestones")
            .unwrap()
            .group
    };
    assert_eq!(group(&d), Some(lib));
    // Pick the other group and Select.
    right_click(&mut d, &key);
    menu_item(&mut d, "reports_menu_report_group");
    let rows = groups::outline_by(&d.app.report_groups(), &[]);
    let at = rows
        .iter()
        .position(|(n, _)| *n == groups::Node::Group(site))
        .unwrap();
    d.click(&format!("pick_group_rows_row_{at}"));
    d.click("pick_group_select");
    assert_eq!(group(&d), Some(site));
    // Clear takes it out of every group.
    let key = RowKey::Report(report_which(&d, "Milestones"));
    right_click(&mut d, &key);
    menu_item(&mut d, "reports_menu_report_group");
    d.click("pick_group_clear");
    assert_eq!(group(&d), None);
    d.assert_clean("lookup");
}

#[test]
fn import_accepts_a_definition_by_content_whatever_its_extension() {
    let mut d = driver("rv-erp");
    open_view(&mut d);
    show_toolbar(&mut d);
    let id = match report_which(&d, "Milestones") {
        Which::Stored(id) => id,
        Which::Wizard(_) | Which::Builtin(_) => unreachable!(),
    };
    let text = store::export(d.app.store.connection(), id).unwrap();
    let dir = temp("rv-erp-files");
    let good = dir.join("milestones.erp");
    std::fs::write(&good, &text).unwrap();
    let count = d.app.stored_reports().len();
    let lib = group_named(&d, store::LIBRARY_GROUP);
    select(&mut d, &RowKey::Group(lib));
    d.click("tool_import_report");
    type_into(&mut d, "import_reports_path", &good.display().to_string());
    d.click("import_reports_ok");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    assert!(d.app.reports.import.is_none());
    assert_eq!(d.app.stored_reports().len(), count + 1);
    let new = d
        .app
        .stored_reports()
        .into_iter()
        .find(|s| s.name == "Milestones (2)")
        .unwrap();
    assert_eq!(new.group, Some(group_named(&d, store::LIBRARY_GROUP)));
    assert_eq!(new.kind, store::KIND_USER);
    assert_eq!(store::load(d.app.store.connection(), new.id).unwrap(), text);
    // A file with that extension that is not definition text is refused.
    let bad = dir.join("other.erp");
    std::fs::write(&bad, [0x50u8, 0x4b, 3, 4, 0, 1, 2, 3]).unwrap();
    d.click("tool_import_report");
    type_into(&mut d, "import_reports_path", &bad.display().to_string());
    d.click("import_reports_ok");
    assert!(d.app.reports.import.as_ref().unwrap().error.is_some());
    assert_eq!(d.app.stored_reports().len(), count + 1);
    d.click("import_reports_cancel");
    // The reverse: definition text in a file with any other extension.
    let odd = dir.join("milestones.dat");
    std::fs::write(&odd, &text).unwrap();
    d.click("tool_import_report");
    type_into(&mut d, "import_reports_path", &odd.display().to_string());
    d.click("import_reports_ok");
    assert!(d.app.error.is_none());
    assert_eq!(d.app.stored_reports().len(), count + 2);
}

/// Saved batches in the workspace, as the Batch Reports dialog saves them.
fn save_batch(d: &Driver, name: &str, scope: ReportScope, reports: &[&str]) {
    let ids: Vec<i64> = reports
        .iter()
        .map(|n| match report_which(d, n) {
            Which::Stored(id) => id,
            Which::Wizard(_) | Which::Builtin(_) => unreachable!(),
        })
        .collect();
    store::save_batch_scoped(d.app.store.connection(), name, &scope, &ids).unwrap();
}

#[test]
fn run_batch_writes_the_reports_in_order_and_records_their_runs() {
    let mut d = driver("rv-batch");
    open_view(&mut d);
    show_toolbar(&mut d);
    let open = d.app.open_project_id();
    save_batch(
        &d,
        "Friday package",
        ReportScope::Global,
        &["Milestones", "Critical Activities"],
    );
    save_batch(
        &d,
        "Elsewhere",
        ReportScope::Project("OTHER".into()),
        &["Milestones"],
    );
    save_batch(&d, "Here only", ReportScope::Project(open), &["Milestones"]);
    // The dialog lists the global batches and this project's own.
    d.click("tool_run_batch");
    let names: Vec<String> = d
        .app
        .runnable_batches()
        .into_iter()
        .map(|b| b.name)
        .collect();
    assert_eq!(names, ["Friday package", "Here only"]);
    assert!(d.has("run_batch_rows_row_1") && !d.has("run_batch_rows_row_2"));
    // No Print Preview for a batch: HTML, text and PDF only.
    assert_eq!(super::batch_run::FORMATS.len(), 3);
    // Nothing is picked yet; Run waits.
    d.click("run_batch_run");
    assert!(d.app.reports.run_batch.is_some());
    d.click("run_batch_rows_row_0");
    let folder = d.dir.join("package");
    type_into(&mut d, "run_batch_folder", &folder.display().to_string());
    d.click("run_batch_run");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    assert!(d.app.reports.run_batch.is_none(), "the dialog closes");
    assert!(folder.join("Milestones.html").exists());
    assert!(folder.join("Critical Activities.html").exists());
    for name in ["Milestones", "Critical Activities"] {
        assert!(
            d.app
                .report_entries()
                .iter()
                .any(|e| e.name == name && e.last_run.is_some()),
            "{name} ran"
        );
    }
    // The files come out in the batch's order.
    let batch = d.app.runnable_batches().remove(0);
    let files = d
        .app
        .run_saved_batch(
            &batch,
            &d.dir.join("again"),
            crate::reports::doc::Output::Pdf,
            "",
        )
        .unwrap();
    let names: Vec<String> = files
        .iter()
        .map(|f| f.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["Milestones.pdf", "Critical Activities.pdf"]);
    // A project batch for another project does not run here.
    let other = store::batches(d.app.store.connection())
        .unwrap()
        .into_iter()
        .find(|b| b.name == "Elsewhere")
        .unwrap();
    assert!(d
        .app
        .run_saved_batch(
            &other,
            &d.dir.join("never"),
            crate::reports::doc::Output::Html,
            ""
        )
        .is_err());
    assert!(!d.dir.join("never").exists());
}

#[test]
fn batch_reports_assigns_orders_and_saves_batches_by_scope() {
    let mut d = driver("rv-batch-setup");
    open_view(&mut d);
    show_toolbar(&mut d);
    d.click("tool_batch_reports");
    // Assign two reports (the check boxes), then reorder them.
    let entries = d.app.report_entries();
    let member: Vec<Option<u32>> = entries.iter().map(|e| e.group).collect();
    let rows = groups::outline_by(&d.app.report_groups(), &member);
    let row = |name: &str| {
        rows.iter()
            .position(|(n, _)| matches!(n, groups::Node::Report(i) if entries[*i].name == name))
            .unwrap()
    };
    d.click(&format!("batch_rows_row_{}", row("Milestones")));
    d.click(&format!("batch_rows_row_{}", row("Critical Activities")));
    let order: Vec<String> = d
        .app
        .reports
        .batch
        .as_ref()
        .unwrap()
        .order
        .iter()
        .map(|&i| entries[i].name.clone())
        .collect();
    assert_eq!(order, ["Milestones", "Critical Activities"], "check order");
    d.click("batch_order_row_1");
    d.click("batch_reports_up");
    let order: Vec<String> = d
        .app
        .reports
        .batch
        .as_ref()
        .unwrap()
        .order
        .iter()
        .map(|&i| entries[i].name.clone())
        .collect();
    assert_eq!(order, ["Critical Activities", "Milestones"], "Move Up");
    // Save as a global batch.
    type_into(&mut d, "batch_reports_name", "Weekly pack");
    d.click("batch_reports_save");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    let saved = store::batches(d.app.store.connection()).unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].name, "Weekly pack");
    assert_eq!(saved[0].scope, ReportScope::Global);
    let names: Vec<String> = saved[0]
        .reports
        .iter()
        .map(|id| {
            d.app
                .report_entries()
                .into_iter()
                .find(|e| e.which == Which::Stored(*id))
                .unwrap()
                .name
        })
        .collect();
    assert_eq!(names, ["Critical Activities", "Milestones"]);
    // A project batch takes only project reports: these are global.
    d.click("batch_reports_scope");
    assert!(d.click_text("Project"));
    type_into(&mut d, "batch_reports_name", "Project pack");
    d.click("batch_reports_save");
    assert!(d
        .app
        .error
        .as_deref()
        .unwrap_or("")
        .contains("takes project reports"));
    d.app.error = None;
    d.frames(2);
    assert_eq!(store::batches(d.app.store.connection()).unwrap().len(), 1);
    // Pick the saved batch: it checks its reports in its order; Delete.
    d.click("batch_reports_saved");
    assert!(d.click_text("Weekly pack"));
    d.frames(2);
    assert_eq!(d.app.reports.batch.as_ref().unwrap().order.len(), 2);
    d.click("batch_reports_delete");
    assert!(store::batches(d.app.store.connection()).unwrap().is_empty());
    d.click("batch_reports_close");
    d.assert_clean("batch setup");
}

#[test]
fn a_cancelled_dialog_leaves_reports_groups_and_the_project_alone() {
    let mut d = driver("rv-cancel");
    open_view(&mut d);
    show_toolbar(&mut d);
    let reports = d.app.report_entries();
    let groups_before = d.app.report_groups();
    let model = d.app.project.model.clone();
    select_report(&mut d, "Milestones");
    let baseline = d.keys();
    for action in [
        "tool_run_batch",
        "tool_modify_report",
        "tool_import_report",
        "tool_export_report",
        "tool_report_wizard",
        "tool_report_groups",
        "tool_batch_reports",
    ] {
        d.click(action);
        d.close_dialogs(&baseline, action);
    }
    assert_eq!(d.app.report_entries(), reports);
    assert_eq!(d.app.report_groups(), groups_before);
    assert!(d.app.project.model == model);
    assert!(!d.app.dirty);
    assert_eq!(d.app.history.counts().0, 0, "no undo step");
}

#[test]
fn render_the_reports_view() {
    use crate::headless_render::SoftwarePainter;
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/r2");
    std::fs::create_dir_all(&out).unwrap();
    let dir = temp("rv-render");
    let (mut app, ctx) = super::test_support::app(&dir);
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    // Groups, a loose report, a project report and a run.
    let mut groups_list = app.report_groups();
    let lib = groups_list[0].id;
    let weekly = groups::add_group(&mut groups_list, Some(lib), "Weekly Packages");
    let site = groups::add_group(&mut groups_list, None, "Site Reports");
    app.save_report_groups(&groups_list);
    let stored = app.stored_reports();
    store::set_group(app.store.connection(), stored[0].id, Some(weekly)).unwrap();
    store::set_group(app.store.connection(), stored[1].id, Some(site)).unwrap();
    store::set_scope(
        app.store.connection(),
        stored[1].id,
        &ReportScope::Project(app.open_project_id()),
    )
    .unwrap();
    let mut def = super::wizard::Definition::new(super::wizard::Subject::Resources);
    def.name = "Crew list".into();
    app.save_report_definitions(&[def]);
    app.mark_run(&Which::Stored(stored[2].id));
    app.show_view(MainView::Reports);
    app.views.reports.selected = Some(RowKey::Report(Which::Stored(stored[2].id)));
    app.views.shown_toolbars.insert("Reports".into());
    for _ in 0..4 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("reports-view.png")).unwrap();
    // The Report Group column, the Display options open.
    app.reports_command("show_group_column");
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image
        .save(out.join("reports-view-group-column.png"))
        .unwrap();
    assert!(app.error.is_none(), "{:?}", app.error);
}

/// Row of a group in the Report Groups dialog's tree.
fn dialog_row(d: &Driver, id: u32) -> usize {
    let entries = d.app.report_entries();
    let member: Vec<Option<u32>> = entries.iter().map(|e| e.group).collect();
    groups::outline_by(&d.app.report_groups(), &member)
        .iter()
        .position(|(n, _)| *n == groups::Node::Group(id))
        .unwrap()
}

#[test]
fn report_groups_cut_paste_arrows_and_the_delete_rules() {
    let mut d = driver("rv-groups");
    let lib = group_named(&d, store::LIBRARY_GROUP);
    let sub = add_group(&mut d, Some(lib), "Sub");
    let other = add_group(&mut d, None, "Other");
    let ids: Vec<i64> = d.app.stored_reports().iter().map(|s| s.id).collect();
    store::set_group(d.app.store.connection(), ids[0], Some(sub)).unwrap();
    open_view(&mut d);
    show_toolbar(&mut d);
    d.click("tool_report_groups");
    let parent = |d: &Driver, id: u32| {
        d.app
            .report_groups()
            .iter()
            .find(|g| g.id == id)
            .and_then(|g| g.parent)
    };
    // A top-level group that holds reports cannot be deleted.
    let r = dialog_row(&d, lib);
    d.click(&format!("report_groups_rows_row_{r}"));
    d.click("report_groups_delete");
    assert!(
        d.app.error.as_deref().unwrap_or("").contains("top-level"),
        "{:?}",
        d.app.error
    );
    d.app.error = None;
    d.frames(2);
    assert_eq!(d.app.report_groups().len(), 3);
    // A sub-group merges its reports into its parent.
    let r = dialog_row(&d, sub);
    d.click(&format!("report_groups_rows_row_{r}"));
    d.click("report_groups_delete");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    assert_eq!(d.app.report_groups().len(), 2);
    let moved = d
        .app
        .stored_reports()
        .into_iter()
        .find(|s| s.id == ids[0])
        .unwrap();
    assert_eq!(moved.group, Some(lib));
    // Cut Other, Paste on the library group: it moves inside.
    let r = dialog_row(&d, other);
    d.click(&format!("report_groups_rows_row_{r}"));
    d.click("report_groups_cut");
    let r = dialog_row(&d, lib);
    d.click(&format!("report_groups_rows_row_{r}"));
    d.click("report_groups_paste");
    assert_eq!(parent(&d, other), Some(lib));
    // The arrows: out to the top, then in under the group before it.
    let r = dialog_row(&d, other);
    d.click(&format!("report_groups_rows_row_{r}"));
    d.click("report_groups_out");
    assert_eq!(parent(&d, other), None);
    d.click("report_groups_up");
    assert_eq!(d.app.report_groups()[0].id, other);
    d.click("report_groups_down");
    assert_eq!(d.app.report_groups()[0].id, lib);
    d.click("report_groups_in");
    assert_eq!(parent(&d, other), Some(lib));
    // Copy and Paste: a copy of the group, under the selected group.
    d.click("report_groups_copy");
    let r = dialog_row(&d, lib);
    d.click(&format!("report_groups_rows_row_{r}"));
    d.click("report_groups_paste");
    assert!(d
        .app
        .report_groups()
        .iter()
        .any(|g| g.name == "Other (copy)" && g.parent == Some(lib)));
    d.click("report_groups_close");
    d.assert_clean("groups");
}
