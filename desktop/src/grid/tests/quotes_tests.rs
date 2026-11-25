//! Quote marks in names: the file writes each one doubled, the table, find
//! and replace and the workspace see one, and export writes the doubled form.
use super::*;

/// The starter project written to file text with two quoted activity names
/// and read back, as opening a file that holds them does.
fn project_from_file_with_quotes() -> (ProjectData, String) {
    let mut p = scheduled_starter();
    p.details.get_mut(&1000).unwrap().name = "Set 8\" valve \"V-1\"".into();
    p.details.get_mut(&1010).unwrap().name = "\"Rough in\" check".into();
    let text = p.updated_document().text();
    let mut q = ProjectData::from_document(
        openxer_xer::Document::parse(&text).unwrap(),
        1,
        "q.xer".into(),
    )
    .unwrap();
    schedule_model(&mut q.model, &q.resources).unwrap();
    (q, text)
}

#[test]
fn the_table_find_and_replace_and_the_workspace_use_single_quote_marks() {
    let (p, file) = project_from_file_with_quotes();
    assert!(
        file.contains("\tSet 8\"\" valve \"\"V-1\"\"\t"),
        "the file form is doubled"
    );
    assert!(file.contains("\t\"\"Rough in\"\" check\t"));
    let first = index_of(&p, 1000);
    assert_eq!(p.name(first), "Set 8\" valve \"V-1\"");
    let ix = columns::Index::new((false, 0.));
    let catalog = columns::catalog(&p);
    let name = columns::find(&catalog, "activity_name").unwrap();
    assert_eq!(
        columns::display(&p, Some(first), &columns::value(&p, &ix, name, first)),
        "Set 8\" valve \"V-1\""
    );

    let (path, ctx, mut app, mut time) = setup("quotes", p);
    let ctrl = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    // Find what a person types: one quote mark.
    key(&mut app, &ctx, Key::F, ctrl, &mut time);
    click(&mut app, &ctx, "find_what", &mut time);
    type_text(&mut app, &ctx, "8\" valve", &mut time);
    click(&mut app, &ctx, "find_next", &mut time);
    assert_eq!(app.selected, Some(first), "{}", app.status);
    // The doubled form is not what the table holds.
    click(&mut app, &ctx, "find_what", &mut time);
    select_all(&mut app, &ctx, &mut time);
    type_text(&mut app, &ctx, "8\"\" valve", &mut time);
    click(&mut app, &ctx, "find_next", &mut time);
    assert!(app.status.contains("not found"), "{}", app.status);
    click(&mut app, &ctx, "find_close", &mut time);

    // Replace keeps the quote marks it does not touch; export doubles them.
    key(&mut app, &ctx, Key::H, ctrl, &mut time);
    let f = app.grid.dialogs.find.as_mut().unwrap();
    f.what = "valve".into();
    f.with = "gate valve".into();
    f.column = Some("activity_name".into());
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "replace_all", &mut time);
    assert_eq!(app.project.name(first), "Set 8\" gate valve \"V-1\"");
    let exported = app.project.updated_document().text();
    assert!(
        exported.contains("\tSet 8\"\" gate valve \"\"V-1\"\"\t"),
        "{exported}"
    );
    assert!(exported.contains("\t\"\"Rough in\"\" check\t"));

    // The workspace keeps the XER text and brings back single quote marks.
    app.store.save(&app.project).unwrap();
    let id = app.store.id_of("q.xer", 1).unwrap();
    let stored: String = app
        .store
        .connection()
        .query_row("SELECT xer_content FROM projects WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(stored.contains("\tSet 8\"\" gate valve \"\"V-1\"\"\t"));
    let loaded = app.store.load(id).unwrap();
    assert_eq!(loaded.name(first), "Set 8\" gate valve \"V-1\"");
    drop(app);
    let _ = std::fs::remove_file(path);
}
