use super::*;

/// `n` and the noun in the right number: "1 project", "2 projects".
pub(crate) fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

pub(super) fn drag_motion(response: &egui::Response, ctx: &egui::Context) -> Vec2 {
    if response.drag_started() {
        // A frame can move to the control and press; pre-press motion is not a drag.
        ctx.input(|i| {
            i.pointer
                .interact_pos()
                .zip(i.pointer.press_origin())
                .map_or(Vec2::ZERO, |(position, origin)| position - origin)
        })
    } else {
        response.drag_delta()
    }
}
pub(super) fn segment_distance(point: Pos2, start: Pos2, end: Pos2) -> f32 {
    let delta = end - start;
    let length = delta.length_sq();
    if length == 0. {
        point.distance(start)
    } else {
        point.distance(start + delta * ((point - start).dot(delta) / length).clamp(0., 1.))
    }
}
pub(super) fn displayed_start(a: &Activity, leveled: bool) -> Option<i64> {
    a.act_start
        .or(if leveled { a.restart } else { a.early_start })
        .or(a.target_start)
}
pub(super) fn displayed_finish(a: &Activity, leveled: bool) -> Option<i64> {
    a.act_finish
        .or(if leveled { a.reend } else { a.early_finish })
        .or(a.target_finish)
}

/// Opens a file with the system's default program. Nothing goes through a
/// command interpreter, so the path is never parsed for shell syntax. Tests
/// never start a program.
#[cfg(all(not(test), windows))]
pub(crate) fn open_in_default_app(path: &std::path::Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(
            hwnd: *mut core::ffi::c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> *mut core::ffi::c_void;
    }
    let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide.contains(&0) {
        return Err(format!("Cannot open {}", path.display()));
    }
    let file: Vec<u16> = wide.into_iter().chain(Some(0)).collect();
    let verb: Vec<u16> = "open\0".encode_utf16().collect();
    // SAFETY: both strings are NUL terminated and outlive the call; the
    // other arguments are null, which the function accepts.
    let result = unsafe {
        ShellExecuteW(
            core::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            core::ptr::null(),
            core::ptr::null(),
            1,
        )
    };
    // Values above 32 mean success.
    if result as usize > 32 {
        Ok(())
    } else {
        Err(format!("Cannot open {}", path.display()))
    }
}

#[cfg(all(not(test), not(windows)))]
pub(crate) fn open_in_default_app(path: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Cannot open {}: {e}", path.display()))
}

#[cfg(test)]
thread_local! {
    /// What the test thread asked to open, in order.
    pub(crate) static OPENED: std::cell::RefCell<Vec<std::path::PathBuf>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
pub(crate) fn open_in_default_app(path: &std::path::Path) -> Result<(), String> {
    OPENED.with(|o| o.borrow_mut().push(path.to_path_buf()));
    Ok(())
}

/// The environment variable that moves OpenXER's per-user data folder (the
/// workspace, its backups and crash reports, the files written for other
/// programs) for tests and managed deployments. Unset: `OpenXER` in the
/// per-user base folder (`%LOCALAPPDATA%` on Windows, the XDG data folder
/// elsewhere) for an installed copy, beside the EXE for a portable one.
pub(crate) const DATA_DIR_VAR: &str = openxer_automation::locate::DATA_DIR_VAR;

/// The file the installer puts beside `OpenXER.exe`. Its presence makes the
/// copy an installed one, which never keeps data in the program folder
/// (`Store::default_path_in`). A portable copy has none. The rule itself
/// lives in `openxer_automation::locate`; the tests name the marker here.
#[cfg(test)]
pub(crate) const INSTALLED_MARKER: &str = openxer_automation::locate::INSTALLED_MARKER;

/// The data folder `OPENXER_DATA_DIR` names (`value`), made absolute; None
/// when it is unset or empty.
pub(crate) fn data_dir_override(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    openxer_automation::locate::data_dir_override(value)
}

/// The per-user data folder: `OPENXER_DATA_DIR` when set, else
/// `OpenXER` in the per-user base folder
/// (`openxer_automation::locate::user_data_base`: `%LOCALAPPDATA%` on
/// Windows, the XDG data folder elsewhere); None when neither is known.
pub(crate) fn user_data_dir() -> Option<PathBuf> {
    data_dir_override(std::env::var_os(DATA_DIR_VAR)).or_else(|| {
        openxer_automation::locate::user_data_base()
            .filter(|v| !v.is_empty())
            .map(|base| PathBuf::from(base).join("OpenXER"))
    })
}

/// A private folder for files this program writes for another program to
/// read: `<data folder>\<name>` ([`user_data_dir`]) when the account has one, else a
/// fresh folder with a random name (`<temp_prefix>-<random>`) created inside
/// the temp directory with `create_dir`, never `create_dir_all`. A folder
/// that is a link, or not a folder, is refused, so another account cannot
/// redirect the output.
pub(crate) fn private_folder(name: &str, temp_prefix: &str) -> Result<PathBuf, String> {
    if let Some(base) = user_data_dir() {
        return private_folder_in(&base, name);
    }
    use std::hash::{BuildHasher, Hasher};
    for attempt in 0..32u32 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u32(attempt);
        h.write_u32(std::process::id());
        let dir = std::env::temp_dir().join(format!("{temp_prefix}-{:016x}", h.finish()));
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("Cannot create {}: {e}", dir.display())),
        }
    }
    Err("Cannot create a private folder".into())
}

/// `<base>\<name>`, made if missing and checked to be a plain folder.
pub(crate) fn private_folder_in(base: &Path, name: &str) -> Result<PathBuf, String> {
    let dir = base.join(name);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    refuse_link(dir)
}

/// The folder itself must be neither a link nor a file.
pub(crate) fn refuse_link(dir: PathBuf) -> Result<PathBuf, String> {
    let meta = std::fs::symlink_metadata(&dir)
        .map_err(|e| format!("Cannot read {}: {e}", dir.display()))?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(format!(
            "{} is not a plain folder; refusing to write there",
            dir.display()
        ));
    }
    Ok(dir)
}

/// Deletes the oldest files named `<prefix>...<.extension>` in `dir` so at
/// most `keep` of them remain. Only for folders the program chose itself
/// (automatic backups, files sent to the PDF viewer), never for a place the
/// user picked. Files that cannot be deleted (open in another program) are
/// left; links and folders are never touched.
pub(crate) fn prune_old(dir: &Path, prefix: &str, extension: &str, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let suffix = format!(".{extension}");
    let mut files: Vec<(std::time::SystemTime, String, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let lower = name.to_ascii_lowercase();
            if !name.starts_with(prefix) || !lower.ends_with(&suffix) {
                return None;
            }
            let meta = std::fs::symlink_metadata(e.path()).ok()?;
            if !meta.is_file() {
                return None;
            }
            let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
            Some((modified, name, e.path()))
        })
        .collect();
    if files.len() <= keep {
        return;
    }
    files.sort();
    let extra = files.len() - keep;
    for (_, _, path) in files.into_iter().take(extra) {
        let _ = std::fs::remove_file(path);
    }
}

/// Writes `bytes` to a new file `<prefix>-<random>.<extension>` in `dir`. The
/// file is created exclusively (an existing path or link is never opened or
/// truncated); a taken name is retried with another suffix.
pub(crate) fn write_new_file(
    dir: &Path,
    prefix: &str,
    extension: &str,
    bytes: &[u8],
) -> Result<PathBuf, String> {
    use std::hash::{BuildHasher, Hasher};
    use std::io::Write;
    refuse_link(dir.to_path_buf())?;
    for attempt in 0..32u32 {
        // RandomState is seeded from the system's random source.
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u32(attempt);
        h.write_u32(std::process::id());
        let path = dir.join(format!("{prefix}-{:016x}.{extension}", h.finish()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                // A file cut short (a full disk) is not handed on: it goes.
                if let Err(e) = file.write_all(bytes).and_then(|()| file.sync_all()) {
                    drop(file);
                    let _ = std::fs::remove_file(&path);
                    return Err(format!("Cannot write {}: {e}", path.display()));
                }
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("Cannot create {}: {e}", path.display())),
        }
    }
    Err(format!("Cannot find a free file name in {}", dir.display()))
}

/// Processor time the calling thread has used, for tests that bound how
/// long some work takes. Unlike the wall clock it does not grow while the
/// thread waits for a busy machine (other test threads, other builds), so a
/// bound on it still catches slow code but not a loaded computer. Work done
/// on other threads is not counted: time only work on the test thread.
#[cfg(test)]
pub(crate) struct CpuTimer(std::time::Duration);

#[cfg(test)]
impl CpuTimer {
    pub(crate) fn start() -> Self {
        Self(thread_cpu_time())
    }
    pub(crate) fn elapsed(&self) -> std::time::Duration {
        thread_cpu_time().saturating_sub(self.0)
    }
}

#[cfg(all(test, windows))]
fn thread_cpu_time() -> std::time::Duration {
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThread() -> isize;
        fn GetThreadTimes(
            thread: isize,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }
    let mut t = [FileTime::default(); 4];
    let [creation, exit, kernel, user] = &mut t;
    // SAFETY: the pseudo handle of the current thread is always valid and
    // every pointer is to a live FileTime.
    let ok = unsafe { GetThreadTimes(GetCurrentThread(), creation, exit, kernel, user) };
    assert!(ok != 0, "GetThreadTimes failed");
    // Units of 100 ns.
    let ticks = |f: &FileTime| (u64::from(f.high) << 32) | u64::from(f.low);
    std::time::Duration::from_nanos((ticks(kernel) + ticks(user)) * 100)
}

#[cfg(all(test, not(windows)))]
fn thread_cpu_time() -> std::time::Duration {
    // No portable per-thread clock in std: the wall clock since first use.
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cpu_timer_counts_work_not_waiting() {
        let timer = CpuTimer::start();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let slept = timer.elapsed();
        // Fixed work, well over the clock's 15.6 ms tick in a debug build.
        let mut x = 0u64;
        for _ in 0..50_000_000u64 {
            x = std::hint::black_box(x.wrapping_mul(6364136223846793005).wrapping_add(1));
        }
        let worked = timer.elapsed().saturating_sub(slept);
        if cfg!(windows) {
            assert!(slept < std::time::Duration::from_millis(100), "{slept:?}");
        }
        assert!(worked >= std::time::Duration::from_millis(15), "{worked:?}");
    }

    #[test]
    fn prune_keeps_the_newest_matching_files_only() {
        let dir = std::env::temp_dir().join(format!("openxer-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for k in 0..5 {
            let path = dir.join(format!("OpenXER-print-2026110{k}-000000-x.pdf"));
            std::fs::write(&path, b"x").unwrap();
            let when = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000 + k);
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(when)
                .unwrap();
        }
        std::fs::write(dir.join("mine.pdf"), b"user file").unwrap();
        std::fs::write(dir.join("OpenXER-print-notes.txt"), b"other").unwrap();
        prune_old(&dir, "OpenXER-print-", "pdf", 2);
        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            [
                "OpenXER-print-20261103-000000-x.pdf",
                "OpenXER-print-20261104-000000-x.pdf",
                "OpenXER-print-notes.txt",
                "mine.pdf"
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
