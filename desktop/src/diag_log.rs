//! Lightweight append-only diagnostic log written beside the workspace
//! database (`<workspace_dir>/openxer-diag.log`, rotated at 2 MB keeping one
//! backup). Each line is flushed immediately so a hard kill still preserves
//! the last phase entered.
//!
//! Privacy rule: log structural counts, flags, phase names, timings and
//! generic error messages only. Never log client project names, activity IDs,
//! activity names or client file paths.
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

const MAX_BYTES: u64 = 2 * 1024 * 1024;
const LOG_NAME: &str = "openxer-diag.log";
const BACKUP_NAME: &str = "openxer-diag.log.1";

static LOG_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug)]
pub(crate) struct DiagLog {
    path: PathBuf,
    backup: PathBuf,
    max_bytes: u64,
}

impl DiagLog {
    /// Diagnostic log beside `store_path` (e.g. `openxer.db` -> `openxer-diag.log`).
    pub(crate) fn for_store(store_path: &Path) -> Self {
        let dir = store_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        Self {
            path: dir.join(LOG_NAME),
            backup: dir.join(BACKUP_NAME),
            max_bytes: MAX_BYTES,
        }
    }

    /// Path of the active log file.
    #[cfg(test)]
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Append one timestamped line and flush immediately. Errors are ignored so
    /// logging never interrupts scheduling.
    pub(crate) fn line(&self, msg: &str) {
        let _guard = LOG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let clean: String = msg
            .chars()
            .map(|c| if c == '\r' || c == '\n' { ' ' } else { c })
            .collect();
        if let Ok(meta) = fs::metadata(&self.path) {
            if meta.len() >= self.max_bytes {
                let _ = fs::remove_file(&self.backup);
                let _ = fs::rename(&self.path, &self.backup);
            }
        }
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        else {
            return;
        };
        let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let pid = std::process::id();
        let _ = writeln!(file, "{stamp} pid={pid} {clean}");
        let _ = file.flush();
    }

    pub(crate) fn phase_start(&self, op: &str, phase: &str) {
        self.line(&format!("{op} phase={phase} state=start"));
    }

    pub(crate) fn phase_done(&self, op: &str, phase: &str, elapsed_ms: u128) {
        self.line(&format!(
            "{op} phase={phase} state=done elapsed_ms={elapsed_ms}"
        ));
    }

    pub(crate) fn error(&self, op: &str, err: &str) {
        self.line(&format!("{op} state=error message={err}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "openxer-diag-test-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writes_beside_store_and_rotates_with_one_backup() {
        let dir = temp_dir("rotate");
        let mut log = DiagLog::for_store(&dir.join("workspace.db"));
        log.max_bytes = 120;
        assert_eq!(log.path(), dir.join(LOG_NAME));

        log.phase_start("schedule", "validate");
        log.phase_done("schedule", "validate", 12);
        log.line("schedule state=done total_ms=42");

        let active = fs::read_to_string(&log.path).unwrap();
        let backup = fs::read_to_string(&log.backup).unwrap();
        assert!(active.contains("total_ms=42"), "active: {active}");
        assert!(
            backup.contains("phase=validate"),
            "backup kept rotated lines: {backup}"
        );

        let _ = fs::remove_dir_all(dir);
    }
}
