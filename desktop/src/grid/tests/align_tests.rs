//! Default column alignment: date columns start Left
//! everywhere a default is chosen, text Left, flags Center, numbers Right.
//! A layout stores only an alignment that differs from the default, so
//! saved layouts pick up the new date default with no migration, and a
//! Right set in Edit Column on a date column is stored and kept.
use super::*;
use crate::grid::columns::{default_align, Align, Column};

/// The rich starter with a number and a date user defined field.
fn project_with_udfs() -> ProjectData {
    let mut p = rich_starter();
    for (id, label, kind) in [(10, "Crew Size", "FT_INT"), (11, "Permit", "FT_END_DATE")] {
        p.document.table_mut("UDFTYPE").add(&[
            ("udf_type_id", id.to_string()),
            ("table_name", "TASK".into()),
            ("udf_type_name", format!("user_field_{id}")),
            ("udf_type_label", label.into()),
            ("logical_data_type", kind.into()),
        ]);
    }
    p
}

#[test]
fn only_dates_moved_left_every_other_kind_keeps_its_default() {
    assert_eq!(default_align(Kind::Text), Align::Left);
    assert_eq!(default_align(Kind::Date), Align::Left);
    assert_eq!(default_align(Kind::Flag), Align::Center);
    for kind in [
        Kind::Duration,
        Kind::Percent,
        Kind::Units,
        Kind::Cost,
        Kind::Number,
    ] {
        assert_eq!(default_align(kind), Align::Right, "{kind:?}");
    }
}

#[test]
fn every_catalog_column_starts_with_its_kinds_default() {
    let p = project_with_udfs();
    let catalog = columns::catalog(&p);
    for c in &catalog {
        assert_eq!(c.align, default_align(c.kind), "{} ({:?})", c.id, c.kind);
    }
    let find = |id: &str| -> &Column {
        columns::find(&catalog, id).unwrap_or_else(|| panic!("{id} in the catalog"))
    };
    // Built-in, user defined, baseline and extended date columns.
    for id in [
        "start",
        "finish",
        "udf:Permit",
        "bl1_start",
        "bl_project_finish",
    ] {
        let c = find(id);
        assert_eq!((c.kind, c.align), (Kind::Date, Align::Left), "{id}");
    }
    let ext_dates = catalog
        .iter()
        .filter(|c| c.kind == Kind::Date && c.group != columns::USER_DEFINED)
        .count();
    assert!(ext_dates > 10, "dates across the registry: {ext_dates}");
    // Numbers stay right, flags center.
    assert_eq!(find("udf:Crew Size").align, Align::Right);
    assert_eq!(find("total_float").align, Align::Right);
    assert_eq!(find("remaining_duration").align, Align::Right);
    let flag = catalog.iter().find(|c| c.kind == Kind::Flag).unwrap();
    assert_eq!(flag.align, Align::Center, "{}", flag.id);
}

#[test]
fn saved_layouts_follow_the_new_date_default_and_keep_set_alignments() {
    let (path, _ctx, mut app, _time) = setup_with("align-saved", rich_starter(), false);
    // A layout as the old build saved it: no alignment on any column,
    // because the dialog stores none when the choice equals the default.
    let mut old = layout::Layout::default().to_json();
    for c in old["columns"].as_array().unwrap() {
        assert!(c["align"].is_null(), "{c}");
    }
    // One deliberate Center on a number column and one Right on a date
    // column (a user choice after this change) survive as they are.
    old["columns"][6]["align"] = serde_json::json!("center");
    old["columns"][5]["align"] = serde_json::json!("right");
    app.grid.layout = layout::Layout::from_json(&old).unwrap();
    let catalog = columns::catalog(&app.project);
    let shown: Vec<(String, Align)> = app
        .visible_columns(&catalog)
        .into_iter()
        .map(|c| (c.id, c.align))
        .collect();
    let at = |id: &str| shown.iter().find(|(i, _)| i == id).unwrap().1;
    assert_eq!(at("start"), Align::Left);
    assert_eq!(at("finish"), Align::Right, "a stored Right is kept");
    assert_eq!(at("total_float"), Align::Center, "a stored Center is kept");
    assert_eq!(at("free_float"), Align::Right);
    assert_eq!(at("remaining_duration"), Align::Right);
    assert_eq!(at("activity_status"), Align::Left);
    let back = layout::Layout::from_json(&app.grid.layout.to_json()).unwrap();
    assert_eq!(back, app.grid.layout);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn edit_column_on_a_date_offers_left_and_stores_right_only_when_picked() {
    let (path, ctx, mut app, mut time) = setup("align-edit", rich_starter());
    click(&mut app, &ctx, "layout_button", &mut time);
    click(&mut app, &ctx, "menu_columns", &mut time);
    let at = app
        .grid
        .dialogs
        .columns
        .as_ref()
        .unwrap()
        .columns
        .iter()
        .position(|c| c.id == "start")
        .unwrap();
    let stored = |app: &App| app.grid.dialogs.columns.as_ref().unwrap().columns[at].align;
    let edit = |app: &mut App, time: &mut f64, pick: &str| {
        click(app, &ctx, &format!("columns_selected_{at}"), time);
        click(app, &ctx, "columns_edit", time);
        assert!(app.grid.dialogs.edit_column.is_some());
        if pick.is_empty() {
            assert_eq!(
                app.grid.dialogs.edit_column.as_ref().unwrap().align,
                Align::Left,
                "the draft shows the date default"
            );
        } else {
            click(app, &ctx, pick, time);
        }
        click(app, &ctx, "edit_column_ok", time);
    };
    edit(&mut app, &mut time, "");
    assert_eq!(stored(&app), None, "OK on the default stores nothing");
    edit(&mut app, &mut time, "edit_column_align_right");
    assert_eq!(stored(&app), Some(Align::Right));
    edit(&mut app, &mut time, "edit_column_default");
    assert_eq!(stored(&app), None, "Default puts the date back to Left");
    edit(&mut app, &mut time, "edit_column_align_right");
    click(&mut app, &ctx, "columns_ok", &mut time);
    let catalog = columns::catalog(&app.project);
    let start = app
        .visible_columns(&catalog)
        .into_iter()
        .find(|c| c.id == "start")
        .unwrap();
    assert_eq!(start.align, Align::Right);
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}
