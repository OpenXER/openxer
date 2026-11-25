//! The Circular Relationships dialog and the App hooks around it.
use super::analysis::{self, Hints, LoopGroup};
use super::*;
use crate::dialog_kit::{self, Col};
use crate::schedule_dialogs::dialog_font;

const WIDTH: f32 = 930.;
const HEIGHT: f32 = 424.;
const CHAIN_COLS: [f32; 6] = [70., 150., 38., 50., 70., 150.];
const SUGGESTED_ROW: Color32 = Color32::from_rgb(255, 238, 190);

#[derive(Default)]
struct Clicks {
    goto: bool,
    delete: bool,
    reverse: bool,
    show: bool,
    recheck: bool,
    schedule: bool,
    close: bool,
    group: Option<usize>,
    row: Option<(usize, bool)>,
}

impl App {
    /// Activity label used in reasons: `ID (Name)`.
    fn loop_label(&self, i: usize) -> String {
        format!(
            "{} ({})",
            self.project.model.activities[i].task_code,
            self.project.name(i)
        )
    }

    /// When relationships were made, as far as this session and the file know.
    pub(crate) fn loop_hints(&self) -> Hints {
        let mut hints = Hints {
            added: self.history.link_additions(),
            ..Hints::default()
        };
        if let Some(t) = self.project.document.table("TASKPRED") {
            if t.field("update_date").is_some() {
                for r in &t.rows {
                    let stamp = t.value(r, "update_date");
                    let kind = match t.value(r, "pred_type") {
                        "PR_FS" => 0,
                        "PR_SS" => 1,
                        "PR_FF" => 2,
                        "PR_SF" => 3,
                        _ => 4,
                    };
                    let id = |name: &str| t.value(r, name).trim().parse::<i64>().ok();
                    if let (Some(succ), Some(pred), false) =
                        (id("task_id"), id("pred_task_id"), stamp.is_empty())
                    {
                        hints.updated.insert((pred, succ, kind), stamp.to_string());
                    }
                }
            }
        }
        hints
    }

    /// Every loop of the active project, with suggestions.
    pub(crate) fn loop_report(&self) -> analysis::LoopReport {
        analysis::analyze(&self.project.model, &self.loop_hints(), &|i| {
            self.loop_label(i)
        })
    }

    pub(crate) fn has_loops(&self) -> bool {
        analysis::has_loop(&self.project.model)
    }

    /// Open (or refresh) the dialog on the current relationships.
    pub(crate) fn open_loops(&mut self) {
        self.loops.open = true;
        self.loops.prompt = false;
        self.loops.view = None;
        self.refresh_loops();
    }

    fn refresh_loops(&mut self) {
        self.loops.report = self.loop_report();
        self.loops.fingerprint = fingerprint(&self.project.model);
        self.loops.view = None;
        self.select_loop_group(0);
    }

    fn select_loop_group(&mut self, group: usize) {
        let l = &mut self.loops;
        l.group = group.min(l.report.groups.len().saturating_sub(1));
        l.row = l
            .report
            .groups
            .get(l.group)
            .and_then(|g| g.suggested_row)
            .unwrap_or(0);
        l.go_succ = false;
    }

    /// Scheduling, Level and Check Schedule come here when the network has a
    /// loop: the dialog opens instead of a one-line error. True when it did.
    pub(crate) fn intercept_loops(&mut self) -> bool {
        if !self.has_loops() {
            return false;
        }
        self.sched.schedule_open = false;
        self.open_loops();
        let groups = self.loops.report.groups.len();
        self.status = format!(
            "The schedule was not run: {groups} circular relationship group{} found.",
            if groups == 1 { "" } else { "s" }
        );
        true
    }

    /// File open: a note in the status bar and a non-blocking offer.
    pub(crate) fn note_loops_on_open(&mut self) {
        self.loops = State::default();
        if !self.has_loops() {
            return;
        }
        let report = self.loop_report();
        let affected: usize = report.groups.iter().map(|g| g.members.len()).sum();
        self.status = format!(
            "{} circular relationship group{} found, {affected} activities affected.",
            report.groups.len(),
            if report.groups.len() == 1 { "" } else { "s" }
        );
        self.loops.prompt = true;
    }

    fn selected_chain_link(&self) -> Option<(usize, analysis::ChainTie)> {
        let g = self.loops.report.groups.get(self.loops.group)?;
        let tie = *g.chain.get(self.loops.row)?;
        (tie.link < self.project.model.links.len()).then_some((self.loops.group, tie))
    }

    fn loop_tie_text(&self, tie: analysis::ChainTie) -> String {
        let m = &self.project.model;
        format!(
            "{} to {} ({})",
            m.activities[tie.pred].task_code,
            m.activities[tie.succ].task_code,
            &m.links[tie.link].link_type.as_str()[3..]
        )
    }

    fn delete_loop_tie(&mut self) {
        let Some((_, tie)) = self.selected_chain_link() else {
            return;
        };
        let text = self.loop_tie_text(tie);
        match relationships::remove(&mut self.project.model, tie.link) {
            Ok(()) => {
                self.dirty = true;
                self.refresh_loops();
                self.loops.note = format!("Deleted {text}. Ctrl+Z restores it.");
                self.status = self.loops.note.clone();
            }
            Err(e) => self.loops.note = e,
        }
    }

    pub(super) fn reverse_loop_tie(&mut self) {
        let Some((_, tie)) = self.selected_chain_link() else {
            return;
        };
        let model = &mut self.project.model;
        let old = model.links[tie.link].clone();
        if let Err(e) = relationships::remove(model, tie.link) {
            self.loops.note = e;
            return;
        }
        match relationships::add(model, old.succ, old.pred, old.link_type, old.lag) {
            Ok(()) => {
                self.dirty = true;
                let text = self.loop_tie_text(analysis::ChainTie {
                    link: self.project.model.links.len() - 1,
                    pred: old.succ,
                    succ: old.pred,
                });
                self.refresh_loops();
                self.loops.note = format!("Reversed to {text}. Ctrl+Z restores it.");
                self.status = self.loops.note.clone();
            }
            Err(e) => {
                model.links.insert(tie.link, old);
                let _ = model.rebuild_links();
                self.loops.note = format!("Not reversed: {e}");
            }
        }
    }

    fn go_to_loop_activity(&mut self) {
        let Some((_, tie)) = self.selected_chain_link() else {
            return;
        };
        let activity = if self.loops.go_succ {
            tie.succ
        } else {
            tie.pred
        };
        if self
            .loops
            .view
            .as_ref()
            .is_some_and(|v| !v.members.contains(&activity))
        {
            self.loops.view = None;
        }
        self.jump_to(activity);
    }

    pub(super) fn toggle_loop_view(&mut self) {
        if self.loops.view.take().is_some() {
            return;
        }
        let Some(group) = self.loops.report.groups.get(self.loops.group) else {
            return;
        };
        let mut view = LoopView::default();
        for tie in &group.chain {
            view.members.insert(tie.pred);
            view.members.insert(tie.succ);
            view.links.insert((tie.pred, tie.succ));
        }
        self.loops.view = Some(view);
    }

    /// Table rows when Show Loop is on: just the loop's activities.
    pub(crate) fn loop_view_rows(&self, rows: Vec<crate::grid::Row>) -> Vec<crate::grid::Row> {
        match &self.loops.view {
            Some(v) if !self.open_set.multi() => {
                let shown: BTreeSet<usize> = rows
                    .iter()
                    .filter_map(|r| match r {
                        crate::grid::Row::Activity(i) => Some(*i),
                        _ => None,
                    })
                    .collect();
                // Rows follow table order; an activity hidden by a collapsed
                // band or the filter is added at the end.
                let mut out: Vec<_> = rows
                    .into_iter()
                    .filter(|r| matches!(r, crate::grid::Row::Activity(i) if v.members.contains(i)))
                    .collect();
                out.extend(
                    v.members
                        .iter()
                        .filter(|i| !shown.contains(i))
                        .map(|&i| crate::grid::Row::Activity(i)),
                );
                out
            }
            _ => rows,
        }
    }

    /// One line per tie of the group's shortest loop, as the dialog's chain
    /// and the Check Schedule results show them.
    pub(crate) fn loop_chain_lines(&self, group: &LoopGroup) -> Vec<String> {
        let m = &self.project.model;
        group
            .chain
            .iter()
            .map(|t| {
                let l = &m.links[t.link];
                format!(
                    "{} {}  -{} {}->  {} {}",
                    m.activities[t.pred].task_code,
                    self.project.name(t.pred),
                    &l.link_type.as_str()[3..],
                    format::duration(l.lag, format::lag_periods(&self.project, t.pred, t.succ)),
                    m.activities[t.succ].task_code,
                    self.project.name(t.succ),
                )
            })
            .collect()
    }

    /// All loops as text lines: a heading per group, then its chain.
    pub(crate) fn loop_text(&self, report: &analysis::LoopReport) -> Vec<String> {
        let mut lines = vec![];
        for (n, g) in report.groups.iter().enumerate() {
            lines.push(format!(
                "Loop {}: {} {}, {} affected",
                n + 1,
                g.chain.len(),
                if g.chain.len() == 1 {
                    "activity"
                } else {
                    "activities"
                },
                g.members.len()
            ));
            lines.extend(
                self.loop_chain_lines(g)
                    .into_iter()
                    .map(|l| format!("    {l}")),
            );
            if let Some(s) = &g.suggestion {
                lines.push(format!("    {}", s.reason));
            }
        }
        lines
    }

    pub(crate) fn loops_dialogs(&mut self, ctx: &egui::Context) {
        if self.loops.open {
            if fingerprint(&self.project.model) != self.loops.fingerprint {
                self.refresh_loops();
                self.loops.note = "The relationships changed; the list was refreshed.".into();
            }
            self.loops_dialog(ctx);
        }
        if self.loops.prompt {
            self.loops_prompt(ctx);
        }
    }

    fn loops_prompt(&mut self, ctx: &egui::Context) {
        let report = &self.loops.report;
        let _ = report;
        let ((show, dismiss), closed, actions) = dialog_kit::window(
            ctx,
            "loops_prompt",
            "Circular Relationships",
            Vec2::new(430., 112.),
            |c| {
                c.fill(0., 0., 430., 112., dialog_kit::FACE);
                let galley = c.ui.painter().layout(
                    "This project has circular relationships, so it cannot be scheduled until \
                     they are fixed. Do you want to see where they are?"
                        .into(),
                    dialog_font(),
                    INK,
                    400.,
                );
                c.ui.painter()
                    .galley(c.origin + Vec2::new(14., 14.), galley, INK);
                let row = dialog_kit::button_row(
                    c,
                    416.,
                    76.,
                    &[
                        dialog_kit::Entry::Button("loops_prompt_show", "Show", true, false),
                        dialog_kit::Entry::Button("loops_prompt_dismiss", "Not Now", true, false),
                    ],
                );
                dialog_kit::mark_default(c, "loops_prompt_show");
                let clicked = |key: &str| row.clicked.as_deref() == Some(key);
                (
                    clicked("loops_prompt_show"),
                    clicked("loops_prompt_dismiss"),
                )
            },
        );
        self.record_dialog_actions(actions);
        if show {
            self.open_loops();
        } else if dismiss || closed {
            self.loops.prompt = false;
        }
    }

    fn loops_dialog(&mut self, ctx: &egui::Context) {
        let m = &self.project.model;
        let state = &self.loops;
        let groups: Vec<String> = state
            .report
            .groups
            .iter()
            .enumerate()
            .map(|(n, g)| {
                format!(
                    "Loop {}: {} {}, {} affected",
                    n + 1,
                    g.chain.len(),
                    if g.chain.len() == 1 {
                        "activity"
                    } else {
                        "activities"
                    },
                    g.members.len()
                )
            })
            .collect();
        let group = state.report.groups.get(state.group);
        let rows: Vec<[String; 6]> = group
            .map(|g| {
                g.chain
                    .iter()
                    .map(|t| {
                        let l = &m.links[t.link];
                        [
                            m.activities[t.pred].task_code.clone(),
                            self.project.name(t.pred).to_string(),
                            l.link_type.as_str()[3..].to_string(),
                            format::duration(
                                l.lag,
                                format::lag_periods(&self.project, t.pred, t.succ),
                            ),
                            m.activities[t.succ].task_code.clone(),
                            self.project.name(t.succ).to_string(),
                        ]
                    })
                    .collect()
            })
            .unwrap_or_default();
        let suggested = group.and_then(|g| g.suggested_row);
        let reason = group
            .and_then(|g| g.suggestion.as_ref())
            .map(|s| s.reason.clone());
        let suggestion_text = group.and_then(|g| {
            g.suggestion.as_ref().map(|s| {
                let l = &m.links[s.link];
                format!(
                    "Suggested: delete {} to {} ({})",
                    m.activities[l.pred].task_code,
                    m.activities[l.succ].task_code,
                    &l.link_type.as_str()[3..]
                )
            })
        });
        let reversible = self.selected_chain_link().is_some_and(|(_, t)| {
            let p = &m.activities[t.pred];
            matches!(p.task_type, TaskType::Loe | TaskType::Wbs | TaskType::Hamwk)
        });
        let none_left = state.report.is_empty();
        let plan = state.report.plan.len();
        let summary = if none_left {
            "No circular relationships remain.".to_string()
        } else {
            format!(
                "Removing the suggested tie in each loop, and checking again, clears every loop in {plan} change{}.",
                if plan == 1 { "" } else { "s" }
            )
        };
        let note = state.note.clone();
        let selected_group = state.group;
        let selected_row = state.row;
        let viewing = state.view.is_some();
        // First shown low on the screen, so the table and chart stay visible.
        let place = egui::Id::new(("dialog_window", "loops"));
        if ctx.data(|d| d.get_temp::<Pos2>(place)).is_none() {
            let screen = ctx.content_rect();
            let size = Vec2::new(WIDTH + 2., HEIGHT + 33.);
            let at = Pos2::new(
                (screen.center().x - size.x / 2.).max(0.),
                (screen.bottom() - size.y - 30.).max(0.),
            );
            ctx.data_mut(|d| d.insert_temp(place, at.round()));
        }
        let mut clicks = Clicks::default();
        let ((), closed, actions) = dialog_kit::window(
            ctx,
            "loops",
            "Circular Relationships",
            Vec2::new(WIDTH, HEIGHT),
            |c| {
                c.fill(0., 0., WIDTH, HEIGHT, dialog_kit::FACE);
                c.label(8., 6., 16., "Loops", true);
                let out = dialog_kit::list(
                    c,
                    "loops_groups",
                    (8., 24., 232., 250.),
                    None,
                    &[Col {
                        title: "Group",
                        width: 213.,
                    }],
                    groups.len(),
                    (!groups.is_empty()).then_some(selected_group),
                    None,
                    &mut |c, r, _, rect, sel| {
                        dialog_kit::cell_text(c, rect, &groups[r], sel, 3.);
                    },
                );
                clicks.group = out.clicked;
                c.label(248., 6., 16., "Shortest loop in this group", true);
                let cols: Vec<Col> = [
                    "Pred ID",
                    "Pred Name",
                    "Type",
                    "Lag",
                    "Succ ID",
                    "Succ Name",
                ]
                .iter()
                .zip(CHAIN_COLS)
                .map(|(title, width)| Col { title, width })
                .collect();
                let chain_left = c.origin.x + 249.;
                let succ_x = chain_left + CHAIN_COLS[..4].iter().sum::<f32>();
                let out = dialog_kit::list(
                    c,
                    "loops_chain",
                    (248., 24., 548., 188.),
                    None,
                    &cols,
                    rows.len(),
                    (!rows.is_empty()).then_some(selected_row),
                    None,
                    &mut |c, r, k, rect, sel| {
                        if suggested == Some(r) && !sel {
                            c.ui.painter().rect_filled(
                                rect.shrink2(Vec2::new(0., 0.5)),
                                0.,
                                SUGGESTED_ROW,
                            );
                        }
                        dialog_kit::cell_text(c, rect, &rows[r][k], sel, 3.);
                    },
                );
                if let Some(r) = out.clicked.or(out.double) {
                    let pos = c.ui.ctx().input(|i| i.pointer.interact_pos());
                    clicks.row = Some((r, pos.is_some_and(|p| p.x >= succ_x)));
                }
                if out.double.is_some() {
                    clicks.goto = true;
                }
                // The reason for the suggestion.
                c.lowered(248., 220., 548., 100.);
                c.fill(249., 221., 546., 98., Color32::WHITE);
                if let Some(head) = &suggestion_text {
                    c.bold(256., 225., 16., head);
                }
                if let Some(text) = reason.as_ref().or(none_left.then_some(&summary)) {
                    let galley =
                        c.ui.painter()
                            .layout(text.clone(), dialog_font(), INK, 532.);
                    let clip = c.r(249., 243., 546., 76.);
                    c.ui.painter().with_clip_rect(clip).galley(
                        c.origin + Vec2::new(256., 246.),
                        galley,
                        INK,
                    );
                }
                // The plan and the last action.
                c.lowered(8., 280., 232., 52.);
                let galley =
                    c.ui.painter()
                        .layout(summary.clone(), dialog_font(), INK, 220.);
                c.ui.painter()
                    .galley(c.origin + Vec2::new(13., 283.), galley, INK);
                if !note.is_empty() {
                    let galley =
                        c.ui.painter()
                            .layout(note.clone(), dialog_font(), INK, 780.);
                    c.ui.painter()
                        .galley(c.origin + Vec2::new(10., 342.), galley, INK);
                }
                if viewing {
                    c.label(
                        10.,
                        384.,
                        16.,
                        "Showing this loop only. Close restores the table.",
                        true,
                    );
                }
                // The standard column: Close first, the commands as groups.
                dialog_kit::strip(c, WIDTH, HEIGHT);
                let has_row = !rows.is_empty();
                use dialog_kit::Entry;
                let column = dialog_kit::button_column(
                    c,
                    WIDTH,
                    &[
                        Entry::close("loops_close"),
                        Entry::Button("loops_goto", "Go To", has_row, true),
                        Entry::Button("loops_delete", "Delete Relationship", has_row, true),
                        Entry::Button("loops_reverse", "Reverse Relationship", reversible, false),
                        Entry::Button(
                            "loops_show",
                            if viewing { "Show All" } else { "Show Loop" },
                            has_row,
                            true,
                        ),
                        Entry::Button("loops_recheck", "Re-check", true, false),
                        Entry::Button("loops_schedule", "Schedule (F9)", none_left, true),
                    ],
                );
                // Enter runs Schedule once no loop is left, else Go To.
                dialog_kit::mark_default(
                    c,
                    if none_left {
                        "loops_schedule"
                    } else if has_row {
                        "loops_goto"
                    } else {
                        "loops_recheck"
                    },
                );
                let clicked = |key: &str| column.clicked.as_deref() == Some(key);
                clicks.close = clicked("loops_close");
                clicks.goto |= clicked("loops_goto");
                clicks.delete = clicked("loops_delete");
                clicks.reverse = clicked("loops_reverse");
                clicks.show = clicked("loops_show");
                clicks.recheck = clicked("loops_recheck");
                clicks.schedule = clicked("loops_schedule");
            },
        );
        self.record_dialog_actions(actions);
        if let Some(g) = clicks.group {
            if g != self.loops.group {
                self.loops.view = None;
            }
            self.select_loop_group(g);
        }
        if let Some((r, succ)) = clicks.row {
            self.loops.row = r;
            self.loops.go_succ = succ;
        }
        if clicks.goto {
            self.go_to_loop_activity();
        }
        if clicks.delete {
            self.delete_loop_tie();
        }
        if clicks.reverse {
            self.reverse_loop_tie();
        }
        if clicks.show {
            self.toggle_loop_view();
        }
        if clicks.recheck {
            self.refresh_loops();
            self.loops.note = if self.loops.report.is_empty() {
                "Checked: no circular relationships.".into()
            } else {
                "Checked again.".into()
            };
        }
        if clicks.close || closed {
            self.loops.open = false;
            self.loops.view = None;
            self.loops.note.clear();
        } else if clicks.schedule && self.loops.report.is_empty() {
            self.loops.open = false;
            self.loops.view = None;
            self.loops.note.clear();
            self.open_schedule_dialog();
        }
    }
}
