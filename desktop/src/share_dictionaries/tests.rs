use super::*;
use crate::qa_sweep::Driver;
use openxer_xer::edit::tables;

/// A workspace made by an earlier OpenXER (each project keeps its own
/// dictionaries) with two projects whose welders differ, opened in the app.
fn old_workspace(d: &mut Driver) -> (i64, i64) {
    let path = d.dir.join("version-1.db");
    let mut store = Store::open_version_1(path);
    let mut a = openxer_xer::starter();
    a.source_name = "A.xer".into();
    tables::add_resource(&mut a, "WLD").unwrap();
    let a_id = store.save(&a).unwrap();
    let mut b = openxer_xer::starter();
    b.source_name = "B.xer".into();
    b.model.project.name = "BRAVO".into();
    tables::add_resource(&mut b, "WLD").unwrap();
    let t = b.document.table_mut("RSRC");
    let i = t
        .rows
        .iter()
        .position(|r| t.value(r, "rsrc_short_name") == "WLD")
        .unwrap();
    t.set(i, "rsrc_name", "Welder, night".into());
    let b_id = store.save(&b).unwrap();
    let project = store.last().unwrap().unwrap();
    let mut app = App::new(&d.ctx, store, project);
    app.restore_open_set();
    app.note_dictionary_sharing();
    d.app = app;
    d.frames(3);
    (a_id, b_id)
}

fn open_dialog(d: &mut Driver) {
    d.home();
    d.click("menu_bar_admin");
    d.click("menu_share_dictionaries");
    d.frames(2);
}

#[test]
fn an_old_workspace_is_previewed_then_converted_only_on_apply() {
    let mut d = Driver::new("share-dict", openxer_xer::starter());
    let (a, b) = old_workspace(&mut d);
    assert!(
        d.app.status.contains("Share Dictionaries Across Projects"),
        "the startup notice points at the command: {}",
        d.app.status
    );
    assert!(!d.app.store.shared().unwrap());
    let before = [
        d.app.store.source(a).unwrap().2,
        d.app.store.source(b).unwrap().2,
    ];
    open_dialog(&mut d);
    let dialog = d.app.share_dictionaries.clone().expect("the preview opens");
    assert_eq!(dialog.projects.len(), 2, "{dialog:?}");
    assert!(
        dialog.details.iter().any(
            |l| l.contains("Resource \"WLD\" is kept as \"WLD (BRAVO)\"")
                && l.contains("rsrc_name")
        ),
        "{:#?}",
        dialog.details
    );
    // Cancel changes nothing.
    d.click("share_dictionaries_cancel");
    d.frames(2);
    assert!(d.app.share_dictionaries.is_none());
    assert!(!d.app.store.shared().unwrap());
    assert_eq!(
        [
            d.app.store.source(a).unwrap().2,
            d.app.store.source(b).unwrap().2
        ],
        before
    );
    // Escape too.
    open_dialog(&mut d);
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.share_dictionaries.is_none());
    assert!(!d.app.store.shared().unwrap());
    // Apply converts, keeps a backup and reopens the project.
    let model = d.app.project.model.clone();
    open_dialog(&mut d);
    d.click("share_dictionaries_apply");
    d.frames(2);
    let done = d.app.share_dictionaries.clone().unwrap();
    assert!(done.error.is_none(), "{:?}", done.error);
    let text = done.done.expect("a result");
    assert!(text.contains("dates and float"), "{text}");
    assert!(d.app.store.shared().unwrap());
    let backups: Vec<_> = std::fs::read_dir(d.dir.join("backups"))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(d.app.project.model, model, "the open project is the same");
    for id in [a, b] {
        let p = d.app.store.load(id).unwrap();
        let names: Vec<String> = p
            .document
            .table("RSRC")
            .unwrap()
            .rows
            .iter()
            .map(|r| {
                p.document
                    .table("RSRC")
                    .unwrap()
                    .value(r, "rsrc_short_name")
                    .to_string()
            })
            .collect();
        assert!(names.contains(&"WLD".to_string()) && names.contains(&"WLD (BRAVO)".to_string()));
    }
    d.click("share_dictionaries_cancel");
    d.frames(2);
    assert!(d.app.share_dictionaries.is_none());
    d.assert_clean("after conversion");
    // The command is unavailable now.
    open_dialog(&mut d);
    assert!(d.app.share_dictionaries.is_none());
}

/// Renders the conversion dialog (preview and result) to the ignored
/// `artifacts/shared-dictionaries/`.
#[test]
#[ignore = "writes review renders to artifacts/shared-dictionaries; run on demand"]
fn render_share_dictionaries() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/shared-dictionaries");
    std::fs::create_dir_all(&out).unwrap();
    let screen = egui::Vec2::new(1100., 700.);
    let mut d = Driver::probed(
        "render-share",
        openxer_xer::starter(),
        screen,
        Some(out.clone()),
    );
    old_workspace(&mut d);
    open_dialog(&mut d);
    d.frames(2);
    d.screenshot(&out.join("01-conversion-preview.png"));
    d.click("share_dictionaries_apply");
    d.frames(3);
    d.screenshot(&out.join("02-conversion-done.png"));
}

/// B13: when the conversion was committed but the open project could not be
/// opened again, the dialog says the workspace was converted (and where the
/// backup is), never "Nothing was changed".
#[test]
fn a_committed_conversion_is_reported_even_when_reopening_fails() {
    let report = serde_json::json!({
        "backup": "C:\\w\\backups\\openxer-before-dictionary-1.db",
        "projects": [{}, {}],
    });
    let text = conversion_message(&report, Err("damaged row".into()));
    assert!(
        text.contains("now shares its dictionaries across 2"),
        "{text}"
    );
    assert!(text.contains("openxer-before-dictionary-1.db"), "{text}");
    assert!(
        text.contains("could not be opened again (damaged row)"),
        "{text}"
    );
    assert!(!text.contains("Nothing was changed"), "{text}");
}
