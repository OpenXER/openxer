//! Typing the units of one period into a usage spreadsheet: a manual
//! period value of one assignment (see `crate::manual_units` for the
//! stored form).
//!
//! What an export of the reference showed after one future day of a not
//! started assignment's remaining units was typed over (five days of 10
//! hours, about 52 a day, 80 typed into the third):
//!
//! - only that day changed; the other days kept their units;
//! - the remaining units became the old total less the day's old units
//!   plus the typed units, and, the assignment being not started, the
//!   budgeted units followed; both units/time became the new total over
//!   the unchanged duration; the dates did not move;
//! - `remain_crv` and `target_crv` held the runs, the curve became Manual;
//! - the activity's units followed (the totals of its assignments);
//! - typing the old value back kept the Manual curve (one merged run).
//!
//! Here the budget follows while budget and remaining are linked (a not
//! started activity in a project that links them, as every other remaining
//! units edit). Periods that end on or before the data date, actual values
//! and budgeted cells of a started activity are refused: only future
//! remaining cells were seen being typed into.
use crate::assignments::{float, quantity_text};
use crate::edit::{curves, records};
use crate::manual_units::{self, Run};
use crate::spread::{self, Item, Plan};
use crate::ProjectData;

/// Where a period falls in an assignment's remaining window.
struct Place {
    /// The activity (model index) and its task id.
    i: usize,
    task_id: i64,
    /// The window's working hours and the period's hours inside it, counted
    /// from the window's start.
    hours: f64,
    from: f64,
    to: f64,
    /// The remaining runs as the spreadsheet shows them now.
    runs: Vec<Run>,
}

impl ProjectData {
    /// Whether the remaining units of TASKRSRC row `row` can be typed for
    /// the period `[start, finish)` (model minutes); the reason when not.
    pub fn check_period_units(&self, row: usize, period: (i64, i64)) -> Result<(), String> {
        self.period_place(row, period).map(|_| ())
    }

    /// Set the remaining units of TASKRSRC row `row` in the period
    /// `[start, finish)` (model minutes) to `units`, spread evenly over the
    /// period's working time; the other periods keep theirs. The assignment
    /// becomes Manual, its remaining total (and its budget while linked)
    /// follows, and so do its activity's units.
    pub fn set_period_units(
        &mut self,
        row: usize,
        period: (i64, i64),
        units: f64,
    ) -> Result<(), String> {
        if !(0. ..1.0e12).contains(&units) {
            return Err("Units must be zero or more".into());
        }
        let place = self.period_place(row, period)?;
        let runs = manual_units::with_period(&place.runs, place.from, place.to, units);
        let total = manual_units::total_units(&runs);
        let text = manual_units::format(&runs);
        let linked = self.linked(place.i);
        let doc = &self.document;
        let t = doc
            .table("TASKRSRC")
            .ok_or("The resource assignment no longer exists")?;
        let r = &t.rows[row];
        let budget = float(t.value(r, "target_qty"));
        let budget_rate = float(t.value(r, "target_qty_per_hr"));
        let budget_hours = if budget > 0. && budget_rate > 0. {
            budget / budget_rate
        } else {
            place.hours
        };
        let manual = curves::manual_curve(doc).map_or(String::new(), |id| id.to_string());
        let mut sets: Vec<(&str, String)> = vec![
            (manual_units::REMAINING, text.clone()),
            ("remain_qty", quantity_text(total)),
            ("remain_qty_per_hr", quantity_text(total / place.hours)),
            ("curv_id", manual),
        ];
        if linked {
            sets.push((manual_units::BUDGET, text));
            sets.push(("target_qty", quantity_text(total)));
            sets.push(("target_qty_per_hr", quantity_text(total / budget_hours)));
        }
        for (column, value) in sets {
            records::set(&mut self.document, "TASKRSRC", row, column, value);
        }
        self.update_costs(row);
        self.sync_activity_durations(place.i);
        self.rebuild_assignments();
        self.sync_task_rollups(place.task_id);
        Ok(())
    }

    fn period_place(&self, row: usize, (start, finish): (i64, i64)) -> Result<Place, String> {
        let (i, task_id) = self.assignment_task(row)?;
        let a = &self.model.activities[i];
        if a.is_completed() {
            return Err("A completed activity has no remaining units to spread".into());
        }
        if finish <= start {
            return Err("The period has no length".into());
        }
        if self.model.project.data_date.is_some_and(|d| finish <= d) {
            return Err("Periods before the data date cannot be typed into".into());
        }
        let shaping = spread::Lookup::new(self)
            .shaping(row, a)
            .ok_or("The resource assignment no longer exists")?;
        let original = self
            .details
            .get(&task_id)
            .map_or(0, |d| d.original_duration);
        let item = Item {
            activity: a,
            original,
            shaping: Some(&shaping),
        };
        let plan = Plan::of(&self.model, &item);
        let (ws, we) = plan
            .windows
            .remaining
            .ok_or("The assignment has no remaining dates yet; schedule the project first")?;
        let cal = &spread::calendar(&self.model, &item).cal;
        let total = if we > ws { cal.duration(ws, we) } else { 0 };
        if total <= 0 {
            return Err("The assignment has no remaining working time to spread over".into());
        }
        let (s, e) = (start.max(ws), finish.min(we));
        let inside = if e > s { cal.duration(s, e) } else { 0 };
        if inside <= 0 {
            return Err("None of the assignment's remaining work falls in this period".into());
        }
        let from = cal.duration(ws, s) as f64 / 60.;
        let hours = total as f64 / 60.;
        let quantity = float(&records::get(&self.document, "TASKRSRC", row, "remain_qty"));
        let manual = shaping.manual.as_ref().and_then(|m| m.remaining.as_deref());
        let runs = match (manual, shaping.shape) {
            (Some(runs), _) => fitted(runs, hours, quantity),
            (None, Some(shape)) => curve_runs(&shape, plan.position, hours, quantity),
            (None, None) => vec![Run {
                units: quantity,
                hours,
            }],
        };
        Ok(Place {
            i,
            task_id,
            hours,
            from,
            to: from + inside as f64 / 60.,
            runs,
        })
    }
}

/// Stored runs stretched to `hours` and scaled to `quantity`, as the
/// spread reads them.
fn fitted(runs: &[Run], hours: f64, quantity: f64) -> Vec<Run> {
    let (h, u) = (
        manual_units::total_hours(runs),
        manual_units::total_units(runs),
    );
    if h <= 0. {
        return vec![Run {
            units: quantity,
            hours,
        }];
    }
    let scale = if u > 0. { quantity / u } else { 1. };
    let fits = (h - hours).abs() < 1e-9 && (scale - 1.).abs() < 1e-12;
    if fits {
        return runs.to_vec();
    }
    runs.iter()
        .map(|r| Run {
            units: r.units * scale,
            hours: r.hours * hours / h,
        })
        .collect()
}

/// The rest of a curve from position `from` (0 to 1) as runs over `hours`
/// holding `quantity`: one run per curve segment, as the spread places
/// them (evenly inside a segment).
fn curve_runs(shape: &spread::Shape, from: f64, hours: f64, quantity: f64) -> Vec<Run> {
    let from = from.clamp(0., 1.);
    let span = shape.cumulative(1.) - shape.cumulative(from);
    if span <= 0. || from >= 1. {
        return vec![Run {
            units: quantity,
            hours,
        }];
    }
    let n = spread::SEGMENTS as f64;
    let mut out = vec![];
    for k in 0..spread::SEGMENTS {
        let lo = (k as f64 / n).max(from);
        let hi = (k + 1) as f64 / n;
        if hi <= lo {
            continue;
        }
        out.push(Run {
            units: quantity * (shape.cumulative(hi) - shape.cumulative(lo)) / span,
            hours: hours * (hi - lo) / (1. - from),
        });
    }
    manual_units::merged(&out)
}
