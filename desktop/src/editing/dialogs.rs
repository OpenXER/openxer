//! Dialogs of the editing commands: Copy Activity Options, the dissolve
//! confirmation, the cell summary, Renumber Activity IDs with its
//! Duplicates step, and Global Change (list, Modify Global Change and the
//! Global Change Report). Each dialog edits a draft; OK or Commit applies it
//! as one undo step, Cancel and Escape drop it.
use super::gc::{self, Assignment, Change, Condition, GlobalChange};
pub use super::gc_report::GcReport;
use super::ops::{self, CopyOptions, Renumber};
use super::*;
use crate::chrome::DialogButton;
use crate::dialog_kit::{self as kit, Col, Entry};
use crate::grid::columns::{Field, Source};
use crate::grid::filters::Op;

/// Renumber Activity IDs choices, kept between sessions.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct RenumberSettings {
    /// 0 increment from the selected activities, 1 auto-number, 2 replace
    /// beginning characters.
    pub mode: u8,
    pub step: String,
    pub prefix: String,
    pub suffix: String,
    pub auto_step: String,
    pub count: String,
    pub with: String,
    pub baselines: bool,
    /// Paste of copied activities shows the dialog for the new IDs. Off
    /// after its "Skip this dialog next time". A stored record without
    /// it reads off, as before the step existed.
    #[serde(default)]
    pub on_paste: bool,
}

impl Default for RenumberSettings {
    fn default() -> Self {
        Self {
            mode: 0,
            step: "10".into(),
            prefix: String::new(),
            suffix: String::new(),
            auto_step: String::new(),
            count: "1".into(),
            with: String::new(),
            baselines: true,
            on_paste: crate::defaults::renumber_on_paste(),
        }
    }
}

impl RenumberSettings {
    fn rule(&self) -> Result<Renumber, String> {
        let whole = |s: &str, what: &str| {
            s.trim()
                .parse::<i64>()
                .ok()
                .filter(|v| *v > 0)
                .ok_or_else(|| format!("{what} must be a positive whole number."))
        };
        Ok(match self.mode {
            0 => Renumber::Increment {
                step: whole(&self.step, "Increment Value")?,
            },
            1 => Renumber::Auto {
                prefix: self.prefix.clone(),
                suffix: self.suffix.clone(),
                step: whole(&self.auto_step, "Increment Value")?,
            },
            _ => Renumber::Replace {
                count: self
                    .count
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| "Number of characters must be a whole number.".to_string())?,
                with: self.with.clone(),
            },
        })
    }
}

pub struct RenumberDraft {
    pub settings: RenumberSettings,
    /// Selected activities in table order.
    pub rows: Vec<usize>,
    /// Set when the dialog gives the IDs of a paste (after Copy Activity
    /// Options) instead of renumbering activities.
    pub paste: Option<PasteIds>,
}

/// The copied activities a paste gives new IDs to, in clipboard order.
#[derive(Clone, Debug, PartialEq)]
pub struct PasteIds {
    pub codes: Vec<String>,
    pub names: Vec<String>,
}

/// The Duplicates step: activities whose new ID is taken, with an editable
/// replacement each.
pub struct DuplicatesDraft {
    /// Every renumbered activity with its planned new ID (for a paste, the
    /// position in the clipboard).
    pub targets: Vec<(usize, String)>,
    /// Positions in `targets` shown for editing.
    pub shown: Vec<usize>,
    /// The names of a paste's copied activities: the IDs are for a paste.
    pub paste: Option<Vec<String>>,
}

pub struct Summary {
    pub title: String,
    pub applied: usize,
    pub rejected: Vec<Rejected>,
}

#[derive(Default)]
pub struct GcList {
    pub selected: Option<usize>,
    /// Definition copied with Copy, added again by Paste.
    pub copied: Option<GlobalChange>,
}

pub struct GcEdit {
    /// Name of the definition being modified; `None` for a new one.
    pub original: Option<String>,
    pub gc: GlobalChange,
    /// Selected row: section (0 If, 1 Then, 2 Else) and index.
    pub selected: (u8, Option<usize>),
    /// Delete was clicked: the Yes / No box for the selected row is up.
    pub confirm_delete: bool,
}

#[derive(Default)]
pub struct EditDialogs {
    pub copy_options: Option<CopyOptions>,
    pub dissolve: Option<Vec<i64>>,
    pub summary: Option<Summary>,
    pub renumber: Option<RenumberDraft>,
    pub duplicates: Option<DuplicatesDraft>,
    /// Cancel was pressed on Duplicates during a paste: the question
    /// whether to go back and resolve them is up.
    pub resolve_question: bool,
    pub gc_list: Option<GcList>,
    pub gc_edit: Option<GcEdit>,
    pub gc_report: Option<GcReport>,
}

impl EditDialogs {
    pub fn any_open(&self) -> bool {
        self.copy_options.is_some()
            || self.dissolve.is_some()
            || self.summary.is_some()
            || self.renumber.is_some()
            || self.duplicates.is_some()
            || self.gc_list.is_some()
            || self.gc_edit.is_some()
            || self.gc_report.is_some()
    }
    /// Whether the dialog `name` is open.
    fn is_open(&self, name: &str) -> bool {
        match name {
            "summary" => self.summary.is_some(),
            "dissolve" => self.dissolve.is_some(),
            "copy_options" => self.copy_options.is_some(),
            "duplicates" => self.duplicates.is_some(),
            "renumber" => self.renumber.is_some(),
            "gc_report" => self.gc_report.is_some(),
            "gc_edit" => self.gc_edit.is_some(),
            "gc_list" => self.gc_list.is_some(),
            _ => false,
        }
    }

    /// Whether `name` is the dialog Escape closes: no dialog that stacks
    /// above it is open. A dialog being drawn has its state taken out of
    /// the struct, so the question is asked about the ones above it only.
    fn escape_closes(&self, name: &str) -> bool {
        const ABOVE_FIRST: [&str; 8] = [
            "summary",
            "dissolve",
            "copy_options",
            "duplicates",
            "renumber",
            "gc_report",
            "gc_edit",
            "gc_list",
        ];
        ABOVE_FIRST
            .iter()
            .take_while(|n| **n != name)
            .all(|n| !self.is_open(n))
    }
}

/// The If heading's two joins: every row must hold, or any one.
const GC_ALL: &str = "(All of these)";
const GC_ANY: &str = "(Any of these)";

fn title_of(catalog: &[Column], id: &str) -> String {
    columns::find(catalog, id).map_or_else(|| id.to_string(), |c| c.title.clone())
}

/// A white list area in the kit's sunken frame, scrolling its rows: the
/// flow-layout body of Modify Global Change.
fn sunken_list<R>(ui: &mut egui::Ui, size: Vec2, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    kit::paint_sunken(ui.painter(), rect, Color32::WHITE);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(3.)));
    egui::ScrollArea::both()
        .id_salt("gc_edit_rows")
        .auto_shrink([false, false])
        .show(&mut child, add)
        .inner
}

impl App {
    pub(super) fn escape_for(&self, ctx: &egui::Context, name: &str) -> bool {
        ctx.input(|i| i.key_pressed(Key::Escape)) && self.editing.dialogs.escape_closes(name)
    }

    /// Draw every open editing dialog.
    pub(crate) fn editing_dialogs(&mut self, ctx: &egui::Context) {
        let time = ctx.input(|i| i.time);
        self.gc_list_dialog(ctx);
        self.gc_edit_dialog(ctx, time);
        self.gc_report_dialog(ctx, time);
        self.renumber_dialog(ctx);
        self.duplicates_dialog(ctx, time);
        self.copy_options_dialog(ctx, time);
        self.dissolve_dialog(ctx, time);
        self.summary_dialog(ctx);
    }

    // ------------------------------------------------- Copy Activity Options

    fn copy_options_dialog(&mut self, ctx: &egui::Context, time: f64) {
        let Some(mut o) = self.editing.dialogs.copy_options.take() else {
            return;
        };
        let mut done = None;
        let mut help = false;
        // 420 x 280 px inside the frame: the choices under a heading, the
        // button column at the right.
        const W: f32 = 420.;
        const H: f32 = 280.;
        let ((), closed, actions) = kit::window(
            ctx,
            "copy_activity_options",
            "Copy Activity Options",
            Vec2::new(W, H),
            |c| {
                kit::strip(c, W, H);
                kit::heading(
                    c,
                    6.,
                    6.,
                    W - kit::STRIP_WIDTH - 12.,
                    "Specify the Activity information to be copied",
                );
                let mut hide = !o.show_dialog;
                let relationships = o.relationships;
                // Key, caption, value, indent, enabled; one 22 px row each,
                // a free row before the last.
                let checks: [(&str, &str, &mut bool, f32, bool); 10] = [
                    (
                        "relationships",
                        "Relationships",
                        &mut o.relationships,
                        12.,
                        true,
                    ),
                    (
                        "only_between",
                        "Only between copied activities",
                        &mut o.only_between,
                        30.,
                        relationships,
                    ),
                    (
                        "assignments",
                        "Resource & Role Assignments",
                        &mut o.assignments,
                        12.,
                        true,
                    ),
                    ("codes", "Activity Codes", &mut o.codes, 12., true),
                    ("notebooks", "Notebooks", &mut o.notebooks, 12., true),
                    ("steps", "Steps", &mut o.steps, 12., true),
                    ("expenses", "Expenses", &mut o.expenses, 12., true),
                    ("documents", "Documents", &mut o.documents, 12., true),
                    ("risks", "Risks", &mut o.risks, 12., true),
                    ("hide", kit::SKIP_DIALOG, &mut hide, 12., true),
                ];
                for (k, (key, caption, value, x, enabled)) in checks.into_iter().enumerate() {
                    let row = if key == "hide" { k + 1 } else { k };
                    c.checkbox(
                        &format!("copy_options_{key}"),
                        x,
                        34. + 22. * row as f32,
                        270.,
                        18.,
                        caption,
                        value,
                        enabled,
                    );
                }
                o.show_dialog = !hide;
                let out = kit::button_column(
                    c,
                    W,
                    &[
                        Entry::ok("copy_options_ok"),
                        Entry::cancel("copy_options_cancel"),
                        Entry::help("copy_options_help"),
                    ],
                );
                match out.clicked.as_deref() {
                    Some("copy_options_ok") => done = Some(true),
                    Some("copy_options_cancel") => done = Some(false),
                    Some(_) => help = true,
                    None => {}
                }
            },
        );
        self.record_dialog_actions(actions);
        if help {
            self.open_help_for("copy_activity_options");
        }
        if closed {
            done = Some(false);
        }
        match done {
            Some(true) => {
                self.editing.options.copy = o;
                self.save_editing_options();
                // A WBS paste with activities asks here too (`wbs_copy`).
                if !self.wbs_paste_after_activity_options(true, time) {
                    self.paste_after_options(time);
                }
            }
            Some(false) => {
                self.wbs_paste_after_activity_options(false, time);
            }
            None if self.escape_for(ctx, "copy_options") => {
                self.wbs_paste_after_activity_options(false, time);
            }
            None => self.editing.dialogs.copy_options = Some(o),
        }
    }

    // ---------------------------------------------------- confirm, summary

    fn dissolve_dialog(&mut self, ctx: &egui::Context, time: f64) {
        let Some(ids) = self.editing.dialogs.dissolve.clone() else {
            return;
        };
        let text = if ids.len() == 1 {
            "Dissolve the selected activity? Its predecessors are linked to its successors."
                .to_string()
        } else {
            format!(
                "Dissolve the {} selected activities? Their predecessors are linked to their successors.",
                ids.len()
            )
        };
        let mut rects = vec![];
        let mut answer = kit::message_box(
            ctx,
            "dissolve",
            "Confirmation",
            kit::MessageIcon::Question,
            &text,
            &["Yes", "No"],
            kit::Question::Dissolve.default_in(&["Yes", "No"]),
            &mut rects,
        )
        .map(|k| k == 0);
        self.record_dialog_actions(rects);
        if self.escape_for(ctx, "dissolve") {
            answer = Some(false);
        }
        match answer {
            Some(true) => {
                self.editing.dialogs.dissolve = None;
                self.dissolve_activities(&ids, time);
            }
            Some(false) => self.editing.dialogs.dissolve = None,
            None => {}
        }
    }

    fn summary_dialog(&mut self, ctx: &egui::Context) {
        let Some(s) = self.editing.dialogs.summary.take() else {
            return;
        };
        let mut close = false;
        // 634 x 290 px inside the frame: the count, the refused cells and
        // OK at the bottom right.
        const W: f32 = 634.;
        const H: f32 = 290.;
        let ((), closed, actions) =
            kit::window(ctx, "edit_summary", &s.title, Vec2::new(W, H), |c| {
                kit::text(
                    c,
                    8.,
                    8.,
                    &format!(
                        "{} cell{} changed. {} cell{} not changed:",
                        s.applied,
                        if s.applied == 1 { " was" } else { "s were" },
                        s.rejected.len(),
                        if s.rejected.len() == 1 {
                            " was"
                        } else {
                            "s were"
                        },
                    ),
                    true,
                );
                let cols = [
                    Col {
                        title: "Activity ID",
                        width: 110.,
                    },
                    Col {
                        title: "Column",
                        width: 140.,
                    },
                    Col {
                        title: "Value",
                        width: 150.,
                    },
                    Col {
                        title: "Reason",
                        width: 218.,
                    },
                ];
                kit::list(
                    c,
                    "summary_rows",
                    (6., 26., W - 12., 222.),
                    None,
                    &cols,
                    s.rejected.len(),
                    None,
                    None,
                    &mut |c, i, col, rect, sel| {
                        let r = &s.rejected[i];
                        match col {
                            0 => kit::cell_text(c, rect, &r.activity, sel, 3.),
                            1 => kit::cell_text(c, rect, &r.column, sel, 3.),
                            2 => kit::cell_text(c, rect, &r.value, sel, 3.),
                            _ => {
                                let at = rect.min - c.origin.to_vec2();
                                kit::error_text(
                                    c,
                                    at.x + 3.,
                                    at.y + 3.,
                                    rect.width() - 6.,
                                    &r.reason,
                                );
                            }
                        }
                    },
                );
                let out = kit::button_row(c, W - 6., H - 31., &[Entry::ok("summary_ok")]);
                close = out.clicked.is_some();
            });
        self.record_dialog_actions(actions);
        close |= closed;
        if !close && !self.escape_for(ctx, "summary") {
            self.editing.dialogs.summary = Some(s);
        }
    }

    // ---------------------------------------------------------- Renumber

    pub(crate) fn open_renumber(&mut self) {
        let rows = self.selected_in_order();
        if rows.is_empty() {
            self.status = "Select the activities to renumber.".into();
            return;
        }
        let mut settings = self.editing.options.renumber.clone();
        let n = ops::numbering(&self.project);
        if settings.prefix.is_empty() && settings.suffix.is_empty() {
            settings.prefix = n.prefix.clone();
            settings.suffix = n.suffix.clone();
        }
        if settings.auto_step.trim().is_empty() {
            settings.auto_step = n.step.to_string();
        }
        self.editing.dialogs.renumber = Some(RenumberDraft {
            settings,
            rows,
            paste: None,
        });
    }

    /// Paste after Copy Activity Options: the Renumber Activity IDs step
    /// when it is shown, else a paste with free IDs.
    pub(crate) fn paste_after_options(&mut self, time: f64) {
        let Some(clip) = self.editing.clipboard.as_ref() else {
            return;
        };
        if !self.editing.options.renumber.on_paste {
            self.paste_activities(time);
            return;
        }
        let codes = clip
            .activities
            .iter()
            .map(|c| c.activity.task_code.clone())
            .collect();
        let names = clip
            .activities
            .iter()
            .map(|c| c.details.name.clone())
            .collect();
        self.open_paste_renumber(PasteIds { codes, names });
    }

    /// Renumber Activity IDs for the copies a paste makes (the Activities
    /// table's paste, and a WBS paste with activities).
    pub(crate) fn open_paste_renumber(&mut self, ids: PasteIds) {
        let mut settings = self.editing.options.renumber.clone();
        let n = ops::numbering(&self.project);
        if settings.prefix.is_empty() && settings.suffix.is_empty() {
            settings.prefix = n.prefix.clone();
            settings.suffix = n.suffix.clone();
        }
        if settings.auto_step.trim().is_empty() {
            settings.auto_step = n.step.to_string();
        }
        self.editing.dialogs.renumber = Some(RenumberDraft {
            settings,
            rows: vec![],
            paste: Some(ids),
        });
    }

    /// New IDs for the draft: (activity, new ID) in table order.
    pub(crate) fn renumber_plan(&self, d: &RenumberDraft) -> Result<Vec<(usize, String)>, String> {
        let rule = d.settings.rule()?;
        if let Some(paste) = &d.paste {
            let existing: std::collections::HashSet<String> = self
                .project
                .model
                .activities
                .iter()
                .map(|a| a.task_code.clone())
                .collect();
            let new = ops::paste_ids(&paste.codes, &rule, &existing)?;
            return Ok(new.into_iter().enumerate().collect());
        }
        let codes: Vec<String> = d
            .rows
            .iter()
            .map(|&i| self.project.model.activities[i].task_code.clone())
            .collect();
        let new = ops::renumber_ids(&codes, &rule)?;
        Ok(d.rows.iter().copied().zip(new).collect())
    }

    fn renumber_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.editing.dialogs.renumber.take() else {
            return;
        };
        let plan = self.renumber_plan(&d);
        let has_baselines = self.has_baselines();
        let conflicts = plan
            .as_ref()
            .map(|p| self.plan_conflicts(d.paste.is_some(), p))
            .unwrap_or_default();
        let paste = d.paste.clone();
        let mut done = None;
        // 550 x 300 px inside the frame: options panel at 5,5, a status strip at 5,268,
        // OK / Cancel / Help in the button column.
        let mut help = false;
        let ((), closed, actions) = kit::window(
            ctx,
            "renumber_ids",
            "Renumber Activity IDs",
            Vec2::new(550., 300.),
            |c| {
                c.fill(0., 0., 550., 300., kit::STRIP);
                c.fill(5., 5., 433., 263., kit::FACE);
                c.lowered(5., 5., 433., 263.);
                c.fill(5., 268., 433., 27., kit::FACE);
                c.lowered(5., 268., 433., 27.);
                let s = &mut d.settings;
                let edit = |c: &mut crate::schedule_dialogs::Canvas,
                            key: &str,
                            x: f32,
                            y: f32,
                            value: &mut String,
                            on: bool| {
                    c.edit(
                        &format!("renumber_{key}"),
                        x,
                        y,
                        102.,
                        21.,
                        value,
                        on,
                        Align::Min,
                    );
                };
                for (mode, key, x, y, label) in [
                    (
                        0,
                        "increment",
                        14.,
                        15.,
                        "Increment Activity ID based on selected activities",
                    ),
                    (1, "auto", 17., 88., "Auto-number"),
                    (2, "replace", 14., 160., "Replace beginning characters"),
                ] {
                    if c.radio(
                        &format!("renumber_mode_{key}"),
                        x,
                        y,
                        304.,
                        17.,
                        label,
                        s.mode == mode,
                        true,
                    ) {
                        s.mode = mode;
                    }
                }
                kit::text(c, 40., 38., "Increment Value", s.mode == 0);
                edit(c, "step", 40., 51., &mut s.step, s.mode == 0);
                let on = s.mode == 1;
                kit::text(c, 40., 113., "Prefix", on);
                kit::text(c, 170., 113., "Suffix", on);
                kit::text(c, 300., 113., "Increment Value", on);
                edit(c, "prefix", 40., 128., &mut s.prefix, on);
                edit(c, "suffix", 170., 128., &mut s.suffix, on);
                edit(c, "auto_step", 300., 128., &mut s.auto_step, on);
                let on = s.mode == 2;
                kit::text(c, 40., 182., "Number of characters", on);
                kit::text(c, 170., 182., "Replace with", on);
                edit(c, "count", 40., 197., &mut s.count, on);
                edit(c, "with", 170., 197., &mut s.with, on);
                if paste.is_some() {
                    // A paste has no baselines to renumber; the dialog can
                    // be turned off for later pastes instead.
                    let mut hide = !s.on_paste;
                    c.checkbox(
                        "renumber_hide",
                        17.,
                        235.,
                        408.,
                        20.,
                        kit::SKIP_DIALOG,
                        &mut hide,
                        true,
                    );
                    s.on_paste = !hide;
                } else {
                    c.checkbox(
                        "renumber_baselines",
                        17.,
                        235.,
                        408.,
                        20.,
                        "Renumber selected activities in baselines",
                        &mut s.baselines,
                        has_baselines,
                    );
                }
                if !has_baselines && paste.is_none() {
                    kit::tooltip(
                        c,
                        "renumber_baselines_tip",
                        17.,
                        235.,
                        408.,
                        20.,
                        "This project has no baselines.",
                    );
                }
                // Status strip: the first new ID, or why the rule cannot run.
                let (status, ok) = match &plan {
                    Ok(plan) => {
                        let first = plan.first().map(|(i, new)| {
                            let old = match &paste {
                                Some(p) => p.codes[*i].clone(),
                                None => self.project.model.activities[*i].task_code.clone(),
                            };
                            format!("{old} becomes {new}")
                        });
                        let mut text = format!(
                            "{} activit{}",
                            plan.len(),
                            if plan.len() == 1 { "y" } else { "ies" }
                        );
                        if let Some(first) = first {
                            text += &format!(": {first}");
                        }
                        if !conflicts.is_empty() {
                            text += &format!("; {} new ID(s) already in use", conflicts.len());
                        }
                        // A paste settles IDs in use in the Duplicates step.
                        (text, conflicts.is_empty() || paste.is_some())
                    }
                    Err(e) => (e.clone(), false),
                };
                if ok {
                    c.ui.painter()
                        .with_clip_rect(c.r(7., 270., 429., 23.))
                        .text(
                            c.origin + Vec2::new(10., 275.),
                            Align2::LEFT_TOP,
                            status,
                            kit::dialog_font(),
                            INK,
                        );
                } else {
                    kit::error_text(c, 10., 275., 426., &status);
                }
                let out = kit::button_column(
                    c,
                    550.,
                    &[
                        Entry::Button("renumber_ok", "OK", plan.is_ok(), false),
                        Entry::cancel("renumber_cancel"),
                        Entry::help("renumber_help"),
                    ],
                );
                match out.clicked.as_deref() {
                    Some("renumber_ok") => done = Some(true),
                    Some("renumber_cancel") => done = Some(false),
                    Some(_) => help = true,
                    None => {}
                }
            },
        );
        self.record_dialog_actions(actions);
        if help {
            self.open_help_for("renumber_ids");
        }
        if closed {
            done = Some(false);
        }
        match done {
            Some(true) => {
                self.editing.options.renumber = d.settings.clone();
                self.save_editing_options();
                let time = ctx.input(|i| i.time);
                match plan {
                    Ok(plan) if conflicts.is_empty() && paste.is_some() => {
                        // A WBS paste with activities numbers its copies
                        // here too (`wbs_copy`).
                        let wbs = d
                            .settings
                            .rule()
                            .is_ok_and(|rule| self.wbs_paste_renumbered(&rule, time));
                        if !wbs {
                            let codes: Vec<String> = plan.into_iter().map(|(_, c)| c).collect();
                            self.paste_activities_as(&codes, time);
                        }
                    }
                    Ok(plan) if conflicts.is_empty() => {
                        self.apply_renumber(&plan, time);
                    }
                    Ok(plan) => {
                        self.editing.dialogs.duplicates = Some(DuplicatesDraft {
                            targets: plan,
                            shown: conflicts,
                            paste: paste.map(|p| p.names),
                        });
                    }
                    Err(e) => {
                        self.error(e);
                        self.editing.dialogs.renumber = Some(d);
                    }
                }
            }
            Some(false) => self.wbs_paste_renumber_cancelled(),
            None if self.escape_for(ctx, "renumber") => self.wbs_paste_renumber_cancelled(),
            None => self.editing.dialogs.renumber = Some(d),
        }
    }

    /// Positions of a plan whose new ID cannot be used: for a paste any ID
    /// in use (or given twice), for a renumber one that clashes with an
    /// activity keeping its ID.
    pub(crate) fn plan_conflicts(&self, paste: bool, plan: &[(usize, String)]) -> Vec<usize> {
        if paste {
            let codes: Vec<String> = plan.iter().map(|(_, c)| c.clone()).collect();
            ops::paste_conflicts(&self.project, &codes)
        } else {
            ops::renumber_conflicts(&self.project, plan)
        }
    }

    /// Set new IDs as one undo step.
    pub(crate) fn apply_renumber(&mut self, plan: &[(usize, String)], time: f64) {
        let conflicts = ops::renumber_conflicts(&self.project, plan);
        if !conflicts.is_empty() {
            self.error("Each Activity ID must be unique.".into());
            return;
        }
        let before = self.project.clone();
        let before_revision = self.records_revision;
        let mut renames = BTreeMap::new();
        for (i, code) in plan {
            let old = &mut self.project.model.activities[*i].task_code;
            if *old != *code {
                renames.insert(std::mem::replace(old, code.clone()), code.clone());
            }
        }
        let changed = renames.len();
        let mut note = String::new();
        if changed > 0 {
            self.edit_step(before, before_revision, time);
            if self.editing.options.renumber.baselines {
                let (done, skipped) = self.renumber_in_baselines(&renames);
                if done > 0 {
                    note = format!(", and in {done} baseline(s)");
                }
                if !skipped.is_empty() {
                    note += &format!(
                        "; not in {} (the new IDs are already used there)",
                        skipped.join(", ")
                    );
                }
            }
        }
        self.status = format!(
            "{changed} activit{} renumbered{note}",
            if changed == 1 { "y" } else { "ies" }
        );
    }

    fn duplicates_dialog(&mut self, ctx: &egui::Context, time: f64) {
        let Some(mut d) = self.editing.dialogs.duplicates.take() else {
            return;
        };
        let conflicts = self.plan_conflicts(d.paste.is_some(), &d.targets);
        let mut done = None;
        let mut help = false;
        // 600 x 270 px inside the frame: the note, the list of clashing IDs
        // with an edit for each new ID, the button column at the right.
        const W: f32 = 600.;
        const H: f32 = 270.;
        let names: Vec<String> = d
            .shown
            .iter()
            .map(|&k| match &d.paste {
                Some(names) => names.get(d.targets[k].0).cloned().unwrap_or_default(),
                None => self.project.name(d.targets[k].0).to_string(),
            })
            .collect();
        let ((), closed, actions) = kit::window(
            ctx,
            "renumber_duplicates",
            "Duplicates",
            Vec2::new(W, H),
            |c| {
                kit::strip(c, W, H);
                kit::text(
                    c,
                    8.,
                    8.,
                    "These Activity IDs are already in use. Enter a different Activity ID for each.",
                    true,
                );
                let cols = [
                    Col {
                        title: "Duplicate Activity ID",
                        width: 130.,
                    },
                    Col {
                        title: "Activity Name",
                        width: 200.,
                    },
                    Col {
                        title: "New Activity ID",
                        width: 140.,
                    },
                ];
                let body = c.r(6., 26., W - kit::STRIP_WIDTH - 12., H - 32.);
                let shown = &d.shown;
                let targets = &mut d.targets;
                kit::list(
                    c,
                    "duplicates_rows",
                    (6., 26., body.width(), body.height()),
                    None,
                    &cols,
                    shown.len(),
                    None,
                    None,
                    &mut |c, row, col, rect, sel| {
                        let k = shown[row];
                        match col {
                            0 if conflicts.contains(&k) => {
                                let at = rect.min - c.origin.to_vec2();
                                kit::error_text(
                                    c,
                                    at.x + 3.,
                                    at.y + 3.,
                                    rect.width() - 6.,
                                    &targets[k].1,
                                );
                            }
                            0 => kit::cell_text(c, rect, &targets[k].1, sel, 3.),
                            1 => kit::cell_text(c, rect, &names[row], sel, 3.),
                            _ => {
                                // The new ID is typed in place.
                                let field = rect.shrink2(Vec2::new(3., 1.)).intersect(body);
                                if field.width() > 0. && field.height() > 0. {
                                    let edit = egui::TextEdit::singleline(&mut targets[k].1)
                                        .id(c.id("duplicates_new").with(k))
                                        .frame(false)
                                        .font(kit::dialog_font())
                                        .text_color(INK)
                                        .margin(egui::Margin::ZERO)
                                        .desired_width(field.width());
                                    c.ui.put(field, edit);
                                }
                                c.act(&format!("duplicates_new_{k}"), field);
                            }
                        }
                    },
                );
                let out = kit::button_column(
                    c,
                    W,
                    &[
                        Entry::Button("duplicates_ok", "OK", conflicts.is_empty(), false),
                        Entry::cancel("duplicates_cancel"),
                        Entry::help("duplicates_help"),
                    ],
                );
                match out.clicked.as_deref() {
                    Some("duplicates_ok") => done = Some(true),
                    Some("duplicates_cancel") => done = Some(false),
                    Some(_) => help = true,
                    None => {}
                }
            },
        );
        self.record_dialog_actions(actions);
        if help {
            self.open_help_for("renumber_duplicates");
        }
        // During a paste, Cancel and the close box ask whether to go back
        // and resolve the duplicates, as observed in the reference: Yes
        // returns to the list, No pastes nothing. Escape on the question is
        // Yes's return to the list.
        let asking = self.editing.dialogs.resolve_question;
        if asking {
            let buttons = ["Yes", "No"];
            let mut rects = vec![];
            let answer = crate::kit_window::message_box(
                ctx,
                "resolve_duplicates",
                "Confirmation",
                crate::kit_window::MessageIcon::Question,
                "Activities whose IDs are still duplicates are left out of the \
                 paste if you stop now. Go back and give them new IDs?",
                &buttons,
                crate::kit_window::Question::ResolveDuplicates.default_in(&buttons),
                &mut rects,
            );
            self.record_dialog_actions(rects);
            match answer {
                Some(0) => self.editing.dialogs.resolve_question = false,
                Some(_) => {
                    self.editing.dialogs.resolve_question = false;
                    self.wbs_paste_renumber_cancelled();
                    self.status = "Nothing pasted.".into();
                    return;
                }
                None if ctx.input(|i| i.key_pressed(Key::Escape)) => {
                    self.editing.dialogs.resolve_question = false;
                }
                None => {}
            }
            self.editing.dialogs.duplicates = Some(d);
            return;
        }
        let escape = ctx.input(|i| i.key_pressed(Key::Escape));
        if closed {
            done = Some(false);
        }
        if done == Some(false) && d.paste.is_some() && !(closed && escape) {
            self.editing.dialogs.resolve_question = true;
            self.editing.dialogs.duplicates = Some(d);
            return;
        }
        match done {
            Some(true) => {
                for (_, code) in d.targets.iter_mut() {
                    *code = crate::records::clean(code.trim());
                }
                if d.paste.is_some() {
                    let codes: Vec<String> = d.targets.iter().map(|(_, c)| c.clone()).collect();
                    if !self.wbs_paste_with_ids(&codes, time) {
                        self.paste_activities_as(&codes, time);
                    }
                } else {
                    self.apply_renumber(&d.targets, time);
                }
            }
            Some(false) => self.wbs_paste_renumber_cancelled(),
            None if self.escape_for(ctx, "duplicates") => self.wbs_paste_renumber_cancelled(),
            None => self.editing.dialogs.duplicates = Some(d),
        }
    }

    // ------------------------------------------------------ Global Change

    pub(crate) fn open_global_change(&mut self) {
        let selected = (!self.editing.global_changes.is_empty()).then_some(0);
        self.editing.dialogs.gc_list = Some(GcList {
            selected,
            copied: None,
        });
    }

    /// A name not used by any saved definition.
    fn unique_gc_name(&self, base: &str) -> String {
        let taken = |n: &str| {
            self.editing
                .global_changes
                .iter()
                .any(|g| g.name.eq_ignore_ascii_case(n))
        };
        if !taken(base) {
            return base.to_string();
        }
        (2..)
            .map(|k| format!("{base} ({k})"))
            .find(|n| !taken(n))
            .unwrap_or_default()
    }

    fn gc_list_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.editing.dialogs.gc_list.take() else {
            return;
        };
        let mut close = false;
        let mut edit = None;
        let mut apply = None;
        let names: Vec<String> = self
            .editing
            .global_changes
            .iter()
            .map(|g| g.name.clone())
            .collect();
        let current = d.selected.filter(|&s| s < names.len());
        // 555 x 401 px inside the frame: the list at 6,6 and the button column.
        let mut help = false;
        let ((), closed, actions) = kit::window(
            ctx,
            "global_change_list",
            "Global Change",
            Vec2::new(555., 401.),
            |c| {
                c.fill(0., 0., 555., 401., kit::STRIP);
                let out = kit::list(
                    c,
                    "gc",
                    (6., 6., 431., 389.),
                    None,
                    &[Col {
                        title: "Name",
                        width: 425.,
                    }],
                    names.len(),
                    current,
                    Some(0),
                    &mut |c, i, _, rect, sel| kit::cell_text(c, rect, &names[i], sel, 21.),
                );
                if let Some(k) = out.clicked {
                    d.selected = Some(k);
                }
                if let Some(k) = out.double {
                    edit = Some(Some(k));
                }
                let any = current.is_some();
                let out = kit::button_column(
                    c,
                    555.,
                    &[
                        Entry::Button("gc_close", "&Close", true, false),
                        Entry::Button("gc_apply", "&Apply Change", any, true),
                        Entry::Button("gc_new", "&New...", true, false),
                        Entry::Button("gc_delete", "&Delete", any, false),
                        Entry::Button("gc_modify", "&Modify...", any, false),
                        Entry::Button("gc_copy", "Cop&y", any, false),
                        Entry::Button("gc_paste", "&Paste", d.copied.is_some(), false),
                        Entry::Button("gc_import", "&Import", true, false),
                        Entry::Button("gc_export", "&Export", any, false),
                        Entry::Button("gc_help", "&Help", true, true),
                    ],
                );
                if let Some(key) = out.clicked.as_deref() {
                    match key {
                        "gc_close" => close = true,
                        "gc_apply" => apply = current,
                        "gc_new" => edit = Some(None),
                        "gc_delete" => {
                            if let Some(s) = current {
                                self.editing.global_changes.remove(s);
                                self.save_global_changes();
                                d.selected = None;
                            }
                        }
                        "gc_modify" => edit = current.map(Some),
                        "gc_copy" => {
                            d.copied = current.map(|s| self.editing.global_changes[s].clone());
                        }
                        "gc_paste" => {
                            if let Some(mut g) = d.copied.clone() {
                                let list = &self.editing.global_changes;
                                g.name = gc::pasted_name(&g.name, |n| {
                                    list.iter().any(|x| x.name.eq_ignore_ascii_case(n))
                                });
                                self.editing.global_changes.push(g);
                                self.save_global_changes();
                                d.selected = Some(self.editing.global_changes.len() - 1);
                            }
                        }
                        "gc_import" => self.import_global_changes(),
                        "gc_export" => {
                            if let Some(s) = current {
                                self.export_global_change(s);
                            }
                        }
                        _ => help = true,
                    }
                }
            },
        );
        self.record_dialog_actions(actions);
        close |= closed;
        if help {
            self.open_help_for("global_change_list");
        }
        if let Some(target) = edit {
            self.editing.dialogs.gc_edit = Some(match target {
                Some(k) => GcEdit {
                    original: Some(self.editing.global_changes[k].name.clone()),
                    gc: self.editing.global_changes[k].clone(),
                    selected: (0, None),
                    confirm_delete: false,
                },
                None => GcEdit {
                    original: None,
                    gc: GlobalChange {
                        name: self.unique_gc_name("(New Global Change)"),
                        conditions: vec![Condition::default()],
                        then: vec![Assignment::default()],
                        otherwise: vec![],
                    },
                    selected: (0, Some(0)),
                    confirm_delete: false,
                },
            });
        }
        if let Some(k) = apply {
            let g = self.editing.global_changes[k].clone();
            self.start_global_change(&g);
        }
        if !close && !self.escape_for(ctx, "gc_list") {
            self.editing.dialogs.gc_list = Some(d);
        }
    }

    fn import_global_changes(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Global change", &["json"])
            .pick_file()
        else {
            return;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => return self.error(format!("Cannot read {}: {e}", path.display())),
        };
        let list = gc::from_json(&text).or_else(|_| {
            serde_json::from_str::<GlobalChange>(&text)
                .map(|g| vec![g])
                .map_err(|e| e.to_string())
        });
        match list {
            Ok(list) => {
                for mut g in list {
                    if gc::check(&g).is_err() {
                        continue;
                    }
                    g.name = self.unique_gc_name(&g.name);
                    self.editing.global_changes.push(g);
                }
                self.save_global_changes();
            }
            Err(e) => self.error(format!("Not a global change file: {e}")),
        }
    }

    fn export_global_change(&mut self, k: usize) {
        let g = self.editing.global_changes[k].clone();
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Global change", &["json"])
            .set_file_name(format!("{}.json", g.name))
            .save_file()
        {
            if let Err(e) = write_global_change(&path, &g) {
                self.error(e);
            }
        }
    }

    /// One section of Modify Global Change. If: the heading row, whose
    /// drop-down joins every row by All or Any, then the "Where" rows (an
    /// older definition that joins its rows both ways keeps a join per
    /// row). Then and Else: the first row has no label and each following
    /// row reads "And", by position. Each drop-down item is recorded as an
    /// action (`<combo>_<value>`) so the tests pick it as a click would.
    fn gc_section(
        &mut self,
        ui: &mut egui::Ui,
        e: &mut GcEdit,
        section: u8,
        catalog: &[Column],
        targets: &[(String, String)],
    ) {
        let caption = ["If", "Then", "Else"][section as usize];
        let grid_id = ("gc_section", section);
        egui::Grid::new(grid_id)
            .num_columns(6)
            .spacing([4., 3.])
            .show(ui, |ui| {
                let r = kit::flow_row_cell(ui, caption, e.selected == (section, None), 48.);
                self.action(&format!("gc_section_{caption}"), &r);
                if r.clicked() {
                    e.selected = (section, None);
                }
                let headings: &[&str] = if section == 0 {
                    &["Parameter", "Is", "Value", "High Value", ""]
                } else {
                    &["Parameter", "", "Value", "Operator", "Value"]
                };
                for h in headings {
                    ui.label(RichText::new(*h).strong());
                }
                ui.end_row();
                if section == 0 {
                    self.gc_if_rows(ui, e, catalog);
                } else {
                    let rows = if section == 1 {
                        &mut e.gc.then
                    } else {
                        &mut e.gc.otherwise
                    };
                    for (n, a) in rows.iter_mut().enumerate() {
                        let label = if n == 0 { "" } else { "And" };
                        let r =
                            kit::flow_row_cell(ui, label, e.selected == (section, Some(n)), 48.);
                        self.action(&format!("gc_{caption}_{n}"), &r);
                        if r.clicked() {
                            e.selected = (section, Some(n));
                        }
                        let key = format!("gc_{caption}_param_{n}");
                        let r = egui::ComboBox::from_id_salt(("gc_set_param", section, n))
                            .selected_text(title_of(catalog, &a.column))
                            .width(140.)
                            .height(300.)
                            .show_ui(ui, |ui| {
                                for (id, title) in targets {
                                    let item =
                                        ui.selectable_value(&mut a.column, id.clone(), title);
                                    self.action(&format!("{key}_{id}"), &item);
                                }
                            });
                        self.action(&key, &r.response);
                        ui.label("=");
                        let r = ui.add_sized([110., 20.], egui::TextEdit::singleline(&mut a.value));
                        self.action(&format!("gc_{caption}_value_{n}"), &r);
                        egui::ComboBox::from_id_salt(("gc_set_op", section, n))
                            .selected_text(&a.operator)
                            .width(44.)
                            .show_ui(ui, |ui| {
                                for o in gc::OPERATORS {
                                    ui.selectable_value(
                                        &mut a.operator,
                                        o.to_string(),
                                        if o.is_empty() { " " } else { o },
                                    );
                                }
                            });
                        ui.add_enabled_ui(!a.operator.is_empty(), |ui| {
                            ui.add_sized([100., 20.], egui::TextEdit::singleline(&mut a.value2));
                        });
                        ui.end_row();
                    }
                }
            });
    }

    /// The If heading row and the "Where" rows of Modify Global Change.
    fn gc_if_rows(&mut self, ui: &mut egui::Ui, e: &mut GcEdit, catalog: &[Column]) {
        // The heading: one join for every row, as the reference shows it.
        let heading = e.gc.heading();
        ui.label("");
        let mut pick = None;
        let r = egui::ComboBox::from_id_salt("gc_if_heading")
            .selected_text(match heading {
                Some(true) => GC_ALL,
                Some(false) => GC_ANY,
                None => "",
            })
            .width(140.)
            .show_ui(ui, |ui| {
                for (all, text, key) in [(true, GC_ALL, "all"), (false, GC_ANY, "any")] {
                    let item = ui.selectable_label(heading == Some(all), text);
                    self.action(&format!("gc_if_heading_{key}"), &item);
                    if item.clicked() {
                        pick = Some(all);
                    }
                }
            });
        self.action("gc_if_heading", &r.response);
        if let Some(all) = pick {
            e.gc.set_heading(all);
        }
        ui.end_row();
        let mixed = e.gc.heading().is_none();
        for (n, c) in e.gc.conditions.iter_mut().enumerate() {
            let r = kit::flow_row_cell(ui, "Where", e.selected == (0, Some(n)), 48.);
            self.action(&format!("gc_if_{n}"), &r);
            if r.clicked() {
                e.selected = (0, Some(n));
            }
            let before = c.column.clone();
            let key = format!("gc_if_param_{n}");
            let r = egui::ComboBox::from_id_salt(("gc_if_param", n))
                .selected_text(title_of(catalog, &c.column))
                .width(140.)
                .height(300.)
                .show_ui(ui, |ui| {
                    for col in catalog {
                        let item = ui.selectable_value(&mut c.column, col.id.clone(), &col.title);
                        self.action(&format!("{key}_{}", col.id), &item);
                    }
                });
            self.action(&key, &r.response);
            let column = columns::find(catalog, &c.column);
            let ops = gc::ops_for(column);
            let mut op = Op::from_key(&c.op).unwrap_or(Op::Equals);
            if c.column != before {
                // WBS starts on "is under", as observed; a choice the new
                // parameter does not offer goes back to equals.
                if column.is_some_and(|col| col.source == Source::Field(Field::Wbs)) {
                    op = Op::IsUnder;
                } else if !ops.contains(&op) {
                    op = Op::Equals;
                }
            }
            let key = format!("gc_if_op_{n}");
            let r = egui::ComboBox::from_id_salt(("gc_if_op", n))
                .selected_text(op.caption())
                .width(110.)
                .show_ui(ui, |ui| {
                    for o in &ops {
                        let item = ui.selectable_value(&mut op, *o, o.caption());
                        self.action(&format!("{key}_{}", o.key()), &item);
                    }
                });
            self.action(&key, &r.response);
            c.op = op.key().into();
            let r = ui.add_sized([100., 20.], egui::TextEdit::singleline(&mut c.value));
            self.action(&format!("gc_if_value_{n}"), &r);
            ui.add_enabled_ui(op.uses_high(), |ui| {
                ui.add_sized([80., 20.], egui::TextEdit::singleline(&mut c.high));
            });
            if mixed && n > 0 {
                // An older definition's own joins.
                egui::ComboBox::from_id_salt(("gc_if_join", n))
                    .selected_text(if c.and { "And" } else { "Or" })
                    .width(50.)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut c.and, true, "And");
                        ui.selectable_value(&mut c.and, false, "Or");
                    });
            } else {
                ui.label("");
            }
            ui.end_row();
        }
    }

    /// Move Up or Move Down on the selected statement: it swaps with its
    /// neighbour in the same section and stays selected.
    fn gc_move(e: &mut GcEdit, up: bool) {
        let (section, Some(n)) = e.selected else {
            return;
        };
        let len = match section {
            0 => e.gc.conditions.len(),
            1 => e.gc.then.len(),
            _ => e.gc.otherwise.len(),
        };
        let to = if up { n.checked_sub(1) } else { Some(n + 1) };
        let Some(to) = to.filter(|&to| to < len) else {
            return;
        };
        match section {
            0 => e.gc.conditions.swap(n, to),
            1 => e.gc.then.swap(n, to),
            _ => e.gc.otherwise.swap(n, to),
        }
        e.selected = (section, Some(to));
    }

    /// Which move arrows apply (left, up, down, right): up and down within
    /// the selected statement's section; left and right are not offered.
    fn gc_move_enabled(e: &GcEdit) -> [bool; 4] {
        let (section, Some(n)) = e.selected else {
            return [false; 4];
        };
        let len = match section {
            0 => e.gc.conditions.len(),
            1 => e.gc.then.len(),
            _ => e.gc.otherwise.len(),
        };
        [false, n > 0, n + 1 < len, false]
    }
    fn gc_edit_dialog(&mut self, ctx: &egui::Context, _time: f64) {
        let Some(mut e) = self.editing.dialogs.gc_edit.take() else {
            return;
        };
        let catalog = columns::catalog(&self.project);
        let targets: Vec<(String, String)> = gc::assignable(&catalog)
            .into_iter()
            .map(|c| (c.id.clone(), c.title.clone()))
            .collect();
        let mut done = None;
        let mut change = false;
        let mut clicked = None;
        let keys = [
            "gc_edit_ok",
            "gc_edit_cancel",
            "gc_edit_change",
            "gc_edit_add",
            "gc_edit_delete",
            "gc_edit_help",
        ];
        let can_delete = e.selected.1.is_some();
        let buttons = [
            DialogButton::ok(),
            DialogButton::cancel(),
            DialogButton::new("Change", None).grouped(),
            DialogButton::new("Add", None),
            DialogButton::new("Delete", None).enabled(can_delete),
            DialogButton::help(),
        ];
        let pad = Self::gc_move_enabled(&e);
        let mut moved = None;
        chrome::dialog_window("Modify Global Change")
            .id(egui::Id::new("global_change_edit"))
            .resizable(false)
            .show(ctx, |ui| {
                let (_, button, responses, (dir, rects)) = chrome::dialog_with_buttons_and_pad(
                    ui,
                    &buttons,
                    Some(("gc_move", pad)),
                    |ui| {
                        ui.set_min_size(Vec2::new(620., 414.));
                        self.gc_edit_body(ui, &mut e, &catalog, &targets);
                    },
                );
                for (key, r) in keys.iter().zip(&responses) {
                    self.action(key, r);
                }
                self.record_dialog_actions(rects);
                moved = dir;
                clicked = button.map(|k| keys[k]);
            });
        match moved {
            Some(kit::Move::Up) => Self::gc_move(&mut e, true),
            Some(kit::Move::Down) => Self::gc_move(&mut e, false),
            _ => {}
        }
        match clicked {
            Some("gc_edit_ok") => done = Some(true),
            Some("gc_edit_cancel") => done = Some(false),
            Some("gc_edit_change") => change = true,
            Some("gc_edit_add") => {
                let (section, _) = e.selected;
                let at = match section {
                    0 => {
                        // A new row joins the others by the heading.
                        let and = e.gc.heading().unwrap_or(true);
                        e.gc.conditions.push(Condition {
                            and,
                            ..Condition::default()
                        });
                        e.gc.conditions.len() - 1
                    }
                    1 => {
                        e.gc.then.push(Assignment::default());
                        e.gc.then.len() - 1
                    }
                    _ => {
                        e.gc.otherwise.push(Assignment::default());
                        e.gc.otherwise.len() - 1
                    }
                };
                e.selected = (section, Some(at));
            }
            Some("gc_edit_delete") => e.confirm_delete = e.selected.1.is_some(),
            Some(_) => self.open_help_for("Modify Global Change"),
            None => {}
        }
        if e.confirm_delete {
            self.gc_confirm_delete(ctx, &mut e);
        }
        self.gc_edit_finish(ctx, e, done, change);
    }

    /// Delete in Modify Global Change asks before the selected line goes:
    /// Yes (the default, so Enter too) removes it; No and Escape keep it.
    fn gc_confirm_delete(&mut self, ctx: &egui::Context, e: &mut GcEdit) {
        let (section, row) = e.selected;
        let text = format!(
            "Delete the selected {} line?",
            ["If", "Then", "Else"][section.min(2) as usize]
        );
        let mut rects = vec![];
        let clicked = crate::kit_window::message_box(
            ctx,
            "gc_delete_line",
            "Confirmation",
            crate::kit_window::MessageIcon::Question,
            &text,
            &["Yes", "No"],
            crate::kit_window::Question::GlobalChangeDeleteLine.default_in(&["Yes", "No"]),
            &mut rects,
        );
        self.record_dialog_actions(rects);
        let escape = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
        match clicked {
            Some(0) => {
                if let Some(n) = row {
                    match section {
                        0 if n < e.gc.conditions.len() => {
                            e.gc.conditions.remove(n);
                        }
                        1 if n < e.gc.then.len() => {
                            e.gc.then.remove(n);
                        }
                        2 if n < e.gc.otherwise.len() => {
                            e.gc.otherwise.remove(n);
                        }
                        _ => {}
                    }
                    e.selected = (section, None);
                }
                e.confirm_delete = false;
            }
            Some(_) => e.confirm_delete = false,
            None if escape => e.confirm_delete = false,
            None => {}
        }
    }

    /// The name, subject area and the If / Then / Else rows of Modify Global
    /// Change.
    fn gc_edit_body(
        &mut self,
        ui: &mut egui::Ui,
        e: &mut GcEdit,
        catalog: &[Column],
        targets: &[(String, String)],
    ) {
        egui::Grid::new("gc_edit_head")
            .num_columns(2)
            .spacing([8., 4.])
            .show(ui, |ui| {
                ui.label("Global Change Name");
                let r = ui.add(egui::TextEdit::singleline(&mut e.gc.name).desired_width(420.));
                self.action("gc_edit_name", &r);
                ui.end_row();
                ui.label("Select Subject Area");
                ui.add_enabled_ui(false, |ui| {
                    egui::ComboBox::from_id_salt("gc_subject")
                        .selected_text("Activities")
                        .width(200.)
                        .show_ui(ui, |_| {});
                });
                ui.end_row();
            });
        ui.add_space(6.);
        sunken_list(ui, Vec2::new(612., 360.), |ui| {
            for section in 0..3u8 {
                self.gc_section(ui, e, section, catalog, targets);
                ui.separator();
            }
        });
    }

    /// OK, Cancel and Change of Modify Global Change.
    fn gc_edit_finish(&mut self, ctx: &egui::Context, e: GcEdit, done: Option<bool>, change: bool) {
        let finished = |app: &mut App, e: &GcEdit| -> Result<(), String> {
            let mut g = e.gc.clone();
            g.name = g.name.trim().to_string();
            gc::check(&g)?;
            let clash = app.editing.global_changes.iter().any(|x| {
                x.name.eq_ignore_ascii_case(&g.name) && Some(&x.name) != e.original.as_ref()
            });
            if clash {
                return Err("A global change with this name already exists.".into());
            }
            match e
                .original
                .as_ref()
                .and_then(|o| app.editing.global_changes.iter().position(|x| &x.name == o))
            {
                Some(k) => app.editing.global_changes[k] = g,
                None => app.editing.global_changes.push(g),
            }
            app.save_global_changes();
            Ok(())
        };
        if change {
            match gc::check(&e.gc) {
                Ok(()) => {
                    let g = e.gc.clone();
                    self.start_global_change(&g);
                }
                Err(err) => self.error(err),
            }
            self.editing.dialogs.gc_edit = Some(e);
            return;
        }
        match done {
            Some(true) => match finished(self, &e) {
                Ok(()) => {
                    if let Some(list) = self.editing.dialogs.gc_list.as_mut() {
                        list.selected = self
                            .editing
                            .global_changes
                            .iter()
                            .position(|x| x.name == e.gc.name.trim());
                    }
                }
                Err(err) => {
                    self.error(err);
                    self.editing.dialogs.gc_edit = Some(e);
                }
            },
            Some(false) => {}
            None if self.escape_for(ctx, "gc_edit") => {}
            None => self.editing.dialogs.gc_edit = Some(e),
        }
    }

    /// Work out a global change and open its report. The project is left as
    /// it was: each change is tried with the in-place editing rules on the
    /// live project, then everything is put back. From an Activities view
    /// only the activities its filter shows are considered (`gc::scope`).
    pub(crate) fn global_change_preview(&mut self, g: &GlobalChange) -> Vec<Change> {
        let saved = self.project.clone();
        let (revision, serial) = (self.records_revision, self.records_serial);
        let (status, error, dirty) = (self.status.clone(), self.error.clone(), self.dirty);
        let catalog = columns::catalog(&self.project);
        let filter = self
            .views
            .main
            .is_activities()
            .then_some((&self.grid.layout.filter, self.grid.user_filters.as_slice()));
        let in_scope = gc::scope(&self.project, &self.grid_index(), &catalog, filter);
        let changes = {
            let mut host = gc::AppHost::new(self, &catalog);
            openxer_xer::edit::global_change::preview(&mut host, g, &in_scope)
        };
        self.project = saved;
        self.records_revision = revision;
        self.records_serial = serial.max(self.records_serial);
        self.status = status;
        self.error = error;
        self.dirty = dirty;
        changes
    }

    fn start_global_change(&mut self, g: &GlobalChange) {
        let changes = self.global_change_preview(g);
        self.editing.dialogs.gc_report = Some(self.gc_report_for(&g.name, changes));
    }

    /// The report window's state for `changes`: header lines and the Save
    /// Report options last used.
    pub(crate) fn gc_report_for(&self, name: &str, changes: Vec<Change>) -> GcReport {
        let mut r = GcReport::new(name, changes);
        r.projects = self.gc_report_projects(&r.changes);
        r.file = self.gc_report_file();
        r
    }

    /// Commit Changes: apply the checked lines as one undo step.
    pub(crate) fn commit_global_change(&mut self, changes: &[Change], time: f64) {
        let before = self.project.clone();
        let before_revision = self.records_revision;
        let catalog = columns::catalog(&self.project);
        let (applied, failed) = {
            let mut host = gc::AppHost::new(self, &catalog);
            openxer_xer::edit::global_change::commit(&mut host, changes)
        };
        if applied > 0 {
            self.edit_step(before, before_revision, time);
        } else {
            self.project = before;
            self.records_revision = before_revision;
        }
        self.error = None;
        self.status = if failed == 0 {
            format!("Global change: {applied} values changed")
        } else {
            format!("Global change: {applied} values changed, {failed} could not be changed")
        };
    }
}

/// Write global change `g` to a global change file at `path`, whole or not
/// at all.
pub(crate) fn write_global_change(path: &Path, g: &GlobalChange) -> Result<(), String> {
    openxer_xer::write_file_atomic(path, gc::to_json(std::slice::from_ref(g)).as_bytes())
        .map_err(|e| format!("Cannot write {}: {e}", path.display()))
}
