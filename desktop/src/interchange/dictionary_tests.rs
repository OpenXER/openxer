//! XER import into a workspace with a shared dictionary (schema 3): the
//! same file twice adds nothing, a calendar or resource whose name the
//! workspace has with other values is added as a copy and reported, Keep
//! and Update choose otherwise, Undo Last Import restores the dictionary,
//! and an export writes only the dictionary rows its project uses.
use super::workspace::{self, Write};
use super::*;
use crate::qa_sweep::Driver;
use openxer_xer::edit::tables;
use openxer_xer::workspace_scope::merge::{Choice, EventKind};

/// A project of a file: the starter with its own ID, its standard calendar
/// set to `hours` a day, and a welder.
fn file_project(name: &str, hours: &str) -> ProjectData {
    let mut p = openxer_xer::starter();
    p.source_name = format!("{name}.xer");
    p.model.project.name = name.into();
    tables::add_resource(&mut p, "WLD").unwrap();
    let t = p.document.table_mut("CALENDAR");
    for i in 0..t.rows.len() {
        if t.value(&t.rows[i], "clndr_name") == "Standard 5-day / 8-hour" {
            t.set(i, "day_hr_cnt", hours.into());
        }
    }
    p
}

fn dictionary_rows(store: &Store, table: &str) -> Vec<String> {
    let (dict, _) = store.dictionary().unwrap();
    let column = match table {
        "CALENDAR" => "clndr_name",
        _ => "rsrc_short_name",
    };
    let mut v: Vec<String> = dict.table(table).map_or(vec![], |t| {
        t.rows
            .iter()
            .map(|r| t.value(r, column).to_string())
            .collect()
    });
    v.sort();
    v
}

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("openxer-{name}-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

#[test]
fn importing_the_same_file_twice_adds_no_dictionary_rows() {
    let path = temp("dict-import-twice");
    let mut store = Store::open(path.clone()).unwrap();
    store.save(&openxer_xer::starter()).unwrap();
    let file = file_project("FILE", "8");
    let first = workspace::commit(&mut store, vec![Write::Insert(file.clone())]).unwrap();
    let calendars = dictionary_rows(&store, "CALENDAR");
    let resources = dictionary_rows(&store, "RSRC");
    let (dict, _) = store.dictionary().unwrap();
    let rows: usize = dict.tables.iter().map(|t| t.rows.len()).sum();
    let (ids, events) = workspace::commit_with(
        &mut store,
        vec![Write::Insert(file.clone())],
        &Default::default(),
        None,
    )
    .unwrap();
    assert!(
        events[0]
            .iter()
            .all(|e| !matches!(e.kind, EventKind::Copied)),
        "{:?}",
        events[0]
    );
    assert_eq!(dictionary_rows(&store, "CALENDAR"), calendars);
    assert_eq!(dictionary_rows(&store, "RSRC"), resources);
    let (dict, _) = store.dictionary().unwrap();
    assert_eq!(
        dict.tables.iter().map(|t| t.rows.len()).sum::<usize>(),
        rows
    );
    // Both imports schedule like the file.
    for id in [first[0], ids[0]] {
        let p = store.load(id).unwrap();
        assert_eq!(p.model.activities.len(), file.model.activities.len());
        assert_eq!(
            p.calendar_names.len(),
            store.load(first[0]).unwrap().calendar_names.len()
        );
    }
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_calendar_with_other_values_is_a_reported_copy_unless_kept_or_updated() {
    let path = temp("dict-import-clash");
    let mut store = Store::open(path.clone()).unwrap();
    let base = store.save(&openxer_xer::starter()).unwrap();
    // Copy (the default): a renamed copy, reported with the field.
    let ten = file_project("TEN", "10");
    let preview =
        workspace::preview(&store, &[&Write::Insert(ten.clone())], &Default::default()).unwrap();
    let clashes = openxer_automation::workspace_db::clash_tables(&preview[0]);
    assert!(clashes.contains("CALENDAR"), "{clashes:?}");
    assert_eq!(
        dictionary_rows(&store, "CALENDAR").len(),
        openxer_xer::starter().calendar_names.len(),
        "a preview writes nothing"
    );
    let (ids, events) = workspace::commit_with(
        &mut store,
        vec![Write::Insert(ten.clone())],
        &Default::default(),
        None,
    )
    .unwrap();
    let report = openxer_automation::workspace_db::import_report(&events[0]);
    assert!(
        report.iter().any(|l| l.contains(
            "Calendar \"Standard 5-day / 8-hour\" is added as \"Standard 5-day / 8-hour_1\""
        ) && l.contains("day_hr_cnt")),
        "{report:#?}"
    );
    let ten_p = store.load(ids[0]).unwrap();
    let pcal = ten_p.model.calendars[ten_p.model.project_calendar]
        .id
        .clone();
    assert_eq!(
        openxer_xer::edit::calendars::name(&ten_p.document, &pcal),
        "Standard 5-day / 8-hour_1",
        "the project's calendar is its copy"
    );
    // Keep: the workspace's calendar is used.
    let nine = file_project("NINE", "9");
    let keep: openxer_automation::workspace_db::Choices =
        [("CALENDAR".to_string(), Choice::Keep)].into();
    let before = dictionary_rows(&store, "CALENDAR");
    let (ids, events) =
        workspace::commit_with(&mut store, vec![Write::Insert(nine)], &keep, None).unwrap();
    assert!(events[0].iter().any(|e| e.kind == EventKind::Kept));
    assert_eq!(dictionary_rows(&store, "CALENDAR"), before);
    let nine_p = store.load(ids[0]).unwrap();
    let pcal = nine_p.model.calendars[nine_p.model.project_calendar]
        .id
        .clone();
    assert_eq!(
        openxer_xer::edit::calendars::name(&nine_p.document, &pcal),
        "Standard 5-day / 8-hour"
    );
    // Update: the workspace's calendar takes the file's values, for every
    // project that uses it.
    let seven = file_project("SEVEN", "7");
    let update: openxer_automation::workspace_db::Choices =
        [("CALENDAR".to_string(), Choice::Update)].into();
    workspace::commit_with(&mut store, vec![Write::Insert(seven)], &update, None).unwrap();
    assert_eq!(dictionary_rows(&store, "CALENDAR"), before);
    let base_p = store.load(base).unwrap();
    let id = base_p.model.calendars[base_p.model.project_calendar]
        .id
        .clone();
    assert_eq!(
        openxer_xer::edit::calendars::value(&base_p.document, &id, "day_hr_cnt"),
        "7"
    );
    // Undo Last Import puts the dictionary back.
    workspace::undo_last_import(&mut store, &[]).unwrap();
    let base_p = store.load(base).unwrap();
    assert_ne!(
        openxer_xer::edit::calendars::value(&base_p.document, &id, "day_hr_cnt"),
        "7"
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

/// B3: Undo Last Import takes back only the rows the import changed.
/// Rows added or edited in the dictionary since (here a calendar and a
/// resource made in another project, and the imported welder renamed) stay.
#[test]
fn undo_last_import_keeps_dictionary_changes_made_since() {
    let path = temp("dict-import-undo-later");
    let mut store = Store::open(path.clone()).unwrap();
    let base = store.save(&openxer_xer::starter()).unwrap();
    let mut file = file_project("FILE", "8");
    tables::add_resource(&mut file, "PIP").unwrap();
    workspace::commit(&mut store, vec![Write::Insert(file)]).unwrap();
    assert!(dictionary_rows(&store, "RSRC").contains(&"WLD".to_string()));
    // Later, in the base project: a new calendar and crane, and the
    // imported welder renamed.
    let mut a = store.load(base).unwrap();
    let cal = openxer_xer::edit::calendars::add(
        &mut a,
        openxer_xer::edit::calendars::Kind::Global,
        None,
        "2026-11-09 09:00",
    )
    .unwrap();
    openxer_xer::edit::calendars::rename(&mut a, &cal, "Night shift").unwrap();
    tables::add_resource(&mut a, "CRN").unwrap();
    tables::set_resource(
        &mut a,
        "WLD",
        "rsrc_name",
        &tables::serde_like::Value::Text("Welder, night".into()),
    )
    .unwrap();
    store.save(&a).unwrap();
    let (ids, kept) = workspace::undo_last_import(&mut store, &[]).unwrap();
    assert_eq!(ids.len(), 1);
    assert!(
        dictionary_rows(&store, "CALENDAR").contains(&"Night shift".to_string()),
        "the calendar made since stays: {:?}",
        dictionary_rows(&store, "CALENDAR")
    );
    let resources = dictionary_rows(&store, "RSRC");
    assert!(resources.contains(&"CRN".to_string()), "{resources:?}");
    assert!(
        resources.contains(&"WLD".to_string()),
        "the welder renamed since stays: {resources:?}"
    );
    assert!(
        !resources.contains(&"PIP".to_string()),
        "a row only the import added and nobody changed goes: {resources:?}"
    );
    assert!(
        kept.iter().any(|k| k.contains("WLD")),
        "the kept row is reported: {kept:?}"
    );
    let a = store.load(base).unwrap();
    assert!(a.calendar_names.contains(&"Night shift".to_string()));
    drop(store);
    let _ = std::fs::remove_file(path);
}

/// B3: an import into a workspace whose dictionary has no row yet is undone
/// too: the rows it added go.
#[test]
fn undo_of_the_first_import_into_an_empty_dictionary_removes_its_rows() {
    let path = temp("dict-import-undo-first");
    let mut store = Store::open(path.clone()).unwrap();
    assert!(dictionary_rows(&store, "RSRC").is_empty());
    workspace::commit(&mut store, vec![Write::Insert(file_project("FILE", "8"))]).unwrap();
    assert!(dictionary_rows(&store, "RSRC").contains(&"WLD".to_string()));
    workspace::undo_last_import(&mut store, &[]).unwrap();
    assert!(
        dictionary_rows(&store, "RSRC").is_empty(),
        "{:?}",
        dictionary_rows(&store, "RSRC")
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn an_export_writes_the_dictionary_rows_its_project_uses() {
    let mut d = Driver::new("dict-export", openxer_xer::starter());
    // Another project brings a crane the sample does not use.
    let mut other = openxer_xer::starter();
    other.source_name = "other.xer".into();
    other.model.project.name = "OTHER".into();
    tables::add_resource(&mut other, "CRN").unwrap();
    d.app.store.save(&other).unwrap();
    d.frames(2);
    let back = d.app.store.load(d.app.open_set.active.unwrap()).unwrap();
    assert!(
        back.document.table("RSRC").is_some_and(|t| t
            .rows
            .iter()
            .any(|r| t.value(r, "rsrc_short_name") == "CRN")),
        "the open project sees the workspace's crane"
    );
    let path = d.dir.join("sample.xer");
    d.export(ExportFormat::Xer, &path);
    let doc = Document::read(&path).unwrap();
    assert!(
        !doc.table("RSRC").is_some_and(|t| t
            .rows
            .iter()
            .any(|r| t.value(r, "rsrc_short_name") == "CRN")),
        "the file has no crane"
    );
    let pid = doc.projects()[0].0;
    let mut p = ProjectData::from_document(doc, pid, "x".into()).unwrap();
    let mut q = d.app.project.clone();
    schedule_model(&mut p.model, &p.resources).unwrap();
    schedule_model(&mut q.model, &q.resources).unwrap();
    let dates = |p: &ProjectData| -> Vec<_> {
        p.model
            .activities
            .iter()
            .map(|a| {
                (
                    a.task_code.clone(),
                    a.early_start,
                    a.early_finish,
                    a.total_float,
                )
            })
            .collect()
    };
    assert_eq!(
        dates(&p),
        dates(&q),
        "the exported project schedules the same"
    );
}

/// The import wizard's summary page with a dictionary clash, to the ignored
/// `artifacts/shared-dictionaries/`.
#[test]
#[ignore = "writes review renders to artifacts/shared-dictionaries; run on demand"]
fn render_import_dictionary_report() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/shared-dictionaries");
    std::fs::create_dir_all(&out).unwrap();
    let mut d = Driver::probed(
        "render-import-dict",
        openxer_xer::starter(),
        egui::Vec2::new(1100., 700.),
        Some(out.clone()),
    );
    let file = d.dir.join("ten.xer");
    file_project("TEN", "10")
        .updated_document()
        .write(&file)
        .unwrap();
    d.app.open_import_wizard();
    {
        let w = d.app.interchange.xer.as_mut().unwrap();
        w.path_text = file.display().to_string();
    }
    d.app.xer_load_files();
    d.app.interchange.xer.as_mut().unwrap().step = 4;
    d.app.xer_prepare();
    d.frames(3);
    assert!(d.has("import_dictionary_calendar"), "the choice is offered");
    d.screenshot(&out.join("03-import-report.png"));
    d.app
        .interchange
        .xer
        .as_mut()
        .unwrap()
        .choices
        .insert("CALENDAR".into(), Choice::Keep);
    d.app.xer_prepare();
    d.frames(3);
    d.screenshot(&out.join("04-import-report-keep.png"));
}
