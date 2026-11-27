//! General, Status, relationship, Summary and Trace Logic tabs (Resources is
//! in `assignments.rs`).
use super::assign_kit::{AssignButton, AssignWindow};
use super::*;
use crate::grid::columns::{Field, Kind, Phase, Value};
use crate::grid::marks::{self, End};
use crate::grid::PickLine;

pub use openxer_xer::edit::activity::{DURATION_TYPES, PERCENT_TYPES};
const ACTIVITY_TYPES: [TaskType; 6] = [
    TaskType::Task,
    TaskType::Resource,
    TaskType::Loe,
    TaskType::Mile,
    TaskType::FinMile,
    TaskType::Wbs,
];
/// The narrowest a Relationships panel gets: its Assign, Remove and GoTo
/// buttons and the first grid columns stay in view.
pub(crate) const RELATIONSHIP_PANEL_MIN: f32 = 300.;
const LINK_TYPES: [LinkType; 4] = [LinkType::FS, LinkType::SS, LinkType::FF, LinkType::SF];

/// Budgeted, actual, remaining and at-completion figures.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Figures {
    pub budget: f64,
    pub actual: f64,
    pub remaining: f64,
}
impl Figures {
    pub fn at_completion(&self) -> f64 {
        self.actual + self.remaining
    }
    fn add(&mut self, budget: f64, actual: f64, remaining: f64) {
        self.budget += budget;
        self.actual += actual;
        self.remaining += remaining;
    }
    pub fn values(&self) -> [f64; 4] {
        [
            self.budget,
            self.actual,
            self.remaining,
            self.at_completion(),
        ]
    }
}
#[derive(Clone, Copy, Default, Debug)]
pub struct Totals {
    pub labor_units: Figures,
    pub nonlabor_units: Figures,
    pub material_units: Figures,
    pub labor_cost: Figures,
    pub nonlabor_cost: Figures,
    pub material_cost: Figures,
    pub expenses: Figures,
}

/// Roll up the activity's resource assignments and expenses.
pub fn totals(doc: &openxer_xer::Document, task_id: i64, pid: i64) -> Totals {
    let mut t = Totals::default();
    let (mut labor, mut nonlabor) = (false, false);
    if let Some(table) = doc.table("TASKRSRC") {
        for index in records::rows_for(doc, "TASKRSRC", task_id, pid) {
            let r = &table.rows[index];
            let v = |k: &str| float(table.value(r, k));
            let mut kind = table.value(r, "rsrc_type").to_string();
            if kind.is_empty() {
                kind = records::lookup(
                    doc,
                    "RSRC",
                    "rsrc_id",
                    num(table.value(r, "rsrc_id")),
                    "rsrc_type",
                );
            }
            let (units, cost) = match kind.as_str() {
                "RT_Equip" => {
                    nonlabor = true;
                    (&mut t.nonlabor_units, &mut t.nonlabor_cost)
                }
                "RT_Mat" => (&mut t.material_units, &mut t.material_cost),
                _ => {
                    labor = true;
                    (&mut t.labor_units, &mut t.labor_cost)
                }
            };
            units.add(
                v("target_qty"),
                v("act_reg_qty") + v("act_ot_qty"),
                v("remain_qty"),
            );
            cost.add(
                v("target_cost"),
                v("act_reg_cost") + v("act_ot_cost"),
                v("remain_cost"),
            );
        }
    }
    // Without an assignment of a type the activity carries its own units.
    for (own, units, columns) in [
        (
            labor,
            &mut t.labor_units,
            ["target_work_qty", "act_work_qty", "remain_work_qty"],
        ),
        (
            nonlabor,
            &mut t.nonlabor_units,
            ["target_equip_qty", "act_equip_qty", "remain_equip_qty"],
        ),
    ] {
        if !own {
            let v = |k: &str| float(&records::task_value(doc, task_id, pid, k));
            units.add(v(columns[0]), v(columns[1]), v(columns[2]));
        }
    }
    if let Some(table) = doc.table("PROJCOST") {
        for index in records::rows_for(doc, "PROJCOST", task_id, pid) {
            let r = &table.rows[index];
            let v = |k: &str| float(table.value(r, k));
            t.expenses
                .add(v("target_cost"), v("act_cost"), v("remain_cost"));
        }
    }
    t
}

/// Driving flags for the given relationships, from the engine's own relationship
/// float calculation (the same one the benchmark checks). `None` until scheduled.
pub fn driving_flags(model: &mut Model, links: &[usize]) -> Vec<Option<bool>> {
    let scheduler = openxer_engine::scheduler::Scheduler::new(model);
    links
        .iter()
        .map(|&r| scheduler.relationship_driving(r))
        .collect()
}
fn yes_no(value: Option<bool>) -> String {
    match value {
        Some(true) => "Yes".into(),
        Some(false) => "No".into(),
        None => String::new(),
    }
}
/// A shown Start or Finish as the activity table shows it: " A" on an
/// actual, the constraint mark on a constrained date; with the table's
/// hover text for a constrained one.
fn marked_date(origin: i64, a: &Activity, leveled: bool, end: End) -> (String, Option<String>) {
    let value = match end {
        End::Start => displayed_start(a, leveled),
        End::Finish => displayed_finish(a, leveled),
    };
    (
        format::date_marked(origin, value, marks::mark(a, end)),
        marks::tooltip(origin, a, end),
    )
}

/// Page geometry of the Status tab: three group boxes side by side
/// (Duration, Status over Constraints, Units) and the label and field
/// columns inside each. At full width the groups keep the measured layout
/// (Duration 415 px, Units 454 px, Status/Constraints stretches); below it
/// every group and column narrows together, down to a compact layout, so the
/// groups never overlap and the Units group is never cut off.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StatusLayout {
    /// Duration group left and width; its fields' left and width.
    pub c1: f32,
    pub dur_w: f32,
    pub dur_field_x: f32,
    pub dur_field_w: f32,
    /// Status and Constraints group left and width.
    pub c2: f32,
    pub mid_w: f32,
    /// First field column (Started, Finished, Exp Finish, Primary).
    pub field1_x: f32,
    pub field1_w: f32,
    /// Second label and field columns (percent, Suspend, Resume, Secondary).
    pub label2_x: f32,
    pub field2_x: f32,
    pub field2_w: f32,
    /// Units group left and width; its fields' left and width.
    pub c3: f32,
    pub units_w: f32,
    pub units_field_x: f32,
    pub units_field_w: f32,
}

impl StatusLayout {
    /// The gap between groups and the margin at both ends of the page.
    const GAP: f32 = 5.;
    const MARGIN: f32 = 7.;

    /// The layout for a page `page_w` wide (page coordinates, as `form::rect`
    /// offsets from the page origin).
    pub(crate) fn new(page_w: f32) -> Self {
        // Offsets inside each group, full and compact: Duration (field x,
        // field w, group w), Status (field 1 x, field 1 w, label 2 x,
        // field 2 x, field 2 w, group w), Units (field x, field w, group w).
        const FULL: [f32; 12] = [
            222., 165., 415., 224., 260., 495., 679., 249., 945., 221., 207., 454.,
        ];
        const COMPACT: [f32; 12] = [
            100., 112., 230., 100., 135., 247., 322., 135., 469., 100., 112., 230.,
        ];
        let fixed = 2. * Self::MARGIN + 2. * Self::GAP;
        let total = |g: &[f32; 12]| fixed + g[2] + g[8] + g[11];
        let (full, compact) = (total(&FULL), total(&COMPACT));
        let t = ((page_w - compact) / (full - compact)).clamp(0., 1.);
        let g: [f32; 12] = std::array::from_fn(|k| COMPACT[k] + t * (FULL[k] - COMPACT[k]));
        // Past full width the Status and Constraints group takes the rest.
        let mid_w = g[8] + (page_w - full).max(0.);
        let c1 = Self::MARGIN;
        let c2 = c1 + g[2] + Self::GAP;
        let c3 = c2 + mid_w + Self::GAP;
        Self {
            c1,
            dur_w: g[2],
            dur_field_x: c1 + g[0],
            dur_field_w: g[1],
            c2,
            mid_w,
            field1_x: c2 + g[3],
            field1_w: g[4],
            label2_x: c2 + g[5],
            field2_x: c2 + g[6],
            field2_w: g[7],
            c3,
            units_w: g[11],
            units_field_x: c3 + g[9],
            units_field_w: g[10],
        }
    }
}

impl App {
    pub(super) fn general_tab(&mut self, ui: &mut egui::Ui, i: usize) {
        let (task_id, pid) = self.ids(i);
        let serial = self.project_serial();
        let (duration_type, percent_type, primary, manager, resources) =
            (*self.memo(serial, "detail_general", (task_id, pid), || {
                let doc = &self.project.document;
                let duration_type = records::task_value(doc, task_id, pid, "duration_type");
                let percent_type = records::task_value(doc, task_id, pid, "complete_pct_type");
                let primary = num(&records::task_value(doc, task_id, pid, "rsrc_id"));
                let wbs = self.project.details.get(&task_id).map_or(0, |d| d.wbs);
                let manager = doc
                    .table("PROJWBS")
                    .and_then(|t| {
                        let wbs_c = t.field("wbs_id")?;
                        let obs_c = t.field("obs_id")?;
                        t.rows
                            .iter()
                            .find(|r| num(r.get(wbs_c).map_or("", String::as_str)) == wbs)
                            .map(|r| num(r.get(obs_c).map_or("", String::as_str)))
                    })
                    .map(|obs| records::lookup(doc, "OBS", "obs_id", obs, "obs_name"))
                    .unwrap_or_default();
                let mut resources: Vec<(i64, String)> = vec![];
                if let Some(t) = doc.table("TASKRSRC") {
                    if let Some(rsrc_c) = t.field("rsrc_id") {
                        for index in records::rows_for(doc, "TASKRSRC", task_id, pid) {
                            let id = num(t.rows[index].get(rsrc_c).map_or("", String::as_str));
                            if id != 0 && !resources.iter().any(|(r, _)| *r == id) {
                                let name = self
                                    .project
                                    .resource_names
                                    .get(&id)
                                    .cloned()
                                    .unwrap_or_else(|| id.to_string());
                                resources.push((id, name));
                            }
                        }
                    }
                }
                (duration_type, percent_type, primary, manager, resources)
            }))
            .clone();
        let calendars: Vec<(usize, String)> =
            crate::calendars::ops::assignable(&self.project, self.project.model.activities[i].cal)
                .into_iter()
                .map(|c| (c, self.project.calendar_names[c].clone()))
                .collect();
        let a = self.project.model.activities[i].clone();
        let d = self
            .project
            .details
            .get(&task_id)
            .cloned()
            .unwrap_or_default();
        let mut edits: Vec<setters::Edit> = vec![];
        let mut actions: Vec<(&str, Rect)> = vec![];
        // Labels above fields at the measured places; the fields scale with
        // the box (1824 px wide in the reference).
        let content = ui.max_rect();
        let o = content.min - Vec2::new(form::PAD, form::CONTENT_TOP);
        let k = content.width() / 1824.;
        let at = |x: f32, y: f32, w: f32, h: f32| {
            form::rect(
                (o.x + 7. + (x - 7.) * k).round(),
                o.y + y,
                (w * k).round(),
                h,
            )
        };
        let p = ui.painter().clone();
        form::section(&p, Rect::from_min_max(content.min, content.max), "", 0., 0.);
        let fields = [
            ("Activity Type", at(35., 59., 382., 22.)),
            ("Duration Type", at(425., 59., 451., 22.)),
            ("% Complete Type", at(891., 59., 381., 22.)),
            ("Activity Calendar", at(1284., 59., 479., 21.)),
            ("WBS", at(35., 104., 606., 21.)),
            ("Responsible Manager", at(650., 104., 553., 21.)),
            ("Primary Resource", at(1214., 104., 479., 21.)),
        ];
        for (caption, r) in &fields {
            form::above(&p, r.left(), r.top(), caption);
        }
        let types: Vec<String> = ACTIVITY_TYPES
            .iter()
            .map(|t| type_label(*t).to_string())
            .collect();
        let (response, picked) = form::combo(
            ui,
            ui.id().with("activity_type"),
            fields[0].1,
            &types,
            ACTIVITY_TYPES.iter().position(|t| *t == a.task_type),
            true,
        );
        actions.push(("detail_activity_type", response.rect));
        if let Some(n) = picked {
            edits.push(setters::Edit::ActivityType(ACTIVITY_TYPES[n]));
        }
        let labels: Vec<String> = DURATION_TYPES.iter().map(|(_, l)| l.to_string()).collect();
        let current = DURATION_TYPES
            .iter()
            .position(|(code, _)| *code == duration_type)
            .unwrap_or(0);
        let (response, picked) = form::combo(
            ui,
            ui.id().with("duration_type"),
            fields[1].1,
            &labels,
            Some(current),
            true,
        );
        actions.push(("detail_duration_type", response.rect));
        if let Some(n) = picked {
            edits.push(setters::Edit::DurationType(DURATION_TYPES[n].0));
        }
        let labels: Vec<String> = PERCENT_TYPES.iter().map(|(_, l)| l.to_string()).collect();
        let current = PERCENT_TYPES
            .iter()
            .position(|(code, _)| *code == percent_type)
            .unwrap_or(1);
        let (response, picked) = form::combo(
            ui,
            ui.id().with("percent_type"),
            fields[2].1,
            &labels,
            Some(current),
            true,
        );
        actions.push(("detail_percent_type", response.rect));
        if let Some(n) = picked {
            edits.push(setters::Edit::PercentType(PERCENT_TYPES[n].0));
        }
        let names: Vec<String> = calendars.iter().map(|(_, n)| n.clone()).collect();
        let (response, picked) = form::picker(
            ui,
            ui.id().with("activity_calendar"),
            fields[3].1,
            chrome::Icon::Calendars,
            &names,
            calendars.iter().position(|(c, _)| *c == a.cal),
            true,
        );
        actions.push(("detail_activity_calendar", response.rect));
        if let Some(n) = picked {
            edits.push(setters::Edit::Calendar(calendars[n].0));
        }
        // The WBS field opens Select WBS, the window the record views use.
        let shown = self
            .project
            .wbs
            .iter()
            .find(|w| w.id == d.wbs)
            .map(|w| format!("{}  {}", w.code, w.name))
            .unwrap_or_default();
        let response = form::picker_field(
            ui,
            ui.id().with("activity_wbs"),
            fields[4].1,
            chrome::Icon::Wbs,
            &shown,
            true,
        );
        actions.push(("detail_activity_wbs", response.rect));
        if response.clicked() {
            self.open_picker(crate::views::record_view::PickTarget::ActivityWbs(task_id));
        }
        let manager_response = ui.interact(
            fields[5].1,
            ui.id().with("responsible_manager"),
            Sense::hover(),
        );
        form::value(&p, fields[5].1, &manager, false, form::READONLY);
        manager_response.on_hover_text("The responsible manager comes from the WBS element.");
        let mut names = vec![String::new()];
        names.extend(resources.iter().map(|(_, n)| n.clone()));
        let current = if primary == 0 {
            Some(0)
        } else {
            resources
                .iter()
                .position(|(id, _)| *id == primary)
                .map(|n| n + 1)
        };
        let (response, picked) = form::picker(
            ui,
            ui.id().with("primary_resource"),
            fields[6].1,
            chrome::Icon::Resources,
            &names,
            current,
            true,
        );
        actions.push(("detail_primary_resource", response.rect));
        if let Some(n) = picked {
            edits.push(setters::Edit::PrimaryResource(if n == 0 {
                0
            } else {
                resources[n - 1].0
            }));
        }
        for (name, r) in actions {
            self.action_rects
                .insert(name.into(), [r.left(), r.top(), r.right(), r.bottom()]);
        }
        if !edits.is_empty() {
            self.unpin_serial();
        }
        for edit in edits {
            if let Err(e) = self.edit_activity(i, edit) {
                self.error(e);
            }
        }
        if self
            .project
            .details
            .get(&task_id)
            .is_some_and(|d| d.resource_driven_duration)
        {
            let o = ui.max_rect().min;
            ui.painter().text(
                Pos2::new(o.x + 28., o.y + 104.),
                Align2::LEFT_TOP,
                "Duration is controlled by resource units/rates.",
                form::font(),
                Color32::GRAY,
            );
        }
    }

    /// Weighted step progress when the project derives physical % from steps.
    pub(crate) fn steps_drive_percent(&self, task_id: i64, pid: i64) -> bool {
        openxer_xer::edit::activity::steps_drive_percent(&self.project, task_id, pid)
    }

    pub(super) fn status_tab(&mut self, ui: &mut egui::Ui, i: usize) {
        use setters::Edit;
        let (task_id, pid) = self.ids(i);
        let origin = self.project.model.origin_serial;
        let data_date = self.project.model.project.data_date;
        let a = self.project.model.activities[i].clone();
        let d = self
            .project
            .details
            .get(&task_id)
            .cloned()
            .unwrap_or_default();
        let calendar = self.project.model.calendars.get(a.cal).cloned();
        let periods = format::activity_periods(&self.project, i);
        let elapsed = |start: Option<i64>, finish: Option<i64>| -> i64 {
            match (start, finish.or(data_date), &calendar) {
                (Some(s), Some(f), Some(cal)) if f > s => cal.between(s, f).max(0),
                _ => 0,
            }
        };
        let actual = elapsed(a.act_start, a.act_finish);
        let serial = self.project_serial();
        let (percent_type, suspend_raw, figures, steps_percent) =
            (*self.memo(serial, "detail_status", (task_id, pid), || {
                (
                    records::task_value(&self.project.document, task_id, pid, "complete_pct_type"),
                    records::task_value(&self.project.document, task_id, pid, "suspend_date"),
                    totals(&self.project.document, task_id, pid),
                    self.steps_drive_percent(task_id, pid),
                )
            }))
            .clone();
        let mut suspend = openxer_xer::parse_relative(origin, &suspend_raw).unwrap_or(None);
        let status = a.status;
        let original = d.original_duration;
        let rem = a.rem;
        let percent = d.percent;
        // Every change goes through the setters shared with the table.
        let mut edits: Vec<Edit> = vec![];
        let mut error = None;
        let content = ui.max_rect();
        let o = content.min - Vec2::new(form::PAD, form::CONTENT_TOP);
        let page_w = content.width() + 2. * form::PAD;
        let bottom = content.bottom() - o.y;
        let at = |x: f32, y: f32, w: f32, h: f32| form::rect(o.x + x, o.y + y, w, h);
        let p = ui.painter().clone();
        let l = StatusLayout::new(page_w);
        let (c1, c2, c3, mid_w) = (l.c1, l.c2, l.c3, l.mid_w);
        let formats = format::current();
        let duration = |ui: &mut egui::Ui,
                        key: &str,
                        y: f32,
                        minutes: i64,
                        enabled: bool,
                        error: &mut Option<String>|
         -> (egui::Response, Option<i64>) {
            let shown = formats.duration(minutes, periods);
            let id = ui.id().with(("status_duration", key));
            let (response, typed) = form::commit_edit(
                ui,
                id,
                at(l.dur_field_x, y, l.dur_field_w, 21.),
                &shown,
                enabled,
                Align::Max,
                6.,
            );
            let value = match typed.map(|t| formats.parse_duration(&t, periods)) {
                Some(Ok(m)) if m != minutes => Some(m),
                Some(Err(e)) => {
                    *error = Some(e);
                    None
                }
                _ => None,
            };
            (response, value)
        };
        // Duration.
        form::section(&p, at(c1, 33., l.dur_w, 145.), "Duration", 21., 7.);
        form::section(&p, at(c1, 183., l.dur_w, bottom - 183.), "", 0., 0.);
        for (label, y) in [
            ("Original", 60.),
            ("Actual", 88.),
            ("Remaining", 116.),
            ("At Complete", 144.),
            ("Total Float", 190.),
            ("Free Float", 219.),
        ] {
            form::label(&p, o.x + c1 + 17., o.y + y, label, true);
        }
        let open = |field| self.field_lock(i, field).is_ok();
        let original_enabled = open(Field::OriginalDuration);
        let remaining_enabled = open(Field::RemainingDuration);
        let (response, typed) =
            duration(ui, "original", 60., original, original_enabled, &mut error);
        self.action("detail_original", &response);
        if let Some(m) = typed {
            edits.push(Edit::OriginalDuration(m));
        }
        let readonly = |y: f32, text: &str| {
            form::value(
                &p,
                at(l.dur_field_x, y, l.dur_field_w, 21.),
                text,
                true,
                form::READONLY,
            );
        };
        readonly(88., &format::duration(actual, periods));
        let (response, typed) = duration(ui, "remaining", 116., rem, remaining_enabled, &mut error);
        self.action("detail_remaining", &response);
        if let Some(m) = typed {
            edits.push(Edit::RemainingDuration(m));
        }
        readonly(144., &format::duration(actual + rem, periods));
        // A completed activity has no float.
        let done = status == StatusType::Completed;
        let float = |v: Option<i64>| {
            if done {
                String::new()
            } else {
                format::float(v, periods)
            }
        };
        readonly(190., &float(a.total_float));
        readonly(219., &float(a.free_float));
        // Status.
        form::section(&p, at(c2, 33., mid_w, 121.), "Status", 21., 18.);
        let mut started = status != StatusType::NotStarted;
        let response = form::checkbox(
            ui,
            ui.id().with("detail_started"),
            o.x + c2 + 23.,
            o.y + 60.,
            "Started",
            &mut started,
            true,
        );
        self.action("detail_started", &response);
        if response.changed() {
            edits.push(Edit::Status(if started {
                StatusType::InProgress
            } else {
                StatusType::NotStarted
            }));
        }
        // Not started: the Start and Finish, typed as in the table's cells
        // (a typed Start asks about holding it, `start_hold`). Started: the
        // actual start; an unchecked Finished box shows, grayed, the date
        // that checking it records.
        let not_started = status == StatusType::NotStarted;
        let planned_finish = Some(self.default_actual_finish(i));
        let mut act_start = if not_started {
            a.early_start.or(Some(self.default_actual_start(i)))
        } else {
            a.act_start
        };
        match self.detail_date_field(
            ui,
            &format!("{i}-act_start"),
            at(l.field1_x, 60., l.field1_w, 21.),
            &mut act_start,
            true,
            (
                i,
                if not_started {
                    Field::Start
                } else {
                    Field::ActualStart
                },
            ),
        ) {
            Ok(true) if not_started => {
                if let Some(v) = act_start {
                    edits.push(Edit::Start(v));
                }
            }
            Ok(true) => edits.push(Edit::ActualStart(act_start)),
            Ok(false) => {}
            Err(e) => error = Some(e),
        }
        let mut finished = status == StatusType::Completed;
        let response = form::checkbox(
            ui,
            ui.id().with("detail_finished"),
            o.x + c2 + 23.,
            o.y + 88.,
            "Finished",
            &mut finished,
            true,
        );
        self.action("detail_finished", &response);
        if response.changed() {
            edits.push(Edit::Status(if finished {
                StatusType::Completed
            } else {
                StatusType::InProgress
            }));
        }
        let mut act_finish = if status == StatusType::Completed {
            a.act_finish
        } else if not_started {
            a.early_finish.or(planned_finish)
        } else {
            planned_finish
        };
        let finish_open = not_started && self.field_lock(i, Field::Finish).is_ok();
        match self.detail_date_field(
            ui,
            &format!("{i}-act_finish"),
            at(l.field1_x, 88., l.field1_w, 21.),
            &mut act_finish,
            status == StatusType::Completed || finish_open,
            (
                i,
                if not_started {
                    Field::Finish
                } else {
                    Field::ActualFinish
                },
            ),
        ) {
            Ok(true) if not_started => {
                if let Some(v) = act_finish {
                    edits.push(Edit::Finish(v));
                }
            }
            Ok(true) => edits.push(Edit::ActualFinish(act_finish)),
            Ok(false) => {}
            Err(e) => error = Some(e),
        }
        form::label(&p, o.x + c2 + 23., o.y + 116., "Exp Finish", true);
        let mut expect_end = a.expect_end;
        match self.detail_date_field(
            ui,
            &format!("{i}-expect_end"),
            at(l.field1_x, 116., l.field1_w, 21.),
            &mut expect_end,
            status != StatusType::Completed,
            (i, Field::ExpectedFinish),
        ) {
            Ok(true) => edits.push(Edit::ExpectedFinish(expect_end)),
            Ok(false) => {}
            Err(e) => error = Some(e),
        }
        let can_suspend = self.field_lock(i, Field::SuspendDate).is_ok();
        let second = l.field2_x;
        let percent_label = match percent_type.as_str() {
            "CP_Phys" => "Physical %",
            "CP_Units" => "Units %",
            _ => "Duration %",
        };
        for (label, y) in [(percent_label, 60.), ("Suspend", 88.), ("Resume", 116.)] {
            form::label(&p, o.x + l.label2_x, o.y + y, label, true);
        }
        let pct_text = |pct: f64| format!("{}%", format_number((pct * 100.).round() / 100.));
        match percent_type.as_str() {
            "CP_Phys" => {
                let id = ui.id().with("detail_physical_percent");
                let (response, typed) = form::commit_edit(
                    ui,
                    id,
                    at(second, 60., l.field2_w, 21.),
                    &pct_text(percent),
                    !steps_percent,
                    Align::Max,
                    6.,
                );
                self.action("detail_physical_percent", &response);
                response.on_disabled_hover_text("Calculated from activity steps.");
                if let Some(text) = typed {
                    match text.trim().trim_end_matches('%').trim().parse::<f64>() {
                        Ok(v) if v != percent => edits.push(Edit::PhysicalPercent(v)),
                        Ok(_) => {}
                        Err(_) => error = Some("Enter a percent from 0 to 100.".into()),
                    }
                }
            }
            "CP_Units" => {
                let all =
                    figures.labor_units.at_completion() + figures.nonlabor_units.at_completion();
                let done = figures.labor_units.actual + figures.nonlabor_units.actual;
                let pct = if all > 0. { done / all * 100. } else { 0. };
                form::value(
                    &p,
                    at(second, 60., l.field2_w, 21.),
                    &pct_text(pct),
                    true,
                    form::READONLY,
                );
            }
            _ => {
                let pct = if original > 0 {
                    ((original - rem) as f64 / original as f64 * 100.).clamp(0., 100.)
                } else if status == StatusType::Completed {
                    100.
                } else {
                    0.
                };
                // Editable where the table's Duration % Complete cell is,
                // through the same setter.
                let editable = self.field_lock(i, Field::DurationPercent).is_ok();
                let id = ui.id().with("detail_duration_percent");
                let (response, typed) = form::commit_edit(
                    ui,
                    id,
                    at(second, 60., l.field2_w, 21.),
                    &pct_text(pct),
                    editable,
                    Align::Max,
                    6.,
                );
                self.action("detail_duration_percent", &response);
                if let Some(text) = typed {
                    match text.trim().trim_end_matches('%').trim().parse::<f64>() {
                        Ok(v) if (v - pct).abs() > 1e-9 => edits.push(Edit::DurationPercent(v)),
                        Ok(_) => {}
                        Err(_) => error = Some("Enter a percent from 0 to 100.".into()),
                    }
                }
            }
        }
        match self.detail_date_field(
            ui,
            &format!("{i}-suspend"),
            at(second, 88., l.field2_w, 21.),
            &mut suspend,
            can_suspend,
            (i, Field::SuspendDate),
        ) {
            Ok(true) => edits.push(Edit::SuspendDate(suspend)),
            Ok(false) => {}
            Err(e) => error = Some(e),
        }
        let mut resume = a.resume;
        match self.detail_date_field(
            ui,
            &format!("{i}-resume"),
            at(second, 116., l.field2_w, 21.),
            &mut resume,
            can_suspend,
            (i, Field::ResumeDate),
        ) {
            Ok(true) => edits.push(Edit::ResumeDate(resume)),
            Ok(false) => {}
            Err(e) => error = Some(e),
        }
        // Constraints.
        form::section(
            &p,
            at(c2, 159., mid_w, bottom - 159.),
            "Constraints",
            21.,
            18.,
        );
        let labels: Vec<String> = (0..=9)
            .map(|n| match CstrType::from_code(n).unwrap() {
                CstrType::None => "< None >".to_string(),
                t => constraint_label(t).to_string(),
            })
            .collect();
        let secondary_enabled = a.cstr != CstrType::None;
        for (label, x, secondary, slot, enabled) in [
            ("Primary", c2 + 23., false, (a.cstr, a.cstr_date), true),
            (
                "Secondary",
                l.label2_x,
                true,
                (a.cstr2, a.cstr_date2),
                secondary_enabled,
            ),
        ] {
            let key = if secondary { "secondary" } else { "primary" };
            let (field_x, width) = if secondary {
                (l.field2_x, l.field2_w)
            } else {
                (l.field1_x, l.field1_w)
            };
            form::label(&p, o.x + x, o.y + 186., label, true);
            form::label(&p, o.x + x, o.y + 214., "Date", true);
            let current = (0..=9).position(|n| CstrType::from_code(n) == Some(slot.0));
            let (response, picked) = form::combo(
                ui,
                ui.id().with(("constraint", key)),
                at(field_x, 186., width, 22.),
                &labels,
                current,
                enabled,
            );
            self.action(&format!("detail_constraint_{key}"), &response);
            if let Some(n) = picked {
                let kind = CstrType::from_code(n as i64).unwrap();
                if kind != slot.0 {
                    edits.push(Edit::Constraint { secondary, kind });
                }
            }
            let mut date = slot.1;
            match self.detail_date_field(
                ui,
                &format!("{i}-{key}"),
                at(field_x, 214., width, 21.),
                &mut date,
                slot.0 != CstrType::None,
                (
                    i,
                    if secondary {
                        Field::SecondaryConstraintDate
                    } else {
                        Field::PrimaryConstraintDate
                    },
                ),
            ) {
                Ok(true) => edits.push(Edit::ConstraintDate { secondary, date }),
                Ok(false) => {}
                Err(e) => error = Some(e),
            }
        }
        // Units (Labor Units by default; the caption bar picks the measure).
        let mode = self.detail.units_mode;
        let bar = form::section(
            &p,
            at(c3, 33., l.units_w, bottom - 33.),
            mode.caption(),
            18.,
            26.,
        );
        form::section_chevron(&p, Pos2::new(o.x + c3 + 12., o.y + 41.));
        let bar_response = ui.interact(bar, ui.id().with("status_units_mode"), Sense::click());
        self.action("detail_units_mode", &bar_response);
        egui::Popup::menu(&bar_response)
            .style(chrome::popup_menu_style)
            .show(|ui| {
                for m in UnitsMode::ALL {
                    let item =
                        chrome::menu_item(ui, chrome::Item::new(m.caption()).checked(m == mode));
                    if item.clicked() {
                        self.detail.units_mode = m;
                        ui.close();
                    }
                }
            });
        let (values, cost) = match mode {
            UnitsMode::LaborUnits => (figures.labor_units, false),
            UnitsMode::NonlaborUnits => (figures.nonlabor_units, false),
            UnitsMode::LaborCost => (figures.labor_cost, true),
            UnitsMode::NonlaborCost => (figures.nonlabor_cost, true),
            UnitsMode::MaterialCost => (figures.material_cost, true),
        };
        let measure = mode.measure();
        for (n, ((label, y), value)) in [
            ("Budgeted", 60.),
            ("Actual", 88.),
            ("Remaining", 116.),
            ("At Complete", 144.),
        ]
        .into_iter()
        .zip(values.values())
        .enumerate()
        {
            form::label(&p, o.x + c3 + 20., o.y + y, label, true);
            let text = if cost {
                money(value)
            } else {
                format::units(value)
            };
            let field = at(l.units_field_x, y, l.units_field_w, 21.);
            // At Complete units and cost are typed (the remaining takes the
            // difference). By status, as observed: Actual is read only
            // before the start, Remaining and At Complete once completed.
            let phase = Phase::ALL[n];
            let status = self.project.model.activities[i].status;
            let lock = match (phase, status) {
                (Phase::Actual, StatusType::NotStarted) => {
                    Err("The activity has not started.".to_string())
                }
                (Phase::Remaining | Phase::AtCompletion, StatusType::Completed) => {
                    Err("The activity is completed.".to_string())
                }
                _ => self.figure_lock(i, measure, Some(phase)),
            };
            let id = ui.id().with(("status_units", n));
            let (response, typed) =
                form::commit_edit(ui, id, field, &text, lock.is_ok(), Align::Max, 6.);
            let key = label.to_lowercase().replace(' ', "_");
            self.action(&format!("detail_units_{key}"), &response);
            if let Err(reason) = &lock {
                response.on_disabled_hover_text(reason);
            }
            if let Some(t) = typed {
                let kind = if cost { Kind::Cost } else { Kind::Units };
                match crate::grid::columns::parse_input(&self.project, Some(i), kind, &t) {
                    Ok(Value::Units(v) | Value::Cost(v)) if v >= 0. => {
                        edits.push(Edit::Figure {
                            measure,
                            phase,
                            value: v,
                        });
                    }
                    Ok(Value::Empty) => edits.push(Edit::Figure {
                        measure,
                        phase,
                        value: 0.,
                    }),
                    Ok(_) => error = Some(format!("{label} must be zero or more.")),
                    Err(e) => error = Some(e),
                }
            }
        }
        if !edits.is_empty() {
            self.unpin_serial();
        }
        for edit in edits {
            let result = match edit {
                // The same question the table's Start cell asks.
                Edit::Start(v) => self.type_start(i, v),
                edit => self.edit_activity(i, edit),
            };
            if let Err(e) = result {
                error = Some(e);
            }
        }
        if let Some(e) = error {
            self.error(e);
        }
    }

    pub(super) fn relationship_tab(&mut self, ui: &mut egui::Ui, i: usize, which: Option<bool>) {
        match which {
            Some(predecessor) => {
                let size = Vec2::new(
                    ui.available_width(),
                    ui.available_height() - form::BUTTON_BAR,
                );
                self.relationship_grid(ui, i, predecessor, 0, size, true);
            }
            None => {
                // Two captioned panels split 873 : 947 in the reference with a
                // 6 px splitter between them; each holds a grid and its buttons.
                // The splitter moves; each panel keeps room for its buttons.
                let content = ui.max_rect();
                let p = ui.painter().clone();
                let left_w = self.detail_split(
                    ui,
                    "relationships",
                    content,
                    (content.width() * 873. / 1824.).round(),
                    [RELATIONSHIP_PANEL_MIN, RELATIONSHIP_PANEL_MIN],
                );
                let panels = [
                    Rect::from_min_max(
                        content.min,
                        Pos2::new(content.left() + left_w, content.bottom()),
                    ),
                    Rect::from_min_max(
                        Pos2::new(content.left() + left_w + 4., content.top()),
                        content.max,
                    ),
                ];
                for (slot, predecessor) in [(0, true), (1, false)] {
                    let r = panels[slot];
                    let (caption, x) = if predecessor {
                        ("Predecessors", 18.)
                    } else {
                        ("Successors", 7.)
                    };
                    form::section(&p, r, caption, 21., x);
                    let inner = Rect::from_min_max(
                        Pos2::new(r.left() + 6., r.top() + 27.),
                        Pos2::new(r.right() - 6., r.bottom() - 6.),
                    );
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(inner)
                            .layout(Layout::top_down(Align::LEFT)),
                    );
                    child.set_clip_rect(inner.intersect(ui.clip_rect()));
                    child.spacing_mut().item_spacing = Vec2::new(form::GAP, form::GAP);
                    let size = Vec2::new(inner.width(), inner.height() - form::BUTTON_BAR);
                    self.relationship_grid(&mut child, i, predecessor, slot, size, false);
                }
                form::vsplitter(
                    &p,
                    content.left() + left_w - 1.,
                    content.top(),
                    content.bottom(),
                );
            }
        }
    }

    fn relationship_grid(
        &mut self,
        ui: &mut egui::Ui,
        activity: usize,
        predecessor: bool,
        slot: usize,
        size: Vec2,
        own_tab: bool,
    ) {
        let shown: Vec<usize> = self
            .project
            .model
            .links
            .iter()
            .enumerate()
            .filter(|(_, l)| {
                if predecessor {
                    l.succ == activity
                } else {
                    l.pred == activity
                }
            })
            .map(|(index, _)| index)
            .collect();
        let driving: BTreeMap<usize, Option<bool>> = shown
            .iter()
            .copied()
            .zip(driving_flags(&mut self.project.model, &shown))
            .collect();
        let comment_of: BTreeMap<usize, String> = shown
            .iter()
            .copied()
            .zip(self.project.relationship_comments(&shown))
            .collect();
        let definition = self.critical_definition();
        let model = &self.project.model;
        let origin = model.origin_serial;
        let leveled = model.options.do_level;
        struct Line {
            index: usize,
            other: usize,
            kind: LinkType,
            lag: i64,
            periods: format::Periods,
            cells: [String; 11],
            /// Hover text of the Start and Finish cells (a constrained date).
            tips: [Option<String>; 2],
            /// A relationship with another project: (owner project, row id).
            cross: Option<(i64, i64)>,
            /// The other activity in another open project: (workspace id, index).
            goto: Option<(i64, usize)>,
        }
        let lines: Vec<Line> = model
            .links
            .iter()
            .enumerate()
            .filter(|(_, l)| {
                if predecessor {
                    l.succ == activity
                } else {
                    l.pred == activity
                }
            })
            .map(|(index, l)| {
                let other = if predecessor { l.pred } else { l.succ };
                let a = &model.activities[other];
                let drives = driving.get(&index).copied().flatten();
                let (start, start_tip) = marked_date(origin, a, leveled, End::Start);
                let (finish, finish_tip) = marked_date(origin, a, leveled, End::Finish);
                let both_critical =
                    crate::grid::columns::is_critical(&model.activities[l.pred], definition)
                        && crate::grid::columns::is_critical(&model.activities[l.succ], definition);
                Line {
                    index,
                    other,
                    kind: l.link_type,
                    lag: l.lag,
                    periods: format::lag_periods(&self.project, l.pred, l.succ),
                    cells: [
                        a.task_code.clone(),
                        self.project.name(other).to_string(),
                        status_label(a.status).into(),
                        start,
                        finish,
                        yes_no(drives),
                        yes_no(drives.map(|d| d && both_critical)),
                        self.project.model.project.name.clone(),
                        self.project
                            .details
                            .get(&a.task_id)
                            .and_then(|d| self.project.wbs.iter().find(|w| w.id == d.wbs))
                            .map(|w| w.code.clone())
                            .unwrap_or_default(),
                        {
                            let doc = &self.project.document;
                            let primary = num(&records::task_value(
                                doc,
                                a.task_id,
                                self.project.model.project.proj_id,
                                "rsrc_id",
                            ));
                            if primary == 0 {
                                String::new()
                            } else {
                                self.project
                                    .resource_names
                                    .get(&primary)
                                    .cloned()
                                    .unwrap_or_default()
                            }
                        },
                        comment_of.get(&index).cloned().unwrap_or_default(),
                    ],
                    tips: [start_tip, finish_tip],
                    cross: None,
                    goto: None,
                }
            })
            .collect();
        let mut lines = lines;
        if self.open_set.multi() || !self.project.external_links().is_empty() {
            let periods = format::activity_periods(&self.project, activity);
            let projects = self.open_projects();
            for (k, x) in self
                .cross_lines(activity, predecessor)
                .into_iter()
                .enumerate()
            {
                let other = x.other.map(|(_, slot, i)| (projects[slot], i));
                let mut cells: [String; 11] = Default::default();
                let mut tips: [Option<String>; 2] = Default::default();
                match other {
                    Some((p, i)) => {
                        let a = &p.model.activities[i];
                        let origin = p.model.origin_serial;
                        let leveled = p.model.options.do_level;
                        cells[0] = a.task_code.clone();
                        cells[1] = p.name(i).to_string();
                        cells[2] = status_label(a.status).into();
                        (cells[3], tips[0]) = marked_date(origin, a, leveled, End::Start);
                        (cells[4], tips[1]) = marked_date(origin, a, leveled, End::Finish);
                        cells[8] = p
                            .details
                            .get(&a.task_id)
                            .and_then(|d| p.wbs.iter().find(|w| w.id == d.wbs))
                            .map(|w| w.code.clone())
                            .unwrap_or_default();
                    }
                    None => cells[0] = x.other_task.to_string(),
                }
                cells[7] = x.other_project.clone();
                cells[10] = projects
                    .iter()
                    .find(|p| p.model.project.proj_id == x.owner)
                    .map(|p| p.relationship_comment_of_row(x.link.row_id))
                    .unwrap_or_default();
                lines.push(Line {
                    index: usize::MAX - k,
                    other: activity,
                    kind: x.link.link_type,
                    lag: x.link.lag,
                    periods,
                    cells,
                    tips,
                    cross: Some((x.owner, x.link.row_id)),
                    goto: x.other.map(|(id, _, i)| (id, i)),
                });
            }
        }
        let selected = self.detail.rows[slot].filter(|&r| r < lines.len());
        let mut edit: Option<(usize, LinkType, f64)> = None;
        let mut cross_edit: Option<(i64, i64, LinkType, i64)> = None;
        let mut lag_field: Option<egui::Response> = None;
        let mut lag_error: Option<String> = None;
        let mut menu_action = None;
        let id = format!("relationship_grid_{slot}_{predecessor}");
        // Each tab's own columns: the Predecessors and Successors tabs and
        // the two halves of the Relationships tab choose theirs apart.
        use super::grid_columns as gc;
        let columns = match (own_tab, predecessor) {
            (true, true) => &gc::PREDECESSORS,
            (true, false) => &gc::SUCCESSORS,
            (false, true) => &gc::RELATIONSHIP_PREDECESSORS,
            (false, false) => &gc::RELATIONSHIP_SUCCESSORS,
        };
        let shown = self.detail_columns(columns, self.detail.all_relationship_columns);
        let mut customize = false;
        let mut all_columns = self.detail.all_relationship_columns;
        let out = grid_columns(
            ui,
            &id,
            &shown,
            lines.len(),
            selected,
            size,
            &mut |ui, event, sel| match event {
                Ev::Cell(r, c) => {
                    let line = &lines[r];
                    match c {
                        0 => cell(ui, &line.cells[0]),
                        1 => cell(ui, &line.cells[1]),
                        2 if sel => {
                            editor_cell(ui);
                            let mut kind = line.kind;
                            let w = ui.available_width().clamp(36., 58.);
                            crate::dialog_kit::flow_combo_with(
                                ui,
                                ("tie_type", line.index),
                                w,
                                &kind.as_str()[3..],
                                true,
                                |ui| {
                                    for value in LINK_TYPES {
                                        let on = kind == value;
                                        if crate::dialog_kit::flow_selectable(
                                            ui,
                                            on,
                                            &value.as_str()[3..],
                                            w - 4.,
                                        )
                                        .clicked()
                                        {
                                            kind = value;
                                            ui.close();
                                        }
                                    }
                                },
                            );
                            if kind != line.kind {
                                match line.cross {
                                    Some((owner, row)) => {
                                        cross_edit = Some((owner, row, kind, line.lag))
                                    }
                                    None => edit = Some((line.index, kind, line.lag as f64 / 60.)),
                                }
                            }
                        }
                        2 => cell(ui, &line.kind.as_str()[3..]),
                        3 if sel => {
                            // Typed and committed on Enter or focus loss: one
                            // change, one undo step (no drag to change it).
                            editor_cell(ui);
                            let shown = format::duration(line.lag, line.periods);
                            let (response, typed) = form::commit_edit(
                                ui,
                                ui.id().with(("lag", line.index, line.cross)),
                                ui.max_rect(),
                                &shown,
                                true,
                                Align::Max,
                                4.,
                            );
                            lag_field = Some(response);
                            let typed = typed.map(|text| {
                                format::parse_duration(&text, line.periods).and_then(|lag| {
                                    if lag.abs() <= 100000 * 60 {
                                        Ok(lag)
                                    } else {
                                        Err(format!(
                                            "Lag must be within {}.",
                                            format::duration(100000 * 60, line.periods)
                                        ))
                                    }
                                })
                            });
                            match typed {
                                Some(Ok(lag)) if lag != line.lag => match line.cross {
                                    Some((owner, row)) => {
                                        cross_edit = Some((owner, row, line.kind, lag))
                                    }
                                    None => edit = Some((line.index, line.kind, lag as f64 / 60.)),
                                },
                                Some(Err(e)) => lag_error = Some(e),
                                _ => {}
                            }
                        }
                        3 => cell(ui, format::duration(line.lag, line.periods)),
                        5 | 6 => {
                            let shown = ui.add(
                                egui::Label::new(line.cells[c - 2].as_str())
                                    .truncate()
                                    .selectable(false),
                            );
                            if let Some(tip) = &line.tips[c - 5] {
                                shown.on_hover_text(tip);
                            }
                        }
                        c => cell(ui, &line.cells[c - 2]),
                    }
                }
                Ev::Menu(row) => {
                    customize |= super::grid_columns::menu_item(ui, columns);
                    let mut menu =
                        crate::context_menu::Menu::on(ui, &format!("{}_grid_menu", columns.key));
                    if menu.check("show_all", "Show All Relationship Columns", all_columns) {
                        all_columns = !all_columns;
                    }
                    menu.separator();
                    // Assign is Edit > Assign > Predecessors / Successors
                    // and the tab's button; the reference menu has only
                    // the Customize item, so it is not repeated here.
                    if menu.item_if("remove", chrome::Item::new("Remove"), row.is_some()) {
                        menu_action = Some((1, row));
                    }
                    if menu.item_if("goto", chrome::Item::new("GoTo"), row.is_some()) {
                        menu_action = Some((2, row));
                    }
                }
            },
        );
        if let Some(r) = out.clicked {
            self.detail.rows[slot] = Some(r);
        }
        if customize {
            self.open_column_customize(columns);
        }
        let mut action = menu_action.map(|(a, row)| (a, row.or(selected)));
        if let Some(r) = out.double {
            action = Some((2, Some(r)));
        }
        self.detail.all_relationship_columns = all_columns;
        form::button_row(ui, |ui| {
            let key = if predecessor { "pred" } else { "succ" };
            let assign = form::add_button(ui, "Assign", true);
            self.action(&format!("detail_{key}_assign_{slot}"), &assign);
            if assign.clicked() {
                action = Some((0, None));
            }
            let remove = form::command_button(ui, "Remove", None, selected.is_some());
            self.action(&format!("detail_{key}_remove_{slot}"), &remove);
            if remove.clicked() {
                action = Some((1, selected));
            }
            let goto = form::command_button(ui, "GoTo", None, selected.is_some());
            self.action(&format!("detail_{key}_goto_{slot}"), &goto);
            if goto.clicked() {
                action = Some((2, selected));
            }
        });
        if let Some(response) = lag_field {
            let key = if predecessor { "pred" } else { "succ" };
            self.action(&format!("detail_{key}_lag_{slot}"), &response);
        }
        if let Some(e) = lag_error {
            self.error(e);
        }
        if let Some((owner, row, kind, lag)) = cross_edit {
            if let Err(error) = self.change_cross_link(owner, row, Some((kind, lag))) {
                self.status = error;
            }
        }
        if let Some((index, kind, hours)) = edit {
            let previous = self
                .project
                .model
                .links
                .get(index)
                .map(|l| (l.link_type, l.lag));
            match relationships::edit(&mut self.project.model, index, kind, hours) {
                Ok(()) => {
                    self.dirty = true;
                    self.check_relationship_sequence(index, previous);
                }
                Err(error) => self.status = error,
            }
        }
        match action {
            Some((0, _)) => {
                self.detail.picker = Some(Picker::Relationship {
                    predecessor,
                    query: String::new(),
                    selected: None,
                    kind: LinkType::FS,
                    lag: 0.,
                })
            }
            // Remove asks first (`details::removals`).
            Some((1, Some(r))) if r < lines.len() && lines[r].cross.is_some() => {
                let (owner, row_id) = lines[r].cross.expect("checked");
                self.detail.removal = Some(super::removals::Removal::CrossRelationship {
                    owner,
                    row_id,
                    slot,
                });
            }
            Some((2, Some(r))) if r < lines.len() && lines[r].cross.is_some() => {
                if let Some((id, i)) = lines[r].goto {
                    self.goto_open_activity(id, i);
                }
            }
            Some((1, Some(r))) if r < lines.len() => {
                self.ask_remove_relationship(lines[r].index, slot);
            }
            Some((2, Some(r))) if r < lines.len() => self.jump_to(lines[r].other),
            _ => {}
        }
    }

    pub(super) fn relationship_picker(&mut self, ctx: &egui::Context, picker: &mut Picker) -> bool {
        let Picker::Relationship {
            predecessor,
            query,
            selected,
            kind,
            lag,
        } = picker
        else {
            return false;
        };
        // The New Activity wizard picks relationships for an activity it has
        // not made yet: the pick goes to the wizard.
        let wizard = self.wizard_picks(crate::activity_wizard::Pick::Relationship);
        let selected_activity = if wizard {
            Some(usize::MAX)
        } else {
            self.selected
                .filter(|&i| i < self.project.model.activities.len())
        };
        let Some(activity) = selected_activity else {
            return false;
        };
        // Every selected activity gets the relationship; none is its own candidate.
        let targets = if wizard {
            vec![]
        } else {
            self.assign_targets()
        };
        let time = ctx.input(|i| i.time);
        // Activities of every open project (indices into `shown()`; the
        // active project's come first at their own indices), under the bands
        // and in the order of the activity table. The lines are kept while
        // the search text, the project, Group and Sort and the selection stay
        // as they are; with several projects open they are built each frame.
        let multi = self.open_set.multi();
        let active_rows = self.active_rows();
        let filter = self.detail.picker_filter;
        let group_mode = self.detail.picker_group;
        let extra_cols: Vec<PickerColumn> = self.detail.picker_columns.iter().copied().collect();
        // Find looks inside folded bands too.
        let searching = !query.trim().is_empty();
        let folded = if searching {
            BTreeSet::new()
        } else {
            self.detail.picker_collapsed.clone()
        };
        let group_override = match group_mode {
            PickerGroup::Wbs => None,
            PickerGroup::List => {
                let mut g = self.grid.layout.group_sort.clone();
                g.levels.clear();
                Some(g)
            }
            PickerGroup::Status => {
                let mut g = self.grid.layout.group_sort.clone();
                g.levels = vec![crate::grid::layout::GroupLevel {
                    column: "status".into(),
                    to_level: None,
                    indent: true,
                    interval: String::new(),
                }];
                Some(g)
            }
        };
        let key = self.shown_serial().map(|serial| PickerLinesKey {
            query: query.clone(),
            collapsed: folded.clone(),
            serial,
            group_sort: self.grid.layout.group_sort.clone(),
            filter,
            group: group_mode,
            targets: std::iter::once(activity)
                .chain(targets.iter().copied())
                .collect(),
        });
        let kept = match (&key, &self.detail.picker_lines) {
            (Some(key), Some((built_for, lines, bands))) if key == built_for => {
                Some((lines.clone(), bands.clone()))
            }
            _ => None,
        };
        let (lines, all_band_keys) = kept.unwrap_or_else(|| {
            let shown_data = self.shown();
            let crit_def = self.critical_definition();
            let (built_lines, bands) =
                self.picker_lines_with(query, &folded, group_override.clone(), &|k| {
                    if k == activity || targets.contains(&k) {
                        return true;
                    }
                    let a = &shown_data.model.activities[k];
                    match filter {
                        PickerFilter::All => false,
                        PickerFilter::CurrentProject => active_rows.is_some_and(|n| k >= n),
                        PickerFilter::NotStarted => a.status != StatusType::NotStarted,
                        PickerFilter::InProgress => a.status != StatusType::InProgress,
                        PickerFilter::Completed => a.status != StatusType::Completed,
                        PickerFilter::Critical => !crate::grid::columns::is_critical(a, crit_def),
                    }
                });
            let lines = std::rc::Rc::new(built_lines);
            self.detail.picker_lines = key.map(|key| (key, lines.clone(), bands.clone()));
            (lines, bands)
        });
        let all = self.shown();
        let origin = all.model.origin_serial;
        let leveled = all.model.options.do_level;
        let projects: Vec<String> = self
            .open_projects()
            .iter()
            .map(|p| p.model.project.name.clone())
            .collect();
        let texts: Vec<Vec<String>> = lines
            .iter()
            .map(|line| match line {
                PickLine::Band { label, .. } => vec![label.clone()],
                PickLine::Activity(k) => {
                    let a = &all.model.activities[*k];
                    let mut row = Vec::with_capacity(3 + extra_cols.len());
                    if multi {
                        let project = self
                            .open_set
                            .composite
                            .as_ref()
                            .and_then(|c| c.origin.get(*k))
                            .and_then(|&(slot, _)| projects.get(slot).cloned())
                            .unwrap_or_default();
                        row.push(project);
                    }
                    row.push(a.task_code.clone());
                    row.push(all.name(*k).to_string());
                    let periods = format::activity_periods(all, *k);
                    for col in &extra_cols {
                        let value = match col {
                            PickerColumn::Status => match a.status {
                                StatusType::NotStarted => "Not Started".into(),
                                StatusType::InProgress => "In Progress".into(),
                                StatusType::Completed => "Completed".into(),
                            },
                            PickerColumn::OriginalDuration => {
                                let orig = all
                                    .details
                                    .get(&a.task_id)
                                    .map_or(a.rem, |d| d.original_duration);
                                format::duration(orig, periods)
                            }
                            PickerColumn::Start => {
                                format::date(origin, displayed_start(a, leveled))
                            }
                            PickerColumn::Finish => {
                                format::date(origin, displayed_finish(a, leveled))
                            }
                            PickerColumn::TotalFloat => a
                                .total_float
                                .map(|f| format::duration(f, periods))
                                .unwrap_or_default(),
                        };
                        row.push(value);
                    }
                    row
                }
            })
            .collect();
        // Band rows wear the table's band colors.
        let band_colors: Vec<Option<(Color32, Color32)>> = lines
            .iter()
            .map(|line| match line {
                PickLine::Band { level, .. } => {
                    let style = self.grid.layout.group_sort.band_style(*level);
                    let rgb = |c: [u8; 3]| Color32::from_rgb(c[0], c[1], c[2]);
                    Some((rgb(style.background), rgb(style.text)))
                }
                PickLine::Activity(_) => None,
            })
            .collect();
        // Left edge of the band row being painted (its first cell's).
        let band_left = std::cell::Cell::new(0.);
        // Where each band row's expand box is, for the click on it.
        let boxes = std::cell::RefCell::new(Vec::<(usize, Rect)>::new());
        let mut fold = None;
        let mut double = None;
        let mut expand_all = false;
        let mut collapse_all = false;
        let mut copy_rows = false;
        let mut open_table_font = false;
        let mut next_filter = None;
        let mut next_group = None;
        let mut toggle_col = None;
        let mut search_focused = false;
        let has_bands = !all_band_keys.is_empty() && !searching;
        let can_expand = has_bands
            && all_band_keys
                .iter()
                .any(|k| self.detail.picker_collapsed.contains(k));
        let can_collapse = has_bands
            && all_band_keys
                .iter()
                .any(|k| !self.detail.picker_collapsed.contains(k));
        const DIALOG: &str = "assign_relationship";
        let title = if *predecessor {
            "Assign Predecessors"
        } else {
            "Assign Successors"
        };
        let window = AssignWindow {
            id: DIALOG,
            title,
            buttons: vec![AssignButton {
                key: "picker_assign",
                caption: "Assign",
                enabled: selected.is_some(),
            }],
            close: "picker_close",
            help: "picker_help",
        };
        // A band is a caption, not a choice.
        let keys: Vec<Option<i64>> = lines
            .iter()
            .map(|line| match line {
                PickLine::Activity(k) => Some(*k as i64),
                PickLine::Band { .. } => None,
            })
            .collect();
        let mut columns: Vec<(&str, f32)> = if multi {
            vec![
                ("Project ID", 90.),
                ("Activity ID", 90.),
                ("Activity Name", 240.),
            ]
        } else {
            vec![("Activity ID", 90.), ("Activity Name", 240.)]
        };
        for col in &extra_cols {
            columns.push((col.label(), col.width()));
        }
        let extra_width: f32 = extra_cols.iter().map(|c| c.width()).sum();
        let list_width = (if multi { 530. } else { 440. } + extra_width).min(780.);
        let mut choice = selected.map(|k| k as i64);
        let (open, clicked) = self.assign_window(ctx, &window, |app, ui| {
            ui.horizontal(|ui| {
                ui.label("Find");
                let response = ui.add(egui::TextEdit::singleline(query).desired_width(220.));
                search_focused = response.has_focus();
                app.action("picker_search", &response);
                if targets.len() > 1 {
                    ui.label(format!(
                        "Assigns to the {} selected activities.",
                        targets.len()
                    ));
                }
            });
            let current_choice = choice;
            let out = app.assign_list_with_menu(
                ui,
                DIALOG,
                "relationship_picker",
                &columns,
                &keys,
                &mut choice,
                Vec2::new(list_width, 210.),
                &mut |ui, r, c, _| {
                    let Some((fill, ink)) = band_colors[r] else {
                        if let Some(t) = texts[r].get(c) {
                            cell(ui, t);
                        }
                        return;
                    };
                    // A band: its color across the row and its caption
                    // from the first column on, indented by its level,
                    // over the column lines.
                    let rect = ui.max_rect();
                    let (level, collapsed) = match &lines[r] {
                        PickLine::Band {
                            level, collapsed, ..
                        } => (*level, *collapsed),
                        PickLine::Activity(_) => (0, false),
                    };
                    let indent = 10. * level as f32;
                    ui.painter().rect_filled(rect, 0., fill);
                    if c == 0 {
                        band_left.set(rect.left());
                        // The table's expand box, centered on the cell.
                        let offset = ((rect.height() - 1. - 9.) / 2.).floor();
                        let offset = offset + offset % 2.;
                        let at = Pos2::new(rect.left() + 2. + indent, rect.top() + offset);
                        crate::grid::expand_box(ui.painter(), at, collapsed);
                        boxes
                            .borrow_mut()
                            .push((r, Rect::from_min_size(at, Vec2::splat(9.)).expand(3.)));
                    }
                    if c + 1 == columns.len() {
                        let clip = ui.clip_rect();
                        let row_clip =
                            Rect::from_min_max(Pos2::new(band_left.get(), clip.top()), clip.max);
                        ui.ctx()
                            .layer_painter(ui.layer_id())
                            .with_clip_rect(row_clip)
                            .text(
                                Pos2::new(band_left.get() + 16. + indent, rect.center().y),
                                Align2::LEFT_CENTER,
                                &texts[r][0],
                                form::font(),
                                ink,
                            );
                    }
                },
                &mut |ui, row| {
                    use crate::chrome::{Icon, Item};
                    let mut m = crate::context_menu::Menu::on(ui, "picker_menu");
                    let row_is_activity =
                        row.and_then(|r| keys.get(r)).is_some_and(Option::is_some);
                    let can_copy = current_choice.is_some() || row_is_activity;
                    if m.item_if(
                        "copy",
                        Item::new("Copy").key("Ctrl+C").icon(Icon::Copy),
                        can_copy,
                    ) {
                        copy_rows = true;
                    }
                    m.submenu("columns", Item::new("Columns").icon(Icon::Columns), |m| {
                        for col in PickerColumn::ALL {
                            if m.check(col.key(), col.label(), extra_cols.contains(&col)) {
                                toggle_col = Some(col);
                            }
                        }
                    });
                    if m.item(
                        "table_font",
                        Item::new("Table Font and Row...").icon(Icon::TableFont),
                    ) {
                        open_table_font = true;
                    }
                    m.separator();
                    m.submenu(
                        "filter_by",
                        Item::new("Filter By").icon(Icon::Filter),
                        |m| {
                            let choices = [
                                ("filter_all", "All Activities", PickerFilter::All),
                                (
                                    "filter_current_project",
                                    "Current Project's Activities",
                                    PickerFilter::CurrentProject,
                                ),
                                (
                                    "filter_not_started",
                                    "Not Started",
                                    PickerFilter::NotStarted,
                                ),
                                (
                                    "filter_in_progress",
                                    "In Progress",
                                    PickerFilter::InProgress,
                                ),
                                ("filter_completed", "Completed", PickerFilter::Completed),
                                ("filter_critical", "Critical", PickerFilter::Critical),
                            ];
                            for (k, label, f) in choices {
                                if m.check(k, label, filter == f) {
                                    next_filter = Some(f);
                                }
                            }
                        },
                    );
                    m.submenu(
                        "group_by",
                        Item::new("Group and Sort By").icon(Icon::GroupSort),
                        |m| {
                            let choices = [
                                ("group_wbs", "WBS", PickerGroup::Wbs),
                                ("group_list", "List", PickerGroup::List),
                                ("group_status", "Activity Status", PickerGroup::Status),
                            ];
                            for (k, label, g) in choices {
                                if m.check(k, label, group_mode == g) {
                                    next_group = Some(g);
                                }
                            }
                        },
                    );
                    m.separator();
                    if m.item_if(
                        "expand_all",
                        Item::new("Expand All")
                            .key("Ctrl+Num +")
                            .icon(Icon::ExpandAll),
                        can_expand,
                    ) {
                        expand_all = true;
                    }
                    if m.item_if(
                        "collapse_all",
                        Item::new("Collapse All")
                            .key("Ctrl+Num -")
                            .icon(Icon::CollapseAll),
                        can_collapse,
                    ) {
                        collapse_all = true;
                    }
                },
            );
            double = out.double;
            // A click on a band's box, or a double click on the band, folds
            // or unfolds it.
            let pointer = ui.ctx().input(|i| i.pointer.interact_pos());
            let on_box = |r: usize| {
                boxes
                    .borrow()
                    .iter()
                    .any(|(row, b)| *row == r && pointer.is_some_and(|p| b.contains(p)))
            };
            let band_key = |r: usize| match lines.get(r) {
                Some(PickLine::Band { key, .. }) => Some(key.clone()),
                _ => None,
            };
            if let Some(r) = out.clicked.filter(|&r| on_box(r)) {
                fold = band_key(r);
            } else if let Some(r) = out.double_row.filter(|&r| !on_box(r)) {
                fold = band_key(r);
            }
            ui.add_space(4.);
            ui.horizontal(|ui| {
                ui.label("Relationship Type");
                crate::dialog_kit::flow_combo_with(
                    ui,
                    "picker_link_type",
                    50.,
                    &kind.as_str()[3..],
                    true,
                    |ui| {
                        for value in LINK_TYPES {
                            let on = *kind == value;
                            if crate::dialog_kit::flow_selectable(ui, on, &value.as_str()[3..], 46.)
                                .clicked()
                            {
                                *kind = value;
                                ui.close();
                            }
                        }
                    },
                );
                ui.label("Lag");
                let mut minutes = (*lag * 60.).round() as i64;
                let periods = format::activity_periods(&app.project, activity);
                let range = -100000 * 60..=100000 * 60;
                if format::duration_editor(ui, true, &mut minutes, periods, range).changed() {
                    *lag = minutes as f64 / 60.;
                }
            });
        });
        // Keyboard shortcuts for Expand All, Collapse All and Copy while open.
        if open && !search_focused {
            let (k_expand, k_collapse, k_copy) = ctx.input(|i| {
                let cmd = i.modifiers.command && !i.modifiers.alt;
                (
                    cmd && (i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals)),
                    cmd && i.key_pressed(Key::Minus),
                    cmd && i.key_pressed(Key::C),
                )
            });
            if k_expand && can_expand {
                expand_all = true;
            }
            if k_collapse && can_collapse {
                collapse_all = true;
            }
            if k_copy && choice.is_some() {
                copy_rows = true;
            }
        }
        *selected = choice.map(|k| k as usize);
        if expand_all {
            self.detail.picker_collapsed.clear();
        } else if collapse_all {
            self.detail.picker_collapsed = all_band_keys.into_iter().collect();
        } else if let Some(key) = fold {
            if !self.detail.picker_collapsed.remove(&key) {
                self.detail.picker_collapsed.insert(key);
            }
        }
        if let Some(f) = next_filter {
            self.detail.picker_filter = f;
        }
        if let Some(g) = next_group {
            if self.detail.picker_group != g {
                self.detail.picker_group = g;
                self.detail.picker_collapsed.clear();
            }
        }
        if let Some(col) = toggle_col {
            if !self.detail.picker_columns.remove(&col) {
                self.detail.picker_columns.insert(col);
            }
        }
        if copy_rows {
            let picked = self.assign_picked(DIALOG, choice);
            let mut out_lines = Vec::with_capacity(picked.len());
            for k in picked {
                if let Some(row_idx) = keys.iter().position(|x| *x == Some(k)) {
                    if let Some(cells) = texts.get(row_idx) {
                        out_lines.push(cells.join("\t"));
                    }
                }
            }
            if !out_lines.is_empty() {
                ctx.copy_text(out_lines.join("\n"));
                self.status = "Copied to clipboard".into();
            }
        }
        if open_table_font {
            self.open_table_font_for(crate::layout_opts::TableTarget::Activities);
        }
        // Assign takes every selected activity; a double click the one.
        let mut picks: Vec<usize> = match clicked {
            Some("picker_assign") => self
                .assign_picked(DIALOG, choice)
                .into_iter()
                .map(|k| k as usize)
                .collect(),
            _ => vec![],
        };
        if let Some(k) = double {
            picks = vec![k as usize];
        }
        let lag_minutes = (*lag * 60.).round() as i64;
        let own_rows = self.active_rows();
        let cross_any = picks.iter().any(|&k| own_rows.is_some_and(|n| k >= n));
        let many = picks.len() > 1 || targets.len() > 1;
        if picks.is_empty() {
            return open;
        }
        if wizard {
            for &other in &picks {
                self.wizard_take_link(other, *predecessor, *kind, lag_minutes);
            }
        } else if cross_any && many {
            self.status =
                "A relationship to another project is added to one activity at a time.".into();
        } else if many {
            let targets = if targets.is_empty() {
                vec![activity]
            } else {
                targets
            };
            self.link_bulk_many(&targets, &picks, *predecessor, *kind, lag_minutes, time);
        } else if cross_any {
            let other = picks[0];
            let (pred, succ) = if *predecessor {
                (other, activity)
            } else {
                (activity, other)
            };
            let result = match (self.activity_key(pred), self.activity_key(succ)) {
                (Some(p), Some(s)) => self.add_cross_link(p, s, *kind, lag_minutes),
                _ => Err("Activity not found".into()),
            };
            self.status = match result {
                Ok(()) => "Relationship to another project added.".into(),
                Err(error) => error,
            };
        } else {
            let other = picks[0];
            let (pred, succ) = if *predecessor {
                (other, activity)
            } else {
                (activity, other)
            };
            match relationships::add(&mut self.project.model, pred, succ, *kind, lag_minutes) {
                Ok(()) => {
                    self.dirty = true;
                    self.status = "Relationship added.".into();
                    self.check_relationship_sequence(self.project.model.links.len() - 1, None);
                }
                Err(error) => self.status = error,
            }
        }
        open
    }

    /// The Summary matrix: measures down the side, Budgeted to Complete
    /// Variance across on a 110 px pitch, and the units / cost / dates
    /// choice under it.
    pub(super) fn summary_tab(&mut self, ui: &mut egui::Ui, i: usize) {
        let (task_id, pid) = self.ids(i);
        let t = totals(&self.project.document, task_id, pid);
        let a = &self.project.model.activities[i];
        let d = self
            .project
            .details
            .get(&task_id)
            .cloned()
            .unwrap_or_default();
        let periods = format::activity_periods(&self.project, i);
        let origin = self.project.model.origin_serial;
        let mut mode = self.detail.summary_mode;
        let percent = |done: f64, all: f64| {
            let pct = if all > 0. { done / all * 100. } else { 0. };
            format!("{}%", format_number((pct * 100.).round() / 100.))
        };
        let measure = |label: &str, v: [f64; 4], show: &dyn Fn(f64) -> String| {
            vec![
                label.to_string(),
                show(v[0]),
                show(v[1]),
                show(v[2]),
                percent(v[1], v[3]),
                show(v[3]),
                show(v[0] - v[3]),
            ]
        };
        let columns: Vec<&str>;
        let rows: Vec<Vec<String>> = match mode {
            SummaryMode::Units => {
                columns = vec![
                    "Budgeted",
                    "Actual",
                    "Remaining",
                    "% Complete",
                    "At Completion",
                    "Complete Variance",
                ];
                let actual = a.act_start.map_or(0, |s| {
                    let end = a.act_finish.or(self.project.model.project.data_date);
                    match (end, self.project.model.calendars.get(a.cal)) {
                        (Some(f), Some(cal)) if f > s => cal.between(s, f).max(0),
                        _ => 0,
                    }
                });
                let duration = |m: f64| format::duration(m as i64, periods);
                let original = d.original_duration as f64;
                let at_completion = (actual + a.rem) as f64;
                let duration_row = vec![
                    "Duration".to_string(),
                    duration(original),
                    duration(actual as f64),
                    duration(a.rem as f64),
                    if a.is_completed() {
                        "100%".to_string()
                    } else {
                        percent((d.original_duration - a.rem).max(0) as f64, original)
                    },
                    duration(at_completion),
                    duration(original - at_completion),
                ];
                vec![
                    measure("Labor Units", t.labor_units.values(), &format::units),
                    measure("Nonlabor Units", t.nonlabor_units.values(), &format::units),
                    duration_row,
                ]
            }
            SummaryMode::Cost => {
                columns = vec![
                    "Budgeted",
                    "Actual",
                    "Remaining",
                    "% Complete",
                    "At Completion",
                    "Complete Variance",
                ];
                let parts = [
                    ("Labor Cost", t.labor_cost.values()),
                    ("Nonlabor Cost", t.nonlabor_cost.values()),
                    ("Material Cost", t.material_cost.values()),
                    ("Expenses", t.expenses.values()),
                ];
                let mut total = [0.; 4];
                for (_, v) in &parts {
                    for k in 0..4 {
                        total[k] += v[k];
                    }
                }
                let mut rows: Vec<Vec<String>> = parts
                    .iter()
                    .map(|(label, v)| measure(label, *v, &money))
                    .collect();
                rows.push(measure("Total Cost", total, &money));
                rows
            }
            SummaryMode::Dates => {
                columns = vec!["Planned", "Actual", "Remaining", "At Completion"];
                let date = |v: Option<i64>| format::date(origin, v);
                let done = a.is_completed();
                vec![
                    vec![
                        "Start".to_string(),
                        date(a.target_start),
                        date(a.act_start),
                        date(if done {
                            None
                        } else {
                            a.restart.or(a.early_start)
                        }),
                        date(a.act_start.or(a.early_start)),
                    ],
                    vec![
                        "Finish".to_string(),
                        date(a.target_finish),
                        date(a.act_finish),
                        date(if done {
                            None
                        } else {
                            a.reend.or(a.early_finish)
                        }),
                        date(a.act_finish.or(a.early_finish)),
                    ],
                ]
            }
        };
        let content = ui.max_rect();
        let frame = Rect::from_min_max(
            content.min,
            Pos2::new(content.right(), content.bottom() - form::BUTTON_BAR),
        );
        let p = ui.painter().with_clip_rect(frame.intersect(ui.clip_rect()));
        form::fill(&p, frame, Color32::WHITE);
        form::lowered(&p, frame);
        let (left, top) = (frame.left() + 1., frame.top() + 1.);
        // Header band: white top, face, black bottom line.
        form::fill(&p, form::rect(left, top, frame.width() - 2., 20.), CHROME);
        chrome::hline(&p, left, frame.right() - 1., top, Color32::WHITE);
        chrome::hline(&p, left, frame.right() - 1., top + 19., INK);
        const PITCH: f32 = 110.;
        // Dates sit left like the table's date columns; units and costs
        // stay right.
        let dates = mode == SummaryMode::Dates;
        let (text_x, text_align) = if dates {
            (3., Align2::LEFT_TOP)
        } else {
            (PITCH - 5., Align2::RIGHT_TOP)
        };
        for (c, caption) in columns.iter().enumerate() {
            let x = left + PITCH * (c + 1) as f32;
            chrome::vline(&p, x, top, top + 19., Color32::WHITE);
            chrome::vline(&p, x + PITCH - 1., top, top + 19., INK);
            p.text(
                Pos2::new(x + text_x, top + 3.),
                text_align,
                *caption,
                form::font(),
                INK,
            );
        }
        chrome::vline(&p, left + PITCH - 1., top, top + 19., INK);
        for (r, row) in rows.iter().enumerate() {
            let y = top + 20. + 20. * r as f32;
            // Row header cell: face with a white top/left edge, black lines.
            form::fill(&p, form::rect(left, y, PITCH, 20.), CHROME);
            chrome::hline(&p, left, left + PITCH - 1., y, Color32::WHITE);
            chrome::vline(&p, left, y, y + 19., Color32::WHITE);
            chrome::vline(&p, left + PITCH - 1., y, y + 20., INK);
            chrome::hline(&p, left, left + PITCH, y + 19., INK);
            p.text(
                Pos2::new(left + 3., y + 3.),
                Align2::LEFT_TOP,
                &row[0],
                form::font(),
                INK,
            );
            for c in 1..=columns.len() {
                let x = left + PITCH * c as f32;
                chrome::vline(&p, x + PITCH - 1., y, y + 20., LINE);
                chrome::hline(&p, x, x + PITCH, y + 19., LINE);
                let text = row.get(c).map_or("", String::as_str);
                p.with_clip_rect(form::rect(x, y, PITCH - 1., 19.).intersect(p.clip_rect()))
                    .text(
                        Pos2::new(x + text_x, y + 3.),
                        text_align,
                        text,
                        form::font(),
                        INK,
                    );
            }
        }
        let y = content.bottom() - 21.;
        for (x, value, caption) in [
            (7., SummaryMode::Units, "Display units"),
            (121., SummaryMode::Cost, "Display cost"),
            (236., SummaryMode::Dates, "Display dates"),
        ] {
            let response = form::radio(
                ui,
                ui.id().with(("summary_mode", caption)),
                content.left() + x,
                y,
                caption,
                mode == value,
            );
            if response.clicked() {
                mode = value;
            }
        }
        ui.allocate_rect(content, Sense::hover());
        self.detail.summary_mode = mode;
    }
}

/// Read-only value box.
pub fn readonly(ui: &mut egui::Ui, text: &str, width: f32) {
    let mut text = text.to_string();
    ui.add_enabled(
        false,
        egui::TextEdit::singleline(&mut text).desired_width(width),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn totals_roll_up_assignments_and_expenses() {
        let mut p = openxer_xer::starter();
        let pid = p.model.project.proj_id;
        let doc = &mut p.document;
        doc.table_mut("TASKRSRC").add(&[
            ("task_id", "1010".into()),
            ("proj_id", pid.to_string()),
            ("rsrc_type", "RT_Labor".into()),
            ("target_qty", "40".into()),
            ("act_reg_qty", "10".into()),
            ("act_ot_qty", "2".into()),
            ("remain_qty", "30".into()),
            ("target_cost", "4000".into()),
        ]);
        records::add_row(
            doc,
            "PROJCOST",
            &[
                ("task_id", "1010".into()),
                ("proj_id", pid.to_string()),
                ("target_cost", "500".into()),
                ("act_cost", "100".into()),
                ("remain_cost", "400".into()),
            ],
        );
        let t = totals(doc, 1010, pid);
        assert_eq!(t.labor_units.values(), [40., 12., 30., 42.]);
        assert_eq!(t.labor_cost.budget, 4000.);
        assert_eq!(t.expenses.at_completion(), 500.);
        assert_eq!(t.nonlabor_units, Figures::default());
    }
    #[test]
    fn driving_relationships_follow_scheduled_dates() {
        let mut p = openxer_xer::starter();
        let all: Vec<usize> = (0..p.model.links.len()).collect();
        crate::schedule_model(&mut p.model, &p.resources).unwrap();
        let flags = driving_flags(&mut p.model, &all);
        // Activity 8 has two predecessors; at least one of them drives it.
        let into: Vec<usize> = all
            .iter()
            .copied()
            .filter(|&r| p.model.links[r].succ == 8)
            .collect();
        assert_eq!(into.len(), 2);
        assert!(into.iter().any(|&r| flags[r] == Some(true)));
        assert!(flags.iter().all(Option::is_some));
    }
}
