//! Files the app writes for the user (print PDFs, reports, schedule logs,
//! exports) are written whole or not at all: a write that fails leaves the
//! file that was there, and no part-written file beside it. A typed or
//! suggested export file name that is there already is replaced only when
//! the user says so.
use super::*;
use crate::qa_sweep::Driver;

fn scheduled_starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

/// Files in `dir` other than the workspace's own.
fn user_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with("workspace.db"))
        .collect();
    names.sort();
    names
}

/// Makes replacing `path` fail (`on`) or work again. On Windows the file
/// itself is made read-only, which a rename over it respects.
#[cfg(windows)]
fn set_read_only(path: &Path, on: bool) {
    let mut p = std::fs::metadata(path).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    p.set_readonly(on);
    std::fs::set_permissions(path, p).unwrap();
}

/// Makes replacing `path` fail (`on`) or work again. A rename over a
/// read-only file succeeds here when its folder is writable, so the folder
/// is made read-only instead: no temporary file can be made beside `path`.
#[cfg(unix)]
fn set_read_only(path: &Path, on: bool) {
    use std::os::unix::fs::PermissionsExt;
    let folder = path.parent().unwrap();
    let mut p = std::fs::metadata(folder).unwrap().permissions();
    let mode = p.mode();
    p.set_mode(if on { mode & !0o222 } else { mode | 0o200 });
    std::fs::set_permissions(folder, p).unwrap();
    if on {
        let probe = folder.join(".probe");
        let made = std::fs::write(&probe, b"").is_ok();
        let _ = std::fs::remove_file(&probe);
        assert!(
            !made,
            "a read-only folder did not stop a write (running as root?); run the tests as a normal user"
        );
    }
}

/// A print PDF that cannot replace the file there (here a read-only file)
/// leaves that file as it was and no temporary file; once it can, the PDF
/// is written whole.
#[test]
fn a_failed_print_pdf_leaves_the_file_that_was_there() {
    let mut d = Driver::new("write-pdf", scheduled_starter());
    let out = d.dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let path = out.join("plan.pdf");
    std::fs::write(&path, b"the good copy").unwrap();
    set_read_only(&path, true);
    let setup = d.app.page_setup();
    let error = d.app.write_pdf(&path, &setup).unwrap_err();
    assert!(error.contains("plan.pdf"), "names the file: {error}");
    assert_eq!(std::fs::read(&path).unwrap(), b"the good copy");
    assert_eq!(user_files(&out), ["plan.pdf"], "no part-written file");
    set_read_only(&path, false);
    let pages = d.app.write_pdf(&path, &setup).unwrap();
    assert!(pages > 0);
    assert!(std::fs::read(&path).unwrap().starts_with(b"%PDF"));
    assert_eq!(user_files(&out), ["plan.pdf"]);
}

/// A report written to a name that is a folder fails with the name and
/// leaves nothing; an appended schedule log keeps the old text, with one
/// byte order mark at the start.
#[test]
fn reports_and_schedule_logs_are_written_whole() {
    let dir = std::env::temp_dir().join(format!(
        "openxer-write-reports-{}-{}",
        std::process::id(),
        chrono::Local::now().format("%H%M%S%f")
    ));
    let folder = dir.join("report.html");
    std::fs::create_dir_all(&folder).unwrap();
    let error = crate::reports::rpt_run::write_file(&folder, b"<html>").unwrap_err();
    assert!(error.contains("report.html"), "{error}");
    assert!(folder.is_dir());
    assert_eq!(user_files(&dir), ["report.html"], "no temporary file");
    let log = dir.join("SCHEDLOG.TXT");
    crate::schedule_dialogs::write_log(&log, &["first run".into()], false, false).unwrap();
    crate::schedule_dialogs::write_log(&log, &["second run".into()], false, true).unwrap();
    let text = std::fs::read_to_string(&log).unwrap();
    assert_eq!(text, "\u{feff}first run\r\nsecond run\r\n");
    set_read_only(&log, true);
    assert!(crate::schedule_dialogs::write_log(&log, &["third run".into()], false, true).is_err());
    assert_eq!(std::fs::read_to_string(&log).unwrap(), text, "unchanged");
    set_read_only(&log, false);
    let _ = std::fs::remove_dir_all(dir);
}

/// File > Export to a typed (or suggested) name of a file that is there
/// already warns first and leaves it; Finish again replaces it. A name
/// that is a folder is refused.
#[test]
fn export_asks_before_replacing_a_file_that_is_there() {
    let mut d = Driver::new("export-replace", scheduled_starter());
    let path = d.dir.join("existing.xer");
    std::fs::write(&path, b"keep me").unwrap();
    let finish = |d: &mut Driver, target: &Path| {
        let w = d.app.interchange.export.as_mut().unwrap();
        w.path = target.display().to_string();
        w.step = 3;
        d.app.export_finish();
    };
    d.app.open_export_wizard();
    let folder = d.dir.clone();
    finish(&mut d, &folder);
    let error = d.app.interchange.export.as_ref().unwrap().error.clone();
    assert!(error.is_some_and(|e| e.contains("is a folder")));
    finish(&mut d, &path);
    let w = d.app.interchange.export.as_ref().expect("still open");
    assert!(
        w.error
            .as_deref()
            .is_some_and(|e| e.contains("already exists")),
        "{:?}",
        w.error
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"keep me", "not replaced");
    finish(&mut d, &path);
    assert!(
        d.app.interchange.export.is_none(),
        "{:?}",
        d.app.interchange.export.as_ref().map(|w| w.error.clone())
    );
    let doc = Document::read(&path).unwrap();
    assert!(!doc.projects().is_empty(), "replaced by the export");
}

/// A file there already that cannot be replaced (read-only here) keeps its
/// text after `write` fails, and no temporary file is left beside it; once
/// it can be replaced, `write` succeeds.
fn assert_failed_write_keeps_the_old_file(
    dir: &Path,
    name: &str,
    mut write: impl FnMut(&Path) -> Result<(), String>,
) {
    let out = dir.join(format!("out-{}", name.replace('.', "-")));
    std::fs::create_dir_all(&out).unwrap();
    let path = out.join(name);
    std::fs::write(&path, b"the good copy").unwrap();
    set_read_only(&path, true);
    let error = write(&path).unwrap_err();
    assert!(error.contains(name), "names the file: {error}");
    assert_eq!(std::fs::read(&path).unwrap(), b"the good copy");
    assert_eq!(user_files(&out), [name], "no part-written file");
    set_read_only(&path, false);
    write(&path).unwrap();
    assert_ne!(std::fs::read(&path).unwrap(), b"the good copy");
    assert_eq!(user_files(&out), [name]);
}

/// Open Layout > Export is written whole or not at all.
#[test]
fn a_failed_layout_export_leaves_the_file_that_was_there() {
    let mut d = Driver::new("write-layout", scheduled_starter());
    let dir = d.dir.clone();
    assert_failed_write_keeps_the_old_file(&dir, "plan.lyt", |path| {
        d.app
            .export_layout_file(crate::grid::layout::STANDARD_NAME, path)
    });
}

/// Global Change's file export and its report are written whole or not at
/// all.
#[test]
fn failed_global_change_exports_leave_the_file_that_was_there() {
    use crate::editing::dialogs::write_global_change;
    use crate::editing::gc_report::{write_gc_report, GcReport};
    let d = Driver::new("write-gc", scheduled_starter());
    let change = crate::editing::gc::GlobalChange {
        name: "Shift".into(),
        ..Default::default()
    };
    assert_failed_write_keeps_the_old_file(&d.dir, "shift.json", |path| {
        write_global_change(path, &change)
    });
    let report = GcReport::new("Shift", vec![]);
    assert_failed_write_keeps_the_old_file(&d.dir, "shift.txt", |path| {
        write_gc_report(path, &report, false)
    });
    assert_failed_write_keeps_the_old_file(&d.dir, "shift.html", |path| {
        write_gc_report(path, &report, true)
    });
}

/// Copy to Excel from a usage spreadsheet is written whole or not at all;
/// the failure is shown to the user with the file it was for.
#[test]
fn a_failed_usage_spreadsheet_export_leaves_the_file_that_was_there() {
    let mut d = Driver::new("write-usage", scheduled_starter());
    d.home();
    d.app
        .set_bottom(Some(crate::panes::BottomView::ActivityUsageSheet));
    d.frames(4);
    let dir = d.dir.clone();
    assert_failed_write_keeps_the_old_file(&dir, "usage.xlsx", |path| {
        d.app.error = None;
        d.app.copy_usage_to_excel_to(path);
        match d.app.error.take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    });
}
