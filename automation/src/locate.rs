//! Where OpenXER keeps its workspace, and `find_schedules`: where the user's
//! schedules are, for an agent that was asked "Use OpenXER to inspect my
//! schedule" and was not given a file. The workspace rule
//! ([`default_workspace_in`]) is the one the desktop opens by
//! (`Store::default_path_in` calls it), so the app and the agent always
//! agree. `find_schedules` names that workspace (and the other copy's, when
//! an installed and a portable copy each have one) with its projects, most
//! recently saved first, and can list the schedule files in one folder. It
//! only reads: workspaces are opened read-only, and the one file it writes
//! is the app's own write check, removed at once.
use crate::{storage, AutomationError, Result};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// The workspace file name.
pub const WORKSPACE_NAME: &str = "openxer.db";
/// Moves the data folder (tests, managed deployments).
pub const DATA_DIR_VAR: &str = "OPENXER_DATA_DIR";
/// The file the installer puts beside the executables of an installed copy.
pub use crate::connect::INSTALLED_MARKER;
/// Most projects listed per workspace, and most files per folder.
const LIMIT: usize = 100;

/// The data folder `OPENXER_DATA_DIR` names (`value`), made absolute; None
/// when it is unset or empty.
pub fn data_dir_override(value: Option<OsString>) -> Option<PathBuf> {
    let dir = PathBuf::from(value.filter(|v| !v.is_empty())?);
    Some(std::path::absolute(&dir).unwrap_or(dir))
}

/// The per-user base folder OpenXER's data folder is made in, for this
/// process: `%LOCALAPPDATA%` on Windows; elsewhere the XDG data folder
/// ([`xdg_data_home`] of `$XDG_DATA_HOME` and `$HOME`). None when it is
/// not known. Callers pass it on as the `user_base` of
/// [`default_workspace_in`], [`candidates`] and [`find_in`].
pub fn user_data_base() -> Option<OsString> {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
    }
    #[cfg(not(windows))]
    {
        xdg_data_home(std::env::var_os("XDG_DATA_HOME"), std::env::var_os("HOME"))
    }
}

/// The XDG data folder, given the values of `$XDG_DATA_HOME` (`xdg`) and
/// `$HOME` (`home`): `xdg` when it is set and absolute (the XDG rule
/// ignores a relative one), else `<home>/.local/share` when `home` is set
/// and absolute; None otherwise. Used on systems other than Windows.
pub fn xdg_data_home(xdg: Option<OsString>, home: Option<OsString>) -> Option<OsString> {
    // Absolute in the Unix sense (it starts with `/`), so the rule reads
    // the same on any system.
    let absolute = |v: Option<OsString>| v.filter(|v| v.as_encoded_bytes().first() == Some(&b'/'));
    absolute(xdg).or_else(|| {
        absolute(home).map(|h| {
            PathBuf::from(h)
                .join(".local")
                .join("share")
                .into_os_string()
        })
    })
}

/// `<user_base>/OpenXER/openxer.db` in the per-user base folder
/// ([`user_data_base`]; `.` when it is unknown).
fn per_user_workspace(user_base: Option<OsString>) -> PathBuf {
    PathBuf::from(
        user_base
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| ".".into()),
    )
    .join("OpenXER")
    .join(WORKSPACE_NAME)
}

/// The workspace a program in `exe_dir` opens, with `data_dir` the value
/// of `OPENXER_DATA_DIR` and `user_base` the per-user base folder
/// ([`user_data_base`]: `%LOCALAPPDATA%` on Windows, the XDG data folder
/// elsewhere):
/// - `OPENXER_DATA_DIR` set: `openxer.db` in that folder;
/// - an installed copy (the installer's marker file beside the EXE):
///   `<user_base>/OpenXER/openxer.db`, never the program folder, which
///   an upgrade or uninstall replaces. A workspace a portable copy left
///   beside the EXE is still used while the per-user one does not exist, so
///   it never drops out of sight;
/// - a portable copy: `openxer.db` beside the EXE when that folder can be
///   written (a test file is made and removed), else
///   `<user_base>/OpenXER/openxer.db`.
pub fn default_workspace_in(
    exe_dir: Option<&Path>,
    data_dir: Option<OsString>,
    user_base: Option<OsString>,
) -> PathBuf {
    if let Some(dir) = data_dir_override(data_dir) {
        return dir.join(WORKSPACE_NAME);
    }
    let adjacent = exe_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    if adjacent.join(INSTALLED_MARKER).is_file() {
        let data = per_user_workspace(user_base);
        let left = adjacent.join(WORKSPACE_NAME);
        if !data.exists() && left.is_file() {
            return left;
        }
        return data;
    }
    let marker = adjacent.join(format!(".openxer-write-check-{}", std::process::id()));
    if std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
        .is_ok()
    {
        let _ = std::fs::remove_file(marker);
        return adjacent.join(WORKSPACE_NAME);
    }
    per_user_workspace(user_base)
}

/// One place a workspace may be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    /// What makes it a candidate, in words.
    pub why: &'static str,
    /// The workspace the copy in `exe_dir` opens.
    pub opened: bool,
}

/// The places a workspace may be for the copy in `exe_dir`: first the one
/// it opens ([`default_workspace_in`]), then the other place (beside the EXE
/// or the per-user data folder), so a workspace the other kind of copy uses
/// is found too. With `OPENXER_DATA_DIR` set, only that one.
pub fn candidates(
    exe_dir: Option<&Path>,
    data_dir: Option<OsString>,
    user_base: Option<OsString>,
) -> Vec<Candidate> {
    let installed = exe_dir.is_some_and(|d| d.join(INSTALLED_MARKER).is_file());
    let why = |path: &Path| {
        if data_dir_override(data_dir.clone()).is_some() {
            "the data folder OPENXER_DATA_DIR names"
        } else if exe_dir.is_some_and(|d| path == d.join(WORKSPACE_NAME)) {
            if installed {
                "left beside this installed copy by an earlier portable copy"
            } else {
                "beside this portable copy"
            }
        } else {
            "the per-user data folder (installed copies, and portable copies in a folder they cannot write)"
        }
    };
    let opened = default_workspace_in(exe_dir, data_dir.clone(), user_base.clone());
    let mut out = vec![Candidate {
        why: why(&opened),
        path: opened,
        opened: true,
    }];
    if data_dir_override(data_dir.clone()).is_some() {
        return out;
    }
    let others = [
        exe_dir.map(|d| d.join(WORKSPACE_NAME)),
        user_base
            .filter(|v| !v.is_empty())
            .map(|l| per_user_workspace(Some(l))),
    ];
    for path in others.into_iter().flatten() {
        if out.iter().all(|c| c.path != path) {
            out.push(Candidate {
                why: why(&path),
                path,
                opened: false,
            });
        }
    }
    out
}

/// A workspace's projects, most recently saved first.
fn projects(c: &Connection) -> Result<(Vec<Value>, usize)> {
    let count: i64 = c
        .query_row("SELECT COUNT(*) FROM projects", [], |r| r.get(0))
        .map_err(storage::invalid)?;
    let mut stmt = c
        .prepare(
            "SELECT id,project_key,title,source_name,COALESCE(updated_at,'') FROM projects ORDER BY updated_at DESC,id DESC LIMIT ?1",
        )
        .map_err(storage::invalid)?;
    let rows = stmt
        .query_map([LIMIT as i64], |r| {
            Ok(json!({
                "workspace_id": r.get::<_, i64>(0)?,
                "project_id": r.get::<_, i64>(1)?,
                "name": r.get::<_, String>(2)?,
                "source_name": r.get::<_, String>(3)?,
                "saved_utc": r.get::<_, String>(4)?,
            }))
        })
        .map_err(storage::invalid)?;
    let rows = rows
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage::invalid)?;
    Ok((rows, count.max(0) as usize))
}

/// `YYYY-MM-DD HH:MM` (UTC) of a file's last change.
fn modified(meta: &std::fs::Metadata) -> String {
    let stamp = meta
        .modified()
        .map(crate::connect::timestamp)
        .unwrap_or_default();
    match stamp.len() {
        15 => format!(
            "{}-{}-{} {}:{}",
            &stamp[0..4],
            &stamp[4..6],
            &stamp[6..8],
            &stamp[9..11],
            &stamp[11..13]
        ),
        _ => stamp,
    }
}

/// The XER files and workspaces directly in `folder`, newest first.
fn folder_files(folder: &Path) -> Result<Value> {
    if !folder.is_absolute() {
        return Err(AutomationError::new(
            "invalid_arguments",
            "folder must be an absolute path, such as C:\\Users\\you\\Documents",
        ));
    }
    let entries = std::fs::read_dir(folder).map_err(|e| {
        AutomationError::new(
            "not_found",
            format!("Cannot list {}: {e}", folder.display()),
        )
    })?;
    let mut files: Vec<(std::time::SystemTime, Value)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            let meta = e.metadata().ok().filter(|m| m.is_file())?;
            let kind = if path
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("xer"))
            {
                "xer"
            } else if storage::is_database_extension(&path)
                && storage::is_database(&path).unwrap_or(false)
            {
                "workspace"
            } else {
                return None;
            };
            let at = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
            Some((
                at,
                json!({"path": path, "kind": kind, "bytes": meta.len(), "modified_utc": modified(&meta)}),
            ))
        })
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    let total = files.len();
    Ok(json!({
        "path": folder,
        "files": files.into_iter().take(LIMIT).map(|(_, v)| v).collect::<Vec<_>>(),
        "file_count": total,
    }))
}

/// The answer for the copy in `exe_dir`, with the environment given.
pub fn find_in(
    exe_dir: Option<&Path>,
    data_dir: Option<OsString>,
    user_base: Option<OsString>,
    folder: Option<&Path>,
) -> Result<Value> {
    let mut workspaces = vec![];
    for c in candidates(exe_dir, data_dir, user_base) {
        if !c.path.is_file() {
            continue;
        }
        let mut entry = json!({
            "path": c.path,
            "opened_by_openxer": c.opened,
            "why": c.why,
        });
        match storage::read_database(&c.path).and_then(|db| projects(&db)) {
            Ok((list, count)) => {
                entry["project_count"] = json!(count);
                entry["projects"] = json!(list);
            }
            Err(e) => entry["error"] = json!(e.message),
        }
        workspaces.push(entry);
    }
    let folder = folder.map(folder_files).transpose()?;
    let next = if workspaces.is_empty() && folder.is_none() {
        "No OpenXER workspace was found on this PC. Ask the user for their XER file's path, or call again with folder set to the folder it is in."
    } else {
        "Pass a workspace path as input with a workspace_id from its projects (or an XER file path as input) to openxer_load, openxer_inspect, openxer_eps_list and the other tools. When several projects are listed and the user did not name one, ask which, or start with the most recently saved."
    };
    let mut out = json!({ "workspaces": workspaces, "next": next });
    if let Some(folder) = folder {
        out["folder"] = folder;
    }
    Ok(out)
}

/// The folder of this process's executable.
fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
}

/// The workspace this process opens by default ([`default_workspace_in`]
/// for this executable's folder and the process environment).
pub fn default_workspace() -> PathBuf {
    default_workspace_in(
        exe_dir().as_deref(),
        std::env::var_os(DATA_DIR_VAR),
        std::env::var_os("LOCALAPPDATA"),
    )
}

/// This process's data folder: the folder that holds its default
/// workspace (and the user's `skills`).
pub fn data_folder() -> PathBuf {
    default_workspace()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The answer for this process: the copy is the one this executable is in.
pub fn find(folder: Option<&Path>) -> Result<Value> {
    find_in(
        exe_dir().as_deref(),
        std::env::var_os(DATA_DIR_VAR),
        user_data_base(),
        folder,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn os(p: &Path) -> Option<OsString> {
        Some(p.as_os_str().to_owned())
    }

    #[test]
    fn the_xdg_data_folder_is_xdg_data_home_when_absolute_else_under_home() {
        let s = |v: &str| Some(OsString::from(v));
        let base = |xdg, home| xdg_data_home(xdg, home).map(PathBuf::from);
        let share = PathBuf::from("/home/ann").join(".local").join("share");
        // An absolute $XDG_DATA_HOME wins.
        assert_eq!(
            base(s("/srv/ann-data"), s("/home/ann")),
            Some(PathBuf::from("/srv/ann-data"))
        );
        // Unset, empty or relative: $HOME/.local/share.
        assert_eq!(base(None, s("/home/ann")), Some(share.clone()));
        assert_eq!(base(s(""), s("/home/ann")), Some(share.clone()));
        assert_eq!(base(s("data"), s("/home/ann")), Some(share.clone()));
        // No usable home either: unknown (the caller falls back to `.`).
        assert_eq!(base(None, None), None);
        assert_eq!(base(None, s("")), None);
        assert_eq!(base(s("rel"), s("rel-home")), None);
        // The workspace is <base>/OpenXER/openxer.db.
        assert_eq!(
            per_user_workspace(xdg_data_home(None, s("/home/ann"))),
            share.join("OpenXER").join(WORKSPACE_NAME)
        );
    }

    /// This process's base is `%LOCALAPPDATA%` on Windows and the XDG rule
    /// elsewhere.
    #[test]
    fn the_per_user_base_follows_the_system() {
        #[cfg(windows)]
        assert_eq!(user_data_base(), std::env::var_os("LOCALAPPDATA"));
        #[cfg(not(windows))]
        assert_eq!(
            user_data_base(),
            xdg_data_home(std::env::var_os("XDG_DATA_HOME"), std::env::var_os("HOME"))
        );
    }

    #[test]
    fn candidates_follow_the_desktop_rule() {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("Programs").join("OpenXER");
        let local = dir.path().join("Local");
        fs::create_dir_all(&program).unwrap();
        let per_user = local.join("OpenXER").join(WORKSPACE_NAME);
        let beside = program.join(WORKSPACE_NAME);

        // Portable, nothing yet: beside the EXE first, the per-user one too.
        let c = candidates(Some(&program), None, os(&local));
        assert_eq!(c[0].path, beside);
        assert!(c[0].opened && !c[1].opened);
        assert_eq!(c[1].path, per_user);
        // Portable in a folder it can write: beside the EXE, even when a
        // per-user workspace exists (that one is listed second).
        fs::create_dir_all(per_user.parent().unwrap()).unwrap();
        fs::write(&per_user, b"").unwrap();
        let c = candidates(Some(&program), None, os(&local));
        assert_eq!((c[0].path.clone(), c[0].opened), (beside.clone(), true));
        assert_eq!((c[1].path.clone(), c[1].opened), (per_user.clone(), false));
        // The first entry is always the desktop's own answer, and the write
        // check leaves nothing behind.
        assert_eq!(
            c[0].path,
            default_workspace_in(Some(&program), None, os(&local))
        );
        assert!(fs::read_dir(&program).unwrap().next().is_none());
        // Installed: the per-user one, even with one left beside the EXE.
        fs::write(program.join(crate::connect::INSTALLED_MARKER), b"").unwrap();
        fs::write(&beside, b"").unwrap();
        let c = candidates(Some(&program), None, os(&local));
        assert_eq!((c[0].path.clone(), c[0].opened), (per_user.clone(), true));
        assert_eq!(c[1].path, beside);
        // Installed and the per-user one does not exist yet: the left one.
        fs::remove_file(&per_user).unwrap();
        let c = candidates(Some(&program), None, os(&local));
        assert_eq!(c[0].path, beside);
        // The override wins; an empty one is no override.
        let moved = dir.path().join("Data");
        let c = candidates(Some(&program), os(&moved), os(&local));
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].path, moved.join(WORKSPACE_NAME));
        let c = candidates(Some(&program), Some(OsString::new()), os(&local));
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn find_lists_workspace_projects_newest_first_and_folder_files() {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("app");
        let local = dir.path().join("Local");
        fs::create_dir_all(&program).unwrap();
        // Nothing yet: the agent is told to ask.
        let out = find_in(Some(&program), None, os(&local), None).unwrap();
        assert_eq!(out["workspaces"], json!([]));
        assert!(out["next"].as_str().unwrap().contains("Ask the user"));

        let db = program.join(WORKSPACE_NAME);
        let c = Connection::open(&db).unwrap();
        c.execute_batch(
            "CREATE TABLE projects(id INTEGER PRIMARY KEY,source_name TEXT NOT NULL,project_key INTEGER NOT NULL,title TEXT NOT NULL,xer_content TEXT NOT NULL,updated_at TEXT DEFAULT CURRENT_TIMESTAMP,UNIQUE(source_name,project_key));
             INSERT INTO projects VALUES(1,'C:\\jobs\\Old job.xer',10,'Old job','', '2026-01-02 08:00:00');
             INSERT INTO projects VALUES(2,'C:\\jobs\\Plant Ünit 2.xer',20,'Plant Unit 2','', '2026-11-09 17:30:00');",
        )
        .unwrap();
        drop(c);
        let before = fs::read(&db).unwrap();
        let out = find_in(Some(&program), None, os(&local), None).unwrap();
        let ws = &out["workspaces"][0];
        assert_eq!(ws["path"], json!(db));
        assert_eq!(ws["opened_by_openxer"], true);
        assert_eq!(ws["project_count"], 2);
        assert_eq!(ws["projects"][0]["name"], "Plant Unit 2", "newest first");
        assert_eq!(ws["projects"][0]["workspace_id"], 2);
        assert_eq!(
            ws["projects"][0]["source_name"],
            "C:\\jobs\\Plant Ünit 2.xer"
        );
        assert_eq!(ws["projects"][1]["project_id"], 10);
        assert!(out["next"].as_str().unwrap().contains("workspace_id"));
        assert_eq!(fs::read(&db).unwrap(), before, "read-only");
        let names: Vec<_> = fs::read_dir(&program)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(
            names,
            vec![OsString::from(WORKSPACE_NAME)],
            "nothing written"
        );

        // A folder: XER files and workspaces only, not subfolders.
        let folder = dir.path().join("Schedules");
        fs::create_dir_all(folder.join("sub")).unwrap();
        fs::write(folder.join("Week 14.XER"), b"ERMHDR").unwrap();
        fs::write(folder.join("notes.txt"), b"").unwrap();
        fs::write(folder.join("fake.db"), b"not sqlite").unwrap();
        fs::write(folder.join("sub").join("deep.xer"), b"").unwrap();
        fs::copy(&db, folder.join("copy.db")).unwrap();
        let out = find_in(Some(&program), None, os(&local), Some(&folder)).unwrap();
        let mut found: Vec<(String, String)> = out["folder"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                (
                    Path::new(f["path"].as_str().unwrap())
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .to_string(),
                    f["kind"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        found.sort();
        assert_eq!(
            found,
            vec![
                ("Week 14.XER".to_string(), "xer".to_string()),
                ("copy.db".to_string(), "workspace".to_string())
            ]
        );
        assert_eq!(
            find_in(
                Some(&program),
                None,
                os(&local),
                Some(Path::new("relative"))
            )
            .unwrap_err()
            .code,
            "invalid_arguments"
        );
    }
}
