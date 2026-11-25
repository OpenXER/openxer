//! The sample project a new workspace starts on: an invented, fully
//! resource and cost loaded process water upgrade, built from the data table
//! `assets/sample-project.txt` (see the notes at its top).
//!
//! [`sample_project`] reads the table into an XER document, schedules it to
//! get the as-planned copy (the baseline), then records progress up to the
//! data date the way Update Progress does, adds the slips and overruns the
//! table lists, and schedules again. The result is the same every time: no
//! clock, no random numbers, no hash ordering.
//!
//! Every name, rate, quantity and date in it is made up. [`starter`]
//! (`crate::starter`) stays the small sample the tests are written against.
use crate::calendar_data::{CalendarData, Shift};
use crate::edit::progress::{apply_progress, ProgressScope, RemainingRule};
use crate::edit::records::{self, add_row};
use crate::{parse_absolute, Document, ProjectData};
use openxer_engine::model::StatusType;
use std::collections::BTreeMap;

/// Project ID of the sample.
pub const SAMPLE_PROJECT_ID: &str = "RIVERBEND";
/// The workspace source name the sample is stored under.
pub const SAMPLE_SOURCE: &str = "Sample project";

const DATA: &str = include_str!("../../assets/sample-project.txt");

/// The sample as planned (scheduled at its start, no progress: the
/// baseline) and as of its data date.
#[derive(Clone, Debug)]
pub struct Sample {
    pub plan: ProjectData,
    pub current: ProjectData,
}

/// Build the sample. Panics only if the built-in table is wrong, which the
/// tests catch.
pub fn sample_project() -> Sample {
    let data = Data::parse(DATA);
    let first = load(build(&data, &BTreeMap::new()));
    // Constraints dated "plan" take the date the plan finishes them on.
    let mut dates = BTreeMap::new();
    for c in &data.constraints {
        if c.date == "plan" {
            let a = first
                .model
                .activities
                .iter()
                .find(|a| a.task_code == c.activity)
                .expect("constrained activity");
            let date = crate::date_string(first.model.origin_serial, a.early_finish);
            dates.insert(c.activity.clone(), date);
        }
    }
    let mut plan = load(build(&data, &dates));
    sync_rollups(&mut plan);
    let current = progress(&data, &plan);
    Sample { plan, current }
}

fn load(doc: Document) -> ProjectData {
    let mut p = ProjectData::from_document(doc, 1, SAMPLE_SOURCE.into()).expect("sample loads");
    schedule(&mut p);
    p
}

fn schedule(p: &mut ProjectData) {
    openxer_engine::scheduler::schedule_with_resources(&mut p.model, &p.resources)
        .expect("sample schedules");
    p.write_assignment_schedule();
}

fn sync_rollups(p: &mut ProjectData) {
    let ids: Vec<i64> = p.model.activities.iter().map(|a| a.task_id).collect();
    for id in ids {
        p.sync_task_rollups(id);
    }
}

// ------------------------------------------------------------- the table

struct Resource {
    id: String,
    name: String,
    kind: String,
    unit: String,
    calendar: i64,
    rate: f64,
    limit: Option<f64>,
    role: String,
    account: String,
}

struct Activity {
    id: String,
    wbs: String,
    calendar: i64,
    /// Working days; `None` for a milestone.
    days: Option<f64>,
    milestone: &'static str,
    name: String,
    preds: Vec<(String, &'static str, f64)>,
    resources: Vec<(String, f64)>,
}

struct Constraint {
    activity: String,
    kind: &'static str,
    date: String,
}

#[derive(Default)]
struct Data {
    project: Vec<String>,
    holidays: Vec<String>,
    units: Vec<Vec<String>>,
    roles: Vec<Vec<String>>,
    accounts: Vec<Vec<String>>,
    expense_categories: Vec<String>,
    topics: Vec<String>,
    resources: Vec<Resource>,
    wbs: Vec<Vec<String>>,
    activities: Vec<Activity>,
    constraints: Vec<Constraint>,
    codes: Vec<Vec<String>>,
    udfs: Vec<Vec<String>>,
    udf_values: Vec<Vec<String>>,
    steps: Vec<Vec<String>>,
    expenses: Vec<Vec<String>>,
    curves: Vec<Vec<String>>,
    notes: Vec<Vec<String>>,
    progress: Vec<Vec<String>>,
}

fn fields(line: &str) -> Vec<String> {
    line.split('|').map(|f| f.trim().to_string()).collect()
}

fn number(text: &str) -> f64 {
    text.trim()
        .parse()
        .unwrap_or_else(|_| panic!("sample table: {text:?} is not a number"))
}

impl Data {
    fn parse(text: &str) -> Self {
        let mut d = Data::default();
        let mut section = String::new();
        for line in text.lines() {
            let line = line.trim_end();
            if let Some(name) = line.strip_prefix("## ") {
                section = name.trim().to_string();
                continue;
            }
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let f = fields(line);
            match section.as_str() {
                "project" => d.project = f,
                "holidays" => d.holidays.extend(f),
                "units" => d.units.push(f),
                "roles" => d.roles.push(f),
                "accounts" => d.accounts.push(f),
                "expense categories" => d.expense_categories.push(f[0].clone()),
                "topics" => d.topics.push(f[0].clone()),
                "resources" => d.resources.push(Resource {
                    id: f[0].clone(),
                    name: f[1].clone(),
                    kind: f[2].clone(),
                    unit: f[3].clone(),
                    calendar: number(&f[4]) as i64,
                    rate: number(&f[5]),
                    limit: (!f[6].is_empty()).then(|| number(&f[6])),
                    role: f[7].clone(),
                    account: f[8].clone(),
                }),
                "wbs" => d.wbs.push(f),
                "activities" => d.activities.push(activity(&f)),
                "constraints" => d.constraints.push(Constraint {
                    activity: f[0].clone(),
                    kind: match f[1].as_str() {
                        "start on or after" => "CS_MSOA",
                        "finish on or after" => "CS_MEOA",
                        "finish on or before" => "CS_MEOB",
                        other => panic!("sample table: constraint {other:?}"),
                    },
                    date: f[2].clone(),
                }),
                "codes" => d.codes.push(f),
                "udfs" => d.udfs.push(f),
                "udf values" => d.udf_values.push(f),
                "steps" => d.steps.push(f),
                "expenses" => d.expenses.push(f),
                "curves" => d.curves.push(f),
                "notes" => d.notes.push(f),
                "progress" => d.progress.push(f),
                other => panic!("sample table: unknown section {other:?}"),
            }
        }
        d
    }

    fn resource(&self, id: &str) -> (usize, &Resource) {
        self.resources
            .iter()
            .enumerate()
            .find(|(_, r)| r.id == id)
            .unwrap_or_else(|| panic!("sample table: no resource {id}"))
    }

    fn task_id(&self, code: &str) -> i64 {
        1000 + self
            .activities
            .iter()
            .position(|a| a.id == code)
            .unwrap_or_else(|| panic!("sample table: no activity {code}")) as i64
    }

    fn wbs_id(&self, code: &str) -> i64 {
        101 + self
            .wbs
            .iter()
            .position(|w| w[0] == code)
            .unwrap_or_else(|| panic!("sample table: no WBS {code}")) as i64
    }

    fn progress_value(&self, key: &str) -> Vec<&[String]> {
        self.progress
            .iter()
            .filter(|f| f[0] == key)
            .map(|f| &f[1..])
            .collect()
    }
}

fn activity(f: &[String]) -> Activity {
    let (days, milestone) = match f[3].as_str() {
        "start" => (None, "TT_Mile"),
        "finish" => (None, "TT_FinMile"),
        d => (Some(number(d)), "TT_Task"),
    };
    let preds = f[5]
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut parts = p.split_whitespace();
            let id = parts.next().unwrap().to_string();
            match parts.next() {
                None => (id, "PR_FS", 0.),
                Some(link) => {
                    let (kind, lag) = link.split_at(2);
                    let kind = match kind {
                        "SS" => "PR_SS",
                        "FF" => "PR_FF",
                        "FS" => "PR_FS",
                        other => panic!("sample table: link {other:?}"),
                    };
                    (id, kind, number(lag))
                }
            }
        })
        .collect();
    let resources = f
        .get(6)
        .map(|r| {
            r.split(',')
                .map(str::trim)
                .filter(|r| !r.is_empty())
                .map(|r| {
                    let (id, units) = r.rsplit_once(' ').expect("resource and units");
                    (id.to_string(), number(units))
                })
                .collect()
        })
        .unwrap_or_default();
    Activity {
        id: f[0].clone(),
        wbs: f[1].clone(),
        calendar: number(&f[2]) as i64,
        days,
        milestone,
        name: f[4].clone(),
        preds,
        resources,
    }
}

// ------------------------------------------------------------ calendars

/// Calendar id, name, working days (0 = Sunday), shifts, hours a day and
/// whether holidays are off.
type CalendarSpec = (
    i64,
    &'static str,
    [bool; 7],
    &'static [(&'static str, &'static str)],
    f64,
    bool,
);

const CALENDARS: [CalendarSpec; 3] = [
    (
        5,
        "Riverbend 5-Day 8-Hour",
        [false, true, true, true, true, true, false],
        &[("07:00", "11:00"), ("11:30", "15:30")],
        8.,
        true,
    ),
    (
        6,
        "Riverbend 6-Day 10-Hour",
        [false, true, true, true, true, true, true],
        &[("06:00", "11:00"), ("11:30", "16:30")],
        10.,
        true,
    ),
    (
        7,
        "Riverbend 7-Day 10-Hour",
        [true; 7],
        &[("06:00", "11:00"), ("11:30", "16:30")],
        10.,
        false,
    ),
];

fn calendar_hours(id: i64) -> f64 {
    CALENDARS
        .iter()
        .find(|c| c.0 == id)
        .unwrap_or_else(|| panic!("sample table: no calendar {id}"))
        .4
}

fn minutes(clock: &str) -> i64 {
    let (h, m) = clock.split_once(':').unwrap();
    h.parse::<i64>().unwrap() * 60 + m.parse::<i64>().unwrap()
}

fn calendar_text(days: [bool; 7], shifts: &[(&str, &str)], holidays: &[i64]) -> String {
    let shifts: Vec<Shift> = shifts
        .iter()
        .map(|(s, f)| (minutes(s), minutes(f)))
        .collect();
    let mut c = CalendarData::default();
    for (d, works) in days.iter().enumerate() {
        if *works {
            c.week[d] = shifts.clone();
        }
    }
    for &h in holidays {
        c.exceptions.insert(h, vec![]);
    }
    c.serialize()
}

// ------------------------------------------------------------- building

fn text(v: f64) -> String {
    crate::assignments::quantity_text(v)
}

fn cost(v: f64) -> String {
    crate::assignments::cost_text((v * 100.).round() / 100.)
}

/// The XER document of the plan: no progress, the data date at the start.
/// `dates` gives the "plan" constraints their dates.
fn build(d: &Data, dates: &BTreeMap<String, String>) -> Document {
    let mut doc = Document {
        header: "ERMHDR\t25.12\t2026-06-01\tProject\tOpenXER\tOpenXER\tdbxDatabaseNoName\tProject Management\tUSD".into(),
        tables: vec![],
        windows_encoding: true,
        warnings: vec![],
        lone_quotes: BTreeMap::new(),
        id_floor: BTreeMap::new(),
    };
    let doc = &mut doc;
    let (short, name, start) = (&d.project[0], &d.project[1], &d.project[2]);
    for (i, u) in d.units.iter().enumerate() {
        add_row(
            doc,
            "UMEASURE",
            &[
                ("unit_id", (i + 1).to_string()),
                ("seq_num", (i + 1).to_string()),
                ("unit_abbrev", u[0].clone()),
                ("unit_name", u[1].clone()),
            ],
        );
    }
    for (i, r) in d.roles.iter().enumerate() {
        add_row(
            doc,
            "ROLES",
            &[
                ("role_id", (i + 1).to_string()),
                ("seq_num", (i + 1).to_string()),
                ("role_name", r[1].clone()),
                ("role_short_name", r[0].clone()),
                ("def_cost_qty_link_flag", "Y".into()),
                ("cost_qty_type", "QT_Hour".into()),
            ],
        );
    }
    let role_id = |code: &str| {
        d.roles
            .iter()
            .position(|r| r[0] == code)
            .map(|i| (i + 1).to_string())
            .unwrap_or_default()
    };
    for (i, a) in d.accounts.iter().enumerate() {
        add_row(
            doc,
            "ACCOUNT",
            &[
                ("acct_id", (i + 1).to_string()),
                ("acct_seq_num", (i + 1).to_string()),
                ("acct_name", a[1].clone()),
                ("acct_short_name", a[0].clone()),
            ],
        );
    }
    let account_id = |code: &str| {
        d.accounts
            .iter()
            .position(|a| a[0] == code)
            .map(|i| (i + 1).to_string())
            .unwrap_or_else(|| panic!("sample table: no account {code}"))
    };
    for (i, c) in d.expense_categories.iter().enumerate() {
        add_row(
            doc,
            "COSTTYPE",
            &[
                ("cost_type_id", (i + 1).to_string()),
                ("seq_num", (i + 1).to_string()),
                ("cost_type", c.clone()),
            ],
        );
    }
    for (i, t) in d.topics.iter().enumerate() {
        add_row(
            doc,
            "MEMOTYPE",
            &[
                ("memo_type_id", (i + 1).to_string()),
                ("seq_num", (i + 1).to_string()),
                ("eps_flag", "N".into()),
                ("proj_flag", "Y".into()),
                ("wbs_flag", "Y".into()),
                ("task_flag", "Y".into()),
                ("memo_type", t.clone()),
            ],
        );
    }
    for (i, u) in d.udfs.iter().enumerate() {
        add_row(
            doc,
            "UDFTYPE",
            &[
                ("udf_type_id", (i + 1).to_string()),
                ("table_name", "TASK".into()),
                ("udf_type_name", format!("user_field_{}", i + 1)),
                ("udf_type_label", u[0].clone()),
                (
                    "logical_data_type",
                    if u[1] == "number" {
                        "FT_INT"
                    } else {
                        "FT_TEXT"
                    }
                    .into(),
                ),
                ("super_flag", "N".into()),
            ],
        );
    }
    doc.table_mut("PROJECT").add(&[
        ("proj_id", "1".into()),
        ("proj_short_name", short.clone()),
        ("clndr_id", "5".into()),
        ("plan_start_date", start.clone()),
        ("last_recalc_date", start.clone()),
        ("rem_target_link_flag", "Y".into()),
        ("def_complete_pct_type", "CP_Phys".into()),
        ("def_duration_type", "DT_FixedDUR2".into()),
        ("def_rate_type", "COST_PER_QTY".into()),
        ("def_task_type", "TT_Task".into()),
        ("step_complete_flag", "N".into()),
        ("task_code_base", "1000".into()),
        ("task_code_step", "10".into()),
    ]);
    let holidays: Vec<i64> = d
        .holidays
        .iter()
        .map(|h| parse_absolute(&format!("{h} 00:00")).expect("holiday date") / 1440)
        .collect();
    for (id, cal_name, days, shifts, hours, off) in CALENDARS {
        let week = days.iter().filter(|w| **w).count() as f64 * hours;
        add_row(
            doc,
            "CALENDAR",
            &[
                ("clndr_id", id.to_string()),
                ("default_flag", "N".into()),
                ("clndr_name", cal_name.into()),
                ("clndr_type", "CA_Base".into()),
                ("day_hr_cnt", text(hours)),
                ("week_hr_cnt", text(week)),
                ("month_hr_cnt", text(week * 52. / 12.)),
                ("year_hr_cnt", text(week * 52.)),
                ("rsrc_private", "N".into()),
                (
                    "clndr_data",
                    calendar_text(days, shifts, if off { &holidays } else { &[] }),
                ),
            ],
        );
    }
    // WBS: the project node, then the table's elements in order.
    add_row(
        doc,
        "PROJWBS",
        &[
            ("wbs_id", "100".into()),
            ("proj_id", "1".into()),
            ("seq_num", "0".into()),
            ("proj_node_flag", "Y".into()),
            ("status_code", "WS_Open".into()),
            ("wbs_short_name", short.clone()),
            ("wbs_name", name.clone()),
        ],
    );
    let mut sequence: BTreeMap<String, usize> = BTreeMap::new();
    for w in &d.wbs {
        let parent = if w[1].is_empty() {
            100
        } else {
            d.wbs_id(&w[1])
        };
        let seq = sequence.entry(w[1].clone()).or_default();
        *seq += 1;
        // The code is the last segment; the WBS path joins them.
        let segment = w[0].rsplit('.').next().unwrap().to_string();
        add_row(
            doc,
            "PROJWBS",
            &[
                ("wbs_id", d.wbs_id(&w[0]).to_string()),
                ("proj_id", "1".into()),
                ("seq_num", (*seq * 10).to_string()),
                ("proj_node_flag", "N".into()),
                ("status_code", "WS_Open".into()),
                ("wbs_short_name", segment),
                ("wbs_name", w[2].clone()),
                ("parent_wbs_id", parent.to_string()),
            ],
        );
    }
    for (i, r) in d.resources.iter().enumerate() {
        let id = (i + 1).to_string();
        let kind = match r.kind.as_str() {
            "labor" => "RT_Labor",
            "nonlabor" => "RT_Equip",
            _ => "RT_Mat",
        };
        let unit = d
            .units
            .iter()
            .position(|u| u[0] == r.unit)
            .map(|i| (i + 1).to_string())
            .unwrap_or_default();
        add_row(
            doc,
            "RSRC",
            &[
                ("rsrc_id", id.clone()),
                ("clndr_id", r.calendar.to_string()),
                ("role_id", role_id(&r.role)),
                ("rsrc_seq_num", (i + 1).to_string()),
                ("rsrc_name", r.name.clone()),
                ("rsrc_short_name", r.id.clone()),
                ("def_qty_per_hr", "1".into()),
                ("cost_qty_type", "QT_Hour".into()),
                (
                    "ot_factor",
                    if kind == "RT_Labor" { "1.5" } else { "" }.into(),
                ),
                ("active_flag", "Y".into()),
                ("auto_compute_act_flag", "N".into()),
                ("def_cost_qty_link_flag", "Y".into()),
                ("ot_flag", "N".into()),
                ("unit_id", unit),
                ("rsrc_type", kind.into()),
            ],
        );
        add_row(
            doc,
            "RSRCRATE",
            &[
                ("rsrc_rate_id", id.clone()),
                ("rsrc_id", id.clone()),
                ("max_qty_per_hr", r.limit.map(text).unwrap_or_default()),
                ("cost_per_qty", cost(r.rate)),
                ("start_date", "2026-01-01 00:00".into()),
            ],
        );
        if !r.role.is_empty() {
            add_row(
                doc,
                "RSRCROLE",
                &[
                    ("rsrc_id", id),
                    ("role_id", role_id(&r.role)),
                    ("skill_level", "3".into()),
                ],
            );
        }
    }
    // Project activity codes.
    let mut code_types: Vec<String> = vec![];
    for c in &d.codes {
        if !code_types.contains(&c[0]) {
            code_types.push(c[0].clone());
            let n = code_types.len();
            add_row(
                doc,
                "ACTVTYPE",
                &[
                    ("actv_code_type_id", n.to_string()),
                    ("actv_short_len", "10".into()),
                    ("seq_num", (n * 10).to_string()),
                    ("actv_code_type", c[0].clone()),
                    ("proj_id", "1".into()),
                    ("actv_code_type_scope", "AS_Project".into()),
                ],
            );
        }
    }
    for (i, c) in d.codes.iter().enumerate() {
        let ty = code_types.iter().position(|t| *t == c[0]).unwrap() + 1;
        add_row(
            doc,
            "ACTVCODE",
            &[
                ("actv_code_id", (i + 1).to_string()),
                ("actv_code_type_id", ty.to_string()),
                ("actv_code_name", c[2].clone()),
                ("short_name", c[1].clone()),
                ("seq_num", ((i + 1) * 10).to_string()),
            ],
        );
    }
    let code_id = |ty: &str, value: &str| -> (usize, usize) {
        let t = code_types.iter().position(|t| t == ty).unwrap() + 1;
        let v = d
            .codes
            .iter()
            .position(|c| c[0] == ty && c[1] == value)
            .unwrap_or_else(|| panic!("sample table: no {ty} code {value}"))
            + 1;
        (t, v)
    };
    let constraint = |code: &str| d.constraints.iter().find(|c| c.activity == code);
    let mut assignment = 0;
    let mut link = 0;
    for a in &d.activities {
        let task_id = d.task_id(&a.id);
        let hours = calendar_hours(a.calendar);
        let duration = a.days.map_or(0., |days| days * hours);
        let mut row = vec![
            ("task_id", task_id.to_string()),
            ("proj_id", "1".into()),
            ("wbs_id", d.wbs_id(&a.wbs).to_string()),
            ("clndr_id", a.calendar.to_string()),
            ("phys_complete_pct", "0".into()),
            ("rev_fdbk_flag", "N".into()),
            ("est_wt", "1".into()),
            ("lock_plan_flag", "N".into()),
            ("auto_compute_act_flag", "N".into()),
            ("complete_pct_type", "CP_Phys".into()),
            ("task_type", a.milestone.into()),
            ("duration_type", "DT_FixedDUR2".into()),
            ("status_code", "TK_NotStart".into()),
            ("task_code", a.id.clone()),
            ("task_name", a.name.clone()),
            ("remain_drtn_hr_cnt", text(duration)),
            ("target_drtn_hr_cnt", text(duration)),
            ("priority_type", "PT_Normal".into()),
        ];
        if let Some(c) = constraint(&a.id) {
            let date = if c.date == "plan" {
                dates.get(&a.id).cloned().unwrap_or_default()
            } else {
                c.date.clone()
            };
            if !date.is_empty() {
                row.push(("cstr_type", c.kind.into()));
                row.push(("cstr_date", date));
            }
        }
        add_row(doc, "TASK", &row);
        for (pred, kind, lag) in &a.preds {
            link += 1;
            let p = &d.activities[(d.task_id(pred) - 1000) as usize];
            add_row(
                doc,
                "TASKPRED",
                &[
                    ("task_pred_id", link.to_string()),
                    ("task_id", task_id.to_string()),
                    ("pred_task_id", d.task_id(pred).to_string()),
                    ("proj_id", "1".into()),
                    ("pred_proj_id", "1".into()),
                    ("pred_type", (*kind).into()),
                    ("lag_hr_cnt", text(lag * calendar_hours(p.calendar))),
                ],
            );
        }
        for (rsrc, units) in &a.resources {
            assignment += 1;
            let (i, r) = d.resource(rsrc);
            let (qty, per_hour) = if r.kind == "material" {
                (*units, if duration > 0. { units / duration } else { 0. })
            } else {
                (units * duration, *units)
            };
            let kind = match r.kind.as_str() {
                "labor" => "RT_Labor",
                "nonlabor" => "RT_Equip",
                _ => "RT_Mat",
            };
            add_row(
                doc,
                "TASKRSRC",
                &[
                    ("taskrsrc_id", assignment.to_string()),
                    ("task_id", task_id.to_string()),
                    ("proj_id", "1".into()),
                    ("cost_qty_link_flag", "Y".into()),
                    ("role_id", role_id(&r.role)),
                    ("acct_id", account_id(&r.account)),
                    ("rsrc_id", (i + 1).to_string()),
                    (
                        "skill_level",
                        if r.role.is_empty() { "" } else { "3" }.into(),
                    ),
                    ("remain_qty", text(qty)),
                    ("target_qty", text(qty)),
                    ("remain_qty_per_hr", text(per_hour)),
                    ("target_lag_drtn_hr_cnt", "0".into()),
                    ("target_qty_per_hr", text(per_hour)),
                    ("act_ot_qty", "0".into()),
                    ("act_reg_qty", "0".into()),
                    ("relag_drtn_hr_cnt", "0".into()),
                    ("ot_factor", "1".into()),
                    ("cost_per_qty", cost(r.rate)),
                    ("target_cost", cost(qty * r.rate)),
                    ("act_reg_cost", "0".into()),
                    ("act_ot_cost", "0".into()),
                    ("remain_cost", cost(qty * r.rate)),
                    ("rollup_dates_flag", "Y".into()),
                    ("rate_type", "COST_PER_QTY".into()),
                    ("rsrc_type", kind.into()),
                    ("cost_per_qty_source_type", "ST_Rsrc".into()),
                ],
            );
        }
        // Codes: area from the WBS, discipline from the activity ID,
        // phase from the top of the WBS, responsibility from the crew.
        let area = match &a.wbs[..a.wbs.len().min(4)] {
            "A100" => "100",
            "A200" => "200",
            "A300" => "300",
            "A400" => "400",
            _ => "GEN",
        };
        let discipline = match &a.id[..2] {
            "MS" | "PM" => "PM",
            "EN" => "ENG",
            "PR" => "PRC",
            "DM" => "DEM",
            "CV" | "ST" => "CIV",
            "EQ" => "EQP",
            "PP" => "PIP",
            "EL" => "ELE",
            "IN" => "INS",
            "PI" => "PNT",
            _ => "COM",
        };
        let top = d
            .wbs
            .iter()
            .find(|w| w[0] == a.wbs)
            .map(|w| {
                if w[1].is_empty() {
                    w[0].clone()
                } else {
                    w[1].clone()
                }
            })
            .unwrap_or_default();
        let top = d
            .wbs
            .iter()
            .find(|w| w[0] == top)
            .map(|w| {
                if w[1].is_empty() {
                    w[0].clone()
                } else {
                    w[1].clone()
                }
            })
            .unwrap_or(top);
        let phase = match top.as_str() {
            "ENG" => "1-ENG",
            "PROC" => "2-PROC",
            "CONST" => "3-CONST",
            "COMM" => "4-COMM",
            _ if a.id.as_str() >= "PM0900" => "4-COMM",
            _ => "1-ENG",
        };
        let craft = |ids: &[&str]| {
            a.resources
                .iter()
                .filter(|(r, _)| ids.contains(&r.as_str()))
                .map(|(_, u)| *u)
                .sum::<f64>()
        };
        let responsible = if a.name.starts_with("Owner delivers") {
            "OWN"
        } else if a.name.starts_with("Vendor")
            || a.name.starts_with("Pump vendor")
            || a.name.starts_with("Steel fabricator")
        {
            "VEND"
        } else {
            let electrical = craft(&["ELEC", "INST"]);
            let mechanical = craft(&["PF", "WLD", "MW"]);
            if electrical > mechanical && electrical > 0. {
                "ELEC"
            } else if mechanical > 0. {
                "MECH"
            } else {
                "GC"
            }
        };
        for (ty, value) in [
            ("Area", area),
            ("Discipline", discipline),
            ("Phase", phase),
            ("Responsibility", responsible),
        ] {
            let (t, v) = code_id(ty, value);
            add_row(
                doc,
                "TASKACTV",
                &[
                    ("task_id", task_id.to_string()),
                    ("actv_code_type_id", t.to_string()),
                    ("actv_code_id", v.to_string()),
                    ("proj_id", "1".into()),
                ],
            );
        }
        // Work package and crew size on the construction work.
        if phase == "3-CONST" && a.days.is_some() {
            let wp = format!("WP-{area}-{discipline}");
            let crew = craft(&[
                "GF", "LAB", "CARP", "IW", "PF", "WLD", "ELEC", "INST", "MW", "PNT", "INSUL", "OPR",
            ]);
            udf(doc, d, task_id, "Work Package", &wp);
            udf(doc, d, task_id, "Crew Size", &text(crew.ceil()));
        }
    }
    for v in &d.udf_values {
        udf(doc, d, d.task_id(&v[0]), &v[1], &v[2]);
    }
    for (i, s) in d.steps.iter().enumerate() {
        let task_id = d.task_id(&s[0]);
        let seq = d.steps[..i].iter().filter(|o| o[0] == s[0]).count() + 1;
        add_row(
            doc,
            "TASKPROC",
            &[
                ("proc_id", (i + 1).to_string()),
                ("task_id", task_id.to_string()),
                ("proj_id", "1".into()),
                ("seq_num", (seq * 10).to_string()),
                ("proc_name", s[1].clone()),
                ("complete_flag", "N".into()),
                ("proc_wt", s[2].clone()),
                ("complete_pct", "0".into()),
            ],
        );
    }
    for (i, e) in d.expenses.iter().enumerate() {
        let category = d
            .expense_categories
            .iter()
            .position(|c| *c == e[1])
            .unwrap_or_else(|| panic!("sample table: no expense category {}", e[1]))
            + 1;
        let amount = number(&e[5]);
        add_row(
            doc,
            "PROJCOST",
            &[
                ("cost_item_id", (i + 1).to_string()),
                ("acct_id", account_id(&e[2])),
                ("cost_type_id", category.to_string()),
                ("proj_id", "1".into()),
                ("task_id", d.task_id(&e[0]).to_string()),
                ("cost_name", e[3].clone()),
                ("po_number", format!("RB-PO-{:03}", 101 + i)),
                ("vendor_name", e[4].clone()),
                ("act_cost", "0".into()),
                ("cost_per_qty", cost(amount)),
                ("remain_cost", cost(amount)),
                ("target_cost", cost(amount)),
                ("cost_load_type", "CL_Uniform".into()),
                ("auto_compute_act_flag", "N".into()),
                ("target_qty", "1".into()),
                ("qty_name", "lump sum".into()),
            ],
        );
    }
    for c in &d.curves {
        let id = crate::edit::curves::ensure(doc, &c[2]).expect("preset curve");
        let task_id = d.task_id(&c[0]).to_string();
        let rsrc = (d.resource(&c[1]).0 + 1).to_string();
        let t = doc.table_mut("TASKRSRC");
        let row = t
            .rows
            .iter()
            .position(|r| t.value(r, "task_id") == task_id && t.value(r, "rsrc_id") == rsrc)
            .unwrap_or_else(|| panic!("sample table: no {} on {}", c[1], c[0]));
        t.set(row, "curv_id", id.to_string());
    }
    for (i, n) in d.notes.iter().enumerate() {
        let topic = d
            .topics
            .iter()
            .position(|t| *t == n[1])
            .unwrap_or_else(|| panic!("sample table: no topic {}", n[1]))
            + 1;
        let memo = records::text_to_memo(&n[2]);
        if d.activities.iter().any(|a| a.id == n[0]) {
            add_row(
                doc,
                "TASKMEMO",
                &[
                    ("memo_id", (i + 1).to_string()),
                    ("task_id", d.task_id(&n[0]).to_string()),
                    ("memo_type_id", topic.to_string()),
                    ("proj_id", "1".into()),
                    ("task_memo", memo),
                ],
            );
        } else {
            let wbs = if n[0] == "PROJECT" {
                100
            } else {
                d.wbs_id(&n[0])
            };
            add_row(
                doc,
                "WBSMEMO",
                &[
                    ("wbs_memo_id", (i + 1).to_string()),
                    ("proj_id", "1".into()),
                    ("memo_type_id", topic.to_string()),
                    ("wbs_id", wbs.to_string()),
                    ("wbs_memo", memo),
                ],
            );
        }
    }
    doc.clone()
}

fn udf(doc: &mut Document, d: &Data, task_id: i64, label: &str, value: &str) {
    let i = d
        .udfs
        .iter()
        .position(|u| u[0] == label)
        .unwrap_or_else(|| panic!("sample table: no user field {label}"));
    let column = if d.udfs[i][1] == "number" {
        "udf_number"
    } else {
        "udf_text"
    };
    add_row(
        doc,
        "UDFVALUE",
        &[
            ("udf_type_id", (i + 1).to_string()),
            ("fk_id", task_id.to_string()),
            ("proj_id", "1".into()),
            (column, value.to_string()),
        ],
    );
}

// ------------------------------------------------------------- progress

/// An activity at the data date: status, the share of its work done and
/// the share left (of the original duration), actual start and finish.
type State = (StatusType, f64, f64, String, String);

/// The plan progressed to the table's data date: what the plan had done by
/// then is done, what straddles it is in progress, with the table's slips
/// added to the remaining work and its overruns to the actual units.
fn progress(d: &Data, plan: &ProjectData) -> ProjectData {
    let mut p = plan.clone();
    let date = &d.progress_value("data date")[0][0];
    let origin = p.model.origin_serial;
    let dd = parse_absolute(date).expect("data date") - origin * 1440;
    apply_progress(&mut p, dd, ProgressScope::All, RemainingRule::Always).expect("progress");
    // Planned remaining before the slips, by task id.
    let planned: BTreeMap<i64, i64> = p
        .model
        .activities
        .iter()
        .map(|a| (a.task_id, a.rem))
        .collect();
    for s in d.progress_value("slip") {
        let task_id = d.task_id(&s[0]);
        let a = p
            .model
            .activities
            .iter_mut()
            .find(|a| a.task_id == task_id)
            .unwrap();
        assert_eq!(a.status, StatusType::InProgress, "slip on {}", s[0]);
        let hours = calendar_hours(d.activities[(task_id - 1000) as usize].calendar);
        a.rem += (number(&s[1]) * hours * 60.) as i64;
    }
    let overrun: BTreeMap<i64, f64> = d
        .progress_value("overrun")
        .iter()
        .map(|o| (d.task_id(&o[0]), number(&o[1])))
        .collect();
    let mut doc = p.updated_document();
    let state: BTreeMap<i64, State> = p
        .model
        .activities
        .iter()
        .map(|a| {
            let original = p.details[&a.task_id].original_duration as f64;
            let done = if original > 0. {
                (original - planned[&a.task_id] as f64) / original
            } else {
                1.
            };
            let left = if original > 0. {
                a.rem as f64 / original
            } else {
                0.
            };
            (
                a.task_id,
                (
                    a.status,
                    done,
                    left,
                    crate::date_string(origin, a.act_start),
                    crate::date_string(origin, a.act_finish),
                ),
            )
        })
        .collect();
    let t = doc.table_mut("TASKRSRC");
    for i in 0..t.rows.len() {
        let task_id = records::num(t.value(&t.rows[i], "task_id"));
        let (status, done, left, start, finish) = &state[&task_id];
        let budget = records::float(t.value(&t.rows[i], "target_qty"));
        let price = records::float(t.value(&t.rows[i], "cost_per_qty"));
        let factor = overrun.get(&task_id).copied().unwrap_or(1.);
        let (actual, remaining) = match status {
            StatusType::Completed => (budget * factor, 0.),
            StatusType::InProgress => (budget * done * factor, budget * left),
            StatusType::NotStarted => (0., budget),
        };
        t.set(i, "act_reg_qty", text(actual));
        t.set(i, "remain_qty", text(remaining));
        t.set(i, "act_reg_cost", cost(actual * price));
        t.set(i, "remain_cost", cost(remaining * price));
        if *status != StatusType::NotStarted {
            t.set(i, "act_start_date", start.clone());
        }
        if *status == StatusType::Completed {
            t.set(i, "act_end_date", finish.clone());
        }
    }
    let t = doc.table_mut("PROJCOST");
    for i in 0..t.rows.len() {
        let task_id = records::num(t.value(&t.rows[i], "task_id"));
        let (status, done, ..) = &state[&task_id];
        let budget = records::float(t.value(&t.rows[i], "target_cost"));
        let actual = match status {
            StatusType::Completed => budget,
            StatusType::InProgress => (budget * done).round(),
            StatusType::NotStarted => 0.,
        };
        t.set(i, "act_cost", cost(actual));
        t.set(i, "remain_cost", cost(budget - actual));
    }
    // Steps done in order up to the physical percent.
    let t = doc.table_mut("TASKPROC");
    let mut reached: BTreeMap<i64, f64> = BTreeMap::new();
    for i in 0..t.rows.len() {
        let task_id = records::num(t.value(&t.rows[i], "task_id"));
        let (status, done, ..) = &state[&task_id];
        let weight = records::float(t.value(&t.rows[i], "proc_wt"));
        let total: f64 = d
            .steps
            .iter()
            .filter(|s| d.task_id(&s[0]) == task_id)
            .map(|s| number(&s[2]))
            .sum();
        let so_far = reached.entry(task_id).or_default();
        *so_far += weight / total;
        let complete = match status {
            StatusType::Completed => true,
            StatusType::InProgress => *so_far <= *done + 1e-9,
            StatusType::NotStarted => false,
        };
        if complete {
            t.set(i, "complete_flag", "Y".into());
            t.set(i, "complete_pct", "100".into());
        }
    }
    let mut current = load(doc);
    sync_rollups(&mut current);
    current
}

#[cfg(test)]
mod tests;
