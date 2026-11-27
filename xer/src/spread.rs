//! Time-phased spreading of assignment units and costs, with resource curves.
//!
//! Every usage view, the time-phased earned value and the CLI `usage`
//! command spread an assignment's quantities over periods through this
//! module, so they all agree. Spreading is display only: it never changes
//! stored units, dates, the schedule or leveling (leveling spreads evenly).
//!
//! Every assignment spreads over its own dates (its lag and own duration
//! inside the activity's dates, its own actual dates) on the calendar it
//! works on (a resource dependent activity's resource calendar); see
//! [`Shaping`]. Without a curve (or where a curve does not apply) a quantity
//! is spread evenly over that working time ([`even`]).
//!
//! The windows and the curve position walk the calendar, so they are worked
//! out once per assignment ([`Plan`]) and every spread of it reuses them.
//! When the periods cover a whole window, the floating-point remainder goes
//! into the window's last non-zero period, so the periods add up to the
//! quantity exactly.
//!
//! What the published documentation of resource curves establishes, and
//! what this module does:
//!
//! - A curve has 21 values at 0%, 5% ... 100% of the assignment's duration,
//!   relative weights normalised to a total of 100.
//! - A curve applies only on activities with duration type Fixed Duration
//!   and Units/Time or Fixed Duration and Units; elsewhere it is ignored.
//! - The curve spans the assignment's own start (with its lag) to its own
//!   finish.
//! - Budgeted and remaining units (and the costs with them) follow the
//!   curve; planned value follows the budgeted spread. Expenses are not
//!   affected.
//! - Actuals spread from the start to the data date along the curve;
//!   remaining starts at the data date.
//!
//! OpenXER's reading where the documentation is silent. UNVERIFIED, TO BE
//! CONFIRMED BY A PROBE:
//!
//! | Question | OpenXER's reading |
//! |---|---|
//! | What the values weigh | value k (1 to 20) is the weight of the k-th of 20 equal segments of the assignment's working duration, the one ending at k x 5% |
//! | The 0% value | ignored for budgeted and remaining spreads; for actuals it is spent at the actual start instant |
//! | Time basis | segment boundaries are positions in working time on the assignment's calendar; non-working time carries nothing; within a segment the units spread evenly over its working time |
//! | In progress | position = actual working time so far / (actual + remaining working time); remaining follows the rest of the curve from that position, rescaled to total the remaining units; actuals follow the curve up to it |
//! | Remaining late | the same shape and position over the late dates |
//! | Units/Time | not changed by a curve (it stays the average) |
//! | Rounding | none beyond what the display does; a window's last non-zero period takes the floating-point remainder when the periods cover the window |
//! | A curve whose remaining part has no weight | that part spreads evenly |
//!
//! Manual period values (`crate::manual_units`) take the curve's place: a
//! window with runs (the budget window with `target_crv`, the remaining and
//! remaining late windows with `remain_crv`, the actual window with
//! `actual_crv`) spreads its quantity by them, each run's units evenly over
//! its working hours counted from the window's start. Where the notes are
//! silent, OpenXER's reading: runs apply on every duration type; when the
//! runs' hours or units differ from the window's working time or the
//! quantity (the dates or the units changed since they were typed), the
//! runs keep their proportions, stretched to the window and scaled to the
//! quantity, as a curve is.
use crate::assignments::DurationType;
use crate::edit::curves::{self, POINTS};
use crate::manual_units::{self, Run};
use crate::Document;
use openxer_engine::calendar::{CalendarAdapter, WorkCalendar};
use openxer_engine::model::{has_recurring_work, Activity, Model, TaskType};
use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;

/// Segments of a curve: the values at 5% ... 100%.
pub const SEGMENTS: usize = POINTS - 1;

/// A curve as spreading uses it: the start value and the 20 segment
/// weights, as stored (any positive total).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub start: f64,
    pub segments: [f64; SEGMENTS],
}

impl Shape {
    /// The shape of 21 curve points; `None` when no segment has weight (the
    /// spread is then even).
    pub fn from_points(points: &[f64; POINTS]) -> Option<Shape> {
        let clean = |v: f64| if v.is_finite() && v > 0. { v } else { 0. };
        let segments: [f64; SEGMENTS] = std::array::from_fn(|k| clean(points[k + 1]));
        (segments.iter().sum::<f64>() > 0.).then_some(Shape {
            start: clean(points[0]),
            segments,
        })
    }

    /// Cumulative segment weight from 0 to curve position `x` (0 to 1),
    /// linear inside a segment. The start value is not included.
    pub fn cumulative(&self, x: f64) -> f64 {
        let x = x.clamp(0., 1.) * SEGMENTS as f64;
        let whole = (x.floor() as usize).min(SEGMENTS);
        let mut sum: f64 = self.segments[..whole].iter().sum();
        if whole < SEGMENTS {
            sum += self.segments[whole] * (x - whole as f64);
        }
        sum
    }
}

/// Whether a curve shapes the spread on an activity of this duration type
/// (`TASK.duration_type`; blank reads as Fixed Duration and Units).
pub fn curve_applies(duration_type: &str) -> bool {
    matches!(
        DurationType::from_code(duration_type),
        DurationType::FixedDurationAndUnitsPerTime | DurationType::FixedDurationAndUnits
    )
}

/// What spreading needs of an assignment beyond its activity, read from its
/// TASKRSRC row: its curve (when one applies), its own lags and durations
/// (working minutes), its calendar and its own actual dates.
///
/// The assignment's own dates follow from the activity's current dates by
/// the rule the schedule writes them into TASKRSRC (`target_start_date` to
/// `rem_late_end_date`, see `ProjectData::write_assignment_schedule`): an
/// assignment as long as the activity (within 1%) without lag keeps the
/// activity's dates; otherwise it starts its lag after the activity's start
/// (no lag once started) and lasts its own duration. Deriving them keeps the
/// views right between an edit and the next save.
///
/// They are not always the stored TASKRSRC dates after a schedule:
///
/// - the stored dates are counted on the activity's calendar; spreading
///   counts lag and own duration on the assignment's calendar (the
///   resource's, on a resource dependent activity);
/// - the stored remaining dates start from the activity's remaining dates
///   (`restart_date`/`reend_date`); spreading uses the early dates unless the
///   project levels;
/// - an assignment without units has no own duration here and spans the
///   activity (so its costs spread over it), where the stored dates are
///   zero length.
#[derive(Clone, Debug, PartialEq)]
pub struct Shaping {
    /// The curve, when the row has one with weight and the activity's
    /// duration type uses curves; `None` spreads evenly.
    pub shape: Option<Shape>,
    /// `target_lag_drtn_hr_cnt` and `relag_drtn_hr_cnt` (task and resource
    /// dependent activities only).
    pub budget_lag: i64,
    pub remaining_lag: i64,
    /// The assignment's own budgeted and remaining durations, from units over
    /// units/time; `None` without units or without a units/time (it spans
    /// the activity, so a cost without units spreads over the activity).
    pub budget_minutes: Option<i64>,
    pub remaining_minutes: Option<i64>,
    /// The calendar the assignment works on (model index): the resource's on
    /// a resource dependent activity (when that calendar has recurring work,
    /// as the engine requires), whatever its remaining units, so the spread
    /// does not change calendar when the assignment completes; `None` uses
    /// the activity's.
    pub cal: Option<usize>,
    /// The assignment's own actual dates where the row carries consistent
    /// ones (not after the data date, inside the activity's actual dates);
    /// `None` follows the activity.
    pub act_start: Option<i64>,
    pub act_finish: Option<i64>,
    /// The row's manual period values, when it has any.
    pub manual: Option<Manual>,
}

impl Shaping {
    /// An assignment that spans its activity evenly, on its calendar.
    pub fn plain() -> Shaping {
        Shaping {
            shape: None,
            budget_lag: 0,
            remaining_lag: 0,
            budget_minutes: None,
            remaining_minutes: None,
            cal: None,
            act_start: None,
            act_finish: None,
            manual: None,
        }
    }

    /// Whether a curve or manual values shape any of its spreads.
    pub fn shaped(&self) -> bool {
        self.shape.is_some() || self.manual.is_some()
    }
}

/// Manual period values of an assignment (see [`crate::manual_units`]):
/// the runs each window spreads by in place of a curve.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Manual {
    pub budget: Option<Vec<Run>>,
    pub remaining: Option<Vec<Run>>,
    pub actual: Option<Vec<Run>>,
}

impl Manual {
    /// The runs of TASKRSRC row `r` of `t`; `None` without any.
    pub fn of_row(t: &crate::Table, r: &[String]) -> Option<Manual> {
        let runs = |k: &str| manual_units::parse(t.value(r, k));
        let m = Manual {
            budget: runs(manual_units::BUDGET),
            remaining: runs(manual_units::REMAINING),
            actual: runs(manual_units::ACTUAL),
        };
        (m != Manual::default()).then_some(m)
    }
}

/// Looks up the shaping of TASKRSRC rows, reading curves and duration types
/// only when a row has a curve.
pub struct Lookup<'a> {
    doc: &'a Document,
    model: &'a Model,
    pid: i64,
    kinds: OnceCell<HashMap<i64, String>>,
    shapes: RefCell<HashMap<i64, Option<Shape>>>,
    /// Each resource's calendar (model index), where it has recurring work.
    calendars: OnceCell<HashMap<i64, usize>>,
    rsrc_cols: OnceCell<crate::FieldIndex>,
}

fn num(s: &str) -> i64 {
    s.trim().parse().unwrap_or(0)
}

fn float(s: &str) -> f64 {
    s.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .unwrap_or(0.)
}

fn minutes(hours: &str) -> i64 {
    (float(hours) * 60.).round_ties_even() as i64
}

impl<'a> Lookup<'a> {
    pub fn new(p: &'a crate::ProjectData) -> Self {
        Self {
            doc: &p.document,
            model: &p.model,
            pid: p.model.project.proj_id,
            kinds: OnceCell::new(),
            shapes: RefCell::new(HashMap::new()),
            calendars: OnceCell::new(),
            rsrc_cols: OnceCell::new(),
        }
    }

    /// The duration type code of an activity of this project.
    pub fn duration_type(&self, task_id: i64) -> &str {
        self.kinds
            .get_or_init(|| {
                self.doc.table("TASK").map_or_else(HashMap::new, |t| {
                    let cols = t.field_index();
                    let c_proj = cols.col("proj_id");
                    let c_task = cols.col("task_id");
                    let c_dur = cols.col("duration_type");
                    let mut map = HashMap::new();
                    for r in &t.rows {
                        if num(crate::FieldIndex::get(r, c_proj)) == self.pid {
                            map.entry(num(crate::FieldIndex::get(r, c_task)))
                                .or_insert_with(|| crate::FieldIndex::get(r, c_dur).to_string());
                        }
                    }
                    map
                })
            })
            .get(&task_id)
            .map_or("", String::as_str)
    }

    fn shape(&self, curv_id: i64) -> Option<Shape> {
        *self.shapes.borrow_mut().entry(curv_id).or_insert_with(|| {
            curves::by_id(self.doc, curv_id).and_then(|c| Shape::from_points(&c.points))
        })
    }

    /// The curve the assignment's row has, when it has one with weight;
    /// whether it applies is not checked.
    pub fn curve_of(&self, row: usize) -> Option<Shape> {
        curves::of_assignment(self.doc, row).and_then(|id| self.shape(id))
    }

    /// The calendar (model index) resource `rsrc` works on, when it has
    /// recurring work: the calendar the engine puts the resource's
    /// assignments on in a resource dependent activity.
    fn resource_calendar(&self, rsrc: i64) -> Option<usize> {
        self.calendars
            .get_or_init(|| {
                let cals = &self.model.calendars;
                let cal_pos: HashMap<&str, usize> = cals
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| has_recurring_work(&c.cal))
                    .map(|(i, c)| (c.id.as_str(), i))
                    .collect();
                self.doc.table("RSRC").map_or_else(HashMap::new, |t| {
                    let cols = t.field_index();
                    let c_clndr = cols.col("clndr_id");
                    let c_rsrc = cols.col("rsrc_id");
                    t.rows
                        .iter()
                        .filter_map(|r| {
                            let id = crate::FieldIndex::get(r, c_clndr);
                            cal_pos
                                .get(id)
                                .map(|&c| (num(crate::FieldIndex::get(r, c_rsrc)), c))
                        })
                        .collect()
                })
            })
            .get(&rsrc)
            .copied()
    }

    /// The shaping of TASKRSRC row `row` on `activity`; `None` only when the
    /// row does not exist.
    pub fn shaping(&self, row: usize, activity: &Activity) -> Option<Shaping> {
        let t = self.doc.table("TASKRSRC")?;
        let r = t.rows.get(row)?;
        let cols = self.rsrc_cols.get_or_init(|| t.field_index());
        let shape = self
            .curve_of(row)
            .filter(|_| curve_applies(self.duration_type(activity.task_id)));
        let own = matches!(activity.task_type, TaskType::Task | TaskType::Resource);
        // Without units there is no own duration: the assignment spans the
        // activity, so a cost on it spreads over the activity's span.
        let length = |qty: &str, rate: &str| {
            let (qty, rate) = (float(cols.value(r, qty)), float(cols.value(r, rate)));
            (own && rate > 0. && qty > 0.).then(|| (qty / rate * 60.).round_ties_even() as i64)
        };
        let lag = |k: &str| if own { minutes(cols.value(r, k)) } else { 0 };
        // A resource dependent activity's assignments work on their
        // resource's calendar. The engine's `Assignment.cal` carries it only
        // while remaining units are left; the spread takes it from the task
        // type and the resource, so it stays put when the assignment
        // completes.
        let cal = if activity.task_type == TaskType::Resource {
            self.resource_calendar(num(cols.value(r, "rsrc_id")))
        } else {
            None
        };
        let origin = self.model.origin_serial * openxer_engine::calendar::DAY;
        let stored = |k: &str| crate::parse_absolute(cols.value(r, k)).map(|v| v - origin);
        let data_date = self.model.project.data_date;
        let started = activity.act_start.is_some();
        let act_start = stored("act_start_date").filter(|&s| {
            started
                && data_date.is_none_or(|d| s <= d)
                && activity.act_start.is_none_or(|x| s >= x)
                && activity.act_finish.is_none_or(|x| s <= x)
        });
        let first = act_start.or(activity.act_start);
        let act_finish = stored("act_end_date").filter(|&f| {
            started
                && data_date.is_none_or(|d| f <= d)
                && first.is_none_or(|s| f >= s)
                && activity.act_finish.is_none_or(|x| f <= x)
        });
        Some(Shaping {
            shape,
            budget_lag: lag("target_lag_drtn_hr_cnt"),
            remaining_lag: lag("relag_drtn_hr_cnt"),
            budget_minutes: length("target_qty", "target_qty_per_hr"),
            remaining_minutes: length("remain_qty", "remain_qty_per_hr"),
            cal,
            act_start,
            act_finish,
            manual: Manual::of_row(t, r),
        })
    }
}

/// Adds one spread's shares to the periods and remembers what it added, so
/// a spread whose periods cover its whole window can put the floating-point
/// remainder into its last non-zero period: summed in period order, the
/// periods then give the quantity exactly.
struct Placer<'o> {
    out: &'o mut [f64],
    /// What the earlier non-zero shares add up to, in period order.
    before: f64,
    /// The last non-zero share and its period.
    last: Option<(usize, f64)>,
}

impl<'o> Placer<'o> {
    fn new(out: &'o mut [f64]) -> Self {
        Self {
            out,
            before: 0.,
            last: None,
        }
    }

    fn add(&mut self, i: usize, v: f64) {
        if v == 0. {
            return;
        }
        if let Some((_, previous)) = self.last {
            self.before += previous;
        }
        self.out[i] += v;
        self.last = Some((i, v));
    }

    /// The periods covered the whole window: make the shares total
    /// `quantity` by correcting the last one.
    fn settle(self, quantity: f64) {
        if let Some((i, v)) = self.last {
            self.out[i] += (quantity - self.before) - v;
        }
    }
}

/// Spread `quantity` evenly over the work time of `calendar` in
/// `[start, finish)` and add each period's share. Zero work time puts it all
/// in the period holding `start`.
pub fn even(
    quantity: f64,
    start: i64,
    finish: i64,
    calendar: &WorkCalendar,
    periods: &[(i64, i64)],
    out: &mut [f64],
) {
    if quantity == 0. {
        return;
    }
    let total = if finish > start {
        calendar.duration(start, finish)
    } else {
        0
    };
    if total <= 0 {
        if let Some(i) = periods.iter().position(|(a, b)| start >= *a && start < *b) {
            out[i] += quantity;
        }
        return;
    }
    let mut placer = Placer::new(out);
    let mut covered = 0;
    for (i, (a, b)) in periods.iter().enumerate() {
        let (s, e) = (start.max(*a), finish.min(*b));
        if s < e {
            let work = calendar.duration(s, e);
            covered += work;
            placer.add(i, quantity * work as f64 / total as f64);
        }
    }
    if covered == total {
        placer.settle(quantity);
    }
}

/// Spread `quantity` over `[start, finish)` along the part of `shape` from
/// curve position `from` to `to` (0 to 1), mapped linearly onto the working
/// time of the window. With `lump`, the curve's start value is spent at
/// `start` (actuals). The part's weight is rescaled to `quantity`; a part
/// with no weight spreads evenly.
#[allow(clippy::too_many_arguments)]
pub fn shaped(
    quantity: f64,
    start: i64,
    finish: i64,
    calendar: &WorkCalendar,
    periods: &[(i64, i64)],
    out: &mut [f64],
    shape: &Shape,
    (from, to): (f64, f64),
    lump: bool,
) {
    if quantity == 0. {
        return;
    }
    let (from, to) = (from.clamp(0., 1.), to.clamp(0., 1.).max(from.clamp(0., 1.)));
    let base = shape.cumulative(from);
    let span = shape.cumulative(to) - base;
    let start_weight = if lump { shape.start } else { 0. };
    let weight = span + start_weight;
    let total = if finish > start {
        calendar.duration(start, finish)
    } else {
        0
    };
    if weight <= 0. || total <= 0 {
        // No weight, or no working time (all at the start): as an even spread.
        even(quantity, start, finish, calendar, periods, out);
        return;
    }
    let at = |w: i64| shape.cumulative(from + (to - from) * w as f64 / total as f64) - base;
    weighted(
        quantity,
        (start, finish, total),
        calendar,
        periods,
        out,
        &at,
        weight,
        (lump, start_weight),
    );
}

/// Spread `quantity` over `[start, finish)` by manual runs: each run's
/// units evenly over its share of the window's working time, the runs
/// stretched to the window and their units scaled to `quantity` (see the
/// module notes). Runs with nothing in them, or no working time, spread
/// evenly.
pub fn by_runs(
    quantity: f64,
    start: i64,
    finish: i64,
    calendar: &WorkCalendar,
    periods: &[(i64, i64)],
    out: &mut [f64],
    runs: &[Run],
) {
    if quantity == 0. {
        return;
    }
    let weight = manual_units::total_units(runs);
    let total = if finish > start {
        calendar.duration(start, finish)
    } else {
        0
    };
    if weight <= 0. || total <= 0 || manual_units::total_hours(runs) <= 0. {
        even(quantity, start, finish, calendar, periods, out);
        return;
    }
    let at = |w: i64| manual_units::cumulative(runs, w as f64 / total as f64);
    weighted(
        quantity,
        (start, finish, total),
        calendar,
        periods,
        out,
        &at,
        weight,
        (false, 0.),
    );
}

/// Place `quantity` over the working time `total` of `[start, finish)` by
/// the cumulative weight `at(w)` at `w` working minutes from `start` (of
/// `weight` in all); with `lump.0`, `lump.1` more is spent at `start`.
#[allow(clippy::too_many_arguments)]
fn weighted(
    quantity: f64,
    (start, finish, total): (i64, i64, i64),
    calendar: &WorkCalendar,
    periods: &[(i64, i64)],
    out: &mut [f64],
    at: &dyn Fn(i64) -> f64,
    weight: f64,
    (lump, start_weight): (bool, f64),
) {
    // A cursor keeps the working time from `start` incremental for periods
    // in time order.
    let mut cursor = (start, 0_i64);
    let mut placer = Placer::new(out);
    let mut covered = 0;
    let mut lumped = start_weight == 0.;
    for (i, (a, b)) in periods.iter().enumerate() {
        let (s, e) = (start.max(*a), finish.min(*b));
        let mut share = 0.;
        if lump && start >= *a && start < *b {
            share += start_weight;
            lumped = true;
        }
        if s < e {
            let ws = if s >= cursor.0 {
                cursor.1 + calendar.duration(cursor.0, s)
            } else {
                calendar.duration(start, s)
            };
            let work = calendar.duration(s, e);
            covered += work;
            let we = ws + work;
            cursor = (e, we);
            share += at(we) - at(ws);
        }
        placer.add(i, quantity * share / weight);
    }
    if covered == total && lumped {
        placer.settle(quantity);
    }
}

/// One assignment as the spreads see it: its activity, the activity's
/// original duration (minutes) and its shaping (`None`: the activity's own
/// dates and calendar, evenly).
#[derive(Clone, Copy, Debug)]
pub struct Item<'a> {
    pub activity: &'a Activity,
    pub original: i64,
    pub shaping: Option<&'a Shaping>,
}

/// The calendar an assignment works on: its own (a resource dependent
/// activity's resource calendar), else the activity's.
pub fn calendar<'m>(m: &'m Model, it: &Item) -> &'m CalendarAdapter {
    let k = it.shaping.and_then(|s| s.cal).unwrap_or(it.activity.cal);
    &m.calendars[k.min(m.calendars.len().saturating_sub(1))]
}

fn shape_of<'s>(it: &Item<'s>) -> Option<&'s Shape> {
    it.shaping.and_then(|s| s.shape.as_ref())
}

/// Which window's manual runs a spread reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Budget,
    Remaining,
    Actual,
}

/// The manual runs the assignment holds for `side`.
fn runs_of<'s>(it: &Item<'s>, side: Side) -> Option<&'s [Run]> {
    let m = it.shaping?.manual.as_ref()?;
    match side {
        Side::Budget => m.budget.as_deref(),
        Side::Remaining => m.remaining.as_deref(),
        Side::Actual => m.actual.as_deref(),
    }
}

/// The assignment's own window inside the activity's `[start, finish)`: its
/// lag after the start and its own length, as the schedule writes the
/// assignment's dates (an assignment as long as the activity, within 1%,
/// and without lag keeps the activity's dates).
fn window(
    cal: &CalendarAdapter,
    start: i64,
    finish: i64,
    lag: i64,
    length: Option<i64>,
    whole: i64,
) -> (i64, i64) {
    let length = length.unwrap_or(whole);
    if lag == 0 && (length - whole).abs() <= (whole.abs() / 100).max(2) {
        return (start, finish.max(start));
    }
    let s = if lag > 0 {
        cal.add_start(start, lag)
    } else {
        start
    };
    let f = if length > 0 { cal.add(s, length) } else { s };
    (s, f.max(s))
}

/// The activity's remaining dates the usage views spread over: leveled when
/// the project levels, else early; from the data date once started.
fn remaining_base(m: &Model, a: &Activity) -> Option<(i64, i64)> {
    let leveled = m.options.do_level;
    let start = if leveled { a.restart } else { a.early_start }.or(a.target_start);
    let finish = if leveled { a.reend } else { a.early_finish }.or(a.target_finish);
    let (mut start, finish) = (start?, finish?);
    if a.act_start.is_some() {
        start = start.max(m.project.data_date.unwrap_or(0));
    }
    Some((start, finish.max(start)))
}

fn late_base(m: &Model, a: &Activity) -> Option<(i64, i64)> {
    let (mut start, finish) = (a.late_start?, a.late_finish?);
    if a.act_start.is_some() {
        start = start.max(m.project.data_date.unwrap_or(0));
    }
    Some((start, finish.max(start)))
}

/// The assignment's actual window: its own actual dates where the row
/// carries them, else the activity's; to the data date until finished.
fn actual_base(m: &Model, it: &Item) -> Option<(i64, i64)> {
    let a = it.activity;
    let own = it.shaping;
    let start = own.and_then(|s| s.act_start).or(a.act_start)?;
    let finish = own
        .and_then(|s| s.act_finish)
        .or(a.act_finish)
        .unwrap_or(m.project.data_date.unwrap_or(0))
        .max(start);
    Some((start, finish))
}

/// The windows an assignment's quantities spread over, model minutes
/// `[start, finish)`: the assignment's own dates (see [`Shaping`]); with no
/// shaping at all, the activity's dates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Windows {
    pub budget: Option<(i64, i64)>,
    pub actual: Option<(i64, i64)>,
    pub remaining: Option<(i64, i64)>,
    pub remaining_late: Option<(i64, i64)>,
}

/// The budget window over planned dates `[start, finish)`.
fn budget_window(m: &Model, it: &Item, start: i64, finish: i64) -> (i64, i64) {
    match it.shaping {
        None => (start, finish.max(start)),
        Some(s) => window(
            calendar(m, it),
            start,
            finish,
            s.budget_lag,
            s.budget_minutes,
            it.original,
        ),
    }
}

/// An assignment's remaining window over the activity's `base` dates.
fn remaining_over(m: &Model, it: &Item, base: Option<(i64, i64)>) -> Option<(i64, i64)> {
    let a = it.activity;
    if a.is_completed() {
        return None;
    }
    let (start, finish) = base?;
    Some(match it.shaping {
        None => (start, finish),
        Some(s) => {
            // Lag counts only before the activity starts.
            let lag = if a.act_start.is_some() {
                0
            } else {
                s.remaining_lag
            };
            window(
                calendar(m, it),
                start,
                finish,
                lag,
                s.remaining_minutes,
                a.rem,
            )
        }
    })
}

/// Every window of an assignment.
pub fn windows(m: &Model, it: &Item) -> Windows {
    let a = it.activity;
    Windows {
        budget: match (
            a.target_start.or(a.early_start),
            a.target_finish.or(a.early_finish),
        ) {
            (Some(s), Some(f)) => Some(budget_window(m, it, s, f)),
            _ => None,
        },
        actual: actual_base(m, it),
        remaining: remaining_over(m, it, remaining_base(m, a)),
        remaining_late: remaining_over(m, it, late_base(m, a)),
    }
}

/// How far through its curve a curved assignment is: actual working time
/// over actual plus remaining working time; 0 before it starts, 1 once
/// completed. 0 without a curve (an even spread needs none).
pub fn position(m: &Model, it: &Item) -> f64 {
    position_in(m, it, &windows(m, it))
}

/// [`position`] over windows already worked out.
fn position_in(m: &Model, it: &Item, w: &Windows) -> f64 {
    if shape_of(it).is_none() {
        return 0.;
    }
    let Some((start, finish)) = w.actual else {
        return 0.;
    };
    if it.activity.is_completed() {
        return 1.;
    }
    let cal = &calendar(m, it).cal;
    let done = cal.duration(start, finish).max(0) as f64;
    let left = w
        .remaining
        .map_or(0., |(rs, rf)| cal.duration(rs, rf).max(0) as f64);
    if done + left <= 0. {
        0.
    } else {
        done / (done + left)
    }
}

/// What every spread of one assignment shares, worked out once: its windows
/// and how far through its curve it is. Both walk the calendar, so a caller
/// that spreads an assignment more than once (budget, actual, remaining and
/// late; again for other periods) keeps its plan. A plan belongs to the
/// model and the assignment it was made from.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Plan {
    pub windows: Windows,
    /// Curve position (see [`position`]); 0 without a curve.
    pub position: f64,
}

impl Plan {
    pub fn of(m: &Model, it: &Item) -> Plan {
        let windows = windows(m, it);
        Plan {
            position: position_in(m, it, &windows),
            windows,
        }
    }
}

/// Spread over `window` by the manual runs of `side` when the assignment
/// has them, else evenly, or along the curve part `part` when the
/// assignment has a curve.
#[allow(clippy::too_many_arguments)]
fn along(
    m: &Model,
    it: &Item,
    quantity: f64,
    (window, side): (Option<(i64, i64)>, Side),
    part: (f64, f64),
    lump: bool,
    periods: &[(i64, i64)],
    out: &mut [f64],
) {
    let Some((start, finish)) = window else {
        return;
    };
    let cal = &calendar(m, it).cal;
    if let Some(runs) = runs_of(it, side) {
        by_runs(quantity, start, finish, cal, periods, out, runs);
        return;
    }
    match shape_of(it) {
        None => even(quantity, start, finish, cal, periods, out),
        Some(shape) => shaped(
            quantity, start, finish, cal, periods, out, shape, part, lump,
        ),
    }
}

/// Budgeted quantity over the planned dates. `plan` is the assignment's
/// [`Plan`] in `m`.
pub fn budget(
    m: &Model,
    it: &Item,
    plan: &Plan,
    quantity: f64,
    periods: &[(i64, i64)],
    out: &mut [f64],
) {
    let w = (plan.windows.budget, Side::Budget);
    along(m, it, quantity, w, (0., 1.), false, periods, out);
}

/// Actual quantity from the actual start to the actual finish or the data
/// date; along the curve up to the assignment's position, with the curve's
/// start value spent at the actual start.
pub fn actual(
    m: &Model,
    it: &Item,
    plan: &Plan,
    quantity: f64,
    periods: &[(i64, i64)],
    out: &mut [f64],
) {
    let w = (plan.windows.actual, Side::Actual);
    along(m, it, quantity, w, (0., plan.position), true, periods, out);
}

/// Remaining quantity over the remaining (early, or leveled) dates; along
/// the rest of the curve from the assignment's position.
pub fn remaining(
    m: &Model,
    it: &Item,
    plan: &Plan,
    quantity: f64,
    periods: &[(i64, i64)],
    out: &mut [f64],
) {
    let w = (plan.windows.remaining, Side::Remaining);
    along(m, it, quantity, w, (plan.position, 1.), false, periods, out);
}

/// Remaining quantity over the remaining late dates, with the same curve
/// part as the early remaining.
pub fn remaining_late(
    m: &Model,
    it: &Item,
    plan: &Plan,
    quantity: f64,
    periods: &[(i64, i64)],
    out: &mut [f64],
) {
    let w = (plan.windows.remaining_late, Side::Remaining);
    along(m, it, quantity, w, (plan.position, 1.), false, periods, out);
}

/// One TASKRSRC row of a project as the time-phased views read it: the
/// usage views and the Resource Assignments view (desktop), the command
/// line's `usage` and the S-curve export all build their assignments here.
#[derive(Clone, Debug, PartialEq)]
pub struct AssignmentRow {
    /// TASKRSRC row index.
    pub row: usize,
    /// The activity (model index).
    pub task: usize,
    pub rsrc: i64,
    /// Budgeted (`target_qty`), actual (regular plus overtime) and remaining
    /// units.
    pub units: [f64; 3],
    /// Budgeted, actual (regular plus overtime) and remaining cost.
    pub costs: [f64; 3],
    /// The activity's original duration in minutes.
    pub original: i64,
    /// See [`Shaping`].
    pub shaping: Option<Shaping>,
}

impl AssignmentRow {
    /// The assignment as the spreads see it, in `m`, the model it was read
    /// from.
    pub fn item<'a>(&'a self, m: &'a Model) -> Item<'a> {
        Item {
            activity: &m.activities[self.task],
            original: self.original,
            shaping: self.shaping.as_ref(),
        }
    }
}

/// The project's assignments on activities of its model, in TASKRSRC order.
pub fn assignment_rows(p: &crate::ProjectData) -> Vec<AssignmentRow> {
    let index: HashMap<i64, usize> = p
        .model
        .activities
        .iter()
        .enumerate()
        .map(|(i, a)| (a.task_id, i))
        .collect();
    let pid = p.model.project.proj_id;
    let lookup = Lookup::new(p);
    p.document.table("TASKRSRC").map_or(vec![], |t| {
        t.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| num(t.value(r, "proj_id")) == pid)
            .filter_map(|(row, r)| {
                let task = *index.get(&num(t.value(r, "task_id")))?;
                let activity = &p.model.activities[task];
                let v = |k: &str| float(t.value(r, k));
                Some(AssignmentRow {
                    row,
                    task,
                    rsrc: num(t.value(r, "rsrc_id")),
                    units: [
                        v("target_qty"),
                        v("act_reg_qty") + v("act_ot_qty"),
                        v("remain_qty"),
                    ],
                    costs: [
                        v("target_cost"),
                        v("act_reg_cost") + v("act_ot_cost"),
                        v("remain_cost"),
                    ],
                    original: p
                        .details
                        .get(&activity.task_id)
                        .map_or(0, |d| d.original_duration),
                    shaping: lookup.shaping(row, activity),
                })
            })
            .collect()
    })
}

/// Ids of the labor resources of `doc`: every RSRC row that is not
/// equipment (`RT_Equip`) or material (`RT_Mat`), as the usage views and
/// the earned value figures count labor.
pub fn labor_resources(doc: &Document) -> std::collections::BTreeSet<i64> {
    doc.table("RSRC").map_or_else(Default::default, |t| {
        t.rows
            .iter()
            .filter(|r| !matches!(t.value(r, "rsrc_type"), "RT_Equip" | "RT_Mat"))
            .map(|r| num(t.value(r, "rsrc_id")))
            .collect()
    })
}

/// The share (0 to 1) of an assignment's budget planned by moment `at` over
/// planned dates `[start, finish)`: the curve's cumulative weight at that
/// point of the assignment's planned window; without a curve, the working
/// time share of the window.
pub fn planned_share(m: &Model, it: &Item, start: i64, finish: i64, at: i64) -> f64 {
    let cal = &calendar(m, it).cal;
    let (ws, wf) = budget_window(m, it, start, finish);
    if at >= wf {
        return 1.;
    }
    if at <= ws {
        return 0.;
    }
    let total = cal.duration(ws, wf);
    if total <= 0 {
        return 1.;
    }
    let x = cal.duration(ws, at) as f64 / total as f64;
    let runs = runs_of(it, Side::Budget)
        .filter(|r| manual_units::total_units(r) > 0. && manual_units::total_hours(r) > 0.);
    if let Some(runs) = runs {
        return manual_units::cumulative(runs, x) / manual_units::total_units(runs);
    }
    match shape_of(it) {
        None => x.clamp(0., 1.),
        Some(shape) => shape.cumulative(x) / shape.cumulative(1.),
    }
}
