//! `export_pdf`: prints a project's layout to a new PDF. Printing paints the
//! desktop's own table, Gantt and network, so this command runs the desktop
//! binary's headless `export-pdf` mode (no window opens) and returns its
//! summary. The binary is `OPENXER_DESKTOP` when set, otherwise `OpenXER`
//! next to this executable.
use crate::storage;
use crate::{AutomationError, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn args(message: impl Into<String>) -> AutomationError {
    AutomationError::new("invalid_arguments", message)
}

/// The desktop executable that renders the pages.
pub fn desktop_binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("OPENXER_DESKTOP") {
        return Some(PathBuf::from(path));
    }
    let name = if cfg!(windows) {
        "OpenXER.exe"
    } else {
        "OpenXER"
    };
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    // Test runners sit one folder down, in `deps`.
    [dir.join(name), dir.parent()?.join(name)]
        .into_iter()
        .find(|p| p.is_file())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn export_pdf(
    input: &Path,
    output: &Path,
    project_id: Option<i64>,
    workspace_id: Option<i64>,
    layout: Option<&str>,
    view: Option<&str>,
    paper: Option<&str>,
    orientation: Option<&str>,
) -> Result<Value> {
    let is_pdf = output
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
    if !is_pdf {
        return Err(args("Output must end in .pdf"));
    }
    if std::fs::symlink_metadata(output).is_ok() {
        return Err(AutomationError::new(
            "output_exists",
            format!("Destination already exists: {}", output.display()),
        ));
    }
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Err(storage::io("Output parent directory does not exist"));
    }
    let mut command = vec![
        "export-pdf".to_string(),
        "--output".into(),
        output.display().to_string(),
    ];
    if storage::is_database(input)? {
        // Resolve the workspace row the same way every other command does.
        let loaded = storage::load(input, project_id, workspace_id)?;
        command.extend(["--database".into(), input.display().to_string()]);
        if let Some(id) = loaded.workspace_id {
            command.extend(["--workspace-id".into(), id.to_string()]);
        }
    } else {
        if workspace_id.is_some() {
            return Err(args("workspace_id needs an OpenXER workspace input"));
        }
        command.extend(["--input".into(), input.display().to_string()]);
        if let Some(id) = project_id {
            command.extend(["--project".into(), id.to_string()]);
        }
    }
    for (flag, value) in [
        ("--layout", layout),
        ("--view", view),
        ("--paper", paper),
        ("--orientation", orientation),
    ] {
        if let Some(v) = value {
            command.extend([flag.to_string(), v.to_string()]);
        }
    }
    let binary = desktop_binary().ok_or_else(|| {
        AutomationError::new(
            "desktop_missing",
            "The OpenXER desktop executable renders PDFs; place it next to openxer-cli or set OPENXER_DESKTOP",
        )
    })?;
    let out = run_with_timeout(&binary, &command, EXPORT_TIMEOUT)?;
    if !out.status.success() {
        let message = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(AutomationError::new("export_failed", message));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(text.trim()).map_err(|e| {
        AutomationError::new(
            "export_failed",
            format!("Unreadable summary from the desktop ({e}): {text}"),
        )
    })
}

/// The desktop renderer is a separate process; a stale or wrong executable
/// (for example one that opens its window instead of printing) must not hang
/// the caller, so the child is killed after this long.
const EXPORT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

fn run_with_timeout(
    binary: &Path,
    args: &[String],
    limit: std::time::Duration,
) -> Result<std::process::Output> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = std::process::Command::new(binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            AutomationError::new(
                "desktop_missing",
                format!("Cannot run {}: {e}", binary.display()),
            )
        })?;
    // Drain the pipes on threads so a chatty child cannot block on a full pipe.
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let out_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(s) = stdout.as_mut() {
            let _ = s.read_to_end(&mut buf);
        }
        buf
    });
    let err_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(s) = stderr.as_mut() {
            let _ = s.read_to_end(&mut buf);
        }
        buf
    });
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= limit => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AutomationError::new(
                    "export_timeout",
                    format!(
                        "{} did not finish the PDF export within {} s; is it an up-to-date OpenXER desktop build?",
                        binary.display(),
                        limit.as_secs()
                    ),
                ));
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Err(e) => {
                return Err(AutomationError::new(
                    "export_failed",
                    format!("Cannot wait for {}: {e}", binary.display()),
                ))
            }
        }
    };
    Ok(std::process::Output {
        status,
        stdout: out_thread.join().unwrap_or_default(),
        stderr: err_thread.join().unwrap_or_default(),
    })
}
