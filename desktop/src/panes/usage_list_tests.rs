//! The resource list of the Resource Usage Profile and Spreadsheet on a
//! synthetic two-project workspace: Filter By, the tree with context
//! parents, expand and collapse, Group and Sort By, the header sort, the
//! columns, the multi-selection sums, the role view, Display Activities for
//! selected and the layout round trip. Renders go to `artifacts/usage-list/`.
use super::usage_list::{self as list, ListFilter, ListGroup, ListKey, ListView};
use super::*;
use crate::qa_sweep::Driver;

/// RIVERBEND (open) and NORTHYARD (parked) share this resource tree:
///
/// ```text
/// CREW        Field crews     labor     no assignment (a parent)
///   CREW.PIPE Pipefitters     labor     RB 1010 24 h, role FITTER, Area NORTH
///   CREW.WELD Welders         labor     RB 1020 16 h, role WELDER, Area SOUTH
///   CREW.ELEC Electricians    labor     NY 2010 8 h only
/// EQUIP       Equipment       nonlabor  no assignment (a parent)
///   EQUIP.CRANE Crane         nonlabor  RB 1010 8 h, Area NORTH
/// MAT         Pipe spools     material  RB 1030 5 (units)
/// SAFETY      Safety officer  labor     owner of a RIVERBEND risk only
/// BENCH       Bench crew      labor     nothing anywhere
/// ```
///
/// Roles TRADES > FITTER, WELDER, PAINTER; RB 1030 also asks for 10 h of
/// FITTER with no resource named (unstaffed).
fn workspace(name: &str) -> Driver {
    workspace_probed(name, Default::default())
}

/// `workspace` with the render probe on from the first frame.
fn workspace_probed(name: &str, probe: crate::dialog_sweep::Probe) -> Driver {
    let mut doc = openxer_xer::starter_portfolio();
    for (id, parent, code, title, kind, seq, role) in [
        ("100", "", "CREW", "Field crews", "RT_Labor", "1", ""),
        (
            "101",
            "100",
            "CREW.PIPE",
            "Pipefitters",
            "RT_Labor",
            "1",
            "11",
        ),
        ("102", "100", "CREW.WELD", "Welders", "RT_Labor", "2", "12"),
        (
            "103",
            "100",
            "CREW.ELEC",
            "Electricians",
            "RT_Labor",
            "3",
            "",
        ),
        ("200", "", "EQUIP", "Equipment", "RT_Equip", "2", ""),
        ("201", "200", "EQUIP.CRANE", "Crane", "RT_Equip", "1", ""),
        ("300", "", "MAT", "Pipe spools", "RT_Mat", "3", ""),
        ("400", "", "SAFETY", "Safety officer", "RT_Labor", "4", ""),
        ("500", "", "BENCH", "Bench crew", "RT_Labor", "5", ""),
    ] {
        doc.table_mut("RSRC").add(&[
            ("rsrc_id", id.into()),
            ("parent_rsrc_id", parent.into()),
            ("clndr_id", "1".into()),
            ("rsrc_short_name", code.into()),
            ("rsrc_name", title.into()),
            ("rsrc_type", kind.into()),
            ("def_qty_per_hr", "1".into()),
            ("active_flag", "Y".into()),
            ("rsrc_seq_num", seq.into()),
            ("role_id", role.into()),
        ]);
    }
    for (id, parent, code, title, seq) in [
        ("10", "", "TRADES", "Trades", "1"),
        ("11", "10", "FITTER", "Pipe fitter", "1"),
        ("12", "10", "WELDER", "Welder", "2"),
        ("13", "10", "PAINTER", "Painter", "3"),
    ] {
        doc.table_mut("ROLES").add(&[
            ("role_id", id.into()),
            ("parent_role_id", parent.into()),
            ("role_short_name", code.into()),
            ("role_name", title.into()),
            ("seq_num", seq.into()),
        ]);
    }
    for (id, task, proj, rsrc, role, qty) in [
        ("1", "1010", "1", "101", "11", "24"),
        ("2", "1020", "1", "102", "12", "16"),
        ("3", "1010", "1", "201", "", "8"),
        ("4", "1030", "1", "300", "", "5"),
        ("5", "1030", "1", "", "11", "10"),
        ("6", "2010", "2", "103", "", "8"),
    ] {
        doc.table_mut("TASKRSRC").add(&[
            ("taskrsrc_id", id.into()),
            ("task_id", task.into()),
            ("proj_id", proj.into()),
            ("rsrc_id", rsrc.into()),
            ("role_id", role.into()),
            ("remain_qty", qty.into()),
            ("remain_qty_per_hr", "1".into()),
            ("target_qty", qty.into()),
            ("rollup_dates_flag", "N".into()),
        ]);
    }
    doc.table_mut("RCATTYPE").add(&[
        ("rsrc_catg_type_id", "1".into()),
        ("rsrc_catg_type", "Area".into()),
    ]);
    for (id, value) in [("1", "NORTH"), ("2", "SOUTH")] {
        doc.table_mut("RCATVAL").add(&[
            ("rsrc_catg_id", id.into()),
            ("rsrc_catg_type_id", "1".into()),
            ("rsrc_catg_short_name", value.into()),
            ("rsrc_catg_name", format!("{value} area")),
        ]);
    }
    for (rsrc, value) in [("101", "1"), ("102", "2"), ("201", "1")] {
        doc.table_mut("RSRCRCAT").add(&[
            ("rsrc_id", rsrc.into()),
            ("rsrc_catg_type_id", "1".into()),
            ("rsrc_catg_id", value.into()),
        ]);
    }
    doc.table_mut("PROJRISK").add(&[
        ("risk_id", "1".into()),
        ("proj_id", "1".into()),
        ("risk_code", "R-1".into()),
        ("risk_name", "Crane access".into()),
        ("rsrc_id", "400".into()),
    ]);
    let make = |key: i64| {
        let mut p = ProjectData::from_document(doc.clone(), key, "usage-list.xer".into()).unwrap();
        scheduler::schedule_with_resources(&mut p.model, &p.resources).unwrap();
        p
    };
    let (rb, ny) = (make(1), make(2));
    let dir = std::env::temp_dir().join(format!(
        "openxer-usage-list-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let mut store = Store::open(dir.join("workspace.db")).unwrap();
    let ny_id = store.save(&ny).unwrap();
    let rb_id = store.save(&rb).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, rb);
    app.restore_open_set();
    app.open_project_set(&[rb_id, ny_id]);
    assert_eq!(app.open_ids(), vec![rb_id, ny_id]);
    let mut d = Driver {
        app,
        ctx,
        time: 0.,
        dir,
        notes: vec![],
        screen: egui::Vec2::new(1500., 900.),
        probe,
        opened_dialog: false,
        origin: None,
        enter_check: false,
        setup: None,
        slice: None,
    };
    // These tests exercise the resource tree (parents, context rows,
    // Collapse All); a new layout lists flat, so they ask for Hierarchy.
    for list in [
        &mut d.app.panes.usage.profile_list,
        &mut d.app.panes.usage.sheet_list,
    ] {
        list.group = ListGroup::Hierarchy;
    }
    d.frames(3);
    d.home();
    d
}

#[test]
fn a_new_layout_lists_resources_flat_and_a_saved_one_keeps_its_choice() {
    let fresh = usage::UsageState::default();
    assert_eq!(fresh.profile_list.group, ListGroup::List);
    assert_eq!(fresh.sheet_list.group, ListGroup::List);
    // A layout saved before keeps its Hierarchy; one without the key lists.
    let mut saved = list::ListState::default();
    saved.restore(&serde_json::json!({"group": "hierarchy"}));
    assert_eq!(saved.group, ListGroup::Hierarchy);
    let mut bare = list::ListState::default();
    bare.restore(&serde_json::json!({"filter": "project"}));
    assert_eq!(bare.group, ListGroup::List);
    // The list itself: no context parents, by Resource ID.
    let mut d = resource_profile("default-list");
    d.app.panes.usage.profile_list.group = ListGroup::default();
    d.frames(2);
    assert!(context_codes(&d, true).is_empty());
    let shown = codes(&d, true);
    assert!(
        !shown.iter().any(|c| c == "CREW" || c == "EQUIP"),
        "{shown:?}"
    );
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

fn resource_profile(name: &str) -> Driver {
    let mut d = workspace(name);
    d.app.set_bottom(Some(BottomView::ResourceUsageProfile));
    d.frames(3);
    d
}

/// Resource ids of the list rows in order; groups and roles as 0 and -id.
fn codes(d: &Driver, profile: bool) -> Vec<String> {
    let resources = d.app.usage_resources();
    let roles = list::roles(&d.app.project);
    d.app
        .usage_list_rows(profile)
        .rows
        .iter()
        .map(|r| match &r.key {
            ListKey::Resource(id) => resources
                .iter()
                .find(|x| x.id == *id)
                .map(|x| x.code.clone())
                .unwrap_or_default(),
            ListKey::Role(id) => roles
                .iter()
                .find(|x| x.id == *id)
                .map(|x| x.code.clone())
                .unwrap_or_default(),
            ListKey::Group(_) => format!("[{}]", r.label),
        })
        .collect()
}

fn context_codes(d: &Driver, profile: bool) -> Vec<String> {
    let all = codes(d, profile);
    d.app
        .usage_list_rows(profile)
        .rows
        .iter()
        .zip(all)
        .filter(|(r, _)| r.context)
        .map(|(_, c)| c)
        .collect()
}

/// Opens the list's Display bar menu and clicks `path` (submenus first).
fn bar_menu(d: &mut Driver, pane: &str, path: &[&str]) {
    let bar = format!("usage_list_bar_{pane}");
    d.click(&bar);
    d.frames(1);
    let (last, subs) = path.split_last().unwrap();
    for sub in subs {
        let at = d.rect(&format!("{bar}_{sub}")).center();
        let _ = d.frame(vec![egui::Event::PointerMoved(at)]);
        d.frames(2);
    }
    d.click(&format!("{bar}_{last}"));
    d.frames(2);
}

fn row_of(d: &Driver, profile: bool, code: &str) -> usize {
    codes(d, profile)
        .iter()
        .position(|c| c == code)
        .unwrap_or_else(|| panic!("{code} is not listed: {:?}", codes(d, profile)))
}

fn click_row(d: &mut Driver, profile: bool, code: &str, modifiers: egui::Modifiers) {
    let k = row_of(d, profile, code);
    let r = d.rect(&format!("usage_resource_{k}"));
    d.press_at(
        Pos2::new(r.right() - 20., r.center().y),
        egui::PointerButton::Primary,
        modifiers,
    );
    d.frames(2);
}

fn shows(d: &mut Driver, text: &str) -> bool {
    d.texts().iter().any(|(t, _)| t == text)
}

fn shows_part(d: &mut Driver, text: &str) -> bool {
    d.texts().iter().any(|(t, _)| t.contains(text))
}

#[test]
fn filter_choices_list_exactly_the_right_resources() {
    let mut d = resource_profile("filter");
    // A new layout lists the current project's resources: assigned in
    // RIVERBEND, named on its risk, or assigned in the other open project
    // NORTHYARD; their unassigned parents stay as greyed context.
    assert_eq!(d.app.panes.usage.profile_list.filter, ListFilter::Project);
    assert!(shows(&mut d, "Display: Current Project's Resources"));
    assert_eq!(
        codes(&d, true),
        [
            "CREW",
            "CREW.PIPE",
            "CREW.WELD",
            "CREW.ELEC",
            "EQUIP",
            "EQUIP.CRANE",
            "MAT",
            "SAFETY"
        ]
    );
    assert_eq!(context_codes(&d, true), ["CREW", "EQUIP"]);
    // Only this project (NORTHYARD closed): CREW.ELEC goes.
    let others = list::project_resources(&d.app.project, &[]);
    assert!(!others.contains(&103) && others.contains(&400) && !others.contains(&500));
    // All Resources: every resource, BENCH too.
    bar_menu(&mut d, "profile", &["filter_by", "filter_all"]);
    assert_eq!(d.app.panes.usage.profile_list.filter, ListFilter::All);
    assert!(shows(&mut d, "Display: All Resources"));
    assert_eq!(codes(&d, true).len(), 9);
    assert!(context_codes(&d, true).is_empty());
    // Customize...: name or ID contains "weld": the welders under CREW.
    bar_menu(&mut d, "profile", &["filter_by", "filter_custom"]);
    assert!(d.app.panes.usage_list_filter.is_some());
    d.app.panes.usage_list_filter.as_mut().unwrap().1.text = "weld".into();
    d.click("usage_list_filter_ok");
    d.frames(2);
    assert_eq!(d.app.panes.usage.profile_list.filter, ListFilter::Custom);
    assert!(shows(&mut d, "Display: Custom Resource Filter"));
    assert_eq!(codes(&d, true), ["CREW", "CREW.WELD"]);
    assert_eq!(context_codes(&d, true), ["CREW"]);
    // Nonlabor only.
    let state = &mut d.app.panes.usage.profile_list.custom;
    state.text.clear();
    state.labor = false;
    state.material = false;
    assert_eq!(codes(&d, true), ["EQUIP", "EQUIP.CRANE"]);
    assert!(context_codes(&d, true).is_empty(), "EQUIP matches itself");
    // Code Area = NORTH.
    let state = &mut d.app.panes.usage.profile_list.custom;
    state.labor = true;
    state.material = true;
    state.code_type = "Area".into();
    state.code_value = "NORTH".into();
    assert_eq!(
        codes(&d, true),
        ["CREW", "CREW.PIPE", "EQUIP", "EQUIP.CRANE"]
    );
    assert_eq!(context_codes(&d, true), ["CREW", "EQUIP"]);
    // Any Area value, project only: SOUTH welders too; BENCH never.
    let state = &mut d.app.panes.usage.profile_list.custom;
    state.code_value.clear();
    state.project_only = true;
    assert_eq!(
        codes(&d, true),
        ["CREW", "CREW.PIPE", "CREW.WELD", "EQUIP", "EQUIP.CRANE"]
    );
    // Cancel changes nothing.
    let before = d.app.panes.usage.profile_list.clone();
    bar_menu(&mut d, "profile", &["filter_by", "filter_custom"]);
    d.app.panes.usage_list_filter.as_mut().unwrap().1.text = "x".into();
    d.click("usage_list_filter_cancel");
    d.frames(2);
    assert_eq!(d.app.panes.usage.profile_list, before);
    // The spreadsheet's list keeps its own filter.
    assert_eq!(d.app.panes.usage.sheet_list.filter, ListFilter::Project);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn the_tree_expands_and_collapses_and_parents_keep_their_own_values() {
    let mut d = resource_profile("tree");
    let k = row_of(&d, true, "CREW");
    assert!(d.has(&format!("usage_list_toggle_profile_{k}")));
    d.click(&format!("usage_list_toggle_profile_{k}"));
    d.frames(2);
    assert_eq!(
        codes(&d, true),
        ["CREW", "EQUIP", "EQUIP.CRANE", "MAT", "SAFETY"],
        "CREW folded"
    );
    assert!(d.app.usage_list_rows(true).rows[0].collapsed);
    d.click(&format!("usage_list_toggle_profile_{k}"));
    d.frames(2);
    assert_eq!(codes(&d, true).len(), 8, "open again");
    bar_menu(&mut d, "profile", &["collapse_all"]);
    assert_eq!(codes(&d, true), ["CREW", "EQUIP", "MAT", "SAFETY"]);
    bar_menu(&mut d, "profile", &["expand_all"]);
    assert_eq!(codes(&d, true).len(), 8);
    // The right-click menu has the same commands.
    let r = d.rect("usage_resource_0");
    d.press_at(
        r.center(),
        egui::PointerButton::Secondary,
        Default::default(),
    );
    d.frames(2);
    assert!(d.has("usage_profile_menu_collapse_all") && d.has("usage_profile_menu_filter_by"));
    d.click("usage_profile_menu_collapse_all");
    d.frames(2);
    assert_eq!(codes(&d, true).len(), 4);
    bar_menu(&mut d, "profile", &["expand_all"]);
    // A parent selected shows its own (no) units, not its children's: the
    // spreadsheet's resource row of CREW totals nothing; CREW.PIPE 24 h.
    d.app.set_bottom(Some(BottomView::ResourceUsageSheet));
    d.frames(3);
    click_row(&mut d, false, "CREW", Default::default());
    let rows = sheet_rows(&d);
    assert_eq!(rows.len(), 1, "no assignments of its own");
    assert!(rows[0].total.abs() < 1e-9);
    click_row(&mut d, false, "CREW.PIPE", Default::default());
    let rows = sheet_rows(&d);
    assert!((rows[0].total - 24.).abs() < 1e-9);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn group_and_sort_modes_and_the_header_sort() {
    let mut d = resource_profile("group");
    // List: flat, in the resources' order.
    bar_menu(&mut d, "profile", &["group_by", "group_list"]);
    assert_eq!(d.app.panes.usage.profile_list.group, ListGroup::List);
    assert_eq!(
        codes(&d, true),
        [
            "CREW.ELEC",
            "CREW.PIPE",
            "CREW.WELD",
            "EQUIP.CRANE",
            "MAT",
            "SAFETY"
        ],
        "no context parents in a flat list, by Resource ID"
    );
    // Header click: Resource ID descending on the second click, an arrow.
    d.click("usage_list_head_profile_0");
    d.frames(2);
    assert_eq!(
        d.app.panes.usage.profile_list.sort.field.as_deref(),
        Some("resource_id")
    );
    assert_eq!(
        codes(&d, true),
        [
            "CREW.ELEC",
            "CREW.PIPE",
            "CREW.WELD",
            "EQUIP.CRANE",
            "MAT",
            "SAFETY"
        ]
    );
    d.click("usage_list_head_profile_0");
    d.frames(2);
    assert!(d.app.panes.usage.profile_list.sort.descending);
    assert_eq!(
        codes(&d, true),
        [
            "SAFETY",
            "MAT",
            "EQUIP.CRANE",
            "CREW.WELD",
            "CREW.PIPE",
            "CREW.ELEC"
        ]
    );
    // Hierarchy keeps the tree and sorts within each level.
    bar_menu(&mut d, "profile", &["group_by", "group_hierarchy"]);
    assert_eq!(
        codes(&d, true),
        [
            "SAFETY",
            "MAT",
            "EQUIP",
            "EQUIP.CRANE",
            "CREW",
            "CREW.WELD",
            "CREW.PIPE",
            "CREW.ELEC"
        ]
    );
    bar_menu(&mut d, "profile", &["group_by", "sort_clear"]);
    assert!(d.app.panes.usage.profile_list.sort.field.is_none());
    // Primary Role groups, "(No Primary Role)" last.
    bar_menu(&mut d, "profile", &["group_by", "group_role"]);
    assert_eq!(
        codes(&d, true),
        [
            "[FITTER - Pipe fitter]",
            "CREW.PIPE",
            "[WELDER - Welder]",
            "CREW.WELD",
            "[(No Primary Role)]",
            "CREW.ELEC",
            "EQUIP.CRANE",
            "MAT",
            "SAFETY"
        ]
    );
    // Resource Type groups.
    bar_menu(&mut d, "profile", &["group_by", "group_type"]);
    assert_eq!(
        codes(&d, true),
        [
            "[Labor]",
            "CREW.ELEC",
            "CREW.PIPE",
            "CREW.WELD",
            "SAFETY",
            "[Nonlabor]",
            "EQUIP.CRANE",
            "[Material]",
            "MAT"
        ]
    );
    // Customize...: by the Area code, descending.
    bar_menu(&mut d, "profile", &["group_by", "group_custom"]);
    let levels = &mut d.app.panes.usage_list_group.as_mut().unwrap().1;
    levels[0] = list::GroupLevel {
        code_type: "Area".into(),
        descending: true,
    };
    d.click("usage_list_group_ok");
    d.frames(2);
    assert_eq!(d.app.panes.usage.profile_list.group, ListGroup::Custom);
    assert_eq!(
        codes(&d, true),
        [
            "[Area: SOUTH]",
            "CREW.WELD",
            "[Area: NORTH]",
            "CREW.PIPE",
            "EQUIP.CRANE",
            "[(No Area)]",
            "CREW.ELEC",
            "MAT",
            "SAFETY"
        ]
    );
    // A group row collapses like a parent.
    let k = row_of(&d, true, "[Area: NORTH]");
    d.click(&format!("usage_list_toggle_profile_{k}"));
    d.frames(2);
    assert!(!codes(&d, true).contains(&"CREW.PIPE".to_string()));
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn columns_are_customized_per_pane_and_persist() {
    let mut d = resource_profile("columns");
    bar_menu(&mut d, "profile", &["columns", "columns_customize"]);
    let draft = d
        .app
        .panes
        .usage_fields
        .as_ref()
        .expect("Spreadsheet Fields");
    let available = usage_fields::available(&d.app.project, draft.which());
    for id in ["calendar", "max_units", "default_units", "price", "rcode_1"] {
        assert!(available.iter().any(|(f, _)| f == id), "{id} offered");
    }
    let draft = d.app.panes.usage_fields.as_mut().unwrap();
    draft.lists[0] = vec![
        "resource_id".into(),
        "rcode_1".into(),
        "default_units".into(),
    ];
    d.click("usage_fields_ok");
    d.frames(2);
    assert!(shows(&mut d, "Area"), "the code column title");
    assert!(shows(&mut d, "NORTH"), "PIPE's Area value");
    // The spreadsheet's list is unchanged.
    assert_eq!(
        d.app.panes.usage.resource_fields,
        usage_fields::default_ids(usage_fields::Which::Resource)
    );
    d.app.save_layout_as("Columns").unwrap();
    let saved = d.app.panes.usage.profile_fields.clone();
    d.app.panes.usage = Default::default();
    d.app.open_layout("Columns", true).unwrap();
    assert_eq!(d.app.panes.usage.profile_fields, saved);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn several_resources_and_a_group_show_their_sum() {
    let mut d = workspace("sums");
    d.app.set_bottom(Some(BottomView::ResourceUsageSheet));
    d.frames(3);
    click_row(&mut d, false, "CREW.PIPE", Default::default());
    click_row(&mut d, false, "CREW.WELD", egui::Modifiers::CTRL);
    assert_eq!(
        d.app.panes.usage.selection(),
        [ListKey::Resource(101), ListKey::Resource(102)]
    );
    let periods = d.app.panes.sheet_periods.clone();
    let rows = sheet_rows(&d);
    // Total, PIPE's heading and assignment, WELD's heading and assignment.
    assert_eq!(rows.len(), 5);
    assert!(rows[0].band && rows[1].sub && rows[3].sub);
    assert!((rows[0].total - 40.).abs() < 1e-9, "24 + 16 = 40");
    assert_eq!(rows[0].texts[0], "2 resources");
    for k in 0..periods.len() {
        let sum = rows[1].values[k] + rows[3].values[k];
        assert!((rows[0].values[k] - sum).abs() < 1e-9, "period {k}");
    }
    let shown: f64 = rows[0].values.iter().sum();
    assert!(shown > 0., "the periods hold the remaining units");
    let body = d.app.panes.body;
    assert!(pane_shows(&d.frame(vec![]), &format::units(40.), body));
    // Shift-click: the range from the row clicked last (PIPE..ELEC).
    click_row(&mut d, false, "CREW.PIPE", Default::default());
    click_row(&mut d, false, "CREW.ELEC", egui::Modifiers::SHIFT);
    assert_eq!(d.app.panes.usage.selection().len(), 3);
    // Ctrl-click takes one out again.
    click_row(&mut d, false, "CREW.ELEC", egui::Modifiers::CTRL);
    assert_eq!(d.app.panes.usage.selection().len(), 2);
    // A group row stands for its members: Type > Labor = PIPE, WELD, ELEC
    // (NORTHYARD only, nothing here) and SAFETY (no assignment): 40 again.
    bar_menu(&mut d, "sheet", &["group_by", "group_type"]);
    click_row(&mut d, false, "[Labor]", Default::default());
    let rows = sheet_rows(&d);
    assert_eq!(rows[0].texts[0], "Labor");
    assert!((rows[0].total - 40.).abs() < 1e-9);
    // Profile: the group's bars are the sum of its members' bars.
    d.app.panes.usage.profile_list = d.app.panes.usage.sheet_list.clone();
    d.app.set_bottom(Some(BottomView::ResourceUsageProfile));
    d.frames(3);
    let group = loaded(&d.frame(vec![]), body);
    d.app.panes.usage.profile_list.group = ListGroup::List;
    d.frames(2);
    click_row(&mut d, true, "CREW.PIPE", Default::default());
    click_row(&mut d, true, "CREW.WELD", egui::Modifiers::CTRL);
    let pair = loaded(&d.frame(vec![]), body);
    assert!(!pair.is_empty());
    assert_eq!(group, pair, "the same bars");
    click_row(&mut d, true, "CREW.PIPE", Default::default());
    let one = loaded(&d.frame(vec![]), body);
    assert_ne!(one, pair, "one resource draws less");
    // Labor with material: no summed bars and a note; costs do add up.
    click_row(&mut d, true, "MAT", egui::Modifiers::CTRL);
    let mixed = d.frame(vec![]);
    assert!(loaded(&mixed, body).is_empty());
    assert!(shows_part(&mut d, "Material units do not add"));
    d.app.panes.usage.costs = true;
    d.frames(2);
    assert!(!shows_part(&mut d, "Material units do not add"));
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn the_role_view_lists_roles_and_their_staffed_and_unstaffed_demand() {
    let mut d = resource_profile("roles");
    bar_menu(&mut d, "profile", &["select_view", "view_role"]);
    assert_eq!(d.app.panes.usage.profile_list.view, ListView::Role);
    assert!(shows(&mut d, "Display: Current Project's Roles"));
    assert!(shows(&mut d, "Role ID"));
    // PAINTER has no assignment; TRADES is their parent.
    assert_eq!(codes(&d, true), ["TRADES", "FITTER", "WELDER"]);
    assert_eq!(context_codes(&d, true), ["TRADES"]);
    click_row(&mut d, true, "FITTER", Default::default());
    assert!(shows(&mut d, "Unstaffed Remaining Units"));
    assert!(shows(&mut d, "Staffed Remaining Units"));
    let body = d.app.panes.body;
    let out = d.frame(vec![]);
    assert!(body_fills(&out, usage::UNSTAFFED.0, body) > 0);
    assert!(body_fills(&out, usage::REMAINING.0, body) > 0);
    // FITTER: PIPE's 24 h (staffed) and 10 h nobody is named for.
    let periods: Vec<(i64, i64)> = vec![(i64::MIN / 4, i64::MAX / 4)];
    let demand = list::role_demand(&d.app.project, &BTreeSet::from([11]), &periods, false);
    assert!((demand.staffed[0] - 24.).abs() < 1e-9, "{demand:?}");
    assert!((demand.unstaffed[0] - 10.).abs() < 1e-9, "{demand:?}");
    // The spreadsheet: the role's row totals both, then its assignments.
    d.app.panes.usage.sheet_list.view = ListView::Role;
    d.app.set_bottom(Some(BottomView::ResourceUsageSheet));
    d.frames(3);
    click_row(&mut d, false, "FITTER", Default::default());
    let rows = sheet_rows(&d);
    assert_eq!(rows.len(), 3);
    assert!((rows[0].total - 34.).abs() < 1e-9);
    // All Roles adds PAINTER.
    d.app.panes.usage.sheet_list.filter = ListFilter::All;
    assert_eq!(codes(&d, false), ["TRADES", "FITTER", "WELDER", "PAINTER"]);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn display_activities_for_selected_filters_the_table_and_clears() {
    let mut d = resource_profile("display-activities");
    let all = d.app.rows().len();
    click_row(&mut d, true, "CREW.PIPE", Default::default());
    d.click("usage_list_profile_resource");
    d.frames(2);
    assert!(d.app.panes.usage.resource_filter);
    let pipe = index_of(&d.app.project, 1010);
    let activities = |d: &Driver| -> Vec<usize> {
        d.app
            .rows()
            .into_iter()
            .filter_map(|r| match r {
                grid::Row::Activity(i) => Some(i),
                _ => None,
            })
            .collect()
    };
    assert_eq!(activities(&d), [pipe]);
    // The Filter caption says so, in the layout bar, with Clear.
    assert!(shows_part(&mut d, "Usage: activities of CREW.PIPE"));
    assert!(d.has("filter_usage_clear"));
    // Ctrl adds the welders: two activities.
    click_row(&mut d, true, "CREW.WELD", egui::Modifiers::CTRL);
    assert_eq!(activities(&d).len(), 2);
    assert!(shows_part(&mut d, "activities of 2 selected"));
    // Time Period: a click on the profile picks a period; only activities
    // working then stay.
    d.click("usage_list_profile_time_period");
    d.app.panes.usage.interval = usage::Interval::Day;
    d.frames(2);
    let start = d.app.project.model.activities[pipe].early_start.unwrap();
    let chart = d.rect("usage_profile_timescale_body");
    let axis = d.app.usage_axis_for_tests(chart.left());
    d.click_at(Pos2::new(axis.x(start as f64 + 60.), chart.center().y));
    d.frames(2);
    let period = d.app.panes.usage.period.expect("a picked period");
    assert!(period.0 <= start && start < period.1);
    assert_eq!(activities(&d), [pipe], "the welders work later");
    assert!(shows_part(&mut d, "working "));
    // Clear shows every activity again.
    d.click("filter_usage_clear");
    d.frames(2);
    assert!(!d.app.panes.usage.resource_filter && !d.app.panes.usage.time_period);
    assert_eq!(d.app.rows().len(), all);
    assert!(!d.has("filter_usage_clear"));
    // Off while Show All Projects is on.
    d.app.panes.usage.resource_filter = true;
    d.app.panes.usage.show_all_projects = true;
    d.frames(2);
    assert_eq!(d.app.rows().len(), all, "no filter with all projects");
    assert!(d.app.usage_filter_caption().is_none());
    d.click("usage_list_profile_resource");
    assert!(d.app.panes.usage.resource_filter, "the box is disabled");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn every_list_choice_survives_a_layout_save_and_reopen() {
    let mut d = resource_profile("layout");
    let u = &mut d.app.panes.usage;
    u.profile_list.filter = ListFilter::Custom;
    u.profile_list.custom.text = "crew".into();
    u.profile_list.custom.material = false;
    u.profile_list.custom.code_type = "Area".into();
    u.profile_list.group = ListGroup::Custom;
    u.profile_list.levels = vec![list::GroupLevel {
        code_type: "Area".into(),
        descending: true,
    }];
    u.profile_list.sort = usage::SheetSort {
        field: Some("resource_name".into()),
        descending: true,
    };
    u.sheet_list.filter = ListFilter::All;
    u.sheet_list.group = ListGroup::Role;
    u.sheet_list.view = ListView::Role;
    u.profile_fields = vec!["resource_id".into(), "price".into()];
    let (profile, sheet) = (u.profile_list.clone(), u.sheet_list.clone());
    d.app.save_layout_as("Lists").unwrap();
    d.app.panes.usage = Default::default();
    assert_eq!(d.app.panes.usage.profile_list.filter, ListFilter::Project);
    d.app.open_layout("Lists", true).unwrap();
    assert_eq!(d.app.panes.usage.profile_list, profile);
    assert_eq!(d.app.panes.usage.sheet_list, sheet);
    assert_eq!(d.app.panes.usage.profile_fields, ["resource_id", "price"]);
    // The workspace copy of the panes too.
    let mut again = PanesState::default();
    again.restore(&d.app.panes.json());
    assert_eq!(again.usage.profile_list, profile);
    // A layout saved before: its one resource sort goes to both lists and
    // the lists start with the current project's resources.
    let mut old = PanesState::default();
    old.restore(&serde_json::json!({"usage": {"sort": {"resource":
        {"field": "resource_name", "descending": true}}}}));
    for l in [&old.usage.profile_list, &old.usage.sheet_list] {
        assert_eq!(l.sort.field.as_deref(), Some("resource_name"));
        assert!(l.sort.descending);
        assert_eq!(l.filter, ListFilter::Project);
    }
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

/// The Resource Usage Spreadsheet's rows for the selection.
fn sheet_rows(d: &Driver) -> std::rc::Rc<Vec<usage::ResRow>> {
    let afields = usage_fields::fields_of(
        &d.app.project,
        usage_fields::Which::Assignment,
        &d.app.panes.usage.assignment_fields,
    );
    d.app
        .selection_sheet_rows(&afields, &d.app.panes.sheet_periods)
        .expect("a selection")
}

fn bar_rects(output: &egui::FullOutput, color: Color32) -> Vec<Rect> {
    output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Rect(r) if r.fill == color => Some(r.rect),
            _ => None,
        })
        .collect()
}

/// The remaining and overallocated bars inside the bottom pane.
fn loaded(output: &egui::FullOutput, body: Rect) -> Vec<Rect> {
    let mut rects = bar_rects(output, usage::REMAINING.0);
    rects.extend(bar_rects(output, usage::OVER.0));
    rects.retain(|r| body.contains_rect(*r));
    rects
}

fn body_fills(output: &egui::FullOutput, color: Color32, body: Rect) -> usize {
    bar_rects(output, color)
        .iter()
        .filter(|r| body.contains_rect(**r))
        .count()
}

fn pane_shows(output: &egui::FullOutput, text: &str, body: Rect) -> bool {
    output.shapes.iter().any(|s| {
        matches!(&s.shape, egui::Shape::Text(t)
            if t.galley.text() == text && body.contains(t.pos))
    })
}

fn index_of(p: &ProjectData, task_id: i64) -> usize {
    p.model
        .activities
        .iter()
        .position(|a| a.task_id == task_id)
        .unwrap()
}

/// A render of the whole window on a fresh context (so it sees every
/// texture), the pointer away; also saved to `artifacts/usage-list/`.
fn render(d: &mut Driver, name: &str) -> image::RgbaImage {
    let render = egui::Context::default();
    crate::chrome::configure(&render);
    render.set_pixels_per_point(1.);
    let mut painter = crate::headless_render::SoftwarePainter::default();
    for _ in 0..3 {
        let gone = vec![egui::Event::PointerGone];
        let _ = painter.frame(&mut d.app, &render, &mut d.time, gone, false);
    }
    let (image, _) = painter
        .frame(&mut d.app, &render, &mut d.time, vec![], true)
        .unwrap();
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/usage-list");
    std::fs::create_dir_all(&out).unwrap();
    image.save(out.join(format!("{name}.png"))).unwrap();
    image
}

fn pixel(image: &image::RgbaImage, x: f32, y: f32) -> [u8; 3] {
    let p = image.get_pixel(x as u32, y as u32).0;
    [p[0], p[1], p[2]]
}

/// Pixels down the column at `x` from `top` to `bottom` that are not white.
fn inked(image: &image::RgbaImage, x: f32, top: f32, bottom: f32) -> Vec<(f32, [u8; 3])> {
    let mut out = vec![];
    let mut y = top.ceil();
    while y < bottom {
        let p = pixel(image, x, y);
        if p != [255; 3] {
            out.push((y, p));
        }
        y += 1.;
    }
    out
}

/// The complaint: with the splitter dragged wider than the list's columns,
/// the row lines ran on across the empty space. They stop at the last
/// column, as in the Activities table, and so do the selected row's fill.
#[test]
fn row_lines_stop_at_the_last_column_like_the_activity_table() {
    let mut d = resource_profile("row-lines");
    d.app.panes.usage.table_width = Some(1000.);
    d.frames(3);
    let image = render(&mut d, "10-profile-splitter-wide");
    // The reference: right of the Activities table's last column, white.
    let heads: Vec<Rect> = d
        .app
        .action_rects
        .keys()
        .filter(|k| k.starts_with("grid_header_"))
        .map(|k| d.rect(k))
        .collect();
    let last = heads.iter().map(|r| r.right()).fold(f32::MIN, f32::max);
    let top = heads[0].bottom() + 2.;
    let splitter = d.rect("splitter");
    assert!(
        splitter.left() - last > 20.,
        "room right of the columns: {last} {splitter:?}"
    );
    assert!(
        inked(&image, last + 15., top, top + 200.).is_empty(),
        "the Activities table paints nothing right of its last column"
    );
    assert!(!inked(&image, last - 3., top, top + 200.).is_empty());
    // The profile's list: the same.
    let titles: Vec<Rect> = (0..)
        .map(|c| format!("usage_list_head_profile_{c}"))
        .take_while(|k| d.has(k))
        .map(|k| d.rect(&k))
        .collect();
    assert_eq!(titles.len(), 3, "Resource ID, Name and Type");
    let last = titles.iter().map(|r| r.right()).fold(f32::MIN, f32::max);
    let rows: Vec<Rect> = (0..)
        .map(|k| format!("usage_resource_{k}"))
        .take_while(|k| d.has(k))
        .map(|k| d.rect(&k))
        .collect();
    assert!(rows.len() >= 4, "{rows:?}");
    let (top, bottom) = (rows[0].top(), rows[rows.len() - 1].bottom());
    assert!(
        rows[0].right() - last > 400.,
        "the list is wider than its columns"
    );
    for x in [last + 3., last + 40., rows[0].right() - 40.] {
        assert!(
            inked(&image, x, top, bottom).is_empty(),
            "no row line or fill right of the last column at x {x}"
        );
    }
    // Inside the last column every row has its #C0C0C0 line.
    let lines = inked(&image, last - 3., top, bottom)
        .into_iter()
        .filter(|(_, p)| *p == [0xC0; 3])
        .count();
    assert!(
        lines >= rows.len() - 1,
        "{lines} lines for {} rows",
        rows.len()
    );
    // The spreadsheet's list too, with line numbers on.
    d.app.panes.usage.line_numbers = true;
    d.app.set_bottom(Some(BottomView::ResourceUsageSheet));
    d.app.panes.usage.list_width = Some(800.);
    d.frames(3);
    let image = render(&mut d, "11-sheet-splitter-wide");
    let titles: Vec<Rect> = (0..)
        .map(|c| format!("usage_list_head_sheet_{c}"))
        .take_while(|k| d.has(k))
        .map(|k| d.rect(&k))
        .collect();
    let last = titles.iter().map(|r| r.right()).fold(f32::MIN, f32::max);
    let first = d.rect("usage_resource_0");
    assert!(
        titles[0].left() - first.left() >= 29.,
        "the line number column"
    );
    let numbered = d
        .texts()
        .into_iter()
        .any(|(t, r)| t == "1" && r.center().x < titles[0].left() && r.center().y < first.bottom());
    assert!(numbered, "row 1 is numbered");
    let row = d.rect("usage_resource_3");
    assert!(inked(&image, last + 20., first.top(), row.bottom()).is_empty());
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

/// The wheel scrolls the list; each pane's list keeps its offset while the
/// other pane is shown.
#[test]
fn the_wheel_scrolls_the_list_and_each_pane_keeps_its_offset() {
    let mut d = resource_profile("scroll");
    // A short window and every resource: more rows than the list shows.
    d.screen = Vec2::new(1500., 760.);
    d.app.panes.usage.profile_list.filter = ListFilter::All;
    d.frames(3);
    let at = d.rect("usage_resource_1").center();
    let _ = d.frame(vec![
        egui::Event::PointerMoved(at),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0., -40.),
            modifiers: Default::default(),
        },
    ]);
    // The wheel's smoothing spends the delta over a few frames.
    d.frames(30);
    let scrolled = d.app.panes.usage.list_scroll[0];
    assert!(
        scrolled > 0.,
        "{scrolled} at {at:?}; rows {:?} {:?}; list {:?}",
        d.rect("usage_resource_0"),
        d.app.action_rects.get("usage_resource_7"),
        d.app.action_rects.get("usage_list_profile_resource")
    );
    d.app.set_bottom(Some(BottomView::ResourceUsageSheet));
    d.frames(3);
    assert_eq!(d.app.panes.usage.list_scroll[1], 0.);
    d.app.set_bottom(Some(BottomView::ResourceUsageProfile));
    d.frames(3);
    assert_eq!(d.app.panes.usage.list_scroll[0], scrolled);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

/// Column edges drag like every shared table's, and the width is kept
/// with the layout.
#[test]
fn a_dragged_column_edge_keeps_its_width_and_survives_the_layout() {
    let mut d = resource_profile("resize");
    let edge = d.rect("usage_list_profile_resize_0").center();
    let before = d.rect("usage_list_head_profile_0").width();
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let end = edge + Vec2::new(60., 0.);
    let _ = d.frame(vec![egui::Event::PointerMoved(edge)]);
    let _ = d.frame(vec![button(edge, true)]);
    for step in 1..=6 {
        let at = edge + Vec2::new(10. * step as f32, 0.);
        let _ = d.frame(vec![egui::Event::PointerMoved(at)]);
    }
    let _ = d.frame(vec![button(end, false)]);
    d.frames(2);
    let after = d.rect("usage_list_head_profile_0").width();
    assert!(after > before + 30., "{before} -> {after}");
    let kept = d.app.panes.usage.list_widths[0]["resource_id"];
    let mut again = usage::UsageState::default();
    again.restore(&d.app.panes.usage.json());
    assert_eq!(again.list_widths[0]["resource_id"], kept);
    assert!(
        again.list_widths[1].is_empty(),
        "the spreadsheet's list keeps its own"
    );
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}
#[test]
fn render_the_resource_list() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/usage-list");
    std::fs::create_dir_all(&out).unwrap();
    let shot = |d: &mut Driver, name: &str| {
        d.frames(2);
        d.screenshot(&out.join(format!("{name}.png")));
    };
    // 1. The filtered tree: current project's resources, greyed parents.
    let mut d = workspace_probed("render", crate::dialog_sweep::Probe::on(Some(out.clone())));
    d.app.set_bottom(Some(BottomView::ResourceUsageProfile));
    d.frames(3);
    d.app.panes.usage.profile_fields = vec![
        "resource_id".into(),
        "resource_name".into(),
        "resource_type".into(),
    ];
    click_row(&mut d, true, "CREW.PIPE", Default::default());
    shot(&mut d, "01-filtered-tree");
    d.click("usage_list_bar_profile");
    d.frames(1);
    let at = d.rect("usage_list_bar_profile_filter_by").center();
    let _ = d.frame(vec![egui::Event::PointerMoved(at)]);
    shot(&mut d, "02-display-bar-menu");
    d.key(egui::Key::Escape);
    // 2. Group by role.
    d.app.panes.usage.profile_list.group = ListGroup::Role;
    d.frames(2);
    click_row(&mut d, true, "[FITTER - Pipe fitter]", Default::default());
    shot(&mut d, "03-group-by-role");
    // 3. Multi-select profile: PIPE and WELD summed.
    d.app.panes.usage.profile_list.group = ListGroup::Hierarchy;
    d.frames(2);
    click_row(&mut d, true, "CREW.PIPE", Default::default());
    click_row(&mut d, true, "CREW.WELD", egui::Modifiers::CTRL);
    shot(&mut d, "04-multi-select-profile");
    // 4. Display Activities for selected Resource: the table filtered, the
    // Filter caption and Clear.
    d.click("usage_list_profile_resource");
    shot(&mut d, "05-display-activities-filter");
    // 5. The role view's demand.
    d.app.panes.usage.resource_filter = false;
    d.app.panes.usage.profile_list.view = ListView::Role;
    d.frames(2);
    click_row(&mut d, true, "FITTER", Default::default());
    shot(&mut d, "06-role-demand");
    // 6. The spreadsheet with a summed selection.
    d.app.set_bottom(Some(BottomView::ResourceUsageSheet));
    d.frames(3);
    click_row(&mut d, false, "CREW.PIPE", Default::default());
    click_row(&mut d, false, "CREW.WELD", egui::Modifiers::CTRL);
    shot(&mut d, "07-spreadsheet-sum");
    // 7. The two dialogs.
    bar_menu(&mut d, "sheet", &["filter_by", "filter_custom"]);
    shot(&mut d, "08-resource-filter-dialog");
    d.key(egui::Key::Escape);
    bar_menu(&mut d, "sheet", &["group_by", "group_custom"]);
    shot(&mut d, "09-group-by-code-dialog");
    d.key(egui::Key::Escape);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}
