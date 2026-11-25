use openxer_engine::{
    calendar::{CalendarAdapter, WorkCalendar, DAY},
    fixture::{self, Case},
    model::{Activity, Assignment, CstrType, Link, LinkType, Model, ProjectSettings, TaskType},
    options::ScheduleOptions,
    scheduler,
};
use std::collections::BTreeMap;

fn model() -> Model {
    Model {
        origin_serial: 45000,
        project_calendar: 0,
        calendars: vec![CalendarAdapter::new(WorkCalendar::new(
            "work".into(),
            false,
            45000,
            std::array::from_fn(|i| if i < 5 { vec![(480, 960)] } else { vec![] }),
            BTreeMap::new(),
        ))],
        activities: vec![Activity {
            task_id: 1,
            task_code: "A".into(),
            rem: 480,
            ..Activity::default()
        }],
        links: vec![],
        project: ProjectSettings {
            data_date: Some(480),
            ..ProjectSettings::default()
        },
        options: ScheduleOptions::default(),
    }
}

fn calendar_fixture(shifts: &str, operation: &str) -> String {
    let mut text = "cal c 0 45000\n".to_owned();
    for day in 0..7 {
        text.push_str(&format!("wk c {day} {shifts}\n"));
    }
    text.push_str(operation);
    text.push('\n');
    text
}

fn model_fixture() -> String {
    fixture::write_model(&model(), "valid")
}

#[test]
fn empty_project_is_a_safe_noop() {
    let mut m = model();
    m.activities.clear();
    m.calendars.clear();
    m.project.data_date = None;
    let before = m.clone();
    scheduler::schedule(&mut m).unwrap();
    assert_eq!(m, before);
}

#[test]
fn empty_project_cannot_hide_dangling_relationships() {
    let mut m = model();
    m.activities.clear();
    m.links.push(Link {
        pred: 0,
        succ: 0,
        link_type: LinkType::FS,
        lag: 0,
        driving_path: false,
    });
    let before = m.clone();
    assert!(scheduler::schedule(&mut m).is_err());
    assert_eq!(m, before);
}

#[test]
fn malformed_normalized_periods_are_rejected_before_scheduling() {
    for shift in [
        (480, 480),
        (960, 480),
        (-1, 480),
        (1440, 1500),
        (0, -1),
        (0, i64::MAX),
    ] {
        let mut m = model();
        m.calendars[0].cal.template[0] = vec![shift];
        let before = m.clone();
        assert!(scheduler::schedule(&mut m).is_err(), "accepted {shift:?}");
        assert_eq!(m, before);
    }
}

#[test]
fn malformed_exception_is_checked_even_outside_current_schedule_dates() {
    let mut m = model();
    m.calendars[0]
        .cal
        .exc
        .insert(45000 + 5000, vec![(900, 600)]);
    assert!(m.validate().unwrap_err().contains("exception"));
}

#[test]
fn project_calendar_also_needs_recurring_work() {
    let mut m = model();
    m.calendars.push(m.calendars[0].clone());
    m.activities[0].cal = 1;
    m.calendars[0].cal.template = std::array::from_fn(|_| vec![]);
    assert!(m.validate().unwrap_err().contains("recurring work"));
}

#[test]
fn overnight_shifts_remain_valid_and_keep_fixture_dates() {
    let mut m = model();
    m.calendars[0] = CalendarAdapter::new(WorkCalendar::new(
        "overnight".into(),
        false,
        45000,
        std::array::from_fn(|_| vec![(1200, 120)]),
        BTreeMap::new(),
    ));
    m.project.data_date = Some(1200);
    m.activities[0].rem = 360;
    scheduler::schedule(&mut m).unwrap();
    assert_eq!(
        (m.activities[0].early_start, m.activities[0].early_finish),
        (Some(1200), Some(1560))
    );
}

#[test]
fn validation_preserves_overlapping_and_unsorted_iteration_order() {
    let mut m = model();
    let shifts = vec![(780, 1020), (480, 900)];
    m.calendars[0].cal.template[0] = shifts.clone();
    m.validate().unwrap();
    assert_eq!(m.calendars[0].cal.template[0], shifts);
}

#[test]
fn negative_minutes_and_long_durations_have_no_arbitrary_horizon_cap() {
    let mut m = model();
    m.project.data_date = Some(-DAY);
    m.activities[0].rem = 1000 * 365 * DAY;
    m.validate().unwrap();
}

/// A negative remaining duration would put the early finish before the early
/// start (FWD-1, REL-11 count the duration forward from the start). It is
/// refused before scheduling and the model is left as it was.
#[test]
fn a_negative_remaining_duration_is_refused() {
    let mut m = model();
    m.activities[0].rem = -480;
    let error = m.validate().unwrap_err();
    assert!(error.contains("negative remaining duration"), "{error}");
    assert!(error.contains('A'), "{error}");
    let before = m.clone();
    assert!(scheduler::schedule(&mut m).is_err());
    assert_eq!(m, before);
    // Zero is fine.
    m.activities[0].rem = 0;
    scheduler::schedule(&mut m).unwrap();
}

/// CAL-10: work positions are counted from the origin day without walking
/// every day, so a Start On or After constraint late in the year 9999 (the
/// last year an XER date can carry) schedules at once instead of hanging.
#[test]
fn a_constraint_in_the_year_9999_schedules_promptly() {
    let mut m = model();
    // 2958464 is 30 Dec 9999 counted from 30 Dec 1899; the origin is 45000.
    let constraint = (2_958_464 - 45000) * DAY + 480;
    m.activities.push(Activity {
        task_id: 2,
        task_code: "B".into(),
        rem: 480,
        cstr: CstrType::StartOnAfter,
        cstr_date: Some(constraint),
        ..Activity::default()
    });
    m.links.push(Link {
        pred: 0,
        succ: 1,
        link_type: LinkType::FS,
        lag: 0,
        driving_path: false,
    });
    m.rebuild_links().unwrap();
    let started = std::time::Instant::now();
    scheduler::schedule(&mut m).unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(60));
    let b = &m.activities[1];
    let (start, finish) = (b.early_start.unwrap(), b.early_finish.unwrap());
    assert!(
        start >= constraint && start < constraint + 3 * DAY,
        "{start}"
    );
    assert_eq!((start.rem_euclid(DAY), finish - start), (480, 480));
    assert_eq!(m.activities[0].early_start, Some(480));
}

/// A date outside the years 1000 to 9999 cannot come from an XER file and is
/// refused before scheduling, leaving the model as it was.
#[test]
fn dates_outside_the_years_1000_to_9999_are_refused() {
    for minute in [(2_958_466 - 45000) * DAY, (-328_717 - 45000) * DAY + 480] {
        let mut m = model();
        m.activities[0].cstr = CstrType::StartOnAfter;
        m.activities[0].cstr_date = Some(minute);
        let error = m.validate().unwrap_err();
        assert!(error.contains("1000 to 9999"), "{error}");
        let before = m.clone();
        assert!(scheduler::schedule(&mut m).is_err());
        assert_eq!(m, before);
        let mut m = model();
        m.project.scd_end = Some(minute);
        assert!(m.validate().unwrap_err().contains("1000 to 9999"));
    }
    let mut m = model();
    m.activities[0].cstr_date = Some((2_958_465 - 45000) * DAY + DAY - 1);
    m.validate().unwrap();
}

#[test]
fn calendar_origin_must_match_shared_model_origin() {
    let mut m = model();
    m.calendars[0].cal.origin_serial += 1;
    assert!(m.validate().unwrap_err().contains("origin"));
}

#[test]
fn rebuild_failure_preserves_all_existing_adjacency() {
    let mut m = model();
    m.activities.push(Activity {
        task_id: 2,
        task_code: "B".into(),
        ..Activity::default()
    });
    m.activities[0].preds = vec![77];
    m.activities[0].succs = vec![88];
    m.links.push(Link {
        pred: 0,
        succ: 1,
        link_type: LinkType::FS,
        lag: 0,
        driving_path: false,
    });
    m.links.push(Link {
        pred: 1,
        succ: 9,
        link_type: LinkType::FS,
        lag: 0,
        driving_path: false,
    });
    let before = m.clone();
    assert!(m.rebuild_links().is_err());
    assert_eq!(m, before);
}

#[test]
fn missing_date_and_cycle_errors_preserve_user_model() {
    let mut no_date = model();
    no_date.project.data_date = None;
    no_date.activities[0].preds = vec![42];
    let before = no_date.clone();
    assert!(scheduler::schedule(&mut no_date).is_err());
    assert_eq!(no_date, before);

    let mut cyclic = model();
    cyclic.activities.push(Activity {
        task_id: 2,
        task_code: "B".into(),
        ..Activity::default()
    });
    cyclic.links = vec![
        Link {
            pred: 0,
            succ: 1,
            link_type: LinkType::FS,
            lag: 0,
            driving_path: false,
        },
        Link {
            pred: 1,
            succ: 0,
            link_type: LinkType::SS,
            lag: 0,
            driving_path: false,
        },
    ];
    let before = cyclic.clone();
    assert!(scheduler::schedule(&mut cyclic)
        .unwrap_err()
        .contains("circular"));
    assert_eq!(cyclic, before);
}

#[test]
fn a_loop_is_named_by_its_activity_ids() {
    let mut m = model();
    m.activities[0].task_code = "A".into();
    for (id, code) in [(2, "B"), (3, "C"), (4, "D")] {
        m.activities.push(Activity {
            task_id: id,
            task_code: code.into(),
            ..Activity::default()
        });
    }
    let link = |pred, succ| Link {
        pred,
        succ,
        link_type: LinkType::FS,
        lag: 0,
        driving_path: false,
    };
    // A -> B -> C -> B is the loop; D hangs off it and is not part of it.
    m.links = vec![link(0, 1), link(1, 2), link(2, 1), link(2, 3)];
    let err = scheduler::schedule(&mut m).unwrap_err();
    assert!(
        err.ends_with(": B -> C -> B") || err.ends_with(": C -> B -> C"),
        "{err}"
    );
}

#[test]
fn late_feature_failure_does_not_publish_partial_schedule() {
    let mut m = model();
    m.activities[0].early_start = Some(123);
    m.activities[0].succs = vec![88];
    m.activities.push(Activity {
        task_id: 2,
        task_code: "LOE".into(),
        task_type: TaskType::Loe,
        ..Activity::default()
    });
    m.options.multiple_float_paths = true;
    m.options.float_paths_end_activity = Some(2); // Exists, but cannot be a dated CPM endpoint.
    let before = m.clone();
    assert!(scheduler::schedule(&mut m).is_err());
    assert_eq!(m, before);
}

#[test]
fn numeric_boundaries_and_assignment_duration_overflow_are_errors() {
    for minute in [i64::MIN, i64::MAX, (i64::MIN.div_euclid(DAY) + 1) * DAY] {
        let mut m = model();
        m.project.data_date = Some(minute);
        assert!(m.validate().is_err());
    }
    let mut m = model();
    m.activities[0].assignments.push(Assignment {
        rem: i64::MAX,
        relag: 1,
        ..Assignment::default()
    });
    assert!(m.validate().unwrap_err().contains("overflows"));
}

#[test]
fn raw_fixture_periods_are_checked_before_constructor_normalization() {
    for shift in [
        "9223372036854775807:9223372036854775807",
        "480:-9223372036854775808",
        "1440:0",
        "0:3000",
    ] {
        assert!(fixture::parse(&calendar_fixture(shift, "op work_at c 0 = 0")).is_err());
    }
    let (calendars, cases) =
        fixture::parse(&calendar_fixture("1200:120", "op work_at c 0 = 0")).unwrap();
    assert_eq!(calendars[0].cal.template[0], vec![(1200, 1560)]);
    assert_eq!(fixture::run(&calendars, &cases).unwrap().exact, 1);
}

#[test]
fn closed_calendar_has_bounded_operations_but_rejects_unbounded_search() {
    let (calendars, cases) =
        fixture::parse(&calendar_fixture("-", "op next_work_time c 0 1000 = 1000")).unwrap();
    assert_eq!(fixture::run(&calendars, &cases).unwrap().exact, 1);
    let (_, cases) = fixture::parse(&calendar_fixture("-", "op snap_start c 0 = 0")).unwrap();
    assert!(fixture::run(&calendars, &cases)
        .unwrap_err()
        .contains("recurring work"));
}

/// Rule CAL-1 in `engine/RULES.md`: an overnight shift's part after midnight
/// is working time of the next day, so a finish request that runs past it
/// continues into the next night (it used to be refused as unable to end).
#[test]
fn overnight_finish_request_runs_on_past_midnight() {
    // 20:00 + 540 minutes: 240 to midnight, 120 to 02:00, 180 from 20:00.
    let (calendars, cases) = fixture::parse(&calendar_fixture(
        "1200:120",
        "op finish_date c 1200 540 = 2820",
    ))
    .unwrap();
    assert_eq!(fixture::run(&calendars, &cases).unwrap().exact, 1);
    let (_, cases) = fixture::parse(&calendar_fixture(
        "1200:120",
        "op finish_date c 1200 200 = 1400",
    ))
    .unwrap();
    assert_eq!(fixture::run(&calendars, &cases).unwrap().exact, 1);
}

#[test]
fn programmatically_constructed_cases_cannot_panic_on_missing_fields() {
    let calendars = model().calendars;
    for case in [
        Case {
            line: 1,
            operation: "add".into(),
            calendar: 0,
            args: vec![],
            expected: 0,
        },
        Case {
            line: 1,
            operation: "work_at".into(),
            calendar: 99,
            args: vec![0],
            expected: 0,
        },
        Case {
            line: 1,
            operation: "unknown".into(),
            calendar: 0,
            args: vec![0],
            expected: 0,
        },
        Case {
            line: 1,
            operation: "add".into(),
            calendar: 0,
            args: vec![0, i64::MIN],
            expected: 0,
        },
    ] {
        assert!(fixture::run(&calendars, &[case]).is_err());
    }
    assert!(fixture::run(&calendars, &[]).is_err());
}

#[test]
fn model_parser_rejects_duplicate_headers_options_and_invalid_booleans() {
    let base = model_fixture();
    for extra in [
        "origin 45000\n",
        "opt retained_logic=1\n",
        "assign 1 4 2 60\n",
        "assign 1 4 1 60 0 0 3\n",
        "expect A\n",
    ] {
        assert!(
            fixture::parse_model(&(base.clone() + extra)).is_err(),
            "accepted {extra}"
        );
    }
    assert!(fixture::parse_model(
        &base.replace("opt retained_logic=", "opt do_level=0 retained_logic=")
    )
    .is_err());
}

#[test]
fn model_parser_rejects_unsafe_calendar_before_any_schedule() {
    let base = model_fixture();
    let closed = base.replace("480:960", "-");
    assert!(fixture::parse_model(&closed)
        .unwrap_err()
        .contains("recurring work"));
    assert!(fixture::parse_model(&base.replace("45000\nprojcal", "45001\nprojcal")).is_err());
}
