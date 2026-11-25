//! User preferences kept in a workspace that an agent may read or set:
//! the Assistance switches of the app's User Preferences (whether Add opens
//! the New Activity wizard). The desktop app reads and writes the same row
//! through [`Assistance`], so both always agree on its key and form.
use crate::storage::{self, invalid, io, parent};
use crate::{AutomationError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// The `preferences` row that holds [`Assistance`].
pub const ASSISTANCE_KEY: &str = "assistance_v1";

/// User Preferences, Assistance: which wizards guide an Add.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Assistance {
    /// Add (menu, toolbar, Insert, row menu) opens the New Activity wizard.
    pub activity_wizard: bool,
    /// The Resources view's Add opens the New Resource wizard.
    pub resource_wizard: bool,
}

impl Default for Assistance {
    /// Both on, as on a first start.
    fn default() -> Self {
        Self {
            activity_wizard: true,
            resource_wizard: true,
        }
    }
}

impl Assistance {
    /// The stored form; anything unreadable counts as the defaults.
    pub fn from_json(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }
    pub fn to_json(self) -> String {
        serde_json::to_string(&self).unwrap_or_default()
    }
    /// The switch a preference name stands for.
    fn switch(&mut self, name: &str) -> Result<&mut bool> {
        match name
            .trim()
            .to_ascii_lowercase()
            .replace([' ', '-'], "_")
            .as_str()
        {
            "new_activity_wizard" | "activity_wizard" => Ok(&mut self.activity_wizard),
            "new_resource_wizard" | "resource_wizard" => Ok(&mut self.resource_wizard),
            _ => Err(AutomationError::new(
                "invalid_arguments",
                format!(
                    "{name:?} is not a preference; use new_activity_wizard or new_resource_wizard"
                ),
            )),
        }
    }
    fn json(self) -> Value {
        json!({
            "new_activity_wizard": self.activity_wizard,
            "new_resource_wizard": self.resource_wizard,
        })
    }
}

fn read(c: &Connection) -> Result<Assistance> {
    let has_table = c
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='preferences'",
            [],
            |_| Ok(()),
        )
        .optional()
        .map_err(invalid)?
        .is_some();
    if !has_table {
        return Ok(Assistance::default());
    }
    let text: Option<String> = c
        .query_row(
            "SELECT value FROM preferences WHERE key=?1",
            [ASSISTANCE_KEY],
            |r| r.get(0),
        )
        .optional()
        .map_err(invalid)?;
    Ok(text.map_or_else(Assistance::default, |t| Assistance::from_json(&t)))
}

/// `workspace preference`: the Assistance switches of a workspace, or one
/// of them set (`name`, `value`) in a copy written to `output` (`dry_run`
/// reports the change and writes nothing).
pub fn preference(
    input: &Path,
    name: Option<&str>,
    value: Option<bool>,
    output: Option<PathBuf>,
    dry_run: bool,
) -> Result<Value> {
    let args = |m: &str| AutomationError::new("invalid_arguments", m.to_string());
    if !storage::is_database(input)? {
        return Err(args(
            "workspace preference needs an OpenXER workspace database (.db)",
        ));
    }
    let Some(value) = value else {
        if output.is_some() || dry_run {
            return Err(args("output and dry_run go with value"));
        }
        let mut now = read(&storage::read_database(input)?)?;
        return Ok(match name {
            Some(name) => json!({"input": input, "name": name, "value": *now.switch(name)?}),
            None => json!({"input": input, "preferences": now.json()}),
        });
    };
    let name = name.ok_or_else(|| args("value needs name"))?;
    match (&output, dry_run) {
        (Some(_), true) => return Err(args("dry_run writes nothing; omit output")),
        (None, false) => return Err(args("output is required unless dry_run is true")),
        (Some(path), false) => {
            storage::check_destination(path)?;
            if !storage::is_database_extension(path) {
                return Err(args(
                    "workspace preference writes an OpenXER workspace (.db/.sqlite/.sqlite3)",
                ));
            }
        }
        (None, true) => {}
    }
    let source = storage::read_database(input)?;
    let before = read(&source)?;
    let mut after = before;
    *after.switch(name)? = value;
    if let Some(path) = &output {
        let temp = tempfile::NamedTempFile::new_in(parent(path)).map_err(io)?;
        source.backup("main", temp.path(), None).map_err(io)?;
        drop(source);
        let c = Connection::open(temp.path()).map_err(io)?;
        c.execute(
            "CREATE TABLE IF NOT EXISTS preferences(key TEXT PRIMARY KEY,value TEXT NOT NULL)",
            [],
        )
        .map_err(io)?;
        c.execute(
            "INSERT INTO preferences(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![ASSISTANCE_KEY, after.to_json()],
        )
        .map_err(io)?;
        c.close().map_err(|(_, e)| io(e))?;
        storage::publish(temp, path)?;
    }
    Ok(json!({
        "input": input,
        "output": output,
        "dry_run": dry_run,
        "name": name,
        "before": *before.clone().switch(name)?,
        "value": value,
        "preferences": after.json(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(dir: &Path) -> PathBuf {
        let path = dir.join("ws.db");
        let c = Connection::open(&path).unwrap();
        c.execute_batch(
            "CREATE TABLE projects(id INTEGER PRIMARY KEY,source_name TEXT NOT NULL,project_key INTEGER NOT NULL,title TEXT NOT NULL,xer_content TEXT NOT NULL,updated_at TEXT DEFAULT CURRENT_TIMESTAMP,source_windows_encoding INTEGER NOT NULL DEFAULT 0,level_resources INTEGER NOT NULL DEFAULT 0,UNIQUE(source_name,project_key));CREATE TABLE preferences(key TEXT PRIMARY KEY,value TEXT NOT NULL);",
        )
        .unwrap();
        path
    }

    #[test]
    fn the_wizard_switch_reads_on_by_default_and_is_set_in_a_copy() {
        let dir = tempfile::tempdir().unwrap();
        let input = workspace(dir.path());
        let now = preference(&input, None, None, None, false).unwrap();
        assert_eq!(now["preferences"]["new_activity_wizard"], true);
        // A dry run changes nothing.
        let dry = preference(&input, Some("new_activity_wizard"), Some(false), None, true).unwrap();
        assert_eq!(dry["before"], true);
        assert_eq!(dry["preferences"]["new_activity_wizard"], false);
        let output = dir.path().join("out.db");
        preference(
            &input,
            Some("new-activity-wizard"),
            Some(false),
            Some(output.clone()),
            false,
        )
        .unwrap();
        let read_back =
            preference(&output, Some("new_activity_wizard"), None, None, false).unwrap();
        assert_eq!(read_back["value"], false);
        let input_now = preference(&input, None, None, None, false).unwrap();
        assert_eq!(
            input_now["preferences"]["new_activity_wizard"], true,
            "input untouched"
        );
        // The row is the one the app reads.
        let c = Connection::open(&output).unwrap();
        let text: String = c
            .query_row(
                "SELECT value FROM preferences WHERE key=?1",
                [ASSISTANCE_KEY],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            Assistance::from_json(&text),
            Assistance {
                activity_wizard: false,
                resource_wizard: true
            }
        );
        // Refusals: an unknown name, value without output or dry run.
        assert!(preference(&input, Some("colors"), Some(true), None, true).is_err());
        assert!(preference(&input, Some("new_activity_wizard"), Some(true), None, false).is_err());
    }
}
