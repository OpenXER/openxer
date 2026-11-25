//! The shared workspace dictionary changes no schedule: every corpus
//! project imported, one after another, into one workspace with a shared
//! dictionary (schema 3), then opened from it, schedules (F9 unleveled and
//! leveled) to the same dates and float as its file read directly, and so
//! does the XER an export writes (only the dictionary rows the project
//! uses). Alone in a workspace a project opens to exactly the text it was
//! stored from. The corpus is client data outside the repository; set
//! `OPENXER_CORPUS` to its folder (the test skips without it).
use openxer_automation::workspace_db::{hydrate, read_dictionary, start_new, store};
use openxer_xer::workspace_scope::{migrate::schedule_result, used_subset};
use openxer_xer::{Document, ProjectData};
use rusqlite::{params, Connection};

fn corpus_files() -> Option<Vec<std::path::PathBuf>> {
    let dir = std::env::var_os("OPENXER_CORPUS")?;
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xer")))
        .collect();
    files.sort();
    (!files.is_empty()).then_some(files)
}

/// A new, empty workspace with a shared dictionary.
fn workspace() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE projects(id INTEGER PRIMARY KEY,source_name TEXT NOT NULL,project_key INTEGER NOT NULL,title TEXT NOT NULL,xer_content TEXT NOT NULL,updated_at TEXT DEFAULT CURRENT_TIMESTAMP,source_windows_encoding INTEGER NOT NULL DEFAULT 0,level_resources INTEGER NOT NULL DEFAULT 0,UNIQUE(source_name,project_key));CREATE TABLE preferences(key TEXT PRIMARY KEY,value TEXT NOT NULL);").unwrap();
    start_new(&c).unwrap();
    c
}

/// Store `doc` as a new project of workspace `c`, as an import does.
fn import(c: &Connection, source: &str, pid: i64, doc: &Document) -> i64 {
    c.execute(
        "INSERT INTO projects(source_name,project_key,title,xer_content) VALUES(?1,?2,?1,'')",
        params![source, pid],
    )
    .unwrap();
    let id = c.last_insert_rowid();
    let stored = store(c, doc, None, Some(id)).unwrap();
    c.execute(
        "UPDATE projects SET xer_content=?1 WHERE id=?2",
        params![stored.text, id],
    )
    .unwrap();
    id
}

/// Project `id` of workspace `c` as a command opens it.
fn open(c: &Connection, id: i64, windows_encoding: bool) -> Document {
    let text: String = c
        .query_row("SELECT xer_content FROM projects WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .unwrap();
    let mut doc = Document::parse(&text).unwrap();
    doc.windows_encoding = windows_encoding;
    hydrate(c, &mut doc, Some(id)).unwrap();
    doc
}

#[test]
fn workspace_scope_preserves_schedules() {
    let Some(files) = corpus_files() else {
        println!("SKIPPED: set OPENXER_CORPUS to the corpus folder");
        return;
    };
    let shared = workspace();
    let mut failures = vec![];
    let mut projects = vec![];
    for file in &files {
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let doc = Document::read(file).unwrap();
        for (pid, _) in doc.projects() {
            let Ok(p) = ProjectData::from_document(doc.clone(), pid, name.clone()) else {
                println!("SKIPPED {name} {pid}: not importable");
                continue;
            };
            let direct = p.updated_document();
            // Alone in a workspace: back to the exact text.
            let alone = workspace();
            let id = import(&alone, &name, pid, &direct);
            if open(&alone, id, direct.windows_encoding).text() != direct.text() {
                failures.push(format!(
                    "{name} {pid}: alone it does not open to its own text"
                ));
            }
            let id = import(&shared, &format!("{name}#{pid}"), pid, &direct);
            projects.push((name.clone(), pid, id, direct));
        }
    }
    // Opened once every project is in the workspace.
    for (name, pid, id, direct) in &projects {
        let doc = open(&shared, *id, direct.windows_encoding);
        let exported = Document::parse(&used_subset(&doc, *pid).text()).unwrap();
        for level in [false, true] {
            let want = schedule_result(direct, *pid, level);
            if schedule_result(&doc, *pid, level) != want {
                failures.push(format!(
                    "{name} {pid}: dates or float differ when opened from the workspace (leveled {level})"
                ));
            }
            if schedule_result(&exported, *pid, level) != want {
                failures.push(format!(
                    "{name} {pid}: the exported file's dates or float differ (leveled {level})"
                ));
            }
        }
    }
    let (dict, _) = read_dictionary(&shared).unwrap();
    println!(
        "{} corpus projects in one workspace; its dictionary holds {} rows",
        projects.len(),
        dict.tables.iter().map(|t| t.rows.len()).sum::<usize>()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
