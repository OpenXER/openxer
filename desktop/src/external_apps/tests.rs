//! External applications: the entry rules, the arguments, the `%xer%`
//! export and launch, the dialog and the View > Launch list.
use super::*;
use crate::qa_sweep::Driver;

fn app(name: &str, command: &str, arguments: &str) -> ExternalApp {
    ExternalApp {
        name: name.into(),
        command: command.into(),
        arguments: arguments.into(),
    }
}

fn starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    crate::scheduling::schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

/// A program every machine running the tests has, and its argument form
/// for "copy this file to that one".
fn copy_command(to: &Path) -> ExternalApp {
    if cfg!(windows) {
        app(
            "Copy",
            "cmd.exe",
            &format!("/c copy /y {XER_TOKEN} {}", to.display()),
        )
    } else {
        app("Copy", "cp", &format!("{XER_TOKEN} {}", to.display()))
    }
}

fn wait_for(path: &Path) -> bool {
    for _ in 0..200 {
        if path.is_file() && std::fs::metadata(path).is_ok_and(|m| m.len() > 0) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    false
}

#[test]
fn arguments_split_at_spaces_and_quotes_group_words() {
    assert_eq!(
        split_arguments(r#"/c  "C:\My Files\a.xer" b"#).unwrap(),
        vec!["/c", r"C:\My Files\a.xer", "b"]
    );
    assert_eq!(split_arguments("").unwrap(), Vec::<String>::new());
    assert_eq!(split_arguments(r#""""#).unwrap(), vec![""]);
    assert!(split_arguments(r#"a "b"#).is_err());
    // Nothing is interpreted: a pipe or a variable is an ordinary word.
    assert_eq!(
        split_arguments("a|b %PATH%").unwrap(),
        vec!["a|b", "%PATH%"]
    );
}

#[test]
fn the_xer_token_is_replaced_in_any_letter_case_and_inside_words() {
    let words = split_arguments(r#"--file=%XER% "%xer%" other"#).unwrap();
    let out = expand_arguments(&words, r"C:\t\p.xer");
    assert_eq!(out, vec![r"--file=C:\t\p.xer", r"C:\t\p.xer", "other"]);
    assert!(app("n", "c", "a %Xer%").uses_xer());
    assert!(!app("n", "c", "a").uses_xer());
}

#[test]
fn invalid_entries_are_refused() {
    let ok = app("Viewer", "notepad.exe", "%xer%");
    assert!(validate(&ok, &[]).is_ok());
    for (bad, why) in [
        (app("", "notepad.exe", ""), "name"),
        (app("  ", "notepad.exe", ""), "blank name"),
        (app("Viewer", "", ""), "command"),
        (app("Viewer", "\"\"", ""), "empty quotes"),
        (app("Viewer", "note|pad", ""), "pipe"),
        (app("Viewer", "a\nb", ""), "newline"),
        (app("Viewer", "notepad.exe", "a \"b"), "open quote"),
        (app("Viewer", "notepad.exe", "a\u{7}b"), "control"),
        (
            app("Viewer", r"C:\No Such Folder\nothing.exe", ""),
            "missing file",
        ),
        (app(&"x".repeat(61), "notepad.exe", ""), "long name"),
    ] {
        assert!(validate(&bad, &[]).is_err(), "{why} is refused");
    }
    assert!(
        validate(&ok, &["viewer"]).is_err(),
        "a name already listed is refused whatever its letter case"
    );
}

#[test]
fn the_list_round_trips_and_stored_junk_is_dropped() {
    let list = vec![app("A", "a.exe", "x"), app("B", r"C:\gone\b.exe", "")];
    // A path that is not on this machine stays in a stored list.
    assert_eq!(from_json(&to_json(&list)), list);
    assert!(from_json("not json").is_empty());
    let junk = r#"[{"name":"","command":"a","arguments":""},
        {"name":"A","command":"a","arguments":""},
        {"name":"a","command":"b","arguments":""},
        {"name":"C","command":"c|d","arguments":""}]"#;
    let got = from_json(junk);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].name, "A");
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "openxer-ext-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A directory link at `link` pointing to `target`, when the account may
/// make one.
fn make_dir_link(target: &Path, link: &Path) -> bool {
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link).is_ok()
    }
    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
}

#[test]
fn an_export_is_a_new_file_with_an_unpredictable_safe_name() {
    let base = scratch("export");
    let dir = crate::util::private_folder_in(&base, "launch").unwrap();
    let a = write_export(&dir, "A/B:C*", b"one").unwrap();
    let b = write_export(&dir, "A/B:C*", b"two").unwrap();
    assert_ne!(a, b, "each launch has a file of its own");
    assert_eq!(std::fs::read(&a).unwrap(), b"one");
    assert_eq!(std::fs::read(&b).unwrap(), b"two");
    let name = a.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        name.starts_with("OpenXER-A_B_C_-") && name.ends_with(".xer"),
        "{name}"
    );
    assert_eq!(a.parent().unwrap(), dir);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn the_export_folder_is_private_and_never_a_link_or_a_file() {
    let base = scratch("private");
    // The folder is made under the base and is a plain directory.
    let dir = crate::util::private_folder_in(&base, "launch").unwrap();
    assert!(dir.starts_with(&base));
    let meta = std::fs::symlink_metadata(&dir).unwrap();
    assert!(meta.is_dir() && !meta.file_type().is_symlink());
    // The real per-user folder lies under the account's own data folder
    // (OPENXER_DATA_DIR when set), or is a fresh randomly named folder in
    // the temp directory.
    let real = crate::util::private_folder(FOLDER, "OpenXER-launch").unwrap();
    match crate::util::user_data_dir() {
        Some(data) => assert!(real.starts_with(data)),
        None => assert!(real.starts_with(std::env::temp_dir())),
    }
    // A file where the folder should be is refused.
    let file = base.join("occupied");
    std::fs::write(&file, b"x").unwrap();
    assert!(crate::util::private_folder_in(&base, "occupied").is_err());
    assert!(write_export(&file, "p", b"x").is_err());
    // A link at the folder's place is refused, and nothing is written
    // through it.
    let elsewhere = scratch("elsewhere");
    let link = base.join("linked");
    if make_dir_link(&elsewhere, &link) {
        assert!(crate::util::private_folder_in(&base, "linked").is_err());
        assert!(write_export(&link, "p", b"x").is_err());
        assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);
        let _ = std::fs::remove_dir(&link);
    } else {
        eprintln!("directory links are not allowed for this account; link case skipped");
    }
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&elsewhere);
}

#[test]
fn files_are_created_exclusively_and_never_overwrite() {
    let base = scratch("exclusive");
    let dir = crate::util::private_folder_in(&base, "launch").unwrap();
    // A file already at a likely name is never opened or truncated.
    let first = crate::util::write_new_file(&dir, "OpenXER-x", "xer", b"first").unwrap();
    let again = crate::util::write_new_file(&dir, "OpenXER-x", "xer", b"second").unwrap();
    assert_ne!(first, again);
    assert_eq!(std::fs::read(&first).unwrap(), b"first");
    // create_new refuses an existing path, including a file link.
    let taken = dir.join("taken.xer");
    std::fs::write(&taken, b"keep").unwrap();
    let opened = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&taken);
    assert_eq!(
        opened.unwrap_err().kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert_eq!(std::fs::read(&taken).unwrap(), b"keep");
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn old_exports_are_purged_but_other_files_and_links_are_left() {
    let base = scratch("purge");
    let dir = crate::util::private_folder_in(&base, "launch").unwrap();
    let ours = dir.join("OpenXER-old-0.xer");
    let other = dir.join("notes.xer");
    std::fs::write(&ours, b"x").unwrap();
    std::fs::write(&other, b"x").unwrap();
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3 * 24 * 3600);
    for f in [&ours, &other] {
        std::fs::File::options()
            .write(true)
            .open(f)
            .unwrap()
            .set_modified(old)
            .unwrap();
    }
    let fresh = write_export(&dir, "p", b"y").unwrap();
    assert!(!ours.exists(), "an old export of ours is deleted");
    assert!(other.exists(), "a file that is not ours stays");
    assert!(fresh.exists());
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn launching_with_the_token_writes_a_fresh_export_and_starts_the_command() {
    let mut d = Driver::new("ext-launch", starter());
    let out = d.dir.join("seen.xer");
    d.app.external.apps = vec![copy_command(&out)];
    // An edit not yet saved to the workspace is in the export.
    d.app.project.details.get_mut(&1010).unwrap().name = "Exported edit".into();
    d.app.touch_records();
    let mut child = d.app.launch_external(0).unwrap();
    assert!(child.wait().unwrap().success(), "the command ran");
    assert!(wait_for(&out), "the export reached the command");
    let text = std::fs::read(&out).unwrap();
    let text = String::from_utf8_lossy(&text).into_owned();
    assert!(Document::parse(&text).unwrap().table("TASK").is_some());
    assert!(
        text.contains("Exported edit"),
        "the open project as it is now"
    );
    assert!(d.app.status.starts_with("Started Copy"));
}

#[test]
fn a_command_without_the_token_gets_no_export() {
    let mut d = Driver::new("ext-plain", starter());
    let command = if cfg!(windows) {
        app("Quiet", "cmd.exe", "/c exit 0")
    } else {
        app("Quiet", "true", "")
    };
    assert!(!command.uses_xer());
    d.app.external.apps = vec![command];
    let mut child = d.app.launch_external(0).unwrap();
    assert!(child.wait().unwrap().success());
}

#[test]
fn a_missing_program_is_reported_and_nothing_else_happens() {
    let mut d = Driver::new("ext-missing", starter());
    d.app.external.apps = vec![
        app("Gone", "openxer-no-such-program-anywhere", ""),
        app("Gone path", r"C:\openxer-no-such-folder\x.exe", ""),
    ];
    for i in 0..2 {
        let err = d.app.launch_external(i).unwrap_err();
        assert!(err.contains("Gone"), "{err}");
    }
    assert!(d.app.launch_external(9).is_err());
}

fn open_dialog(d: &mut Driver) {
    d.click("menu_external_apps");
    d.frames(3);
    assert!(d.has("external_apps_close"), "the dialog opened");
}

fn fill(d: &mut Driver, name: &str, command: &str, arguments: &str) {
    let dialog = d.app.external.dialog.as_mut().unwrap();
    dialog.name = name.into();
    dialog.command = command.into();
    dialog.arguments = arguments.into();
}

#[test]
fn the_dialog_adds_changes_and_deletes_and_refuses_bad_entries() {
    let mut d = Driver::new("ext-dialog", starter());
    open_dialog(&mut d);
    assert!(!d.has("external_apps_row_0"));
    assert!(!d.app.project.model.activities.is_empty());
    // Refused: no name, then no command; the list stays empty.
    fill(&mut d, "", "notepad.exe", "");
    d.click("external_apps_add");
    assert!(d.app.external.apps.is_empty());
    assert!(d.app.external.dialog.as_ref().unwrap().error.is_some());
    fill(&mut d, "Viewer", "", "");
    d.click("external_apps_add");
    assert!(d.app.external.apps.is_empty());
    // Accepted.
    fill(&mut d, "Viewer", "notepad.exe", "%xer%");
    d.click("external_apps_add");
    assert_eq!(d.app.external.apps.len(), 1);
    assert!(d.app.external.dialog.as_ref().unwrap().error.is_none());
    d.frames(2);
    assert!(d.has("external_apps_row_0"));
    // A second entry with the same name is refused.
    fill(&mut d, "VIEWER", "calc.exe", "");
    d.click("external_apps_add");
    assert_eq!(d.app.external.apps.len(), 1);
    // The saved list is in the workspace.
    let stored = d.app.store.preference(PREF_KEY).unwrap().unwrap();
    assert_eq!(from_json(&stored), d.app.external.apps);
    // Change the selected entry.
    d.click("external_apps_row_0");
    assert_eq!(d.app.external.dialog.as_ref().unwrap().name, "Viewer");
    fill(&mut d, "Viewer 2", "notepad.exe", "");
    d.app.external.dialog.as_mut().unwrap().selected = Some(0);
    d.click("external_apps_change");
    assert_eq!(d.app.external.apps[0].name, "Viewer 2");
    assert_eq!(d.app.external.apps[0].arguments, "");
    // Delete it.
    d.click("external_apps_delete");
    assert!(d.app.external.apps.is_empty());
    // Close.
    d.click("external_apps_close");
    assert!(d.app.external.dialog.is_none());
}

#[test]
fn escape_closes_the_dialog_and_the_project_is_untouched() {
    let mut d = Driver::new("ext-escape", starter());
    let before = d.app.project.model.clone();
    open_dialog(&mut d);
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.external.dialog.is_none());
    assert_eq!(d.app.project.model, before);
}

#[test]
fn view_launch_lists_the_applications_and_starts_one() {
    let mut d = Driver::new("ext-menu", starter());
    d.click("menu_bar_view");
    assert!(d.hover_text("Run Program"));
    assert!(d.has("menu_launch_none"), "an empty list says so");
    d.key(Key::Escape);
    d.frames(2);
    let out = d.dir.join("menu-seen.xer");
    d.app.external.apps = vec![copy_command(&out)];
    d.click("menu_bar_view");
    assert!(d.hover_text("Run Program"));
    assert!(d.has("menu_launch_0"));
    d.click("menu_launch_0");
    assert!(wait_for(&out), "the menu item started the command");
}
