//! OpenXER's own starter reports: scheduling reports written for this
//! project, stored as definition files under `assets/openxer_reports/` and
//! loaded into every new workspace.
use super::store;
use super::stream::Stream;
use std::sync::OnceLock;

pub struct Entry {
    /// The file name without its extension; stable across versions.
    pub id: &'static str,
    pub name: String,
    pub text: &'static str,
}

const FILES: [(&str, &str); 13] = [
    (
        "activities-by-status",
        include_str!("../../../../assets/openxer_reports/activities-by-status.rpt"),
    ),
    (
        "assignments-by-resource",
        include_str!("../../../../assets/openxer_reports/assignments-by-resource.rpt"),
    ),
    (
        "baseline-variance",
        include_str!("../../../../assets/openxer_reports/baseline-variance.rpt"),
    ),
    (
        "critical-activities",
        include_str!("../../../../assets/openxer_reports/critical-activities.rpt"),
    ),
    (
        "critical-and-near-critical",
        include_str!("../../../../assets/openxer_reports/critical-and-near-critical.rpt"),
    ),
    (
        "lookahead-2-week-by-wbs",
        include_str!("../../../../assets/openxer_reports/lookahead-2-week-by-wbs.rpt"),
    ),
    (
        "lookahead-3-week-by-wbs",
        include_str!("../../../../assets/openxer_reports/lookahead-3-week-by-wbs.rpt"),
    ),
    (
        "milestones",
        include_str!("../../../../assets/openxer_reports/milestones.rpt"),
    ),
    (
        "open-ends-and-logic-gaps",
        include_str!("../../../../assets/openxer_reports/open-ends-and-logic-gaps.rpt"),
    ),
    (
        "predecessors-and-successors",
        include_str!("../../../../assets/openxer_reports/predecessors-and-successors.rpt"),
    ),
    (
        "progress-this-period",
        include_str!("../../../../assets/openxer_reports/progress-this-period.rpt"),
    ),
    (
        "resource-usage-by-week",
        include_str!("../../../../assets/openxer_reports/resource-usage-by-week.rpt"),
    ),
    (
        "wbs-summary",
        include_str!("../../../../assets/openxer_reports/wbs-summary.rpt"),
    ),
];

pub fn entries() -> &'static [Entry] {
    static LIST: OnceLock<Vec<Entry>> = OnceLock::new();
    LIST.get_or_init(|| {
        FILES
            .iter()
            .map(|(id, text)| Entry {
                id,
                name: Stream::parse(text)
                    .map(|s| store::name_of(&s))
                    .unwrap_or_else(|_| (*id).to_string()),
                text,
            })
            .collect()
    })
}
