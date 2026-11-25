//! What a panic leaves behind. The release build has no console, so without
//! this a panic closes the window and says nothing. The hook writes a crash
//! log beside the workspace and tells the user, in a native message box,
//! what happened, where the workspace is and that work since the last save
//! may be lost.
//!
//! The message box is passed in (`on_panic`), so tests never open one; only
//! `install`, called from `main`, uses the native box.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Minutes between recovery copies, as the running app last set them
/// (0: off). The panic message says whether the next start can recover.
static AUTOSAVE_MINUTES: AtomicU32 = AtomicU32::new(0);
/// The first panic on the window's thread already showed its message.
static SHOWN: AtomicBool = AtomicBool::new(false);

/// Tell the panic message how often recovery copies are written.
pub(crate) fn note_autosave(minutes: u32) {
    AUTOSAVE_MINUTES.store(minutes, Ordering::Relaxed);
}

/// A native error message box with an OK button.
pub(crate) fn native_error_box(title: &str, text: &str) {
    let _ = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(title)
        .set_description(text)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

/// The workspace `main` will open: `--database <path>` or the default.
fn workspace_from(args: &[std::ffi::OsString]) -> PathBuf {
    args.windows(2)
        .find(|a| a[0] == "--database")
        .map(|a| PathBuf::from(&a[1]))
        .unwrap_or_else(crate::store::Store::default_path)
}

/// Install the panic hook for the application run with `args`.
pub(crate) fn install(args: &[std::ffi::OsString]) {
    let workspace = workspace_from(args);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        let what = payload_text(info.payload());
        let at = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let thread = std::thread::current();
        let main = thread.name() == Some("main");
        if main {
            // The app closes: Chat's work folders (project snapshots) go.
            crate::chat::live::work::remove_all();
        }
        on_panic(
            &workspace,
            &what,
            &at,
            thread.name().unwrap_or("unnamed"),
            main,
            &native_error_box,
        );
    }));
}

/// The text a panic was raised with.
fn payload_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "an unknown error".into())
}

/// Write the crash log and, for a panic on the window's thread (which
/// closes the app), show the message once. Returns the log's path.
pub(crate) fn on_panic(
    workspace: &Path,
    what: &str,
    at: &str,
    thread: &str,
    main: bool,
    show: &dyn Fn(&str, &str),
) -> Option<PathBuf> {
    let log = write_log(workspace, &report(workspace, what, at, thread));
    if main && !SHOWN.swap(true, Ordering::SeqCst) {
        let minutes = AUTOSAVE_MINUTES.load(Ordering::Relaxed);
        show(
            "OpenXER has stopped",
            &user_message(workspace, what, log.as_deref(), minutes),
        );
    }
    log
}

/// What the user reads: plain words, where the workspace is, what may be
/// lost and where the log went.
pub(crate) fn user_message(
    workspace: &Path,
    what: &str,
    log: Option<&Path>,
    autosave_minutes: u32,
) -> String {
    let mut text = format!(
        "OpenXER ran into a problem it could not get past and has to close.\n\n\
         The problem: {what}\n\n\
         Work since your last save may be lost. Projects you saved are safe in \
         your workspace:\n{}\n",
        workspace.display()
    );
    if autosave_minutes > 0 {
        text.push_str(&format!(
            "\nOpenXER keeps a recovery copy of unsaved changes every {autosave_minutes} \
             minute{}. The next time it starts, it offers to recover them.\n",
            if autosave_minutes == 1 { "" } else { "s" }
        ));
    }
    match log {
        Some(log) => text.push_str(&format!(
            "\nA crash report was written to:\n{}",
            log.display()
        )),
        None => text.push_str("\nThe crash report could not be written."),
    }
    text
}

/// The crash log's text.
fn report(workspace: &Path, what: &str, at: &str, thread: &str) -> String {
    format!(
        "OpenXER {} crash report\nTime: {}\nWorkspace: {}\nThread: {thread}\nPanic: {what}\nLocation: {at}\n\nBacktrace:\n{}\n",
        env!("CARGO_PKG_VERSION"),
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        workspace.display(),
        std::backtrace::Backtrace::force_capture(),
    )
}

/// Write `text` to `openxer-crash-<time>.log` beside the workspace, or in
/// the temporary folder when that folder cannot be written.
fn write_log(workspace: &Path, text: &str) -> Option<PathBuf> {
    let name = format!(
        "openxer-crash-{}.log",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    );
    let beside = workspace.parent().map(|d| d.join(&name));
    beside
        .into_iter()
        .chain([std::env::temp_dir().join(&name)])
        .find(|path| std::fs::write(path, text).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn a_panic_writes_a_log_beside_the_workspace_and_tells_the_user_once() {
        let dir = std::env::temp_dir().join(format!("openxer-crash-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let workspace = dir.join("openxer.db");
        let shown = RefCell::new(vec![]);
        let show = |title: &str, text: &str| shown.borrow_mut().push((title.into(), text.into()));
        SHOWN.store(false, Ordering::SeqCst);
        note_autosave(5);
        let log = on_panic(
            &workspace,
            "index out of range",
            "src/x.rs:1",
            "main",
            true,
            &show,
        )
        .expect("the log is written");
        assert_eq!(
            log.parent(),
            Some(dir.as_path()),
            "the log sits beside the workspace"
        );
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(text.contains("index out of range") && text.contains("src/x.rs:1"));
        {
            let shown: &Vec<(String, String)> = &shown.borrow();
            assert_eq!(shown.len(), 1);
            let message = &shown[0].1;
            assert!(
                message.contains(&workspace.display().to_string()),
                "{message}"
            );
            assert!(
                message.contains("since your last save may be lost"),
                "{message}"
            );
            assert!(message.contains("every 5 minutes"), "{message}");
            assert!(message.contains(&log.display().to_string()), "{message}");
            assert!(!message.contains('\u{2014}') && !message.contains('\u{2013}'));
        }
        // A second panic, or one on another thread, only logs.
        on_panic(&workspace, "again", "", "main", true, &show);
        on_panic(&workspace, "worker", "", "autosave", false, &show);
        assert_eq!(shown.borrow().len(), 1);
        note_autosave(0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn without_autosave_the_message_does_not_promise_recovery() {
        let text = user_message(Path::new("C:/w/openxer.db"), "boom", None, 0);
        assert!(!text.contains("recover"), "{text}");
        assert!(text.contains("could not be written"));
    }
}
