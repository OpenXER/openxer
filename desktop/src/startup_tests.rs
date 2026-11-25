//! Starting the app: the workspace the window opens on, the project it
//! starts with, and what the user is told when either cannot be opened.
//! `launch` is driven with a recording message function, so no window opens.
use super::*;
use std::ffi::OsString;

fn temp_db(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "openxer-startup-{name}-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("openxer.db")
}

fn args(db: &Path) -> Vec<OsString> {
    vec!["--database".into(), db.as_os_str().to_owned()]
}

/// `launch` with `args`, and the message it gave when it could not start.
fn start(args: &[OsString]) -> (Option<Startup>, Option<String>) {
    let mut told = None;
    let started = launch(args, |m| told = Some(m.to_string()));
    (started, told)
}

/// B5: the project the user built in the sample (it keeps the sample's
/// name and key) fails to load at startup. The sample shown instead is
/// saved under a name of its own and the user's project is left as it was.
#[test]
fn startup_never_saves_the_sample_over_a_project_that_failed_to_load() {
    let db = temp_db("b5");
    let (id, broken) = {
        let mut store = Store::open(db.clone()).unwrap();
        let mut mine = openxer_xer::sample_project::sample_project().current;
        let first = mine.model.activities[1].task_id;
        mine.details.get_mut(&first).unwrap().name = "My own first activity".into();
        let id = store.save(&mine).unwrap();
        // The stored text no longer reads as a schedule (a damaged row).
        let broken = "ERMHDR\t25.12\r\n%T\tPROJECT\r\n%F\tproj_id\r\n%E\r\n".to_string();
        store
            .connection()
            .execute(
                "UPDATE projects SET xer_content=?1 WHERE id=?2",
                rusqlite::params![broken, id],
            )
            .unwrap();
        (id, broken)
    };
    let (started, told) = start(&args(&db));
    assert_eq!(told, None, "the workspace opens");
    let started = started.unwrap();
    let error = started.error.clone().unwrap_or_default();
    assert!(
        error.contains("still in the workspace"),
        "the user is told the project did not open: {error}"
    );
    let text: String = started
        .store
        .connection()
        .query_row("SELECT xer_content FROM projects WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(text, broken, "the user's project row is untouched");
    assert_ne!(
        started.project.source_name,
        openxer_xer::sample_project::SAMPLE_SOURCE,
        "the sample shown has a name of its own"
    );
    assert_eq!(
        started.store.list().unwrap().len(),
        2,
        "the sample is a second row"
    );
}

/// An empty workspace starts on the sample under its own name, with its
/// as-planned copy as the project and Primary baseline.
#[test]
fn a_new_workspace_starts_on_the_sample() {
    let db = temp_db("new");
    let (started, told) = start(&args(&db));
    assert_eq!(told, None);
    let started = started.unwrap();
    assert_eq!(
        started.project.source_name,
        openxer_xer::sample_project::SAMPLE_SOURCE
    );
    assert_eq!(
        started.project.model.project.name,
        openxer_xer::sample_project::SAMPLE_PROJECT_ID
    );
    assert_eq!(started.error, None);
    assert_eq!(started.store.list().unwrap().len(), 1);
    let row = started.store.list().unwrap()[0].0;
    let c = started.store.connection();
    let baselines = crate::baselines::store::list(c, row).unwrap();
    assert_eq!(baselines.len(), 1);
    assert_eq!(
        openxer_interchange::baseline::project_value(
            &started.project.document,
            started.project.model.project.proj_id,
            "sum_base_proj_id"
        ),
        baselines[0].project_key.to_string()
    );
    assert_eq!(
        crate::baselines::store::user_baselines(c, row).0[0],
        crate::baselines::store::Choice::Baseline(baselines[0].id)
    );
    // Reopened, it is the same sample with its baseline (no second copy).
    drop(started);
    let (again, _) = start(&args(&db));
    let again = again.unwrap();
    assert_eq!(again.store.list().unwrap().len(), 1);
    assert_eq!(
        crate::baselines::store::list(again.store.connection(), row)
            .unwrap()
            .len(),
        1
    );
}

/// A workspace made by the first start holds the sample project, the
/// starter reports and nothing else: no other project, no dictionary row
/// the sample did not bring, no imported notebook topic.
#[test]
fn a_new_workspace_holds_only_the_sample_and_the_starter_reports() {
    use openxer_automation::workspace_db;
    let db = temp_db("fresh-contents");
    let (started, _) = start(&args(&db));
    let started = started.unwrap();
    let c = started.store.connection();
    let titles: Vec<String> = started
        .store
        .list()
        .unwrap()
        .into_iter()
        .map(|(_, t)| t)
        .collect();
    assert_eq!(titles, vec!["RIVERBEND".to_string()]);
    let kinds: Vec<String> = c
        .prepare("SELECT rpt_type FROM report_defs")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(!kinds.is_empty());
    assert!(
        kinds
            .iter()
            .all(|k| k == crate::reports::rpt::store::KIND_LIBRARY),
        "{kinds:?}"
    );
    let baselines: i64 = c
        .query_row("SELECT COUNT(*) FROM baselines", [], |r| r.get(0))
        .unwrap();
    assert_eq!(baselines, 1, "the sample's as-planned copy only");
    // The dictionary is the sample's: every row is noted as the sample's,
    // and its notebook topics are the sample's own seven.
    let (dict, _) = workspace_db::read_dictionary(c).unwrap();
    let record = workspace_db::read_sample(c).unwrap().unwrap();
    assert_eq!(record.row, started.store.list().unwrap()[0].0);
    assert_eq!(workspace_db::workspace_rows(&dict), record.rows);
    let t = dict.table("MEMOTYPE").unwrap();
    let topics: Vec<&str> = t.rows.iter().map(|r| t.value(r, "memo_type")).collect();
    assert_eq!(
        topics,
        [
            "Scope",
            "Basis and Assumptions",
            "Constraints and Access",
            "Safety",
            "Quality and Inspection",
            "Field Notes",
            "Change Notes"
        ]
    );
}

/// A project of the user's own, with a resource "PF" and a notebook topic
/// "Safety" that differ from the sample's, a topic the sample lacks, and an
/// assignment and a note that use them.
fn users_project() -> ProjectData {
    let mut doc = openxer_xer::starter().document;
    let id = |s: &str| s.to_string();
    doc.table_mut("RSRC").add(&[
        ("rsrc_id", id("1")),
        ("rsrc_short_name", id("PF")),
        ("rsrc_name", id("Pipefitter, union hall")),
        ("rsrc_type", id("RT_Labor")),
        ("clndr_id", id("1")),
    ]);
    doc.table_mut("RSRCRATE").add(&[
        ("rsrc_rate_id", id("1")),
        ("rsrc_id", id("1")),
        ("max_qty_per_hr", id("3")),
        ("cost_per_qty", id("101")),
        ("start_date", id("2026-01-01 00:00")),
    ]);
    doc.table_mut("TASKRSRC").add(&[
        ("taskrsrc_id", id("1")),
        ("task_id", id("1090")),
        ("proj_id", id("1")),
        ("rsrc_id", id("1")),
        ("target_qty", id("48")),
        ("remain_qty", id("48")),
        ("target_qty_per_hr", id("1")),
        ("remain_qty_per_hr", id("1")),
        ("rsrc_type", id("RT_Labor")),
    ]);
    for (n, topic) in [(1, "Safety"), (2, "Weekly Reports")] {
        doc.table_mut("MEMOTYPE").add(&[
            ("memo_type_id", n.to_string()),
            ("seq_num", (n * 7).to_string()),
            ("memo_type", topic.into()),
        ]);
    }
    doc.table_mut("TASKMEMO").add(&[
        ("memo_id", id("1")),
        ("task_id", id("1090")),
        ("memo_type_id", id("1")),
        ("proj_id", id("1")),
        ("task_memo", id("<p>Hot work permit needed.</p>")),
    ]);
    let mut p = ProjectData::from_document(doc, 1, "C:/jobs/own.xer".into()).unwrap();
    p.model.project.name = "OWNJOB".into();
    p
}

/// The value of `column` in the dictionary rows of `table`.
fn names(dict: &Document, table: &str, column: &str) -> Vec<String> {
    dict.table(table)
        .map(|t| {
            t.rows
                .iter()
                .map(|r| t.value(r, column).to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Starting on the sample and importing a schedule that shares a resource
/// and a topic name with it, then deleting the sample in the Projects view:
/// the imported project is intact under its own names and nothing the
/// sample alone brought stays in the dictionary. Undo puts the sample back.
#[test]
fn deleting_the_sample_takes_its_dictionary_rows_with_it() {
    use crate::interchange::workspace::{commit, Write};
    use openxer_automation::workspace_db;
    let db = temp_db("remove-sample");
    let (started, _) = start(&args(&db));
    let started = started.unwrap();
    let mut store = started.store;
    let sample_row = store.list().unwrap()[0].0;
    let record = workspace_db::read_sample(store.connection())
        .unwrap()
        .unwrap();
    let own = commit(&mut store, vec![Write::Insert(users_project())]).unwrap()[0];
    let (dict, _) = store.dictionary().unwrap();
    assert!(names(&dict, "RSRC", "rsrc_short_name").contains(&"PF_1".to_string()));
    assert!(names(&dict, "MEMOTYPE", "memo_type").contains(&"Safety_1".to_string()));
    let mine = store.load(own).unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx, store, mine);
    app.request_project_delete(sample_row);
    assert!(app.views.projects.delete.as_ref().unwrap().sample);
    app.views.projects.delete = None;
    app.delete_workspace_project(sample_row, 1.).unwrap();
    assert!(
        app.status.starts_with("Sample project RIVERBEND removed"),
        "{}",
        app.status
    );
    let c = app.store.connection();
    assert_eq!(app.store.list().unwrap().len(), 1);
    assert_eq!(workspace_db::read_sample(c).unwrap(), None);
    let (dict, _) = app.store.dictionary().unwrap();
    let left = workspace_db::workspace_rows(&dict);
    assert!(
        left.iter().all(|k| !record.rows.contains(k)),
        "sample rows left"
    );
    let resources = names(&dict, "RSRC", "rsrc_short_name");
    assert_eq!(resources, vec!["PF".to_string()]);
    let mut topics = names(&dict, "MEMOTYPE", "memo_type");
    topics.sort();
    assert_eq!(topics, vec!["Safety", "Weekly Reports"]);
    assert!(names(&dict, "CALENDAR", "clndr_name")
        .iter()
        .all(|n| !n.starts_with("Riverbend")));
    // The user's project reads as it was imported.
    let back = app.store.load(own).unwrap();
    let t = back.document.table("TASKRSRC").unwrap();
    let rsrc: i64 = t.value(&t.rows[0], "rsrc_id").parse().unwrap();
    assert_eq!(
        back.resource_names.get(&rsrc).map(String::as_str),
        Some("Pipefitter, union hall")
    );
    let rates = back.document.table("RSRCRATE").unwrap();
    assert!(rates
        .rows
        .iter()
        .any(|r| rates.value(r, "cost_per_qty") == "101"));
    let memo = back.document.table("TASKMEMO").unwrap();
    let topic: i64 = memo.value(&memo.rows[0], "memo_type_id").parse().unwrap();
    let t = back.document.table("MEMOTYPE").unwrap();
    assert!(t
        .rows
        .iter()
        .any(|r| t.value(r, "memo_type_id") == topic.to_string()
            && t.value(r, "memo_type") == "Safety"));
    // Undo puts the sample, its rows and its record back.
    app.history_move(&ctx, false);
    assert_eq!(app.store.list().unwrap().len(), 2);
    let c = app.store.connection();
    assert_eq!(workspace_db::read_sample(c).unwrap(), Some(record.clone()));
    let (dict, _) = app.store.dictionary().unwrap();
    assert!(record
        .rows
        .iter()
        .all(|k| workspace_db::workspace_rows(&dict).contains(k)));
    assert!(names(&dict, "RSRC", "rsrc_short_name").contains(&"PF_1".to_string()));
}

/// The release package is the programs, their documentation and license
/// files: never a workspace, a schedule or anything a build folder holds
/// besides those (an `openxer.db` a portable run left in the output folder
/// stays out of Setup).
#[test]
fn the_release_package_carries_no_workspace_or_schedule() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let iss = std::fs::read_to_string(root.join("tools/installer/openxer.iss")).unwrap();
    let files: Vec<&str> = iss
        .lines()
        .skip_while(|l| l.trim() != "[Files]")
        .skip(1)
        .take_while(|l| !l.trim_start().starts_with('['))
        .filter(|l| l.trim_start().starts_with("Source:"))
        .collect();
    assert_eq!(files.len(), 7, "{files:?}");
    for line in &files {
        let source = line.split('"').nth(1).unwrap().to_ascii_lowercase();
        assert!(!source.contains('*') && !source.contains('?'), "{line}");
        for banned in [".db", ".xer", "-wal", "backups"] {
            assert!(!source.contains(banned), "{line}");
        }
    }
    let script = std::fs::read_to_string(root.join("desktop/build_release.py")).unwrap();
    for call in [
        "copytree",
        "make_archive",
        "glob(",
        "iterdir",
        "listdir",
        "zipfile",
    ] {
        assert!(!script.contains(call), "build_release.py uses {call}");
    }
    let copies: Vec<&str> = script
        .lines()
        .filter(|l| l.contains("shutil.copy"))
        .collect();
    assert_eq!(copies.len(), 3, "{copies:?}");
    for line in copies {
        let lower = line.to_ascii_lowercase();
        assert!(!lower.contains(".db") && !lower.contains(".xer"), "{line}");
    }
}

/// B11: a workspace file that is not a database stops the start with a
/// message that says what is wrong and where the file is, and the file is
/// left as it was.
#[test]
fn a_damaged_workspace_is_reported_with_its_path() {
    let db = temp_db("b11-damaged");
    let junk = b"This is not a workspace database, it is plain text and nothing else.".repeat(80);
    std::fs::write(&db, &junk).unwrap();
    let (started, told) = start(&args(&db));
    assert!(started.is_none(), "nothing starts");
    let told = told.expect("the user is told");
    assert!(told.contains(&db.display().to_string()), "{told}");
    assert!(told.contains("cannot open its workspace"), "{told}");
    assert_eq!(std::fs::read(&db).unwrap(), junk, "the file is unchanged");
}

/// B10, B11: a workspace from a newer OpenXER stops the start with a
/// message naming the file, and is not written.
#[test]
fn a_workspace_from_a_newer_build_is_reported_and_left_alone() {
    let db = temp_db("b10-newer");
    {
        let s = Store::open(db.clone()).unwrap();
        s.set_preference(openxer_automation::workspace_db::SCHEMA_KEY, "99")
            .unwrap();
    }
    let before = std::fs::read(&db).unwrap();
    let (started, told) = start(&args(&db));
    assert!(started.is_none());
    let told = told.expect("the user is told");
    assert!(told.contains("newer OpenXER"), "{told}");
    assert!(told.contains(&db.display().to_string()), "{told}");
    assert_eq!(std::fs::read(&db).unwrap(), before, "nothing written");
}

/// B6: --open of a file the workspace already has a copy of does not
/// replace the copy at startup; the window opens on the copy and asks.
#[test]
fn opening_a_file_at_startup_never_replaces_the_workspace_copy() {
    let db = temp_db("b6-open");
    let file = db.with_file_name("x.xer");
    let source = file.display().to_string();
    let mut p = openxer_xer::starter();
    p.source_name = source.clone();
    p.updated_document().write(&file).unwrap();
    let id = {
        let mut store = Store::open(db.clone()).unwrap();
        p.details.get_mut(&1010).unwrap().name = "Edited in the workspace".into();
        store.save(&p).unwrap()
    };
    let mut a = args(&db);
    a.extend(["--open".into(), file.as_os_str().to_owned()]);
    let (started, told) = start(&a);
    assert_eq!(told, None);
    let started = started.unwrap();
    assert_eq!(
        started.replace_prompt.as_ref().map(|(_, row)| *row),
        Some(id)
    );
    assert_eq!(
        started.project.details[&1010].name,
        "Edited in the workspace"
    );
    assert_eq!(
        started.store.load(id).unwrap().details[&1010].name,
        "Edited in the workspace",
        "the copy is not replaced"
    );
}

/// B9: a second app on a workspace another app has open is refused with a
/// message and changes nothing; once the first app is gone (here its lock
/// is dropped, as the system does when an app exits or crashes) it starts.
#[test]
fn a_second_app_on_the_same_workspace_is_refused() {
    let db = temp_db("b9-lock");
    let (first, told) = start(&args(&db));
    assert_eq!(told, None);
    let first = first.unwrap();
    let before = first.store.list().unwrap();
    let (second, told) = start(&args(&db));
    assert!(second.is_none(), "the second app does not start");
    let told = told.expect("the user is told");
    assert!(
        told.contains("already open in another OpenXER window"),
        "{told}"
    );
    assert!(told.contains(&db.display().to_string()), "{told}");
    assert_eq!(first.store.list().unwrap(), before, "nothing changed");
    drop(first);
    let (third, told) = start(&args(&db));
    assert_eq!(told, None, "the lock went with the first app");
    assert!(third.is_some());
}

/// A recovery copy an earlier run left.
fn leave_recovery_copy(db: &Path) {
    let c = rusqlite::Connection::open(db).unwrap();
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS recovery_copies(session TEXT NOT NULL, source_name TEXT NOT NULL, project_key INTEGER NOT NULL, title TEXT NOT NULL, xer_content TEXT NOT NULL, windows_encoding INTEGER NOT NULL, level_resources INTEGER NOT NULL, saved_at TEXT NOT NULL, PRIMARY KEY(session, source_name, project_key));
         INSERT INTO recovery_copies VALUES('earlier','A.xer',1,'A','text',0,0,'2026-11-09 10:00');",
    )
    .unwrap();
}

fn recovery_copies(db: &Path) -> i64 {
    rusqlite::Connection::open(db)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM recovery_copies", [], |r| r.get(0))
        .unwrap()
}

/// Startup order: the lock, then the schema check (and upgrade), and only
/// then the window, whose first frame offers recovery copies. A workspace
/// that is refused (made by a newer OpenXER, or open in another app) never
/// gets that far, so its recovery copies are neither offered nor deleted.
#[test]
fn a_refused_workspace_keeps_its_recovery_copies() {
    let newer = temp_db("recovery-newer");
    {
        let s = Store::open(newer.clone()).unwrap();
        s.set_preference(openxer_automation::workspace_db::SCHEMA_KEY, "99")
            .unwrap();
    }
    leave_recovery_copy(&newer);
    let (started, told) = start(&args(&newer));
    assert!(started.is_none() && told.is_some());
    assert_eq!(recovery_copies(&newer), 1);

    let locked = temp_db("recovery-locked");
    let (first, _) = start(&args(&locked));
    let first = first.unwrap();
    leave_recovery_copy(&locked);
    let (second, told) = start(&args(&locked));
    assert!(second.is_none() && told.is_some());
    assert_eq!(recovery_copies(&locked), 1);
    drop(first);
}

/// The sample passes every check of Check Schedule with the default
/// targets, against its as-planned baseline.
#[test]
fn the_sample_passes_check_schedule() {
    use crate::schedule_check::{run_checks_with, CheckParams};
    let sample = openxer_xer::sample_project::sample_project();
    let (plan, current) = (&sample.plan, &sample.current);
    let shift = (plan.model.origin_serial - current.model.origin_serial) * 1440;
    let baseline: Vec<Option<i64>> = current
        .model
        .activities
        .iter()
        .map(|a| {
            plan.model
                .activities
                .iter()
                .find(|b| b.task_id == a.task_id)
                .and_then(|b| b.early_finish)
                .map(|f| f + shift)
        })
        .collect();
    let t = current.document.table("PROJCOST").unwrap();
    let with_expenses = t
        .rows
        .iter()
        .map(|r| t.value(r, "task_id").parse().unwrap())
        .collect();
    let results = run_checks_with(
        &current.model,
        &CheckParams::default(),
        &with_expenses,
        Some(&baseline),
    );
    let failed: Vec<String> = results
        .iter()
        .filter(|r| r.passed == Some(false))
        .map(|r| format!("{} {:?}", r.check.name(), r.value))
        .collect();
    assert!(failed.is_empty(), "{failed:?}");
    assert!(results.iter().filter(|r| r.passed == Some(true)).count() >= 13);
}
