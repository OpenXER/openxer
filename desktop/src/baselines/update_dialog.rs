//! Update Baseline (Maintain Baselines > Update...): the option pages
//! General, Activities, Activity and Resource Assignment, the filter
//! picker, Preview Changes, the message after an update with its log, and
//! the question whether to keep changed options. The update itself is
//! `openxer_automation::baseline_ops::update`, shared with the CLI and MCP.
use super::*;
use crate::dialog_kit::{self as kit, Canvas, Col, Entry};
use openxer_interchange::baseline_update::{ChangeKind, Preview};

/// The Update Baseline window: the options being edited and those saved.
pub(crate) struct UpdateDialog {
    pub id: i64,
    pub opts: UpdateOptions,
    pub saved: UpdateOptions,
    /// The page shown: 0 General, 1 Activities, 3 Activity, 4 Resource
    /// Assignment (2 is the Update Options heading, never shown).
    pub page: usize,
    /// Close asked whether to keep changed options.
    pub prompt: bool,
}

/// Preview Changes: the changes and the selected line of the tree.
pub(crate) struct PreviewView {
    pub preview: Preview,
    pub selected: Option<usize>,
}

/// The filter picker of the Activities page: the ticked filters (keys)
/// and whether an activity must pass all of them or any.
#[derive(Default)]
pub(crate) struct FilterPick {
    pub ticked: Vec<String>,
    pub match_all: bool,
    /// The highlighted row.
    pub selected: Option<usize>,
}

/// The category list: caption, indent, whether it opens a page.
const CATEGORIES: [(&str, f32, bool); 5] = [
    ("General", 0., true),
    ("Activities", 0., true),
    ("Update Options", 0., false),
    ("Activity", 12., true),
    ("Resource Assignment", 12., true),
];

/// The filters the Activities page offers: (key, caption).
pub(crate) fn update_filters(users: &[crate::grid::filters::UserFilter]) -> Vec<(String, String)> {
    use crate::grid::filters::Standard;
    let mut out: Vec<(String, String)> = Standard::LISTED
        .iter()
        .map(|s| (s.key(), s.caption().to_string()))
        .collect();
    out.extend(users.iter().map(|u| (u.key(), u.name.clone())));
    out
}

/// Every option on, without a log (tests).
#[cfg(test)]
pub(crate) fn all_update_options() -> UpdateOptions {
    UpdateOptions {
        project_udfs: true,
        risks_issues: true,
        risk_udfs: true,
        issue_udfs: true,
        documents: true,
        document_udfs: true,
        wbs_udfs: true,
        log_to_file: false,
        add_new: true,
        delete_missing: true,
        relationships: true,
        constraints: true,
        dates: true,
        risks: true,
        activity_udfs: true,
        step_udfs: true,
        expenses: true,
        expense_udfs: true,
        budget_no_assignments: true,
        actual_no_assignments: true,
        update_assignments: true,
        update_assignment_budget: true,
        update_assignment_actual: true,
        assignment_udfs: true,
        assignment_codes: true,
        ..UpdateOptions::default()
    }
}

/// The message after an update.
pub(crate) const UPDATE_DONE: &str = "The baseline has been updated.";
/// Close after the options changed.
pub(crate) const KEEP_OPTIONS: &str = "Keep the changed update options for the next time?";

/// One check box of a page: its action name, caption, the option, whether
/// it is indented under the one before and whether it can be changed.
type Check<'a> = (&'a str, &'a str, &'a mut bool, bool, bool);

/// Check boxes from client y `top`, one every `pitch`; indented ones sit
/// 18 px in.
fn checks(c: &mut Canvas, x: f32, top: f32, pitch: f32, w: f32, items: Vec<Check>) -> f32 {
    let mut y = top;
    for (key, label, value, indent, enabled) in items {
        let dx = if indent { 18. } else { 0. };
        c.checkbox(key, x + dx, y, w - dx, 18., label, value, enabled);
        y += pitch;
    }
    y
}

impl App {
    /// Maintain Baselines > Update...: the options as this user last kept
    /// them.
    pub(crate) fn open_update_baseline(&mut self, id: i64) {
        let saved = self.saved_update_options();
        self.baselines.update = Some(UpdateDialog {
            id,
            opts: saved.clone(),
            saved,
            page: 0,
            prompt: false,
        });
    }

    /// The names of the ticked filters, joined ("" when none), with "all"
    /// or "any" in front when there are several.
    fn filter_caption(&self, keys: &[String], match_all: bool) -> String {
        let filters = update_filters(&self.grid.user_filters);
        let names: Vec<String> = keys
            .iter()
            .map(|k| {
                filters
                    .iter()
                    .find(|f| f.0 == *k)
                    .map_or_else(|| k.clone(), |f| f.1.clone())
            })
            .collect();
        match names.len() {
            0 | 1 => names.join(""),
            _ if match_all => format!("All of: {}", names.join(", ")),
            _ => format!("Any of: {}", names.join(", ")),
        }
    }

    pub(crate) fn update_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.baselines.update.take() else {
            return;
        };
        let time = ctx.input(|i| i.time);
        let filter_text = self.filter_caption(&d.opts.filters, d.opts.filter_match_all);
        const W: f32 = 700.;
        const H: f32 = 470.;
        const PX: f32 = 170.;
        const PW: f32 = W - PX - 6.;
        let blocked = d.prompt
            || self.baselines.preview.is_some()
            || self.baselines.filter_pick.is_some()
            || self.baselines.update_done.is_some()
            || self.baselines.log_view.is_some();
        let mut pick_filter = false;
        let (clicked, closed, actions) = kit::window(
            ctx,
            "baselines_update",
            "Update Baseline",
            Vec2::new(W, H),
            |c| {
                // The category list.
                let (lx, ly, lw, lh) = (5., 5., 155., H - 46.);
                c.lowered(lx, ly, lw, lh);
                c.fill(lx + 1., ly + 1., lw - 2., lh - 2., Color32::WHITE);
                let body = c.r(lx + 1., ly + 1., lw - 2., lh - 2.);
                let page = d.page;
                let picked = kit::rows_in(
                    c,
                    "update_category",
                    body,
                    24.,
                    CATEGORIES.len(),
                    Some(page),
                    &mut |c, r, rect, _| {
                        let (label, indent, on) = CATEGORIES[r];
                        let sel = r == page;
                        if sel {
                            c.ui.painter().rect_filled(rect, 0., kit::SELECTED);
                        }
                        if on {
                            kit::cell_text(c, rect, label, sel, 6. + indent);
                        } else {
                            let pos = rect.left_top() + Vec2::new(6. + indent, 5.);
                            c.text_at(pos, label, false, kit::body_font());
                        }
                    },
                );
                if let Some(r) = picked.clicked.filter(|r| CATEGORIES[*r].2) {
                    d.page = r;
                }
                let o = &mut d.opts;
                match d.page {
                    0 => {
                        kit::heading(c, PX, 5., PW, "Project");
                        kit::text(c, PX, 34., "Project data to update:", true);
                        let (pd, ri, wp) = (o.project_details, o.risks_issues, o.documents);
                        let y = checks(
                            c,
                            PX,
                            56.,
                            24.,
                            PW,
                            vec![
                                (
                                    "update_opt_project_details",
                                    "Project details",
                                    &mut o.project_details,
                                    false,
                                    true,
                                ),
                                (
                                    "update_opt_project_udfs",
                                    "Project UDFs",
                                    &mut o.project_udfs,
                                    true,
                                    pd,
                                ),
                                (
                                    "update_opt_risks_issues",
                                    "Risks, issues, thresholds",
                                    &mut o.risks_issues,
                                    false,
                                    true,
                                ),
                                (
                                    "update_opt_risk_udfs",
                                    "Risk UDFs",
                                    &mut o.risk_udfs,
                                    true,
                                    ri,
                                ),
                                (
                                    "update_opt_issue_udfs",
                                    "Issue UDFs",
                                    &mut o.issue_udfs,
                                    true,
                                    ri,
                                ),
                                (
                                    "update_opt_documents",
                                    "Work products, documents",
                                    &mut o.documents,
                                    false,
                                    true,
                                ),
                                (
                                    "update_opt_document_udfs",
                                    "Document UDFs",
                                    &mut o.document_udfs,
                                    true,
                                    wp,
                                ),
                                (
                                    "update_opt_wbs_udfs",
                                    "WBS UDFs",
                                    &mut o.wbs_udfs,
                                    false,
                                    true,
                                ),
                            ],
                        );
                        kit::heading(c, PX, y + 10., PW, "Advanced");
                        // OpenXER always compares every record, so the
                        // greyed box is shown ticked.
                        let mut compare_all = true;
                        let y = checks(
                            c,
                            PX,
                            y + 40.,
                            24.,
                            PW,
                            vec![
                                (
                                    "update_opt_ignore_last_update",
                                    "Compare all records",
                                    &mut compare_all,
                                    false,
                                    false,
                                ),
                                (
                                    "update_opt_run_optimized",
                                    "Run optimized",
                                    &mut o.run_optimized,
                                    false,
                                    false,
                                ),
                                (
                                    "update_opt_log_to_file",
                                    "Log to file",
                                    &mut o.log_to_file,
                                    false,
                                    true,
                                ),
                            ],
                        );
                        let log = o.log_to_file;
                        c.edit(
                            "update_log_path",
                            PX + 18.,
                            y,
                            PW - 40.,
                            21.,
                            &mut o.log_path,
                            log,
                            Align::Min,
                        );
                        let frame = c.r(PX + 18., y, PW - 18., 21.);
                        if c.ellipsis("update_log_browse", frame, log).clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_file_name("UpdateBaselineLog.txt")
                                .save_file()
                            {
                                o.log_path = path.display().to_string();
                            }
                        }
                    }
                    1 => {
                        kit::heading(c, PX, 5., PW, "Activities");
                        kit::text(c, PX, 34., "Activities to update:", true);
                        if c.radio(
                            "update_all_activities",
                            PX,
                            54.,
                            PW,
                            18.,
                            "All Activities",
                            !o.use_filter,
                            true,
                        ) {
                            o.use_filter = false;
                        }
                        if c.radio(
                            "update_filter_activities",
                            PX,
                            78.,
                            PW,
                            18.,
                            "Only activities in filter:",
                            o.use_filter,
                            true,
                        ) {
                            o.use_filter = true;
                        }
                        kit::field(c, PX + 18., 102., PW - 40., 21., &filter_text, false);
                        let frame = c.r(PX + 18., 102., PW - 18., 21.);
                        if c.ellipsis("update_filter_pick", frame, o.use_filter)
                            .clicked()
                        {
                            pick_filter = true;
                        }
                        checks(
                            c,
                            PX,
                            138.,
                            24.,
                            PW,
                            vec![
                                (
                                    "update_opt_add_new",
                                    "Add activities missing from the baseline (no actuals)",
                                    &mut o.add_new,
                                    false,
                                    true,
                                ),
                                (
                                    "update_opt_delete_missing",
                                    "Remove baseline activities deleted from the project",
                                    &mut o.delete_missing,
                                    false,
                                    true,
                                ),
                                (
                                    "update_opt_update_existing",
                                    "Update matching activities",
                                    &mut o.update_existing,
                                    false,
                                    true,
                                ),
                            ],
                        );
                    }
                    3 => {
                        kit::heading(c, PX, 5., PW, "Activity");
                        kit::text(c, PX, 34., "Activity data to update:", true);
                        let (steps, expenses) = (o.steps, o.expenses);
                        let y = checks(
                            c,
                            PX,
                            54.,
                            22.,
                            PW,
                            vec![
                                (
                                    "update_opt_general",
                                    "General activity information",
                                    &mut o.general,
                                    false,
                                    true,
                                ),
                                (
                                    "update_opt_relationships",
                                    "Relationships",
                                    &mut o.relationships,
                                    false,
                                    true,
                                ),
                                (
                                    "update_opt_constraints",
                                    "Constraints",
                                    &mut o.constraints,
                                    false,
                                    true,
                                ),
                                (
                                    "update_opt_dates",
                                    "Dates, durations, data date",
                                    &mut o.dates,
                                    false,
                                    true,
                                ),
                                ("update_opt_wbs", "WBS assignments", &mut o.wbs, false, true),
                                (
                                    "update_opt_risks",
                                    "Risk assignments",
                                    &mut o.risks,
                                    false,
                                    true,
                                ),
                                (
                                    "update_opt_activity_udfs",
                                    "Activity UDFs",
                                    &mut o.activity_udfs,
                                    false,
                                    true,
                                ),
                                (
                                    "update_opt_codes",
                                    "Activity code assignments",
                                    &mut o.codes,
                                    false,
                                    true,
                                ),
                                (
                                    "update_opt_notebooks",
                                    "Activity notebooks",
                                    &mut o.notebooks,
                                    false,
                                    true,
                                ),
                                ("update_opt_steps", "Steps", &mut o.steps, false, true),
                                (
                                    "update_opt_step_udfs",
                                    "Step UDFs",
                                    &mut o.step_udfs,
                                    true,
                                    steps,
                                ),
                                (
                                    "update_opt_expenses",
                                    "Expenses",
                                    &mut o.expenses,
                                    false,
                                    true,
                                ),
                                (
                                    "update_opt_expense_udfs",
                                    "Expense UDFs",
                                    &mut o.expense_udfs,
                                    true,
                                    expenses,
                                ),
                            ],
                        );
                        kit::text(c, PX, y + 10., "Activities with no assignments", true);
                        checks(
                            c,
                            PX,
                            y + 32.,
                            22.,
                            PW,
                            vec![
                                (
                                    "update_opt_budget_no_assignments",
                                    "Budgeted units, cost",
                                    &mut o.budget_no_assignments,
                                    true,
                                    true,
                                ),
                                (
                                    "update_opt_actual_no_assignments",
                                    "Actual units, cost",
                                    &mut o.actual_no_assignments,
                                    true,
                                    true,
                                ),
                            ],
                        );
                    }
                    _ => {
                        kit::heading(c, PX, 5., PW, "Resource Assignment");
                        kit::text(c, PX, 34., "Assignment data to update:", true);
                        let (upd, add) = (o.update_assignments, o.add_assignments);
                        checks(
                            c,
                            PX,
                            54.,
                            24.,
                            PW,
                            vec![
                                (
                                    "update_opt_update_assignments",
                                    "Update matching assignments",
                                    &mut o.update_assignments,
                                    false,
                                    true,
                                ),
                                (
                                    "update_opt_update_assignment_budget",
                                    "Budgeted units, cost",
                                    &mut o.update_assignment_budget,
                                    true,
                                    upd,
                                ),
                                (
                                    "update_opt_update_assignment_actual",
                                    "Actual units, cost",
                                    &mut o.update_assignment_actual,
                                    true,
                                    upd,
                                ),
                                (
                                    "update_opt_add_assignments",
                                    "Add missing assignments",
                                    &mut o.add_assignments,
                                    false,
                                    true,
                                ),
                                (
                                    "update_opt_add_assignment_budget",
                                    "Budgeted units, cost",
                                    &mut o.add_assignment_budget,
                                    true,
                                    add,
                                ),
                                (
                                    "update_opt_add_assignment_actual",
                                    "Actual units, cost",
                                    &mut o.add_assignment_actual,
                                    true,
                                    add,
                                ),
                                (
                                    "update_opt_assignment_udfs",
                                    "Assignment UDFs",
                                    &mut o.assignment_udfs,
                                    false,
                                    true,
                                ),
                                (
                                    "update_opt_assignment_codes",
                                    "Assignment Codes",
                                    &mut o.assignment_codes,
                                    false,
                                    true,
                                ),
                            ],
                        );
                    }
                }
                c.etched(0., H - 37., W);
                kit::button_row(
                    c,
                    W - 6.,
                    H - 31.,
                    &[
                        Entry::Button("update_default", "Default", true, false),
                        Entry::Button("update_preview", "Preview", true, false),
                        Entry::Button("update_ok", "Update", true, false),
                        Entry::close("update_close"),
                        Entry::help("update_help"),
                    ],
                )
                .clicked
            },
        );
        self.record_dialog_actions(actions);
        if blocked {
            // A window above it answers first; this one waits.
            self.baselines.update = Some(d);
            return;
        }
        if pick_filter {
            let filters = update_filters(&self.grid.user_filters);
            self.baselines.filter_pick = Some(FilterPick {
                ticked: d.opts.filters.clone(),
                match_all: d.opts.filter_match_all,
                selected: filters.iter().position(|f| d.opts.filters.contains(&f.0)),
            });
        }
        match clicked.as_deref() {
            Some("update_help") => self.open_help_for("baselines_update"),
            Some("update_default") => d.opts = Self::default_update_options(),
            Some("update_preview") => match self.preview_baseline_update(d.id, &d.opts) {
                Ok(preview) => {
                    self.baselines.preview = Some(PreviewView {
                        preview,
                        selected: None,
                    })
                }
                Err(e) => self.baseline_failed(e),
            },
            Some("update_ok") => match self.update_baseline(d.id, &d.opts, time) {
                Ok(log) => self.baselines.update_done = Some(log),
                Err(e) => self.baseline_failed(e),
            },
            Some("update_close") => {
                if d.opts == d.saved {
                    return;
                }
                d.prompt = true;
            }
            _ if closed => {
                if d.opts == d.saved {
                    return;
                }
                d.prompt = true;
            }
            _ => {}
        }
        self.baselines.update = Some(d);
    }
}

/// A line of the Preview Changes tree.
#[derive(Clone, Copy)]
enum Line {
    /// Project, WBS or Activities.
    Group(usize),
    /// A change in a group.
    Item(usize, usize),
}

const GROUPS: [&str; 3] = ["Project", "WBS", "Activities"];

fn group(p: &Preview, g: usize) -> &[openxer_interchange::baseline_update::ObjectChange] {
    match g {
        0 => &p.project,
        1 => &p.wbs,
        _ => &p.activities,
    }
}

fn tree(p: &Preview) -> Vec<Line> {
    let mut out = vec![];
    for g in 0..GROUPS.len() {
        out.push(Line::Group(g));
        out.extend((0..group(p, g).len()).map(|k| Line::Item(g, k)));
    }
    out
}

fn kind_text(k: ChangeKind) -> &'static str {
    k.label()
}

impl App {
    /// The Activities page's filter picker: one saved or standard filter.
    pub(crate) fn filter_pick_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.baselines.filter_pick.take() else {
            return;
        };
        let filters = update_filters(&self.grid.user_filters);
        const W: f32 = 440.;
        const H: f32 = 380.;
        let (clicked, closed, actions) = kit::window(
            ctx,
            "update_filter_select",
            "Activity Filters",
            Vec2::new(W, H),
            |c| {
                kit::text(c, 8., 8., "Activities must pass:", true);
                if c.radio(
                    "update_filter_all",
                    16.,
                    26.,
                    260.,
                    18.,
                    "All selected filters",
                    d.match_all,
                    true,
                ) {
                    d.match_all = true;
                }
                if c.radio(
                    "update_filter_any",
                    16.,
                    48.,
                    260.,
                    18.,
                    "Any selected filters",
                    !d.match_all,
                    true,
                ) {
                    d.match_all = false;
                }
                let (lx, ly, lw, lh) = (5., 74., W - 122., H - 79.);
                let ticked = &d.ticked;
                let list = kit::list(
                    c,
                    "update_filter_list",
                    (lx, ly, lw, lh),
                    None,
                    &[Col {
                        title: "Filter",
                        width: lw - 19.,
                    }],
                    filters.len(),
                    d.selected,
                    None,
                    &mut |c, r, _, cell, sel| {
                        let local = cell.min - c.origin.to_vec2();
                        kit::check_glyph(
                            c,
                            local.x + 4.,
                            local.y + 2.,
                            ticked.contains(&filters[r].0),
                            true,
                        );
                        kit::cell_text(c, cell, &filters[r].1, sel, 22.);
                    },
                );
                if let Some(r) = list.clicked {
                    d.selected = Some(r);
                    let key = &filters[r].0;
                    match d.ticked.iter().position(|k| k == key) {
                        Some(at) => {
                            d.ticked.remove(at);
                        }
                        None => d.ticked.push(key.clone()),
                    }
                }
                kit::strip(c, W, H);
                kit::button_column(
                    c,
                    W,
                    &[
                        Entry::ok("update_filter_ok"),
                        Entry::cancel("update_filter_cancel"),
                        Entry::help("update_filter_help"),
                    ],
                )
                .clicked
            },
        );
        self.record_dialog_actions(actions);
        match clicked.as_deref() {
            Some("update_filter_help") => self.open_help_for("baselines_update"),
            Some("update_filter_ok") => {
                if let Some(u) = self.baselines.update.as_mut() {
                    // Kept in the list's order, so the box reads the same way.
                    let ticked: Vec<String> = filters
                        .iter()
                        .filter(|f| d.ticked.contains(&f.0))
                        .map(|f| f.0.clone())
                        .collect();
                    u.opts.use_filter = !ticked.is_empty();
                    u.opts.filters = ticked;
                    u.opts.filter_match_all = d.match_all;
                }
                return;
            }
            Some("update_filter_cancel") => return,
            _ => {}
        }
        if !closed {
            self.baselines.filter_pick = Some(d);
        }
    }
    /// Preview Changes: the tree of Project, WBS and Activities with each
    /// changed object and its change, and the selected one's fields.
    pub(crate) fn preview_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut v) = self.baselines.preview.take() else {
            return;
        };
        let lines = tree(&v.preview);
        const W: f32 = 900.;
        const H: f32 = 460.;
        let (clicked, closed, actions) = kit::window(
            ctx,
            "update_preview",
            "Preview Changes",
            Vec2::new(W, H),
            |c| {
                let (tx, ty, tw, th) = (5., 5., 330., H - 46.);
                let list = kit::list(
                    c,
                    "preview_tree",
                    (tx, ty, tw, th),
                    None,
                    &[
                        Col {
                            title: "Object",
                            width: 220.,
                        },
                        Col {
                            title: "Change Type",
                            width: tw - 2. - 220.,
                        },
                    ],
                    lines.len(),
                    v.selected,
                    None,
                    &mut |c, r, col, cell, sel| match (lines[r], col) {
                        (Line::Group(g), 0) => kit::cell_text(c, cell, GROUPS[g], sel, 3.),
                        (Line::Item(g, k), 0) => {
                            kit::cell_text(c, cell, &group(&v.preview, g)[k].object, sel, 20.)
                        }
                        (Line::Item(g, k), _) => kit::cell_text(
                            c,
                            cell,
                            kind_text(group(&v.preview, g)[k].kind),
                            sel,
                            3.,
                        ),
                        _ => {}
                    },
                );
                if let Some(r) = list.clicked {
                    v.selected = Some(r);
                }
                let fields = match v.selected.and_then(|r| lines.get(r)) {
                    Some(Line::Item(g, k)) => group(&v.preview, *g)[*k].fields.clone(),
                    _ => vec![],
                };
                let (fx, fw) = (tx + tw + 8., W - (tx + tw + 8.) - 5.);
                let third = ((fw - 19.) / 3.).floor();
                kit::list(
                    c,
                    "preview_fields",
                    (fx, ty, fw, th),
                    None,
                    &[
                        Col {
                            title: "Field",
                            width: third,
                        },
                        Col {
                            title: "Project Value",
                            width: third,
                        },
                        Col {
                            title: "Baseline Value",
                            width: third,
                        },
                    ],
                    fields.len(),
                    None,
                    None,
                    &mut |c, r, col, cell, sel| {
                        let f = &fields[r];
                        let text = match col {
                            0 => &f.field,
                            1 => &f.project,
                            _ => &f.baseline,
                        };
                        kit::cell_text(c, cell, text, sel, 3.);
                    },
                );
                c.etched(0., H - 37., W);
                kit::button_row(c, W - 6., H - 31., &[Entry::close("preview_close")]).clicked
            },
        );
        self.record_dialog_actions(actions);
        if clicked.as_deref() == Some("preview_close") || closed {
            return;
        }
        self.baselines.preview = Some(v);
    }

    /// Close (or Escape) after the options changed: keep them? Yes keeps
    /// them and closes, No closes, Cancel goes back to the options.
    pub(crate) fn save_prompt(&mut self, ctx: &egui::Context) {
        if !self.baselines.update.as_ref().is_some_and(|d| d.prompt) {
            return;
        }
        let mut rects = vec![];
        let clicked = kit::message_box(
            ctx,
            "update_keep",
            "Update Baseline",
            kit::MessageIcon::Question,
            KEEP_OPTIONS,
            &["Yes", "No", "Cancel"],
            kit::Question::SaveUpdateBaselineSettings.default_in(&["Yes", "No", "Cancel"]),
            &mut rects,
        );
        self.record_dialog_actions(rects);
        let escape = ctx.input(|i| i.key_pressed(Key::Escape));
        match clicked {
            Some(0) => {
                if let Some(d) = self.baselines.update.take() {
                    self.save_update_options(&d.opts);
                }
            }
            Some(1) => self.baselines.update = None,
            Some(_) => {
                if let Some(d) = self.baselines.update.as_mut() {
                    d.prompt = false;
                }
            }
            None if escape => {
                if let Some(d) = self.baselines.update.as_mut() {
                    d.prompt = false;
                }
            }
            None => {}
        }
    }

    /// The message after an update; View Log when a log was written.
    pub(crate) fn update_done_dialog(&mut self, ctx: &egui::Context) {
        let Some(log) = self.baselines.update_done.clone() else {
            return;
        };
        let buttons: &[&str] = if log.is_some() {
            &["View Log", "OK"]
        } else {
            &["OK"]
        };
        let mut rects = vec![];
        let clicked = kit::message_box(
            ctx,
            "update_done",
            "Update Baseline",
            kit::MessageIcon::Info,
            UPDATE_DONE,
            buttons,
            Some(buttons.len() - 1),
            &mut rects,
        );
        self.record_dialog_actions(rects);
        let escape = ctx.input(|i| i.key_pressed(Key::Escape));
        match clicked {
            Some(0) if log.is_some() => {
                let path = log.expect("a log");
                let text = std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| format!("The log could not be read: {e}"));
                self.baselines.log_view = Some((path, text));
                self.baselines.update_done = None;
            }
            Some(_) => self.baselines.update_done = None,
            None if escape => self.baselines.update_done = None,
            None => {}
        }
    }

    /// View Log: the update log, read only.
    pub(crate) fn log_view_dialog(&mut self, ctx: &egui::Context) {
        let Some((path, text)) = self.baselines.log_view.clone() else {
            return;
        };
        const W: f32 = 760.;
        const H: f32 = 480.;
        let title = format!("Update Baseline Log - {}", path.display());
        let (clicked, closed, actions) =
            kit::window(ctx, "update_log_view", &title, Vec2::new(W, H), |c| {
                // Typing changes a copy; the log stays as written.
                let mut shown = text.clone();
                kit::text_area(
                    c,
                    "update_log_text",
                    (5., 5., W - 10., H - 46.),
                    &mut shown,
                    kit::body_font(),
                    Color32::BLACK,
                );
                c.etched(0., H - 37., W);
                kit::button_row(c, W - 6., H - 31., &[Entry::close("update_log_close")]).clicked
            });
        self.record_dialog_actions(actions);
        if clicked.is_some() || closed {
            self.baselines.log_view = None;
        }
    }
}
