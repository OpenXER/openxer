//! The project's own definition of a critical activity, shared by the
//! activity table, filters, Gantt bars, reports and the spreadsheet export.
//!
//! The PROJECT row holds the definition: `critical_path_type` is
//! `CT_DrivPath` for the longest path, otherwise total float at or below
//! `critical_drtn_hr_cnt` hours. Completed activities are never critical.
use crate::ProjectData;
use openxer_engine::model::Activity;

/// A project's critical-activity definition: (longest path, total float
/// threshold in hours).
pub type CriticalDefinition = (bool, f64);

/// The critical-activity definition stored on `p`'s PROJECT row. A project
/// with no row, or blank values, uses total float at or below zero.
pub fn critical_definition(p: &ProjectData) -> CriticalDefinition {
    let pid = p.model.project.proj_id.to_string();
    let value = |key: &str| {
        p.document
            .table("PROJECT")
            .and_then(|t| {
                t.rows
                    .iter()
                    .find(|r| t.value(r, "proj_id") == pid)
                    .map(|r| t.value(r, key).trim().to_string())
            })
            .unwrap_or_default()
    };
    let longest = value("critical_path_type") == "CT_DrivPath";
    let hours = value("critical_drtn_hr_cnt").parse::<f64>().unwrap_or(0.);
    (longest, hours)
}

/// True when `a` is critical by `definition`; completed activities never are.
pub fn is_critical(a: &Activity, definition: CriticalDefinition) -> bool {
    if a.is_completed() {
        return false;
    }
    if definition.0 {
        a.driving_path
    } else {
        a.total_float
            .is_some_and(|f| f as f64 <= definition.1 * 60. + 1e-6)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openxer_engine::model::StatusType;

    fn set_project(p: &mut ProjectData, key: &str, value: &str) {
        let pid = p.model.project.proj_id.to_string();
        let t = p.document.table_mut("PROJECT");
        let row = t
            .rows
            .iter()
            .position(|r| t.value(r, "proj_id") == pid)
            .expect("the starter has a PROJECT row");
        t.set(row, key, value.into());
    }

    fn activity(float_hours: i64, longest: bool) -> Activity {
        Activity {
            total_float: Some(float_hours * 60),
            driving_path: longest,
            ..Default::default()
        }
    }

    #[test]
    fn blank_definition_is_zero_float() {
        let mut p = crate::starter();
        set_project(&mut p, "critical_path_type", "");
        set_project(&mut p, "critical_drtn_hr_cnt", "");
        let d = critical_definition(&p);
        assert_eq!(d, (false, 0.));
        assert!(is_critical(&activity(0, false), d));
        assert!(is_critical(&activity(-8, false), d));
        assert!(!is_critical(&activity(1, false), d));
        assert!(
            !is_critical(&Activity::default(), d),
            "no float, not critical"
        );
    }

    #[test]
    fn longest_path_ignores_zero_float_off_the_path() {
        let mut p = crate::starter();
        set_project(&mut p, "critical_path_type", "CT_DrivPath");
        let d = critical_definition(&p);
        assert!(d.0);
        assert!(!is_critical(&activity(0, false), d));
        assert!(is_critical(&activity(40, true), d));
    }

    #[test]
    fn float_threshold_takes_in_activities_at_or_below_it() {
        let mut p = crate::starter();
        set_project(&mut p, "critical_path_type", "CT_TotFloat");
        set_project(&mut p, "critical_drtn_hr_cnt", "16");
        let d = critical_definition(&p);
        assert_eq!(d, (false, 16.));
        assert!(is_critical(&activity(8, false), d));
        assert!(is_critical(&activity(16, false), d));
        assert!(!is_critical(&activity(17, false), d));
    }

    #[test]
    fn completed_activities_are_never_critical() {
        let mut a = activity(-8, true);
        a.status = StatusType::Completed;
        assert!(!is_critical(&a, (false, 0.)));
        assert!(!is_critical(&a, (true, 0.)));
    }
}
