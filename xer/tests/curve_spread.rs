//! Resource curves shape the time-phased spread (`openxer_xer::spread`).
//! Every expected number is worked out by hand in the comments, on a
//! synthetic calendar: Monday to Friday, 08:00-12:00 and 13:00-17:00 (8
//! hours), day 0 a Monday. Days 5, 6, 12, 13 are the weekends.
//!
//! The test curve "3 then 1" weighs the first ten 5% segments 3 and the last
//! ten 1 (total 40): three quarters of the units in the first half of the
//! working duration.
use openxer_engine::calendar::{CalendarAdapter, WorkCalendar, DAY};
use openxer_engine::model::{Activity, Model, StatusType};
use openxer_xer::edit::curves::{self, POINTS};
use openxer_xer::spread::{self, Item, Lookup, Plan, Shape, Shaping};
use std::collections::BTreeMap;

const H: i64 = 60;

fn calendar(holidays: &[i64]) -> CalendarAdapter {
    let exc: BTreeMap<i64, Vec<(i64, i64)>> = holidays.iter().map(|d| (*d, vec![])).collect();
    CalendarAdapter::new(WorkCalendar::new(
        "5x8".into(),
        false,
        0,
        std::array::from_fn(|i| {
            if i < 5 {
                vec![(8 * H, 12 * H), (13 * H, 17 * H)]
            } else {
                vec![]
            }
        }),
        exc,
    ))
}

/// Day `d` at `hour`.
fn at(d: i64, hour: i64) -> i64 {
    d * DAY + hour * H
}

fn days(n: i64) -> Vec<(i64, i64)> {
    (0..n).map(|d| (d * DAY, (d + 1) * DAY)).collect()
}

fn model(holidays: &[i64], data_date: i64) -> Model {
    let mut m = openxer_xer::starter().model;
    m.calendars = vec![calendar(holidays)];
    m.options.do_level = false;
    m.project.data_date = Some(data_date);
    m
}

/// A not-started activity over `[start, finish)` with `minutes` of work.
fn activity(start: i64, finish: i64, minutes: i64) -> Activity {
    Activity {
        cal: 0,
        rem: minutes,
        status: StatusType::NotStarted,
        early_start: Some(start),
        early_finish: Some(finish),
        late_start: Some(start),
        late_finish: Some(finish),
        target_start: Some(start),
        target_finish: Some(finish),
        ..Activity::default()
    }
}

fn three_then_one() -> Shape {
    let points: [f64; POINTS] = std::array::from_fn(|k| match k {
        0 => 0.,
        1..=10 => 3.,
        _ => 1.,
    });
    Shape::from_points(&points).unwrap()
}

fn shaping(shape: Shape) -> Shaping {
    Shaping {
        shape: Some(shape),
        ..Shaping::plain()
    }
}

fn close(got: &[f64], want: &[f64]) {
    assert_eq!(got.len(), want.len());
    for (k, (g, w)) in got.iter().zip(want).enumerate() {
        assert!((g - w).abs() < 1e-9, "period {k}: {g} vs {w}\n{got:?}");
    }
}

fn remaining(m: &Model, it: &Item, q: f64, n: i64) -> Vec<f64> {
    let mut out = vec![0.; n as usize];
    spread::remaining(m, it, &Plan::of(m, it), q, &days(n), &mut out);
    out
}

#[test]
fn a_ten_day_assignment_follows_the_curve_day_by_day() {
    // Ten working days from Monday day 0 to Friday day 11 (4800 minutes):
    // 20 segments of 240 minutes, two a day. Days 0-4 hold segments of
    // weight 3: 6/40 x 80 = 12 a day; days 7-11 weight 1: 2/40 x 80 = 4.
    let m = model(&[], 0);
    let a = activity(at(0, 8), at(11, 17), 4800);
    let s = shaping(three_then_one());
    let it = Item {
        activity: &a,
        original: 4800,
        shaping: Some(&s),
    };
    let want = [12., 12., 12., 12., 12., 0., 0., 4., 4., 4., 4., 4.];
    close(&remaining(&m, &it, 80., 12), &want);
    let mut budget = vec![0.; 12];
    spread::budget(&m, &it, &Plan::of(&m, &it), 80., &days(12), &mut budget);
    close(&budget, &want);
    let mut late = vec![0.; 12];
    spread::remaining_late(&m, &it, &Plan::of(&m, &it), 80., &days(12), &mut late);
    close(&late, &want);
    // No curve: today's even spread, 8 a working day.
    let even = Item {
        shaping: None,
        ..it
    };
    close(
        &remaining(&m, &even, 80., 12),
        &[8., 8., 8., 8., 8., 0., 0., 8., 8., 8., 8., 8.],
    );
}

#[test]
fn a_thirteen_day_assignment_splits_a_segment_inside_a_day() {
    // Thirteen working days (6240 minutes; days 0-4, 7-11, 14-16), 104
    // units. The weight-3 half is the first 3120 minutes, six and a half
    // days: 480/3120 x 0.75 x 104 = 12 a full day. The seventh working day
    // (day 8) is half in each: 6 + 2 = 8. The last six: 480/3120 x 0.25 x
    // 104 = 4. Weekends carry nothing.
    let m = model(&[], 0);
    let a = activity(at(0, 8), at(16, 17), 6240);
    let s = shaping(three_then_one());
    let it = Item {
        activity: &a,
        original: 6240,
        shaping: Some(&s),
    };
    let want = [
        12., 12., 12., 12., 12., 0., 0., 12., 8., 4., 4., 4., 0., 0., 4., 4., 4.,
    ];
    close(&remaining(&m, &it, 104., 17), &want);
}

#[test]
fn a_holiday_carries_no_units_and_the_curve_moves_past_it() {
    // A holiday on Wednesday day 2: ten working days are days 0, 1, 3, 4,
    // 7, 8, 9, 10, 11, 14. The first five get 12, the last five 4.
    let m = model(&[2], 0);
    let a = activity(at(0, 8), at(14, 17), 4800);
    let s = shaping(three_then_one());
    let it = Item {
        activity: &a,
        original: 4800,
        shaping: Some(&s),
    };
    let want = [
        12., 12., 0., 12., 12., 0., 0., 12., 4., 4., 4., 4., 0., 0., 4.,
    ];
    close(&remaining(&m, &it, 80., 15), &want);
}

#[test]
fn every_preset_totals_exactly_the_budget() {
    // Each preset over the thirteen-day window, by day and by a coarse
    // split of uneven periods: the periods always add up to the budget.
    let m = model(&[2], 0);
    let a = activity(at(0, 8), at(17, 17), 6240);
    let uneven: Vec<(i64, i64)> = vec![
        (0, at(1, 10)),
        (at(1, 10), at(3, 15)),
        (at(3, 15), at(9, 0)),
        (at(9, 0), 30 * DAY),
    ];
    for p in curves::presets() {
        let s = shaping(Shape::from_points(&p.points).unwrap());
        let it = Item {
            activity: &a,
            original: 6240,
            shaping: Some(&s),
        };
        for periods in [days(20), uneven.clone()] {
            let mut out = vec![0.; periods.len()];
            spread::budget(&m, &it, &Plan::of(&m, &it), 123.45, &periods, &mut out);
            let total: f64 = out.iter().sum();
            assert!((total - 123.45).abs() < 1e-9, "{}: {total}", p.name);
            assert!(out.iter().all(|v| *v >= -1e-12), "{}: {out:?}", p.name);
        }
    }
    // Early Weighted puts more in the first working day than Late Weighted.
    let first = |name: &str| {
        let s = shaping(Shape::from_points(&curves::preset(name).unwrap().points).unwrap());
        let it = Item {
            activity: &a,
            original: 6240,
            shaping: Some(&s),
        };
        remaining(&m, &it, 100., 20)[0]
    };
    assert!(first("Early Weighted") > 3. * first("Late Weighted"));
}

#[test]
fn an_in_progress_assignment_splits_the_curve_at_the_data_date() {
    // Ten working days from day 0; data date Friday day 4 08:00. Done so
    // far: 1920 minutes; left: 2880 (days 4, 7-11), so the position is
    // 1920 / 4800 = 0.4, eight segments.
    let dd = at(4, 8);
    let m = model(&[], dd);
    let mut a = activity(at(0, 8), at(11, 17), 2880);
    a.status = StatusType::InProgress;
    a.act_start = Some(at(0, 8));
    a.early_start = Some(dd);
    let s = shaping(three_then_one());
    let it = Item {
        activity: &a,
        original: 4800,
        shaping: Some(&s),
    };
    assert!((spread::position(&m, &it) - 0.4).abs() < 1e-12);
    // Actual 30 along segments 1-8 (all weight 3): 7.5 a day, days 0-3.
    let mut actual = vec![0.; 12];
    spread::actual(&m, &it, &Plan::of(&m, &it), 30., &days(12), &mut actual);
    close(
        &actual,
        &[7.5, 7.5, 7.5, 7.5, 0., 0., 0., 0., 0., 0., 0., 0.],
    );
    // Remaining 50 along segments 9-20 (weight 6 + 10 = 16), rescaled:
    // day 4 holds segments 9 and 10, 6/16 x 50 = 18.75; days 7-11 two of
    // weight 1 each, 2/16 x 50 = 6.25.
    close(
        &remaining(&m, &it, 50., 12),
        &[0., 0., 0., 0., 18.75, 0., 0., 6.25, 6.25, 6.25, 6.25, 6.25],
    );
}

#[test]
fn the_start_value_counts_only_for_actuals() {
    // Point 0 is 10, the segments 3 then 1 (total 50). Before the start it
    // adds nothing: the budget is as without it.
    let mut points: [f64; POINTS] = std::array::from_fn(|k| if k <= 10 { 3. } else { 1. });
    points[0] = 10.;
    let s = shaping(Shape::from_points(&points).unwrap());
    let a = activity(at(0, 8), at(11, 17), 4800);
    let m = model(&[], 0);
    let it = Item {
        activity: &a,
        original: 4800,
        shaping: Some(&s),
    };
    let mut budget = vec![0.; 12];
    spread::budget(&m, &it, &Plan::of(&m, &it), 80., &days(12), &mut budget);
    close(
        &budget,
        &[12., 12., 12., 12., 12., 0., 0., 4., 4., 4., 4., 4.],
    );
    // In progress at position 0.4: actual weight 10 + 24 = 34. Actual 34:
    // day 0 has the start value and two segments, 10 + 6 = 16; days 1-3
    // two segments, 6.
    let dd = at(4, 8);
    let m = model(&[], dd);
    let mut a = a.clone();
    a.status = StatusType::InProgress;
    a.act_start = Some(at(0, 8));
    a.early_start = Some(dd);
    a.rem = 2880;
    let it = Item {
        activity: &a,
        original: 4800,
        shaping: Some(&s),
    };
    let mut actual = vec![0.; 12];
    spread::actual(&m, &it, &Plan::of(&m, &it), 34., &days(12), &mut actual);
    close(&actual, &[16., 6., 6., 6., 0., 0., 0., 0., 0., 0., 0., 0.]);
}

#[test]
fn resource_lag_starts_the_curve_late_and_own_length_ends_it_early() {
    // A ten-day activity; the assignment lags 16 hours (two days) and lasts
    // 48 hours (six days, 48 units at 1 an hour): days 2, 3, 4, 7, 8, 9.
    // Curve 3 then 1 over its own 2880 minutes: 12 a day for days 2-4, 4
    // for days 7-9; days 0, 1, 10 and 11 nothing.
    let m = model(&[], 0);
    let a = activity(at(0, 8), at(11, 17), 4800);
    let s = Shaping {
        budget_lag: 16 * H,
        remaining_lag: 16 * H,
        budget_minutes: Some(2880),
        remaining_minutes: Some(2880),
        ..shaping(three_then_one())
    };
    let it = Item {
        activity: &a,
        original: 4800,
        shaping: Some(&s),
    };
    let want = [0., 0., 12., 12., 12., 0., 0., 4., 4., 4., 0., 0.];
    close(&remaining(&m, &it, 48., 12), &want);
    let mut budget = vec![0.; 12];
    spread::budget(&m, &it, &Plan::of(&m, &it), 48., &days(12), &mut budget);
    close(&budget, &want);
    let w = spread::windows(&m, &it);
    assert_eq!(w.remaining, Some((at(2, 8), at(9, 17))));
}

#[test]
fn planned_share_follows_the_curve() {
    // At 40% of the ten-day window (day 4 08:00) the curve 3 then 1 has
    // planned 24/40 = 0.6 of the budget; an even spread 0.4.
    let m = model(&[], 0);
    let a = activity(at(0, 8), at(11, 17), 4800);
    let s = shaping(three_then_one());
    let curved = Item {
        activity: &a,
        original: 4800,
        shaping: Some(&s),
    };
    let even = Item {
        shaping: None,
        ..curved
    };
    let (start, finish) = (at(0, 8), at(11, 17));
    let share = |it: &Item, t| spread::planned_share(&m, it, start, finish, t);
    assert!((share(&curved, at(4, 8)) - 0.6).abs() < 1e-12);
    assert!((share(&even, at(4, 8)) - 0.4).abs() < 1e-12);
    assert_eq!(share(&curved, at(0, 0)), 0.);
    assert_eq!(share(&curved, at(20, 0)), 1.);
    // Halfway (day 7 08:00): 30/40.
    assert!((share(&curved, at(7, 8)) - 0.75).abs() < 1e-12);
}

#[test]
fn curves_apply_only_on_fixed_duration_types() {
    use openxer_xer::edit::records;
    let mut p = openxer_xer::starter();
    // A curve with a weight and one without any.
    let id = curves::ensure(&mut p.document, "Early Weighted").unwrap();
    p.document.table_mut("RSRC").add(&[
        ("rsrc_id", "5".into()),
        ("rsrc_short_name", "CREW".into()),
        ("rsrc_type", "RT_Labor".into()),
    ]);
    p.add_assignment(1010, Some(5), None).unwrap();
    let row = p.assignment_rows(1010)[0];
    let i = p
        .model
        .activities
        .iter()
        .position(|a| a.task_id == 1010)
        .unwrap();
    let task_row = p
        .document
        .table("TASK")
        .unwrap()
        .rows
        .iter()
        .position(|r| r[0] == "1010")
        .unwrap();
    // No curve yet.
    let none = Lookup::new(&p)
        .shaping(row, &p.model.activities[i])
        .unwrap();
    assert!(none.shape.is_none());
    records::set(&mut p.document, "TASKRSRC", row, "curv_id", id.to_string());
    for (code, applies) in [
        ("DT_FixedDUR2", true),
        ("DT_FixedDrtn", true),
        ("", true),
        ("DT_FixedQty", false),
        ("DT_FixedRate", false),
    ] {
        records::set(
            &mut p.document,
            "TASK",
            task_row,
            "duration_type",
            code.into(),
        );
        let got = Lookup::new(&p)
            .shaping(row, &p.model.activities[i])
            .unwrap();
        assert_eq!(got.shape.is_some(), applies, "{code}");
        assert_eq!(spread::curve_applies(code), applies, "{code}");
    }
    // A curve whose segments are all zero spreads evenly.
    let zero = [0.; POINTS];
    assert!(Shape::from_points(&zero).is_none());
}

#[test]
fn even_spreads_follow_the_assignment_lag_and_its_shorter_duration() {
    // No curve. Ten-day activity, 40 units; the assignment lags one day (8
    // hours) and lasts five days (40 units at 1 an hour): days 1-4 and 7 get
    // 8 each, nothing on day 0 or after day 7.
    let m = model(&[], 0);
    let a = activity(at(0, 8), at(11, 17), 4800);
    let s = Shaping {
        budget_lag: 8 * H,
        remaining_lag: 8 * H,
        budget_minutes: Some(2400),
        remaining_minutes: Some(2400),
        ..Shaping::plain()
    };
    let it = Item {
        activity: &a,
        original: 4800,
        shaping: Some(&s),
    };
    let want = [0., 8., 8., 8., 8., 0., 0., 8., 0., 0., 0., 0.];
    close(&remaining(&m, &it, 40., 12), &want);
    let mut budget = vec![0.; 12];
    spread::budget(&m, &it, &Plan::of(&m, &it), 40., &days(12), &mut budget);
    close(&budget, &want);
    // A shorter assignment without lag starts with the activity: days 0-4.
    let short = Shaping {
        remaining_minutes: Some(2400),
        ..Shaping::plain()
    };
    let it = Item {
        shaping: Some(&short),
        ..it
    };
    close(
        &remaining(&m, &it, 40., 12),
        &[8., 8., 8., 8., 8., 0., 0., 0., 0., 0., 0., 0.],
    );
    // Once the activity has started the lag no longer applies.
    let mut started = a.clone();
    started.status = StatusType::InProgress;
    started.act_start = Some(at(0, 8));
    let m = model(&[], at(0, 8));
    let it = Item {
        activity: &started,
        original: 4800,
        shaping: Some(&s),
    };
    assert_eq!(
        spread::windows(&m, &it).remaining,
        Some((at(0, 8), at(4, 17)))
    );
}

#[test]
fn a_resource_dependent_assignment_spreads_on_its_resource_calendar() {
    // The activity calendar works Monday to Friday; the resource calendar
    // (model calendar 1) works every day 08:00-12:00 only. 12 units over
    // three days of the resource calendar: 4 a day on days 4, 5, 6
    // (Friday to Sunday), none on the activity calendar's hours.
    let mut m = model(&[], 0);
    m.calendars.push(CalendarAdapter::new(WorkCalendar::new(
        "7x4".into(),
        false,
        0,
        std::array::from_fn(|_| vec![(8 * H, 12 * H)]),
        BTreeMap::new(),
    )));
    let mut a = activity(at(4, 8), at(6, 12), 720);
    a.task_type = openxer_engine::model::TaskType::Resource;
    let s = Shaping {
        cal: Some(1),
        ..Shaping::plain()
    };
    let it = Item {
        activity: &a,
        original: 720,
        shaping: Some(&s),
    };
    close(
        &remaining(&m, &it, 12., 8),
        &[0., 0., 0., 0., 4., 4., 4., 0.],
    );
    // On the activity calendar the weekend would carry nothing.
    let plain = Shaping::plain();
    let it = Item {
        shaping: Some(&plain),
        ..it
    };
    close(
        &remaining(&m, &it, 12., 8),
        &[0., 0., 0., 0., 12., 0., 0., 0.],
    );
}

#[test]
fn the_lookup_reads_the_resource_calendar_and_own_actual_dates() {
    use openxer_xer::edit::records;
    let mut p = openxer_xer::starter();
    p.document.table_mut("RSRC").add(&[
        ("rsrc_id", "5".into()),
        ("clndr_id", "1".into()),
        ("rsrc_short_name", "CREW".into()),
        ("rsrc_type", "RT_Labor".into()),
    ]);
    p.add_assignment(1010, Some(5), None).unwrap();
    let row = p.assignment_rows(1010)[0];
    let i = p
        .model
        .activities
        .iter()
        .position(|a| a.task_id == 1010)
        .unwrap();
    // A resource dependent activity's assignment works on its resource's
    // calendar (calendar 1, model index 0); a task's uses the activity's.
    assert_eq!(
        Lookup::new(&p)
            .shaping(row, &p.model.activities[i])
            .unwrap()
            .cal,
        None
    );
    p.model.activities[i].task_type = openxer_engine::model::TaskType::Resource;
    assert_eq!(
        Lookup::new(&p)
            .shaping(row, &p.model.activities[i])
            .unwrap()
            .cal,
        Some(0)
    );
    // Own actual dates count only when consistent with the activity.
    let origin = p.model.origin_serial;
    let dd = p.model.project.data_date.unwrap();
    let a = &mut p.model.activities[i];
    a.status = StatusType::InProgress;
    a.act_start = Some(dd - 2 * DAY);
    records::set(
        &mut p.document,
        "TASKRSRC",
        row,
        "act_start_date",
        openxer_xer::date_string(origin, Some(dd - DAY)),
    );
    let own = Lookup::new(&p)
        .shaping(row, &p.model.activities[i])
        .unwrap();
    assert_eq!(own.act_start, Some(dd - DAY));
    records::set(
        &mut p.document,
        "TASKRSRC",
        row,
        "act_start_date",
        openxer_xer::date_string(origin, Some(dd - 3 * DAY)),
    );
    let early = Lookup::new(&p)
        .shaping(row, &p.model.activities[i])
        .unwrap();
    assert_eq!(early.act_start, None, "before the activity's actual start");
}

#[test]
fn zero_working_time_puts_everything_in_the_start_period_with_or_without_a_curve() {
    // A window entirely in Saturday's non-work time: as the even spread
    // always has, all of it lands in the period holding the start.
    let m = model(&[], 0);
    let a = activity(at(5, 9), at(5, 15), 0);
    let curved = shaping(three_then_one());
    for s in [None, Some(&curved)] {
        let it = Item {
            activity: &a,
            original: 0,
            shaping: s,
        };
        close(&remaining(&m, &it, 6., 7), &[0., 0., 0., 0., 0., 6., 0.]);
    }
}

/// `clndr_data` of a calendar working every day 08:00-16:00.
fn seven_day_data() -> String {
    let week: String = (1..=7)
        .map(|d| format!("(0||{d}()((0||0(s|08:00|f|16:00)())))"))
        .collect();
    format!("(0||CalendarData()((0||DaysOfWeek()({week}))(0||Exceptions()())))")
}

/// The starter with resource 5 on a seven-day calendar (id 2) assigned to
/// 1010 with `fields`, 1010 made `task_type`, scheduled.
fn starter_with(task_type: &str, fields: &[(&str, &str)]) -> (openxer_xer::ProjectData, usize) {
    use openxer_xer::edit::records;
    let mut doc = openxer_xer::starter().document;
    doc.table_mut("CALENDAR").add(&[
        ("clndr_id", "2".into()),
        ("clndr_name", "Seven days".into()),
        ("clndr_data", seven_day_data()),
    ]);
    doc.table_mut("RSRC").add(&[
        ("rsrc_id", "5".into()),
        ("clndr_id", "2".into()),
        ("rsrc_short_name", "CREW".into()),
        ("rsrc_type", "RT_Labor".into()),
    ]);
    let mut row: Vec<(&str, String)> = vec![
        ("taskrsrc_id", "1".into()),
        ("task_id", "1010".into()),
        ("proj_id", "1".into()),
        ("rsrc_id", "5".into()),
        ("rollup_dates_flag", "Y".into()),
    ];
    row.extend(fields.iter().map(|(k, v)| (*k, v.to_string())));
    doc.table_mut("TASKRSRC").add(&row);
    records::set_task_value(&mut doc, 1010, 1, "task_type", task_type.into());
    let mut p = openxer_xer::ProjectData::from_document(doc, 1, "synthetic".into()).unwrap();
    openxer_engine::scheduler::schedule_with_resources(&mut p.model, &p.resources).unwrap();
    let i = p
        .model
        .activities
        .iter()
        .position(|a| a.task_id == 1010)
        .unwrap();
    (p, i)
}

fn day_periods_around(a: &Activity, n: i64) -> Vec<(i64, i64)> {
    let first = a.target_start.or(a.early_start).unwrap().div_euclid(DAY) * DAY - DAY;
    (0..n)
        .map(|d| (first + d * DAY, first + (d + 1) * DAY))
        .collect()
}

#[test]
fn a_completed_resource_dependent_assignment_keeps_its_resource_calendar() {
    // A resource dependent activity's assignment works on its resource's
    // seven-day calendar. When its remaining units reach zero the engine no
    // longer carries that calendar, but the spread must not change calendar:
    // the budget per day is the same before and after.
    use openxer_xer::edit::records;
    let (mut p, i) = starter_with(
        "TT_Rsrc",
        &[
            ("target_qty", "40"),
            ("target_qty_per_hr", "1"),
            ("remain_qty", "40"),
            ("remain_qty_per_hr", "1"),
        ],
    );
    let seven = p.model.calendars.iter().position(|c| c.id == "2").unwrap();
    let row = p.assignment_rows(1010)[0];
    let budget = |p: &openxer_xer::ProjectData| {
        let a = &p.model.activities[i];
        let s = Lookup::new(p).shaping(row, a).unwrap();
        let it = Item {
            activity: a,
            original: p.details[&1010].original_duration,
            shaping: Some(&s),
        };
        let periods = day_periods_around(a, 12);
        let mut out = vec![0.; periods.len()];
        spread::budget(
            &p.model,
            &it,
            &Plan::of(&p.model, &it),
            40.,
            &periods,
            &mut out,
        );
        (s.cal, out)
    };
    let (cal, before) = budget(&p);
    assert_eq!(cal, Some(seven));
    // Weekend days carry units on the seven-day calendar.
    assert_eq!(working_days(&before), 5, "{before:?}");
    records::set(&mut p.document, "TASKRSRC", row, "remain_qty", "0".into());
    p.rebuild_assignments();
    let (cal, after) = budget(&p);
    assert_eq!(cal, Some(seven), "completed: still the resource calendar");
    close(&after, &before);
    // A task dependent activity stays on its own calendar.
    let (p, i) = starter_with(
        "TT_Task",
        &[
            ("target_qty", "40"),
            ("target_qty_per_hr", "1"),
            ("remain_qty", "40"),
            ("remain_qty_per_hr", "1"),
        ],
    );
    let row = p.assignment_rows(1010)[0];
    let s = Lookup::new(&p)
        .shaping(row, &p.model.activities[i])
        .unwrap();
    assert_eq!(s.cal, None);
}

fn working_days(v: &[f64]) -> usize {
    v.iter().filter(|x| x.abs() > 1e-12).count()
}

#[test]
fn a_cost_without_units_spreads_over_the_activity() {
    // No budgeted units but a units/time and a budgeted cost of 10,000: the
    // cost spreads over the activity's working days, not into its first.
    let (p, i) = starter_with(
        "TT_Task",
        &[
            ("target_qty", "0"),
            ("target_qty_per_hr", "1"),
            ("target_cost", "10000"),
            ("remain_qty", "0"),
            ("remain_qty_per_hr", "1"),
            ("remain_cost", "10000"),
        ],
    );
    let row = p.assignment_rows(1010)[0];
    let a = &p.model.activities[i];
    let s = Lookup::new(&p).shaping(row, a).unwrap();
    let it = Item {
        activity: a,
        original: p.details[&1010].original_duration,
        shaping: Some(&s),
    };
    let even = Item {
        shaping: None,
        ..it
    };
    let periods = day_periods_around(a, 20);
    let spread_of = |it: &Item| {
        let mut b = vec![0.; periods.len()];
        let mut r = vec![0.; periods.len()];
        spread::budget(
            &p.model,
            it,
            &Plan::of(&p.model, it),
            10000.,
            &periods,
            &mut b,
        );
        spread::remaining(
            &p.model,
            it,
            &Plan::of(&p.model, it),
            10000.,
            &periods,
            &mut r,
        );
        (b, r)
    };
    let (budget, remaining) = spread_of(&it);
    let (want_budget, want_remaining) = spread_of(&even);
    assert!(working_days(&want_budget) > 1, "{want_budget:?}");
    close(&budget, &want_budget);
    close(&remaining, &want_remaining);
}

#[test]
fn day_week_and_month_totals_equal_the_stored_quantities() {
    // An in-progress curved assignment with a budget lag and its own
    // budgeted length, Early Weighted, with quantities that do not divide
    // evenly: whenever the periods cover a window, its periods add up to
    // the stored quantity exactly, by day, by week and by month.
    let dd = at(9, 10);
    let mut m = model(&[2, 15], dd);
    m.project.data_date = Some(dd);
    let mut a = activity(at(0, 8), at(25, 17), 0);
    a.status = StatusType::InProgress;
    a.act_start = Some(at(0, 9));
    a.early_start = Some(dd);
    a.late_start = Some(at(3, 8));
    a.late_finish = Some(at(30, 17));
    a.rem = 6000;
    let s = Shaping {
        budget_lag: 7 * H,
        remaining_lag: 5 * H,
        budget_minutes: Some(5555),
        remaining_minutes: Some(4321),
        ..shaping(Shape::from_points(&curves::preset("Early Weighted").unwrap().points).unwrap())
    };
    let it = Item {
        activity: &a,
        original: 9600,
        shaping: Some(&s),
    };
    let blocks = |len: &[i64]| -> Vec<(i64, i64)> {
        let mut out = vec![];
        let mut s = -3 * DAY;
        for k in 0..20 {
            let e = s + len[k % len.len()] * DAY;
            out.push((s, e));
            s = e;
        }
        out
    };
    for (name, periods) in [
        ("day", (-3..60).map(|d| (d * DAY, (d + 1) * DAY)).collect()),
        ("week", blocks(&[7])),
        ("month", blocks(&[31, 30, 31, 28])),
    ] {
        let periods: Vec<(i64, i64)> = periods;
        for (series, q) in [
            ("budget", 83.17),
            ("actual", 31.9),
            ("remaining", 51.27),
            ("late", 51.27),
        ] {
            let mut out = vec![0.; periods.len()];
            match series {
                "budget" => spread::budget(&m, &it, &Plan::of(&m, &it), q, &periods, &mut out),
                "actual" => spread::actual(&m, &it, &Plan::of(&m, &it), q, &periods, &mut out),
                "remaining" => {
                    spread::remaining(&m, &it, &Plan::of(&m, &it), q, &periods, &mut out)
                }
                _ => spread::remaining_late(&m, &it, &Plan::of(&m, &it), q, &periods, &mut out),
            }
            if name == "day" {
                assert!(working_days(&out) > 1, "{name} {series}: {out:?}");
            }
            let total: f64 = out.iter().sum();
            assert_eq!(total, q, "{name} {series}: {out:?}");
        }
    }
}
