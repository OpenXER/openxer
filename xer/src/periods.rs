//! Calendar periods for time-phased answers: days, weeks, months and the
//! project's financial periods, as model minutes `[start, finish)`.
//!
//! The command line's `usage` (Monday weeks) and the S-curve export (weeks
//! from the timescale's week start, Sunday by default, as the usage views
//! bucket them) both cut their periods here.
use crate::edit::records::num;
use crate::{date_string, parse_absolute, parse_relative, Document};
use openxer_engine::calendar::DAY;

/// The length of a period.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interval {
    Day,
    /// Weeks starting on Sunday (`sunday`) or on Monday.
    Week {
        sunday: bool,
    },
    Month,
}

/// Start of the period holding model minute `t` (`origin` is the model's
/// origin day serial).
pub fn period_start(origin: i64, interval: Interval, t: i64) -> i64 {
    let day = t.div_euclid(DAY);
    match interval {
        Interval::Day => day * DAY,
        // Day serial 0 (1899-12-30) is a Saturday: serial 1 a Sunday and 2 a
        // Monday.
        Interval::Week { sunday } => {
            let first = if sunday { 1 } else { 2 };
            (day - (origin + day - first).rem_euclid(7)) * DAY
        }
        Interval::Month => {
            let text = date_string(origin, Some(day * DAY));
            parse_relative(origin, &format!("{}-01", &text[..7]))
                .ok()
                .flatten()
                .unwrap_or(day * DAY)
        }
    }
}

/// Start of the period after the one starting at `start`.
pub fn next_start(origin: i64, interval: Interval, start: i64) -> i64 {
    match interval {
        Interval::Day => start + DAY,
        Interval::Week { .. } => start + 7 * DAY,
        Interval::Month => period_start(origin, interval, start + 32 * DAY),
    }
}

/// The periods from the one holding `first` until one reaches `last` (at
/// least one period when `first <= last`); `Err(max)` past `max` periods.
pub fn sweep(
    origin: i64,
    interval: Interval,
    first: i64,
    last: i64,
    max: usize,
) -> Result<Vec<(i64, i64)>, usize> {
    let mut periods: Vec<(i64, i64)> = vec![];
    if first > last {
        return Ok(periods);
    }
    let mut s = period_start(origin, interval, first);
    while s <= last && (s < last || periods.is_empty()) {
        if periods.len() == max {
            return Err(max);
        }
        let e = next_start(origin, interval, s);
        periods.push((s, e));
        s = e;
    }
    Ok(periods)
}

/// The financial periods of project `proj_id` (FINDATES), in model minutes
/// of a model whose origin is day serial `origin`: the project's financial
/// period calendar (`PROJECT.fintmpl_id`) when it names one that has
/// periods, else the calendar with the lowest id. A period runs from the
/// start of its first day to the end of its last day; periods are in date
/// order and do not overlap.
pub fn financial(doc: &Document, proj_id: i64, origin: i64) -> Vec<(i64, i64)> {
    let Some(t) = doc.table("FINDATES") else {
        return vec![];
    };
    let calendar_of = |r: &Vec<String>| {
        t.field("fintmpl_id")
            .map_or(0, |_| num(t.value(r, "fintmpl_id")))
    };
    let own = doc
        .table("PROJECT")
        .and_then(|p| {
            p.rows
                .iter()
                .find(|r| num(p.value(r, "proj_id")) == proj_id)
                .map(|r| num(p.value(r, "fintmpl_id")))
        })
        .filter(|id| t.rows.iter().any(|r| calendar_of(r) == *id));
    let Some(calendar) = own.or_else(|| t.rows.iter().map(calendar_of).min()) else {
        return vec![];
    };
    let base = origin * DAY;
    let mut out: Vec<(i64, i64)> = t
        .rows
        .iter()
        .filter(|r| calendar_of(r) == calendar)
        .filter_map(|r| {
            let start = parse_absolute(t.value(r, "start_date"))?.div_euclid(DAY) * DAY;
            let end = (parse_absolute(t.value(r, "end_date"))?.div_euclid(DAY) + 1) * DAY;
            (end > start).then_some((start - base, end - base))
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    // A period that overlaps the one before it starts where that one ends.
    let mut clean: Vec<(i64, i64)> = Vec::with_capacity(out.len());
    for (s, e) in out {
        let s = clean.last().map_or(s, |&(_, pe)| s.max(pe));
        if e > s {
            clean.push((s, e));
        }
    }
    clean
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weeks_start_on_the_chosen_day_and_months_on_the_first() {
        // Origin day serial 46000 is 2025-12-09, a Tuesday.
        let origin = 46000;
        let tuesday = 0;
        let monday = period_start(origin, Interval::Week { sunday: false }, tuesday + 600);
        let sunday = period_start(origin, Interval::Week { sunday: true }, tuesday + 600);
        assert_eq!(monday, -DAY);
        assert_eq!(sunday, -2 * DAY);
        assert_eq!(date_string(origin, Some(monday)), "2025-12-08 00:00");
        let month = period_start(origin, Interval::Month, 30 * DAY);
        assert_eq!(date_string(origin, Some(month)), "2026-01-01 00:00");
        assert_eq!(
            date_string(origin, Some(next_start(origin, Interval::Month, month))),
            "2026-02-01 00:00"
        );
        let weeks = sweep(origin, Interval::Week { sunday: true }, 0, 15 * DAY, 100).unwrap();
        assert_eq!(weeks.len(), 3);
        assert!(weeks.windows(2).all(|w| w[0].1 == w[1].0));
        assert_eq!(
            sweep(origin, Interval::Day, 0, 50 * DAY, 10),
            Err(10),
            "a cap stops the sweep"
        );
    }

    #[test]
    fn financial_periods_cover_whole_days_of_the_projects_calendar() {
        let mut doc = crate::starter().document;
        for (id, cal, start, end) in [
            (1, 5, "2026-11-01 00:00", "2026-11-30 23:59"),
            (2, 5, "2026-12-01 00:00", "2026-12-31 23:59"),
            (3, 9, "2026-11-01 00:00", "2026-11-14 23:59"),
        ] {
            doc.table_mut("FINDATES").add(&[
                ("fin_dates_id", id.to_string()),
                ("fintmpl_id", cal.to_string()),
                ("fin_dates_name", format!("P{id}")),
                ("start_date", start.into()),
                ("end_date", end.into()),
            ]);
        }
        let p = crate::ProjectData::from_document(doc.clone(), 1, String::new()).unwrap();
        let origin = p.model.origin_serial;
        let periods = financial(&doc, 1, origin);
        assert_eq!(periods.len(), 2, "the lowest calendar without a choice");
        assert_eq!(
            date_string(origin, Some(periods[0].1)),
            "2026-12-01 00:00",
            "a period ends after its last day"
        );
        assert_eq!(periods[0].1, periods[1].0);
        doc.table_mut("PROJECT").set(0, "fintmpl_id", "9".into());
        assert_eq!(financial(&doc, 1, origin).len(), 1);
    }
}
