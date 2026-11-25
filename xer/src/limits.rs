//! A resource's limit per period: its rate rows (`RSRCRATE` max units per
//! hour from each effective date) times the working hours of the period on
//! the resource's own calendar (the project's default calendar when it has
//! none). The Resource Usage Profile draws this as its limit line, and the
//! agents' `resource_loading` answer measures against it, so the two agree.
use crate::{parse_absolute, ProjectData};
use openxer_engine::calendar::DAY;

fn num(s: &str) -> i64 {
    s.trim().parse().unwrap_or(0)
}

fn float(s: &str) -> f64 {
    s.trim().parse().unwrap_or(0.)
}

/// Rate rows for a resource: (effective date as absolute minutes, max units
/// per hour, price per unit), sorted by date.
pub fn rates(p: &ProjectData, rsrc: i64) -> Vec<(Option<i64>, f64, f64)> {
    let mut out: Vec<_> = p.document.table("RSRCRATE").map_or(vec![], |t| {
        t.rows
            .iter()
            .filter(|r| num(t.value(r, "rsrc_id")) == rsrc)
            .map(|r| {
                (
                    parse_absolute(t.value(r, "start_date")),
                    float(t.value(r, "max_qty_per_hr")),
                    float(t.value(r, "cost_per_qty")),
                )
            })
            .collect()
    });
    out.sort_by_key(|r| r.0);
    out
}

/// The rate in effect at `when` (absolute minutes): the latest one starting
/// on or before it, else the earliest.
pub fn rate_at(rates: &[(Option<i64>, f64, f64)], when: i64) -> Option<(f64, f64)> {
    rates
        .iter()
        .rev()
        .find(|r| r.0.is_none_or(|d| d <= when))
        .or(rates.first())
        .map(|r| (r.1, r.2))
}

/// The index in the model's calendars of resource `rsrc`'s calendar; the
/// project's default calendar when it names none the model has.
pub fn resource_calendar(p: &ProjectData, rsrc: i64) -> usize {
    let m = &p.model;
    p.document
        .table("RSRC")
        .and_then(|t| {
            t.rows
                .iter()
                .find(|r| num(t.value(r, "rsrc_id")) == rsrc)
                .map(|r| t.value(r, "clndr_id").to_string())
        })
        .and_then(|id| m.calendars.iter().position(|c| c.id == id))
        .unwrap_or(m.project_calendar)
}

/// Resource `rsrc`'s limit in units for each of `periods` (model-relative
/// `[start, finish)`): max units per hour in effect at the period's start
/// times its working hours on the resource's calendar. None when the
/// resource has no rate row: no limit, as leveling reads it (a row with
/// max units 0 is a real limit of zero). Unbounded edge periods (from
/// `i64::MIN / 8` or to `i64::MAX / 8`) return 0 without walking the calendar.
pub fn per_period(p: &ProjectData, rsrc: i64, periods: &[(i64, i64)]) -> Option<Vec<f64>> {
    const EDGE_GUARD: i64 = i64::MAX / 16;
    let rates = rates(p, rsrc);
    if rates.is_empty() {
        return None;
    }
    let m = &p.model;
    let base = m.origin_serial * DAY;
    let calendar = &m.calendars[resource_calendar(p, rsrc)].cal;
    Some(
        periods
            .iter()
            .map(|&(a, b)| {
                if a <= -EDGE_GUARD || b >= EDGE_GUARD || b <= a {
                    return 0.;
                }
                rate_at(&rates, a + base)
                    .map_or(0., |(max, _)| max * calendar.duration(a, b) as f64 / 60.)
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_period_skips_unbounded_edge_periods() {
        let p = crate::sample_project::sample_project().current;
        let rsrc = p
            .document
            .table("RSRCRATE")
            .and_then(|t| t.rows.first().map(|r| num(t.value(r, "rsrc_id"))))
            .expect("sample project has a resource rate");
        let edge = i64::MAX / 8;
        let periods = [(-edge, 0), (0, 5 * DAY), (5 * DAY, edge)];
        let got = per_period(&p, rsrc, &periods).expect("resource has a limit");
        assert_eq!(got[0], 0.);
        assert!(got[1] > 0.);
        assert_eq!(got[2], 0.);
    }
}
