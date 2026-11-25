//! Headless tests of report tabs: a run opens a tab in the main view area,
//! the viewer shows it, and Export, Print, Copy and Refresh act on it. They
//! click through the production UI with `qa_sweep::Driver` (synthetic egui
//! input only, no window).
use super::doc::{self, Output};
use super::report_tab::{default_file_name, seam, Doc, Export, ReportTab, ReportTabId};
use super::reports_view_tests::{
    driver, open_view, report_row, report_which, row_point, select_report, show_toolbar,
};
use super::rpt::writers::TextOptions;
use super::rpt_entries::Which;
use super::rpt_run::Target;
use crate::qa_sweep::Driver;
use crate::views::MainView;
use eframe::egui::{self, Key};
use std::path::{Path, PathBuf};

fn ids(d: &Driver) -> Vec<ReportTabId> {
    d.app.reports.tabs.ids()
}

fn tab(d: &Driver, id: ReportTabId) -> &ReportTab {
    d.app.reports.tabs.get(id).expect("the tab is open")
}

/// Runs the selected report with the toolbar and returns its tab.
fn run_selected(d: &mut Driver) -> ReportTabId {
    let before = ids(d);
    d.click("tool_run_report");
    d.assert_clean("run");
    let new: Vec<ReportTabId> = ids(d).into_iter().filter(|i| !before.contains(i)).collect();
    assert_eq!(new.len(), 1, "one new tab");
    new[0]
}

fn reports_ready() -> Driver {
    let mut d = driver("rt");
    open_view(&mut d);
    show_toolbar(&mut d);
    d
}

/// Where a report's tab is in the tab strip, whichever group has it.
fn dock_tab(d: &Driver, id: ReportTabId) -> egui::Pos2 {
    let plain = format!("dock_tab_report_{}", id.0);
    let key = d
        .app
        .action_rects
        .keys()
        .find(|k| **k == plain || k.starts_with(&format!("{plain}@g")))
        .unwrap_or_else(|| panic!("{plain} is not drawn"));
    d.rect(key).center()
}

/// Debug builds paint a red note over a widget id that two controls share.
fn no_id_clash(d: &mut Driver) {
    d.frames(2);
    let clashes: Vec<String> = d
        .texts()
        .into_iter()
        .map(|(t, _)| t)
        .filter(|t| t.contains("use of widget ID"))
        .collect();
    assert!(clashes.is_empty(), "{clashes:?}");
}

fn minute() -> String {
    chrono::Local::now().format("%Y%m%d%H%M%S").to_string()
}

/// Runs `f` until it finishes inside one second: the reports print the time
/// they were run, to the minute, and a workbook records when it was created,
/// to the second, so two exports only match when they share a second. `f`
/// must not assert on what it produced: a run that crossed a second is
/// thrown away and repeated, so the caller compares the value returned.
fn within_a_minute<T>(mut f: impl FnMut() -> T) -> T {
    loop {
        let start = minute();
        let out = f();
        if start == minute() {
            return out;
        }
    }
}

/// Saves the open tab as `format` to `path` by clicking Export.
fn export_to(d: &mut Driver, format: Export, path: &Path) {
    seam::answer(Some(path.to_path_buf()));
    d.click("report_tab_export");
    d.click(&super::report_tab_ui::export_action(format));
    if format == Export::Text {
        d.click("report_text_ok");
    }
    d.assert_clean("export");
}

#[test]
fn running_a_report_opens_a_tab_and_running_it_again_opens_another() {
    let mut d = reports_ready();
    let model = d.app.project.model.clone();
    select_report(&mut d, "Critical Activities");
    let first = run_selected(&mut d);
    assert_eq!(d.app.views.main, MainView::Report(first));
    assert_eq!(tab(&d, first).caption, "Critical Activities");
    assert!(!tab(&d, first).pages.is_empty());
    // The tab strip names it, and the viewer's controls are on screen.
    assert!(d.has(&format!("dock_tab_report_{}", first.0)));
    for action in [
        "report_tab_export",
        "report_tab_print",
        "report_tab_refresh",
        "report_tab_zoom_box",
        "report_tab_page_box",
        "report_tab_thumbs",
        "report_tab_find",
    ] {
        assert!(d.has(action), "{action} is in the report tab");
    }
    // The header line: report, project, data date, run time, page count.
    let t = tab(&d, first);
    assert!(t.project.contains(&d.app.project.model.project.name));
    assert!(!t.data_date.is_empty() && !t.ran.is_empty());
    let shown: Vec<String> = d.texts().into_iter().map(|(t, _)| t).collect();
    assert!(shown.iter().any(|t| t == "Critical Activities"));
    assert!(shown.iter().any(|t| t.starts_with("Project: ")));
    assert!(shown.iter().any(|t| t.starts_with("Data date: ")));
    assert!(shown.iter().any(|t| t.starts_with("Run: ")));
    assert!(shown.iter().any(|t| t.contains("page")));
    // A double-click on the row runs it again, into a second tab.
    open_view(&mut d);
    let row = report_row(&d, "Critical Activities");
    d.double_click_at(row_point(&d, row));
    let second = ids(&d)
        .into_iter()
        .find(|i| *i != first)
        .expect("a 2nd tab");
    assert_eq!(d.app.views.main, MainView::Report(second));
    assert_eq!(tab(&d, second).caption, "Critical Activities (2)");
    assert_eq!(tab(&d, second).name, "Critical Activities");
    assert_eq!(ids(&d).len(), 2);
    // Closing frees the documents.
    d.click(&format!("report_tab_close_{}", first.0));
    assert!(d.app.reports.tabs.get(first).is_none());
    assert!(!d.app.views.groups.tabs().contains(&MainView::Report(first)));
    assert!(!d.has(&format!("dock_tab_report_{}", first.0)));
    assert_eq!(d.app.views.main, MainView::Report(second));
    d.key_with(Key::F4, egui::Modifiers::COMMAND);
    d.frames(2);
    assert!(ids(&d).is_empty());
    assert!(d
        .app
        .views
        .groups
        .tabs()
        .iter()
        .all(|v| !matches!(v, MainView::Report(_))));
    assert_ne!(d.app.views.main, MainView::Report(second));
    // A run changes nothing in the project.
    assert!(d.app.project.model == model);
    assert!(!d.app.dirty);
    d.assert_clean("tabs");
}

#[test]
fn the_tab_context_menu_closes_tabs_and_splits_the_group() {
    let mut d = reports_ready();
    select_report(&mut d, "Critical Activities");
    let a = run_selected(&mut d);
    open_view(&mut d);
    let b = {
        let row = report_row(&d, "Milestones");
        d.click_at(row_point(&d, row));
        run_selected(&mut d)
    };
    assert_eq!(ids(&d).len(), 2);
    // Right-click the first tab: the menu lists the tab commands.
    let pos = dock_tab(&d, a);
    d.press_at(pos, egui::PointerButton::Secondary, Default::default());
    d.frames(2);
    for item in [
        "report_tab_menu_close",
        "report_tab_menu_close_others",
        "report_tab_menu_close_all",
        "report_tab_menu_horizontal",
        "report_tab_menu_vertical",
        "report_tab_menu_merge",
    ] {
        assert!(d.has(item), "{item} is on the tab menu");
    }
    // New Vertical Tab Group puts this tab in a group of its own.
    let pos = d.rect("report_tab_menu_vertical").center();
    d.click_at(pos);
    d.frames(3);
    assert_eq!(d.app.views.groups.groups.len(), 2);
    assert!(d
        .app
        .views
        .groups
        .groups
        .iter()
        .any(|g| g.tabs == vec![MainView::Report(a)]));
    // Both report tabs draw their viewers: the other group's controls carry
    // its index.
    d.click(&format!("dock_tab_report_{}@g0", b.0));
    assert_eq!(d.app.views.main, MainView::Report(b));
    assert!(d.has("report_tab_export") && d.has("report_tab_export@g1"));
    no_id_clash(&mut d);
    // Close Other Report Tabs from the first one.
    let pos = dock_tab(&d, a);
    d.press_at(pos, egui::PointerButton::Secondary, Default::default());
    d.frames(2);
    let pos = d.rect("report_tab_menu_close_others").center();
    d.click_at(pos);
    d.frames(3);
    assert_eq!(ids(&d), vec![a]);
    assert!(d.app.reports.tabs.get(b).is_none());
    // Close from the menu; the empty group goes away.
    let pos = dock_tab(&d, a);
    d.press_at(pos, egui::PointerButton::Secondary, Default::default());
    d.frames(2);
    let pos = d.rect("report_tab_menu_close").center();
    d.click_at(pos);
    d.frames(3);
    assert!(ids(&d).is_empty());
    assert_eq!(d.app.views.groups.groups.len(), 1);
    d.assert_clean("menu");
}

#[test]
fn tabs_are_not_saved_and_a_new_session_starts_without_them() {
    let mut d = reports_ready();
    select_report(&mut d, "Critical Activities");
    run_selected(&mut d);
    d.app.save_views();
    let json = d.app.views.json();
    assert!(!json.contains("report_tab"));
    let mut fresh = crate::views::Views::default();
    fresh.restore(&json);
    assert!(fresh
        .groups
        .tabs()
        .iter()
        .all(|v| !matches!(v, MainView::Report(_))));
    // A project switch frees the reports run on the old one.
    assert_eq!(ids(&d).len(), 1);
    d.app.project.source_name = "another.xer".into();
    d.frames(3);
    assert!(ids(&d).is_empty(), "the tabs close with their project");
    d.assert_clean("restart");
}

#[test]
fn a_report_tab_works_in_a_split_group() {
    let mut d = reports_ready();
    select_report(&mut d, "Critical Activities");
    let id = run_selected(&mut d);
    d.click("tool_tab_horizontal");
    assert_eq!(d.app.views.groups.groups.len(), 2);
    assert_eq!(d.app.views.groups.current(), MainView::Report(id));
    d.frames(3);
    // The viewer still takes its keys: Page Down in a one-page report
    // leaves the page, the zoom box works, and Ctrl+F opens its Find bar
    // rather than the Find dialog.
    d.click("report_tab_zoom_in");
    assert!(tab(&d, id).zoom.is_some());
    d.key_with(Key::F, egui::Modifiers::COMMAND);
    d.frames(2);
    assert!(tab(&d, id).view.find.open);
    assert!(d.app.top_grid_dialog() != "find", "not the Activities Find");
    d.key(Key::Escape);
    assert!(!tab(&d, id).view.find.open, "Escape closes Find first");
    assert!(ids(&d).len() == 1);
    // Ctrl+F4 closes the tab and its group.
    d.key_with(Key::F4, egui::Modifiers::COMMAND);
    d.frames(2);
    assert!(ids(&d).is_empty());
    assert_eq!(d.app.views.groups.groups.len(), 1);
    d.assert_clean("split");
}

/// The bytes the old path (Run Report to File) writes for the report.
fn old_stored(d: &mut Driver, id: i64, format: Export, text: &TextOptions) -> Vec<u8> {
    let path = d
        .dir
        .join(format!("old-{}.{}", format.key(), format.extension()));
    match format {
        Export::Text => {
            d.app
                .run_stored_report(id, Target::Text, &path, text, "")
                .unwrap();
        }
        _ => {
            let output = match format {
                Export::Pdf => Output::Pdf,
                Export::Excel => Output::Excel,
                Export::Html => Output::Html,
                _ => Output::Csv,
            };
            d.app.write_stored_as(id, output, &path, "").unwrap();
        }
    }
    std::fs::read(&path).unwrap()
}

#[test]
fn exports_of_a_definition_report_equal_the_old_path() {
    let mut d = reports_ready();
    select_report(&mut d, "Critical Activities");
    let Which::Stored(sid) = report_which(&d, "Critical Activities") else {
        panic!("a definition report");
    };
    let tab_id = run_selected(&mut d);
    let default_text = TextOptions {
        delimiter: "\t".into(),
        qualifier: "\"".into(),
    };
    for format in Export::ALL {
        let path = d.dir.join(format!("tab.{}", format.extension()));
        let _ = std::fs::remove_file(&path);
        // Compared once both were written inside one second (a run that
        // crossed a second boundary is repeated, not failed).
        let (new, old) = within_a_minute(|| {
            // Run again so both paths read the same minute.
            d.app.refresh_report_tab(tab_id);
            export_to(&mut d, format, &path);
            let new = std::fs::read(&path).expect("the file is written");
            let old = old_stored(&mut d, sid, format, &default_text);
            (new, old)
        });
        assert!(!new.is_empty());
        assert!(new == old, "{format:?} differs from the old path");
    }
    // The tab stays open and the project is untouched.
    assert_eq!(ids(&d), vec![tab_id]);
    assert!(!d.app.dirty);
}

#[test]
fn exports_of_a_wizard_report_equal_the_old_path() {
    let mut d = reports_ready();
    let mut def = super::wizard::Definition::new(super::wizard::Subject::Resources);
    def.name = "Crew list".into();
    d.app.save_report_definitions(&[def.clone()]);
    d.frames(2);
    select_report(&mut d, "Crew list");
    let tab_id = run_selected(&mut d);
    assert!(matches!(tab(&d, tab_id).doc, Doc::Sheet(_)));
    for format in Export::ALL {
        let path = d.dir.join(format!("crew.{}", format.extension()));
        // Compared once both were written inside one second (a run that
        // crossed a second boundary is repeated, not failed).
        let (new, old) = within_a_minute(|| {
            d.app.refresh_report_tab(tab_id);
            export_to(&mut d, format, &path);
            let new = std::fs::read(&path).unwrap();
            let report = d.app.build_report(&def);
            let old_path = d.dir.join(format!("crew-old.{}", format.extension()));
            let old = match format {
                Export::Text => super::csv::file_bytes(super::csv::write_with(&report, "\t", "\"")),
                _ => {
                    let output = match format {
                        Export::Pdf => Output::Pdf,
                        Export::Excel => Output::Excel,
                        Export::Html => Output::Html,
                        _ => Output::Csv,
                    };
                    doc::write(&report, output, &old_path).unwrap();
                    std::fs::read(&old_path).unwrap()
                }
            };
            (new, old)
        });
        assert_eq!(new, old, "{format:?}");
    }
}

#[test]
fn save_as_offers_a_sensible_name_remembers_the_folder_and_links_the_file() {
    let mut d = reports_ready();
    select_report(&mut d, "Critical Activities");
    let id = run_selected(&mut d);
    let project = d.app.project.model.project.name.clone();
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let folder = d.dir.join("exports");
    std::fs::create_dir_all(&folder).unwrap();
    seam::offered();
    let first = folder.join("one.pdf");
    export_to(&mut d, Export::Pdf, &first);
    let offered = seam::offered();
    assert_eq!(offered.len(), 1);
    let name = &offered[0].1;
    assert_eq!(
        *name,
        default_file_name("Critical Activities", &project, &day, Export::Pdf)
    );
    assert!(name.starts_with("Critical Activities") && name.contains(&project));
    assert!(name.contains(&day) && name.ends_with(".pdf"));
    // The next Save As opens in the folder the last file went to.
    let second = folder.join("two.xlsx");
    export_to(&mut d, Export::Excel, &second);
    let offered = seam::offered();
    assert_eq!(offered[0].0, folder);
    assert!(offered[0].1.ends_with(".xlsx"));
    // Status note with Open and Open Folder.
    assert_eq!(tab(&d, id).saved.as_deref(), Some(second.as_path()));
    assert!(d.has("report_tab_open") && d.has("report_tab_open_folder"));
    assert!(d.app.status.contains("two.xlsx"));
    crate::util::OPENED.with(|o| o.borrow_mut().clear());
    d.click("report_tab_open");
    d.click("report_tab_open_folder");
    let opened = crate::util::OPENED.with(|o| o.borrow().clone());
    assert_eq!(opened, vec![second.clone(), folder.clone()]);
    // A cancelled Save As writes nothing and keeps the last note.
    seam::answer(None);
    d.click("report_tab_export");
    d.click("report_tab_export_html");
    assert_eq!(tab(&d, id).saved.as_deref(), Some(second.as_path()));
    assert!(!folder.join("three.html").exists());
    // An unwritable place says so and leaves the tab alone.
    let missing: PathBuf = d.dir.join("no such folder").join("x.csv");
    seam::answer(Some(missing.clone()));
    d.click("report_tab_export");
    d.click("report_tab_export_csv");
    assert!(d.app.error.is_some(), "the failure is reported");
    d.app.error = None;
    d.frames(2);
    assert!(!missing.exists());
    assert_eq!(tab(&d, id).saved.as_deref(), Some(second.as_path()));
}

#[test]
fn text_export_asks_for_the_delimiter_and_qualifier() {
    let mut d = reports_ready();
    select_report(&mut d, "Critical Activities");
    let id = run_selected(&mut d);
    let path = d.dir.join("pipes.txt");
    seam::offered();
    d.click("report_tab_export");
    d.click("report_tab_export_txt");
    assert!(
        d.app.reports.text_export.is_some(),
        "the options come first"
    );
    assert!(seam::offered().is_empty(), "Save As waits for them");
    // Cancel and Escape write nothing.
    d.click("report_text_cancel");
    assert!(d.app.reports.text_export.is_none());
    d.click("report_tab_export");
    d.click("report_tab_export_txt");
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.reports.text_export.is_none());
    assert!(seam::offered().is_empty());
    // Pipe and single quote.
    d.click("report_tab_export");
    d.click("report_tab_export_txt");
    d.click("report_text_delimiter");
    assert!(d.click_text("Pipe"));
    d.click("report_text_qualifier");
    assert!(d.click_text("Single quote"));
    seam::answer(Some(path.clone()));
    d.click("report_text_ok");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.lines()
            .any(|l| l.starts_with("'RB-") && l.contains("'|'")),
        "{text}"
    );
    assert_eq!(d.app.reports.text_choice, (4, 1));
    assert_eq!(tab(&d, id).saved.as_deref(), Some(path.as_path()));
    d.assert_clean("text");
}

#[test]
fn print_sends_the_pdf_to_the_viewer() {
    let mut d = reports_ready();
    select_report(&mut d, "Critical Activities");
    run_selected(&mut d);
    d.click("report_tab_print");
    d.assert_clean("print");
    assert!(
        d.app.status.contains("sent to the PDF viewer for printing"),
        "{}",
        d.app.status
    );
    d.app.status.clear();
    d.key_with(Key::P, egui::Modifiers::COMMAND);
    d.frames(2);
    assert!(
        d.app.status.contains("sent to the PDF viewer"),
        "Ctrl+P prints the report"
    );
}

fn copied(output: &egui::FullOutput) -> Vec<String> {
    output
        .platform_output
        .commands
        .iter()
        .filter_map(|c| match c {
            egui::OutputCommand::CopyText(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn copy_takes_the_selected_page_or_the_whole_report() {
    let mut d = reports_ready();
    select_report(&mut d, "Critical Activities");
    let id = run_selected(&mut d);
    // Nothing selected: the whole report.
    let out = d.frame(vec![egui::Event::Copy]);
    let all = copied(&out);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0], tab(&d, id).report_text());
    assert!(all[0].contains("Critical Activities"));
    assert!(all[0].contains("RB-"));
    assert!(all[0].contains('\t'), "cells are tab separated");
    // The page strip selects a page: Copy takes that page.
    d.click("report_tab_thumbs");
    assert!(tab(&d, id).view.thumbs);
    let out = d.frame(vec![egui::Event::Copy]);
    let page = copied(&out);
    assert_eq!(page, vec![tab(&d, id).page_text(0)]);
    assert!(d.app.reports.tabs.get(id).unwrap().note.is_some());
}

#[test]
fn refresh_reruns_in_place_and_keeps_the_page_and_the_zoom() {
    let mut d = reports_ready();
    select_report(&mut d, "Critical Activities");
    let id = run_selected(&mut d);
    let before = tab(&d, id).report_text();
    d.click("report_tab_zoom_in");
    let zoom = tab(&d, id).zoom;
    assert!(zoom.is_some());
    // Change the project under the tab: the first critical activity gets a
    // new name. The report does not change until it is refreshed.
    let task = *d
        .app
        .project
        .details
        .iter()
        .find(|(_, v)| !v.name.is_empty() && before.contains(&v.name))
        .expect("a critical activity is listed")
        .0;
    d.app.project.details.get_mut(&task).unwrap().name = "Renamed after the run".into();
    d.frames(2);
    assert_eq!(tab(&d, id).report_text(), before, "the tab is a snapshot");
    d.click("report_tab_refresh");
    d.assert_clean("refresh");
    let after = tab(&d, id).report_text();
    assert_ne!(after, before);
    assert!(after.contains("Renamed after the run"));
    assert_eq!(tab(&d, id).zoom, zoom, "the zoom is kept");
    assert_eq!(ids(&d), vec![id], "refreshed in place, no new tab");
    assert_eq!(tab(&d, id).caption, "Critical Activities");
    // F5 refreshes too.
    d.app.status.clear();
    d.key(Key::F5);
    d.frames(2);
    assert_eq!(tab(&d, id).note.as_deref(), Some("Refreshed"));
}

#[test]
fn refresh_keeps_the_page_of_a_long_report() {
    let mut d = reports_ready();
    // Enough activities for several pages.
    let template = d.app.project.model.activities[0].clone();
    let detail = d.app.project.details[&template.task_id].clone();
    for n in 0..300 {
        let mut a = template.clone();
        a.task_id = 900_000 + n;
        a.task_code = format!("LONG-{n}");
        a.preds.clear();
        a.succs.clear();
        d.app.project.model.activities.push(a);
        let mut detail = detail.clone();
        detail.name = format!("Long report activity {n}");
        d.app.project.details.insert(900_000 + n, detail);
    }
    let mut def = super::wizard::Definition::new(super::wizard::Subject::Activities);
    def.name = "Everything".into();
    d.app.save_report_definitions(&[def]);
    d.frames(2);
    select_report(&mut d, "Everything");
    let id = run_selected(&mut d);
    let pages = tab(&d, id).pages.len();
    assert!(pages >= 3, "{pages} pages");
    d.click("report_tab_last");
    assert_eq!(tab(&d, id).page, pages - 1);
    d.click("report_tab_refresh");
    assert_eq!(tab(&d, id).pages.len(), pages);
    assert_eq!(tab(&d, id).page, pages - 1, "the page is kept");
}

#[test]
fn the_wizard_runs_into_a_tab_and_batches_still_write_files() {
    let mut d = reports_ready();
    // Wizard: Next to the last page, then Finish with a saved name.
    d.click("add");
    for _ in 0..3 {
        d.click("wizard_next");
    }
    d.app.reports.wizard.as_mut().unwrap().save_name = "Tab lookahead".into();
    // Run keeps the wizard open and shows the report in a tab behind it.
    d.click("wizard_run");
    assert!(d.app.reports.wizard.is_some());
    assert_eq!(ids(&d).len(), 1, "Run opens the report in a tab");
    d.click("wizard_finish");
    assert!(d.app.reports.wizard.is_none(), "{:?}", d.app.status);
    let tabs = ids(&d);
    assert_eq!(tabs.len(), 2, "Finish opens the report in another tab");
    assert_eq!(tab(&d, tabs[1]).name, "Tab lookahead");
    assert_eq!(tab(&d, tabs[0]).caption, "Tab lookahead");
    assert_eq!(tab(&d, tabs[1]).caption, "Tab lookahead (2)");
    assert_eq!(d.app.views.main, MainView::Report(tabs[1]));
    // A batch writes its files and opens no tab.
    let batch_dir = d.dir.join("batch");
    std::fs::create_dir_all(&batch_dir).unwrap();
    let id = match report_which(&d, "Critical Activities") {
        Which::Stored(id) => id,
        Which::Wizard(_) | Which::Builtin(_) => unreachable!(),
    };
    let batch = super::rpt::store::Batch {
        id: 0,
        name: "Tabs".into(),
        scope: super::rpt::store::ReportScope::Global,
        reports: vec![id],
    };
    let files = d
        .app
        .run_saved_batch(&batch, &batch_dir, Output::Html, "")
        .unwrap();
    assert_eq!(files.len(), 1);
    assert!(files[0].exists());
    assert_eq!(ids(&d).len(), 2, "no tab for a batch");
    d.assert_clean("wizard");
}

#[test]
fn run_to_file_keeps_working_and_opens_no_tab() {
    let mut d = reports_ready();
    select_report(&mut d, "Critical Activities");
    d.app.reports_command("run_to_file");
    d.frames(2);
    let path = d.dir.join("direct.html");
    super::reports_view_tests::type_into(&mut d, "run_report_path", &path.display().to_string());
    d.click("run_report_run");
    d.assert_clean("file");
    assert!(path.exists());
    assert!(ids(&d).is_empty());
}

#[test]
fn render_a_report_tab_with_the_export_menu_open() {
    use crate::headless_render::SoftwarePainter;
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/r3");
    std::fs::create_dir_all(&out).unwrap();
    let dir = super::test_support::temp("r3-render");
    let (mut app, ctx) = super::test_support::app(&dir);
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let first = app.stored_reports()[2].id;
    app.run_report_in_tab(&Which::Stored(first), "").unwrap();
    // A second tab, and the Export menu open on the first.
    app.run_report_in_tab(&Which::Stored(first), "").unwrap();
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("report-tab.png")).unwrap();
    painter.click_action(&mut app, &ctx, &mut time, "report_tab_export");
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    assert!(app.action_rects.contains_key("report_tab_export_pdf"));
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("report-tab-export-menu.png")).unwrap();
    // After saving: the status line offers Open and Open Folder.
    let path = dir.join("shot.pdf");
    seam::answer(Some(path));
    painter.click_action(&mut app, &ctx, &mut time, "report_tab_export_pdf");
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("report-tab-saved.png")).unwrap();
    // Two groups: the report beside the Activities view.
    painter.click_action(&mut app, &ctx, &mut time, "tool_tab_vertical");
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("report-tab-split.png")).unwrap();
    assert!(app.error.is_none(), "{:?}", app.error);
}
