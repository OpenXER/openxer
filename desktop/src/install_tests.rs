//! An installed copy and a new user's first start: where the workspace
//! lives, a double-clicked XER file, a second double-click while the app
//! runs, and a data folder that cannot be made. `launch` is driven with a
//! recording message function, so no window opens; no test reads or sets
//! the process environment.
use super::*;
use std::ffi::OsString;

/// A fresh folder of its own under the temp directory.
fn scratch(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "openxer-install-{name}-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn start(args: &[OsString]) -> (Option<Startup>, Option<String>) {
    let mut told = None;
    let started = launch(args, |m| told = Some(m.to_string()));
    (started, told)
}

/// The installer's marker beside the EXE sends the workspace to the
/// per-user data folder, never the (writable) program folder; a portable
/// copy keeps it beside the EXE; OPENXER_DATA_DIR wins over both.
#[test]
fn an_installed_copy_keeps_its_workspace_out_of_the_program_folder() {
    let root = scratch("marker");
    let program = root.join("Programs").join("OpenXER");
    let local = root.join("Local");
    std::fs::create_dir_all(&program).unwrap();
    let local_os = Some(OsString::from(local.as_os_str()));

    // Portable: the program folder can be written, so the workspace is there.
    assert_eq!(
        Store::default_path_in(Some(&program), None, local_os.clone()),
        program.join("openxer.db")
    );
    // Installed: the per-user data folder.
    std::fs::write(program.join(util::INSTALLED_MARKER), "installed").unwrap();
    let per_user = local.join("OpenXER").join("openxer.db");
    assert_eq!(
        Store::default_path_in(Some(&program), None, local_os.clone()),
        per_user
    );
    // No write check is left in the program folder.
    let names: Vec<_> = std::fs::read_dir(&program)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, vec![OsString::from(util::INSTALLED_MARKER)]);
    // A workspace an earlier portable copy left in the program folder is
    // still used while there is no per-user one ...
    std::fs::write(program.join("openxer.db"), b"left by a portable copy").unwrap();
    assert_eq!(
        Store::default_path_in(Some(&program), None, local_os.clone()),
        program.join("openxer.db")
    );
    // ... and the per-user one wins once it exists.
    std::fs::create_dir_all(per_user.parent().unwrap()).unwrap();
    std::fs::write(&per_user, b"").unwrap();
    assert_eq!(
        Store::default_path_in(Some(&program), None, local_os.clone()),
        per_user
    );
    // The override, installed or portable.
    let moved = root.join("Data folder");
    for exe_dir in [program.as_path(), root.as_path()] {
        assert_eq!(
            Store::default_path_in(
                Some(exe_dir),
                Some(moved.clone().into_os_string()),
                local_os.clone()
            ),
            moved.join("openxer.db")
        );
    }
    // An empty override is no override.
    assert_eq!(
        Store::default_path_in(Some(&program), Some(OsString::new()), local_os),
        per_user
    );
    let _ = std::fs::remove_dir_all(root);
}

/// The AI Agents menu finds the openxer-cli Setup installs beside the app
/// (unless OPENXER_CLI names another, which the test then accepts).
#[test]
fn the_ai_agents_menu_finds_the_installed_cli() {
    let program = scratch("cli").join("Open XER Ä");
    std::fs::create_dir_all(&program).unwrap();
    let app = format!("OpenXER{}", std::env::consts::EXE_SUFFIX);
    let cli = openxer_automation::connect::CLI_NAME;
    for name in [app.as_str(), cli] {
        std::fs::write(program.join(name), b"").unwrap();
    }
    let found = openxer_automation::connect::resolve_cli(Some(&program.join(&app))).unwrap();
    if std::env::var_os("OPENXER_CLI").is_none_or(|v| v.is_empty()) {
        assert_eq!(found, program.join(cli));
    }
    let _ = std::fs::remove_dir_all(program.parent().unwrap());
}

/// A double-clicked XER file arrives as the only argument; a path with
/// spaces and letters outside ASCII opens like any other.
#[test]
fn a_double_clicked_xer_file_opens_at_startup() {
    let dir = scratch("double-click").join("Plant Ünit 2 (Zürich)");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("Schedule Rev Ä.xer");
    let mut p = openxer_xer::starter();
    p.details.get_mut(&1010).unwrap().name = "Opened by double click".into();
    p.updated_document().write(&file).unwrap();
    let db = dir.join("openxer.db");
    let args: Vec<OsString> = vec![
        "--database".into(),
        db.clone().into_os_string(),
        file.clone().into_os_string(),
    ];
    let (started, told) = start(&args);
    assert_eq!(told, None);
    let started = started.expect("the app starts");
    assert_eq!(started.error, None);
    assert_eq!(started.project.source_name, file.display().to_string());
    assert_eq!(
        started.project.details[&1010].name,
        "Opened by double click"
    );
    drop(started);
    let _ = std::fs::remove_dir_all(dir.parent().unwrap());
}

/// A second double-click while OpenXER runs: the second app does not start,
/// and the user is told it is already running and how to open the file.
#[test]
fn a_second_double_click_while_running_says_how_to_open_the_file() {
    let dir = scratch("second");
    let db = dir.join("openxer.db");
    let base = vec!["--database".into(), db.clone().into_os_string()];
    let (first, told) = start(&base);
    assert_eq!(told, None);
    let first = first.unwrap();
    let file = dir.join("Another job.xer");
    let mut second = base.clone();
    second.push(file.clone().into_os_string());
    let (started, told) = start(&second);
    assert!(started.is_none());
    let told = told.expect("the user is told");
    assert!(told.contains("already running"), "{told}");
    assert!(told.contains(&file.display().to_string()), "{told}");
    assert!(told.contains("File > Import"), "{told}");
    assert!(!told.contains('\u{2014}') && !told.contains('\u{2013}'));
    drop(first);
    let _ = std::fs::remove_dir_all(dir);
}

/// A data folder that cannot be made (a file is where the folder should
/// be) ends in a message naming the workspace, not a panic.
#[test]
fn a_data_folder_that_cannot_be_made_is_reported() {
    let dir = scratch("blocked");
    let blocker = dir.join("OpenXER");
    std::fs::write(&blocker, b"a file, not a folder").unwrap();
    let db = blocker.join("openxer.db");
    let (started, told) = start(&["--database".into(), db.clone().into_os_string()]);
    assert!(started.is_none());
    let told = told.expect("the user is told");
    assert!(told.contains("cannot open its workspace"), "{told}");
    assert!(told.contains(&db.display().to_string()), "{told}");
    let _ = std::fs::remove_dir_all(dir);
}
