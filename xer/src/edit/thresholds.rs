//! Thresholds (`PROJTHRS`) and Monitor Thresholds, shared by the desktop's
//! Thresholds view and the command line. A threshold watches one parameter
//! (`THRSPARM`) over the activities or WBS elements of a WBS branch inside a
//! time window; monitoring raises an issue (`PROJISSU`) for every activity or
//! element whose value falls below the lower or above the upper threshold. A
//! second run updates the open issue it raised before instead of adding
//! another.
//!
//! Every parameter is evaluated:
//!
//! - total float, free float, and the start and finish date variances
//!   against the project baseline (the project's own planned dates when it
//!   has none), in days of the activity's calendar;
//! - Duration % of Original: actual duration over original duration, x 100;
//! - Cost % of Budget: actual cost over budget at completion, x 100;
//! - the earned value amounts, as the earned value columns compute them
//!   (the WBS techniques apply): AV = PV - AC (accounting variance),
//!   CV = EV - AC, SV = EV - PV and VAC = BAC - EAC;
//! - the indexes CPI = EV / AC, SPI = EV / PV, CVI = CV / EV and
//!   SVI = SV / PV. An index whose divisor is zero has no value, so it
//!   raises nothing.
//!
//! At WBS level the element sums the amounts of its activities and takes
//! the indexes and percents from the sums. Values are rounded to two
//! decimals. A threshold whose parameter is missing, or one at WBS level on
//! a parameter that has none (free float), is skipped.
use super::project_records;
use super::records::{self, num};
use crate::ev::{self, BlDates, Parts};
use crate::{Document, ProjectData};
use openxer_engine::model::Activity;

pub const STATUS: [(&str, &str); 2] = [("TS_Enable", "Enabled"), ("TS_Disable", "Disabled")];
pub const DETAIL: [(&str, &str); 2] = [("TM_Task", "Activities"), ("TM_WBS", "WBS")];

/// One threshold parameter (a THRSPARM row).
#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    pub id: i64,
    pub name: String,
    /// Column the parameter measures.
    pub field: String,
    pub kind: String,
    pub wbs: bool,
    pub task: bool,
}

/// The parameter set of a new database, used when a file carries no
/// THRSPARM table: (id, sequence, WBS, activity, type, name, field).
const BUILT_IN: [(i64, i64, bool, bool, &str, &str, &str); 14] = [
    (
        15,
        1,
        true,
        true,
        "TH_Drtn",
        "Start Date Variance (days)",
        "var_start_date_drtn_hr_cnt",
    ),
    (
        16,
        2,
        true,
        true,
        "TH_Drtn",
        "Finish Date Variance (days)",
        "var_end_date_drtn_hr_cnt",
    ),
    (
        17,
        3,
        true,
        true,
        "TH_Drtn",
        "Total Float (days)",
        "total_float_hr_cnt",
    ),
    (
        18,
        4,
        false,
        true,
        "TH_Drtn",
        "Free Float (days)",
        "free_float_hr_cnt",
    ),
    (
        11,
        5,
        true,
        true,
        "TH_Percent",
        "Duration % of Original (%)",
        "drtn_pct_spent",
    ),
    (
        5,
        6,
        true,
        true,
        "TH_Percent",
        "Cost % of Budget (%)",
        "act_pct_spent",
    ),
    (
        1,
        7,
        true,
        true,
        "TH_Money",
        "AV - Accounting Variance ($)",
        "AV",
    ),
    (2, 8, true, true, "TH_Money", "CV - Cost Variance ($)", "CV"),
    (
        3,
        9,
        true,
        true,
        "TH_Money",
        "SV - Schedule Variance ($)",
        "SV",
    ),
    (
        4,
        10,
        true,
        true,
        "TH_Money",
        "VAC - Variance at Completion ($)",
        "VAC",
    ),
    (
        6,
        11,
        true,
        true,
        "TH_Ratio",
        "CPI - Cost Performance Index (ratio)",
        "CPI",
    ),
    (
        7,
        12,
        true,
        true,
        "TH_Ratio",
        "CVI - Cost Variance Index (ratio)",
        "CVI",
    ),
    (
        9,
        13,
        true,
        true,
        "TH_Ratio",
        "SPI - Schedule Performance Index (ratio)",
        "SPI",
    ),
    (
        10,
        14,
        true,
        true,
        "TH_Ratio",
        "SVI - Schedule Variance Index (ratio)",
        "SVI",
    ),
];

/// Columns the monitor evaluates (every built-in parameter).
const EVALUATED: [&str; 14] = [
    "total_float_hr_cnt",
    "free_float_hr_cnt",
    "var_start_date_drtn_hr_cnt",
    "var_end_date_drtn_hr_cnt",
    "drtn_pct_spent",
    "act_pct_spent",
    "AV",
    "CV",
    "SV",
    "VAC",
    "CPI",
    "CVI",
    "SPI",
    "SVI",
];

/// Cost and earned value figures of one activity for the monitor: the
/// earned value parts in cost and the original and actual durations in
/// working minutes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Figures {
    pub parts: Parts,
    pub original: f64,
    pub actual: f64,
}

impl Figures {
    fn add(&mut self, o: &Figures) {
        self.parts.bac += o.parts.bac;
        self.parts.pv += o.parts.pv;
        self.parts.ev += o.parts.ev;
        self.parts.ac += o.parts.ac;
        self.parts.etc += o.parts.etc;
        self.original += o.original;
        self.actual += o.actual;
    }
    /// The value of a cost or earned value parameter; `None` when a ratio
    /// has a zero divisor.
    pub fn value(&self, field: &str) -> Option<f64> {
        let x = &self.parts;
        let ratio = |a: f64, b: f64| (b.abs() > 1e-9).then(|| a / b);
        let v = match field {
            "drtn_pct_spent" => ratio(self.actual, self.original)? * 100.,
            "act_pct_spent" => ratio(x.ac, x.bac)? * 100.,
            "AV" => x.pv - x.ac,
            "CV" => x.ev - x.ac,
            "SV" => x.ev - x.pv,
            "VAC" => x.bac - (x.ac + x.etc),
            "CPI" => ratio(x.ev, x.ac)?,
            "SPI" => ratio(x.ev, x.pv)?,
            "CVI" => ratio(x.ev - x.ac, x.ev)?,
            "SVI" => ratio(x.ev - x.pv, x.pv)?,
            _ => return None,
        };
        Some((v * 100.).round() / 100.)
    }
}

/// The threshold parameters: the file's THRSPARM rows, else the built-in set.
pub fn parameters(doc: &Document) -> Vec<Parameter> {
    let mut list: Vec<(i64, Parameter)> = match doc.table("THRSPARM").filter(|t| !t.rows.is_empty())
    {
        Some(t) => t
            .rows
            .iter()
            .map(|r| {
                (
                    num(t.value(r, "seq_num")),
                    Parameter {
                        id: num(t.value(r, "thresh_parm_id")),
                        name: t.value(r, "thresh_parm_name").to_string(),
                        field: t.value(r, "thresh_field_name").to_string(),
                        kind: t.value(r, "thresh_parm_type").to_string(),
                        wbs: t.value(r, "wbs_flag") == "Y",
                        task: t.value(r, "task_flag") == "Y",
                    },
                )
            })
            .collect(),
        None => BUILT_IN
            .iter()
            .map(|&(id, seq, wbs, task, kind, name, field)| {
                (
                    seq,
                    Parameter {
                        id,
                        name: name.into(),
                        field: field.into(),
                        kind: kind.into(),
                        wbs,
                        task,
                    },
                )
            })
            .collect(),
    };
    list.sort_by_key(|(seq, p)| (*seq, p.id));
    list.into_iter().map(|(_, p)| p).collect()
}

/// Write the built-in parameter set when the file has none, so exported
/// thresholds refer to rows that exist.
pub fn ensure_parameters(doc: &mut Document) {
    if doc.table("THRSPARM").is_some_and(|t| !t.rows.is_empty()) {
        return;
    }
    let yn = |b: bool| if b { "Y" } else { "N" }.to_string();
    for (id, seq, wbs, task, kind, name, field) in BUILT_IN {
        records::add_row(
            doc,
            "THRSPARM",
            &[
                ("thresh_parm_id", id.to_string()),
                ("seq_num", seq.to_string()),
                ("wbs_flag", yn(wbs)),
                ("task_flag", yn(task)),
                ("rsrc_flag", "N".into()),
                ("thresh_parm_type", kind.into()),
                ("thresh_parm_name", name.into()),
                ("thresh_field_name", field.into()),
            ],
        );
    }
}

/// One threshold.
#[derive(Clone, Debug, PartialEq)]
pub struct Threshold {
    pub id: i64,
    pub parameter: i64,
    pub low: Option<f64>,
    pub high: Option<f64>,
    pub wbs_level: bool,
    pub wbs_id: i64,
    pub obs_id: String,
    pub priority: String,
    pub enabled: bool,
    pub window_start: String,
    pub window_end: String,
}

fn bound(text: &str) -> Option<f64> {
    let t = text.trim();
    (!t.is_empty()).then(|| records::float(t))
}

pub fn thresholds(doc: &Document, pid: i64) -> Vec<Threshold> {
    let mut list: Vec<Threshold> = project_records::project_rows(doc, "PROJTHRS", pid)
        .into_iter()
        .map(|r| {
            let v = |k: &str| records::get(doc, "PROJTHRS", r, k);
            Threshold {
                id: num(&v("thresh_id")),
                parameter: num(&v("thresh_parm_id")),
                low: bound(&v("lo_parm_value")),
                high: bound(&v("hi_parm_value")),
                wbs_level: v("thresh_type") == "TM_WBS",
                wbs_id: num(&v("wbs_id")),
                obs_id: v("obs_id"),
                priority: v("priority_type"),
                enabled: v("status_code") != "TS_Disable",
                window_start: v("window_start"),
                window_end: v("window_end"),
            }
        })
        .collect();
    list.sort_by_key(|t| t.id);
    list
}

/// Add an enabled total float threshold (issue below zero days).
pub fn add_threshold(p: &mut ProjectData) -> i64 {
    let pid = p.model.project.proj_id;
    let doc = &mut p.document;
    ensure_parameters(doc);
    let parameter = parameters(doc)
        .iter()
        .find(|q| q.field == "total_float_hr_cnt")
        .map_or(17, |q| q.id);
    let id = records::next_id(doc, "PROJTHRS", "thresh_id");
    records::add_row(
        doc,
        "PROJTHRS",
        &[
            ("thresh_id", id.to_string()),
            ("proj_id", pid.to_string()),
            ("thresh_parm_id", parameter.to_string()),
            ("status_code", "TS_Enable".into()),
            ("priority_type", "PT_Normal".into()),
            ("thresh_type", "TM_Task".into()),
            ("lo_parm_value", "0".into()),
        ],
    );
    id
}

/// Delete a threshold. Issues it raised stay, no longer tied to it.
pub fn delete_threshold(p: &mut ProjectData, id: i64) -> bool {
    let doc = &mut p.document;
    let Some(i) = project_records::row_of(doc, "PROJTHRS", "thresh_id", id) else {
        return false;
    };
    records::remove_row(doc, "PROJTHRS", i);
    if let Some(t) = doc.table("PROJISSU") {
        let raised: Vec<usize> = (0..t.rows.len())
            .filter(|&r| num(t.value(&t.rows[r], "thresh_id")) == id)
            .collect();
        for r in raised {
            records::set(doc, "PROJISSU", r, "thresh_id", String::new());
        }
    }
    true
}

/// What one monitor run did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MonitorResult {
    pub created: usize,
    pub updated: usize,
    /// Enabled thresholds whose parameter the monitor cannot evaluate.
    pub skipped: usize,
    pub thresholds: usize,
}

/// A value measured for one activity or WBS element: days, a percent, an
/// amount or a ratio, by the parameter.
struct Measure {
    task_id: i64,
    wbs_id: i64,
    label: String,
    value: f64,
}

/// Working minutes from `from` to `to` on a calendar, negative when `to` is
/// earlier.
fn signed_work(p: &ProjectData, cal: usize, from: i64, to: i64) -> i64 {
    let Some(c) = p.model.calendars.get(cal) else {
        return to - from;
    };
    if to >= from {
        c.between(from, to).max(0)
    } else {
        -c.between(to, from).max(0)
    }
}

fn days(minutes: i64, day: i64) -> f64 {
    let v = minutes as f64 / day.max(1) as f64;
    (v * 100.).round() / 100.
}

/// The WBS elements under `root` (itself included); every element when 0.
fn branch(p: &ProjectData, root: i64) -> Vec<i64> {
    if root == 0 || !p.wbs.iter().any(|w| w.id == root) {
        return p.wbs.iter().map(|w| w.id).collect();
    }
    let mut out = vec![root];
    let mut k = 0;
    while k < out.len() {
        let parent = out[k];
        let kids: Vec<i64> = p
            .wbs
            .iter()
            .filter(|w| w.parent == Some(parent) && !out.contains(&w.id))
            .map(|w| w.id)
            .collect();
        out.extend(kids);
        k += 1;
    }
    out
}

/// An id column value: blank for none.
fn id_text(id: i64) -> String {
    if id == 0 {
        String::new()
    } else {
        id.to_string()
    }
}

fn window_bound(origin: i64, text: &str) -> Option<i64> {
    crate::parse_relative(origin, text).ok().flatten()
}

/// Evaluate every enabled threshold of the open project and raise or update
/// issues. `baseline` gives each activity's project baseline dates and
/// `figures` its cost and earned value figures (both by model index);
/// `window` replaces each threshold's own time window.
pub fn monitor(
    p: &mut ProjectData,
    baseline: &[Option<BlDates>],
    figures: &[Figures],
    window: Option<(Option<i64>, Option<i64>)>,
    today: &str,
    wbs_path: &dyn Fn(&ProjectData, i64) -> String,
) -> MonitorResult {
    let pid = p.model.project.proj_id;
    let origin = p.model.origin_serial;
    let parms = parameters(&p.document);
    let list = thresholds(&p.document, pid);
    let mut result = MonitorResult::default();
    let mut raised: Vec<(Threshold, Parameter, Measure)> = vec![];
    for t in list.into_iter().filter(|t| t.enabled) {
        result.thresholds += 1;
        let Some(parm) = parms.iter().find(|q| q.id == t.parameter).cloned() else {
            result.skipped += 1;
            continue;
        };
        if !EVALUATED.contains(&parm.field.as_str()) || (t.wbs_level && !parm.wbs) {
            result.skipped += 1;
            continue;
        }
        let (from, to) = window.unwrap_or((
            window_bound(origin, &t.window_start),
            window_bound(origin, &t.window_end),
        ));
        let scope = branch(p, t.wbs_id);
        let in_window = |start: Option<i64>, finish: Option<i64>| {
            let (s, f) = (start.or(finish), finish.or(start));
            from.is_none_or(|lo| f.is_none_or(|f| f >= lo))
                && to.is_none_or(|hi| s.is_none_or(|s| s <= hi))
        };
        let measures = measure(
            p,
            baseline,
            figures,
            wbs_path,
            &parm.field,
            t.wbs_level,
            &scope,
            &in_window,
        );
        for m in measures {
            let low = t.low.is_some_and(|lo| m.value < lo);
            let high = t.high.is_some_and(|hi| m.value > hi);
            if low || high {
                raised.push((t.clone(), parm.clone(), m));
            }
        }
    }
    for (t, parm, m) in raised {
        let doc = &mut p.document;
        let existing = doc.table("PROJISSU").and_then(|table| {
            table.rows.iter().position(|r| {
                num(table.value(r, "proj_id")) == pid
                    && num(table.value(r, "thresh_id")) == t.id
                    && num(table.value(r, "task_id")) == m.task_id
                    && num(table.value(r, "wbs_id")) == m.wbs_id
                    && table.value(r, "status_code") != "IS_Closed"
            })
        });
        let value = records::format_number(m.value);
        let low = t.low.map_or(String::new(), records::format_number);
        let high = t.high.map_or(String::new(), records::format_number);
        match existing {
            Some(r) => {
                let before = records::get(doc, "PROJISSU", r, "issue_value");
                records::set(doc, "PROJISSU", r, "issue_value", value);
                records::set(doc, "PROJISSU", r, "lo_parm_value", low);
                records::set(doc, "PROJISSU", r, "hi_parm_value", high);
                if before != records::get(doc, "PROJISSU", r, "issue_value") {
                    result.updated += 1;
                }
            }
            None => {
                let id = records::next_id(doc, "PROJISSU", "issue_id");
                let short = parm.name.split(" (").next().unwrap_or(&parm.name);
                let name = format!("{short} {value} for {}", m.label);
                records::add_row(
                    doc,
                    "PROJISSU",
                    &[
                        ("issue_id", id.to_string()),
                        ("obs_id", t.obs_id.clone()),
                        ("proj_id", pid.to_string()),
                        ("task_id", id_text(m.task_id)),
                        ("thresh_id", t.id.to_string()),
                        ("thresh_parm_id", t.parameter.to_string()),
                        ("wbs_id", id_text(m.wbs_id)),
                        ("add_by_name", "Monitor Thresholds".into()),
                        ("issue_name", records::clean(&name)),
                        ("add_date", today.to_string()),
                        (
                            "priority_type",
                            if t.priority.is_empty() {
                                "PT_Normal".into()
                            } else {
                                t.priority.clone()
                            },
                        ),
                        ("status_code", "IS_Open".into()),
                        ("hi_parm_value", high),
                        ("issue_value", value),
                        ("lo_parm_value", low),
                    ],
                );
                result.created += 1;
            }
        }
    }
    result
}

/// The parameter's value for each activity (or WBS element) in scope.
#[allow(clippy::too_many_arguments)]
fn measure(
    p: &ProjectData,
    baseline: &[Option<BlDates>],
    figures: &[Figures],
    wbs_path: &dyn Fn(&ProjectData, i64) -> String,
    field: &str,
    wbs_level: bool,
    scope: &[i64],
    in_window: &dyn Fn(Option<i64>, Option<i64>) -> bool,
) -> Vec<Measure> {
    let start_of = |a: &Activity| a.act_start.or(a.early_start);
    let finish_of = |a: &Activity| a.act_finish.or(a.early_finish);
    let float_field = matches!(field, "total_float_hr_cnt" | "free_float_hr_cnt");
    let date_field = float_field
        || matches!(
            field,
            "var_start_date_drtn_hr_cnt" | "var_end_date_drtn_hr_cnt"
        );
    let members: Vec<(usize, i64)> = p
        .model
        .activities
        .iter()
        .enumerate()
        .filter_map(|(i, a)| {
            let wbs = p.details.get(&a.task_id).map_or(0, |d| d.wbs);
            (scope.contains(&wbs) && in_window(start_of(a), finish_of(a))).then_some((i, wbs))
        })
        .filter(|(i, _)| !(float_field && p.model.activities[*i].is_completed()))
        .collect();
    if !date_field {
        // Cost and earned value parameters.
        let fig = |i: usize| figures.get(i).copied().unwrap_or_default();
        if !wbs_level {
            return members
                .into_iter()
                .filter_map(|(i, _)| {
                    let a = &p.model.activities[i];
                    Some(Measure {
                        task_id: a.task_id,
                        wbs_id: 0,
                        label: a.task_code.clone(),
                        value: fig(i).value(field)?,
                    })
                })
                .collect();
        }
        let mut out = vec![];
        for w in p.wbs.iter().filter(|w| scope.contains(&w.id)) {
            let below = branch(p, w.id);
            let mut sum = Figures::default();
            let mut any = false;
            for (i, _) in members.iter().filter(|(_, wbs)| below.contains(wbs)) {
                sum.add(&fig(*i));
                any = true;
            }
            if let Some(value) = sum.value(field).filter(|_| any) {
                out.push(Measure {
                    task_id: 0,
                    wbs_id: w.id,
                    label: wbs_label(p, w, wbs_path),
                    value,
                });
            }
        }
        return out;
    }
    if !wbs_level {
        return members
            .into_iter()
            .filter_map(|(i, _)| {
                let a = &p.model.activities[i];
                let day = ev::day_minutes(p, a.cal);
                let minutes = match field {
                    "total_float_hr_cnt" => a.total_float?,
                    "free_float_hr_cnt" => a.free_float?,
                    "var_start_date_drtn_hr_cnt" => {
                        let bl = baseline.get(i).copied().flatten()?.start?;
                        signed_work(p, a.cal, start_of(a)?, bl)
                    }
                    _ => {
                        let bl = baseline.get(i).copied().flatten()?.finish?;
                        signed_work(p, a.cal, finish_of(a)?, bl)
                    }
                };
                Some(Measure {
                    task_id: a.task_id,
                    wbs_id: 0,
                    label: a.task_code.clone(),
                    value: days(minutes, day),
                })
            })
            .collect();
    }
    // WBS level: each element rolls up the activities below it.
    let cal = p.model.project_calendar;
    let day = ev::day_minutes(p, p.model.project_calendar);
    let mut out = vec![];
    for w in p.wbs.iter().filter(|w| scope.contains(&w.id)) {
        let below = branch(p, w.id);
        let acts: Vec<usize> = members
            .iter()
            .filter(|(_, wbs)| below.contains(wbs))
            .map(|(i, _)| *i)
            .collect();
        if acts.is_empty() {
            continue;
        }
        let all = || acts.iter().map(|&i| &p.model.activities[i]);
        let minutes = match field {
            "total_float_hr_cnt" => all().filter_map(|a| a.total_float).min(),
            "var_start_date_drtn_hr_cnt" => {
                let start = all().filter_map(start_of).min();
                let bl = acts
                    .iter()
                    .filter_map(|&i| baseline.get(i).copied().flatten()?.start)
                    .min();
                start.zip(bl).map(|(s, b)| signed_work(p, cal, s, b))
            }
            _ => {
                let finish = all().filter_map(finish_of).max();
                let bl = acts
                    .iter()
                    .filter_map(|&i| baseline.get(i).copied().flatten()?.finish)
                    .max();
                finish.zip(bl).map(|(f, b)| signed_work(p, cal, f, b))
            }
        };
        if let Some(minutes) = minutes {
            out.push(Measure {
                task_id: 0,
                wbs_id: w.id,
                label: wbs_label(p, w, wbs_path),
                value: days(minutes, day),
            });
        }
    }
    out
}

/// A WBS element's issue label: its code path, else its name.
fn wbs_label(
    p: &ProjectData,
    w: &crate::Wbs,
    wbs_path: &dyn Fn(&ProjectData, i64) -> String,
) -> String {
    let code = wbs_path(p, w.id);
    if code.is_empty() {
        w.name.clone()
    } else {
        code
    }
}

/// The monitor's inputs from an earned value pass: each activity's project
/// baseline dates and its figures (by model index).
pub fn inputs(p: &ProjectData, calc: &ev::Calculator) -> (Vec<Option<BlDates>>, Vec<Figures>) {
    let baseline = p
        .model
        .activities
        .iter()
        .map(|a| calc.project_baseline(a))
        .collect();
    let figures = p
        .model
        .activities
        .iter()
        .enumerate()
        .map(|(i, a)| Figures {
            parts: calc.parts(i, ev::Basis::Cost),
            original: ev::original(p, a) as f64,
            actual: ev::elapsed(p, a) as f64,
        })
        .collect();
    (baseline, figures)
}
