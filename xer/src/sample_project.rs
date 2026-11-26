//! The sample project a new workspace starts on: an invented, fully
//! resource and cost loaded process water upgrade, built from the data table
//! `assets/sample-project.txt` (see the notes at its top).
//!
//! [`sample_project`] reads the table into an XER document, schedules it to
//! get the as-planned copy (the project baseline), progresses a mid-project
//! copy for the secondary baseline, then records progress up to the data date
//! the way Update Progress does, adds the slips, overruns and stored past
//! period performance, and schedules again. The result is the same every time:
//! no clock, no random numbers, no hash ordering.
//!
//! Every name, rate, quantity and date in it is made up. [`starter`]
//! (`crate::starter`) stays the small sample the tests are written against.
use crate::calendar_data::{CalendarData, Shift};
use crate::edit::progress::{apply_progress, ProgressScope, RemainingRule};
use crate::edit::records::{self, add_row};
use crate::{parse_absolute, Document, ProjectData};
use openxer_engine::model::{LinkType, StatusType, TaskType};
use std::collections::BTreeMap;

/// Project ID of the sample.
pub const SAMPLE_PROJECT_ID: &str = "RIVERBEND";
/// The workspace source name the sample is stored under.
pub const SAMPLE_SOURCE: &str = "Sample project";

const DATA: &str = include_str!("../../assets/sample-project.txt");

/// The sample as planned (Baseline 1: scheduled at its start, no progress),
/// at its mid-project rebaseline (Baseline 2), and as of its data date.
#[derive(Clone, Debug)]
pub struct Sample {
    pub plan: ProjectData,
    pub revision: ProjectData,
    pub current: ProjectData,
}

/// Build the sample. Panics only if the built-in table is wrong, which the
/// tests catch.
pub fn sample_project() -> Sample {
    let data = Data::parse(DATA);
    let first = load(build(&data, &BTreeMap::new(), &BTreeMap::new()));
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
    // LOE activities take their planned hours from their scheduled span so
    // their resource assignments are sized to their full planned duration.
    let mut loe_hours = BTreeMap::new();
    for a in &first.model.activities {
        if a.task_type == TaskType::Loe {
            let hours = loe_duration_hours(&first, a);
            if hours > 0. {
                loe_hours.insert(a.task_code.clone(), hours);
            }
        }
    }
    let mut plan = load(build(&data, &dates, &loe_hours));
    sync_rollups(&mut plan);

    // Mid-project rebaseline (Baseline 2): progressed to revision date with
    // early engineering/submittal overruns only, no October slips.
    let rev_date = data.progress_value("revision date")[0][0].clone();
    let rev_overruns: Vec<(String, f64)> = vec![
        ("EN0200".into(), 1.04),
        ("PR0110".into(), 1.05),
        ("EN0060".into(), 1.03),
    ];
    let revision = progress(&data, &plan, &rev_date, &[], &rev_overruns, false);

    // Current schedule as of the data date, with slips, overruns and stored
    // past period performance.
    let dd = data.progress_value("data date")[0][0].clone();
    let slips: Vec<(String, f64)> = data
        .progress_value("slip")
        .into_iter()
        .map(|s| (s[0].clone(), number(&s[1])))
        .collect();
    let overruns: Vec<(String, f64)> = data
        .progress_value("overrun")
        .into_iter()
        .map(|o| (o[0].clone(), number(&o[1])))
        .collect();
    let current = progress(&data, &plan, &dd, &slips, &overruns, true);
    Sample {
        plan,
        revision,
        current,
    }
}

fn loe_duration_hours(p: &ProjectData, a: &openxer_engine::model::Activity) -> f64 {
    let (Some(s), Some(f)) = (a.early_start, a.early_finish) else {
        return 0.;
    };
    if f <= s {
        return 0.;
    }
    let cal = &p.model.calendars[a.cal];
    cal.cal.duration(s, f) as f64 / 60.
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
    parent: String,
    name: String,
    kind: String,
    unit: String,
    calendar: i64,
    rate: f64,
    limit: Option<f64>,
    role: String,
    account: String,
    trade: String,
    source: String,
}

impl Resource {
    fn is_group(&self) -> bool {
        self.kind.starts_with("group_")
    }

    fn xer_type(&self) -> &'static str {
        match self.kind.as_str() {
            "labor" | "group_labor" => "RT_Labor",
            "nonlabor" | "group_nonlabor" => "RT_Equip",
            _ => "RT_Mat",
        }
    }
}

struct Activity {
    id: String,
    wbs: String,
    calendar: i64,
    /// Working days; `None` for milestones, LOE and WBS summary.
    days: Option<f64>,
    task_type: &'static str,
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
    obs: Vec<Vec<String>>,
    roles: Vec<Vec<String>>,
    accounts: Vec<Vec<String>>,
    expense_categories: Vec<String>,
    topics: Vec<String>,
    baseline_types: Vec<String>,
    project_codes: Vec<Vec<String>>,
    funding: Vec<Vec<String>>,
    resource_codes: Vec<Vec<String>>,
    assignment_codes: Vec<Vec<String>>,
    resources: Vec<Resource>,
    wbs: Vec<Vec<String>>,
    wbs_steps: Vec<Vec<String>>,
    budget_log: Vec<Vec<String>>,
    spending_plan: Vec<Vec<String>>,
    activities: Vec<Activity>,
    constraints: Vec<Constraint>,
    codes: Vec<Vec<String>>,
    udfs: Vec<Vec<String>>,
    udf_values: Vec<Vec<String>>,
    steps: Vec<Vec<String>>,
    expenses: Vec<Vec<String>>,
    curves: Vec<Vec<String>>,
    doc_categories: Vec<String>,
    doc_statuses: Vec<String>,
    documents: Vec<Vec<String>>,
    risk_categories: Vec<String>,
    risks: Vec<Vec<String>>,
    thresholds: Vec<Vec<String>>,
    issues: Vec<Vec<String>>,
    notes: Vec<Vec<String>>,
    financial_periods: Vec<Vec<String>>,
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
                "obs" => d.obs.push(f),
                "roles" => d.roles.push(f),
                "accounts" => d.accounts.push(f),
                "expense categories" => d.expense_categories.push(f[0].clone()),
                "topics" => d.topics.push(f[0].clone()),
                "baseline types" => d.baseline_types.push(f[0].clone()),
                "project codes" => d.project_codes.push(f),
                "funding" => d.funding.push(f),
                "resource codes" => d.resource_codes.push(f),
                "assignment codes" => d.assignment_codes.push(f),
                "resources" => d.resources.push(Resource {
                    id: f[0].clone(),
                    parent: f[1].clone(),
                    name: f[2].clone(),
                    kind: f[3].clone(),
                    unit: f[4].clone(),
                    calendar: number(&f[5]) as i64,
                    rate: number(&f[6]),
                    limit: (!f[7].is_empty()).then(|| number(&f[7])),
                    role: f[8].clone(),
                    account: f[9].clone(),
                    trade: f.get(10).cloned().unwrap_or_default(),
                    source: f.get(11).cloned().unwrap_or_default(),
                }),
                "wbs" => d.wbs.push(f),
                "wbs steps" => d.wbs_steps.push(f),
                "budget log" => d.budget_log.push(f),
                "spending plan" => d.spending_plan.push(f),
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
                "doc categories" => d.doc_categories.push(f[0].clone()),
                "doc statuses" => d.doc_statuses.push(f[0].clone()),
                "documents" => d.documents.push(f),
                "risk categories" => d.risk_categories.push(f[0].clone()),
                "risks" => d.risks.push(f),
                "thresholds" => d.thresholds.push(f),
                "issues" => d.issues.push(f),
                "notes" => d.notes.push(f),
                "financial periods" => d.financial_periods.push(f),
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

    fn obs_id(&self, code: &str) -> i64 {
        1 + self
            .obs
            .iter()
            .position(|o| o[0] == code)
            .unwrap_or_else(|| panic!("sample table: no OBS {code}")) as i64
    }

    fn task_id(&self, code: &str) -> i64 {
        1000 + self
            .activities
            .iter()
            .position(|a| a.id == code)
            .unwrap_or_else(|| panic!("sample table: no activity {code}")) as i64
    }

    fn has_activity(&self, code: &str) -> bool {
        self.activities.iter().any(|a| a.id == code)
    }

    fn wbs_id(&self, code: &str) -> i64 {
        if code == "PROJECT" {
            return 100;
        }
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
    let (days, task_type) = match f[3].as_str() {
        "start" => (None, "TT_Mile"),
        "finish" => (None, "TT_FinMile"),
        "loe" => (None, "TT_LOE"),
        "wbs" => (None, "TT_WBS"),
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
        task_type,
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

fn priority_code(label: &str) -> &'static str {
    match label {
        "Top" => "PT_Top",
        "High" => "PT_High",
        "Normal" => "PT_Normal",
        "Low" => "PT_Low",
        "Lowest" => "PT_Lowest",
        other => panic!("sample table: priority {other:?}"),
    }
}

fn risk_status_code(label: &str) -> &'static str {
    match label {
        "Proposed" => "RS_Propose",
        "Open" => "RS_Open",
        "Active" => "RS_Active",
        "Rejected" => "RS_Reject",
        "Impacted" => "RS_Impact",
        "Managed" => "RS_Manage",
        other => panic!("sample table: risk status {other:?}"),
    }
}

fn risk_response_code(label: &str) -> &'static str {
    match label {
        "Accept" => "RP_Accept",
        "Avoid" => "RP_Avoid",
        "Transfer" => "RP_Trans",
        "Reduce" => "RP_Reduce",
        "Exploit" => "RP_Exploit",
        "Facilitate" => "RP_Facil",
        "Enhance" => "RP_Enhance",
        "Reject" => "RP_Reject",
        other => panic!("sample table: risk response {other:?}"),
    }
}

/// The XER document of the plan: no progress, the data date at the start.
/// `dates` gives the "plan" constraints their dates, and `loe_hours` gives
/// each Level of Effort activity its planned working hours.
fn build(
    d: &Data,
    dates: &BTreeMap<String, String>,
    loe_hours: &BTreeMap<String, f64>,
) -> Document {
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

    // Units of measure.
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

    // OBS (Responsible Managers).
    for (i, o) in d.obs.iter().enumerate() {
        let parent = if o[1].is_empty() {
            String::new()
        } else {
            d.obs_id(&o[1]).to_string()
        };
        add_row(
            doc,
            "OBS",
            &[
                ("obs_id", (i + 1).to_string()),
                ("parent_obs_id", parent),
                ("seq_num", ((i + 1) * 10).to_string()),
                ("obs_name", o[2].clone()),
            ],
        );
    }

    // Roles and role rates (hierarchical).
    let role_id = |code: &str| {
        d.roles
            .iter()
            .position(|r| r[0] == code)
            .map(|i| (i + 1).to_string())
            .unwrap_or_default()
    };
    for (i, r) in d.roles.iter().enumerate() {
        let id = (i + 1).to_string();
        let parent = if r[1].is_empty() {
            String::new()
        } else {
            role_id(&r[1])
        };
        add_row(
            doc,
            "ROLES",
            &[
                ("role_id", id.clone()),
                ("parent_role_id", parent),
                ("seq_num", ((i + 1) * 10).to_string()),
                ("role_name", r[2].clone()),
                ("role_short_name", r[0].clone()),
                ("def_cost_qty_link_flag", "Y".into()),
                ("cost_qty_type", "QT_Hour".into()),
            ],
        );
        if let Some(rate) = r.get(3).filter(|s| !s.is_empty()) {
            let limit = r.get(4).cloned().unwrap_or_default();
            add_row(
                doc,
                "ROLERATE",
                &[
                    ("role_rate_id", id.clone()),
                    ("role_id", id),
                    ("cost_per_qty", cost(number(rate))),
                    ("start_date", "2026-01-01 00:00".into()),
                    ("max_qty_per_hr", limit),
                ],
            );
        }
    }

    // Cost accounts (hierarchical).
    let account_id = |code: &str| {
        d.accounts
            .iter()
            .position(|a| a[0] == code)
            .map(|i| (i + 1).to_string())
            .unwrap_or_else(|| panic!("sample table: no account {code}"))
    };
    for (i, a) in d.accounts.iter().enumerate() {
        let parent = if a[1].is_empty() {
            String::new()
        } else {
            account_id(&a[1])
        };
        add_row(
            doc,
            "ACCOUNT",
            &[
                ("acct_id", (i + 1).to_string()),
                ("parent_acct_id", parent),
                ("acct_seq_num", ((i + 1) * 10).to_string()),
                ("acct_name", a[2].clone()),
                ("acct_short_name", a[0].clone()),
            ],
        );
    }

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

    for (i, b) in d.baseline_types.iter().enumerate() {
        add_row(
            doc,
            "BASETYPE",
            &[
                ("base_type_id", (i + 1).to_string()),
                ("base_type_seq_num", ((i + 1) * 10).to_string()),
                ("base_type", b.clone()),
            ],
        );
    }

    // Funding sources and project funding.
    for (i, f) in d.funding.iter().enumerate() {
        let id = (i + 1).to_string();
        add_row(
            doc,
            "FUNDSRC",
            &[
                ("fund_id", id.clone()),
                ("seq_num", ((i + 1) * 10).to_string()),
                ("fund_name", f[0].clone()),
                ("fund_descr", f.get(3).cloned().unwrap_or_default()),
            ],
        );
        add_row(
            doc,
            "PROJFUND",
            &[
                ("proj_fund_id", id.clone()),
                ("proj_id", "1".into()),
                ("fund_id", id),
                ("fund_cost", cost(number(&f[1]))),
                ("fund_wt", text(number(&f[2]))),
            ],
        );
    }

    // Project codes.
    let mut pcat_types: Vec<String> = vec![];
    for c in &d.project_codes {
        if !pcat_types.contains(&c[0]) {
            pcat_types.push(c[0].clone());
            let n = pcat_types.len();
            add_row(
                doc,
                "PCATTYPE",
                &[
                    ("proj_catg_type_id", n.to_string()),
                    ("seq_num", (n * 10).to_string()),
                    ("proj_catg_short_len", "10".into()),
                    ("proj_catg_type", c[0].clone()),
                    ("export_flag", "Y".into()),
                ],
            );
        }
    }
    for (i, c) in d.project_codes.iter().enumerate() {
        let ty = pcat_types.iter().position(|t| *t == c[0]).unwrap() + 1;
        let val = i + 1;
        add_row(
            doc,
            "PCATVAL",
            &[
                ("proj_catg_id", val.to_string()),
                ("proj_catg_type_id", ty.to_string()),
                ("seq_num", (val * 10).to_string()),
                ("proj_catg_short_name", c[1].clone()),
                ("proj_catg_name", c[2].clone()),
            ],
        );
        if c.get(3).is_some_and(|v| v.eq_ignore_ascii_case("yes")) {
            add_row(
                doc,
                "PROJPCAT",
                &[
                    ("proj_id", "1".into()),
                    ("proj_catg_type_id", ty.to_string()),
                    ("proj_catg_id", val.to_string()),
                ],
            );
        }
    }

    // Resource codes.
    let mut rcat_types: Vec<String> = vec![];
    for c in &d.resource_codes {
        if !rcat_types.contains(&c[0]) {
            rcat_types.push(c[0].clone());
            let n = rcat_types.len();
            add_row(
                doc,
                "RCATTYPE",
                &[
                    ("rsrc_catg_type_id", n.to_string()),
                    ("seq_num", (n * 10).to_string()),
                    ("rsrc_catg_short_len", "10".into()),
                    ("rsrc_catg_type", c[0].clone()),
                    ("export_flag", "Y".into()),
                ],
            );
        }
    }
    for (i, c) in d.resource_codes.iter().enumerate() {
        let ty = rcat_types.iter().position(|t| *t == c[0]).unwrap() + 1;
        add_row(
            doc,
            "RCATVAL",
            &[
                ("rsrc_catg_id", (i + 1).to_string()),
                ("rsrc_catg_type_id", ty.to_string()),
                ("seq_num", ((i + 1) * 10).to_string()),
                ("rsrc_catg_short_name", c[1].clone()),
                ("rsrc_catg_name", c[2].clone()),
            ],
        );
    }
    let rcat_id = |ty: &str, short: &str| -> Option<(usize, usize)> {
        let t = rcat_types.iter().position(|t| t == ty)? + 1;
        let v = d
            .resource_codes
            .iter()
            .position(|c| c[0] == ty && c[1] == short)?
            + 1;
        Some((t, v))
    };

    // Assignment codes.
    let mut acat_types: Vec<String> = vec![];
    for c in &d.assignment_codes {
        if !acat_types.contains(&c[0]) {
            acat_types.push(c[0].clone());
            let n = acat_types.len();
            add_row(
                doc,
                "ASGNMNTCATTYPE",
                &[
                    ("asgnmnt_catg_type_id", n.to_string()),
                    ("seq_num", (n * 10).to_string()),
                    ("asgnmnt_catg_short_len", "10".into()),
                    ("asgnmnt_catg_type", c[0].clone()),
                    ("export_flag", "Y".into()),
                ],
            );
        }
    }
    for (i, c) in d.assignment_codes.iter().enumerate() {
        let ty = acat_types.iter().position(|t| *t == c[0]).unwrap() + 1;
        add_row(
            doc,
            "ASGNMNTCATVAL",
            &[
                ("asgnmnt_catg_id", (i + 1).to_string()),
                ("asgnmnt_catg_type_id", ty.to_string()),
                ("seq_num", ((i + 1) * 10).to_string()),
                ("asgnmnt_catg_short_name", c[1].clone()),
                ("asgnmnt_catg_name", c[2].clone()),
            ],
        );
    }
    let acat_id = |ty: &str, short: &str| -> (usize, usize) {
        let t = acat_types.iter().position(|t| t == ty).unwrap() + 1;
        let v = d
            .assignment_codes
            .iter()
            .position(|c| c[0] == ty && c[1] == short)
            .unwrap_or_else(|| panic!("sample table: no assignment code {ty}/{short}"))
            + 1;
        (t, v)
    };

    // Document categories & statuses.
    for (i, c) in d.doc_categories.iter().enumerate() {
        add_row(
            doc,
            "DOCCATG",
            &[
                ("doc_catg_id", (i + 1).to_string()),
                ("seq_num", ((i + 1) * 10).to_string()),
                ("doc_catg_name", c.clone()),
            ],
        );
    }
    for (i, s) in d.doc_statuses.iter().enumerate() {
        add_row(
            doc,
            "DOCSTAT",
            &[
                ("doc_status_id", (i + 1).to_string()),
                ("seq_num", ((i + 1) * 10).to_string()),
                ("doc_status_code", s.clone()),
            ],
        );
    }

    // Risk categories.
    for (i, r) in d.risk_categories.iter().enumerate() {
        add_row(
            doc,
            "RISKTYPE",
            &[
                ("risk_type_id", (i + 1).to_string()),
                ("seq_num", ((i + 1) * 10).to_string()),
                ("risk_type", r.clone()),
            ],
        );
    }

    // Financial period calendar and periods.
    if let Some(first_fp) = d.financial_periods.first() {
        add_row(
            doc,
            "FINTMPL",
            &[
                ("fintmpl_id", "1".into()),
                ("fintmpl_name", first_fp[0].clone()),
                ("default_flag", "Y".into()),
            ],
        );
        for (i, fp) in d.financial_periods.iter().enumerate() {
            add_row(
                doc,
                "FINDATES",
                &[
                    ("fin_dates_id", (i + 1).to_string()),
                    ("fin_dates_name", fp[1].clone()),
                    ("start_date", fp[2].clone()),
                    ("end_date", fp[3].clone()),
                    ("fintmpl_id", "1".into()),
                ],
            );
        }
    }

    // Standard threshold parameters (matching xer::edit::thresholds::BUILT_IN).
    for (id, seq, wbs_f, task_f, kind, pname, field) in [
        (
            15,
            1,
            "Y",
            "Y",
            "TH_Drtn",
            "Start Date Variance (days)",
            "var_start_date_drtn_hr_cnt",
        ),
        (
            16,
            2,
            "Y",
            "Y",
            "TH_Drtn",
            "Finish Date Variance (days)",
            "var_end_date_drtn_hr_cnt",
        ),
        (
            17,
            3,
            "Y",
            "Y",
            "TH_Drtn",
            "Total Float (days)",
            "total_float_hr_cnt",
        ),
        (
            18,
            4,
            "N",
            "Y",
            "TH_Drtn",
            "Free Float (days)",
            "free_float_hr_cnt",
        ),
        (
            11,
            5,
            "Y",
            "Y",
            "TH_Percent",
            "Duration % of Original (%)",
            "drtn_pct_spent",
        ),
        (
            5,
            6,
            "Y",
            "Y",
            "TH_Percent",
            "Cost % of Budget (%)",
            "act_pct_spent",
        ),
        (
            1,
            7,
            "Y",
            "Y",
            "TH_Money",
            "AV - Accounting Variance ($)",
            "AV",
        ),
        (2, 8, "Y", "Y", "TH_Money", "CV - Cost Variance ($)", "CV"),
        (
            3,
            9,
            "Y",
            "Y",
            "TH_Money",
            "SV - Schedule Variance ($)",
            "SV",
        ),
        (
            4,
            10,
            "Y",
            "Y",
            "TH_Money",
            "VAC - Variance at Completion ($)",
            "VAC",
        ),
        (
            6,
            11,
            "Y",
            "Y",
            "TH_Ratio",
            "CPI - Cost Performance Index (ratio)",
            "CPI",
        ),
        (
            7,
            12,
            "Y",
            "Y",
            "TH_Ratio",
            "CVI - Cost Variance Index (ratio)",
            "CVI",
        ),
        (
            9,
            13,
            "Y",
            "Y",
            "TH_Ratio",
            "SPI - Schedule Performance Index (ratio)",
            "SPI",
        ),
        (
            10,
            14,
            "Y",
            "Y",
            "TH_Ratio",
            "SVI - Schedule Variance Index (ratio)",
            "SVI",
        ),
    ] {
        add_row(
            doc,
            "THRSPARM",
            &[
                ("thresh_parm_id", id.to_string()),
                ("seq_num", seq.to_string()),
                ("wbs_flag", wbs_f.into()),
                ("task_flag", task_f.into()),
                ("rsrc_flag", "N".into()),
                ("thresh_parm_type", kind.into()),
                ("thresh_parm_name", pname.into()),
                ("thresh_field_name", field.into()),
                ("thresh_short_name", field.into()),
            ],
        );
    }

    // UDF types (TASK and PROJECT).
    for (i, u) in d.udfs.iter().enumerate() {
        let data_type = match u[2].as_str() {
            "number" => "FT_INT",
            "cost" => "FT_MONEY",
            "start_date" => "FT_START_DATE",
            "finish_date" => "FT_END_DATE",
            _ => "FT_TEXT",
        };
        add_row(
            doc,
            "UDFTYPE",
            &[
                ("udf_type_id", (i + 1).to_string()),
                ("table_name", u[0].clone()),
                ("udf_type_name", format!("user_field_{}", i + 1)),
                ("udf_type_label", u[1].clone()),
                ("logical_data_type", data_type.into()),
                ("super_flag", "N".into()),
                ("export_flag", "Y".into()),
            ],
        );
    }

    doc.table_mut("PROJECT").add(&[
        ("proj_id", "1".into()),
        ("proj_short_name", short.clone()),
        ("clndr_id", "5".into()),
        ("fintmpl_id", "1".into()),
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
        ("priority_num", "10".into()),
        ("rsrc_self_add_flag", "Y".into()),
        ("allow_complete_flag", "Y".into()),
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
    let root_obs = d
        .obs
        .first()
        .map(|o| d.obs_id(&o[0]).to_string())
        .unwrap_or_default();
    add_row(
        doc,
        "PROJWBS",
        &[
            ("wbs_id", "100".into()),
            ("proj_id", "1".into()),
            ("obs_id", root_obs),
            ("seq_num", "0".into()),
            ("proj_node_flag", "Y".into()),
            ("status_code", "WS_Open".into()),
            ("wbs_short_name", short.clone()),
            ("wbs_name", name.clone()),
            ("orig_cost", "5195000".into()),
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
        let segment = w[0].rsplit('.').next().unwrap().to_string();
        let obs = w
            .get(3)
            .filter(|s| !s.is_empty())
            .map(|c| d.obs_id(c).to_string())
            .unwrap_or_default();
        let orig_cost = w.get(4).cloned().unwrap_or_default();
        add_row(
            doc,
            "PROJWBS",
            &[
                ("wbs_id", d.wbs_id(&w[0]).to_string()),
                ("proj_id", "1".into()),
                ("obs_id", obs),
                ("seq_num", (*seq * 10).to_string()),
                ("proj_node_flag", "N".into()),
                ("status_code", "WS_Open".into()),
                ("wbs_short_name", segment),
                ("wbs_name", w[2].clone()),
                ("parent_wbs_id", parent.to_string()),
                ("orig_cost", orig_cost),
            ],
        );
    }

    // WBS Milestones (WBSSTEP).
    let mut wbs_step_seq: BTreeMap<i64, usize> = BTreeMap::new();
    for (i, s) in d.wbs_steps.iter().enumerate() {
        let wbs_id = d.wbs_id(&s[0]);
        let seq = wbs_step_seq.entry(wbs_id).or_default();
        *seq += 1;
        add_row(
            doc,
            "WBSSTEP",
            &[
                ("wbs_step_id", (i + 1).to_string()),
                ("proj_id", "1".into()),
                ("wbs_id", wbs_id.to_string()),
                ("seq_num", (*seq * 10).to_string()),
                ("step_name", s[1].clone()),
                ("step_wt", s[2].clone()),
                ("complete_flag", s[3].clone()),
            ],
        );
    }

    // Budget Log (BUDGCHNG).
    for (i, b) in d.budget_log.iter().enumerate() {
        add_row(
            doc,
            "BUDGCHNG",
            &[
                ("budg_chng_id", (i + 1).to_string()),
                ("proj_id", "1".into()),
                ("wbs_id", d.wbs_id(&b[0]).to_string()),
                ("chng_num", b[1].clone()),
                ("chng_date", b[2].clone()),
                ("chng_cost", cost(number(&b[3]))),
                ("status_code", b[4].clone()),
                ("chng_by_name", b[5].clone()),
                ("chng_descr", b[6].clone()),
            ],
        );
    }

    // Spending Plan (WBSBUDG).
    for (i, s) in d.spending_plan.iter().enumerate() {
        add_row(
            doc,
            "WBSBUDG",
            &[
                ("wbs_budg_id", (i + 1).to_string()),
                ("proj_id", "1".into()),
                ("wbs_id", d.wbs_id(&s[0]).to_string()),
                ("start_date", s[1].clone()),
                ("end_date", s[2].clone()),
                ("spend_cost", cost(number(&s[3]))),
                ("benefit_cost", cost(number(&s[4]))),
            ],
        );
    }

    // Resources (2-level hierarchy), rates, roles and resource codes.
    for (i, r) in d.resources.iter().enumerate() {
        let id = (i + 1).to_string();
        let parent = if r.parent.is_empty() {
            String::new()
        } else {
            (d.resource(&r.parent).0 + 1).to_string()
        };
        let kind = r.xer_type();
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
                ("parent_rsrc_id", parent),
                ("clndr_id", r.calendar.to_string()),
                ("role_id", role_id(&r.role)),
                ("rsrc_seq_num", ((i + 1) * 10).to_string()),
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
        if !r.is_group() {
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
                        ("rsrc_id", id.clone()),
                        ("role_id", role_id(&r.role)),
                        ("skill_level", "3".into()),
                    ],
                );
            }
        }
        if !r.trade.is_empty() {
            if let Some((ty, val)) = rcat_id("Trade Group", &r.trade) {
                add_row(
                    doc,
                    "RSRCRCAT",
                    &[
                        ("rsrc_id", id.clone()),
                        ("rsrc_catg_type_id", ty.to_string()),
                        ("rsrc_catg_id", val.to_string()),
                    ],
                );
            }
        }
        if !r.source.is_empty() {
            if let Some((ty, val)) = rcat_id("Resource Source", &r.source) {
                add_row(
                    doc,
                    "RSRCRCAT",
                    &[
                        ("rsrc_id", id),
                        ("rsrc_catg_type_id", ty.to_string()),
                        ("rsrc_catg_id", val.to_string()),
                    ],
                );
            }
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
                    ("export_flag", "Y".into()),
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
        let duration = if a.task_type == "TT_LOE" {
            loe_hours.get(&a.id).copied().unwrap_or(0.)
        } else {
            a.days.map_or(0., |days| days * hours)
        };
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
            ("task_type", a.task_type.into()),
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
        let shift_code = match a.calendar {
            5 => "OFFICE",
            7 => "OUTAGE",
            _ => "DAY",
        };
        let location_code = if a.wbs == "FAB" {
            "SHOP"
        } else if a.calendar == 5 {
            "HOME"
        } else {
            "FIELD"
        };
        for (rsrc, units) in &a.resources {
            assignment += 1;
            let (i, r) = d.resource(rsrc);
            let (qty, per_hour) = if r.kind == "material" {
                (*units, if duration > 0. { units / duration } else { 0. })
            } else {
                (units * duration, *units)
            };
            let kind = r.xer_type();
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
            let billing_code = match r.kind.as_str() {
                "material" => "MATL",
                "nonlabor" => "EQUIP",
                _ if r.role == "STAFF" || r.role == "SUPV" => "INDIRECT",
                _ => "DIRECT",
            };
            for (ty, val_short) in [
                ("Shift", shift_code),
                ("Work Location", location_code),
                ("Billing Type", billing_code),
            ] {
                let (t, v) = acat_id(ty, val_short);
                add_row(
                    doc,
                    "ASGNMNTACAT",
                    &[
                        ("taskrsrc_id", assignment.to_string()),
                        ("asgnmnt_catg_type_id", t.to_string()),
                        ("asgnmnt_catg_id", v.to_string()),
                        ("proj_id", "1".into()),
                    ],
                );
            }
        }
        // Codes: area from the WBS, discipline from the activity ID,
        // phase from the top of the WBS, responsibility from the crew.
        let area = match &a.wbs[..a.wbs.len().min(4)] {
            "A100" => "100",
            "A200" => "200",
            "A300" => "300",
            "A400" => "400",
            "A500" => "500",
            "A600" => "600",
            _ => "GEN",
        };
        let discipline = match &a.id[..2] {
            "MS" | "PM" | "LO" | "WS" => "PM",
            "EN" => "ENG",
            "PR" => "PRC",
            "DM" => "DEM",
            "CV" | "ST" => "CIV",
            "EQ" => "EQP",
            "PP" | "UT" => "PIP",
            "EL" => "ELE",
            "IN" => "INS",
            "DC" => "AUT",
            "PI" => "PNT",
            _ => "COM",
        };
        let mut top = a.wbs.clone();
        while let Some(w) = d.wbs.iter().find(|w| w[0] == top) {
            if w[1].is_empty() {
                break;
            }
            top = w[1].clone();
        }
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
            let automation = craft(&["AUTO"]);
            let electrical = craft(&["ELEC", "INST"]);
            let mechanical = craft(&["PF", "WLD", "MW"]);
            if automation > 0. && automation >= electrical && automation >= mechanical {
                "AUTO"
            } else if electrical > mechanical && electrical > 0. {
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
                "GF", "LAB", "CARP", "IW", "CFIN", "PF", "WLD", "ELEC", "INST", "MW", "PNT",
                "INSUL", "OPR",
            ]);
            udf(doc, d, "TASK", task_id, "Work Package", &wp);
            udf(doc, d, "TASK", task_id, "Crew Size", &text(crew.ceil()));
        }
    }

    // Explicit UDF values (PROJECT or TASK).
    for v in &d.udf_values {
        if v[0] == "PROJECT" {
            udf(doc, d, "PROJECT", 1, &v[1], &v[2]);
        } else {
            udf(doc, d, "TASK", d.task_id(&v[0]), &v[1], &v[2]);
        }
    }

    // Activity steps.
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

    // Expenses.
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

    // Resource curves.
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

    // Work Products and Documents (DOCUMENT, TASKDOC).
    let doc_id = |code: &str| -> i64 {
        1 + d
            .documents
            .iter()
            .position(|x| x[0] == code)
            .unwrap_or_else(|| panic!("sample table: no document {code}")) as i64
    };
    let mut taskdoc_id = 0i64;
    for (i, item) in d.documents.iter().enumerate() {
        let id = (i + 1) as i64;
        let parent = if item[1].is_empty() {
            String::new()
        } else {
            doc_id(&item[1]).to_string()
        };
        let cat = d
            .doc_categories
            .iter()
            .position(|c| *c == item[4])
            .unwrap_or_else(|| panic!("sample table: no doc category {}", item[4]))
            + 1;
        let stat = d
            .doc_statuses
            .iter()
            .position(|s| *s == item[5])
            .unwrap_or_else(|| panic!("sample table: no doc status {}", item[5]))
            + 1;
        add_row(
            doc,
            "DOCUMENT",
            &[
                ("doc_id", id.to_string()),
                ("doc_seq_num", ((i + 1) * 10).to_string()),
                ("deliv_flag", item[9].clone()),
                ("doc_name", item[3].clone()),
                ("parent_doc_id", parent),
                ("proj_id", "1".into()),
                ("doc_status_id", stat.to_string()),
                ("doc_catg_id", cat.to_string()),
                ("doc_date", item[8].clone()),
                ("version_name", item[6].clone()),
                ("doc_short_name", item[2].clone()),
                ("author_name", item[7].clone()),
                ("doc_content", records::text_to_memo(&item[11])),
            ],
        );
        for target in item[10].split(',').map(str::trim).filter(|t| !t.is_empty()) {
            taskdoc_id += 1;
            let (tid, wid) = if d.has_activity(target) {
                let tid = d.task_id(target);
                let act = &d.activities[(tid - 1000) as usize];
                (tid.to_string(), d.wbs_id(&act.wbs).to_string())
            } else {
                (String::new(), d.wbs_id(target).to_string())
            };
            add_row(
                doc,
                "TASKDOC",
                &[
                    ("taskdoc_id", taskdoc_id.to_string()),
                    ("doc_id", id.to_string()),
                    ("proj_id", "1".into()),
                    ("wbs_id", wid),
                    ("wp_flag", item[9].clone()),
                    ("task_id", tid),
                ],
            );
        }
    }

    // Risks (PROJRISK, TASKRISK).
    for (i, r) in d.risks.iter().enumerate() {
        let id = (i + 1) as i64;
        let cat = d
            .risk_categories
            .iter()
            .position(|c| *c == r[2])
            .unwrap_or_else(|| panic!("sample table: no risk category {}", r[2]))
            + 1;
        let kind = match r[3].as_str() {
            "Threat" => "RT_Threat",
            "Opportunity" => "RT_Opport",
            other => panic!("sample table: risk kind {other:?}"),
        };
        let owner = (d.resource(&r[5]).0 + 1).to_string();
        let by = (d.resource(&r[6]).0 + 1).to_string();
        add_row(
            doc,
            "PROJRISK",
            &[
                ("risk_id", id.to_string()),
                ("risk_code", r[0].clone()),
                ("proj_id", "1".into()),
                ("add_date", r[7].clone()),
                ("status_code", risk_status_code(&r[4]).into()),
                ("risk_name", r[1].clone()),
                ("risk_type_id", cat.to_string()),
                ("rsrc_id", owner),
                ("risk_desc", r[16].clone()),
                ("risk_to_type", kind.into()),
                ("identified_by_id", by),
                ("response_type", risk_response_code(&r[8]).into()),
                ("response_text", r[19].clone()),
                ("pre_rsp_prblty", r[9].clone()),
                ("pre_rsp_schd_prblty", r[10].clone()),
                ("pre_rsp_cost_prblty", r[11].clone()),
                ("post_rsp_prblty", r[12].clone()),
                ("post_rsp_schd_prblty", r[13].clone()),
                ("post_rsp_cost_prblty", r[14].clone()),
                ("risk_cause", r[17].clone()),
                ("risk_effect", r[18].clone()),
                ("notes", r[20].clone()),
            ],
        );
        for code in r[15].split(',').map(str::trim).filter(|c| !c.is_empty()) {
            add_row(
                doc,
                "TASKRISK",
                &[
                    ("task_id", d.task_id(code).to_string()),
                    ("risk_id", id.to_string()),
                    ("proj_id", "1".into()),
                ],
            );
        }
    }

    // Thresholds (PROJTHRS).
    for (i, th) in d.thresholds.iter().enumerate() {
        let detail = match th[2].as_str() {
            "Activities" => "TM_Task",
            "WBS" => "TM_WBS",
            other => panic!("sample table: threshold detail {other:?}"),
        };
        let status = match th[4].as_str() {
            "Enabled" => "TS_Enable",
            "Disabled" => "TS_Disable",
            other => panic!("sample table: threshold status {other:?}"),
        };
        add_row(
            doc,
            "PROJTHRS",
            &[
                ("thresh_id", (i + 1).to_string()),
                ("proj_id", "1".into()),
                ("obs_id", d.obs_id(&th[5]).to_string()),
                ("thresh_parm_id", th[1].clone()),
                ("status_code", status.into()),
                ("priority_type", priority_code(&th[3]).into()),
                ("thresh_type", detail.into()),
                ("wbs_id", d.wbs_id(&th[0]).to_string()),
                ("lo_parm_value", th[6].clone()),
                ("hi_parm_value", th[7].clone()),
                ("window_start", th[8].clone()),
                ("window_end", th[9].clone()),
            ],
        );
    }

    // Issues (PROJISSU, ISSUHIST).
    for (i, iss) in d.issues.iter().enumerate() {
        let id = (i + 1) as i64;
        let status = match iss[2].as_str() {
            "Open" => "IS_Open",
            "On Hold" => "IS_On_Hold",
            "Closed" => "IS_Closed",
            other => panic!("sample table: issue status {other:?}"),
        };
        let (tid, wid) = if d.has_activity(&iss[7]) {
            let tid = d.task_id(&iss[7]);
            let act = &d.activities[(tid - 1000) as usize];
            (tid.to_string(), d.wbs_id(&act.wbs).to_string())
        } else {
            (String::new(), d.wbs_id(&iss[7]).to_string())
        };
        add_row(
            doc,
            "PROJISSU",
            &[
                ("issue_id", id.to_string()),
                ("obs_id", d.obs_id(&iss[3]).to_string()),
                ("proj_id", "1".into()),
                ("task_id", tid),
                ("thresh_parm_id", iss[8].clone()),
                ("wbs_id", wid),
                ("add_by_name", iss[4].clone()),
                ("issue_name", iss[0].clone()),
                ("add_date", iss[5].clone()),
                ("resolv_date", iss[6].clone()),
                ("priority_type", priority_code(&iss[1]).into()),
                ("status_code", status.into()),
                ("issue_value", iss[9].clone()),
                ("lo_parm_value", iss[10].clone()),
                ("hi_parm_value", iss[11].clone()),
                ("issue_notes", records::text_to_memo(&iss[12])),
            ],
        );
        if let Some(hist) = iss.get(13).filter(|s| !s.is_empty()) {
            add_row(
                doc,
                "ISSUHIST",
                &[
                    ("issue_id", id.to_string()),
                    ("proj_id", "1".into()),
                    ("issue_history", hist.clone()),
                ],
            );
        }
    }

    // Notebook notes (TASKMEMO, WBSMEMO).
    for (i, n) in d.notes.iter().enumerate() {
        let topic = d
            .topics
            .iter()
            .position(|t| *t == n[1])
            .unwrap_or_else(|| panic!("sample table: no topic {}", n[1]))
            + 1;
        let memo = records::text_to_memo(&n[2]);
        if d.has_activity(&n[0]) {
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
            let wbs = d.wbs_id(&n[0]);
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

fn udf(doc: &mut Document, d: &Data, table: &str, fk_id: i64, label: &str, value: &str) {
    let i = d
        .udfs
        .iter()
        .position(|u| u[0] == table && u[1] == label)
        .unwrap_or_else(|| panic!("sample table: no {table} user field {label}"));
    let column = match d.udfs[i][2].as_str() {
        "number" | "cost" => "udf_number",
        "start_date" | "finish_date" => "udf_date",
        _ => "udf_text",
    };
    add_row(
        doc,
        "UDFVALUE",
        &[
            ("udf_type_id", (i + 1).to_string()),
            ("fk_id", fk_id.to_string()),
            ("proj_id", "1".into()),
            (column, value.to_string()),
        ],
    );
}

// ------------------------------------------------------------- progress

/// An activity at the data date: status, the share of its work done and
/// the share left (of the original duration), actual start and finish.
type State = (StatusType, f64, f64, String, String);

/// The plan progressed to `date`: what the plan had done by then is done, what
/// straddles it is in progress, with `slips` added to the remaining work and
/// `overruns` to the actual units. When `store_past_periods` is true, completed
/// financial periods prior to `date` receive stored period performance
/// (`TASKFIN` and `TRSRCFIN`).
fn progress(
    d: &Data,
    plan: &ProjectData,
    date: &str,
    slips: &[(String, f64)],
    overruns: &[(String, f64)],
    store_past_periods: bool,
) -> ProjectData {
    let mut p = plan.clone();
    let origin = p.model.origin_serial;
    let dd = parse_absolute(date).expect("data date") - origin * 1440;
    apply_progress(&mut p, dd, ProgressScope::All, RemainingRule::Always).expect("progress");

    // Progress LOE activities whose start anchor (SS predecessor) has started
    // or completed by the data date, so the engine tracks them as In Progress.
    let loe_updates: Vec<(usize, i64, i64, f64)> = p
        .model
        .activities
        .iter()
        .enumerate()
        .filter(|(_, a)| a.task_type == TaskType::Loe)
        .filter_map(|(idx, a)| {
            let ss_pred = a
                .preds
                .iter()
                .map(|&l| &p.model.links[l])
                .find(|l| l.link_type == LinkType::SS)
                .map(|l| &p.model.activities[l.pred])?;
            let act_start = ss_pred.act_start?;
            if act_start >= dd {
                return None;
            }
            let cal = &p.model.calendars[a.cal];
            let orig = p
                .details
                .get(&a.task_id)
                .map_or(0, |dt| dt.original_duration);
            let elapsed = cal.cal.duration(act_start, dd);
            let rem = (orig - elapsed).max(60);
            let pct = if orig > 0 {
                ((elapsed as f64 / orig as f64) * 100.).clamp(1., 99.)
            } else {
                10.
            };
            Some((idx, act_start, rem, pct))
        })
        .collect();
    for (idx, act_start, rem, pct) in loe_updates {
        let a = &mut p.model.activities[idx];
        a.status = StatusType::InProgress;
        a.act_start = Some(act_start);
        a.rem = rem;
        if let Some(dt) = p.details.get_mut(&a.task_id) {
            dt.percent = pct;
        }
    }

    // Planned remaining before the slips, by task id.
    let planned: BTreeMap<i64, i64> = p
        .model
        .activities
        .iter()
        .map(|a| (a.task_id, a.rem))
        .collect();
    for (code, days) in slips {
        let task_id = d.task_id(code);
        let a = p
            .model
            .activities
            .iter_mut()
            .find(|a| a.task_id == task_id)
            .unwrap();
        assert_eq!(a.status, StatusType::InProgress, "slip on {code}");
        let hours = calendar_hours(d.activities[(task_id - 1000) as usize].calendar);
        a.rem += (*days * hours * 60.) as i64;
    }
    let overrun: BTreeMap<i64, f64> = overruns
        .iter()
        .map(|(code, factor)| (d.task_id(code), *factor))
        .collect();
    let mut doc = p.updated_document();
    let state: BTreeMap<i64, State> = p
        .model
        .activities
        .iter()
        .map(|a| {
            let original = p.details[&a.task_id].original_duration as f64;
            let done = if original > 0. {
                ((original - planned[&a.task_id] as f64) / original).clamp(0., 1.)
            } else {
                1.
            };
            let left = if original > 0. {
                (a.rem as f64 / original).max(0.)
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
    if store_past_periods {
        populate_past_period_actuals(&mut doc, d, &p, dd);
    }
    let mut current = load(doc);
    sync_rollups(&mut current);
    current
}

/// Populate `TASKFIN` and `TRSRCFIN` rows for completed financial periods
/// ending before the data date, distributing each started/completed activity's
/// actuals across the months in which the work occurred.
fn populate_past_period_actuals(doc: &mut Document, d: &Data, p: &ProjectData, dd: i64) {
    let origin = p.model.origin_serial;
    let abs_dd = origin * 1440 + dd;
    let periods: Vec<(i64, i64, i64)> = d
        .financial_periods
        .iter()
        .enumerate()
        .filter_map(|(i, fp)| {
            let s = parse_absolute(&fp[2])?;
            let e = parse_absolute(&fp[3])?;
            (e < abs_dd).then_some(((i + 1) as i64, s, e + 1))
        })
        .collect();
    if periods.is_empty() {
        return;
    }
    if let Some((last_id, _, _)) = periods.last() {
        let proj = doc.table_mut("PROJECT");
        if !proj.rows.is_empty() {
            proj.set(0, "last_fin_dates_id", last_id.to_string());
        }
    }
    // Per activity: (act_start_abs, act_end_or_dd_abs).
    let spans: BTreeMap<i64, (i64, i64)> = p
        .model
        .activities
        .iter()
        .filter_map(|a| {
            let s = a.act_start? + origin * 1440;
            let e = a
                .act_finish
                .map_or(abs_dd, |f| f + origin * 1440)
                .max(s + 1);
            Some((a.task_id, (s, e)))
        })
        .collect();
    let share_in = |task_id: i64, ps: i64, pe: i64| -> f64 {
        let Some(&(s, e)) = spans.get(&task_id) else {
            return 0.;
        };
        let lo = s.max(ps);
        let hi = e.min(pe);
        if hi <= lo {
            return 0.;
        }
        (hi - lo) as f64 / (e - s) as f64
    };

    // Snapshot assignment actuals before adding TRSRCFIN/TASKFIN rows.
    let mut assign_rows: Vec<(i64, i64, String, f64, f64)> = vec![];
    if let Some(t) = doc.table("TASKRSRC") {
        for r in &t.rows {
            let act_qty = records::float(t.value(r, "act_reg_qty"));
            let act_cost = records::float(t.value(r, "act_reg_cost"));
            if act_qty <= 0. && act_cost <= 0. {
                continue;
            }
            assign_rows.push((
                records::num(t.value(r, "taskrsrc_id")),
                records::num(t.value(r, "task_id")),
                t.value(r, "rsrc_type").to_string(),
                act_qty,
                act_cost,
            ));
        }
    }
    let mut expense_rows: Vec<(i64, f64)> = vec![];
    if let Some(t) = doc.table("PROJCOST") {
        for r in &t.rows {
            let act_cost = records::float(t.value(r, "act_cost"));
            if act_cost > 0. {
                expense_rows.push((records::num(t.value(r, "task_id")), act_cost));
            }
        }
    }

    // Aggregate per (fin_dates_id, task_id):
    // [labor_qty, labor_cost, equip_qty, equip_cost, mat_cost, expense_cost]
    let mut task_fin: BTreeMap<(i64, i64), [f64; 6]> = BTreeMap::new();
    for &(fid, ps, pe) in &periods {
        for &(tr_id, tid, ref kind, qty, amt) in &assign_rows {
            let f = share_in(tid, ps, pe);
            if f <= 0. {
                continue;
            }
            let pq = qty * f;
            let pc = amt * f;
            add_row(
                doc,
                "TRSRCFIN",
                &[
                    ("fin_dates_id", fid.to_string()),
                    ("taskrsrc_id", tr_id.to_string()),
                    ("task_id", tid.to_string()),
                    ("proj_id", "1".into()),
                    ("act_qty", text(pq)),
                    ("act_cost", cost(pc)),
                ],
            );
            let slot = task_fin.entry((fid, tid)).or_insert([0.; 6]);
            match kind.as_str() {
                "RT_Labor" => {
                    slot[0] += pq;
                    slot[1] += pc;
                }
                "RT_Equip" => {
                    slot[2] += pq;
                    slot[3] += pc;
                }
                _ => slot[4] += pc,
            }
        }
        for &(tid, amt) in &expense_rows {
            let f = share_in(tid, ps, pe);
            if f > 0. {
                task_fin.entry((fid, tid)).or_insert([0.; 6])[5] += amt * f;
            }
        }
    }
    for ((fid, tid), v) in task_fin {
        let total = v[1] + v[3] + v[4] + v[5];
        add_row(
            doc,
            "TASKFIN",
            &[
                ("fin_dates_id", fid.to_string()),
                ("task_id", tid.to_string()),
                ("proj_id", "1".into()),
                ("act_work_qty", text(v[0])),
                ("act_work_cost", cost(v[1])),
                ("act_equip_qty", text(v[2])),
                ("act_equip_cost", cost(v[3])),
                ("act_mat_cost", cost(v[4])),
                ("act_expense_cost", cost(v[5])),
                ("bcwp", cost(total)),
                ("perfm_work_qty", text(v[0])),
                ("bcws", cost(total)),
                ("sched_work_qty", text(v[0])),
            ],
        );
    }
}

#[cfg(test)]
mod tests;
