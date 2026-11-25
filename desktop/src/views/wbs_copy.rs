//! Paste of a copied WBS element in the WBS view. Copy WBS Options chooses
//! what comes along with the elements (notebook, WBS milestones, documents,
//! activities); with activities, the shared Copy Activity Options follows,
//! then the Activities table's own Renumber Activity IDs step (and its
//! Duplicates step) when paste shows it, and the whole paste is one undo
//! step. Without that step the copies keep the IDs paste gives them: their
//! own ID when it is free, else the project's auto-numbering.
use super::wbs::{copy_subtree, num, project_node, subtree_activities, Clip, CopyParts};
use super::*;
use crate::dialog_kit::{self as kit, Entry};
use crate::editing::dialogs::PasteIds;
use crate::editing::ops::{self, CopyOptions, Renumber};
use std::collections::{HashMap, HashSet};

/// The Copy WBS Options choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WbsCopyOptions {
    pub notebook: bool,
    pub milestones: bool,
    pub documents: bool,
    pub activities: bool,
    /// Paste shows Copy WBS Options; off after its "Skip this dialog
    /// next time", for the rest of the session.
    pub show_dialog: bool,
}

impl Default for WbsCopyOptions {
    /// Everything ticked, as the reference starts (and Copy Activity
    /// Options).
    fn default() -> Self {
        Self {
            notebook: true,
            milestones: true,
            documents: true,
            activities: true,
            show_dialog: true,
        }
    }
}

impl WbsCopyOptions {
    fn parts(self) -> CopyParts {
        CopyParts {
            notebook: self.notebook,
            milestones: self.milestones,
            documents: self.documents,
        }
    }
}

/// Where a WBS paste stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Copy WBS Options is open.
    WbsOptions,
    /// Copy Activity Options is open for the activities.
    ActivityOptions,
    /// Renumber Activity IDs (or its Duplicates step) is open for the
    /// copies.
    Renumber,
}

/// A paste of copied WBS elements waiting for its options.
#[derive(Clone, Debug, PartialEq)]
pub struct WbsPaste {
    pub clip: Clip,
    /// The element to paste under (`None`: the project row).
    pub target: Option<i64>,
    pub options: WbsCopyOptions,
    pub stage: Stage,
}

/// What a WBS paste made.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WbsPasted {
    /// The copy of the copied element.
    pub root: i64,
    /// The pasted activities' model indices.
    pub rows: Vec<usize>,
    /// What the activity paste left out, for the status line.
    pub note: Option<String>,
}

/// Paste a copy of the element `clip.root` under `target` with what
/// `options` chooses: its notebook, milestones and documents (the shared
/// `copy_subtree`), and with `activities` (Copy Activity Options) each of
/// its activities and those of the elements below, pasted under the copy of
/// its own element. `codes` are the copies' new IDs (the renumber step), one
/// per [`copied_activities`] entry; `None` keeps the IDs the paste gives
/// them.
pub fn paste_wbs_copy(
    p: &mut ProjectData,
    clip: &Clip,
    target: Option<i64>,
    options: WbsCopyOptions,
    activities: &CopyOptions,
    codes: Option<&[String]>,
) -> Result<WbsPasted, String> {
    let parent = target.or(project_node(p));
    // The activities to copy, in table order, before the elements change.
    let indices = copied_activities(p, clip, options);
    if let Some(codes) = codes {
        if codes.len() != indices.len() || !ops::paste_conflicts(p, codes).is_empty() {
            return Err("Each Activity ID must be unique.".into());
        }
    }
    let map = copy_subtree(p, clip.root, parent, options.parts())?;
    let root = map[&clip.root];
    if indices.is_empty() {
        return Ok(WbsPasted {
            root,
            ..WbsPasted::default()
        });
    }
    let copied = ops::copy(p, &indices);
    let pasted = match codes {
        Some(codes) => ops::paste_renumbered(p, &copied, activities, root, codes)?,
        None => ops::paste_activities(p, &copied, activities, root, None)?,
    };
    // Each copy goes under the copy of its own element.
    let moved: HashMap<i64, i64> = pasted
        .rows
        .iter()
        .zip(&copied.activities)
        .map(|(&i, c)| {
            let to = map.get(&c.details.wbs).copied().unwrap_or(root);
            (p.model.activities[i].task_id, to)
        })
        .collect();
    place_copies(p, &moved);
    Ok(WbsPasted {
        root,
        note: pasted.note(),
        rows: pasted.rows,
    })
}

/// The activities a WBS paste copies (model indices, table order): those of
/// the copied element and every element below it, when `options` takes
/// activities.
pub fn copied_activities(p: &ProjectData, clip: &Clip, options: WbsCopyOptions) -> Vec<usize> {
    if !options.activities {
        return vec![];
    }
    let tasks: HashSet<i64> = subtree_activities(p, clip.root)
        .into_iter()
        .map(|(task, _)| task)
        .collect();
    (0..p.model.activities.len())
        .filter(|&i| tasks.contains(&p.model.activities[i].task_id))
        .collect()
}

/// The copies' IDs by the Renumber Activity IDs choice `how`, as that step
/// plans them for a paste (`ops::paste_ids`), one per [`copied_activities`]
/// entry.
pub fn wbs_paste_ids(
    p: &ProjectData,
    clip: &Clip,
    options: WbsCopyOptions,
    how: &Renumber,
) -> Result<Vec<String>, String> {
    let codes: Vec<String> = copied_activities(p, clip, options)
        .into_iter()
        .map(|i| p.model.activities[i].task_code.clone())
        .collect();
    let existing: HashSet<String> = p
        .model
        .activities
        .iter()
        .map(|a| a.task_code.clone())
        .collect();
    ops::paste_ids(&codes, how, &existing)
}

/// Put pasted activities (`task_id` to WBS element) under their elements:
/// their details and every row of theirs that names a WBS element.
fn place_copies(p: &mut ProjectData, moved: &HashMap<i64, i64>) {
    for (task, wbs) in moved {
        if let Some(d) = p.details.get_mut(task) {
            d.wbs = *wbs;
        }
    }
    let pid = p.model.project.proj_id;
    let cell = |r: &[String], c: usize| r.get(c).map(|v| num(v));
    for t in p.document.tables.iter_mut() {
        let (Some(task_col), Some(wbs_col)) = (t.field("task_id"), t.field("wbs_id")) else {
            continue;
        };
        let proj_col = t.field("proj_id");
        for r in t.rows.iter_mut() {
            let Some(to) = cell(r, task_col).and_then(|task| moved.get(&task)) else {
                continue;
            };
            let mine = proj_col.is_none_or(|c| cell(r, c) == Some(pid));
            if let Some(v) = r.get_mut(wbs_col).filter(|v| mine && !v.trim().is_empty()) {
                *v = to.to_string();
            }
        }
    }
}

impl App {
    /// Paste of copied WBS elements: Copy WBS Options first.
    pub(crate) fn request_wbs_paste(&mut self, clip: Clip, target: Option<i64>) {
        self.views.wbs.paste = Some(WbsPaste {
            clip,
            target,
            options: self.views.wbs.copy_options,
            stage: Stage::WbsOptions,
        });
    }

    /// Copy WBS Options (420 x 170 px inside the frame): the choices in an
    /// "Optionally Include" group, "Skip this dialog next time" below
    /// it, OK, Cancel and Help in the button column.
    pub(super) fn copy_wbs_options_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut paste) = self
            .views
            .wbs
            .paste
            .clone()
            .filter(|p| p.stage == Stage::WbsOptions)
        else {
            return;
        };
        // Turned off (by an earlier OK, not by the box ticked in this
        // open dialog): the paste goes on with the choices kept.
        if !self.views.wbs.copy_options.show_dialog {
            self.wbs_paste_options_done(paste, ctx.input(|i| i.time));
            return;
        }
        const W: f32 = 420.;
        const H: f32 = 170.;
        let mut done = None;
        let mut help = false;
        let ((), closed, actions) = kit::window(
            ctx,
            "copy_wbs_options",
            "Copy WBS Options",
            Vec2::new(W, H),
            |c| {
                kit::strip(c, W, H);
                // The group: a 21 px heading bar, then one 22 px row per
                // choice.
                kit::section(
                    c,
                    6.,
                    6.,
                    W - kit::STRIP_WIDTH - 12.,
                    120.,
                    "Optionally Include",
                );
                let o = &mut paste.options;
                let mut hide = !o.show_dialog;
                let checks: [(&str, &str, &mut bool); 4] = [
                    ("notebook", "Notebook", &mut o.notebook),
                    ("milestones", "WBS Milestones", &mut o.milestones),
                    ("documents", "WPs & Docs", &mut o.documents),
                    ("activities", "Activities", &mut o.activities),
                ];
                for (row, (key, caption, value)) in checks.into_iter().enumerate() {
                    c.checkbox(
                        &format!("copy_wbs_options_{key}"),
                        12.,
                        34. + 22. * row as f32,
                        270.,
                        18.,
                        caption,
                        value,
                        true,
                    );
                }
                c.checkbox(
                    "copy_wbs_options_hide",
                    12.,
                    136.,
                    270.,
                    18.,
                    kit::SKIP_DIALOG,
                    &mut hide,
                    true,
                );
                o.show_dialog = !hide;
                let out = kit::button_column(
                    c,
                    W,
                    &[
                        Entry::ok("copy_wbs_options_ok"),
                        Entry::cancel("copy_wbs_options_cancel"),
                        Entry::help("copy_wbs_options_help"),
                    ],
                );
                match out.clicked.as_deref() {
                    Some("copy_wbs_options_ok") => done = Some(true),
                    Some("copy_wbs_options_cancel") => done = Some(false),
                    Some(_) => help = true,
                    None => {}
                }
            },
        );
        self.record_dialog_actions(actions);
        if help {
            self.open_help_for("copy_wbs_options");
        }
        if closed {
            done = Some(false);
        }
        match done {
            Some(true) => {
                self.views.wbs.copy_options = paste.options;
                self.wbs_paste_options_done(paste, ctx.input(|i| i.time));
            }
            Some(false) => self.views.wbs.paste = None,
            None => self.views.wbs.paste = Some(paste),
        }
    }

    /// Copy WBS Options closed with OK: Copy Activity Options next when
    /// activities come along (unless it is turned off), else the renumber
    /// step.
    fn wbs_paste_options_done(&mut self, paste: WbsPaste, time: f64) {
        let has_activities = paste.options.activities
            && !subtree_activities(&self.project, paste.clip.root).is_empty();
        if has_activities && self.editing.options.copy.show_dialog {
            self.views.wbs.paste = Some(WbsPaste {
                stage: Stage::ActivityOptions,
                ..paste
            });
            self.editing.dialogs.copy_options = Some(self.editing.options.copy.clone());
        } else {
            self.views.wbs.paste = None;
            self.wbs_paste_renumber(paste, time);
        }
    }

    /// Copy Activity Options closed (`ok`: with OK). When it was opened for
    /// a WBS paste, that paste goes on or is dropped here and `true` comes
    /// back; otherwise `false` (an Activities table paste).
    pub(crate) fn wbs_paste_after_activity_options(&mut self, ok: bool, time: f64) -> bool {
        match self.views.wbs.paste.take() {
            Some(paste) if paste.stage == Stage::ActivityOptions => {
                if ok {
                    self.wbs_paste_renumber(paste, time);
                }
                true
            }
            other => {
                self.views.wbs.paste = other;
                false
            }
        }
    }

    /// The renumber step of a WBS paste with activities: the Activities
    /// table's Renumber Activity IDs step for the copies when paste shows
    /// it (`defaults::renumber_on_paste`, until its "Skip this dialog next
    /// time"), else the paste with the IDs paste gives them.
    fn wbs_paste_renumber(&mut self, paste: WbsPaste, time: f64) {
        let rows = copied_activities(&self.project, &paste.clip, paste.options);
        if rows.is_empty() || !self.editing.options.renumber.on_paste {
            self.finish_wbs_paste(&paste, None, time);
            return;
        }
        let p = &self.project;
        let codes = rows
            .iter()
            .map(|&i| p.model.activities[i].task_code.clone())
            .collect();
        let names = rows.iter().map(|&i| p.name(i).to_string()).collect();
        self.views.wbs.paste = Some(WbsPaste {
            stage: Stage::Renumber,
            ..paste
        });
        self.open_paste_renumber(PasteIds { codes, names });
    }

    /// The WBS paste waiting at the renumber step, taken out.
    fn take_wbs_paste_at_renumber(&mut self) -> Option<WbsPaste> {
        match self.views.wbs.paste.take() {
            Some(paste) if paste.stage == Stage::Renumber => Some(paste),
            other => {
                self.views.wbs.paste = other;
                None
            }
        }
    }

    /// Renumber Activity IDs closed with OK and no ID in use. When it was
    /// opened for a WBS paste, the paste is made with the copies numbered by
    /// `rule` and `true` comes back; otherwise `false` (an Activities table
    /// paste).
    pub(crate) fn wbs_paste_renumbered(&mut self, rule: &Renumber, time: f64) -> bool {
        let Some(paste) = self.take_wbs_paste_at_renumber() else {
            return false;
        };
        self.finish_wbs_paste(&paste, Some(rule), time);
        true
    }

    /// The Duplicates step after Renumber Activity IDs closed with OK. When
    /// it was opened for a WBS paste, the paste is made with `codes` and
    /// `true` comes back; otherwise `false`.
    pub(crate) fn wbs_paste_with_ids(&mut self, codes: &[String], time: f64) -> bool {
        let Some(paste) = self.take_wbs_paste_at_renumber() else {
            return false;
        };
        self.finish_wbs_paste_as(&paste, Some(codes), time);
        true
    }

    /// Renumber Activity IDs or its Duplicates step closed without OK: a
    /// WBS paste waiting there is dropped, nothing is pasted.
    pub(crate) fn wbs_paste_renumber_cancelled(&mut self) {
        self.take_wbs_paste_at_renumber();
    }

    /// Paste as one undo step, the copies numbered by `renumber` (the
    /// Renumber Activity IDs choice) when given.
    pub(crate) fn finish_wbs_paste(
        &mut self,
        paste: &WbsPaste,
        renumber: Option<&Renumber>,
        time: f64,
    ) {
        let codes = match renumber
            .map(|how| wbs_paste_ids(&self.project, &paste.clip, paste.options, how))
            .transpose()
        {
            Ok(codes) => codes,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        self.finish_wbs_paste_as(paste, codes.as_deref(), time);
    }

    /// Paste as one undo step, the copies given `codes` when given.
    fn finish_wbs_paste_as(&mut self, paste: &WbsPaste, codes: Option<&[String]>, time: f64) {
        let before = self.project.clone();
        let before_revision = self.records_revision;
        let activities = self.editing.options.copy.clone();
        match paste_wbs_copy(
            &mut self.project,
            &paste.clip,
            paste.target,
            paste.options,
            &activities,
            codes,
        ) {
            Ok(pasted) => {
                if !pasted.rows.is_empty() {
                    self.project.rebuild_assignments();
                }
                self.edit_step(before, before_revision, time);
                self.views.wbs.selected = Some(pasted.root);
                if let Some(target) = paste.target {
                    self.views.wbs.collapsed.remove(&target);
                }
                let mut status = match pasted.rows.len() {
                    0 => "WBS pasted".to_string(),
                    1 => "WBS pasted with 1 activity".to_string(),
                    n => format!("WBS pasted with {n} activities"),
                };
                if let Some(note) = pasted.note {
                    status = format!("{status}. {note}");
                }
                self.status = status;
            }
            Err(e) => {
                self.project = before;
                self.status = e;
            }
        }
    }
}
