//! Progress and leveling tools from the Tools menu: Level Resources, Update
//! Progress, Apply Actuals, Store Period Performance and Recalculate
//! Assignment Costs.
//!
//! The rules that change project data are pure functions with unit tests.
//! The dialogs use the scheduling dialogs' chrome (this module is a child of
//! `schedule_dialogs`). Every action that changes the project is one undo
//! step holding the whole project, so document tables (SCHEDOPTIONS,
//! TASKRSRC, PROJECT) undo together with the scheduling model.
use super::*;
use openxer_xer::Document;

// ============================================================ leveling settings

/// Leveling priority fields the engine ranks by: stored key and caption.
const PRIORITY_FIELDS: [(&str, &str); 6] = [
    ("priority_type", "Activity Leveling Priority"),
    ("total_float_hr_cnt", "Total Float"),
    ("early_start_date", "Early Start"),
    ("late_start_date", "Late Start"),
    ("remain_drtn_hr_cnt", "Remaining Duration"),
    ("wbs_id", "WBS"),
];
/// Separator between items of the stored leveling priority list.
const ITEM_END: &str = "\u{7f}\u{7f}";

/// One row of the leveling priorities grid.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PriorityRow {
    pub key: String,
    pub ascending: bool,
    /// The stored item text, kept so an unchanged item is written back as read.
    raw: Option<String>,
}

fn parse_priority_item(item: &str) -> Option<(String, bool)> {
    let key = item.split(',').next()?.split('|').next()?.trim();
    (!key.is_empty()).then(|| (key.to_string(), !item.contains("DESC")))
}

impl PriorityRow {
    pub(crate) fn new(key: &str, ascending: bool) -> Self {
        Self {
            key: key.into(),
            ascending,
            raw: None,
        }
    }
    fn order(&self) -> &'static str {
        if self.ascending {
            "ASC"
        } else {
            "DESC"
        }
    }
    /// The stored item text for this row.
    fn text(&self) -> String {
        if let Some(raw) = &self.raw {
            match parse_priority_item(raw) {
                Some((key, ascending)) if key == self.key && ascending == self.ascending => {
                    return raw.clone();
                }
                Some((key, _)) if key == self.key => {
                    if let Some((head, _)) = raw.rsplit_once('/') {
                        return format!("{head}/{}", self.order());
                    }
                }
                _ => {}
            }
        }
        if self.key.starts_with("ACTVCODE_") {
            format!("{}|seq_num*,ASC_BY_FIELD/{}", self.key, self.order())
        } else if self.key == "priority_type" {
            format!("priority_type,ASC_BY_FIELD/{}", self.order())
        } else {
            format!("{},/{}", self.key, self.order())
        }
    }
}

impl crate::grid::sort_keys::SortRow for PriorityRow {
    fn field(&self) -> &str {
        &self.key
    }
    fn set_field(&mut self, key: &str) {
        self.key = key.to_string();
    }
    fn descending(&self) -> bool {
        !self.ascending
    }
    fn set_descending(&mut self, descending: bool) {
        self.ascending = !descending;
    }
}

/// Parse the stored leveling priority list.
pub(crate) fn parse_priority_list(text: &str) -> Vec<PriorityRow> {
    text.split('\u{7f}')
        .filter(|s| !s.is_empty())
        .filter_map(|item| {
            parse_priority_item(item).map(|(key, ascending)| PriorityRow {
                key,
                ascending,
                raw: Some(item.to_string()),
            })
        })
        .collect()
}

/// Serialize the leveling priority list; each item ends with two DEL marks.
pub(crate) fn priority_list_text(rows: &[PriorityRow]) -> String {
    rows.iter().map(|r| r.text() + ITEM_END).collect()
}

fn same_priorities(a: &[PriorityRow], b: &[PriorityRow]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x.key == y.key && x.ascending == y.ascending)
}

/// Activity code types usable as leveling priorities: stored key and name.
/// The key matches the one the leveling inputs resolve.
pub(crate) fn code_priority_fields(doc: &Document, pid: i64) -> Vec<(String, String)> {
    let Some(table) = doc.table("ACTVTYPE") else {
        return vec![];
    };
    table
        .rows
        .iter()
        .filter_map(|row| {
            let name = table.value(row, "actv_code_type");
            let project = table.value(row, "proj_id");
            if !project.is_empty() && records::num(project) != pid {
                return None;
            }
            let normalized = name
                .to_ascii_uppercase()
                .split(|c: char| !c.is_ascii_alphanumeric())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("_");
            let key = if project.is_empty() {
                format!("ACTVCODE_{normalized}")
            } else {
                format!("ACTVCODE_{normalized}_{project}")
            };
            Some((key, name.to_string()))
        })
        .collect()
}

/// Every priority field offered by the grid: key, caption, engine support.
fn priority_choices(doc: &Document, pid: i64) -> Vec<(String, String)> {
    PRIORITY_FIELDS
        .iter()
        .map(|(k, l)| (k.to_string(), l.to_string()))
        .chain(code_priority_fields(doc, pid))
        .collect()
}

fn schedoptions_row(doc: &Document, pid: i64) -> Option<usize> {
    let t = doc.table("SCHEDOPTIONS")?;
    t.rows
        .iter()
        .position(|r| records::num(t.value(r, "proj_id")) == pid)
}

/// A stored SCHEDOPTIONS value of the project, or `None` without a row.
fn sched_value(doc: &Document, pid: i64, key: &str) -> Option<String> {
    let i = schedoptions_row(doc, pid)?;
    Some(records::get(doc, "SCHEDOPTIONS", i, key))
}

/// Set a SCHEDOPTIONS value, creating the project's row with the values a new
/// row carries when the project has none.
fn set_sched_value(doc: &mut Document, pid: i64, key: &str, value: String) {
    let row = match schedoptions_row(doc, pid) {
        Some(i) => i,
        None => {
            let id = records::next_id(doc, "SCHEDOPTIONS", "schedoptions_id");
            let table = doc.table_mut("SCHEDOPTIONS");
            table.add(&[
                ("schedoptions_id", id.to_string()),
                ("proj_id", pid.to_string()),
                ("level_float_thrs_cnt", "0".into()),
                ("level_outer_assign_flag", "N".into()),
                ("level_outer_assign_priority", "5".into()),
                ("level_over_alloc_pct", "0".into()),
                ("level_within_float_flag", "N".into()),
                ("level_keep_sched_date_flag", "Y".into()),
                ("level_all_rsrc_flag", "Y".into()),
            ]);
            table.rows.len() - 1
        }
    };
    doc.table_mut("SCHEDOPTIONS").set(row, key, value);
}

fn flag(stored: Option<&str>, default: bool) -> bool {
    match stored {
        Some("Y") => true,
        Some("N") => false,
        _ => default,
    }
}

/// The Level Resources dialog's settings. Text fields are kept as typed and
/// validated when the settings are written.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LevelSettings {
    /// Automatically level resources when scheduling (the engine's `do_level`).
    pub auto: bool,
    pub outer_assign: bool,
    pub outer_priority: String,
    pub keep_dates: bool,
    pub within_float: bool,
    /// Preserve minimum float when leveling, as a duration.
    pub float_text: String,
    /// Max percent to over-allocate resources.
    pub over_alloc: String,
    pub all_resources: bool,
    /// The resources chosen for leveling, used when `all_resources` is off.
    pub selected: BTreeSet<i64>,
    /// Recalculate assignment costs after leveling (a workspace preference;
    /// XER files carry no column for it).
    pub recalc_costs: bool,
    pub priorities: Vec<PriorityRow>,
}

impl LevelSettings {
    /// Read the project's stored settings. `auto` is the scheduling option.
    pub(crate) fn read(p: &ProjectData, auto: bool) -> Self {
        let doc = &p.document;
        let pid = p.model.project.proj_id;
        let v = |k: &str| sched_value(doc, pid, k);
        let periods = format::project_periods(p);
        let hours = records::float(&v("level_float_thrs_cnt").unwrap_or_default());
        let number = |k: &str, default: &str| {
            let s = v(k).unwrap_or_default();
            if s.trim().is_empty() {
                default.to_string()
            } else {
                records::format_number(records::float(&s))
            }
        };
        let all_resources = flag(v("level_all_rsrc_flag").as_deref(), true);
        let stored_selection = selected_level_resources(p);
        Self {
            auto,
            outer_assign: flag(v("level_outer_assign_flag").as_deref(), false),
            outer_priority: number("level_outer_assign_priority", "5"),
            keep_dates: flag(v("level_keep_sched_date_flag").as_deref(), true),
            within_float: flag(v("level_within_float_flag").as_deref(), false),
            float_text: format::duration((hours * 60.).round() as i64, periods),
            over_alloc: number("level_over_alloc_pct", "0"),
            all_resources,
            selected: if all_resources && stored_selection.is_empty() {
                every_leveled_resource(p)
            } else {
                stored_selection
            },
            recalc_costs: false,
            priorities: parse_priority_list(&v("LevelPriorityList").unwrap_or_default()),
        }
    }

    /// The Default button: the values of a new project.
    pub(crate) fn defaults(auto: bool, p: &ProjectData) -> Self {
        Self {
            auto,
            outer_assign: false,
            outer_priority: "5".into(),
            keep_dates: true,
            within_float: false,
            float_text: format::duration(0, format::project_periods(p)),
            over_alloc: "0".into(),
            all_resources: true,
            selected: every_leveled_resource(p),
            recalc_costs: false,
            priorities: vec![PriorityRow::new("priority_type", true)],
        }
    }

    fn parsed(&self, p: &ProjectData) -> Result<(f64, f64, i64), String> {
        let periods = format::project_periods(p);
        let float = format::parse_duration(&self.float_text, periods).map_err(|_| {
            format!(
                "Enter the minimum float to preserve, for example {}.",
                format::duration(0, periods)
            )
        })?;
        if float < 0 {
            return Err("The minimum float to preserve cannot be negative.".into());
        }
        let over = self
            .over_alloc
            .trim()
            .trim_end_matches('%')
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && (0.0..=1000.).contains(v))
            .ok_or("Enter an over-allocation percent from 0 to 1000.")?;
        let priority = self
            .outer_priority
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|v| (1..=5).contains(v))
            .ok_or("Enter a project priority from 1 to 5.")?;
        Ok((float as f64 / 60., over, priority))
    }

    /// Validate, then write the changed values into the project's SCHEDOPTIONS
    /// row. Unchanged values keep their stored text. Returns whether anything
    /// was written.
    pub(crate) fn write(&self, p: &mut ProjectData) -> Result<bool, String> {
        let (float_hours, over, priority) = self.parsed(p)?;
        let stored = Self::read(p, self.auto);
        let (stored_float, stored_over, stored_priority) =
            stored.parsed(p).unwrap_or((f64::NAN, f64::NAN, -1));
        let pid = p.model.project.proj_id;
        let raw_selection = selected_level_resources(p);
        let doc = &mut p.document;
        let mut changed = false;
        let yn = |b: bool| if b { "Y" } else { "N" }.to_string();
        for (key, new, old) in [
            (
                "level_outer_assign_flag",
                self.outer_assign,
                stored.outer_assign,
            ),
            (
                "level_keep_sched_date_flag",
                self.keep_dates,
                stored.keep_dates,
            ),
            (
                "level_within_float_flag",
                self.within_float,
                stored.within_float,
            ),
            (
                "level_all_rsrc_flag",
                self.all_resources,
                stored.all_resources,
            ),
        ] {
            if new != old {
                set_sched_value(doc, pid, key, yn(new));
                changed = true;
            }
        }
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        if !close(float_hours, stored_float) {
            set_sched_value(
                doc,
                pid,
                "level_float_thrs_cnt",
                records::format_number(float_hours),
            );
            changed = true;
        }
        if !close(over, stored_over) {
            set_sched_value(
                doc,
                pid,
                "level_over_alloc_pct",
                records::format_number(over),
            );
            changed = true;
        }
        if priority != stored_priority {
            set_sched_value(
                doc,
                pid,
                "level_outer_assign_priority",
                priority.to_string(),
            );
            changed = true;
        }
        // The selection list matters only while not every resource is leveled;
        // with "Level all resources" on, the stored list is left as it is.
        if !self.all_resources && self.selected != raw_selection {
            write_level_list(doc, pid, &self.selected);
            changed = true;
        }
        if !same_priorities(&self.priorities, &stored.priorities) {
            set_sched_value(
                doc,
                pid,
                "LevelPriorityList",
                priority_list_text(&self.priorities),
            );
            changed = true;
        }
        Ok(changed)
    }

    /// Chosen values the leveling engine does not implement.
    pub(crate) fn engine_gaps(&self, choices: &[(String, String)]) -> Vec<String> {
        let mut gaps = vec![];
        if self.outer_assign {
            gaps.push("Consider assignments in other projects".to_string());
        }
        if !self.keep_dates {
            gaps.push("Replace scheduled early and late dates".into());
        }
        for row in &self.priorities {
            if !choices.iter().any(|(k, _)| *k == row.key) {
                gaps.push(format!("Leveling priority {}", row.key));
            }
        }
        gaps
    }
}

/// Replace the project's selection list (RSRCLEVELLIST) with `chosen`: rows
/// of other resources go, missing ones are added, kept rows stay as they are.
fn write_level_list(doc: &mut Document, pid: i64, chosen: &BTreeSet<i64>) {
    let Some(row) = schedoptions_row(doc, pid) else {
        return;
    };
    let options = records::get(doc, "SCHEDOPTIONS", row, "schedoptions_id");
    let mut present = BTreeSet::new();
    let drop: Vec<usize> = doc
        .table("RSRCLEVELLIST")
        .map(|t| {
            t.rows
                .iter()
                .enumerate()
                .filter(|(_, r)| t.value(r, "schedoptions_id") == options)
                .filter_map(|(i, r)| {
                    let id = records::num(t.value(r, "rsrc_id"));
                    if chosen.contains(&id) && present.insert(id) {
                        None
                    } else {
                        Some(i)
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    for i in drop.into_iter().rev() {
        records::remove_row(doc, "RSRCLEVELLIST", i);
    }
    for &id in chosen.difference(&present) {
        let key = records::next_id(doc, "RSRCLEVELLIST", "rsrc_level_list_id");
        records::add_row(
            doc,
            "RSRCLEVELLIST",
            &[
                ("rsrc_level_list_id", key.to_string()),
                ("schedoptions_id", options.clone()),
                ("rsrc_id", id.to_string()),
            ],
        );
    }
}

/// Every resource assigned in the project.
fn every_leveled_resource(p: &ProjectData) -> BTreeSet<i64> {
    leveled_resources(p)
        .into_iter()
        .map(|(id, _, _)| id)
        .collect()
}

/// Resources the leveler places: those assigned in the project, sorted by ID.
fn leveled_resources(p: &ProjectData) -> Vec<(i64, String, String)> {
    let ids: BTreeSet<i64> = p
        .resources
        .demands
        .values()
        .flatten()
        .map(|d| d.resource_id)
        .collect();
    let mut out: Vec<_> = ids
        .into_iter()
        .map(|id| {
            let short = records::lookup(&p.document, "RSRC", "rsrc_id", id, "rsrc_short_name");
            let name = records::lookup(&p.document, "RSRC", "rsrc_id", id, "rsrc_name");
            (id, short, name)
        })
        .collect();
    out.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    out
}

/// Resources named in the project's stored selection list (RSRCLEVELLIST).
fn selected_level_resources(p: &ProjectData) -> BTreeSet<i64> {
    let doc = &p.document;
    let pid = p.model.project.proj_id;
    let Some(row) = schedoptions_row(doc, pid) else {
        return BTreeSet::new();
    };
    let id = records::get(doc, "SCHEDOPTIONS", row, "schedoptions_id");
    doc.table("RSRCLEVELLIST")
        .map(|t| {
            t.rows
                .iter()
                .filter(|r| t.value(r, "schedoptions_id") == id)
                .map(|r| records::num(t.value(r, "rsrc_id")))
                .collect()
        })
        .unwrap_or_default()
}

/// Activities the leveler moved later than their scheduled early start.
pub(crate) fn delayed_by_leveling(m: &Model) -> Vec<usize> {
    m.activities
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            !a.is_completed()
                && a.act_start.is_none()
                && matches!((a.restart, a.early_start), (Some(r), Some(e)) if r > e)
        })
        .map(|(i, _)| i)
        .collect()
}

/// Add the leveling settings and delayed activities to a schedule log.
pub(crate) fn leveling_log(
    mut log: Vec<String>,
    settings: &LevelSettings,
    p: &ProjectData,
    left_over: &[i64],
) -> Vec<String> {
    let choices = priority_choices(&p.document, p.model.project.proj_id);
    let mut block = vec![
        "\tLeveling".to_string(),
        "\t--------".to_string(),
        leader(
            "Consider assignments in other projects ",
            yes_no(settings.outer_assign),
        ),
    ];
    if settings.outer_assign {
        block.push(leader(
            "with priority equal/higher than ",
            &settings.outer_priority,
        ));
    }
    block.push(leader(
        "Preserve scheduled early and late dates ",
        yes_no(settings.keep_dates),
    ));
    block.push(leader(
        "Level resources only within activity Total Float ",
        yes_no(settings.within_float),
    ));
    block.push(leader(
        "Preserve minimum float when leveling ",
        &settings.float_text,
    ));
    block.push(leader(
        "Max percent to over-allocate resources ",
        format!("{}%", settings.over_alloc.trim().trim_end_matches('%')),
    ));
    block.push(leader(
        "Level all resources ",
        yes_no(settings.all_resources),
    ));
    block.push("\tLevel Priority".into());
    for row in &settings.priorities {
        let label = choices
            .iter()
            .find(|(k, _)| *k == row.key)
            .map_or(row.key.as_str(), |(_, l)| l.as_str());
        block.push(format!(
            "\t\t{label}\t{}",
            if row.ascending {
                "Ascending"
            } else {
                "Descending"
            }
        ));
    }
    block.push("\tResources to be leveled".into());
    for (_, short, name) in leveled_resources(p)
        .into_iter()
        .filter(|(id, _, _)| settings.all_resources || settings.selected.contains(id))
    {
        block.push(format!("\t\t{short}\t{name}"));
    }
    block.push(String::new());
    let at = log
        .iter()
        .position(|l| l == "Statistics:")
        .unwrap_or(log.len());
    log.splice(at..at, block);
    let m = &p.model;
    let delayed = delayed_by_leveling(m);
    log.push(leader(
        "Activities delayed due to resource leveling",
        delayed.len(),
    ));
    for i in delayed {
        log.push(format!(
            "\t\tProject:\t{}\tActivity:\t{}\t{}",
            m.project.name,
            m.activities[i].task_code,
            p.name(i)
        ));
    }
    if settings.within_float {
        let kept: Vec<usize> = left_over
            .iter()
            .filter_map(|id| m.activities.iter().position(|a| a.task_id == *id))
            .collect();
        log.push(leader(
            "Activities left over-allocated, their float does not allow a delay",
            kept.len(),
        ));
        for i in kept {
            log.push(format!(
                "\t\tProject:\t{}\tActivity:\t{}\t{}",
                m.project.name,
                m.activities[i].task_code,
                p.name(i)
            ));
        }
    }
    log.push(String::new());
    log
}

// ============================================================ progress rules

pub(crate) use openxer_xer::edit::progress::{
    apply_actuals, apply_progress, recalculate_assignment_costs, ProgressScope, RecalcReport,
    RemainingRule,
};
#[cfg(test)]
pub(crate) use openxer_xer::edit::progress::{
    auto_progress, DurationKind, PercentKind, ProgressInput,
};

// ============================================================ dialog state

/// Height of a group of two radios under a section heading in the
/// progress dialogs.
const GROUP_H: f32 = 76.;
/// Width of the progress dialogs' date edits.
const DATE_W: f32 = 140.;
/// The right-hand button strip.
const STRIP_W: f32 = 112.;
/// Update Progress: width of the panel left of the button strip.
const UPDATE_PANEL: f32 = 366.;
/// The remaining duration choices, in the order the dialogs list them.
const REMAINING_RULES: [RemainingRule; 2] = [RemainingRule::DurationType, RemainingRule::Always];

/// Draft of the Update Progress and Apply Actuals dialogs.
#[derive(Clone, Debug, Default)]
pub(crate) struct ProgressDraft {
    pub new_date: String,
    /// Update Progress: only the selected activities.
    pub selected_only: bool,
    /// Apply Actuals: each project uses its own new data date.
    pub own_date: bool,
    /// Apply Actuals with several projects open: the new data date of each
    /// other open project (in open order, after the active one, whose own
    /// date is `new_date`), used when each project has its own date.
    pub others: Vec<String>,
    pub rule: RemainingRule,
    /// Apply Actuals: the project row whose New Data Date cell is being
    /// edited; the other cells show their date as plain text.
    pub editing_row: Option<usize>,
}

#[derive(Clone, Debug)]
pub(crate) struct RecalcDraft {
    pub active_only: bool,
    pub sync_overtime: bool,
}

#[derive(Default)]
pub(crate) struct ProgressTools {
    /// Level Resources draft while the dialog is open.
    pub level: Option<LevelSettings>,
    pub level_row: usize,
    pub select_resources: bool,
    /// The Select Resources checks while that dialog is open.
    pub select_draft: BTreeSet<i64>,
    /// Activities the last leveling run left over-allocated because their
    /// float would not allow a delay, by activity ID.
    pub left_over: Vec<i64>,
    /// Leveled dates are shown after a Level run while automatic leveling is
    /// off; the next schedule run drops them, as the reference does.
    pub manual_level: bool,
    pub update: Option<ProgressDraft>,
    pub actuals: Option<ProgressDraft>,
    pub store_period: bool,
    pub recalc: Option<RecalcDraft>,
    /// Apply Actuals waits for the answer to the future data date question.
    pub confirm_future: Option<String>,
    /// The question was answered Yes: the next run does not ask again.
    pub future_ok: bool,
    /// The remaining duration choice of the last Update Progress and Apply
    /// Actuals run: the reference opens each dialog with it.
    pub update_rule: RemainingRule,
    pub actuals_rule: RemainingRule,
    /// The notice a finished run shows (title, text), OK to close.
    pub done: Option<(&'static str, &'static str)>,
}

const COMBOS: [(&str, &str); 2] = [("level", "level_field"), ("level", "level_order")];

// ============================================================ App hooks

impl App {
    /// Tools menu entries for this module.
    pub(crate) fn progress_tools_menu(&mut self, ui: &mut egui::Ui) {
        use chrome::{Icon, Item};
        for (key, item) in [
            (
                "menu_level_resources",
                Item::new("&Level Resources...")
                    .key("Shift+F9")
                    .icon(Icon::Level),
            ),
            (
                "menu_apply_actuals",
                Item::new("&Apply Actuals...").icon(Icon::ApplyActuals),
            ),
            (
                "menu_update_progress",
                Item::new("&Update Progress...").icon(Icon::UpdateProgress),
            ),
            (
                "menu_recalc_costs",
                Item::new("Recalculate Assignment Costs...").icon(Icon::RecalcCosts),
            ),
            (
                "menu_store_period",
                Item::new("Save Peri&od Actuals...").icon(Icon::StorePeriod),
            ),
        ] {
            if self.item(ui, key, item) {
                match key {
                    "menu_level_resources" => self.open_level_resources(),
                    "menu_apply_actuals" => self.open_apply_actuals(),
                    "menu_update_progress" => self.open_update_progress(),
                    "menu_store_period" => self.sched.progress.store_period = true,
                    _ => self.open_recalc_costs(),
                }
            }
        }
    }

    /// Before every schedule run: a manual Level's dates are dropped.
    pub(crate) fn level_hook_before_schedule(&mut self) {
        if std::mem::take(&mut self.sched.progress.manual_level) {
            self.project.model.options.do_level = false;
        }
    }
    /// Schedule Options shows the automatic leveling setting, not the
    /// leveled display of a manual Level.
    pub(crate) fn level_hook_options_opened(&mut self) {
        if self.sched.progress.manual_level {
            if let Some(draft) = self.draft_options.as_mut() {
                draft.do_level = false;
            }
        }
    }
    pub(crate) fn level_hook_options_applied(&mut self) {
        self.sched.progress.manual_level = false;
    }

    fn auto_level(&self) -> bool {
        self.project.model.options.do_level && !self.sched.progress.manual_level
    }
    /// Set automatic leveling. Turning it off keeps leveled dates on screen
    /// until the next schedule run.
    fn set_auto_level(&mut self, auto: bool) {
        if auto {
            self.project.model.options.do_level = true;
            self.sched.progress.manual_level = false;
        } else if self.project.model.options.do_level {
            self.sched.progress.manual_level = true;
        }
    }

    fn bulk_step(&mut self, before: ProjectData, before_revision: u64, time: f64) {
        self.records_serial += 1;
        self.records_revision = self.records_serial;
        self.history.record_bulk(
            &before,
            before_revision,
            &self.project,
            self.records_revision,
            time,
        );
        self.dirty = true;
    }

    /// Run `edit` on every open project, the active one first, as one undo
    /// step across the set (one entry of the open-set journal when several
    /// are open). `changed` says whether a result changed its project; when
    /// none did, nothing is recorded and the other projects stay as they
    /// were. Returns each project's result.
    pub(crate) fn edit_every_open_project<R>(
        &mut self,
        time: f64,
        mut edit: impl FnMut(&mut ProjectData) -> R,
        changed: impl Fn(&R) -> bool,
    ) -> Vec<R> {
        let before = self.project.clone();
        let revision = self.records_revision;
        let mut results = vec![edit(&mut self.project)];
        let mut others = vec![];
        for parked in &self.open_set.parked {
            let mut next = parked.project.clone();
            results.push(edit(&mut next));
            others.push(next);
        }
        if !results.iter().any(&changed) {
            return results;
        }
        for (parked, project) in self.open_set.parked.iter_mut().zip(others) {
            let previous = std::mem::replace(&mut parked.project, project);
            let previous_revision = parked.records_revision;
            self.records_serial += 1;
            parked.records_revision = self.records_serial;
            parked.history.record_bulk(
                &previous,
                previous_revision,
                &parked.project,
                parked.records_revision,
                time,
            );
            parked.dirty = true;
        }
        self.bulk_step(before, revision, time);
        if self.open_set.multi() {
            self.open_set.steps_seen = self.history.steps();
            self.open_set.undo.push(self.open_ids());
            self.open_set.redo.clear();
            self.open_set.version += 1;
        }
        results
    }

    pub(crate) fn top_progress_dialog(&self) -> bool {
        let p = &self.sched.progress;
        p.level.is_some()
            || p.update.is_some()
            || p.actuals.is_some()
            || p.store_period
            || p.recalc.is_some()
            || p.confirm_future.is_some()
            || p.done.is_some()
    }

    // ---------------------------------------------------------- Level

    pub(crate) fn open_level_resources(&mut self) {
        let mut settings = LevelSettings::read(&self.project, self.auto_level());
        settings.recalc_costs = self.recalc_costs_after_leveling();
        self.sched.progress.level = Some(settings);
        self.sched.progress.level_row = 0;
        self.sched.progress.select_resources = false;
    }

    /// Keep the Recalculate assignment costs after leveling check box.
    fn save_level_recalc(&mut self, on: bool) {
        if on == self.recalc_costs_after_leveling() {
            return;
        }
        let text = if on { "Y" } else { "N" };
        if let Err(e) = self
            .store
            .set_preference(super::LEVEL_RECALC_COSTS_KEY, text)
        {
            self.error(format!("Cannot save the leveling option: {e}"));
        }
    }

    /// Close: keep the settings (one undo step when they changed).
    fn close_level(&mut self, time: f64) {
        let Some(settings) = self.sched.progress.level.clone() else {
            return;
        };
        let before = self.project.clone();
        let revision = self.records_revision;
        match settings.write(&mut self.project) {
            Ok(changed) => {
                let auto_changed = settings.auto != self.auto_level();
                if auto_changed {
                    self.set_auto_level(settings.auto);
                }
                if changed || auto_changed {
                    self.bulk_step(before, revision, time);
                    self.save();
                }
                self.save_level_recalc(settings.recalc_costs);
                self.sched.progress.level = None;
            }
            Err(e) => self.error(e),
        }
    }

    /// Validate leveling settings on a cloned copy of the active project
    /// without mutating `App.project`, for the background leveling worker.
    pub(crate) fn prepare_level_run(&mut self) -> Option<(LevelSettings, ProjectData)> {
        let settings = self.sched.progress.level.clone()?;
        let choices = priority_choices(&self.project.document, self.project.model.project.proj_id);
        let gaps = settings.engine_gaps(&choices);
        if !gaps.is_empty() {
            let msg = format!(
                "Leveling canceled; the leveling engine does not support: {}.",
                gaps.join("; ")
            );
            self.diag_log().error("level", &msg);
            self.error(msg);
            return None;
        }
        let mut next = self.project.clone();
        if let Err(e) = settings.write(&mut next) {
            self.diag_log().error("level", &e);
            self.error(e);
            return None;
        }
        next.model.options.do_level = true;
        Some((settings, next))
    }

    /// Post-commit steps after a background leveling run has applied its
    /// scheduled result to `self.project`.
    pub(crate) fn finish_level_run(
        &mut self,
        settings: LevelSettings,
        before: ProjectData,
        revision: u64,
        time: f64,
    ) {
        self.save_level_recalc(settings.recalc_costs);
        self.sched.progress.manual_level = !settings.auto;
        self.log = leveling_log(
            std::mem::take(&mut self.log),
            &settings,
            &self.project,
            &self.sched.progress.left_over,
        );
        if self.sched.log_to_file {
            if let Err(error) = write_log(
                Path::new(&self.sched.log_path),
                &self.log,
                self.sched.log_html,
                self.sched.log_append,
            ) {
                self.status = format!("Fail to create log file: {error}  Log disabled.");
            }
        }
        self.bulk_step(before, revision, time);
        let delayed = delayed_by_leveling(&self.project.model).len();
        let kept = self.sched.progress.left_over.len();
        self.status = if kept > 0 {
            format!("Leveling complete · {delayed} activities delayed · {kept} left over-allocated")
        } else {
            format!("Leveling complete · {delayed} activities delayed")
        };
        self.sched.progress.level = None;
    }

    /// Level: keep the settings, then schedule with leveling through the same
    /// path as F9 with leveling on. A failure rolls everything back.
    #[cfg(test)]
    pub(crate) fn run_level(&mut self, time: f64) {
        let Some(settings) = self.sched.progress.level.clone() else {
            return;
        };
        let choices = priority_choices(&self.project.document, self.project.model.project.proj_id);
        let gaps = settings.engine_gaps(&choices);
        if !gaps.is_empty() {
            self.error(format!(
                "Leveling canceled; the leveling engine does not support: {}.",
                gaps.join("; ")
            ));
            return;
        }
        let before = self.project.clone();
        let revision = self.records_revision;
        let manual = self.sched.progress.manual_level;
        if let Err(e) = settings.write(&mut self.project) {
            self.error(e);
            return;
        }
        self.project.model.options.do_level = true;
        self.sched.progress.manual_level = false;
        let recalc_before = self.recalc_costs_after_leveling();
        self.save_level_recalc(settings.recalc_costs);
        let runs = self.schedule_runs;
        self.schedule();
        if self.schedule_runs == runs {
            let reason = self.error.clone().unwrap_or_default();
            self.project = before;
            self.save_level_recalc(recalc_before);
            self.sched.progress.manual_level = manual;
            if self.loops.open {
                // The Circular Relationships dialog explains why.
                self.status = "Leveling canceled; no changes were made. Fix the circular relationships first.".into();
                return;
            }
            self.error(format!("Leveling canceled; no changes were made. {reason}"));
            return;
        }
        self.sched.progress.manual_level = !settings.auto;
        self.log = leveling_log(
            std::mem::take(&mut self.log),
            &settings,
            &self.project,
            &self.sched.progress.left_over,
        );
        if self.sched.log_to_file {
            if let Err(error) = write_log(
                Path::new(&self.sched.log_path),
                &self.log,
                self.sched.log_html,
                self.sched.log_append,
            ) {
                self.status = format!("Fail to create log file: {error}  Log disabled.");
            }
        }
        self.bulk_step(before, revision, time);
        let delayed = delayed_by_leveling(&self.project.model).len();
        let kept = self.sched.progress.left_over.len();
        self.status = if kept > 0 {
            format!("Leveling complete · {delayed} activities delayed · {kept} left over-allocated")
        } else {
            format!("Leveling complete · {delayed} activities delayed")
        };
        self.sched.progress.level = None;
    }

    // ---------------------------------------------------------- progress

    pub(crate) fn open_update_progress(&mut self) {
        // The reference proposes the current data date, with the Progress
        // Spotlight on as well, and the last remaining duration choice. A
        // spotlight whose end was dragged proposes that end.
        let dragged = self.layout_opts.spotlight_end.and(self.spotlight_window());
        let m = &self.project.model;
        let proposed = dragged.map(|(_, end)| end).or(m.project.data_date);
        let p = &mut self.sched.progress;
        p.done = None;
        p.update = Some(ProgressDraft {
            new_date: display_date(m.origin_serial, proposed),
            rule: p.update_rule,
            ..ProgressDraft::default()
        });
    }

    pub(crate) fn open_apply_actuals(&mut self) {
        // The reference proposes each project's current data date, each
        // project its own date. The legacy defaults of tests
        // (`crate::defaults`) propose today at midnight for every project.
        let origin = self.project.model.origin_serial;
        let (new_date, others) = if crate::defaults::reference() {
            let data_date =
                |p: &ProjectData| display_date(p.model.origin_serial, p.model.project.data_date);
            let others = self
                .open_set
                .parked
                .iter()
                .map(|p| data_date(&p.project))
                .collect();
            (data_date(&self.project), others)
        } else {
            let today = chrono::Local::now().date_naive();
            let days = (today - epoch().date()).num_days() - origin;
            let today_text = display_date(origin, Some(days * DAY));
            let others = vec![today_text.clone(); self.open_set.parked.len()];
            (today_text, others)
        };
        let p = &mut self.sched.progress;
        p.done = None;
        p.actuals = Some(ProgressDraft {
            new_date,
            own_date: true,
            others,
            rule: p.actuals_rule,
            ..ProgressDraft::default()
        });
    }

    /// The spotlighted activities of open project `slot` (0 is the active
    /// one), as indices into that project.
    fn spotlit_in(&self, slot: usize) -> BTreeSet<usize> {
        match &self.open_set.composite {
            None => self.spotlight_active(),
            Some(c) => self
                .spotlight_activities()
                .into_iter()
                .filter_map(|i| c.origin.get(i).copied())
                .filter(|&(s, _)| s == slot)
                .map(|(_, local)| local)
                .collect(),
        }
    }

    /// Minutes since 1899-12-30 of the current local time.
    fn now_minutes() -> i64 {
        (chrono::Local::now().naive_local() - epoch()).num_minutes()
    }

    /// Apply of Update Progress (`actuals` false) or Apply Actuals, to every
    /// open project as one undo step: nothing changes unless every project
    /// accepts its date.
    pub(crate) fn run_progress(&mut self, actuals: bool, time: f64) {
        let draft = if actuals {
            self.sched.progress.actuals.clone()
        } else {
            self.sched.progress.update.clone()
        };
        let Some(draft) = draft else {
            return;
        };
        let origins: Vec<i64> = self
            .open_projects()
            .iter()
            .map(|p| p.model.origin_serial)
            .collect();
        let names: Vec<String> = self
            .open_projects()
            .iter()
            .map(|p| p.model.project.name.clone())
            .collect();
        // The new data date of each open project, in minutes from its origin.
        let mut dates = vec![];
        for (slot, origin) in origins.iter().enumerate() {
            let text = if actuals && draft.own_date && slot > 0 {
                draft.others.get(slot - 1).unwrap_or(&draft.new_date)
            } else {
                &draft.new_date
            };
            let absolute = if actuals && draft.own_date {
                parse_display_date(*origin, text).map(|d| d + origin * DAY)
            } else {
                parse_display_date(origins[0], text).map(|d| d + origins[0] * DAY)
            };
            match absolute {
                Ok(d) => dates.push(d - origin * DAY),
                Err(_) => {
                    self.error("Please specify a new Data Date.".into());
                    return;
                }
            }
        }
        // A data date after today is probably a typing slip: ask first.
        if actuals && !self.sched.progress.future_ok {
            let now = Self::now_minutes();
            let future = dates
                .iter()
                .zip(&origins)
                .find(|(d, o)| **d + **o * DAY > now);
            if let Some((d, o)) = future {
                self.sched.progress.confirm_future = Some(display_date(*o, Some(*d)));
                return;
            }
        }
        self.sched.progress.future_ok = false;
        // Every selected activity of the active project, not just the current one.
        let selected: BTreeSet<usize> = self.selected_in_order().into_iter().collect();
        let multi = self.open_set.multi();
        let mut results: Vec<(ProjectData, usize)> = vec![];
        for slot in 0..origins.len() {
            let spotlit = self.spotlit_in(slot);
            let none = BTreeSet::new();
            let scope = if actuals {
                ProgressScope::AutoComputeActuals
            } else if draft.selected_only {
                ProgressScope::Selected(if slot == 0 { &selected } else { &none })
            } else if self.gantt.options.spotlight {
                // All highlighted activities.
                ProgressScope::Selected(&spotlit)
            } else {
                ProgressScope::All
            };
            let mut next = if slot == 0 {
                self.project.clone()
            } else {
                self.open_set.parked[slot - 1].project.clone()
            };
            // Apply Actuals is the shared rule the command line runs too.
            let result = if actuals {
                apply_actuals(&mut next, dates[slot], draft.rule)
            } else {
                apply_progress(&mut next, dates[slot], scope, draft.rule)
            };
            match result {
                Ok(count) => results.push((next, count)),
                Err(e) => {
                    self.error(if multi {
                        format!("{}: {e}", names[slot])
                    } else {
                        e
                    });
                    return;
                }
            }
        }
        let total: usize = results.iter().map(|(_, count)| count).sum();
        let before = self.project.clone();
        let revision = self.records_revision;
        let mut results = results.into_iter();
        self.project = results.next().expect("the active project").0;
        for (parked, (project, _)) in self.open_set.parked.iter_mut().zip(results) {
            let previous = std::mem::replace(&mut parked.project, project);
            let previous_revision = parked.records_revision;
            self.records_serial += 1;
            parked.records_revision = self.records_serial;
            parked.history.record_bulk(
                &previous,
                previous_revision,
                &parked.project,
                parked.records_revision,
                time,
            );
            parked.dirty = true;
        }
        self.data_date = date_string(origins[0], Some(dates[0]));
        // The next spotlight starts from the new data date.
        self.layout_opts.spotlight_end = None;
        self.bulk_step(before, revision, time);
        if multi {
            // One journal entry undoes the active project and the others together.
            self.open_set.steps_seen = self.history.steps();
            self.open_set.undo.push(self.open_ids());
            self.open_set.redo.clear();
            self.open_set.version += 1;
        }
        self.save();
        let projects = if multi {
            format!(" in {} projects", origins.len())
        } else {
            String::new()
        };
        self.status = if actuals {
            format!(
                "Actuals applied to {total} activities set to compute actuals{projects}; timesheet actuals are not stored in XER files. Press F9 to schedule."
            )
        } else {
            format!("Progress updated for {total} activities{projects}. Press F9 to schedule.")
        };
        // The reference closes the dialog with a notice and opens it next
        // time with the same remaining duration choice.
        let p = &mut self.sched.progress;
        if actuals {
            p.actuals = None;
            p.actuals_rule = draft.rule;
            p.done = Some(("Apply Actuals", "Actuals are applied."));
        } else {
            p.update = None;
            p.update_rule = draft.rule;
            p.done = Some(("Update Progress", "Progress is updated."));
        }
    }

    // ---------------------------------------------------------- costs

    pub(crate) fn open_recalc_costs(&mut self) {
        self.sched.progress.recalc = Some(RecalcDraft {
            active_only: false,
            sync_overtime: crate::defaults::recalc_sync_overtime(),
        });
    }

    pub(crate) fn run_recalc_costs(&mut self, time: f64) {
        let Some(draft) = self.sched.progress.recalc.clone() else {
            return;
        };
        // Every open project, as the dialog's "current project(s)" says.
        let multi = self.open_set.multi();
        let reports = self.edit_every_open_project(
            time,
            |p| {
                let pid = p.model.project.proj_id;
                let mut report = recalculate_assignment_costs(
                    &mut p.document,
                    pid,
                    draft.active_only,
                    draft.sync_overtime,
                );
                if multi {
                    let name = &p.model.project.name;
                    for s in &mut report.skipped {
                        *s = format!("{name}: {s}");
                    }
                }
                report
            },
            |r| r.updated > 0,
        );
        let projects = reports.len();
        let mut report = RecalcReport::default();
        for r in reports {
            report.updated += r.updated;
            report.unchanged += r.unchanged;
            report.skipped.extend(r.skipped);
        }
        if report.updated > 0 {
            self.save();
        }
        self.status = format!(
            "Assignment costs recalculated{}: {} updated, {} unchanged, {} skipped",
            if multi {
                format!(" in {projects} projects")
            } else {
                String::new()
            },
            report.updated,
            report.unchanged,
            report.skipped.len()
        );
        if !report.skipped.is_empty() {
            self.log = std::iter::once("Recalculate Assignment Costs: skipped assignments".into())
                .chain(report.skipped.iter().map(|s| format!("\t{s}")))
                .collect();
        }
        self.sched.progress.recalc = None;
    }

    // ---------------------------------------------------------- drawing

    /// Draw the open progress dialogs and route Enter and Esc to them.
    pub(crate) fn progress_dialogs(&mut self, ctx: &egui::Context) {
        // Shift+F9 levels; the plain F9 handler has already opened Schedule.
        let shift_f9 = ctx.input(|i| {
            i.events.iter().any(|e| {
                matches!(e, egui::Event::Key { key: Key::F9, pressed: true, modifiers, .. } if modifiers.shift)
            })
        });
        if shift_f9 && self.sched.progress.level.is_none() {
            self.sched.schedule_open = false;
            self.open_level_resources();
        }
        if !self.top_progress_dialog() {
            return;
        }
        let time = ctx.input(|i| i.time);
        if let Some((title, text)) = self.sched.progress.done {
            self.progress_done_notice(ctx, title, text);
            return;
        }
        let confirming = self.sched.progress.confirm_future.clone();
        let combo_open = ctx.data(|d| {
            COMBOS
                .iter()
                .any(|key| d.get_temp::<bool>(egui::Id::new(*key)).unwrap_or(false))
        });
        if self.top_schedule_dialog().is_none() && !combo_open && confirming.is_none() {
            let p = &self.sched.progress;
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                let p = &mut self.sched.progress;
                if p.select_resources {
                    p.select_resources = false;
                } else {
                    p.level = None;
                    p.update = None;
                    p.actuals = None;
                    p.store_period = false;
                    p.recalc = None;
                }
            } else if ctx.input(|i| i.key_pressed(Key::Enter)) {
                if p.select_resources {
                    self.sched.progress.select_resources = false;
                } else if p.level.is_some() {
                    self.begin_busy(ctx, crate::busy::Busy::Level(time));
                } else if p.update.is_some() {
                    self.run_progress(false, time);
                } else if p.actuals.is_some() {
                    self.run_progress(true, time);
                } else if p.recalc.is_some() {
                    self.run_recalc_costs(time);
                }
            }
        }
        if self.sched.progress.level.is_some() {
            self.level_dialog(ctx, time);
        }
        if self.sched.progress.select_resources {
            self.select_resources_dialog(ctx);
        }
        if self.sched.progress.update.is_some() {
            self.update_progress_dialog(ctx, time);
        }
        if self.sched.progress.actuals.is_some() {
            self.apply_actuals_dialog(ctx, time);
        }
        if self.sched.progress.store_period {
            self.store_period_dialog(ctx);
        }
        if self.sched.progress.recalc.is_some() {
            self.recalc_dialog(ctx, time);
        }
        if let Some(date) = confirming {
            self.future_date_dialog(ctx, &date, time);
        }
    }

    /// The notice after Update Progress or Apply Actuals: OK (the default)
    /// or Escape closes it.
    fn progress_done_notice(&mut self, ctx: &egui::Context, title: &str, text: &str) {
        let mut rects = vec![];
        let clicked = crate::kit_window::message_box(
            ctx,
            "progress_done",
            title,
            crate::kit_window::MessageIcon::Info,
            text,
            &["OK"],
            Some(0),
            &mut rects,
        );
        self.record_dialog_actions(rects);
        if clicked.is_some() || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.sched.progress.done = None;
        }
    }

    /// Apply Actuals with a new data date after today: ask before applying.
    fn future_date_dialog(&mut self, ctx: &egui::Context, date: &str, time: f64) {
        let mut rects = vec![];
        let clicked = crate::kit_window::message_box(
            ctx,
            "future_date",
            "Apply Actuals",
            crate::kit_window::MessageIcon::Question,
            &format!("The new data date, {date}, is later than today. Apply actuals up to a future date?"),
            &["Yes", "No"],
            crate::kit_window::Question::FutureActuals.default_in(&["Yes", "No"]),
            &mut rects,
        );
        self.record_dialog_actions(rects);
        let answer = if ctx.input(|i| i.key_pressed(Key::Escape)) {
            Some(false)
        } else {
            clicked.map(|k| k == 0)
        };
        match answer {
            Some(true) => {
                self.sched.progress.confirm_future = None;
                self.sched.progress.future_ok = true;
                self.run_progress(true, time);
                self.sched.progress.future_ok = false;
            }
            Some(false) => self.sched.progress.confirm_future = None,
            None => {}
        }
    }

    fn level_dialog(&mut self, ctx: &egui::Context, time: f64) {
        #[derive(Default)]
        struct Clicks {
            level: bool,
            close: bool,
            log: bool,
            default: bool,
            help: bool,
            select: bool,
        }
        let choices = priority_choices(&self.project.document, self.project.model.project.proj_id);
        let project = self.project.model.project.name.clone();
        let defaults = LevelSettings::defaults(self.auto_level(), &self.project);
        let unsupported = "The leveling engine does not support this option.";
        let mut log_settings = self.sched_log_settings();
        let sched = &mut log_settings;
        let state = &mut self.sched.progress;
        let Some(settings) = state.level.as_mut() else {
            return;
        };
        let row = &mut state.level_row;
        let (clicks, closed, actions) = modal(
            ctx,
            "level",
            "Level Resources",
            Vec2::new(600., 508.),
            |c| {
                let mut k = Clicks::default();
                c.fill(5., 5., 478., 498., FACE);
                c.lowered(5., 5., 478., 498.);
                c.checkbox(
                    "level_auto",
                    14.,
                    14.,
                    400.,
                    20.,
                    "Automatically level resources when scheduling",
                    &mut settings.auto,
                    true,
                );
                // Options the engine lacks stay enabled only to turn them off.
                let enabled = settings.outer_assign;
                let r = c.checkbox(
                    "level_outer",
                    14.,
                    38.,
                    390.,
                    20.,
                    "Consider assignments in other projects with priority equal/higher than",
                    &mut settings.outer_assign,
                    enabled,
                );
                r.on_hover_text(unsupported);
                c.spin(
                    "level_outer_priority",
                    410.,
                    37.,
                    64.,
                    23.,
                    &mut settings.outer_priority,
                    1.,
                    1.0..=5.,
                    false,
                );
                let enabled = !settings.keep_dates;
                let r = c.checkbox(
                    "level_keep_dates",
                    14.,
                    62.,
                    400.,
                    20.,
                    "Preserve scheduled early and late dates",
                    &mut settings.keep_dates,
                    enabled,
                );
                r.on_hover_text(
                    "The leveling engine always preserves scheduled early and late dates.",
                );
                c.checkbox(
                    "level_recalc_costs",
                    14.,
                    86.,
                    400.,
                    20.,
                    "Recalculate assignment costs after leveling",
                    &mut settings.recalc_costs,
                    true,
                );
                c.checkbox(
                    "level_all_resources",
                    14.,
                    110.,
                    300.,
                    20.,
                    "Level all resources",
                    &mut settings.all_resources,
                    true,
                );
                k.select = crate::dialog_kit::button(
                    c,
                    "level_select_resources",
                    330.,
                    108.,
                    144.,
                    25.,
                    "Select Resources...",
                    !settings.all_resources,
                );
                c.checkbox(
                    "level_within_float",
                    14.,
                    140.,
                    400.,
                    20.,
                    "Level resources only within activity Total Float",
                    &mut settings.within_float,
                    true,
                );
                let within = settings.within_float;
                c.label(
                    32.,
                    165.,
                    16.,
                    "Preserve minimum float when leveling",
                    within,
                );
                c.edit(
                    "level_min_float",
                    330.,
                    163.,
                    144.,
                    21.,
                    &mut settings.float_text,
                    within,
                    Align::Min,
                );
                c.label(
                    14.,
                    192.,
                    16.,
                    "Max percent to over-allocate resources",
                    true,
                );
                c.spin(
                    "level_over_alloc",
                    330.,
                    189.,
                    144.,
                    23.,
                    &mut settings.over_alloc,
                    5.,
                    0.0..=1000.,
                    true,
                );
                c.etched(11., 222., 466.);
                c.bold(14., 226., 16., "Leveling priorities");
                // Priorities: the shared sort-key table (as in Sort).
                let rows = settings.priorities.len();
                let mut picked = (rows > 0).then(|| (*row).min(rows - 1));
                crate::grid::sort_keys::table(
                    c,
                    "level",
                    (14., 250., 340., 150.),
                    [214., 120.],
                    &choices,
                    &mut settings.priorities,
                    &mut picked,
                );
                *row = picked.unwrap_or(0);
                if crate::dialog_kit::button(c, "level_add", 362., 250., 108., 25., "Add", true) {
                    let next = choices
                        .iter()
                        .find(|(key, _)| !settings.priorities.iter().any(|r| r.key == *key))
                        .map_or("priority_type", |(key, _)| key.as_str());
                    settings.priorities.push(PriorityRow::new(next, true));
                    *row = settings.priorities.len() - 1;
                }
                if crate::dialog_kit::button(
                    c,
                    "level_remove",
                    362.,
                    280.,
                    108.,
                    25.,
                    "Remove",
                    rows > 0,
                ) && *row < settings.priorities.len()
                {
                    settings.priorities.remove(*row);
                    *row = (*row).min(settings.priorities.len().saturating_sub(1));
                }
                if crate::dialog_kit::button(
                    c,
                    "level_up",
                    362.,
                    320.,
                    108.,
                    25.,
                    "Move Up",
                    *row > 0 && *row < rows,
                ) {
                    settings.priorities.swap(*row, *row - 1);
                    *row -= 1;
                }
                if crate::dialog_kit::button(
                    c,
                    "level_down",
                    362.,
                    350.,
                    108.,
                    25.,
                    "Move Down",
                    *row + 1 < rows,
                ) {
                    settings.priorities.swap(*row, *row + 1);
                    *row += 1;
                }
                c.label(14., 412., 16., "Use scheduling options from", true);
                let mut shown = project.clone();
                c.edit(
                    "level_options_project",
                    190.,
                    410.,
                    170.,
                    21.,
                    &mut shown,
                    false,
                    Align::Min,
                );
                crate::dialog_kit::button(
                    c,
                    "level_change",
                    366.,
                    408.,
                    108.,
                    25.,
                    "Change",
                    false,
                );
                c.checkbox(
                    "level_log_to_file",
                    14.,
                    442.,
                    120.,
                    20.,
                    "Log to file",
                    &mut sched.0,
                    true,
                );
                let log = sched.0;
                c.checkbox(
                    "level_log_html",
                    150.,
                    442.,
                    120.,
                    20.,
                    "As Html",
                    &mut sched.1,
                    log,
                );
                c.edit(
                    "level_log_path",
                    32.,
                    468.,
                    442.,
                    21.,
                    &mut sched.2,
                    log,
                    Align::Min,
                );
                let column = crate::dialog_kit::button_column(
                    c,
                    600.,
                    &[
                        Entry::Button("level_run", "Level", true, false),
                        Entry::close("level_close"),
                        Entry::Button("level_view_log", "View Log", true, true),
                        Entry::Button("level_default", "Default", true, false),
                        Entry::help("level_help"),
                    ],
                );
                crate::dialog_kit::mark_default(c, "level_run");
                let clicked = |key: &str| column.clicked.as_deref() == Some(key);
                k.level = clicked("level_run");
                k.close = clicked("level_close");
                k.log = clicked("level_view_log");
                k.default = clicked("level_default");
                k.help = clicked("level_help");
                k
            },
        );
        let (log_to_file, log_html, log_path) = log_settings;
        self.sched.log_to_file = log_to_file;
        self.sched.log_html = log_html;
        self.sched.log_path = log_path;
        self.record(actions);
        if closed {
            self.sched.progress.level = None;
        } else if clicks.level {
            self.begin_busy(ctx, crate::busy::Busy::Level(time));
        } else if clicks.close {
            self.close_level(time);
        } else if clicks.log {
            self.show_log = true;
        } else if clicks.default {
            self.sched.progress.level = Some(defaults);
            self.sched.progress.level_row = 0;
        } else if clicks.help {
            self.open_help_for("level");
        } else if clicks.select {
            let chosen = self
                .sched
                .progress
                .level
                .as_ref()
                .map(|s| s.selected.clone())
                .unwrap_or_default();
            self.sched.progress.select_draft = chosen;
            self.sched.progress.select_resources = true;
        }
    }

    /// The shared log settings of the Schedule dialog, for the Level dialog.
    fn sched_log_settings(&self) -> (bool, bool, String) {
        (
            self.sched.log_to_file,
            self.sched.log_html,
            self.sched.log_path.clone(),
        )
    }

    fn select_resources_dialog(&mut self, ctx: &egui::Context) {
        let list = leveled_resources(&self.project);
        let draft = &mut self.sched.progress.select_draft;
        let ((ok, cancel), closed, actions) = modal(
            ctx,
            "select_rsrc",
            "Select Resources",
            Vec2::new(452., 300.),
            |c| {
                c.fill(5., 5., 330., 290., FACE);
                c.lowered(5., 5., 330., 290.);
                let cols = [
                    ("Resource ID", 90.),
                    ("Resource Name", 170.),
                    ("Level", 54.),
                ]
                .map(|(title, width)| crate::dialog_kit::Col { title, width });
                crate::dialog_kit::list(
                    c,
                    "select_rsrc_list",
                    (11., 11., 318., 278.),
                    None,
                    &cols,
                    list.len(),
                    None,
                    None,
                    &mut |c, i, k, rect, sel| {
                        let (id, short, name) = &list[i];
                        match k {
                            0 => crate::dialog_kit::cell_text(c, rect, short, sel, 4.),
                            1 => crate::dialog_kit::cell_text(c, rect, name, sel, 4.),
                            _ => {
                                let at = rect.min - c.origin;
                                let mut on = draft.contains(id);
                                c.checkbox(
                                    &format!("select_rsrc_{i}"),
                                    (at.x + rect.width() / 2. - 6.5).round(),
                                    at.y + 1.,
                                    16.,
                                    16.,
                                    "",
                                    &mut on,
                                    true,
                                );
                                if on {
                                    draft.insert(*id);
                                } else {
                                    draft.remove(id);
                                }
                            }
                        }
                    },
                );
                let column = crate::dialog_kit::button_column(
                    c,
                    452.,
                    &[
                        Entry::ok("select_rsrc_ok"),
                        Entry::cancel("select_rsrc_cancel"),
                    ],
                );
                let clicked = |key: &str| column.clicked.as_deref() == Some(key);
                (clicked("select_rsrc_ok"), clicked("select_rsrc_cancel"))
            },
        );
        self.record(actions);
        if ok {
            let chosen = std::mem::take(&mut self.sched.progress.select_draft);
            if let Some(settings) = self.sched.progress.level.as_mut() {
                settings.selected = chosen;
            }
        }
        if ok || cancel || closed {
            self.sched.progress.select_resources = false;
        }
    }

    /// New Data Date edit with its calendar drop-down.
    #[allow(clippy::too_many_arguments)]
    fn date_field(
        c: &mut Canvas,
        key: &'static str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        text: &mut String,
        origin: i64,
        enabled: bool,
    ) {
        // The shared date edit: the `...` inside the edit's frame.
        crate::views::sheet::date_text_field(
            c,
            key,
            &format!("{key}_picker"),
            (x, y, w, h),
            text,
            origin,
            enabled,
        );
    }

    /// The working panel shared by both progress dialogs: the dialog face
    /// with a lowered edge, `w` x `h`, inside a 5 px margin of the client.
    fn progress_panel(c: &mut Canvas, w: f32, h: f32) {
        c.fill(5., 5., w - 10., h - 10., FACE);
        c.lowered(5., 5., w - 10., h - 10.);
    }

    /// A group of two radios under a section heading at (x, y), `w` wide:
    /// the heading bar, then the radios 23 px apart. Returns the clicked
    /// radio.
    fn radio_group(
        c: &mut Canvas,
        (x, y, w): (f32, f32, f32),
        title: &str,
        radios: [(&str, &str, bool, bool); 2],
    ) -> Option<usize> {
        crate::dialog_kit::section(c, x, y, w, GROUP_H, title);
        let mut clicked = None;
        for (n, (key, label, on, enabled)) in radios.into_iter().enumerate() {
            let top = y + 29. + 23. * n as f32;
            if c.radio(key, x + 12., top, w - 24., 17., label, on, enabled) {
                clicked = Some(n);
            }
        }
        clicked
    }

    fn update_progress_dialog(&mut self, ctx: &egui::Context, time: f64) {
        let origin = self.project.model.origin_serial;
        let current = display_date(origin, self.project.model.project.data_date);
        let has_selection = self.selected.is_some();
        let Some(draft) = self.sched.progress.update.as_mut() else {
            return;
        };
        // The panel at the left, the button column in the strip at x 366.
        let (w, h) = (UPDATE_PANEL + STRIP_W, 248.);
        let ((apply, cancel, help), closed, actions) =
            modal(ctx, "update", "Update Progress", Vec2::new(w, h), |c| {
                Self::progress_panel(c, UPDATE_PANEL, h);
                let (x, inner) = (15., UPDATE_PANEL - 30.);
                // The two dates, labels at the left, values in one column.
                let field_x = x + inner - DATE_W;
                c.label(x + 4., 17., 21., "Current Data Date", true);
                c.label(field_x + 4., 17., 21., &current, true);
                c.label(x + 4., 44., 21., "New Data Date", true);
                Self::date_field(
                    c,
                    "update_new_date",
                    field_x,
                    44.,
                    DATE_W,
                    21.,
                    &mut draft.new_date,
                    origin,
                    true,
                );
                let pick = Self::radio_group(
                    c,
                    (x, 76., inner),
                    "With the spotlight on, update",
                    [
                        (
                            "update_all",
                            "All highlighted activities",
                            !draft.selected_only,
                            true,
                        ),
                        (
                            "update_selected",
                            "Selected activities only",
                            draft.selected_only,
                            has_selection,
                        ),
                    ],
                );
                if let Some(n) = pick {
                    draft.selected_only = n == 1;
                }
                let by_type = draft.rule == RemainingRule::DurationType;
                let pick = Self::radio_group(
                    c,
                    (x, 76. + GROUP_H + 9., inner),
                    "How to set remaining durations",
                    [
                        ("update_by_type", "From its duration type", by_type, true),
                        ("update_always", "Always recalculate", !by_type, true),
                    ],
                );
                if let Some(n) = pick {
                    draft.rule = REMAINING_RULES[n];
                }
                let column = crate::dialog_kit::button_column(
                    c,
                    w,
                    &[
                        Entry::Button("update_apply", "Apply", true, false),
                        Entry::cancel("update_cancel"),
                        Entry::help("update_help"),
                    ],
                );
                crate::dialog_kit::mark_default(c, "update_apply");
                let clicked = |key: &str| column.clicked.as_deref() == Some(key);
                (
                    clicked("update_apply"),
                    clicked("update_cancel"),
                    clicked("update_help"),
                )
            });
        self.record(actions);
        if cancel || closed {
            self.sched.progress.update = None;
        } else if apply {
            self.run_progress(false, time);
        } else if help {
            self.open_help_for("update");
        }
    }

    fn apply_actuals_dialog(&mut self, ctx: &egui::Context, time: f64) {
        use crate::dialog_kit::{self as kit, Col};
        let origin = self.project.model.origin_serial;
        // One row per open project, the active one first: Project ID,
        // Project Name, Current Data Date, Project Planned Start.
        let projects: Vec<[String; 4]> = self
            .open_projects()
            .iter()
            .map(|p| {
                let o = p.model.origin_serial;
                [
                    p.model.project.name.clone(),
                    crate::views::project_title(p),
                    display_date(o, p.model.project.data_date),
                    display_date(o, p.model.project.plan_start),
                ]
            })
            .collect();
        let Some(draft) = self.sched.progress.actuals.as_mut() else {
            return;
        };
        // Projects opened or closed while the dialog is up keep their rows.
        let wanted = projects.len().saturating_sub(1);
        let fill = draft.new_date.clone();
        draft.others.resize(wanted, fill);
        let widths = [76., 136., 100., 100., 116.];
        let panel = 15. + widths.iter().sum::<f32>() + 2. + 15.;
        let (w, h) = (panel + STRIP_W, 392.);
        let ((apply, cancel, help), closed, actions) =
            modal(ctx, "actuals", "Apply Actuals", Vec2::new(w, h), |c| {
                Self::progress_panel(c, panel, h);
                let (x, inner) = (15., panel - 30.);
                // The open projects on the kit list. A click on a New Data
                // Date cell edits it while each project has its own date.
                c.bold(x + 2., 13., 16., "Projects to update");
                let titles = [
                    "Project ID",
                    "Project Name",
                    "Current Data Date",
                    "New Data Date",
                    "Project Planned Start",
                ];
                let cols: Vec<Col> = titles
                    .into_iter()
                    .zip(widths)
                    .map(|(title, width)| Col { title, width })
                    .collect();
                let own = draft.own_date;
                let selected = draft.editing_row.unwrap_or(0);
                let new_date = &mut draft.new_date;
                let others = &mut draft.others;
                let mut editing = draft.editing_row;
                let out = kit::list(
                    c,
                    "actuals_projects",
                    (x, 33., inner, 150.),
                    None,
                    &cols,
                    projects.len(),
                    Some(selected),
                    None,
                    &mut |c, n, k, cell, sel| {
                        if k != 3 {
                            let text = &projects[n][if k > 3 { 3 } else { k }];
                            kit::cell_text(c, cell, text, sel, 3.);
                            return;
                        }
                        let key = if n == 0 {
                            "actuals_row_new_date".to_string()
                        } else {
                            format!("actuals_row_new_date_{n}")
                        };
                        let text = if own && n > 0 {
                            &mut others[n - 1]
                        } else {
                            &mut *new_date
                        };
                        if own && editing == Some(n) {
                            let at = cell.min - c.origin;
                            let (cw, ch) = (cell.width() - 1., cell.height() - 1.);
                            c.edit(&key, at.x, at.y, cw, ch, text, true, Align::Min);
                        } else {
                            kit::cell_text(c, cell, text, sel, 3.);
                            if c.hit(&key, cell, own).clicked() {
                                editing = Some(n);
                            }
                        }
                    },
                );
                if let Some(n) = out.clicked {
                    editing = Some(n);
                }
                draft.editing_row = editing;
                // One new data date for every project, or each its own.
                let top = 33. + 150. + 10.;
                let group = GROUP_H + 27.;
                kit::section(c, x, top, inner, group, "New data date");
                if c.radio(
                    "actuals_own_date",
                    x + 12.,
                    top + 29.,
                    inner - 24.,
                    17.,
                    "A new date for each project",
                    own,
                    true,
                ) {
                    draft.own_date = true;
                }
                if c.radio(
                    "actuals_same_date",
                    x + 12.,
                    top + 52.,
                    inner - 24.,
                    17.,
                    "One new date for every project",
                    !own,
                    true,
                ) {
                    draft.own_date = false;
                }
                c.label(x + 30., top + 76., 21., "New Data Date", !own);
                Self::date_field(
                    c,
                    "actuals_new_date",
                    x + 160.,
                    top + 76.,
                    DATE_W,
                    21.,
                    &mut draft.new_date,
                    origin,
                    !own,
                );
                let by_type = draft.rule == RemainingRule::DurationType;
                let pick = Self::radio_group(
                    c,
                    (x, top + group + 9., inner),
                    "How to set remaining durations",
                    [
                        ("actuals_by_type", "From its duration type", by_type, true),
                        ("actuals_always", "Always recalculate", !by_type, true),
                    ],
                );
                if let Some(n) = pick {
                    draft.rule = REMAINING_RULES[n];
                }
                let column = kit::button_column(
                    c,
                    w,
                    &[
                        Entry::Button("actuals_apply", "Apply", true, false),
                        Entry::cancel("actuals_cancel"),
                        Entry::help("actuals_help"),
                    ],
                );
                kit::mark_default(c, "actuals_apply");
                let clicked = |key: &str| column.clicked.as_deref() == Some(key);
                (
                    clicked("actuals_apply"),
                    clicked("actuals_cancel"),
                    clicked("actuals_help"),
                )
            });
        self.record(actions);
        if cancel || closed {
            self.sched.progress.actuals = None;
        } else if apply {
            self.run_progress(true, time);
        } else if help {
            self.open_help_for("actuals");
        }
    }

    fn store_period_dialog(&mut self, ctx: &egui::Context) {
        self.store_period_body(ctx);
    }

    fn recalc_dialog(&mut self, ctx: &egui::Context, time: f64) {
        let Some(draft) = self.sched.progress.recalc.as_mut() else {
            return;
        };
        let ((run, close, help), closed, actions) = modal(
            ctx,
            "recalc",
            "Recalculate Assignment Costs",
            Vec2::new(480., 190.),
            |c| {
                c.fill(5., 5., 358., 180., FACE);
                c.lowered(5., 5., 358., 180.);
                c.bold(
                    14.,
                    12.,
                    16.,
                    "Recalculate assignment costs on current project(s)",
                );
                c.etched(11., 36., 346.);
                c.bold(14., 46., 16., "Recalculation options");
                if c.radio(
                    "recalc_all",
                    32.,
                    74.,
                    300.,
                    17.,
                    "All resources on project(s)",
                    !draft.active_only,
                    true,
                ) {
                    draft.active_only = false;
                }
                if c.radio(
                    "recalc_active",
                    32.,
                    96.,
                    300.,
                    17.,
                    "Active resources only",
                    draft.active_only,
                    true,
                ) {
                    draft.active_only = true;
                }
                c.checkbox(
                    "recalc_overtime",
                    14.,
                    130.,
                    340.,
                    20.,
                    "Synchronize Overtime Factor while recalculating cost",
                    &mut draft.sync_overtime,
                    true,
                );
                let column = crate::dialog_kit::button_column(
                    c,
                    480.,
                    &[
                        Entry::Button("recalc_run", "Recalculate", true, false),
                        Entry::close("recalc_close"),
                        Entry::help("recalc_help"),
                    ],
                );
                crate::dialog_kit::mark_default(c, "recalc_run");
                let clicked = |key: &str| column.clicked.as_deref() == Some(key);
                (
                    clicked("recalc_run"),
                    clicked("recalc_close"),
                    clicked("recalc_help"),
                )
            },
        );
        self.record(actions);
        if close || closed {
            self.sched.progress.recalc = None;
        } else if run {
            self.run_recalc_costs(time);
        } else if help {
            self.open_help_for("recalc");
        }
    }
}

#[cfg(test)]
#[path = "progress_tools_tests.rs"]
mod tests;
