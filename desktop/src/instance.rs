//! The running app as other programs see it: the named mutex Setup checks
//! before it replaces or removes the program files, and the file a
//! double-clicked `.xer` (or `--open`) asks the app to open.
use std::ffi::OsString;
use std::path::PathBuf;

/// The mutex the app holds while it runs. `tools/installer/openxer.iss`
/// names it in `AppMutex`, so Setup and the uninstaller ask the user to
/// close OpenXER first instead of replacing files under it.
#[cfg(windows)]
pub(crate) const APP_MUTEX: &str = "OpenXER.App";

/// Create the app's named mutex, in this session and globally (Setup run by
/// another user of the PC sees the global one). The handles stay open until
/// the process ends, which is when the system releases them.
#[cfg(windows)]
pub(crate) fn hold_app_mutex() {
    #[link(name = "kernel32")]
    extern "system" {
        fn CreateMutexW(
            attributes: *const core::ffi::c_void,
            initial_owner: i32,
            name: *const u16,
        ) -> *mut core::ffi::c_void;
    }
    for name in [APP_MUTEX.to_string(), format!("Global\\{APP_MUTEX}")] {
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        // SAFETY: the name is NUL terminated and outlives the call; null
        // attributes are accepted. The handle is deliberately never closed.
        unsafe {
            CreateMutexW(core::ptr::null(), 0, wide.as_ptr());
        }
    }
}

#[cfg(not(windows))]
pub(crate) fn hold_app_mutex() {}

/// Options that take the next argument as their value.
const VALUED: [&str; 4] = ["--database", "--open", "--project", "--capture-dir"];

/// The file the app is asked to open: `--open <path>`, or the first plain
/// argument (Explorer starts `OpenXER.exe "<file>"` for a double-clicked
/// `.xer`).
pub(crate) fn file_to_open(args: &[OsString]) -> Option<PathBuf> {
    if let Some(a) = args.windows(2).find(|a| a[0] == "--open") {
        return Some(PathBuf::from(&a[1]));
    }
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if VALUED.iter().any(|v| a == *v) {
            i += 2;
            continue;
        }
        if !a.to_string_lossy().starts_with("--") {
            return Some(PathBuf::from(a));
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn the_file_comes_from_open_or_the_first_plain_argument() {
        assert_eq!(file_to_open(&args(&[])), None);
        assert_eq!(
            file_to_open(&args(&[r"C:\Jobs\Plant A\Ünit 2.xer"])),
            Some(PathBuf::from(r"C:\Jobs\Plant A\Ünit 2.xer"))
        );
        assert_eq!(
            file_to_open(&args(&["--open", "a.xer", "b.xer"])),
            Some(PathBuf::from("a.xer"))
        );
        // A value of another option is never taken for the file.
        assert_eq!(
            file_to_open(&args(&["--database", "w.db", "--project", "7"])),
            None
        );
        assert_eq!(
            file_to_open(&args(&["--database", "w.db", "x.xer"])),
            Some(PathBuf::from("x.xer"))
        );
    }
}
