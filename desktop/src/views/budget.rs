//! The Budget Log and Spending Plan tabs of the Projects and WBS views, and
//! the WBS Milestones tab: rows of the `BUDGCHNG`, `WBSBUDG` and `WBSSTEP`
//! tables, read, added, deleted and edited in place. Each change is one
//! undo step through the shared record snapshots.
//!
//! The column names of `BUDGCHNG` and `WBSBUDG` are not confirmed against a
//! file that carries them (no corpus file does), so a column is found by a
//! list of plausible names and a row only ever sets columns the table has.
//! A table that does not exist yet is made with the columns of
//! `records::standard_fields`. The meanings of the totals (current and
//! proposed budget, tallies and variances) are OpenXER's own.
use super::*;
use crate::records::{self, num};
use crate::schedule_dialogs::Canvas;
use chrono::{Datelike, NaiveDate};
use openxer_xer::{Document, Table};

pub const LOG: &str = "BUDGCHNG";
pub const PLAN: &str = "WBSBUDG";
pub const STEPS: &str = "WBSSTEP";

/// Statuses of a budget change, in the order the log offers them.
pub const STATUSES: [&str; 3] = ["Pending", "Approved", "Rejected"];

const DATE: &[&str] = &["chng_date", "change_date", "budg_chng_date", "date"];
const AMOUNT: &[&str] = &[
    "chng_cost",
    "chng_amt",
    "change_amount",
    "budg_chng_cost",
    "amount",
];
const RESPONSIBLE: &[&str] = &["chng_by_name", "resp_name", "responsible", "resp_by"];
const STATUS: &[&str] = &["status_code", "chng_status", "status"];
const REASON: &[&str] = &["chng_descr", "chng_reason", "reason", "descr"];
const NUMBER: &[&str] = &["chng_num", "change_num", "change_number", "chng_no"];
const ID_LOG: &[&str] = &["budg_chng_id", "budget_change_id"];
const PERIOD_START: &[&str] = &["start_date", "period_start"];
const PERIOD_END: &[&str] = &["end_date", "period_end"];
const SPEND: &[&str] = &["spend_cost", "spending_plan", "spend_amt"];
const BENEFIT: &[&str] = &["benefit_cost", "benefit_plan", "benefit_amt"];
const ID_PLAN: &[&str] = &["wbs_budg_id"];

/// The first of `names` the table has, as the table spells it.
fn pick(t: &Table, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|n| t.field(n).map(|i| t.fields[i].clone()))
}

fn money(text: &str) -> f64 {
    records::float(text)
}

/// Text a user typed as an amount, in the user's currency format (decimal
/// symbol, digit grouping, negative pattern; the symbol may be left out).
pub fn parse_amount(text: &str) -> Option<f64> {
    format::parse_cost(text).filter(|v| v.abs() < 1e13)
}

/// Shown amounts: the user's currency format.
pub fn amount_text(v: f64) -> String {
    format::cost(v)
}

// ----------------------------------------------------------- budget log

/// One row of the budget change log.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    /// Row of the table.
    pub row: usize,
    /// Absolute minutes.
    pub date: Option<i64>,
    pub amount: f64,
    pub responsible: String,
    pub status: String,
    pub reason: String,
    pub number: String,
}

fn in_scope(t: &Table, r: &[String], proj: i64, wbs: i64) -> bool {
    num(t.value(r, "wbs_id")) == wbs
        && (t.field("proj_id").is_none() || num(t.value(r, "proj_id")) == proj)
}

pub fn changes(doc: &Document, proj: i64, wbs: i64) -> Vec<Change> {
    let Some(t) = doc.table(LOG) else {
        return vec![];
    };
    let get = |r: &[String], names: &[&str]| {
        pick(t, names).map_or(String::new(), |c| t.value(r, &c).to_string())
    };
    let mut out: Vec<Change> = t
        .rows
        .iter()
        .enumerate()
        .filter(|(_, r)| in_scope(t, r, proj, wbs))
        .take(MAX_ROWS)
        .map(|(row, r)| Change {
            row,
            date: openxer_xer::parse_absolute(&get(r, DATE)),
            amount: money(&get(r, AMOUNT)),
            responsible: get(r, RESPONSIBLE),
            status: get(r, STATUS),
            reason: get(r, REASON),
            number: get(r, NUMBER),
        })
        .collect();
    out.sort_by_key(|c| (c.date, c.row));
    out
}

/// Original budget, current budget (the original with the approved
/// changes) and proposed budget (the current one with the pending changes).
pub fn budget_totals(original: f64, log: &[Change]) -> (f64, f64, f64) {
    let sum = |status: &str| -> f64 {
        log.iter()
            .filter(|c| c.status.eq_ignore_ascii_case(status))
            .map(|c| c.amount)
            .sum()
    };
    let current = original + sum("Approved");
    (original, current, current + sum("Pending"))
}

/// Adds a change dated `date` (absolute minutes) as a pending one of zero.
/// Returns its table row.
pub fn add_change(doc: &mut Document, proj: i64, wbs: i64, date: i64) -> usize {
    let id = records::next_id(doc, LOG, "budg_chng_id");
    let row = records::add_row(doc, LOG, &[]);
    let t = doc.table_mut(LOG);
    let set = |t: &mut Table, names: &[&str], value: String| {
        if let Some(c) = pick(t, names) {
            t.set(row, &c, value);
        }
    };
    set(t, ID_LOG, id.to_string());
    set(t, &["proj_id"], proj.to_string());
    set(t, &["wbs_id"], wbs.to_string());
    set(t, DATE, openxer_xer::date_string(0, Some(date)));
    set(t, AMOUNT, "0".into());
    set(t, STATUS, "Pending".into());
    row
}

/// A field of the change log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogField {
    Date,
    Amount,
    Responsible,
    Status,
    Reason,
    Number,
}

impl LogField {
    pub const ALL: [LogField; 6] = [
        LogField::Date,
        LogField::Amount,
        LogField::Responsible,
        LogField::Status,
        LogField::Reason,
        LogField::Number,
    ];
}

/// Sets one field of a change from the text typed in its cell.
pub fn set_change(
    doc: &mut Document,
    row: usize,
    field: LogField,
    text: &str,
) -> Result<(), String> {
    let names: &[&str] = match field {
        LogField::Date => DATE,
        LogField::Amount => AMOUNT,
        LogField::Responsible => RESPONSIBLE,
        LogField::Status => STATUS,
        LogField::Reason => REASON,
        LogField::Number => NUMBER,
    };
    let t = doc.table_mut(LOG);
    let column = pick(t, names).ok_or("This file's budget log has no such column.")?;
    if row >= t.rows.len() {
        return Err("That budget change is gone.".into());
    }
    let value = match field {
        LogField::Date => match format::parse_date(0, text, None)? {
            Some(minutes) => openxer_xer::date_string(0, Some(minutes.div_euclid(1440) * 1440)),
            None => return Err("Enter the date of the change.".into()),
        },
        LogField::Amount => {
            records::format_number(parse_amount(text).ok_or("Enter an amount such as 25,000.00.")?)
        }
        LogField::Status => STATUSES
            .iter()
            .find(|s| s.eq_ignore_ascii_case(text.trim()))
            .ok_or("The status is Pending, Approved or Rejected.")?
            .to_string(),
        LogField::Responsible | LogField::Reason | LogField::Number => {
            let clean = records::clean(text.trim());
            if clean.chars().count() > 255 {
                return Err("The text is longer than 255 characters.".into());
            }
            clean
        }
    };
    t.set(row, &column, value);
    Ok(())
}

pub fn delete_change(doc: &mut Document, row: usize) {
    records::remove_row(doc, LOG, row);
}

// -------------------------------------------------------- spending plan

/// One month of the spending plan.
#[derive(Clone, Debug, PartialEq)]
pub struct Month {
    pub first: NaiveDate,
    pub last: NaiveDate,
    pub spend: f64,
    pub benefit: f64,
    /// Row of the table, when the month has one.
    pub row: Option<usize>,
    /// Further rows that start in the same month (their amounts are in the
    /// month's totals).
    pub more: Vec<usize>,
}

fn month_end(first: NaiveDate) -> NaiveDate {
    let next = if first.month() == 12 {
        NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)
    };
    next.and_then(|d| d.pred_opt()).unwrap_or(first)
}

fn month_start(day: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(day.year(), day.month(), 1).unwrap_or(day)
}

fn next_month(first: NaiveDate) -> NaiveDate {
    month_end(first).succ_opt().unwrap_or(first)
}

/// The calendar day of absolute minutes (from 1899-12-30), `None` when it
/// is out of the calendar's range.
pub fn safe_day(minutes: i64) -> Option<NaiveDate> {
    let days = chrono::Duration::try_days(minutes.div_euclid(1440))?;
    NaiveDate::from_ymd_opt(1899, 12, 30)?.checked_add_signed(days)
}

fn day_of(text: &str) -> Option<NaiveDate> {
    openxer_xer::parse_absolute(text).and_then(safe_day)
}

/// Months shown at most (a hundred years). Dates in a file decide nothing
/// about how much is built beyond this.
pub const MAX_MONTHS: usize = 1200;
/// The calendar years a plan month may lie in; rows outside are ignored.
const YEARS: std::ops::RangeInclusive<i32> = 1900..=2200;
/// Rows a table is read for.
pub const MAX_ROWS: usize = 5000;

fn sane(day: NaiveDate) -> bool {
    YEARS.contains(&day.year())
}

fn blank(first: NaiveDate) -> Month {
    Month {
        first,
        last: month_end(first),
        spend: 0.,
        benefit: 0.,
        row: None,
        more: vec![],
    }
}

/// The months to show: every month from the first of `from` to the last of
/// `to` (held to [`MAX_MONTHS`] and to the years 1900 to 2200) and every
/// month the table has a row for, at least `least` of them. A row that
/// covers several months is shown at its first month; rows outside the
/// years are left out.
pub fn months(
    doc: &Document,
    proj: i64,
    wbs: i64,
    from: NaiveDate,
    to: NaiveDate,
    least: usize,
) -> Vec<Month> {
    months_checked(doc, proj, wbs, from, to, least).0
}

/// The spending plan's total spending and total benefit of WBS node `wbs`
/// (the Budget Summary tab's and the WBS table's figures).
pub fn plan_totals(doc: &Document, proj: i64, wbs: i64) -> (f64, f64) {
    let far = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap_or_default();
    months(doc, proj, wbs, far, far, 0)
        .iter()
        .filter(|m| m.row.is_some())
        .fold((0., 0.), |(s, b), m| (s + m.spend, b + m.benefit))
}

/// The budget figures of WBS node `wbs` whose original budget is
/// `original`: original, current and proposed (`budget_totals`).
pub fn node_budget(doc: &Document, proj: i64, wbs: i64, original: f64) -> (f64, f64, f64) {
    budget_totals(original, &changes(doc, proj, wbs))
}

/// An amount as a PROJWBS or PROJECT column stores it.
pub fn stored_amount(text: &str) -> f64 {
    money(text)
}

/// [`months`] and how many rows were left out as outside the years.
pub fn months_checked(
    doc: &Document,
    proj: i64,
    wbs: i64,
    from: NaiveDate,
    to: NaiveDate,
    least: usize,
) -> (Vec<Month>, usize) {
    let mut by_month: std::collections::BTreeMap<NaiveDate, Month> = Default::default();
    let mut skipped = 0;
    if let Some(t) = doc.table(PLAN) {
        let (s, e) = (pick(t, PERIOD_START), pick(t, PERIOD_END));
        let (sp, be) = (pick(t, SPEND), pick(t, BENEFIT));
        for (row, r) in t
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| in_scope(t, r, proj, wbs))
            .take(MAX_ROWS)
        {
            let Some(start) = s.as_ref().and_then(|c| day_of(t.value(r, c))) else {
                continue;
            };
            if !sane(start) {
                skipped += 1;
                continue;
            }
            let first = month_start(start);
            let last = e
                .as_ref()
                .and_then(|c| day_of(t.value(r, c)))
                .filter(|d| *d >= first && sane(*d))
                .unwrap_or_else(|| month_end(first));
            let spend = sp.as_ref().map_or(0., |c| money(t.value(r, c)));
            let benefit = be.as_ref().map_or(0., |c| money(t.value(r, c)));
            match by_month.entry(first) {
                std::collections::btree_map::Entry::Occupied(mut e) => {
                    // A second row in the month adds to it.
                    let m = e.get_mut();
                    m.spend += spend;
                    m.benefit += benefit;
                    m.last = m.last.max(last);
                    m.more.push(row);
                }
                std::collections::btree_map::Entry::Vacant(e) => {
                    e.insert(Month {
                        first,
                        last,
                        spend,
                        benefit,
                        row: Some(row),
                        more: vec![],
                    });
                }
            }
        }
    }
    // The window, clamped to the years and to a hundred years.
    let low = NaiveDate::from_ymd_opt(*YEARS.start(), 1, 1).unwrap_or(from);
    let high = NaiveDate::from_ymd_opt(*YEARS.end(), 12, 1).unwrap_or(to);
    let clamp = |d: NaiveDate| month_start(d).clamp(low, high);
    let (a, b) = (clamp(from.min(to)), clamp(to.max(from)));
    let mut m = a;
    for _ in 0..MAX_MONTHS {
        if m > b {
            break;
        }
        by_month.entry(m).or_insert_with(|| blank(m));
        let next = next_month(m);
        if next <= m {
            break;
        }
        m = next;
    }
    // Pad to the least number of months.
    for _ in 0..least.min(MAX_MONTHS) {
        if by_month.len() >= least {
            break;
        }
        let Some(last) = by_month.keys().next_back().copied() else {
            break;
        };
        let m = next_month(last);
        if m <= last || !sane(m) {
            break;
        }
        by_month.insert(m, blank(m));
    }
    (by_month.into_values().collect(), skipped)
}

/// The running figures of a plan: per month the spending plan tally, the
/// benefit plan tally, the undistributed variance (the current budget less
/// the spending tally) and the benefit variance (benefit tally less
/// spending tally), and the totals.
#[derive(Clone, Debug, PartialEq)]
pub struct Tallies {
    pub spend_tally: Vec<f64>,
    pub benefit_tally: Vec<f64>,
    pub undistributed: Vec<f64>,
    pub benefit_variance: Vec<f64>,
    pub total_spend: f64,
    pub total_benefit: f64,
}

pub fn tallies(plan: &[Month], current_budget: f64) -> Tallies {
    let (mut s, mut b) = (0., 0.);
    let mut out = Tallies {
        spend_tally: vec![],
        benefit_tally: vec![],
        undistributed: vec![],
        benefit_variance: vec![],
        total_spend: 0.,
        total_benefit: 0.,
    };
    for m in plan {
        s += m.spend;
        b += m.benefit;
        out.spend_tally.push(s);
        out.benefit_tally.push(b);
        out.undistributed.push(current_budget - s);
        out.benefit_variance.push(b - s);
    }
    out.total_spend = s;
    out.total_benefit = b;
    out
}

/// Which amount of a month is edited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanField {
    Spend,
    Benefit,
}

/// Sets the spending or benefit plan of the month starting on `first`. A
/// month whose two amounts are both zero loses its row.
pub fn set_month(
    doc: &mut Document,
    proj: i64,
    wbs: i64,
    first: NaiveDate,
    field: PlanField,
    value: f64,
) -> Result<(), String> {
    if !value.is_finite() || value.abs() >= 1e13 {
        return Err("The amount is out of range.".into());
    }
    if !sane(first) {
        return Err(format!(
            "The spending plan covers the years {} to {}.",
            YEARS.start(),
            YEARS.end()
        ));
    }
    let month = months(doc, proj, wbs, first, first, 0)
        .into_iter()
        .find(|m| m.first == first);
    let existing = month.as_ref().and_then(|m| m.row);
    let more = month.map_or(vec![], |m| m.more);
    let row = match existing {
        Some(r) => r,
        None => {
            if value == 0. {
                return Ok(());
            }
            let id = records::next_id(doc, PLAN, "wbs_budg_id");
            let row = records::add_row(doc, PLAN, &[]);
            let t = doc.table_mut(PLAN);
            let set = |t: &mut Table, names: &[&str], v: String| {
                if let Some(c) = pick(t, names) {
                    t.set(row, &c, v);
                }
            };
            set(t, ID_PLAN, id.to_string());
            set(t, &["proj_id"], proj.to_string());
            set(t, &["wbs_id"], wbs.to_string());
            set(
                t,
                PERIOD_START,
                format!("{} 00:00", first.format("%Y-%m-%d")),
            );
            set(
                t,
                PERIOD_END,
                format!("{} 23:59", month_end(first).format("%Y-%m-%d")),
            );
            set(t, SPEND, "0".into());
            set(t, BENEFIT, "0".into());
            row
        }
    };
    let t = doc.table_mut(PLAN);
    let names = if field == PlanField::Spend {
        SPEND
    } else {
        BENEFIT
    };
    let column = pick(t, names).ok_or("This file's spending plan has no such column.")?;
    t.set(row, &column, records::format_number(value));
    // The month's other rows give up this amount: the typed value is the
    // month's total.
    for &r in &more {
        t.set(r, &column, "0".into());
    }
    let zero = |t: &Table, r: usize, names: &[&str]| {
        pick(t, names).is_none_or(|c| money(t.value(&t.rows[r], &c)) == 0.)
    };
    let mut empty: Vec<usize> = std::iter::once(row)
        .chain(more)
        .filter(|&r| zero(t, r, SPEND) && zero(t, r, BENEFIT))
        .collect();
    empty.sort_unstable();
    for r in empty.into_iter().rev() {
        records::remove_row(doc, PLAN, r);
    }
    Ok(())
}

// ------------------------------------------------------- WBS milestones

/// One WBS milestone (a row of `WBSSTEP`).
#[derive(Clone, Debug, PartialEq)]
pub struct Milestone {
    pub row: usize,
    pub seq: i64,
    pub name: String,
    pub weight: f64,
    pub done: bool,
}

pub fn milestones(doc: &Document, proj: i64, wbs: i64) -> Vec<Milestone> {
    let Some(t) = doc.table(STEPS) else {
        return vec![];
    };
    let mut out: Vec<Milestone> = t
        .rows
        .iter()
        .enumerate()
        .filter(|(_, r)| in_scope(t, r, proj, wbs))
        .take(MAX_ROWS)
        .map(|(row, r)| Milestone {
            row,
            seq: num(t.value(r, "seq_num")),
            name: t.value(r, "step_name").to_string(),
            weight: records::float(t.value(r, "step_wt")),
            done: t.value(r, "complete_flag") == "Y",
        })
        .collect();
    out.sort_by_key(|m| (m.seq, m.row));
    out
}

/// Percent complete of a WBS element's milestones: the completed weight
/// over the total weight, or the completed share when no weights are set.
pub fn milestone_percent(list: &[Milestone]) -> f64 {
    let total: f64 = list.iter().map(|m| m.weight).sum();
    if total > 0. {
        list.iter()
            .filter(|m| m.done)
            .map(|m| m.weight)
            .sum::<f64>()
            / total
            * 100.
    } else if list.is_empty() {
        0.
    } else {
        list.iter().filter(|m| m.done).count() as f64 / list.len() as f64 * 100.
    }
}

/// Adds a milestone after the last one. Returns its table row.
pub fn add_milestone(doc: &mut Document, proj: i64, wbs: i64) -> usize {
    let seq = milestones(doc, proj, wbs)
        .last()
        .map_or(10, |m| m.seq.max(0) + 10);
    let id = records::next_id(doc, STEPS, "wbs_step_id");
    let row = records::add_row(doc, STEPS, &[]);
    let t = doc.table_mut(STEPS);
    for (name, value) in [
        ("wbs_step_id", id.to_string()),
        ("proj_id", proj.to_string()),
        ("wbs_id", wbs.to_string()),
        ("seq_num", seq.to_string()),
        ("step_name", "New WBS Milestone".to_string()),
        ("step_wt", "1".to_string()),
        ("complete_flag", "N".to_string()),
    ] {
        if t.field(name).is_some() {
            t.set(row, name, value);
        }
    }
    row
}

/// A field of a milestone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepField {
    Name,
    Weight,
    Done,
}

pub fn set_milestone(
    doc: &mut Document,
    row: usize,
    field: StepField,
    text: &str,
) -> Result<(), String> {
    let t = doc.table_mut(STEPS);
    if row >= t.rows.len() {
        return Err("That milestone is gone.".into());
    }
    let (column, value) = match field {
        StepField::Name => {
            let name = records::clean(text.trim());
            if name.is_empty() {
                return Err("Enter a name for the milestone.".into());
            }
            if name.chars().count() > 120 {
                return Err("The name is longer than 120 characters.".into());
            }
            ("step_name", name)
        }
        StepField::Weight => {
            let v = parse_amount(text).filter(|v| *v >= 0.);
            (
                "step_wt",
                records::format_number(v.ok_or("Enter a weight of zero or more.")?),
            )
        }
        StepField::Done => {
            let yes = match text.trim().to_ascii_lowercase().as_str() {
                "yes" | "y" | "true" | "1" => true,
                "no" | "n" | "false" | "0" => false,
                _ => return Err("Completed is Yes or No.".into()),
            };
            ("complete_flag", if yes { "Y" } else { "N" }.to_string())
        }
    };
    if t.field(column).is_none() {
        return Err("This file's milestones have no such column.".into());
    }
    t.set(row, column, value);
    Ok(())
}

pub fn delete_milestone(doc: &mut Document, row: usize) {
    records::remove_row(doc, STEPS, row);
}

/// Swaps the order of the milestone at `index` of the element's list with
/// its neighbour above (`up`) or below. Returns the new index.
pub fn move_milestone(
    doc: &mut Document,
    proj: i64,
    wbs: i64,
    index: usize,
    up: bool,
) -> Option<usize> {
    let list = milestones(doc, proj, wbs);
    let other = if up {
        index.checked_sub(1)?
    } else {
        (index + 1 < list.len()).then_some(index + 1)?
    };
    // Renumber the whole list so the order is explicit and unique.
    let mut order: Vec<usize> = list.iter().map(|m| m.row).collect();
    order.swap(index, other);
    let t = doc.table_mut(STEPS);
    t.field("seq_num")?;
    for (n, row) in order.iter().enumerate() {
        t.set(*row, "seq_num", ((n + 1) * 10).to_string());
    }
    Some(other)
}

// ------------------------------------------------------------------ state

#[derive(Default)]
pub struct BudgetState {
    pub log_row: Option<usize>,
    pub plan_row: Option<usize>,
    /// First month shown of the spending plan (index into the months).
    pub plan_first: usize,
    pub step_row: Option<usize>,
}

/// The element budget tabs describe: the project and its WBS node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scope {
    pub proj: i64,
    pub wbs: i64,
}

fn sheet_cols_log() -> [(&'static str, f32, bool); 6] {
    [
        ("Date", 100., false),
        ("Amount", 100., true),
        ("Responsible", 100., false),
        ("Status", 100., false),
        ("Reason", 200., false),
        ("Change Number", 100., false),
    ]
}

impl App {
    fn budget_changed(&mut self) {
        self.touch_records();
    }

    /// The Budget Log tab. `scope` is `None` where the data is not the open
    /// project's (another project, an EPS node): the tab shows no rows.
    pub(crate) fn budget_log_tab(
        &mut self,
        c: &mut Canvas,
        key: &str,
        w: f32,
        h: f32,
        scope: Option<Scope>,
        node: &BTreeMap<String, String>,
    ) {
        let original = money(node.get("orig_cost").map_or("", String::as_str));
        let log = scope.map_or(vec![], |s| changes(&self.project.document, s.proj, s.wbs));
        let (orig, current, proposed) = budget_totals(original, &log);
        for (x, caption, field_x, value) in [
            (11., "Original Budget:", 201., orig),
            (373., "Current Budget:", 545., current),
            (716., "Proposed Budget:", 899., proposed),
        ] {
            c.label(x, 12., 21., caption, true);
            sheet::value_box(
                c,
                &format!("{key}_budget_{field_x}"),
                field_x,
                12.,
                167.,
                &amount_text(value),
                true,
            );
        }
        c.fill(12., 42., w - 18., 22., CHROME);
        c.label(18., 42., 22., "Budget Change Log", true);
        let rows: Vec<Vec<String>> = log
            .iter()
            .map(|ch| {
                vec![
                    ch.date
                        .and_then(safe_day)
                        .map(format::day)
                        .unwrap_or_default(),
                    amount_text(ch.amount),
                    ch.responsible.clone(),
                    ch.status.clone(),
                    ch.reason.clone(),
                    ch.number.clone(),
                ]
            })
            .collect();
        let cols = sheet_cols_log();
        let selected = self
            .views
            .budget
            .log_row
            .filter(|r| *r < log.len())
            .or((!log.is_empty()).then_some(0))
            .filter(|_| scope.is_some());
        let (x, y, tw, th) = (12., 64., w - 18., h - 96.);
        if let Some(i) = sheet::table(
            c,
            &format!("{key}_budget_log"),
            x,
            y,
            tw,
            th,
            &cols,
            &rows,
            selected,
        ) {
            self.views.budget.log_row = Some(i);
        }
        if rows.is_empty() {
            let text = if scope.is_some() {
                "No budget changes are recorded."
            } else {
                "Open the project to see its budget changes."
            };
            c.label(x + 12., y + 28., 18., text, false);
        }
        let mut edit: Option<(usize, LogField, String)> = None;
        let log_key = format!("{key}_budget_log");
        let top = selected.and_then(|i| sheet::row_top(c, &log_key, y, th, i));
        if let (Some(i), Some(_), Some(top)) = (selected, scope, top) {
            let mut cx = x + 2.;
            for (column, (field, (_, width, right))) in LogField::ALL.iter().zip(cols).enumerate() {
                let align = if right { Align::Max } else { Align::Min };
                if let Some(text) = sheet::commit_cell(
                    c,
                    &format!("{key}_budget_cell_{column}"),
                    (cx, top, width, 18.),
                    &rows[i][column],
                    true,
                    align,
                ) {
                    edit = Some((log[i].row, *field, text));
                }
                cx += width;
            }
        }
        let clicked = sheet::buttons(
            c,
            &format!("{key}_budget_buttons"),
            11.,
            h - 26.,
            &[("Add", scope.is_some()), ("Delete", selected.is_some())],
        );
        if let Some((row, field, text)) = edit {
            match set_change(&mut self.project.document, row, field, &text) {
                Ok(()) => self.budget_changed(),
                Err(e) => self.status = e,
            }
        }
        match (clicked, scope) {
            (Some(0), Some(s)) => {
                let day = self.data_day_minutes();
                let row = add_change(&mut self.project.document, s.proj, s.wbs, day);
                let list = changes(&self.project.document, s.proj, s.wbs);
                self.views.budget.log_row = list.iter().position(|ch| ch.row == row);
                self.budget_changed();
            }
            (Some(1), Some(_)) => {
                if let Some(i) = selected {
                    delete_change(&mut self.project.document, log[i].row);
                    self.views.budget.log_row = Some(i.saturating_sub(1));
                    self.budget_changed();
                }
            }
            _ => {}
        }
    }

    /// The Budget Summary tab, from the log and the spending plan.
    pub(crate) fn budget_summary_tab(
        &mut self,
        c: &mut Canvas,
        w: f32,
        h: f32,
        scope: Option<Scope>,
        node: &BTreeMap<String, String>,
    ) {
        let original = money(node.get("orig_cost").map_or("", String::as_str));
        let doc = &self.project.document;
        let log = scope.map_or(vec![], |s| changes(doc, s.proj, s.wbs));
        let (_, current, _) = budget_totals(original, &log);
        let (spend, benefit) = scope.map_or((0., 0.), |s| plan_totals(doc, s.proj, s.wbs));
        projects::budget_summary(
            c,
            w,
            h,
            &projects::SummaryFigures {
                current,
                spend,
                benefit,
            },
        );
    }

    /// The data date's day as absolute minutes.
    fn data_day_minutes(&self) -> i64 {
        let dd = self.project.model.project.data_date.unwrap_or(0)
            + self.project.model.origin_serial * DAY;
        dd.div_euclid(DAY) * DAY
    }

    /// The Spending Plan tab: one row per month with a Total row on top.
    pub(crate) fn spending_plan_tab(
        &mut self,
        c: &mut Canvas,
        key: &str,
        w: f32,
        h: f32,
        scope: Option<Scope>,
        node: &BTreeMap<String, String>,
    ) {
        let cols = [
            ("Date", 110., false),
            ("Spending Plan", 111., true),
            ("Spending Plan Tally", 107., true),
            ("Undistributed Current Variance", 104., true),
            ("Benefit Plan", 102., true),
            ("Benefit Plan Tally", 103., true),
            ("Benefit Variance", 115., true),
        ];
        let (x, y, tw, th) = (6., 6., w - 12., h - 12.);
        // Rows that fit under the header and the Total row.
        let capacity = (((th - 4. - 19. - 18.) / 18.).floor() as usize).max(1);
        let origin = self.project.model.origin_serial;
        let start_day =
            |v: Option<i64>| v.and_then(|m| safe_day(m.checked_add(origin.checked_mul(DAY)?)?));
        let from = start_day(self.project.model.project.plan_start)
            .or(start_day(self.project.model.project.data_date))
            .unwrap_or(NaiveDate::from_ymd_opt(2000, 1, 1).unwrap());
        let to = self
            .project
            .model
            .activities
            .iter()
            .filter_map(|a| a.early_finish.or(a.target_finish))
            .max()
            .and_then(|m| start_day(Some(m)))
            .unwrap_or(from);
        let (plan, skipped) = scope.map_or((vec![], 0), |s| {
            months_checked(&self.project.document, s.proj, s.wbs, from, to, 24)
        });
        if skipped > 0 {
            self.status = format!(
                "{skipped} spending plan row(s) with dates outside {} to {} are not shown.",
                YEARS.start(),
                YEARS.end()
            );
        }
        let original = money(node.get("orig_cost").map_or("", String::as_str));
        let log = scope.map_or(vec![], |s| changes(&self.project.document, s.proj, s.wbs));
        let (_, current, _) = budget_totals(original, &log);
        let t = tallies(&plan, current);
        // Scrolling: the wheel over the grid moves the first month shown.
        let rect = c.r(x, y, tw, th);
        if c.ui.rect_contains_pointer(rect) {
            let wheel = c.ui.input(|i| i.raw_scroll_delta.y);
            if wheel != 0. {
                let step = if wheel > 0. { -3_i64 } else { 3 };
                let max = plan.len().saturating_sub(capacity) as i64;
                self.views.budget.plan_first =
                    (self.views.budget.plan_first as i64 + step).clamp(0, max) as usize;
            }
        }
        let first = self
            .views
            .budget
            .plan_first
            .min(plan.len().saturating_sub(capacity));
        let shown: Vec<usize> = (first..plan.len().min(first + capacity)).collect();
        let mut rows = vec![vec![
            "Total".to_string(),
            amount_text(t.total_spend),
            amount_text(t.total_spend),
            amount_text(current - t.total_spend),
            amount_text(t.total_benefit),
            amount_text(t.total_benefit),
            amount_text(t.total_benefit - t.total_spend),
        ]];
        for &i in &shown {
            let m = &plan[i];
            rows.push(vec![
                m.first.format("%y - %b").to_string(),
                amount_text(m.spend),
                amount_text(t.spend_tally[i]),
                amount_text(t.undistributed[i]),
                amount_text(m.benefit),
                amount_text(t.benefit_tally[i]),
                amount_text(t.benefit_variance[i]),
            ]);
        }
        let selected = self
            .views
            .budget
            .plan_row
            .filter(|r| shown.contains(r))
            .filter(|_| scope.is_some());
        let table_selected = selected.map(|r| r - first + 1);
        if let Some(clicked) = sheet::table(
            c,
            &format!("{key}_spending"),
            x,
            y,
            tw,
            th,
            &cols,
            &rows,
            table_selected,
        ) {
            self.views.budget.plan_row = (clicked > 0).then(|| first + clicked - 1);
        }
        if scope.is_none() {
            c.label(
                x + 12.,
                y + 70.,
                18.,
                "Open the project to see its spending plan.",
                false,
            );
        }
        let mut edit: Option<(NaiveDate, PlanField, String)> = None;
        if let (Some(r), Some(_)) = (selected, scope) {
            let top = y + 2. + 19. + 18. * (r - first + 1) as f32;
            let mut cx = x + 2.;
            for (column, (_, width, _)) in cols.iter().enumerate() {
                let field = match column {
                    1 => Some(PlanField::Spend),
                    4 => Some(PlanField::Benefit),
                    _ => None,
                };
                if let Some(field) = field {
                    if let Some(text) = sheet::commit_cell(
                        c,
                        &format!("{key}_spending_cell_{column}"),
                        (cx, top, *width, 18.),
                        &rows[r - first + 1][column],
                        true,
                        Align::Max,
                    ) {
                        edit = Some((plan[r].first, field, text));
                    }
                }
                cx += width;
            }
        }
        if let (Some((month, field, text)), Some(s)) = (edit, scope) {
            match parse_amount(&text) {
                Some(v) => {
                    match set_month(&mut self.project.document, s.proj, s.wbs, month, field, v) {
                        Ok(()) => self.budget_changed(),
                        Err(e) => self.status = e,
                    }
                }
                None => self.status = "Enter an amount such as 25,000.00.".into(),
            }
        }
    }
}
