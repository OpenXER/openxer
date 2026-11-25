//! Crash protection: timed recovery copies of unsaved changes.
//!
//! While any open project has unsaved changes, a copy of each changed
//! project is written every few minutes (User Preferences, Application,
//! default 5, 0 is off) to its own table, `recovery_copies`, in the
//! workspace database. The saved projects (`projects`) are never touched.
//! The copies of this run are deleted when every open project is saved,
//! and so on a clean exit, which saves.
//!
//! The UI thread only clones the changed projects; a worker thread with its
//! own connection turns them into XER text and writes them. A save bumps a
//! generation counter before it deletes the copies, and the worker writes
//! inside an immediate transaction only while the generation it was given
//! is still current, so a copy taken before a save never lands after it.
//!
//! On the next start, copies left by an earlier run are offered in a
//! message box: Recover puts them in place of the saved projects as
//! unsaved changes, Discard deletes them, Escape leaves them for next time.
//!
//! A copy is only ever offered while it is newer than the saved project it
//! belongs to. Each copy keeps `base`, a fingerprint of the project's saved
//! text read in the same transaction that writes the copy. When the saved
//! text has changed since (a save in this app, a later run, the CLI or an
//! import), the copy is older than the saved work: it is not offered, and
//! it is deleted at the next start and after every save. Clock stamps are
//! not compared: the workspace's `updated_at` moves on every write, even
//! one that changes nothing (each start saves the last project again), so
//! it cannot tell newer saved work from the same work saved again.
use super::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};

/// The table of recovery copies.
const TABLE: &str = "recovery_copies";
/// A copy's `base` when the workspace had no saved copy of its project.
const NOT_SAVED: &str = "-";
/// Workspace preference: minutes between recovery copies, 0 for none.
pub const MINUTES_KEY: &str = "autosave_minutes";
pub const DEFAULT_MINUTES: u32 = 5;
pub const MAX_MINUTES: u32 = 120;

/// One project's recovery copy as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct RecoveryCopy {
    pub session: String,
    pub source_name: String,
    pub project_key: i64,
    pub title: String,
    pub text: String,
    pub windows_encoding: bool,
    pub level_resources: bool,
    pub saved_at: String,
    /// Fingerprint of the saved project's text when the copy was written
    /// ([`NOT_SAVED`] when there was none). `None` for a copy written by a
    /// build that did not keep it: such a copy is offered, as before.
    pub base: Option<String>,
}

fn ensure(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS recovery_copies(session TEXT NOT NULL, \
             source_name TEXT NOT NULL, project_key INTEGER NOT NULL, title TEXT NOT NULL, \
             xer_content TEXT NOT NULL, windows_encoding INTEGER NOT NULL, \
             level_resources INTEGER NOT NULL, saved_at TEXT NOT NULL, base TEXT, \
             PRIMARY KEY(session, source_name, project_key));",
        )
        .map_err(|e| e.to_string())?;
    let has_base = connection
        .query_row(
            "SELECT 1 FROM pragma_table_info('recovery_copies') WHERE name='base'",
            [],
            |_| Ok(()),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .is_some();
    if !has_base {
        connection
            .execute_batch("ALTER TABLE recovery_copies ADD COLUMN base TEXT")
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn has(connection: &Connection, table: &str) -> bool {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |_| Ok(()),
        )
        .optional()
        .ok()
        .flatten()
        .is_some()
}

fn has_table(connection: &Connection) -> bool {
    has(connection, TABLE)
}

/// FNV-1a 64 of `text`: the same in every build, unlike std's hasher.
fn fingerprint(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// The fingerprint of the saved project `source`/`key`, [`NOT_SAVED`] when
/// the workspace has none.
fn saved_base(connection: &Connection, source: &str, key: i64) -> Result<String, String> {
    if !has(connection, "projects") {
        return Ok(NOT_SAVED.into());
    }
    let text: Option<String> = connection
        .query_row(
            "SELECT xer_content FROM projects WHERE source_name=?1 AND project_key=?2",
            params![source, key],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(text.map_or_else(|| NOT_SAVED.to_string(), |t| fingerprint(&t)))
}

/// A copy with base `base` is older than its saved project, whose
/// fingerprint is now `now`: the project was saved with other content
/// after the copy was written. A project no longer in the workspace has no
/// newer saved work for the copy to overwrite, so its copy is not stale.
fn stale(base: Option<&str>, now: &str) -> bool {
    match base {
        None => false,
        Some(_) if now == NOT_SAVED => false,
        Some(b) => b != now,
    }
}

/// Fingerprints of saved projects, each read once.
struct Bases<'a> {
    connection: &'a Connection,
    seen: HashMap<(String, i64), String>,
}

impl<'a> Bases<'a> {
    fn new(connection: &'a Connection) -> Self {
        Self {
            connection,
            seen: HashMap::new(),
        }
    }
    fn stale(&mut self, source: &str, key: i64, base: Option<&str>) -> Result<bool, String> {
        if base.is_none() {
            return Ok(false);
        }
        let k = (source.to_string(), key);
        if !self.seen.contains_key(&k) {
            let now = saved_base(self.connection, source, key)?;
            self.seen.insert(k.clone(), now);
        }
        Ok(stale(base, &self.seen[&k]))
    }
}

/// Delete every copy, of any run, that is older than its saved project.
fn remove_stale(connection: &Connection) -> Result<usize, String> {
    if !has_table(connection) {
        return Ok(0);
    }
    ensure(connection)?;
    let rows: Vec<(String, String, i64, Option<String>)> = {
        let mut stmt = connection
            .prepare("SELECT session,source_name,project_key,base FROM recovery_copies")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    let mut bases = Bases::new(connection);
    let mut removed = 0;
    for (session, source, key, base) in rows {
        if bases.stale(&source, key, base.as_deref())? {
            removed += connection
                .execute(
                    "DELETE FROM recovery_copies WHERE session=?1 AND source_name=?2 AND project_key=?3",
                    params![session, source, key],
                )
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(removed)
}

/// Replace `session`'s copies with `copies`, unless a save made them stale
/// (`generation` is no longer `current`).
fn write(
    connection: &Connection,
    session: &str,
    copies: &[RecoveryCopy],
    generation: u64,
    current: &AtomicU64,
) -> Result<bool, String> {
    ensure(connection)?;
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    let written = write_in(connection, session, copies, generation, current);
    let end = if matches!(written, Ok(true)) {
        "COMMIT"
    } else {
        "ROLLBACK"
    };
    match connection.execute_batch(end) {
        Ok(()) => written,
        Err(e) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(e.to_string())
        }
    }
}

/// [`write`] inside its immediate transaction.
fn write_in(
    tx: &Connection,
    session: &str,
    copies: &[RecoveryCopy],
    generation: u64,
    current: &AtomicU64,
) -> Result<bool, String> {
    if current.load(Ordering::SeqCst) != generation {
        return Ok(false);
    }
    tx.execute("DELETE FROM recovery_copies WHERE session=?1", [session])
        .map_err(|e| e.to_string())?;
    for c in copies {
        // Read in this transaction, so no save lands between the read and
        // the copy.
        let base = saved_base(tx, &c.source_name, c.project_key)?;
        tx.execute(
            "INSERT INTO recovery_copies(session,source_name,project_key,title,xer_content,\
             windows_encoding,level_resources,saved_at,base) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                session,
                c.source_name,
                c.project_key,
                c.title,
                c.text,
                c.windows_encoding,
                c.level_resources,
                c.saved_at,
                base
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(true)
}

/// Delete the copies of `session`.
fn clear(connection: &Connection, session: &str) -> Result<(), String> {
    if !has_table(connection) {
        return Ok(());
    }
    connection
        .execute("DELETE FROM recovery_copies WHERE session=?1", [session])
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The newest copy of each project left by other runs, oldest project
/// first. A copy older than its saved project is never among them.
pub fn left_behind(connection: &Connection, session: &str) -> Result<Vec<RecoveryCopy>, String> {
    if !has_table(connection) {
        return Ok(vec![]);
    }
    ensure(connection)?;
    let mut stmt = connection
        .prepare(
            "SELECT session,source_name,project_key,title,xer_content,windows_encoding,\
             level_resources,saved_at,base FROM recovery_copies WHERE session<>?1 \
             ORDER BY saved_at DESC, rowid DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([session], |r| {
            Ok(RecoveryCopy {
                session: r.get(0)?,
                source_name: r.get(1)?,
                project_key: r.get(2)?,
                title: r.get(3)?,
                text: r.get(4)?,
                windows_encoding: r.get(5)?,
                level_resources: r.get(6)?,
                saved_at: r.get(7)?,
                base: r.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut bases = Bases::new(connection);
    let mut newest: Vec<RecoveryCopy> = vec![];
    for c in rows {
        if bases.stale(&c.source_name, c.project_key, c.base.as_deref())? {
            continue;
        }
        if !newest
            .iter()
            .any(|n| n.source_name == c.source_name && n.project_key == c.project_key)
        {
            newest.push(c);
        }
    }
    newest.reverse();
    Ok(newest)
}

/// Make the copies `kept` this run's own and delete every other copy of an
/// earlier run.
fn adopt(connection: &Connection, session: &str, kept: &[RecoveryCopy]) -> Result<(), String> {
    let tx = connection
        .unchecked_transaction()
        .map_err(|e| e.to_string())?;
    for c in kept {
        tx.execute(
            "DELETE FROM recovery_copies WHERE session=?1 AND source_name=?2 AND project_key=?3",
            params![session, c.source_name, c.project_key],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE recovery_copies SET session=?1 WHERE session=?2 AND source_name=?3 AND project_key=?4",
            params![session, c.session, c.source_name, c.project_key],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.execute("DELETE FROM recovery_copies WHERE session<>?1", [session])
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

/// A changed project handed to the worker.
struct Snapshot {
    project: ProjectData,
    saved_at: String,
}

enum Job {
    Write {
        session: String,
        generation: u64,
        snapshots: Vec<Snapshot>,
    },
    #[cfg(test)]
    Flush(mpsc::Sender<()>),
}

/// The thread that writes recovery copies.
struct Worker {
    jobs: mpsc::Sender<Job>,
}

impl Worker {
    fn start(path: PathBuf, generation: Arc<AtomicU64>) -> Option<Self> {
        let (jobs, inbox) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("autosave".into())
            .spawn(move || {
                let Ok(connection) = Connection::open(&path) else {
                    return;
                };
                for job in inbox {
                    match job {
                        Job::Write {
                            session,
                            generation: at,
                            snapshots,
                        } => {
                            let copies: Vec<RecoveryCopy> = snapshots
                                .into_iter()
                                .map(|s| RecoveryCopy {
                                    session: session.clone(),
                                    source_name: s.project.source_name.clone(),
                                    project_key: s.project.model.project.proj_id,
                                    title: s.project.model.project.name.clone(),
                                    text: s.project.updated_document().text(),
                                    windows_encoding: s.project.document.windows_encoding,
                                    level_resources: s.project.model.options.do_level,
                                    saved_at: s.saved_at,
                                    base: None,
                                })
                                .collect();
                            let _ = write(&connection, &session, &copies, at, &generation);
                        }
                        #[cfg(test)]
                        Job::Flush(done) => {
                            let _ = done.send(());
                        }
                    }
                }
            })
            .ok()
            .map(|_| Self { jobs })
    }
}

/// Recovery state of the running app.
pub struct Recovery {
    /// This run's key in `recovery_copies`.
    session: String,
    /// Minutes between copies; 0 is off. `None` until read.
    minutes: Option<u32>,
    /// When the next copy is due (egui time, seconds).
    due: Option<f64>,
    /// Copies of this run may exist (one was sent since the last clear).
    written: bool,
    /// Bumped by every clear; a write taken before it is dropped.
    generation: Arc<AtomicU64>,
    worker: Option<Worker>,
    /// Earlier runs were looked for.
    checked: bool,
    /// Copies left by an earlier run, offered until the user answers.
    pub offer: Option<Vec<RecoveryCopy>>,
}

impl Default for Recovery {
    fn default() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        Self {
            session: format!("{}-{nanos}", std::process::id()),
            minutes: None,
            due: None,
            written: false,
            generation: Default::default(),
            worker: None,
            checked: false,
            offer: None,
        }
    }
}

/// The message box's id.
const OFFER_ID: &str = "recovery";

/// When a copy was written (`saved_at`, local time), in the user's date
/// and time format, as printed pages show the print date and time.
fn shown_stamp(saved_at: &str) -> String {
    match chrono::NaiveDateTime::parse_from_str(saved_at, "%Y-%m-%d %H:%M:%S") {
        Ok(at) => format!(
            "{} {}",
            crate::format::day(at.date()),
            crate::format::time_of_day(at.time())
        ),
        Err(_) => saved_at.to_string(),
    }
}

impl App {
    /// Minutes between recovery copies (0: off), from the workspace.
    pub(crate) fn autosave_minutes(&mut self) -> u32 {
        if let Some(m) = self.open_set.recovery.minutes {
            return m;
        }
        let m = self
            .store
            .preference(MINUTES_KEY)
            .ok()
            .flatten()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .map(|m| m.min(MAX_MINUTES))
            .unwrap_or(DEFAULT_MINUTES);
        self.open_set.recovery.minutes = Some(m);
        crate::crash::note_autosave(m);
        m
    }

    /// Keep `minutes` (0: off) as the interval between recovery copies.
    pub(crate) fn set_autosave_minutes(&mut self, minutes: u32) {
        let minutes = minutes.min(MAX_MINUTES);
        self.open_set.recovery.minutes = Some(minutes);
        self.open_set.recovery.due = None;
        crate::crash::note_autosave(minutes);
        if let Err(e) = self.store.set_preference(MINUTES_KEY, &minutes.to_string()) {
            self.error(format!("Cannot save the recovery copy setting: {e}"));
        }
    }

    /// Every frame: offer copies an earlier run left, write a copy when one
    /// is due, and delete this run's copies once everything is saved.
    pub(crate) fn recovery_frame(&mut self, ctx: &egui::Context) {
        if !self.open_set.recovery.checked {
            self.open_set.recovery.checked = true;
            // Copies older than their saved projects go first; the rest
            // are offered.
            let _ = remove_stale(self.store.connection());
            let left = left_behind(self.store.connection(), &self.open_set.recovery.session)
                .unwrap_or_default();
            if !left.is_empty() {
                self.open_set.recovery.offer = Some(left);
            }
        }
        self.recovery_offer_dialog(ctx);
        if self.open_set.recovery.offer.is_some() {
            // Copies of this run wait until the user has answered.
            return;
        }
        if !self.any_dirty() {
            self.open_set.recovery.due = None;
            self.clear_recovery_copies();
            return;
        }
        let minutes = self.autosave_minutes();
        if minutes == 0 {
            return;
        }
        let now = ctx.input(|i| i.time);
        let interval = minutes as f64 * 60.;
        let due = *self.open_set.recovery.due.get_or_insert(now + interval);
        if now >= due {
            self.write_recovery_copies();
            self.open_set.recovery.due = Some(now + interval);
        } else {
            // An idle window still gets its frame when the copy is due.
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(due - now));
        }
    }

    /// Hand a copy of every changed open project to the worker.
    pub(crate) fn write_recovery_copies(&mut self) {
        let saved_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let mut snapshots = vec![];
        if self.dirty {
            snapshots.push(Snapshot {
                project: self.project.clone(),
                saved_at: saved_at.clone(),
            });
        }
        for p in self.open_set.parked.iter().filter(|p| p.dirty) {
            snapshots.push(Snapshot {
                project: p.project.clone(),
                saved_at: saved_at.clone(),
            });
        }
        if snapshots.is_empty() {
            return;
        }
        let r = &mut self.open_set.recovery;
        if r.worker.is_none() {
            r.worker = Worker::start(self.store.path.clone(), r.generation.clone());
        }
        let job = Job::Write {
            session: r.session.clone(),
            generation: r.generation.load(Ordering::SeqCst),
            snapshots,
        };
        if let Some(worker) = &r.worker {
            if worker.jobs.send(job).is_ok() {
                r.written = true;
            } else {
                r.worker = None;
            }
        }
    }

    /// Write a copy of every changed open project now, on this thread (a
    /// save failed and the app may be closed without one). Returns when the
    /// copies were kept, or why they could not be written. A copy the worker
    /// was given earlier is older and is dropped.
    pub(crate) fn write_recovery_copies_now(&mut self) -> Result<String, String> {
        let saved_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let session = self.open_set.recovery.session.clone();
        let copy = |p: &ProjectData| RecoveryCopy {
            session: session.clone(),
            source_name: p.source_name.clone(),
            project_key: p.model.project.proj_id,
            title: p.model.project.name.clone(),
            text: p.updated_document().text(),
            windows_encoding: p.document.windows_encoding,
            level_resources: p.model.options.do_level,
            saved_at: saved_at.clone(),
            base: None,
        };
        let mut copies = vec![];
        if self.dirty {
            copies.push(copy(&self.project));
        }
        copies.extend(
            self.open_set
                .parked
                .iter()
                .filter(|p| p.dirty)
                .map(|p| copy(&p.project)),
        );
        if copies.is_empty() {
            return Err("there are no unsaved changes".into());
        }
        let r = &mut self.open_set.recovery;
        let generation = r.generation.fetch_add(1, Ordering::SeqCst) + 1;
        match write(
            self.store.connection(),
            &session,
            &copies,
            generation,
            &r.generation,
        ) {
            Ok(_) => {
                r.written = true;
                r.due = None;
                Ok(saved_at)
            }
            Err(e) => Err(e),
        }
    }

    /// Delete this run's copies (everything is saved). A copy still on its
    /// way to the database is dropped by the generation check.
    pub(crate) fn clear_recovery_copies(&mut self) {
        let r = &mut self.open_set.recovery;
        if !r.written {
            return;
        }
        r.generation.fetch_add(1, Ordering::SeqCst);
        if clear(self.store.connection(), &r.session).is_ok() {
            r.written = false;
        }
    }

    /// After a save, by any path: a copy taken before it is dropped on its
    /// way to the database (the generation moves on), and every copy now
    /// older than its saved project is deleted, so a crash before the next
    /// timed copy never leaves one to be offered.
    pub(crate) fn drop_stale_recovery_copies(&mut self) {
        self.open_set
            .recovery
            .generation
            .fetch_add(1, Ordering::SeqCst);
        if let Err(e) = remove_stale(self.store.connection()) {
            self.status = format!("Older recovery copies could not be removed: {e}");
        }
    }

    /// Wait until the worker has written what it was given.
    #[cfg(test)]
    pub(crate) fn flush_recovery(&mut self) {
        if let Some(worker) = &self.open_set.recovery.worker {
            let (done, wait) = mpsc::channel();
            if worker.jobs.send(Job::Flush(done)).is_ok() {
                let _ = wait.recv();
            }
        }
    }

    /// The question asked at startup when an earlier run left copies.
    fn recovery_offer_dialog(&mut self, ctx: &egui::Context) {
        let Some(copies) = self.open_set.recovery.offer.clone() else {
            return;
        };
        let lines: Vec<String> = copies
            .iter()
            .map(|c| format!("{}: changes kept at {}", c.title, shown_stamp(&c.saved_at)))
            .collect();
        let text = if copies.len() == 1 {
            "OpenXER closed before these changes were saved. Recover them, or discard \
             them and keep the saved project?"
        } else {
            "OpenXER closed before these changes were saved. Recover them, or discard \
             them and keep the saved projects?"
        };
        let mut rects = vec![];
        let clicked = crate::kit_window::message_box_with_lines(
            ctx,
            OFFER_ID,
            "Recover Unsaved Changes",
            crate::kit_window::MessageIcon::Question,
            text,
            &lines,
            &["Recover", "Discard"],
            crate::kit_window::Question::RecoverChanges.default_in(&["Recover", "Discard"]),
            &mut rects,
        );
        self.record_dialog_actions(rects);
        let escape = crate::kit_window::escape_goes_to(
            ctx,
            egui::Id::new(("kit_message", OFFER_ID)),
            egui::Order::Foreground,
        );
        match clicked {
            Some(0) => self.recover_copies(copies),
            Some(_) => self.discard_copies(),
            None if escape => {
                // Asked again at the next start.
                self.open_set.recovery.offer = None;
                self.status = "Unsaved changes from the last session are kept for later".into();
            }
            None => {}
        }
    }

    /// Discard: delete every copy of an earlier run.
    pub(crate) fn discard_copies(&mut self) {
        self.open_set.recovery.offer = None;
        let session = self.open_set.recovery.session.clone();
        let result = self
            .store
            .connection()
            .execute("DELETE FROM recovery_copies WHERE session<>?1", [&session]);
        match result {
            Ok(_) => {
                self.status = "Recovered changes discarded; the saved projects are open".into()
            }
            Err(e) => self.error(format!("Cannot discard the recovery copies: {e}")),
        }
    }

    /// Recover: each copy replaces its project as unsaved changes. A
    /// project that is not open is opened beside the others.
    pub(crate) fn recover_copies(&mut self, copies: Vec<RecoveryCopy>) {
        self.open_set.recovery.offer = None;
        let mut recovered = vec![];
        let mut failed = vec![];
        for c in &copies {
            match self.recover_one(c) {
                Ok(()) => recovered.push(c.title.clone()),
                Err(e) => failed.push(format!("{}: {e}", c.title)),
            }
        }
        // The copies stay, now as this run's, until the projects are saved.
        let session = self.open_set.recovery.session.clone();
        let kept = adopt(self.store.connection(), &session, &copies);
        self.open_set.recovery.written = true;
        self.open_set.version += 1;
        self.refresh_composite();
        self.drop_frame_cache();
        if !failed.is_empty() {
            self.error(format!(
                "Some changes could not be recovered:\n{}",
                failed.join("\n")
            ));
        } else if let Err(e) = kept {
            self.error(format!("Cannot keep the recovery copies: {e}"));
        }
        if !recovered.is_empty() {
            self.status = format!(
                "Recovered unsaved changes to {}. Save to keep them.",
                recovered.join(", ")
            );
        }
    }

    fn recover_one(&mut self, c: &RecoveryCopy) -> Result<(), String> {
        let mut document = openxer_xer::Document::parse(&c.text)?;
        document.windows_encoding = c.windows_encoding;
        let mut project =
            ProjectData::from_document(document, c.project_key, c.source_name.clone())?;
        project.model.options.do_level = c.level_resources;
        // On a shared-dictionary workspace the copy holds the dictionary
        // rows as they were when it was taken. Every dictionary edit was
        // synced to the workspace when it was made, so the rows there now
        // are as new or newer; the copy takes them, keeping its own rows, as
        // a reopened project does. Otherwise its next sync or save would
        // write the older rows back for every project.
        let row = self.store.id_of(&c.source_name, c.project_key);
        if row.is_some() && self.store.shared()? {
            self.store.rehydrate(&mut project, row)?;
        }
        self.forget_synced(&project);
        let same = |p: &ProjectData| {
            p.source_name == c.source_name && p.model.project.proj_id == c.project_key
        };
        if same(&self.project) {
            self.project = project;
            self.history = history::History::new(&self.project);
            self.open_set.steps_seen = 0;
            self.records_revision = 0;
            self.dirty = true;
            self.reset_project_ui();
            self.selected = (!self.project.model.activities.is_empty()).then_some(0);
            return Ok(());
        }
        if let Some(p) = self.open_set.parked.iter_mut().find(|p| same(&p.project)) {
            p.history = history::History::new(&project);
            p.project = project;
            p.records_revision = 0;
            p.dirty = true;
            return Ok(());
        }
        // Not open: open it beside the others. Loading the saved copy first
        // lets a later save know the project's workspace rows.
        let id = match self.store.id_of(&c.source_name, c.project_key) {
            Some(id) => {
                self.store.load(id)?;
                id
            }
            None => self.store.save(&project)?,
        };
        self.park(id, project);
        if let Some(p) = self.open_set.parked.last_mut() {
            p.dirty = true;
        }
        self.save_open_set();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qa_sweep::Driver;

    fn copy(session: &str, name: &str, at: &str) -> RecoveryCopy {
        RecoveryCopy {
            session: session.into(),
            source_name: "A.xer".into(),
            project_key: 1,
            title: name.into(),
            text: "x".into(),
            windows_encoding: false,
            level_resources: false,
            saved_at: at.into(),
            base: None,
        }
    }

    #[test]
    fn a_stale_copy_never_lands_after_a_save() {
        let path =
            std::env::temp_dir().join(format!("openxer-recovery-stale-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let connection = Connection::open(&path).unwrap();
        let generation = AtomicU64::new(0);
        assert!(write(&connection, "s", &[copy("s", "A", "1")], 0, &generation).unwrap());
        assert_eq!(left_behind(&connection, "other").unwrap().len(), 1);
        // A save happened after the snapshot: the write is dropped.
        generation.fetch_add(1, Ordering::SeqCst);
        clear(&connection, "s").unwrap();
        assert!(!write(&connection, "s", &[copy("s", "A", "2")], 0, &generation).unwrap());
        assert!(left_behind(&connection, "other").unwrap().is_empty());
        drop(connection);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn the_newest_copy_of_each_project_is_offered() {
        let path =
            std::env::temp_dir().join(format!("openxer-recovery-newest-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let connection = Connection::open(&path).unwrap();
        let generation = AtomicU64::new(0);
        write(
            &connection,
            "old",
            &[copy("old", "Old", "2026-01-01")],
            0,
            &generation,
        )
        .unwrap();
        write(
            &connection,
            "new",
            &[copy("new", "New", "2026-02-01")],
            0,
            &generation,
        )
        .unwrap();
        let left = left_behind(&connection, "now").unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].title, "New");
        adopt(&connection, "now", &left).unwrap();
        assert!(left_behind(&connection, "later-run-of-now").unwrap().len() == 1);
        assert!(left_behind(&connection, "now").unwrap().is_empty());
        drop(connection);
        let _ = std::fs::remove_file(path);
    }

    /// Edit, let a copy fall due, and end the run without saving.
    fn crashed_run(name: &str) -> (PathBuf, String) {
        let mut d = Driver::new(name, crate::reports::test_support::scheduled_starter());
        let first = d.app.project.model.activities[0].task_id;
        d.app.project.details.get_mut(&first).unwrap().name = "Changed before the crash".into();
        d.app.set_autosave_minutes(1);
        d.app.dirty = true;
        d.frames(1);
        // A minute passes (egui time otherwise moves 0.1 s per frame).
        d.time += 61.;
        d.frames(2);
        d.app.flush_recovery();
        let path = d.dir.join("workspace.db");
        let saved = Store::open(path.clone()).unwrap().last().unwrap().unwrap();
        assert_ne!(
            saved.details[&first].name, "Changed before the crash",
            "the saved copy is untouched"
        );
        // The crash: the app goes away without on_exit or a save. The
        // driver would delete its folder, so it is handed a scratch one.
        let scratch = d.dir.with_extension("gone");
        let dir = std::mem::replace(&mut d.dir, scratch);
        drop(d);
        (dir, "Changed before the crash".into())
    }

    /// End a run without a save or on_exit (a crash, or the driver simply
    /// going away), keeping its workspace folder.
    fn end_run(mut d: Driver) -> PathBuf {
        let scratch = d.dir.with_extension("gone");
        let dir = std::mem::replace(&mut d.dir, scratch);
        drop(d);
        dir
    }

    fn restart(dir: &Path) -> Driver {
        let mut store = Store::open(dir.join("workspace.db")).unwrap();
        let mut project = store.last().unwrap().unwrap();
        // As main.rs does before the window opens: the last project is
        // saved again as the workspace stores it, with nothing changed.
        if let Some(doc) = store.save_stored(&project).unwrap().1 {
            crate::store::adopt_document(&mut project, doc).unwrap();
        }
        let mut d = Driver::new("recovery-restart", project);
        // The restarted app uses the crashed run's workspace, and the
        // driver deletes that folder when the test ends.
        d.app.store = store;
        let own = std::mem::replace(&mut d.dir, dir.to_path_buf());
        let _ = std::fs::remove_dir_all(own);
        d.app.open_set.recovery = Recovery::default();
        d.app.open_set.active = d.app.store.id_of(
            &d.app.project.source_name,
            d.app.project.model.project.proj_id,
        );
        d.frames(4);
        d
    }

    #[test]
    fn changes_survive_a_crash_and_are_recovered_on_the_next_start() {
        let (dir, name) = crashed_run("recovery-crash");
        let mut d = restart(&dir);
        assert!(
            d.app.open_set.recovery.offer.is_some(),
            "the copy is offered"
        );
        assert!(d.has("recovery_recover") && d.has("recovery_discard"));
        let first = d.app.project.model.activities[0].task_id;
        assert_ne!(d.app.project.details[&first].name, name);
        d.click("recovery_recover");
        assert!(d.app.open_set.recovery.offer.is_none());
        assert_eq!(d.app.project.details[&first].name, name);
        assert!(
            d.app.any_dirty(),
            "recovered changes are unsaved until saved"
        );
        assert!(d.app.status.contains("Recovered"), "{}", d.app.status);
        // Saving keeps them and deletes the copies.
        d.app.save();
        d.frames(2);
        let store = Store::open(dir.join("workspace.db")).unwrap();
        assert_eq!(store.last().unwrap().unwrap().details[&first].name, name);
        assert!(left_behind(store.connection(), "any").unwrap().is_empty());
    }

    #[test]
    fn discard_keeps_the_saved_project_and_deletes_the_copies() {
        let (dir, name) = crashed_run("recovery-discard");
        let mut d = restart(&dir);
        let first = d.app.project.model.activities[0].task_id;
        d.click("recovery_discard");
        assert!(d.app.open_set.recovery.offer.is_none());
        assert_ne!(d.app.project.details[&first].name, name);
        assert!(!d.app.any_dirty());
        let store = Store::open(dir.join("workspace.db")).unwrap();
        assert!(left_behind(store.connection(), "any").unwrap().is_empty());
    }

    /// Enter answers Recover, the button framed as the default: it never
    /// discards the changes kept from the crashed run.
    #[test]
    fn enter_recovers_and_never_discards() {
        let (dir, name) = crashed_run("recovery-enter");
        let mut d = restart(&dir);
        assert_eq!(
            crate::dialog_keys::top_default_label(&d.ctx).as_deref(),
            Some("Recover")
        );
        d.key(egui::Key::Enter);
        assert!(d.app.open_set.recovery.offer.is_none(), "Enter answers");
        let first = d.app.project.model.activities[0].task_id;
        assert_eq!(d.app.project.details[&first].name, name, "recovered");
        assert!(d.app.any_dirty(), "the recovered changes are open");
    }

    #[test]
    fn escape_leaves_the_copies_for_the_next_start() {
        let (dir, name) = crashed_run("recovery-escape");
        let mut d = restart(&dir);
        d.key(egui::Key::Escape);
        assert!(d.app.open_set.recovery.offer.is_none(), "Escape closes it");
        let first = d.app.project.model.activities[0].task_id;
        assert_ne!(d.app.project.details[&first].name, name, "nothing changed");
        let store = Store::open(dir.join("workspace.db")).unwrap();
        assert_eq!(left_behind(store.connection(), "any").unwrap().len(), 1);
    }

    /// The whole flow the review described: a crash; the next start offers
    /// the copy and the user presses Escape; a day's work is saved the
    /// normal way; at the start after that, the old copy is not offered
    /// (it would put the older project back as unsaved changes and the
    /// save on close would write it over the newer one) and it is gone.
    #[test]
    fn escape_then_a_normal_save_never_offers_the_old_copy_again() {
        let (dir, crashed) = crashed_run("recovery-escape-save");
        let mut d = restart(&dir);
        assert!(d.app.open_set.recovery.offer.is_some(), "offered once");
        d.key(egui::Key::Escape);
        assert!(d.app.open_set.recovery.offer.is_none());
        let first = d.app.project.model.activities[0].task_id;
        d.app.project.details.get_mut(&first).unwrap().name = "Saved the next day".into();
        d.app.dirty = true;
        d.frames(1);
        d.app.save();
        assert!(!d.app.any_dirty());
        d.frames(2);
        let dir = end_run(d);
        let d = restart(&dir);
        assert!(
            d.app.open_set.recovery.offer.is_none(),
            "an older copy is never offered over newer saved work"
        );
        assert!(!d.app.any_dirty());
        assert_eq!(d.app.project.details[&first].name, "Saved the next day");
        let store = Store::open(dir.join("workspace.db")).unwrap();
        let saved = store.last().unwrap().unwrap();
        assert_eq!(saved.details[&first].name, "Saved the next day");
        assert_ne!(saved.details[&first].name, crashed);
        assert!(
            left_behind(store.connection(), "any").unwrap().is_empty(),
            "the stale copy was removed"
        );
    }

    /// Escape still means "ask me next time" while the copy is newer than
    /// the saved project: a run that changes nothing (and saves nothing new)
    /// leaves it, and the start after offers it again and Recover restores
    /// it. The no-change save every start makes does not count as newer
    /// saved work.
    #[test]
    fn a_copy_still_newer_than_the_saved_project_is_offered_again_and_recovered() {
        let (dir, name) = crashed_run("recovery-escape-again");
        let mut d = restart(&dir);
        d.key(egui::Key::Escape);
        assert!(d.app.open_set.recovery.offer.is_none());
        let dir = end_run(d);
        let mut d = restart(&dir);
        assert!(d.app.open_set.recovery.offer.is_some(), "offered again");
        let first = d.app.project.model.activities[0].task_id;
        d.click("recovery_recover");
        assert_eq!(d.app.project.details[&first].name, name);
        assert!(d.app.any_dirty());
    }

    /// A save by another writer (the CLI or an agent, on the same workspace)
    /// after the copy was taken is newer saved work too: the copy is not
    /// offered and is removed.
    #[test]
    fn a_copy_older_than_a_save_by_another_writer_is_not_offered() {
        let (dir, name) = crashed_run("recovery-cli-save");
        {
            let mut store = Store::open(dir.join("workspace.db")).unwrap();
            let mut p = store.last().unwrap().unwrap();
            let first = p.model.activities[0].task_id;
            p.details.get_mut(&first).unwrap().name = "Saved by an agent".into();
            store.save(&p).unwrap();
        }
        let d = restart(&dir);
        assert!(d.app.open_set.recovery.offer.is_none());
        let first = d.app.project.model.activities[0].task_id;
        assert_eq!(d.app.project.details[&first].name, "Saved by an agent");
        assert_ne!(d.app.project.details[&first].name, name);
        let store = Store::open(dir.join("workspace.db")).unwrap();
        assert!(left_behind(store.connection(), "any").unwrap().is_empty());
    }

    /// A copy whose project is no longer in the workspace has no newer
    /// saved work to overwrite: it is still offered.
    #[test]
    fn a_copy_of_a_project_no_longer_in_the_workspace_is_offered() {
        let (dir, _) = crashed_run("recovery-gone");
        {
            let store = Store::open(dir.join("workspace.db")).unwrap();
            store
                .connection()
                .execute("DELETE FROM projects", [])
                .unwrap();
            assert_eq!(left_behind(store.connection(), "any").unwrap().len(), 1);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A workspace whose copies were written before copies kept their base:
    /// the table gains the column, and such a copy is offered as before
    /// (nothing says it is older than the saved project).
    #[test]
    fn a_copy_from_before_the_base_column_is_still_offered() {
        let path =
            std::env::temp_dir().join(format!("openxer-recovery-legacy-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE recovery_copies(session TEXT NOT NULL, \
                 source_name TEXT NOT NULL, project_key INTEGER NOT NULL, title TEXT NOT NULL, \
                 xer_content TEXT NOT NULL, windows_encoding INTEGER NOT NULL, \
                 level_resources INTEGER NOT NULL, saved_at TEXT NOT NULL, \
                 PRIMARY KEY(session, source_name, project_key)); \
                 INSERT INTO recovery_copies VALUES('old','A.xer',1,'A','x',0,0,'2026-11-01 08:00:00');",
            )
            .unwrap();
        let left = left_behind(&connection, "now").unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].base, None);
        assert_eq!(remove_stale(&connection).unwrap(), 0);
        assert!(stale(Some("a"), "b"));
        assert!(!stale(Some("a"), "a"), "same saved text: the copy is newer");
        assert!(!stale(Some("a"), NOT_SAVED), "project gone: offered");
        assert!(
            stale(Some(NOT_SAVED), "b"),
            "saved after a copy of an unsaved project"
        );
        drop(connection);
        let _ = std::fs::remove_file(path);
    }

    /// Two projects with unsaved changes and a copy of each. Closing one
    /// saves it, and its copy goes at once: a crash before the next timed
    /// copy must not leave its older copy to be offered.
    #[test]
    fn closing_a_project_removes_its_copy_at_once() {
        let doc = openxer_xer::starter_portfolio();
        let rb = ProjectData::from_document(doc.clone(), 1, "portfolio.xer".into()).unwrap();
        let ny = ProjectData::from_document(doc, 2, "portfolio.xer".into()).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "openxer-recovery-close-{}-{}",
            std::process::id(),
            chrono::Local::now().format("%H%M%S%f")
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut store = Store::open(dir.join("openxer.db")).unwrap();
        let ny_id = store.save(&ny).unwrap();
        let rb_id = store.save(&rb).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx, store, rb);
        app.restore_open_set();
        app.open_project_set(&[rb_id, ny_id]);
        app.dirty = true;
        for p in &mut app.open_set.parked {
            p.project.model.project.name = "NORTHYARD edited".into();
            p.dirty = true;
        }
        app.write_recovery_copies();
        app.flush_recovery();
        let keys = |app: &App| -> Vec<i64> {
            let mut k: Vec<i64> = left_behind(app.store.connection(), "other")
                .unwrap()
                .iter()
                .map(|c| c.project_key)
                .collect();
            k.sort();
            k
        };
        assert_eq!(keys(&app), vec![1, 2], "a copy of each");
        app.close_project_id(ny_id);
        assert!(!app.is_open(ny_id));
        assert_eq!(
            keys(&app),
            vec![1],
            "the closed project's copy went with its save"
        );
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The offer says, per project, when its copy was written, in the
    /// user's date and time format.
    #[test]
    fn the_offer_shows_when_each_copy_was_written_in_the_users_format() {
        let (dir, _) = crashed_run("recovery-stamp");
        let saved_at = {
            let store = Store::open(dir.join("workspace.db")).unwrap();
            left_behind(store.connection(), "any").unwrap()[0]
                .saved_at
                .clone()
        };
        let mut d = restart(&dir);
        let at = chrono::NaiveDateTime::parse_from_str(&saved_at, "%Y-%m-%d %H:%M:%S").unwrap();
        let shown = format!(
            "{}: changes kept at {} {}",
            d.app.project.model.project.name,
            crate::format::day(at.date()),
            crate::format::time_of_day(at.time())
        );
        let texts: Vec<String> = d.texts().into_iter().map(|(t, _)| t).collect();
        assert!(texts.contains(&shown), "{shown:?} in {texts:?}");
    }

    /// The shared calendar of a workspace project, as the dictionary holds it.
    fn dictionary_calendar_names(dir: &Path) -> Vec<String> {
        let store = Store::open(dir.join("workspace.db")).unwrap();
        let (dict, _) = store.dictionary().unwrap();
        let t = dict.table("CALENDAR").unwrap();
        t.rows
            .iter()
            .map(|r| t.value(r, "clndr_name").to_string())
            .collect()
    }

    /// A recovery copy is the project as it was before the crash, with the
    /// shared dictionary rows it held then. A row changed in the dictionary
    /// after the copy was taken (here by another project) keeps its newer
    /// content when the copy is recovered and synced and saved; the
    /// recovered project takes the newer row and keeps its own changes.
    #[test]
    fn a_recovered_copy_never_writes_stale_dictionary_rows_back() {
        let (dir, name) = crashed_run("recovery-stale-dictionary");
        {
            let mut store = Store::open(dir.join("workspace.db")).unwrap();
            assert!(store.shared().unwrap());
            let mut b = crate::reports::test_support::scheduled_starter();
            b.source_name = "B.xer".into();
            let id = store.save(&b).unwrap();
            let mut b = store.load(id).unwrap();
            let cal = b.document.table("CALENDAR").unwrap();
            let (k, n) = (
                cal.field("clndr_id").unwrap(),
                cal.field("clndr_name").unwrap(),
            );
            let row = cal.rows.iter().position(|r| !r[n].is_empty()).unwrap();
            let cal_id = cal.rows[row][k].clone();
            openxer_xer::edit::calendars::rename(&mut b, &cal_id, "Renamed after the copy")
                .unwrap();
            store.save(&b).unwrap();
        }
        assert!(dictionary_calendar_names(&dir)
            .iter()
            .any(|n| n == "Renamed after the copy"));
        let mut d = restart(&dir);
        d.click("recovery_recover");
        d.frames(3);
        d.app.save();
        d.frames(2);
        let names = dictionary_calendar_names(&dir);
        assert!(
            names.iter().any(|n| n == "Renamed after the copy"),
            "the newer name stays: {names:?}"
        );
        // The recovered project (parked beside B, which was saved last) was
        // saved with its own edit and the newer calendar.
        let store = Store::open(dir.join("workspace.db")).unwrap();
        let recovered: Vec<ProjectData> = store
            .list()
            .unwrap()
            .into_iter()
            .map(|(id, _)| store.load(id).unwrap())
            .filter(|p| p.details.values().any(|a| a.name == name))
            .collect();
        assert_eq!(recovered.len(), 1, "the recovered edit was saved");
        assert!(recovered[0]
            .calendar_names
            .iter()
            .any(|n| n == "Renamed after the copy"));
    }

    /// The worker writes with its own connection while the app holds the
    /// workspace lock (a lock file beside the database, not a database
    /// lock), and a save that meets a copy being written waits for it
    /// (both take BEGIN IMMEDIATE; the busy timeout covers the wait)
    /// instead of failing.
    #[test]
    fn the_worker_writes_under_the_workspace_lock_and_a_save_waits_for_it() {
        let dir = std::env::temp_dir().join(format!(
            "openxer-recovery-lock-{}-{}",
            std::process::id(),
            chrono::Local::now().format("%H%M%S%f")
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("openxer.db");
        let _lock = crate::store::WorkspaceLock::take(&path).unwrap();
        let mut store = Store::open(path.clone()).unwrap();
        let p = crate::reports::test_support::scheduled_starter();
        store.save(&p).unwrap();
        let worker = Connection::open(&path).unwrap();
        let generation = AtomicU64::new(0);
        assert!(write(&worker, "s", &[copy("s", "A", "1")], 0, &generation).unwrap());
        // A copy is being written (the worker holds the write lock) when
        // the app saves.
        let holder = std::thread::spawn({
            let path = path.clone();
            move || {
                let mut c = Connection::open(&path).unwrap();
                let tx = c
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                    .unwrap();
                tx.execute("UPDATE recovery_copies SET saved_at='2'", [])
                    .unwrap();
                std::thread::sleep(std::time::Duration::from_millis(400));
                tx.commit().unwrap();
            }
        });
        std::thread::sleep(std::time::Duration::from_millis(100));
        let mut edited = p.clone();
        edited.model.project.name = "Saved while a copy was written".into();
        store.save(&edited).expect("the save waits and succeeds");
        holder.join().unwrap();
        assert_eq!(
            store.last().unwrap().unwrap().model.project.name,
            "Saved while a copy was written"
        );
        drop(store);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A recovery copy is never a save: the stored project the undo rules
    /// read (B7/B8: the saved copy of the project) is untouched by it.
    #[test]
    fn a_recovery_copy_does_not_change_the_saved_project() {
        let mut d = Driver::new(
            "recovery-not-a-save",
            crate::reports::test_support::scheduled_starter(),
        );
        let id = d.app.open_set.active.unwrap();
        let saved = d.app.store.source(id).unwrap().2;
        openxer_xer::edit::tables::add_resource(&mut d.app.project, "WLD").unwrap();
        d.app.touch_records();
        d.app.dirty = true;
        d.frames(2);
        d.app.write_recovery_copies();
        d.app.flush_recovery();
        assert_eq!(
            left_behind(d.app.store.connection(), "other")
                .unwrap()
                .len(),
            1,
            "a copy was written"
        );
        assert_eq!(d.app.store.source(id).unwrap().2, saved, "not a save");
        assert!(d.app.any_dirty());
    }

    #[test]
    fn a_save_deletes_this_runs_copies_and_off_writes_none() {
        let mut d = Driver::new(
            "recovery-save",
            crate::reports::test_support::scheduled_starter(),
        );
        d.app.set_autosave_minutes(1);
        d.app.dirty = true;
        d.frames(1);
        d.time += 61.;
        d.frames(2);
        d.app.flush_recovery();
        let path = d.dir.join("workspace.db");
        let count = || {
            let s = Store::open(path.clone()).unwrap();
            left_behind(s.connection(), "any").unwrap().len()
        };
        assert_eq!(count(), 1, "a copy was written");
        d.app.save();
        d.app.flush_recovery();
        assert_eq!(count(), 0, "the save deleted it");
        // Off: no copy, however long the changes wait.
        d.app.set_autosave_minutes(0);
        d.app.dirty = true;
        d.frames(1);
        d.time += 600.;
        d.frames(2);
        d.app.flush_recovery();
        assert_eq!(count(), 0);
    }

    #[test]
    fn user_preferences_set_the_interval_and_explain_a_bad_entry() {
        let mut d = Driver::new(
            "recovery-prefs",
            crate::reports::test_support::scheduled_starter(),
        );
        assert_eq!(d.app.autosave_minutes(), DEFAULT_MINUTES, "on by default");
        d.app.open_user_preferences();
        d.app.prefs.tab = crate::user_prefs::PrefsTab::Application;
        d.frames(3);
        let ctrl = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        let type_minutes = |d: &mut Driver, text: &str| {
            d.click("prefs_autosave_minutes");
            d.key_with(egui::Key::A, ctrl);
            d.type_text(text);
            d.frames(2);
        };
        type_minutes(&mut d, "0");
        let error = d.app.prefs.autosave.error.clone().expect("explained");
        assert!(
            d.texts().iter().any(|(t, _)| *t == error),
            "shown in the dialog"
        );
        assert_eq!(d.app.autosave_minutes(), DEFAULT_MINUTES, "nothing changed");
        type_minutes(&mut d, "10");
        assert!(d.app.prefs.autosave.error.is_none());
        assert_eq!(d.app.autosave_minutes(), 10);
        d.click("prefs_autosave");
        assert_eq!(d.app.autosave_minutes(), 0, "unchecked is off");
        d.key(egui::Key::Escape);
        assert!(!d.app.prefs.open);
        // Kept in the workspace.
        let store = Store::open(d.dir.join("workspace.db")).unwrap();
        assert_eq!(store.preference(MINUTES_KEY).unwrap().as_deref(), Some("0"));
    }
}
