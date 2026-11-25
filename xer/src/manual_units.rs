//! Manual period values of an assignment: the runs a TASKRSRC row keeps in
//! `target_crv` (budgeted), `remain_crv` (remaining) and `actual_crv`
//! (actual) once a period's units were typed into a usage spreadsheet.
//!
//! The stored form, as observed in an export after one remaining value was
//! typed: runs separated by `;`, each `units:hours`. `units` is what the run
//! holds in total and `hours` the working hours it covers, counted in time
//! order from the start of the assignment's window (its remaining start for
//! the remaining runs). Neighbouring periods that hold the same amount per
//! hour are one run: five 10 hour days of 52 units with 80 typed into the
//! third day read `104:20;80:10;104:20`. The hours add up to the window's
//! working time. A blank column means no manual values.
//!
//! Text is kept as read: a row is rewritten only by an edit of its periods
//! (`ProjectData::set_period_units`) or by clearing them, so a file's own
//! runs, and every row without any, are written back byte for byte.
use crate::assignments::quantity_text;
use crate::edit::records;
use crate::Document;

/// The columns, in the order budgeted, remaining, actual.
pub const BUDGET: &str = "target_crv";
pub const REMAINING: &str = "remain_crv";
pub const ACTUAL: &str = "actual_crv";
pub const COLUMNS: [&str; 3] = [BUDGET, REMAINING, ACTUAL];

/// The name a usage view and the Curve column give an assignment with
/// manual values.
pub const MANUAL: &str = "Manual";

/// One run: `units` spread evenly over `hours` of working time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Run {
    pub units: f64,
    pub hours: f64,
}

impl Run {
    fn rate(&self) -> f64 {
        self.units / self.hours
    }
}

/// The runs of a stored text; `None` when it is blank or not in the run
/// form (it is then not read, and kept as it is).
pub fn parse(text: &str) -> Option<Vec<Run>> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut out = vec![];
    for piece in text.split(';') {
        let (units, hours) = piece.split_once(':')?;
        let units: f64 = units.trim().parse().ok()?;
        let hours: f64 = hours.trim().parse().ok()?;
        if !units.is_finite() || !hours.is_finite() || units < 0. || hours < 0. {
            return None;
        }
        out.push(Run { units, hours });
    }
    Some(out)
}

/// The stored text of `runs`: units and hours written as the file writes
/// quantities (up to 15 significant digits, no trailing zeros).
pub fn format(runs: &[Run]) -> String {
    runs.iter()
        .map(|r| format!("{}:{}", quantity_text(r.units), quantity_text(r.hours)))
        .collect::<Vec<_>>()
        .join(";")
}

/// Same amount per hour, to the precision the file keeps.
fn same_rate(a: &Run, b: &Run) -> bool {
    if a.hours <= 0. || b.hours <= 0. {
        return false;
    }
    let (x, y) = (a.rate(), b.rate());
    (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1e-9)
}

/// Neighbouring runs with the same amount per hour joined into one; runs
/// without hours are dropped when they hold nothing.
pub fn merged(runs: &[Run]) -> Vec<Run> {
    let mut out: Vec<Run> = vec![];
    for r in runs {
        if r.hours <= 0. && r.units == 0. {
            continue;
        }
        match out.last_mut() {
            Some(last) if same_rate(last, r) => {
                last.units += r.units;
                last.hours += r.hours;
            }
            _ => out.push(*r),
        }
    }
    out
}

pub fn total_units(runs: &[Run]) -> f64 {
    runs.iter().map(|r| r.units).sum()
}

pub fn total_hours(runs: &[Run]) -> f64 {
    runs.iter().map(|r| r.hours).sum()
}

/// The units the runs hold up to fraction `x` (0 to 1) of their hours,
/// evenly inside a run. A run without hours counts once its place is
/// passed.
pub fn cumulative(runs: &[Run], x: f64) -> f64 {
    if x >= 1. {
        return total_units(runs);
    }
    let pos = x.max(0.) * total_hours(runs);
    let mut sum = 0.;
    let mut at = 0.;
    for r in runs {
        if r.hours <= 0. {
            if at < pos {
                sum += r.units;
            }
            continue;
        }
        let end = at + r.hours;
        if pos >= end {
            sum += r.units;
        } else {
            if pos > at {
                sum += r.units * (pos - at) / r.hours;
            }
            break;
        }
        at = end;
    }
    sum
}

/// `runs` with the hours `[from, to)` (counted from the start) holding
/// `units`, split where the edges fall inside a run and merged again. Hours
/// past the runs' end are not added.
pub fn with_period(runs: &[Run], from: f64, to: f64, units: f64) -> Vec<Run> {
    let mut out = vec![];
    let mut at = 0.;
    let mut placed = false;
    let place = |out: &mut Vec<Run>, placed: &mut bool| {
        if !*placed {
            out.push(Run {
                units,
                hours: to - from,
            });
            *placed = true;
        }
    };
    for r in runs {
        let end = at + r.hours;
        if r.hours <= 0. {
            if at < from || at >= to {
                out.push(*r);
            }
            continue;
        }
        let rate = r.rate();
        if from > at {
            let h = from.min(end) - at;
            out.push(Run {
                units: rate * h,
                hours: h,
            });
        }
        if end > from && at < to {
            place(&mut out, &mut placed);
        }
        if end > to {
            let h = end - to.max(at);
            out.push(Run {
                units: rate * h,
                hours: h,
            });
        }
        at = end;
    }
    place(&mut out, &mut placed);
    merged(&out)
}

/// One row's runs of column `column`, when it holds the run form.
pub fn runs_of(doc: &Document, row: usize, column: &str) -> Option<Vec<Run>> {
    parse(&records::get(doc, "TASKRSRC", row, column))
}

/// Whether TASKRSRC row `row` has manual period values.
pub fn is_manual(doc: &Document, row: usize) -> bool {
    COLUMNS
        .iter()
        .any(|c| !records::get(doc, "TASKRSRC", row, c).trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runs(pairs: &[(f64, f64)]) -> Vec<Run> {
        pairs
            .iter()
            .map(|&(units, hours)| Run { units, hours })
            .collect()
    }

    #[test]
    fn the_stored_form_reads_and_writes_back() {
        // Four 8 hour days of 30 units, 45 typed on the second: day 1 alone,
        // day 2, then days 3 and 4 merged.
        let text = "30:8;45:8;60:16";
        let r = parse(text).unwrap();
        assert_eq!(r, runs(&[(30., 8.), (45., 8.), (60., 16.)]));
        assert_eq!(format(&r), text);
        assert_eq!(total_units(&r), 135.);
        assert_eq!(total_hours(&r), 32.);
        // Decimals as the file writes quantities.
        let odd = runs(&[(12.345, 7.5), (0.1 + 0.2, 0.5)]);
        assert_eq!(format(&odd), "12.345:7.5;0.3:0.5");
        assert!(parse(" ").is_none());
        assert!(parse("").is_none());
        for bad in ["30", "30:x", "30:8;", ":8", "-1:8", "30:-8", "1:inf"] {
            assert!(parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn typing_a_period_splits_and_merges_runs() {
        // One even run of four 8 hour days, 30 a day.
        let even = runs(&[(120., 32.)]);
        let typed = with_period(&even, 8., 16., 45.);
        assert_eq!(format(&typed), "30:8;45:8;60:16");
        // Typing the old value back joins the runs again.
        let back = with_period(&typed, 8., 16., 30.);
        assert_eq!(format(&back), "120:32");
        // The first and the last period, and a period across two runs.
        assert_eq!(format(&with_period(&even, 0., 8., 0.)), "0:8;90:24");
        assert_eq!(format(&with_period(&even, 24., 32., 10.)), "90:24;10:8");
        let across = with_period(&typed, 12., 20., 20.);
        assert_eq!(format(&across), "30:8;22.5:4;20:8;45:12");
        assert!((total_units(&across) - 117.5).abs() < 1e-9);
        assert_eq!(total_hours(&across), 32.);
    }

    #[test]
    fn cumulative_follows_the_runs() {
        let r = runs(&[(30., 8.), (45., 8.), (60., 16.)]);
        assert_eq!(cumulative(&r, 0.), 0.);
        assert_eq!(cumulative(&r, 0.125), 15.);
        assert_eq!(cumulative(&r, 0.25), 30.);
        assert_eq!(cumulative(&r, 0.5), 75.);
        assert_eq!(cumulative(&r, 0.75), 105.);
        assert_eq!(cumulative(&r, 1.), 135.);
    }

    /// A row with runs and rows without any survive read and write as their
    /// text was, and the runs read back as written.
    #[test]
    fn rows_keep_their_text_through_a_file_round_trip() {
        let mut doc = crate::starter().document;
        let fields: Vec<String> = crate::standard_fields("TASKRSRC")
            .unwrap()
            .iter()
            .map(|f| f.to_string())
            .collect();
        let t = doc.table_mut("TASKRSRC");
        t.fields = fields;
        let texts = [
            ("1", "30:8;45:8;60:16", "30:8;45:8;60:16", ""),
            ("2", "", "", ""),
            // Not the run form: kept as it is, not read.
            ("3", "1.50:8.0", "odd text", "0:0"),
        ];
        for (id, budget, remaining, actual) in texts {
            t.add(&[
                ("taskrsrc_id", id.into()),
                ("task_id", "1010".into()),
                ("proj_id", "1".into()),
                (BUDGET, budget.into()),
                (REMAINING, remaining.into()),
                (ACTUAL, actual.into()),
            ]);
        }
        let text = doc.text();
        let back = Document::parse(&text).unwrap();
        assert_eq!(back.text(), text);
        for (row, (_, budget, remaining, actual)) in texts.iter().enumerate() {
            assert_eq!(records::get(&back, "TASKRSRC", row, BUDGET), *budget);
            assert_eq!(records::get(&back, "TASKRSRC", row, REMAINING), *remaining);
            assert_eq!(records::get(&back, "TASKRSRC", row, ACTUAL), *actual);
        }
        let read = runs_of(&back, 0, REMAINING).unwrap();
        assert_eq!(format(&read), "30:8;45:8;60:16");
        assert!(runs_of(&back, 1, REMAINING).is_none());
        assert!(runs_of(&back, 2, REMAINING).is_none());
        assert_eq!(format(&runs_of(&back, 2, BUDGET).unwrap()), "1.5:8");
        assert!(is_manual(&back, 0));
        assert!(!is_manual(&back, 1));
    }
}
