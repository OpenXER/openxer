//! Right-click menus of the Projects, Resources and record views, the
//! Units & Prices and Roles grids, the Roles dialog, the toolbar popup and
//! the Gantt timescale and Bars / Columns rules, through the production UI.
//! egui `RawInput` only; no native window or system pointer.
use super::*;
use crate::qa_sweep::Driver;

fn starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn right_click(d: &mut Driver, pos: Pos2) {
    d.press_at(pos, egui::PointerButton::Secondary, Default::default());
}

/// Whether the item runs when clicked: an enabled item closes the menu, a
/// disabled one leaves it open. The menu must be open.
fn closes_on_click(d: &mut Driver, probe: &str, item: &str) -> bool {
    let pos = d.rect(item).center();
    d.press_at(pos, egui::PointerButton::Primary, Default::default());
    !d.has(probe)
}

/// A point in the row `index` of the view table `id`, clear of the expander.
fn row_point(d: &Driver, id: &str, index: usize) -> Pos2 {
    let r = d.rect(&format!("{id}_row_{index}"));
    Pos2::new(r.left() + 90., r.center().y)
}

/// A point under the last row of view table `id`.
fn below_rows(d: &Driver, id: &str) -> Pos2 {
    let rows = d
        .app
        .action_rects
        .keys()
        .filter(|k| k.starts_with(&format!("{id}_row_")))
        .count();
    let last = d.rect(&format!("{id}_row_{}", rows - 1));
    // Under the rows, and above the columns' scroll bar when there is one.
    let mut y = last.bottom() + 60.;
    let bar = format!("{id}_columns_hscroll");
    if d.has(&bar) {
        y = y.min(d.rect(&bar).top() - 4.);
    }
    Pos2::new(last.left() + 90., y)
}

fn assert_order(d: &Driver, keys: &[&str]) {
    for pair in keys.windows(2) {
        assert!(
            d.rect(pair[0]).top() < d.rect(pair[1]).top(),
            "{} comes before {}",
            pair[0],
            pair[1]
        );
    }
}

fn rows(d: &Driver, table: &str) -> usize {
    d.app
        .project
        .document
        .table(table)
        .map_or(0, |t| t.rows.len())
}

// -------------------------------------------------------------- projects

fn open_projects(d: &mut Driver) {
    d.click("directory_projects");
    d.frames(2);
}

/// Right-clicks rows until one is a project row; its index.
fn right_click_project_row(d: &mut Driver) -> usize {
    let count = d
        .app
        .action_rects
        .keys()
        .filter(|k| k.starts_with("projects_row_"))
        .count();
    for i in 0..count {
        d.app.views.projects.selected = None;
        let at = row_point(d, "projects", i);
        right_click(d, at);
        if d.app.views.projects.selected.is_some() {
            return i;
        }
        d.key(Key::Escape);
    }
    panic!("no project row among {count} rows");
}

#[test]
fn the_projects_menu_lists_its_items_in_order_and_selects_the_row() {
    let mut d = Driver::new("vm-projects", starter());
    open_projects(&mut d);
    right_click_project_row(&mut d);
    assert!(
        d.app.views.projects.selected.is_some(),
        "the right-click selected the unselected project row"
    );
    let order = [
        "projects_menu_open_project",
        "projects_menu_details",
        "projects_menu_edit_details",
        "projects_menu_cut",
        "projects_menu_copy",
        "projects_menu_paste",
        "projects_menu_fill_down",
        "projects_menu_add",
        "projects_menu_delete",
        "projects_menu_columns",
        "projects_menu_group_by",
    ];
    for key in order {
        assert!(d.has(key), "{key} is listed");
    }
    assert_order(&d, &order);
    // The unbuilt layout, chart, summary and reflection commands were
    // removed (2026-11-07 cleanup).
    for key in [
        "projects_menu_layout",
        "projects_menu_bars",
        "projects_menu_timescale",
        "projects_menu_reorganize",
        "projects_menu_summarize",
        "projects_menu_delete_summaries",
        "projects_menu_reflection",
        "projects_menu_merge_source",
        "projects_menu_launch",
        "projects_menu_show_bottom",
        "projects_menu_hint_help",
    ] {
        assert!(!d.has(key), "{key} is gone");
    }
    assert!(d.texts().iter().any(|(t, _)| t == "Edit Project Details"));
    // No integration caption anywhere in the menu.
    for (text, _) in d.texts() {
        for banned in ["Send to", "Unifier", "Cloud", "Exchange Data"] {
            assert!(!text.contains(banned), "{text:?}");
        }
    }
    // Paste has nothing to paste yet.
    assert!(!closes_on_click(
        &mut d,
        "projects_menu_open_project",
        "projects_menu_paste"
    ));
}

#[test]
fn the_projects_menu_runs_its_commands() {
    let mut d = Driver::new("vm-projects-run", starter());
    open_projects(&mut d);
    right_click_project_row(&mut d);
    // Copy then Paste on the EPS node is offered.
    assert!(closes_on_click(
        &mut d,
        "projects_menu_open_project",
        "projects_menu_copy"
    ));
    assert!(
        d.app.views.projects.copy.is_some(),
        "Copy marks the project"
    );
    d.frames(2);
    right_click_project_row(&mut d);
    assert!(
        d.has("projects_menu_paste"),
        "Paste is listed with a copy waiting"
    );
    // Project Details toggles the pane; No Bottom Layout hides it.
    let shown = d.app.views.projects.show_details;
    assert!(closes_on_click(
        &mut d,
        "projects_menu_open_project",
        "projects_menu_details"
    ));
    assert_eq!(d.app.views.projects.show_details, !shown);
    // Group and Sort By > Status groups the table by status.
    right_click_project_row(&mut d);
    let hover = d.rect("projects_menu_group_by").center();
    let _ = d.frame(vec![egui::Event::PointerMoved(hover)]);
    d.frames(3);
    assert!(d.has("projects_menu_group_status"), "the submenu opens");
    assert!(closes_on_click(
        &mut d,
        "projects_menu_open_project",
        "projects_menu_group_status"
    ));
    assert!(d.app.views.projects.group == project_rollups::Group::Status);
    d.app.views.projects.group = project_rollups::Group::Eps;
    d.frames(2);
    // Edit Project Details shows the pane on the General tab.
    d.app.views.projects.tab = 3;
    right_click_project_row(&mut d);
    assert!(closes_on_click(
        &mut d,
        "projects_menu_open_project",
        "projects_menu_edit_details"
    ));
    assert!(d.app.views.projects.show_details);
    assert_eq!(d.app.views.projects.tab, 0);
}

#[test]
fn the_projects_menu_opens_a_project_and_serves_the_empty_area() {
    let mut d = Driver::new("vm-projects-open", starter());
    open_projects(&mut d);
    right_click_project_row(&mut d);
    assert!(closes_on_click(
        &mut d,
        "projects_menu_open_project",
        "projects_menu_open_project"
    ));
    d.frames(2);
    // Opening a project switches to its Activities view; back to Projects.
    open_projects(&mut d);
    // Under the last row: Add applies, the project commands do not.
    let at = below_rows(&d, "projects");
    right_click(&mut d, at);
    assert!(d.has("projects_menu_add"), "the empty area has a menu");
    for key in [
        "projects_menu_open_project",
        "projects_menu_cut",
        "projects_menu_copy",
        "projects_menu_delete",
    ] {
        assert!(
            !closes_on_click(&mut d, "projects_menu_add", key),
            "{key} needs a project"
        );
    }
}

// ------------------------------------------------------------- resources

fn with_resources() -> Driver {
    let mut d = Driver::new("vm-resources", resources::tests::with_resource());
    d.click("directory_resources");
    d.app.views.resources.assigned_only = false;
    d.frames(3);
    d
}

#[test]
fn the_resources_menu_selects_the_row_and_runs_resource_commands() {
    let mut d = with_resources();
    // Row 0 is PIPE, row 1 its child, row 2 IDLE (tree order by sequence).
    d.app.views.resources.selected = None;
    let at = row_point(&d, "resources", 1);
    right_click(&mut d, at);
    assert!(d.app.views.resources.selected.is_some(), "the row selects");
    let order = [
        "resources_menu_details",
        "resources_menu_cut",
        "resources_menu_copy",
        "resources_menu_paste",
        "resources_menu_fill_down",
        "resources_menu_add",
        "resources_menu_delete",
        "resources_menu_del_merge",
        "resources_menu_columns_menu",
        "resources_menu_filter_by",
        "resources_menu_group_by",
        "resources_menu_expand_all",
        "resources_menu_collapse_all",
    ];
    for key in order {
        assert!(d.has(key), "{key} is listed");
    }
    assert_order(&d, &order);
    // Paste is disabled with an empty clipboard; Del. / Merge opens its
    // dialog (resource_merge_tests covers it).
    assert!(!closes_on_click(
        &mut d,
        "resources_menu_add",
        "resources_menu_paste"
    ));
    // Add: one resource, one undo step.
    let (before, steps) = (rows(&d, "RSRC"), d.app.history.counts().0);
    assert!(closes_on_click(
        &mut d,
        "resources_menu_add",
        "resources_menu_add"
    ));
    d.frames(2);
    assert_eq!(rows(&d, "RSRC"), before + 1);
    assert_eq!(d.app.history.counts().0, steps + 1, "Add is one undo step");
    // Copy marks the clipboard and is not an edit.
    let at = row_point(&d, "resources", 0);
    right_click(&mut d, at);
    assert!(closes_on_click(
        &mut d,
        "resources_menu_add",
        "resources_menu_copy"
    ));
    assert!(!d.app.views.resources.clipboard.is_empty());
    assert_eq!(d.app.history.counts().0, steps + 1, "Copy changes nothing");
    // Paste: one more resource, one more undo step.
    right_click(&mut d, at);
    assert!(closes_on_click(
        &mut d,
        "resources_menu_add",
        "resources_menu_paste"
    ));
    d.frames(2);
    assert_eq!(rows(&d, "RSRC"), before + 2);
    assert_eq!(
        d.app.history.counts().0,
        steps + 2,
        "Paste is one undo step"
    );
}

#[test]
fn cut_and_paste_move_a_resource_under_the_selected_ones_parent() {
    let mut d = with_resources();
    // PIPE.CRANE (8) is a child of PIPE (7); IDLE (9) is at the top.
    d.app.views.resources.selected = Some(8);
    d.app.resource_command("Cut", 0.);
    assert_eq!(d.app.views.resources.cut, [8]);
    assert!(d.app.views.resources.clipboard.is_empty());
    assert_eq!(rows(&d, "RSRC"), 3, "Cut copies nothing");
    let steps = d.app.history.counts().0;
    d.app.views.resources.selected = Some(9);
    d.app.resource_command("Paste", 1.);
    let parent = resource_edit::value(&d.app.project.document, 8, "parent_rsrc_id");
    assert_eq!(parent, "", "the cut resource moved to the top level");
    assert_eq!(rows(&d, "RSRC"), 3, "Paste after Cut moves, never copies");
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    assert!(d.app.views.resources.cut.is_empty());
    // Pasting a resource under its own child is refused and changes nothing.
    d.app.views.resources.selected = Some(7);
    d.app.resource_command("Cut", 2.);
    d.app.views.resources.selected = Some(9);
    d.app.resource_command("Copy", 3.);
    assert!(d.app.views.resources.cut.is_empty(), "Copy replaces Cut");
}

#[test]
fn moving_a_resource_refuses_cycles() {
    let mut p = resources::tests::with_resource();
    // 8 is under 7: moving 7 under 8 would loop.
    let doc = &mut p.document;
    let err = resource_edit::move_resource(doc, 7, Some(8)).unwrap_err();
    assert!(err.contains("under itself"), "{err}");
    assert!(resource_edit::move_resource(doc, 7, Some(7)).is_err());
    assert_eq!(resource_edit::value(doc, 8, "parent_rsrc_id"), "7");
    resource_edit::move_resource(doc, 9, Some(7)).unwrap();
    assert_eq!(resource_edit::value(doc, 9, "parent_rsrc_id"), "7");
    resource_edit::move_resource(doc, 9, None).unwrap();
    assert_eq!(resource_edit::value(doc, 9, "parent_rsrc_id"), "");
}

#[test]
fn the_dock_cut_button_is_enabled_in_the_resources_view() {
    let mut d = with_resources();
    d.app.views.resources.selected = Some(9);
    d.frames(2);
    d.click("dock_cut");
    assert_eq!(d.app.views.resources.cut, [9]);
}

#[test]
fn the_resources_menu_switches_the_bottom_pane_and_the_display() {
    let mut d = with_resources();
    let at = row_point(&d, "resources", 0);
    // Details, a check item at the top, shows and hides Resource Details.
    for details in [
        !d.app.views.resources.show_bottom,
        d.app.views.resources.show_bottom,
    ] {
        right_click(&mut d, at);
        assert!(closes_on_click(
            &mut d,
            "resources_menu_add",
            "resources_menu_details"
        ));
        assert_eq!(d.app.views.resources.show_bottom, details);
        d.frames(2);
    }
    for key in [
        "resources_menu_show_bottom",
        "resources_menu_display",
        "resources_menu_hint_help",
    ] {
        assert!(!d.has(key), "{key} is gone");
    }
    // A double-click still opens the details.
    d.app.views.resources.show_bottom = false;
    d.frames(2);
    d.double_click_at(at);
    d.frames(2);
    assert!(d.app.views.resources.show_bottom);
    // Filter By carries the two choices.
    right_click(&mut d, at);
    let hover = d.rect("resources_menu_filter_by").center();
    let _ = d.frame(vec![egui::Event::PointerMoved(hover)]);
    d.frames(3);
    assert!(closes_on_click(
        &mut d,
        "resources_menu_add",
        "resources_menu_display_project"
    ));
    assert!(d.app.views.resources.assigned_only);
}

#[test]
fn units_and_prices_rows_have_add_and_delete() {
    let mut d = with_resources();
    d.app.views.resources.selected = Some(7);
    d.app.views.resources.tab = 3;
    d.frames(3);
    let rates = rows(&d, "RSRCRATE");
    assert_eq!(rates, 2);
    let first = d.rect("resource_rates_row_0");
    right_click(&mut d, first.center());
    for key in [
        "resource_rates_menu_add",
        "resource_rates_menu_delete",
        "resource_rates_menu_customize",
    ] {
        assert!(d.has(key), "{key} is listed");
    }
    // Customize Resource Rate Columns opens the shared column customizer.
    assert!(closes_on_click(
        &mut d,
        "resource_rates_menu_add",
        "resource_rates_menu_customize"
    ));
    d.frames(2);
    assert!(d.app.detail.column_dialog.is_some());
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.detail.column_dialog.is_none());
    right_click(&mut d, first.center());
    assert!(closes_on_click(
        &mut d,
        "resource_rates_menu_add",
        "resource_rates_menu_add"
    ));
    d.frames(2);
    assert_eq!(rows(&d, "RSRCRATE"), rates + 1);
    // Delete the row that was right-clicked.
    let first = d.rect("resource_rates_row_0");
    right_click(&mut d, first.center());
    assert!(closes_on_click(
        &mut d,
        "resource_rates_menu_add",
        "resource_rates_menu_delete"
    ));
    d.frames(2);
    assert_eq!(rows(&d, "RSRCRATE"), rates);
}

#[test]
fn the_roles_grid_menu_assigns_and_removes() {
    let mut p = resources::tests::with_resource();
    {
        let doc = &mut p.document;
        doc.table_mut("ROLES").add(&[
            ("role_id", "1".into()),
            ("role_short_name", "FITTER".into()),
            ("role_name", "Fitter".into()),
        ]);
        doc.table_mut("RSRCROLE").add(&[
            ("rsrc_id", "7".into()),
            ("role_id", "1".into()),
            ("skill_level", "1".into()),
        ]);
    }
    let p = ProjectData::from_document(p.document.clone(), 1, "roles".into()).unwrap();
    let mut d = Driver::new("vm-roles-grid", p);
    d.click("directory_resources");
    d.app.views.resources.selected = Some(7);
    d.app.views.resources.tab = 4;
    d.frames(3);
    let row = d.rect("resource_roles_row_0");
    right_click(&mut d, row.center());
    assert!(d.has("resource_roles_menu_assign"));
    assert_order(
        &d,
        &["resource_roles_menu_assign", "resource_roles_menu_remove"],
    );
    let before = rows(&d, "RSRCROLE");
    assert!(closes_on_click(
        &mut d,
        "resource_roles_menu_assign",
        "resource_roles_menu_remove"
    ));
    d.frames(2);
    assert_eq!(rows(&d, "RSRCROLE"), before - 1);
}

// ----------------------------------------------------------- record views

/// A point in the empty table area of a view that has no rows: under the
/// header strip and the column titles.
fn empty_area(d: &Driver, id: &str) -> Pos2 {
    let strip = d.rect(&format!("{id}_layout"));
    Pos2::new(strip.left() + 90., strip.bottom() + 70.)
}

fn set_document_location(d: &mut Driver, id: i64, column: &str, text: &str) {
    let ok = record_view::set_value(
        &mut d.app.project.document,
        "DOCUMENT",
        "doc_id",
        id,
        column,
        text.into(),
    );
    assert!(ok);
}

#[test]
fn the_documents_menu_adds_assigns_removes_and_launches() {
    let mut d = Driver::new("vm-docs", starter());
    d.click("dock_wps_docs");
    d.frames(3);
    assert_eq!(rows(&d, "DOCUMENT"), 0);
    // No row yet: the empty area has the menu. Add applies; Delete, the
    // assignment items and Remove do not.
    let at = empty_area(&d, "documents");
    right_click(&mut d, at);
    assert!(d.has("documents_menu_add"), "the empty list has a menu");
    for key in [
        "documents_menu_delete",
        "documents_menu_assign_activity",
        "documents_menu_assign_wbs",
        "documents_menu_remove",
    ] {
        assert!(!closes_on_click(&mut d, "documents_menu_add", key), "{key}");
    }
    assert!(closes_on_click(
        &mut d,
        "documents_menu_add",
        "documents_menu_add"
    ));
    d.frames(2);
    assert_eq!(rows(&d, "DOCUMENT"), 1, "Add creates a document row");
    let id = d.app.views.records.documents.selected.expect("selected");
    // On the new row: assignment items apply, Remove still has nothing to
    // remove, Launch has no file to open.
    let at = row_point(&d, "documents", 0);
    right_click(&mut d, at);
    let order = [
        "documents_menu_add",
        "documents_menu_delete",
        "documents_menu_assign_activity",
        "documents_menu_assign_wbs",
        "documents_menu_remove",
        "documents_menu_launch",
        "documents_menu_details",
        "documents_menu_hide_details",
        "documents_menu_customize",
    ];
    for key in order {
        assert!(d.has(key), "{key} is listed");
    }
    assert_order(&d, &order);
    assert!(!closes_on_click(
        &mut d,
        "documents_menu_add",
        "documents_menu_remove"
    ));
    assert!(closes_on_click(
        &mut d,
        "documents_menu_add",
        "documents_menu_assign_activity"
    ));
    assert!(d.app.views.records.picker.is_some(), "the picker opens");
    d.app.views.records.picker = None;
    d.frames(2);
    // An assignment selected on the Assignments tab can be removed.
    let task = d.app.project.model.activities[0].task_id;
    assert!(documents::assign(&mut d.app.project, id, Some(task), None));
    d.app.touch_records();
    d.app.views.records.documents.sub_row = Some(0);
    d.frames(2);
    assert_eq!(rows(&d, "TASKDOC"), 1);
    let at = row_point(&d, "documents", 0);
    right_click(&mut d, at);
    assert!(closes_on_click(
        &mut d,
        "documents_menu_add",
        "documents_menu_remove"
    ));
    d.frames(2);
    assert_eq!(rows(&d, "TASKDOC"), 0, "Remove unassigns the row");
}

/// A scratch folder with the files the Launch tests point at.
struct Files {
    dir: std::path::PathBuf,
}

impl Files {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "openxer-launch-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for file in [
            "plan.pdf",
            "run.exe",
            "link.lnk",
            "bundle.zip",
            "Sheet.XLSX",
        ] {
            std::fs::write(dir.join(file), b"x").unwrap();
        }
        std::fs::create_dir_all(dir.join("folder.pdf")).unwrap();
        Self { dir }
    }
    fn path(&self, name: &str) -> String {
        self.dir.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn launch_target_allows_documents_and_refuses_every_other_form() {
    let f = Files::new("rules");
    // A document on a drive passes, in its canonical form, any case.
    let ok = documents::launch_target(&f.path("plan.pdf")).unwrap();
    assert!(ok.ends_with("plan.pdf") && !ok.starts_with("\\\\"), "{ok}");
    let ok = documents::launch_target(&f.path("Sheet.XLSX")).unwrap();
    assert!(ok.to_lowercase().ends_with("sheet.xlsx"));
    let dir = f.path("");
    let refused = |location: &str, why: &str| {
        let e = documents::launch_target(location).unwrap_err();
        assert!(e.contains(why), "{location:?}: {e}");
    };
    // Network, device, address, stream and control forms.
    refused("\\\\server\\share\\plan.pdf", "UNC");
    refused("//server/share/plan.pdf", "UNC");
    refused("\\\\?\\C:\\plan.pdf", "device");
    refused("\\\\.\\C:\\plan.pdf", "device");
    refused("https://example.com/plan.pdf", "address");
    refused("file:///C:/plan.pdf", "address");
    refused("file:C:\\plan.pdf", "address");
    refused("mailto:someone@example.com", "address");
    #[cfg(windows)]
    refused(&format!("{}:evil.exe", f.path("plan.pdf")), "stream");
    #[cfg(windows)]
    refused(&format!("{}::$DATA", f.path("plan.pdf")), "stream");
    refused(&format!("{}\u{1}x.pdf", dir), "control");
    refused("plan.pdf", "absolute");
    refused("C:plan.pdf", "absolute");
    refused("..\\plan.pdf", "absolute");
    refused("", "no location");
    // Types that are not documents, whatever the file is.
    refused(&f.path("run.exe"), "type");
    refused(&f.path("link.lnk"), "type");
    refused(&f.path("bundle.zip"), "type");
    refused(&f.path("plan"), "type");
    // A directory named like a document, and a file that is not there.
    refused(&f.path("folder.pdf"), "regular file");
    refused(&f.path("missing.pdf"), "does not exist");
    // A real file's name with a trailing dot or space, or an extension
    // spoofed that way, is not the name that was checked (Windows drops
    // them).
    #[cfg(windows)]
    {
        refused(&format!("{}.", f.path("plan.pdf")), "dot or a space");
        refused(&format!("{} ", f.path("plan.pdf")), "dot or a space");
        refused(&format!("{}.", f.path("run.exe")), "dot or a space");
        refused(&format!("{} ", f.path("run.exe")), "dot or a space");
        refused(&format!("{}.pdf.", f.path("run.exe")), "dot or a space");
    }
    // Elsewhere a colon is part of the name, and devices are refused.
    #[cfg(not(windows))]
    {
        refused(&format!("{}:evil.exe", f.path("plan.pdf")), "type");
        std::os::unix::fs::symlink("/dev/null", f.path("null.pdf")).unwrap();
        refused(&f.path("null.pdf"), "device");
        refused("C:\\plan.pdf", "absolute");
    }
}

/// Right-clicks the document row and launches its private or public file.
fn launch_from_menu(d: &mut Driver, key: &str) {
    let at = row_point(d, "documents", 0);
    right_click(d, at);
    let hover = d.rect("documents_menu_launch").center();
    let _ = d.frame(vec![egui::Event::PointerMoved(hover)]);
    d.frames(3);
    assert!(closes_on_click(d, "documents_menu_add", key));
    d.frames(3);
}

#[test]
fn launch_asks_first_and_opens_only_on_open() {
    let f = Files::new("ui");
    let mut d = Driver::new("vm-launch", starter());
    d.click("dock_wps_docs");
    d.frames(3);
    d.app.document_command_add();
    let id = d.app.views.records.documents.selected.expect("selected");
    set_document_location(&mut d, id, "private_loc", &f.path("plan.pdf"));
    set_document_location(&mut d, id, "public_loc", &f.path("run.exe"));
    d.app.touch_records();
    d.frames(3);
    d.app.status = "before".into();
    // An allowed document: a confirmation with the whole path, nothing
    // opened yet.
    launch_from_menu(&mut d, "documents_menu_launch_private");
    assert!(d.app.views.records.launch.is_some(), "the box is up");
    assert_eq!(d.app.status, "before", "nothing opened before Open");
    let shown = d.app.views.records.launch.clone().unwrap();
    assert!(
        d.texts().iter().any(|(t, _)| t.contains(&shown)),
        "the box shows the full path {shown}"
    );
    assert!(d.has("launch_open") && d.has("launch_cancel"));
    // Enter does not confirm; Cancel and Escape do nothing.
    d.key(Key::Enter);
    assert!(d.app.views.records.launch.is_some());
    assert_eq!(d.app.status, "before");
    d.click("launch_cancel");
    d.frames(2);
    assert!(d.app.views.records.launch.is_none());
    assert_eq!(d.app.status, "before", "Cancel opens nothing");
    launch_from_menu(&mut d, "documents_menu_launch_private");
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.views.records.launch.is_none());
    assert_eq!(d.app.status, "before", "Escape opens nothing");
    // Open runs the (test) opener on the canonical path.
    launch_from_menu(&mut d, "documents_menu_launch_private");
    d.click("launch_open");
    d.frames(2);
    assert!(d.app.views.records.launch.is_none());
    assert_eq!(d.app.status, format!("Opened {shown}"));
    // A program is refused with the reason and no box.
    d.app.status = "before".into();
    launch_from_menu(&mut d, "documents_menu_launch_public");
    assert!(d.app.views.records.launch.is_none(), "no box for a program");
    assert!(d.app.status.starts_with("Not opened:"), "{}", d.app.status);
    assert!(d.app.status.contains("type"), "{}", d.app.status);
}

#[test]
fn launch_checks_again_when_open_is_pressed() {
    let f = Files::new("again");
    let mut d = Driver::new("vm-launch-again", starter());
    d.click("dock_wps_docs");
    d.frames(3);
    d.app.document_command_add();
    let id = d.app.views.records.documents.selected.expect("selected");
    set_document_location(&mut d, id, "private_loc", &f.path("plan.pdf"));
    d.app.touch_records();
    d.frames(3);
    launch_from_menu(&mut d, "documents_menu_launch_private");
    assert!(d.app.views.records.launch.is_some());
    // The file goes away while the box is up.
    std::fs::remove_file(f.dir.join("plan.pdf")).unwrap();
    d.click("launch_open");
    d.frames(2);
    assert!(d.app.status.starts_with("Not opened:"), "{}", d.app.status);
}

#[test]
fn the_other_record_views_have_add_delete_details_and_customize() {
    for (name, dock, id, table) in [
        ("risks", "dock_risks", "risks", "RISK"),
        ("expenses", "dock_expenses", "expenses", "PROJCOST"),
    ] {
        let mut d = Driver::new(&format!("vm-{name}"), starter());
        d.click(dock);
        d.frames(3);
        let at = if d.has(&format!("{id}_row_0")) {
            row_point(&d, id, 0)
        } else {
            empty_area(&d, id)
        };
        right_click(&mut d, at);
        let add = format!("{id}_menu_add");
        for key in ["add", "delete", "details", "hide_details", "customize"] {
            assert!(d.has(&format!("{id}_menu_{key}")), "{name}: {key}");
        }
        assert!(
            closes_on_click(&mut d, &add, &format!("{id}_menu_customize")),
            "{name}: Customize Columns runs"
        );
        d.frames(2);
        assert!(d.app.detail.column_dialog.is_some(), "{name}: it opens");
        d.key(Key::Escape);
        d.frames(2);
        assert!(d.app.detail.column_dialog.is_none());
        right_click(&mut d, at);
        if name == "risks" {
            let before = rows(&d, table).max(rows(&d, "RISK"));
            assert!(closes_on_click(&mut d, &add, &add));
            d.frames(2);
            assert!(d.app.views.records.risks.selected.is_some());
            let _ = before;
        }
    }
}

// ------------------------------------------------------------ Roles list

#[test]
fn the_roles_dialog_list_has_a_right_click_menu() {
    let mut p = starter();
    p.document.table_mut("ROLES").add(&[
        ("role_id", "1".into()),
        ("role_short_name", "FITTER".into()),
        ("role_name", "Fitter".into()),
        ("parent_role_id", String::new()),
        ("seq_num", "1".into()),
    ]);
    let p = ProjectData::from_document(p.document.clone(), 1, "roles".into()).unwrap();
    let mut d = Driver::new("vm-roles-dialog", p);
    d.click("menu_roles");
    d.frames(4);
    assert!(d.has("roles_list_row_0"), "the role is listed");
    d.app.enterprise.clip = None;
    let row = d.rect("roles_list_row_0");
    right_click(&mut d, Pos2::new(row.left() + 40., row.center().y));
    let order = [
        "roles_menu_add",
        "roles_menu_delete",
        "roles_menu_cut",
        "roles_menu_copy",
        "roles_menu_paste",
        "roles_menu_details",
    ];
    for key in order {
        assert!(d.has(key), "{key} is listed");
    }
    assert_order(&d, &order);
    assert!(!closes_on_click(
        &mut d,
        "roles_menu_add",
        "roles_menu_paste"
    ));
    assert!(closes_on_click(&mut d, "roles_menu_add", "roles_menu_copy"));
    assert!(d.app.enterprise.clip.is_some(), "Copy fills the clipboard");
    d.frames(2);
    // Add runs the dialog's own Add: one more role.
    let row = d.rect("roles_list_row_0");
    right_click(&mut d, Pos2::new(row.left() + 40., row.center().y));
    let roles = rows(&d, "ROLES");
    assert!(closes_on_click(&mut d, "roles_menu_add", "roles_menu_add"));
    d.frames(4);
    assert_eq!(rows(&d, "ROLES"), roles + 1);
    d.key(Key::Escape);
}

// ---------------------------------------------------------------- chrome

#[test]
fn the_toolbar_popup_matches_view_toolbars() {
    let mut d = Driver::new("vm-chrome", starter());
    // An empty spot of the toolbar rows.
    let empty = Pos2::new(1380., crate::menu::MENU_ROW + 10.);
    right_click(&mut d, empty);
    assert!(d.has("toolbar_popup_toolbar_Print"), "the popup is open");
    for name in crate::menu::TOOLBAR_LIST {
        assert!(d.has(&format!("toolbar_popup_toolbar_{name}")), "{name}");
    }
    assert!(d.has("toolbar_popup_customize"));
    // Status Bar is in the View menu, not in this popup (as the reference).
    assert!(!d.has("toolbar_popup_status_bar"));
    // Navigation Bar and Activity Toolbar have no counterpart here.
    for (text, _) in d.texts() {
        assert!(!text.contains("Navigation Bar"), "{text}");
        assert!(!text.contains("Activity Toolbar"), "{text}");
    }
    // Unchecking Print hides it, as View > Toolbars does.
    assert!(d.app.toolbar_visible("Print"));
    assert!(closes_on_click(
        &mut d,
        "toolbar_popup_toolbar_Print",
        "toolbar_popup_toolbar_Print"
    ));
    d.frames(2);
    assert!(!d.app.toolbar_visible("Print"));
    assert!(!d.has("tool_print"), "the Print bar is gone");
    // View > Toolbars shows the same state.
    d.click("menu_bar_view");
    let toolbars = d.hover_text("Toolbars");
    assert!(toolbars);
    d.frames(2);
    assert!(d.has("menu_toolbar_Print"));
    d.key(Key::Escape);
    d.key(Key::Escape);
    d.frames(2);
    // Customize... opens the toolbar dialog.
    right_click(&mut d, empty);
    assert!(closes_on_click(
        &mut d,
        "toolbar_popup_customize",
        "toolbar_popup_customize"
    ));
    assert!(d.app.views.customize.is_some());
}

#[test]
fn the_toolbar_popup_opens_over_a_button_and_over_the_side_docks() {
    let mut d = Driver::new("vm-chrome-button", starter());
    let button = d.rect("tool_table").center();
    right_click(&mut d, button);
    assert!(d.has("toolbar_popup_customize"), "over a toolbar button");
    d.key(Key::Escape);
    d.frames(2);
    assert!(!d.has("toolbar_popup_customize"), "Escape closes it");
    let dock = d.rect("new");
    right_click(&mut d, Pos2::new(dock.center().x, 700.));
    assert!(d.has("toolbar_popup_customize"), "over the left dock");
    d.key(Key::Escape);
}

// ---------------------------------------------------------------- Gantt

#[test]
fn the_timescale_bands_open_the_chart_menu() {
    let mut d = Driver::new("vm-timescale", starter());
    // Both bands give the one chart menu (no Zoom items of their own: View
    // > Zoom has them), as the reference does.
    for band in ["timescale_zoom", "timescale_pan"] {
        let at = d.rect(band).center();
        right_click(&mut d, at);
        let order = [
            "gantt_menu_details",
            "gantt_menu_bars",
            "gantt_menu_options",
            "gantt_menu_timescale",
            "gantt_menu_attachments",
        ];
        for key in order {
            assert!(d.has(key), "{band}: {key} is listed");
        }
        assert_order(&d, &order);
        assert!(!d.has("timescale_menu_zoom_in"), "no separate menu");
        d.key(Key::Escape);
        d.frames(2);
    }
    let at = d.rect("timescale_pan").center();
    right_click(&mut d, at);
    assert!(closes_on_click(
        &mut d,
        "gantt_menu_timescale",
        "gantt_menu_timescale"
    ));
    assert!(d.app.gantt_ui.timescale.is_some(), "Timescale... opens");
}

#[test]
fn bars_and_columns_follow_the_views_on_screen() {
    let mut d = Driver::new("vm-bars-columns", starter());
    assert!(d.app.gantt_on_screen() && d.app.table_on_screen());
    // The table alone has no Gantt chart: Bars is off, Columns stays on.
    d.app.set_top(panes::TopView::Table);
    d.frames(3);
    assert!(!d.app.gantt_on_screen());
    assert!(d.app.table_on_screen());
    d.click("tool_bars");
    assert!(d.app.gantt_ui.bars.is_none(), "Bars... does not open");
    d.click("tool_columns");
    d.frames(2);
    assert!(d.app.grid.dialogs.columns.is_some(), "Columns... opens");
    d.key(Key::Escape);
    d.frames(2);
    // The network has no table: Columns is off.
    d.app.set_top(panes::TopView::Network);
    d.frames(3);
    assert!(!d.app.table_on_screen());
    d.click("tool_columns");
    d.frames(2);
    assert!(d.app.grid.dialogs.columns.is_none());
    // A Gantt chart at the bottom brings Bars back.
    d.app.set_top(panes::TopView::Table);
    d.app.set_bottom(Some(panes::BottomView::Gantt));
    d.frames(3);
    assert!(d.app.gantt_on_screen());
    d.click("tool_bars");
    d.frames(2);
    assert!(d.app.gantt_ui.bars.is_some(), "Bars... opens");
}

#[test]
fn copy_from_is_gone_from_the_bars_and_columns_dialogs() {
    let mut d = Driver::new("vm-copy-from", starter());
    d.app.open_bars_dialog();
    d.frames(4);
    assert!(d.has("bars_ok") && !d.has("bars_copy"));
    let (delete, up) = (d.rect("bars_delete"), d.rect("bars_up"));
    assert!(
        up.top() > delete.bottom() && up.top() - delete.bottom() < 8.,
        "no gap is left"
    );
    d.key(Key::Escape);
    d.frames(3);
    d.app.open_columns_dialog();
    d.frames(4);
    assert!(d.has("columns_ok") && !d.has("columns_copy_from"));
    let (apply, default) = (d.rect("columns_apply"), d.rect("columns_default"));
    assert!(default.top() > apply.bottom());
}

// ---------------------------------------------------------------- keyboard

#[test]
fn shift_f10_opens_the_menus_of_the_selected_row() {
    let shift = egui::Modifiers {
        shift: true,
        ..Default::default()
    };
    let mut d = Driver::new("vm-keyboard", resources::tests::with_resource());
    d.click("directory_resources");
    d.app.views.resources.assigned_only = false;
    d.app.views.resources.selected = Some(7);
    d.frames(3);
    d.key_with(Key::F10, shift);
    assert!(d.has("resources_menu_add"), "the Resources menu opens");
    d.key(Key::Escape);
    d.frames(2);
    d.click("directory_projects");
    d.frames(3);
    right_click_project_row(&mut d);
    d.key(Key::Escape);
    d.frames(2);
    assert!(!d.has("projects_menu_add"));
    d.key_with(Key::F10, shift);
    assert!(d.has("projects_menu_add"), "the Projects menu opens");
}

// ----------------------------------------------------------------- renders

/// An app whose every frame goes through the software painter.
struct Shot {
    app: App,
    ctx: egui::Context,
    painter: crate::headless_render::SoftwarePainter,
    time: f64,
    dir: std::path::PathBuf,
}

impl Shot {
    fn new(name: &str, project: ProjectData) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "openxer-vm-shot-{name}-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let app = App::new(&ctx, Store::open(dir.clone()).unwrap(), project);
        let mut shot = Self {
            app,
            ctx,
            painter: Default::default(),
            time: 0.,
            dir,
        };
        for _ in 0..4 {
            shot.frame(vec![]);
        }
        shot
    }
    fn frame(&mut self, events: Vec<egui::Event>) {
        let _ = self
            .painter
            .frame(&mut self.app, &self.ctx, &mut self.time, events, false);
    }
    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.frame(vec![]);
        }
    }
    fn rect(&self, action: &str) -> Rect {
        let [l, t, r, b] = *self
            .app
            .action_rects
            .get(action)
            .unwrap_or_else(|| panic!("no {action}"));
        Rect::from_min_max(Pos2::new(l, t), Pos2::new(r, b))
    }
    fn press(&mut self, at: Pos2, button: egui::PointerButton) {
        for pressed in [true, false] {
            self.frame(vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button,
                    pressed,
                    modifiers: Default::default(),
                },
            ]);
        }
        self.frames(3);
    }
    fn hover(&mut self, at: Pos2) {
        self.frame(vec![egui::Event::PointerMoved(at)]);
        self.frames(3);
    }
    fn save(&mut self, out: &std::path::Path, name: &str) {
        let (image, _) = self
            .painter
            .frame(&mut self.app, &self.ctx, &mut self.time, vec![], true)
            .unwrap();
        image.save(out.join(format!("{name}.png"))).unwrap();
    }
}

impl Drop for Shot {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.dir);
    }
}

/// Renders of the menus for a look: `artifacts/viewmenus/*.png`.
#[test]
fn render_the_view_menus() {
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/viewmenus");
    std::fs::create_dir_all(&out).unwrap();
    let secondary = egui::PointerButton::Secondary;
    let primary = egui::PointerButton::Primary;
    // Projects, with Group and Sort By open.
    let mut s = Shot::new("projects", starter());
    s.press(s.rect("directory_projects").center(), primary);
    let r = s.rect("projects_row_1");
    s.press(Pos2::new(r.left() + 90., r.center().y), secondary);
    assert!(s
        .app
        .action_rects
        .contains_key("projects_menu_open_project"));
    s.hover(s.rect("projects_menu_group_by").center());
    s.save(&out, "projects");
    // Resources.
    let mut s = Shot::new("resources", resources::tests::with_resource());
    s.press(s.rect("directory_resources").center(), primary);
    s.app.views.resources.assigned_only = false;
    s.frames(3);
    let r = s.rect("resources_row_0");
    s.press(Pos2::new(r.left() + 90., r.center().y), secondary);
    s.hover(s.rect("resources_menu_filter_by").center());
    s.save(&out, "resources");
    // Units & Prices.
    let mut s = Shot::new("rates", resources::tests::with_resource());
    s.press(s.rect("directory_resources").center(), primary);
    s.app.views.resources.assigned_only = false;
    s.app.views.resources.selected = Some(7);
    s.app.views.resources.tab = 3;
    s.frames(3);
    let r = s.rect("resource_rates_row_0");
    s.press(r.center(), secondary);
    s.save(&out, "units-and-prices");
    // WPs & Docs on a row with a file, Launch open.
    let mut s = Shot::new("docs", starter());
    s.press(s.rect("dock_wps_docs").center(), primary);
    s.app.document_command_add();
    s.frames(3);
    let r = s.rect("documents_row_0");
    s.press(Pos2::new(r.left() + 90., r.center().y), secondary);
    s.hover(s.rect("documents_menu_launch").center());
    s.save(&out, "documents");
    // Risks, empty area.
    let mut s = Shot::new("risks", starter());
    s.press(s.rect("dock_risks").center(), primary);
    let strip = s.rect("risks_layout");
    s.press(
        Pos2::new(strip.left() + 90., strip.bottom() + 70.),
        secondary,
    );
    s.save(&out, "risks-empty-area");
    // Toolbar popup.
    let mut s = Shot::new("chrome", starter());
    s.press(Pos2::new(1380., crate::menu::MENU_ROW + 10.), secondary);
    s.save(&out, "toolbar-popup");
    // Timescale band.
    let mut s = Shot::new("timescale", starter());
    s.press(s.rect("timescale_zoom").center(), secondary);
    s.save(&out, "timescale-band");
    // Roles dialog list.
    let mut p = starter();
    p.document.table_mut("ROLES").add(&[
        ("role_id", "1".into()),
        ("role_short_name", "FITTER".into()),
        ("role_name", "Fitter".into()),
        ("parent_role_id", String::new()),
        ("seq_num", "1".into()),
    ]);
    let p = ProjectData::from_document(p.document.clone(), 1, "roles".into()).unwrap();
    let mut s = Shot::new("roles", p);
    s.press(s.rect("menu_bar_enterprise").center(), primary);
    s.press(s.rect("menu_roles").center(), primary);
    s.frames(3);
    let r = s.rect("roles_list_row_0");
    s.press(Pos2::new(r.left() + 40., r.center().y), secondary);
    assert!(s.app.action_rects.contains_key("roles_menu_add"));
    s.save(&out, "roles-dialog");
}

/// Every view's row menu opens in the popup style (22 px items), closes on
/// Escape and leaves the project and the undo history alone.
#[test]
fn every_view_menu_renders_in_the_popup_style_and_closes_unchanged() {
    for (view, id) in [
        (MainView::Projects, "projects"),
        (MainView::Wbs, "wbs"),
        (MainView::Resources, "resources"),
        (MainView::Assignments, "assignments"),
        (MainView::Expenses, "expenses"),
        (MainView::Documents, "documents"),
        (MainView::Risks, "risks"),
        (MainView::Issues, "issues"),
        (MainView::Thresholds, "thresholds"),
    ] {
        let mut d = Driver::new(&format!("vm-sweep-{id}"), resources::tests::with_resource());
        let model = d.app.project.model.clone();
        let undo = d.app.history.counts().0;
        d.app.show_view(view);
        d.app.views.resources.assigned_only = false;
        d.frames(4);
        let at = if d.has(&format!("{id}_row_0")) {
            row_point(&d, id, 0)
        } else {
            empty_area(&d, id)
        };
        right_click(&mut d, at);
        let items: Vec<String> = d
            .app
            .action_rects
            .keys()
            .filter(|k| k.starts_with(&format!("{id}_menu_")))
            .cloned()
            .collect();
        assert!(!items.is_empty(), "{id}: the menu opened");
        for key in &items {
            let r = d.rect(key);
            assert!(
                (r.height() - 22.).abs() < 0.6,
                "{id}: {key} is {} px high, not 22",
                r.height()
            );
        }
        d.key(Key::Escape);
        d.frames(2);
        assert!(
            !d.app
                .action_rects
                .keys()
                .any(|k| k.starts_with(&format!("{id}_menu_"))),
            "{id}: Escape closes the menu"
        );
        assert_eq!(d.app.project.model, model, "{id}");
        assert_eq!(d.app.history.counts().0, undo, "{id}");
        assert_eq!(d.app.error, None, "{id}");
    }
}

/// The Files tab's Launch... buttons open the location through the same
/// checks and confirmation as the right-click Launch: enabled once a
/// location is set, a program refused, nothing opened before Open.
#[test]
fn files_tab_launch_buttons_open_the_document_after_asking() {
    let f = Files::new("files-tab");
    let mut d = Driver::new("vm-launch-files-tab", starter());
    d.click("dock_wps_docs");
    d.frames(3);
    d.app.document_command_add();
    let id = d.app.views.records.documents.selected.expect("selected");
    d.app.views.records.documents.tab = 2;
    d.frames(3);
    // No location yet: the buttons stay disabled.
    d.click("document_private_loc_launch");
    d.frames(2);
    assert!(d.app.views.records.launch.is_none());
    set_document_location(&mut d, id, "private_loc", &f.path("plan.pdf"));
    set_document_location(&mut d, id, "public_loc", &f.path("run.exe"));
    d.app.touch_records();
    d.frames(3);
    d.app.status = "before".into();
    d.click("document_private_loc_launch");
    d.frames(2);
    let shown = d.app.views.records.launch.clone().expect("the box is up");
    assert!(shown.ends_with("plan.pdf"), "{shown}");
    assert_eq!(d.app.status, "before", "nothing opened before Open");
    d.click("launch_open");
    d.frames(2);
    assert_eq!(d.app.status, format!("Opened {shown}"));
    // A program behind the public location is refused, with no box.
    d.click("document_public_loc_launch");
    d.frames(2);
    assert!(d.app.views.records.launch.is_none(), "no box for a program");
    assert!(d.app.status.contains("type"), "{}", d.app.status);
}

#[test]
fn the_wbs_layout_menu_opens_the_shared_column_chooser() {
    let mut d = Driver::new("vm-wbs-columns", starter());
    d.click("directory_wbs");
    d.frames(2);
    d.click("wbs_layout");
    d.frames(2);
    d.click("wbs_layout_columns");
    let dialog = d
        .app
        .detail
        .column_dialog
        .clone()
        .expect("Columns... opens");
    assert_eq!(dialog.caption, "WBS");
    let keep: Vec<String> = dialog.available.iter().take(2).cloned().collect();
    d.app.detail.column_dialog.as_mut().unwrap().selected = keep;
    d.click("detail_columns_ok");
    assert!(d.app.detail.column_dialog.is_none());
    assert_eq!(d.app.views.wbs.columns.order, [0, 1]);
    assert!(d.app.error.is_none());
}
#[test]
fn an_empty_record_table_says_so_under_its_header() {
    let mut d = Driver::new("vm-empty-expenses", starter());
    d.app.show_view(MainView::Expenses);
    d.frames(3);
    assert!(expenses::expenses(&d.app.project).is_empty());
    assert!(d.has("expenses_empty"), "the empty-state line is drawn");
    let note = d.rect("expenses_empty");
    let header = d.rect("expenses_col_0");
    assert!(note.top() >= header.bottom() && note.top() < header.bottom() + 20.);
    assert!(note.width() > 100., "{note:?}");
}
