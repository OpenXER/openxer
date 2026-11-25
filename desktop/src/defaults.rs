//! Out-of-the-box values: what a new workspace, a new set of user
//! preferences and a new layout start with. They follow the reference
//! scheduler's first launch: dates without a time, durations without a unit
//! label, its default Activities columns, activity icons on, the Activity
//! Details tabs General, Status, Resources, Predecessors and Successors, a
//! Quarter / Month timescale, landscape pages, the Activity Details grid
//! columns below, the smaller defaults (Hide if empty off, Show bar when
//! collapsed off), and the record views' columns (Project Expenses
//! ungrouped).
//!
//! Defaults only fill in where nothing was stored. A stored preference or a
//! saved layout is read exactly as stored, and a stored record that lacks a
//! newer key reads it as the value it had before these defaults.
//!
//! Tests start on the earlier defaults (the legacy defaults), so a test
//! that is not about the defaults keeps its expectations; a test about them
//! calls `use_reference` first. Each test runs on its own thread, and the
//! switch is per thread, so the choice never leaks into another test.

#[cfg(test)]
#[path = "defaults_tests.rs"]
mod tests;

#[cfg(test)]
thread_local! {
    static REFERENCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the reference defaults apply. Always outside tests.
pub fn reference() -> bool {
    #[cfg(test)]
    {
        REFERENCE.with(std::cell::Cell::get)
    }
    #[cfg(not(test))]
    {
        true
    }
}

/// Tests about the defaults: this thread uses the reference defaults from
/// here on. Call it before the app or any value is built.
#[cfg(test)]
pub fn use_reference() {
    REFERENCE.with(|r| r.set(true));
}

/// Paste of copied activities asks for their new Activity IDs (the Renumber
/// Activity IDs step after Copy Activity Options), as the reference does.
/// The earlier default gave free IDs without asking.
pub fn renumber_on_paste() -> bool {
    reference()
}

/// The Predecessors and Successors tabs' columns out of the box (the
/// Relationships tab keeps Project ID, WBS and Comments).
pub const LINK_TAB_COLUMNS: &[&str] = &[
    "Activity ID",
    "Activity Name",
    "Relationship Type",
    "Lag",
    "Activity Status",
    "Primary Resource",
];

/// The WBS table's columns out of the box, as the reference's Columns
/// chooser lists them selected (also the earlier default).
pub const WBS_COLUMNS: &[&str] = &["WBS Code", "WBS Name", "Total Activities"];

/// The Projects table's columns out of the box: the reference's four
/// narrow columns, leaving the width to the chart.
pub const PROJECT_COLUMNS: &[&str] = &[
    "Project ID",
    "Project Name",
    "Total Activities",
    "Strategic Priority",
];

/// The positions in `titles` of the default columns `wanted`.
pub fn positions(titles: &[&str], wanted: &[&str]) -> Vec<usize> {
    wanted
        .iter()
        .filter_map(|w| titles.iter().position(|t| t == w))
        .collect()
}

/// The Resource Assignments window's columns out of the box, by field id
/// (`views::assignment_columns`): the reference's "Activity Resource
/// Assignments" layout shows these in its Columns dialog.
pub const ASSIGNMENT_COLUMNS: &[&str] = &[
    "activity_id",
    "activity_name",
    "resource_id_name",
    "start",
    "finish",
];
/// The Steps tab's columns out of the box.
pub const STEP_COLUMNS: &[&str] = &["Step Name", "Completed"];

/// The Expenses tab's columns out of the box.
pub const EXPENSE_COLUMNS: &[&str] = &[
    "Expense Item",
    "Expense Category",
    "Accrual Type",
    "Budgeted Units",
    "Price / Unit",
    "Budgeted Cost",
    "Actual Cost",
    "Remaining Cost",
    "Vendor",
    "Auto Compute Actuals",
];

/// The record views' first-launch columns, by view, as column titles in
/// order: the reference opens Project Expenses, WPs & Docs, Risks, Project
/// Issues and Project Thresholds with these. A title the view does not
/// have is skipped (the reference's Exposure Start and Exposure Finish of
/// Risks have no OpenXER column yet).
pub const RECORD_COLUMNS: [(&str, &[&str]); 5] = [
    (
        "expenses",
        &[
            "Expense Item",
            "Expense Category",
            "WBS",
            "Cost Account",
            "Vendor",
        ],
    ),
    (
        "documents",
        &["Title", "Reference No.", "Status", "Document Category"],
    ),
    (
        "risks",
        &[
            "Risk ID",
            "Risk Name",
            "Risk Category",
            "Risk Type",
            "Risk Status",
            "Risk Owner",
            "Identified By",
            "Identified On",
            "Exposure Start",
            "Exposure Finish",
            "Project",
        ],
    ),
    (
        "issues",
        &[
            "Issue",
            "WBS",
            "Status",
            "Priority",
            "Activity ID",
            "Activity Name",
        ],
    ),
    (
        "thresholds",
        &["Threshold Parameter", "WBS", "Status", "Priority"],
    ),
];

/// The columns record view `id` starts with, as positions in its `titles`:
/// the reference's set ([`RECORD_COLUMNS`]) under the reference defaults,
/// otherwise every column in title order (the earlier default).
pub fn record_columns(id: &str, titles: &[&str]) -> Vec<usize> {
    let every = || (0..titles.len()).collect();
    if !reference() {
        return every();
    }
    let Some((_, wanted)) = RECORD_COLUMNS.iter().find(|(view, _)| *view == id) else {
        return every();
    };
    let order: Vec<usize> = wanted
        .iter()
        .filter_map(|w| titles.iter().position(|t| t == w))
        .collect();
    if order.is_empty() {
        every()
    } else {
        order
    }
}

/// Whether the Project Expenses view groups its items by WBS at first:
/// the reference lists them ungrouped; the earlier default grouped by WBS.
pub fn expenses_grouped() -> bool {
    !reference()
}

/// Whether Recalculate Assignment Costs opens with "Synchronize Overtime
/// Factor" ticked: the reference ticks it; the earlier default left it
/// clear.
pub fn recalc_sync_overtime() -> bool {
    reference()
}

/// The usage profiles' series out of the box, as (By Date, Cumulative) for
/// Budgeted, Actual, Remaining Early and Remaining Late, then the activity
/// profile's project and primary baseline. The reference's Resource Usage
/// Profile Options were seen with bars and curves for budgeted, actual and
/// remaining early and nothing for remaining late; its activity profile's
/// were not seen and start the same. Baselines start off.
pub fn profile_series(resource: bool) -> [(bool, bool); 6] {
    if reference() {
        let on = (true, true);
        let off = (false, false);
        [on, on, on, off, off, off]
    } else {
        earlier_profile_series(resource)
    }
}

/// What the profiles drew before their series could be switched: the
/// activity profile every series as bars, the resource profile actual and
/// remaining early; no curves until Display > Cumulative.
pub fn earlier_profile_series(resource: bool) -> [(bool, bool); 6] {
    let bars = (true, false);
    let off = (false, false);
    if resource {
        [off, bars, bars, off, off, off]
    } else {
        [bars, bars, bars, bars, off, off]
    }
}
