//! Assign on the Codes tab (and Assign > Activity Codes from the table) when
//! the project has no activity codes: the Activity Codes dictionary opens,
//! and closing it with a value to assign opens the code picker.
use super::headless_tests::{cleanup, click, frame, open_app, temp_db};
use super::*;
use crate::dictionaries::{ops, Cmd};

/// A starter project with every activity code removed, on the Codes tab.
fn no_codes(name: &str) -> (egui::Context, App, f64, std::path::PathBuf) {
    let path = temp_db(name);
    let (ctx, mut app, mut time) = open_app(&path);
    for table in ["ACTVTYPE", "ACTVCODE", "TASKACTV"] {
        if app.project.document.table(table).is_some() {
            app.project.document.table_mut(table).rows.clear();
        }
    }
    app.selected = Some(0);
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "detail_tab_codes", &mut time);
    (ctx, app, time, path)
}

fn type_count(app: &App) -> usize {
    app.project
        .document
        .table("ACTVTYPE")
        .map_or(0, |t| t.rows.len())
}

fn assigned(app: &App) -> usize {
    app.project
        .document
        .table("TASKACTV")
        .map_or(0, |t| t.rows.len())
}

/// Add a project code type and one value as the dictionary's own buttons
/// do, each in its own frame (one undo step each), then Close it.
fn add_type_and_value(app: &mut App, ctx: &egui::Context, time: &mut f64) -> (i64, i64) {
    let kind = ops::CodeKind::Activity;
    let type_id = app
        .run_dictionary(Cmd::AddType(kind, ops::Scope::Project))
        .unwrap()
        .unwrap();
    let _ = frame(app, ctx, vec![], time);
    let value = app
        .run_dictionary(Cmd::AddValue(kind, type_id, None))
        .unwrap()
        .unwrap();
    let _ = frame(app, ctx, vec![], time);
    click(app, ctx, "codes_close", time);
    (type_id, value)
}

#[test]
fn assign_with_no_codes_opens_the_dictionary_then_the_picker() {
    let (ctx, mut app, mut time, path) = no_codes("codes-assign");
    let start = app.history.counts().0;
    click(&mut app, &ctx, "detail_codes_assign", &mut time);
    let cs = app.dictionaries.codes.as_ref().expect("dictionary opens");
    assert!(cs.hint.is_some(), "the dialog says what to do");
    assert_eq!(
        cs.scope,
        ops::Scope::Project,
        "opens on the project's codes"
    );
    assert!(app.dictionaries.notice.is_none());
    assert!(app.detail.picker.is_none());

    let (type_id, value) = add_type_and_value(&mut app, &ctx, &mut time);
    assert!(app.dictionaries.codes.is_none(), "Close closes it");
    assert!(
        matches!(app.detail.picker, Some(Picker::Code { .. })),
        "closing with a value opens the picker"
    );
    app.detail.picker = Some(Picker::Code {
        type_id: Some(type_id),
        selected: Some(value),
    });
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "picker_assign", &mut time);
    assert_eq!(assigned(&app), 1);
    assert_eq!(
        app.history.counts().0,
        start + 3,
        "type, value and assignment are one step each"
    );
    click(&mut app, &ctx, "picker_close", &mut time);
    for _ in 0..3 {
        app.history_move(&ctx, false);
    }
    assert_eq!(assigned(&app), 0);
    assert_eq!(type_count(&app), 0, "three undos return to the start");

    // The picker's Activity Codes... link opens the dictionary on the type
    // and comes back to the picker when it closes.
    for _ in 0..3 {
        app.history_move(&ctx, true);
    }
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "detail_codes_assign", &mut time);
    assert!(matches!(app.detail.picker, Some(Picker::Code { .. })));
    assert!(app.dictionaries.codes.is_none());
    click(&mut app, &ctx, "picker_code_dictionary", &mut time);
    assert!(app.detail.picker.is_none());
    assert_eq!(
        app.dictionaries.codes.as_ref().and_then(|c| c.type_id),
        Some(type_id)
    );
    click(&mut app, &ctx, "codes_close", &mut time);
    assert!(matches!(app.detail.picker, Some(Picker::Code { .. })));
    drop(app);
    cleanup(path);
}

#[test]
fn closing_the_dictionary_with_no_codes_opens_nothing() {
    let (ctx, mut app, mut time, path) = no_codes("codes-none");
    click(&mut app, &ctx, "detail_codes_assign", &mut time);
    assert!(app.dictionaries.codes.is_some());
    click(&mut app, &ctx, "codes_close", &mut time);
    assert!(app.dictionaries.codes.is_none());
    assert!(app.detail.picker.is_none());
    assert!(!app.detail.assign_codes_after);
    drop(app);
    cleanup(path);
}

#[test]
fn table_assign_activity_codes_takes_the_same_path() {
    let (ctx, mut app, mut time, path) = no_codes("codes-table");
    app.editing.pending = Some(crate::editing::Command::Assign(DetailTab::Codes));
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    assert!(app.dictionaries.codes.is_some(), "dictionary opens");
    assert!(app.detail.picker.is_none());
    let (type_id, value) = add_type_and_value(&mut app, &ctx, &mut time);
    assert!(matches!(app.detail.picker, Some(Picker::Code { .. })));
    app.detail.picker = Some(Picker::Code {
        type_id: Some(type_id),
        selected: Some(value),
    });
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "picker_assign", &mut time);
    assert_eq!(assigned(&app), 1);
    drop(app);
    cleanup(path);
}

#[test]
fn the_picker_lists_global_eps_and_project_types() {
    let (ctx, mut app, mut time, path) = no_codes("codes-scopes");
    click(&mut app, &ctx, "detail_codes_assign", &mut time);
    let kind = ops::CodeKind::Activity;
    let mut ids = vec![];
    for scope in [ops::Scope::Global, ops::Scope::Eps, ops::Scope::Project] {
        let t = app
            .run_dictionary(Cmd::AddType(kind, scope))
            .unwrap()
            .unwrap();
        app.run_dictionary(Cmd::AddValue(kind, t, None)).unwrap();
        ids.push(t);
        let _ = frame(&mut app, &ctx, vec![], &mut time);
    }
    click(&mut app, &ctx, "codes_close", &mut time);
    assert!(matches!(app.detail.picker, Some(Picker::Code { .. })));
    let pid = app.project.model.project.proj_id;
    let listed: Vec<i64> = app.code_types(pid).into_iter().map(|(id, _)| id).collect();
    for t in ids {
        assert!(listed.contains(&t), "type {t} missing from {listed:?}");
    }
    drop(app);
    cleanup(path);
}
