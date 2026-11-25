//! The constraint marker on shown Start and Finish dates. A Start or Finish
//! that is not an actual carries a trailing ` *` when the activity's primary
//! or secondary constraint applies to that end:
//! the start-type constraints mark Start, the finish-type ones mark Finish.
//! As Late As Possible has no date and marks neither. The marker is display
//! only; typed and pasted text parses with or without it. The rule lives in
//! the shared field catalog (`openxer_xer::edit::fields`), which reads Start and
//! Finish with their markers for the command line too.
use crate::format::{self, DateMark};
use openxer_engine::model::Activity;
pub use openxer_xer::edit::fields::{constraints_on, finish_value, mark, start_value, End};
#[cfg(test)]
use {super::columns::Value, openxer_engine::model::CstrType};

/// Hover text for a marked Start or Finish cell, one line per constraint on
/// that end, e.g. `Constraint: Start On or After 06-Apr-26 08:00`. `None`
/// when the shown date carries no constraint marker.
pub fn tooltip(origin: i64, a: &Activity, end: End) -> Option<String> {
    if mark(a, end) != DateMark::Constrained {
        return None;
    }
    let lines: Vec<String> = constraints_on(a, end)
        .into_iter()
        .map(|(secondary, c, d)| {
            format!(
                "{}: {} {}",
                if secondary {
                    "Secondary constraint"
                } else {
                    "Constraint"
                },
                openxer_xer::constraint_label(c),
                format::date(origin, Some(d))
            )
        })
        .collect();
    Some(lines.join("\n"))
}

/// The column header hover text for Start and Finish.
pub const HEADER_HINT: &str = "A = actual date, * = constrained date";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::columns::{self, Kind};

    const ALL: [CstrType; 10] = [
        CstrType::None,
        CstrType::StartOn,
        CstrType::StartOnBefore,
        CstrType::StartOnAfter,
        CstrType::Alap,
        CstrType::FinishOn,
        CstrType::FinishOnBefore,
        CstrType::FinishOnAfter,
        CstrType::MandatoryStart,
        CstrType::MandatoryFinish,
    ];

    fn act(primary: CstrType, secondary: CstrType) -> Activity {
        Activity {
            early_start: Some(1_000),
            early_finish: Some(5_000),
            cstr: primary,
            cstr_date: (primary != CstrType::None).then_some(1_000),
            cstr2: secondary,
            cstr_date2: (secondary != CstrType::None).then_some(5_000),
            ..Default::default()
        }
    }

    fn expected(c: CstrType) -> Option<End> {
        match c {
            CstrType::StartOn
            | CstrType::StartOnBefore
            | CstrType::StartOnAfter
            | CstrType::MandatoryStart => Some(End::Start),
            CstrType::FinishOn
            | CstrType::FinishOnBefore
            | CstrType::FinishOnAfter
            | CstrType::MandatoryFinish => Some(End::Finish),
            CstrType::None | CstrType::Alap => None,
        }
    }

    fn marks(a: &Activity) -> (DateMark, DateMark) {
        (mark(a, End::Start), mark(a, End::Finish))
    }

    #[test]
    fn each_primary_constraint_marks_its_own_end() {
        for c in ALL {
            let a = act(c, CstrType::None);
            let want = |end| {
                if expected(c) == Some(end) {
                    DateMark::Constrained
                } else {
                    DateMark::Plain
                }
            };
            assert_eq!(marks(&a), (want(End::Start), want(End::Finish)), "{c:?}");
        }
    }

    #[test]
    fn each_secondary_constraint_marks_its_own_end() {
        for c in ALL {
            let a = act(CstrType::None, c);
            let want = |end| {
                if expected(c) == Some(end) {
                    DateMark::Constrained
                } else {
                    DateMark::Plain
                }
            };
            assert_eq!(marks(&a), (want(End::Start), want(End::Finish)), "{c:?}");
        }
    }

    #[test]
    fn primary_and_secondary_mark_both_ends() {
        let a = act(CstrType::StartOnAfter, CstrType::FinishOnBefore);
        assert_eq!(marks(&a), (DateMark::Constrained, DateMark::Constrained));
        let a = act(CstrType::Alap, CstrType::MandatoryFinish);
        assert_eq!(marks(&a), (DateMark::Plain, DateMark::Constrained));
    }

    #[test]
    fn actual_dates_keep_a_and_never_the_constraint_mark() {
        let mut a = act(CstrType::StartOnAfter, CstrType::FinishOnBefore);
        a.act_start = Some(900);
        assert_eq!(marks(&a), (DateMark::Actual, DateMark::Constrained));
        a.act_finish = Some(4_000);
        assert_eq!(marks(&a), (DateMark::Actual, DateMark::Actual));
        assert_eq!(tooltip(0, &a, End::Start), None);
        assert_eq!(tooltip(0, &a, End::Finish), None);
    }

    #[test]
    fn a_dated_constraint_without_a_date_does_not_mark() {
        let mut a = act(CstrType::StartOn, CstrType::None);
        a.cstr_date = None;
        assert_eq!(marks(&a), (DateMark::Plain, DateMark::Plain));
    }

    #[test]
    fn alap_marks_nothing() {
        let mut a = act(CstrType::Alap, CstrType::None);
        a.cstr_date = Some(1_000);
        assert_eq!(marks(&a), (DateMark::Plain, DateMark::Plain));
        assert_eq!(tooltip(0, &a, End::Start), None);
    }

    #[test]
    fn start_and_finish_values_carry_the_mark() {
        let a = act(CstrType::MandatoryStart, CstrType::None);
        assert_eq!(
            start_value(&a, false),
            Value::Date {
                minute: 1_000,
                mark: DateMark::Constrained
            }
        );
        assert_eq!(
            finish_value(&a, false),
            Value::Date {
                minute: 5_000,
                mark: DateMark::Plain
            }
        );
        assert_eq!(start_value(&Activity::default(), false), Value::Empty);
    }

    #[test]
    fn tooltip_names_the_constraints_on_that_end() {
        let origin = 46_000;
        let mut a = act(CstrType::StartOnAfter, CstrType::StartOnBefore);
        a.cstr_date = Some(8 * 60);
        a.cstr_date2 = Some(3 * 24 * 60 + 17 * 60);
        let tip = tooltip(origin, &a, End::Start).expect("marked");
        let lines: Vec<&str> = tip.lines().collect();
        assert_eq!(lines.len(), 2, "{tip}");
        assert_eq!(
            lines[0],
            format!(
                "Constraint: Start On or After {}",
                format::date(origin, Some(8 * 60))
            )
        );
        assert!(lines[1].starts_with("Secondary constraint: Start On or Before "));
        assert_eq!(tooltip(origin, &a, End::Finish), None);
    }

    #[test]
    fn marked_text_round_trips_through_parse() {
        let p = openxer_xer::starter();
        let origin = p.model.origin_serial;
        for mark in [DateMark::Plain, DateMark::Actual, DateMark::Constrained] {
            let v = Value::Date {
                minute: 2 * 24 * 60 + 8 * 60,
                mark,
            };
            let text = columns::display(&p, None, &v);
            match mark {
                DateMark::Plain => {
                    assert!(!text.ends_with(['A', '*', format::OLD_CONSTRAINT_MARK]))
                }
                DateMark::Actual => {
                    assert!(text.ends_with(" A"), "{text}");
                    assert!(!text.contains('*'), "{text}");
                }
                DateMark::Constrained => {
                    assert!(text.ends_with(" *"), "{text}");
                    assert!(!text.ends_with(" A"), "{text}");
                    assert!(!text.contains(format::OLD_CONSTRAINT_MARK), "{text}");
                }
            }
            let back = columns::parse_input(&p, None, Kind::Date, &text).expect(&text);
            assert_eq!(
                back,
                Value::Date {
                    minute: 2 * 24 * 60 + 8 * 60,
                    mark: DateMark::Plain
                },
                "{text}"
            );
            let bare = format::date(origin, Some(2 * 24 * 60 + 8 * 60));
            for typed in [
                format!("{bare} \u{2021}"),
                format!("{bare}\u{2021}"),
                format!("{bare}*"),
                format!("{bare} *"),
                format!("{bare} A"),
                bare.clone(),
            ] {
                assert_eq!(
                    format::parse_date(origin, &typed, None),
                    Ok(Some(2 * 24 * 60 + 8 * 60)),
                    "{typed}"
                );
            }
        }
    }

    #[test]
    fn strip_date_marker_handles_every_marker() {
        for (typed, bare) in [
            ("06-Apr-26 08:00 \u{2021}", "06-Apr-26 08:00"),
            ("06-Apr-26 08:00\u{2021}", "06-Apr-26 08:00"),
            ("06-Apr-26 08:00 *", "06-Apr-26 08:00"),
            ("06-Apr-26 08:00*", "06-Apr-26 08:00"),
            ("06-Apr-26 08:00 A", "06-Apr-26 08:00"),
            ("  06-Apr-26  ", "06-Apr-26"),
            ("06-Apr-26 08:00 AM", "06-Apr-26 08:00 AM"),
        ] {
            assert_eq!(format::strip_date_marker(typed), bare, "{typed}");
        }
    }

    #[test]
    fn the_constraint_mark_is_an_asterisk_after_a_space() {
        assert_eq!(format::CONSTRAINT_MARK, '*');
        let p = openxer_xer::starter();
        let origin = p.model.origin_serial;
        let bare = format::date(origin, Some(8 * 60));
        assert_eq!(
            format::date_marked(origin, Some(8 * 60), DateMark::Constrained),
            format!("{bare} *")
        );
        assert_eq!(
            format::date_marked(origin, Some(8 * 60), DateMark::Actual),
            format!("{bare} A")
        );
        assert_eq!(HEADER_HINT, "A = actual date, * = constrained date");
    }

    #[test]
    fn the_pdf_encodes_the_constraint_mark() {
        assert_eq!(crate::print::pdf::win_ansi("1 *"), vec![b'1', b' ', b'*']);
    }

    #[test]
    fn the_pdf_encodes_the_old_double_dagger() {
        assert_eq!(
            crate::print::pdf::win_ansi("1 \u{2021}"),
            vec![b'1', b' ', 0x87]
        );
    }
}
