//! Two open projects with the same internal project number. A relationship
//! names the other project by that number, so links between such projects
//! cannot be told apart (and cannot be made). New projects get a number of
//! their own (`App::free_project_number`); this handles workspaces from
//! before that, where several projects share one.
//!
//! Stored projects are never renumbered just because they were loaded.
//! When a project is about to be opened beside an open project with the
//! same number, the project being opened (the later one) is given a fresh
//! number in the workspace, before it loads, in one transaction with every
//! row that carries the number: its own rows, its project UDF values and
//! baseline fields, and the relationships into it held by projects read
//! from the same file. The project that was already open keeps its number,
//! so nothing held in memory, in undo histories or in recovery copies ever
//! refers to a number that is gone. The status line says what happened.
//!
//! It is left as it is, with a note, when a project read from the same file
//! is open (its copy in memory would write the old number back) or a
//! recovery copy from that file is waiting.
use super::*;

/// What [`App::clear_number_clashes`] knows of a project already in the set.
struct Seen {
    pid: i64,
    source: String,
    name: String,
}

impl App {
    /// Before the closed workspace projects among `ids` open beside the
    /// projects in `staying` (workspace ids of open projects that stay
    /// open): give each one whose number an earlier one uses a fresh number
    /// (see the module comment). `ids` is in opening order. Returns the
    /// notes for the status line.
    pub(crate) fn clear_number_clashes(&mut self, ids: &[i64], staying: &[i64]) -> Vec<String> {
        let open = self.open_ids();
        let mut seen: Vec<Seen> = self
            .open_projects()
            .into_iter()
            .zip(open.iter().copied())
            .filter(|(_, id)| staying.contains(id))
            .map(|(p, _)| Seen {
                pid: p.model.project.proj_id,
                source: p.source_name.clone(),
                name: p.model.project.name.clone(),
            })
            .collect();
        let held: Vec<String> = seen.iter().map(|s| s.source.clone()).collect();
        let titles = self.store.list().unwrap_or_default();
        let title = |id: i64| {
            titles
                .iter()
                .find(|(i, _)| *i == id)
                .map_or_else(String::new, |(_, t)| t.clone())
        };
        let mut notes = vec![];
        for &id in ids {
            if staying.contains(&id) {
                continue;
            }
            let Ok((source, key, _)) = self.store.source(id) else {
                continue;
            };
            let name = title(id);
            let mut pid = key;
            if let Some(other) = seen.iter().find(|s| s.pid == key) {
                let other = other.name.clone();
                if held.contains(&source) {
                    notes.push(format!(
                        "{name} and {other} share internal project number {key}; a project from the same file is open, so it is kept"
                    ));
                } else {
                    let also: Vec<i64> = seen.iter().map(|s| s.pid).collect();
                    match self.store.give_fresh_number(id, &also) {
                        Ok((_, to)) => {
                            pid = to;
                            notes.push(format!(
                                "{name} now has internal project number {to}; {other} uses {key}"
                            ));
                        }
                        Err(e) => notes.push(format!(
                            "{name} and {other} share internal project number {key}; it is kept because {e}"
                        )),
                    }
                }
            }
            seen.push(Seen { pid, source, name });
        }
        notes
    }
}
