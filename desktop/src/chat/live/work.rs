//! A conversation's own folder and the project snapshot inside it. Each
//! conversation gets a fresh folder in the temp directory (never the data
//! folder), named `openxer-chat-<random>`; the snapshot the tools read is
//! `snapshot.xer` in it. Every folder made is listed here until it is
//! removed, so closing the app and a panic on the window's thread remove
//! what is left ([`remove_all`]), and a later start clears folders a
//! killed run left behind a day ago or more ([`remove_stale`]; a second
//! copy of the app may be running, so nothing newer is touched).
use openxer_xer::ProjectData;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The start of every work folder's name.
pub(crate) const PREFIX: &str = "openxer-chat-";
/// The snapshot's file name in a work folder.
pub(crate) const SNAPSHOT: &str = "snapshot.xer";

/// Work folders that exist now.
static LIVE: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// A new folder name under `base` that does not exist yet (an agent host
/// session creates it, so it knows the folder is its own).
pub(crate) fn fresh_path(base: &Path) -> PathBuf {
    use std::hash::{BuildHasher, Hasher};
    for attempt in 0..64u32 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u32(attempt);
        h.write_u32(std::process::id());
        let dir = base.join(format!("{PREFIX}{:016x}", h.finish()));
        if !dir.exists() {
            note(&dir);
            return dir;
        }
    }
    let dir = base.join(format!("{PREFIX}{}", std::process::id()));
    note(&dir);
    dir
}

fn note(dir: &Path) {
    if let Ok(mut live) = LIVE.lock() {
        live.push(dir.to_path_buf());
    }
}

/// Remove `dir`; it stays listed while it is still there (a stopped
/// agent can hold a file a moment longer), so exit removes it then.
pub(crate) fn remove(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
    if !dir.exists() {
        if let Ok(mut live) = LIVE.lock() {
            live.retain(|d| d != dir);
        }
    }
}

/// Remove every work folder this process made (app exit, panic).
pub(crate) fn remove_all() {
    let dirs = LIVE
        .lock()
        .map(|mut l| std::mem::take(&mut *l))
        .unwrap_or_default();
    for dir in dirs {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Whether this process lists `dir` as a work folder that exists.
#[cfg(test)]
pub(crate) fn listed(dir: &Path) -> bool {
    LIVE.lock()
        .map(|l| l.iter().any(|d| d == dir))
        .unwrap_or(false)
}

/// How old a work folder must be before a start removes it.
pub(crate) const STALE: std::time::Duration = std::time::Duration::from_secs(24 * 3600);

/// Work folders in `base` last changed `STALE` or longer ago: left by a run
/// that did not end cleanly.
pub(crate) fn remove_stale(base: &Path) {
    let now = std::time::SystemTime::now();
    for entry in std::fs::read_dir(base).into_iter().flatten().flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with(PREFIX) || !entry.path().is_dir() {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age >= STALE);
        if old {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Write `project` as the snapshot in `dir` (made if missing), whole or not
/// at all: written beside it, then moved over it.
pub(crate) fn write_snapshot(dir: &Path, project: &ProjectData) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Cannot make {}: {e}", dir.display()))?;
    let path = dir.join(SNAPSHOT);
    let part = dir.join(format!("{SNAPSHOT}.part"));
    project
        .updated_document()
        .write(&part)
        .map_err(|e| format!("Cannot write the project for the assistant: {e}"))?;
    std::fs::rename(&part, &path)
        .map_err(|e| format!("Cannot write the project for the assistant: {e}"))?;
    Ok(path)
}
