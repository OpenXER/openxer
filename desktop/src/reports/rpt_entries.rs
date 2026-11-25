//! The saved reports as one list for the Reports view, the group and batch
//! dialogs: wizard reports saved in the preferences and definition-format
//! reports stored in the workspace database, with the commands that act on
//! one of them (move to a group, copy, paste, delete, scope, last run).
use super::rpt::store::{self, ReportScope, Stored};
use super::wizard::Definition;
use crate::App;

/// Which kind of saved report an entry is.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Which {
    /// A wizard report (index in the saved wizard list).
    Wizard(usize),
    /// A definition report (workspace id).
    Stored(i64),
    /// A report built into OpenXER, listed in the Reports view but never
    /// stored: it runs through its own dialog (Tools > Reports).
    Builtin(Builtin),
}

/// The reports built into OpenXER that the Reports view lists beside the
/// saved ones, in the library's group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Builtin {
    Scurve,
    EarnedHours,
}

impl Builtin {
    pub const ALL: [Builtin; 2] = [Builtin::Scurve, Builtin::EarnedHours];
    pub fn name(self) -> &'static str {
        match self {
            Builtin::Scurve => "S-Curve",
            Builtin::EarnedHours => "Earned Hours",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Builtin::Scurve => "scurve",
            Builtin::EarnedHours => "earned_hours",
        }
    }
}

/// Preference holding when each built-in report last ran.
const BUILTIN_RUNS_KEY: &str = "builtin_report_runs_v1";

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub which: Which,
    pub name: String,
    pub group: Option<u32>,
    /// The type column: the wizard's subject area, or where the report
    /// comes from.
    pub kind: String,
    pub scope: ReportScope,
    /// `YYYY-MM-DD HH:MM:SS`, local time.
    pub last_run: Option<String>,
}

impl Entry {
    /// Made by the Report Wizard (else by the Report Editor, or imported).
    pub fn is_wizard(&self) -> bool {
        matches!(self.which, Which::Wizard(_))
    }
    /// A built-in report: it runs, and nothing else (no editing, export of
    /// its definition, delete, copy, scope or group).
    pub fn is_builtin(&self) -> bool {
        matches!(self.which, Which::Builtin(_))
    }
}

/// A copied or cut report, waiting for Paste.
#[derive(Clone, Debug)]
pub enum Clip {
    Wizard(Box<Definition>),
    Stored {
        name: String,
        data: Vec<u8>,
        scope: ReportScope,
    },
    /// Cut: Paste moves this report into the chosen group.
    Move(Which),
}

impl App {
    pub(crate) fn stored_reports(&self) -> Vec<Stored> {
        store::list(self.store.connection()).unwrap_or_default()
    }

    /// Every saved report: wizard reports first, then definition reports.
    pub(crate) fn report_entries(&self) -> Vec<Entry> {
        let mut out: Vec<Entry> = self
            .report_definitions()
            .iter()
            .enumerate()
            .map(|(i, d)| Entry {
                which: Which::Wizard(i),
                name: d.name.clone(),
                group: d.group,
                kind: format!("Wizard: {}", d.subject.caption()),
                scope: d.scope.clone(),
                last_run: d.last_run.clone(),
            })
            .collect();
        out.extend(self.stored_reports().into_iter().map(|s| Entry {
            which: Which::Stored(s.id),
            name: s.name,
            group: s.group,
            kind: if s.kind == store::KIND_LIBRARY {
                "OpenXER".into()
            } else {
                "Definition".into()
            },
            scope: s.scope,
            last_run: s.last_run,
        }));
        out
    }

    /// What the Reports view lists: the saved reports and the built-in ones.
    /// The built-in reports are not stored, so every workspace has them
    /// exactly once; they sit in the library's group while it exists.
    pub(crate) fn report_list(&self) -> Vec<Entry> {
        let group = self
            .report_groups()
            .iter()
            .find(|g| g.name == store::LIBRARY_GROUP && g.parent.is_none())
            .map(|g| g.id);
        let runs = self.builtin_runs();
        let mut out = self.report_entries();
        out.extend(Builtin::ALL.into_iter().map(|b| Entry {
            which: Which::Builtin(b),
            name: b.name().into(),
            group,
            kind: "Built-in".into(),
            scope: ReportScope::Global,
            last_run: runs[b.key()].as_str().map(String::from),
        }));
        out
    }

    fn builtin_runs(&self) -> serde_json::Value {
        self.store
            .preference(BUILTIN_RUNS_KEY)
            .ok()
            .flatten()
            .and_then(|t| serde_json::from_str(&t).ok())
            .filter(serde_json::Value::is_object)
            .unwrap_or_else(|| serde_json::json!({}))
    }

    /// Moves a saved report into a group.
    pub(crate) fn set_entry_group(&mut self, e: &Entry, group: Option<u32>) {
        match e.which {
            Which::Wizard(i) => {
                let mut list = self.report_definitions();
                if let Some(d) = list.get_mut(i) {
                    d.group = group;
                    self.save_report_definitions(&list);
                }
            }
            Which::Stored(id) => {
                if let Err(err) = store::set_group(self.store.connection(), id, group) {
                    self.error(err);
                }
            }
            Which::Builtin(_) => {}
        }
    }

    /// Adds a blank definition report and opens it in the editor.
    pub(crate) fn new_blank_report(&mut self, group: Option<u32>) -> Result<i64, String> {
        let stream = super::rpt::edit::new_report("New Report", true);
        let name = store::free_name(&self.stored_reports(), "New Report");
        let mut stream = stream;
        stream.root.set_string("ReportName", &name);
        stream.root.set_string("Title", &name);
        let id = store::insert(
            self.store.connection(),
            &name,
            store::KIND_USER,
            group,
            None,
            &stream.to_bytes(),
        )?;
        Ok(id)
    }

    pub(crate) fn copy_entry(&mut self, e: &Entry) {
        let clip = match e.which {
            Which::Wizard(i) => self
                .report_definitions()
                .get(i)
                .cloned()
                .map(|d| Clip::Wizard(Box::new(d))),
            Which::Stored(id) => {
                store::load(self.store.connection(), id)
                    .ok()
                    .map(|data| Clip::Stored {
                        name: e.name.clone(),
                        data,
                        scope: e.scope.clone(),
                    })
            }
            Which::Builtin(_) => return,
        };
        self.reports.clipboard = clip;
    }

    /// Pastes the copied report into `group`; returns its new name.
    pub(crate) fn paste_clip(&mut self, group: Option<u32>) -> Result<String, String> {
        match self.reports.clipboard.clone() {
            None => Err("Copy a report first.".into()),
            Some(Clip::Wizard(def)) => {
                let mut list = self.report_definitions();
                let mut d = (*def).clone();
                let mut name = d.name.clone();
                let mut n = 2;
                while list.iter().any(|x| x.name == name) {
                    name = format!("{} ({n})", d.name);
                    n += 1;
                }
                d.name = name.clone();
                d.group = group;
                d.last_run = None;
                list.push(d);
                self.save_report_definitions(&list);
                Ok(name)
            }
            Some(Clip::Move(which)) => {
                let entry = self
                    .report_entries()
                    .into_iter()
                    .find(|e| e.which == which)
                    .ok_or("That report is no longer in the list.")?;
                self.set_entry_group(&entry, group);
                self.reports.clipboard = None;
                Ok(entry.name)
            }
            Some(Clip::Stored { name, data, scope }) => {
                let existing = self.stored_reports();
                let new = store::free_name(&existing, &name);
                let mut data = data;
                // Keep the printed name in step with a renamed copy.
                if let Ok(mut s) = super::rpt::stream::Stream::parse_bytes(&data) {
                    if new != name {
                        s.root.set_string("ReportName", &new);
                        data = s.to_bytes();
                    }
                }
                let id = store::insert(
                    self.store.connection(),
                    &new,
                    store::KIND_USER,
                    group,
                    None,
                    &data,
                )?;
                store::set_scope(self.store.connection(), id, &scope)?;
                Ok(new)
            }
        }
    }

    pub(crate) fn delete_entry(&mut self, e: &Entry) {
        match e.which {
            Which::Wizard(i) => {
                let mut list = self.report_definitions();
                if i < list.len() {
                    list.remove(i);
                    self.save_report_definitions(&list);
                }
            }
            Which::Stored(id) => {
                if let Err(err) = store::delete(self.store.connection(), id) {
                    self.error(err);
                }
            }
            Which::Builtin(_) => {}
        }
    }

    /// The Project ID of the open project (what a project report names).
    pub(crate) fn open_project_id(&self) -> String {
        self.project.model.project.name.clone()
    }

    /// The report can run on the open project: a global report on any, a
    /// project report on its own.
    pub(crate) fn can_run_entry(&self, e: &Entry) -> bool {
        e.scope.runs_on(&self.open_project_id())
    }

    /// Why a report cannot run here, for the status line.
    pub(crate) fn scope_refusal(&self, e: &Entry) -> String {
        match &e.scope {
            ReportScope::Project(p) => format!(
                "{} is a project report for {p}; open that project to run it.",
                e.name
            ),
            ReportScope::Global => String::new(),
        }
    }

    /// Makes a report global, or a project report of the open project.
    pub(crate) fn set_entry_scope(&mut self, e: &Entry, global: bool) {
        let scope = if global {
            ReportScope::Global
        } else {
            ReportScope::Project(self.open_project_id())
        };
        match e.which {
            Which::Wizard(i) => {
                let mut list = self.report_definitions();
                if let Some(d) = list.get_mut(i) {
                    d.scope = scope;
                    self.save_report_definitions(&list);
                }
            }
            Which::Stored(id) => {
                if let Err(err) = store::set_scope(self.store.connection(), id, &scope) {
                    self.error(err);
                }
            }
            Which::Builtin(_) => {}
        }
    }

    /// Records that a report ran now.
    pub(crate) fn mark_run(&mut self, which: &Which) {
        let stamp = chrono::Local::now().naive_local().format(STAMP).to_string();
        match which {
            Which::Wizard(i) => {
                let mut list = self.report_definitions();
                if let Some(d) = list.get_mut(*i) {
                    d.last_run = Some(stamp);
                    self.save_report_definitions(&list);
                }
            }
            Which::Stored(id) => {
                if let Err(err) = store::set_last_run(self.store.connection(), *id, &stamp) {
                    self.error(err);
                }
            }
            Which::Builtin(b) => {
                let mut runs = self.builtin_runs();
                runs[b.key()] = serde_json::Value::String(stamp);
                if let Err(err) = self
                    .store
                    .set_preference(BUILTIN_RUNS_KEY, &runs.to_string())
                {
                    self.error(err);
                }
            }
        }
    }
}

/// The text form of a last-run time.
pub const STAMP: &str = "%Y-%m-%d %H:%M:%S";

/// A last-run time as the Last Run Date column shows it, in the user's date
/// format; empty when the report has not run.
pub fn last_run_text(stamp: Option<&str>) -> String {
    stamp
        .and_then(|s| chrono::NaiveDateTime::parse_from_str(s, STAMP).ok())
        .map_or(String::new(), |at| {
            crate::format::current().dates.format(at)
        })
}
