//! Workflow: User Defined columns in the view tables besides the
//! Activities window (backlog T2.6).
//! - Each table's Columns chooser offers its subject area's user defined
//!   fields under "User Defined"; the column shows the values and its cells
//!   are typed or picked in place (text, start and finish dates, cost,
//!   integer, number, indicator, code), one undo step each, through the
//!   shared edit layer.
//! - WBS: every data type, Replace in a text field, export as XER and XML
//!   and import back, then Undo of every value.
//! - Projects: a value on the open project and a sort on the column; with
//!   a User Defined column shown and two projects selected, Fill Down on
//!   Project Name still works, is off on the User Defined column, and that
//!   column edits the open project's value only.
//! - Resources: values on two resources, a sort on the column and Fill
//!   Down.
//! - Resource Assignments: a cost and an indicator, chosen in Spreadsheet
//!   Fields.
use super::*;
use crate::views::udf_columns;
use openxer_xer::edit::udf::{fields_of, UdfField};

/// The scenario's fields: id, subject area, title and data type.
const FIELDS: [(&str, &str, &str, &str); 12] = [
    ("801", "PROJWBS", "Lead Crew", "FT_TEXT"),
    ("802", "PROJWBS", "Walkdown", "FT_START_DATE"),
    ("803", "PROJWBS", "Turnover", "FT_END_DATE"),
    ("804", "PROJWBS", "Bid Cost", "FT_MONEY"),
    ("805", "PROJWBS", "Crew Count", "FT_INT"),
    ("806", "PROJWBS", "Weight", "FT_FLOAT_2_DECIMALS"),
    ("807", "PROJWBS", "Health", "FT_STATICTYPE"),
    ("808", "PROJWBS", "Zone", "FT_CODE"),
    ("811", "PROJECT", "Approval Code", "FT_TEXT"),
    ("821", "RSRC", "Allowance", "FT_MONEY"),
    ("831", "TASKRSRC", "Bill Rate", "FT_MONEY"),
    ("832", "TASKRSRC", "Status Flag", "FT_STATICTYPE"),
];

/// The resource project of the table scenarios with the fields above and
/// the Zone field's two codes.
fn udf_project() -> ProjectData {
    let mut p = views::resources::tests::with_resource();
    let doc = &mut p.document;
    for (id, table, title, kind) in FIELDS {
        doc.table_mut("UDFTYPE").add(&[
            ("udf_type_id", id.into()),
            ("table_name", table.into()),
            ("udf_type_name", format!("user_field_{id}")),
            ("udf_type_label", title.into()),
            ("logical_data_type", kind.into()),
        ]);
    }
    for (id, short, name) in [("8081", "Z1", "Zone one"), ("8082", "Z2", "Zone two")] {
        doc.table_mut("UDFCODE").add(&[
            ("udf_code_id", id.into()),
            ("udf_type_id", "808".into()),
            ("short_name", short.into()),
            ("udf_code_name", name.into()),
        ]);
    }
    p
}

fn type_id(title: &str) -> i64 {
    let field = FIELDS.iter().find(|f| f.2 == title).expect("a field");
    field.0.parse().unwrap()
}

/// The field number of `title`'s column in a view table's columns.
fn field_of(columns: &views::grid::Columns, title: &str) -> usize {
    let id = udf_columns::id_of(type_id(title));
    let at = columns.extra.iter().position(|e| *e == id);
    columns.own() + at.unwrap_or_else(|| panic!("{title} is shown"))
}

/// `title`'s value on record `fk` of subject area `table`, as stored.
fn stored_value(p: &ProjectData, table: &str, fk: i64, title: &str) -> UdfField {
    fields_of(&p.document, table, fk, p.model.origin_serial)
        .into_iter()
        .find(|f| f.label == title)
        .unwrap_or_else(|| panic!("{title} is a field of {table}"))
}

/// Click control `key`, drawn now (a failure names the caller's line).
#[track_caller]
fn tap(d: &mut Driver, key: &str) {
    d.frames(2);
    assert!(d.has(key), "{key} is drawn");
    d.click(key);
}

/// The rectangle of control `key`, drawn now.
#[track_caller]
fn rect_of(d: &Driver, key: &str) -> egui::Rect {
    assert!(d.has(key), "{key} is drawn");
    d.rect(key)
}

/// In the open Columns chooser: the User Defined field `title` goes after
/// the first selected column, then OK. `list` shows Available Options as
/// one alphabetical list first (a long list of the table's own columns
/// would hide the group).
fn add_column(d: &mut Driver, title: &str, list: bool) {
    assert!(d.app.detail.column_dialog.is_some(), "Columns... is open");
    if list {
        // Available Options > Group and Sort By > List.
        tap(d, "detail_columns_available_header");
        assert!(d.hover_text("Group and Sort By"), "the header menu");
        tap(d, "available_view_list");
        d.frames(1);
    } else {
        assert!(
            d.has("detail_columns_available_group"),
            "the User Defined group heading"
        );
    }
    tap(d, "detail_columns_selected_0");
    let offered = super::wbs_projects_tables::pick_available(d, title);
    assert!(offered, "{title} is offered");
    tap(d, "detail_columns_add");
    tap(d, "detail_columns_ok");
    d.frames(2);
    assert!(d.app.detail.column_dialog.is_none(), "OK closes it");
}

/// Click the cell at (`row`, `field`) of view table `table`.
#[track_caller]
fn click_cell(d: &mut Driver, table: &str, row: usize, field: usize, modifiers: egui::Modifiers) {
    d.frames(1);
    let x = rect_of(d, &format!("{table}_col_{field}")).center().x;
    let y = rect_of(d, &format!("{table}_row_{row}")).center().y;
    d.press_at(Pos2::new(x, y), egui::PointerButton::Primary, modifiers);
    d.frames(1);
}

/// Open the cell's editor: a click selects the row, a click on the
/// focused cell opens it.
#[track_caller]
fn open_cell(d: &mut Driver, table: &str, row: usize, field: usize) {
    click_cell(d, table, row, field, Default::default());
    click_cell(d, table, row, field, Default::default());
    assert!(
        d.has(&format!("{table}_edit")),
        "the {table} cell editor opens on row {row}, field {field}: {}",
        d.app.status
    );
}

/// Type `text` into the cell and press Enter.
#[track_caller]
fn type_cell(d: &mut Driver, table: &str, row: usize, field: usize, text: &str) {
    open_cell(d, table, row, field);
    d.type_text(text);
    d.key(Key::Enter);
    d.frames(2);
    d.assert_clean(text);
    assert!(!d.has(&format!("{table}_edit")), "Enter closes the editor");
}

/// Pick item `item` of the cell's list.
#[track_caller]
fn pick_cell(d: &mut Driver, table: &str, row: usize, field: usize, item: usize) {
    open_cell(d, table, row, field);
    tap(d, &format!("{table}_choice_{item}"));
    d.frames(2);
    d.assert_clean("pick");
}

fn undo(d: &mut Driver) {
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
}

/// The table row whose drawn text `text` it holds.
#[track_caller]
fn row_with(d: &mut Driver, table: &str, text: &str) -> usize {
    let texts = d.texts();
    (0..60)
        .find(|r| {
            let key = format!("{table}_row_{r}");
            d.has(&key)
                && texts
                    .iter()
                    .any(|(t, rect)| t == text && d.rect(&key).contains(rect.center()))
        })
        .expect("a row of the table shows the text")
}

#[test]
fn workflow_wbs_user_defined_columns_take_every_data_type_and_survive_export() {
    let mut d = Driver::new("wf-udf-wbs", udf_project());
    show(&mut d, "wbs");
    let p = &d.app.project;
    let element = p
        .wbs
        .iter()
        .find(|w| !views::wbs::is_project_node(p, w.id))
        .expect("a WBS element")
        .id;
    let start = d.app.history.counts().0;
    // Typed values, each in its own new column right after WBS Code.
    let typed = [
        ("Lead Crew", "Qz crew north"),
        ("Walkdown", "12-Jan-27"),
        ("Turnover", "26-Feb-27"),
        ("Bid Cost", "1250.5"),
        ("Crew Count", "12"),
        ("Weight", "3.25"),
    ];
    for (title, text) in typed {
        tap(&mut d, "wbs_layout");
        tap(&mut d, "wbs_layout_columns");
        add_column(&mut d, title, false);
        let field = field_of(&d.app.views.wbs.columns, title);
        let row = wbs_row(&d, element);
        type_cell(&mut d, "wbs", row, field, text);
    }
    // Picked values: an indicator and a code.
    for (title, item) in [("Health", 3), ("Zone", 2)] {
        tap(&mut d, "wbs_layout");
        tap(&mut d, "wbs_layout_columns");
        add_column(&mut d, title, false);
        let field = field_of(&d.app.views.wbs.columns, title);
        let row = wbs_row(&d, element);
        pick_cell(&mut d, "wbs", row, field, item);
    }
    let p = &d.app.project;
    let value = |title: &str| stored_value(p, "PROJWBS", element, title);
    assert_eq!(value("Lead Crew").text, "Qz crew north");
    let day = |field: &str| date(p, value(field).date);
    let (walkdown, turnover) = (day("Walkdown"), day("Turnover"));
    assert!(walkdown.starts_with("2027-01-12"), "{walkdown}");
    assert!(turnover.starts_with("2027-02-26"), "{turnover}");
    assert_eq!(value("Bid Cost").number, Some(1250.5));
    assert_eq!(value("Crew Count").number, Some(12.));
    assert_eq!(value("Weight").number, Some(3.25));
    assert_eq!(value("Health").text, "Green");
    assert_eq!(value("Zone").text, "Z2");
    assert_eq!(d.app.history.counts().0, start + 8, "a step a value");
    // The cell shows the value: the indicator by name.
    let row = wbs_row(&d, element);
    assert_eq!(row_with(&mut d, "wbs", "Green"), row);

    // Replace in the text column, one undo step, then undone.
    d.click("menu_bar_edit");
    d.click("menu_replace");
    d.frames(2);
    type_into(&mut d, "find_what", "Qz");
    type_into(&mut d, "replace_with", "Kx");
    tap(&mut d, "replace_all");
    d.frames(2);
    tap(&mut d, "find_close");
    let text = stored_value(&d.app.project, "PROJWBS", element, "Lead Crew").text;
    assert_eq!(text, "Kx crew north", "{}", d.app.status);
    assert_eq!(d.app.history.counts().0, start + 9);
    undo(&mut d);
    let text = stored_value(&d.app.project, "PROJWBS", element, "Lead Crew").text;
    assert_eq!(text, "Qz crew north");

    // XER and XML exports carry the values.
    let named = d.app.project.wbs.iter().find(|w| w.id == element);
    let name = named.expect("the element").name.clone();
    let back = xer_round_trip(&mut d, "udf-wbs.xer");
    let in_file = |p: &ProjectData, title: &str| {
        let w = p.wbs.iter().find(|w| w.name == name).expect("the element");
        stored_value(p, "PROJWBS", w.id, title)
    };
    assert_eq!(in_file(&back, "Lead Crew").text, "Qz crew north");
    assert_eq!(in_file(&back, "Bid Cost").number, Some(1250.5));
    assert_eq!(in_file(&back, "Health").text, "Green");
    assert_eq!(in_file(&back, "Zone").text, "Z2");
    let xml = d.dir.join("udf-wbs.xml");
    d.export(interchange::ExportFormat::Xml, &xml);
    let doc = openxer_interchange::project_xml::read_file(&xml).unwrap();
    let key = d.app.project.model.project.proj_id;
    let from_xml = ProjectData::from_document(doc, key, "xml".into()).unwrap();
    for title in ["Lead Crew", "Crew Count", "Weight", "Health"] {
        assert_eq!(
            in_file(&from_xml, title).text,
            in_file(&back, title).text,
            "{title} in the XML file"
        );
    }
    assert_eq!(in_file(&from_xml, "Bid Cost").number, Some(1250.5));
    assert_eq!(
        in_file(&from_xml, "Walkdown").date,
        in_file(&back, "Walkdown").date
    );

    // Undo takes every value back.
    for _ in 0..8 {
        undo(&mut d);
    }
    let origin = d.app.project.model.origin_serial;
    let left = fields_of(&d.app.project.document, "PROJWBS", element, origin);
    let empty = |f: &UdfField| f.text.is_empty() && f.number.is_none() && f.date.is_none();
    assert!(left.iter().all(empty), "undone: {left:?}");
    assert_eq!(d.app.history.counts().0, start);

    // The exported file comes back in through Import with its values.
    let ids = |d: &Driver| -> Vec<i64> {
        interchange::workspace::rows(&d.app.store)
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect()
    };
    let before = ids(&d);
    let file = d.dir.join("udf-wbs.xer");
    d.import(&file);
    let new = ids(&d).into_iter().find(|id| !before.contains(id));
    let imported = stored(&d, new.expect("the import adds a project"));
    assert_eq!(in_file(&imported, "Lead Crew").text, "Qz crew north");
    assert_eq!(in_file(&imported, "Crew Count").number, Some(12.));
    assert_eq!(in_file(&imported, "Zone").text, "Z2");
}

#[test]
fn workflow_projects_user_defined_column_types_and_sorts() {
    let mut d = Driver::new("wf-udf-projects", udf_project());
    show(&mut d, "projects");
    let id = d.app.project.model.project.name.clone();
    let row = row_with(&mut d, "projects", &id);
    let at = rect_of(&d, &format!("projects_row_{row}"));
    d.press_at(
        Pos2::new(at.left() + 40., at.center().y),
        egui::PointerButton::Secondary,
        Default::default(),
    );
    assert!(d.hover_text("Columns"), "the Columns submenu");
    tap(&mut d, "projects_menu_customize");
    d.frames(2);
    add_column(&mut d, "Approval Code", true);
    let field = field_of(&d.app.views.projects.columns, "Approval Code");
    let start = d.app.history.counts().0;
    let row = row_with(&mut d, "projects", &id);
    type_cell(&mut d, "projects", row, field, "AC-200");
    let pid = d.app.project.model.project.proj_id;
    let value = stored_value(&d.app.project, "PROJECT", pid, "Approval Code");
    assert_eq!(value.text, "AC-200");
    assert_eq!(d.app.history.counts().0, start + 1, "one undo step");
    let shown = row_with(&mut d, "projects", "AC-200");
    assert_eq!(shown, row, "the cell shows it");

    // A click on the column title sorts on it, a second click descending.
    use views::project_rollups::Sort;
    tap(&mut d, &format!("projects_col_{field}"));
    d.frames(1);
    let ascending = Sort {
        field,
        descending: false,
    };
    assert_eq!(d.app.views.projects.sort, Some(ascending));
    tap(&mut d, &format!("projects_col_{field}"));
    d.frames(1);
    assert_eq!(
        d.app.views.projects.sort.map(|s| (s.field, s.descending)),
        Some((field, true))
    );

    undo(&mut d);
    let value = stored_value(&d.app.project, "PROJECT", pid, "Approval Code");
    assert_eq!(value.text, "", "undone");
}

/// The open project's User Defined values of field `type_id`, by record.
fn udf_rows(p: &ProjectData, type_id: i64) -> Vec<String> {
    let Some(t) = p.document.table("UDFVALUE") else {
        return vec![];
    };
    t.rows
        .iter()
        .filter(|r| t.value(r, "udf_type_id") == type_id.to_string())
        .map(|r| t.value(r, "fk_id").to_string())
        .collect()
}

#[test]
fn workflow_projects_fill_down_on_name_with_a_user_defined_column() {
    let mut d = fresh("wf-udf-projects-fill");
    let first = views::wbs::project_name(&d.app.project);
    let first_id = d.app.project.model.project.name.clone();
    new_project(&mut d, "PMP2", "Pump Two", "01-Dec-26", "31-Dec-26");
    // The open project's file defines a project field.
    let (id, table, title, kind) = FIELDS[8];
    d.app.project.document.table_mut("UDFTYPE").add(&[
        ("udf_type_id", id.into()),
        ("table_name", table.into()),
        ("udf_type_name", format!("user_field_{id}")),
        ("udf_type_label", title.into()),
        ("logical_data_type", kind.into()),
    ]);
    show(&mut d, "projects");
    let row = row_with(&mut d, "projects", "PMP2");
    let at = rect_of(&d, &format!("projects_row_{row}"));
    d.press_at(
        Pos2::new(at.left() + 40., at.center().y),
        egui::PointerButton::Secondary,
        Default::default(),
    );
    assert!(d.hover_text("Columns"), "the Columns submenu");
    tap(&mut d, "projects_menu_customize");
    d.frames(2);
    add_column(&mut d, "Approval Code", true);
    let field = field_of(&d.app.views.projects.columns, "Approval Code");
    let start = d.app.history.counts().0;
    let pid = d.app.project.model.project.proj_id;
    let row = row_with(&mut d, "projects", "PMP2");
    type_cell(&mut d, "projects", row, field, "AC-7");
    assert_eq!(udf_rows(&d.app.project, 811), vec![pid.to_string()]);

    // Both projects selected on Project Name: Fill Down copies the first
    // row's name to the open project, as without the column.
    let source = row_with(&mut d, "projects", &first_id);
    let target = row_with(&mut d, "projects", "PMP2");
    click_cell(&mut d, "projects", source, 1, Default::default());
    click_cell(&mut d, "projects", target, 1, ctrl());
    assert!(!d.has("projects_edit"), "selecting opens no cell editor");
    d.frames(1);
    assert!(
        d.app.view_fill_ready(),
        "Fill Down is enabled on Project Name with two rows"
    );
    d.click("menu_bar_edit");
    d.click("edit_fill_down");
    d.frames(2);
    d.assert_clean("projects fill down");
    assert_eq!(
        views::wbs::project_name(&d.app.project),
        first,
        "the open project takes the first row's name: {}",
        d.app.status
    );
    assert_eq!(d.app.history.counts().0, start + 2, "one undo step");
    let value = stored_value(&d.app.project, "PROJECT", pid, "Approval Code");
    assert_eq!(value.text, "AC-7", "Fill Down leaves the column alone");

    // Both selected on the User Defined column: Fill Down is off there.
    // The rows keep their places (the table is in Project ID order); the
    // open project's name may now read like the other project's ID.
    assert_eq!(row_with(&mut d, "projects", "PMP2"), target);
    click_cell(&mut d, "projects", source, field, Default::default());
    click_cell(&mut d, "projects", target, field, ctrl());
    d.frames(1);
    assert!(
        !d.app.view_fill_ready(),
        "Fill Down is off on a User Defined column"
    );
    // The other project's cell opens no editor; the open project's does,
    // and writes the open project's value only.
    click_cell(&mut d, "projects", source, field, Default::default());
    click_cell(&mut d, "projects", source, field, Default::default());
    assert!(!d.has("projects_edit"), "no editor on another project");
    type_cell(&mut d, "projects", target, field, "AC-8");
    let value = stored_value(&d.app.project, "PROJECT", pid, "Approval Code");
    assert_eq!(value.text, "AC-8");
    assert_eq!(udf_rows(&d.app.project, 811), vec![pid.to_string()]);
    assert_eq!(d.app.history.counts().0, start + 3, "one undo step");

    // Undo takes the value, the name and the first value back.
    undo(&mut d);
    let value = stored_value(&d.app.project, "PROJECT", pid, "Approval Code");
    assert_eq!(value.text, "AC-7");
    undo(&mut d);
    assert_eq!(views::wbs::project_name(&d.app.project), "Pump Two");
    undo(&mut d);
    assert!(udf_rows(&d.app.project, 811).is_empty(), "undone");
    assert_eq!(d.app.history.counts().0, start);
}

#[test]
fn workflow_resources_user_defined_column_sorts_and_fills_down() {
    let mut d = Driver::new("wf-udf-resources", udf_project());
    show(&mut d, "resources");
    d.app.views.resources.assigned_only = false;
    d.frames(3);
    let at = rect_of(&d, "resources_row_0");
    d.press_at(
        Pos2::new(at.left() + 30., at.center().y),
        egui::PointerButton::Secondary,
        Default::default(),
    );
    assert!(d.hover_text("Columns"), "the Columns submenu");
    tap(&mut d, "resources_menu_columns");
    d.frames(2);
    add_column(&mut d, "Allowance", true);
    let field = d
        .app
        .views
        .resources
        .fields
        .iter()
        .position(|f| f == "rudf_821")
        .expect("the Allowance column is shown");
    let start = d.app.history.counts().0;
    let pipe = row_with(&mut d, "resources", "PIPE");
    type_cell(&mut d, "resources", pipe, field, "300");
    let idle = row_with(&mut d, "resources", "IDLE");
    type_cell(&mut d, "resources", idle, field, "100");
    let allowance = |d: &Driver, rsrc: i64| stored_value(&d.app.project, "RSRC", rsrc, "Allowance");
    assert_eq!(allowance(&d, 7).number, Some(300.));
    assert_eq!(allowance(&d, 9).number, Some(100.));
    assert_eq!(d.app.history.counts().0, start + 2);
    assert_eq!(
        row_with(&mut d, "resources", &format::cost(300.)),
        row_with(&mut d, "resources", "PIPE"),
        "the cell shows the cost"
    );

    // Sorted on the column: IDLE (100) before PIPE (300).
    tap(&mut d, &format!("resources_col_{field}"));
    d.frames(2);
    assert_eq!(
        d.app.views.resources.list.sort.field.as_deref(),
        Some("rudf_821")
    );
    let (idle, pipe) = (
        row_with(&mut d, "resources", "IDLE"),
        row_with(&mut d, "resources", "PIPE"),
    );
    assert!(idle < pipe, "IDLE {idle} sorts before PIPE {pipe}");

    // Fill Down from IDLE to PIPE: one undo step.
    click_cell(&mut d, "resources", pipe, 0, Default::default());
    click_cell(&mut d, "resources", idle, field, Default::default());
    click_cell(&mut d, "resources", pipe, field, ctrl());
    d.click("menu_bar_edit");
    d.click("edit_fill_down");
    d.frames(2);
    assert_eq!(allowance(&d, 7).number, Some(100.), "{}", d.app.status);
    assert_eq!(d.app.history.counts().0, start + 3);
    for _ in 0..3 {
        undo(&mut d);
    }
    assert_eq!(allowance(&d, 7).number, None, "undone");
    assert_eq!(allowance(&d, 9).number, None, "undone");
}

#[test]
fn workflow_assignment_user_defined_columns_come_from_spreadsheet_fields() {
    let mut d = Driver::new("wf-udf-assignments", udf_project());
    d.app.show_view(views::MainView::Assignments);
    d.frames(3);
    let start = d.app.history.counts().0;
    for title in ["Bill Rate", "Status Flag"] {
        let at = rect_of(&d, "assignments_row_1");
        d.press_at(
            Pos2::new(at.left() + 30., at.center().y),
            egui::PointerButton::Secondary,
            Default::default(),
        );
        assert!(d.has("assignments_menu_options"), "the table menu");
        tap(&mut d, "assignments_menu_options");
        d.frames(2);
        assert!(d.app.panes.usage_fields.is_some(), "the fields dialog");
        // The window's own catalog fields sit above the User Defined ones:
        // scroll the Available Options list until `title` is clicked.
        let list_at = (0..60)
            .find_map(|r| {
                let key = format!("usage_fields_available_row_{r}");
                if d.has(&key) {
                    Some(d.rect(&key).center() + Vec2::new(0., 40.))
                } else {
                    None
                }
            })
            .expect("usage_fields_available shows rows");
        let mut picked = false;
        for _ in 0..16 {
            if d.click_text(title)
                && d.app
                    .panes
                    .usage_fields
                    .as_ref()
                    .is_some_and(|f| f.available.is_some())
            {
                picked = true;
                break;
            }
            let _ = d.frame(vec![
                egui::Event::PointerMoved(list_at),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::Vec2::new(0., -108.),
                    modifiers: Default::default(),
                },
            ]);
            d.frames(2);
        }
        assert!(picked, "{title} is offered");
        tap(&mut d, "usage_fields_add");
        // Up past Activity Name and the column added before.
        let ups = if title == "Bill Rate" { 1 } else { 2 };
        for _ in 0..ups {
            tap(&mut d, "usage_fields_up");
        }
        tap(&mut d, "usage_fields_ok");
        d.frames(2);
    }
    let columns = &d.app.views.assignments.columns;
    let rate = field_of(columns, "Bill Rate");
    let flag = field_of(columns, "Status Flag");
    // Row 0 is the PIPE band, row 1 its assignment (TASKRSRC 1).
    pick_cell(&mut d, "assignments", 1, flag, 3);
    type_cell(&mut d, "assignments", 1, rate, "85.5");
    let p = &d.app.project;
    let bill = stored_value(p, "TASKRSRC", 1, "Bill Rate");
    assert_eq!(bill.number, Some(85.5));
    assert_eq!(stored_value(p, "TASKRSRC", 1, "Status Flag").text, "Green");
    assert_eq!(d.app.history.counts().0, start + 2);
    assert_eq!(row_with(&mut d, "assignments", "Green"), 1);
    // The band row holds no record: its cells take no editor.
    click_cell(&mut d, "assignments", 0, rate, Default::default());
    click_cell(&mut d, "assignments", 0, rate, Default::default());
    assert!(!d.has("assignments_edit"), "a band cell is not edited");

    let back = xer_round_trip(&mut d, "udf-assignments.xer");
    let exported = stored_value(&back, "TASKRSRC", 1, "Bill Rate");
    assert_eq!(exported.number, Some(85.5), "in the exported file");
    undo(&mut d);
    undo(&mut d);
    let p = &d.app.project;
    assert_eq!(stored_value(p, "TASKRSRC", 1, "Bill Rate").number, None);
    assert_eq!(stored_value(p, "TASKRSRC", 1, "Status Flag").text, "");
}
