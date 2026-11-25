//! Undo restores every field the shared edit layer can write on an activity.
//!
//! An undo step keeps the scheduling model, the activity details, the WBS,
//! the calendar names and a `RecordSnapshot` of the record tables
//! (`records::DETAIL_TABLES`) plus the raw `TASK` columns listed in
//! `records::TASK_COLUMNS`. A `TASK` column the edit layer writes that is in
//! none of these is not restored by Undo. This test walks every
//! `activity::Edit` variant (the match in `variant` fails to compile when a
//! new one is added), applies it to a small invented project, restores the
//! snapshot the way the app's Undo does and compares the whole exported file
//! with the one before the edit. Every table and column that differs is
//! named in the failure.
use openxer_engine::model::{CstrType, StatusType, TaskType};
use openxer_xer::edit::activity::{self as act, Edit, Measure, Phase};
use openxer_xer::edit::records::{self, RecordSnapshot};
use openxer_xer::{starter, Document, ProjectData};
use std::collections::BTreeSet;

const CREW: i64 = 77;

/// The starter project with one labor resource in the file.
fn project() -> ProjectData {
    let mut p = starter();
    p.document.table_mut("RSRC").add(&[
        ("rsrc_id", CREW.to_string()),
        ("rsrc_name", "Pipe crew".into()),
        ("rsrc_short_name", "PIPE".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("def_qty_per_hr", "1".into()),
        ("clndr_id", "1".into()),
    ]);
    p.document.table_mut("RSRCRATE").add(&[
        ("rsrc_id", CREW.to_string()),
        ("start_date", "2020-01-01 00:00".into()),
        ("cost_per_qty", "1".into()),
    ]);
    // Actual This Period follows Actual (the project option), so the
    // this-period figures can be edited too.
    p.document
        .table_mut("PROJECT")
        .set(0, "act_this_per_link_flag", "Y".into());
    p
}

fn index(p: &ProjectData, task_id: i64) -> usize {
    p.model
        .activities
        .iter()
        .position(|a| a.task_id == task_id)
        .unwrap()
}

/// The variant's name. Exhaustive on purpose: a new edit does not compile
/// until it is named here, and the test then asks for a case (or a
/// `SNAPSHOT_TABLES` entry) for it.
fn variant(e: &Edit) -> &'static str {
    match e {
        Edit::ActivityType(_) => "ActivityType",
        Edit::Status(_) => "Status",
        Edit::Wbs(_) => "Wbs",
        Edit::Calendar(_) => "Calendar",
        Edit::PrimaryResource(_) => "PrimaryResource",
        Edit::DurationType(_) => "DurationType",
        Edit::PercentType(_) => "PercentType",
        Edit::Priority(_) => "Priority",
        Edit::AutoComputeActuals(_) => "AutoComputeActuals",
        Edit::Start(_) => "Start",
        Edit::StartUnheld(_) => "StartUnheld",
        Edit::Finish(_) => "Finish",
        Edit::ActualStart(_) => "ActualStart",
        Edit::ActualFinish(_) => "ActualFinish",
        Edit::PlannedStart(_) => "PlannedStart",
        Edit::PlannedFinish(_) => "PlannedFinish",
        Edit::ExpectedFinish(_) => "ExpectedFinish",
        Edit::SuspendDate(_) => "SuspendDate",
        Edit::ResumeDate(_) => "ResumeDate",
        Edit::OriginalDuration(_) => "OriginalDuration",
        Edit::ActualDuration(_) => "ActualDuration",
        Edit::RemainingDuration(_) => "RemainingDuration",
        Edit::AtCompletionDuration(_) => "AtCompletionDuration",
        Edit::PhysicalPercent(_) => "PhysicalPercent",
        Edit::ActivityPercent(_) => "ActivityPercent",
        Edit::DurationPercent(_) => "DurationPercent",
        Edit::Constraint { .. } => "Constraint",
        Edit::ConstraintDate { .. } => "ConstraintDate",
        Edit::Code { .. } => "Code",
        Edit::Udf { .. } => "Udf",
        Edit::MoveStart(_) => "MoveStart",
        Edit::Figure { .. } => "Figure",
        Edit::ThisPeriod { .. } => "ThisPeriod",
        Edit::BarDuration(_) => "BarDuration",
    }
}

/// Edits whose rows live wholly in a snapshotted record table, with that
/// table: Undo restores the whole table, so no column list is involved.
const SNAPSHOT_TABLES: [(&str, &str); 2] = [("Code", "TASKACTV"), ("Udf", "UDFVALUE")];

/// What the app's Undo keeps for one step.
struct Step {
    model: openxer_engine::model::Model,
    details: std::collections::BTreeMap<i64, openxer_xer::ActivityDetails>,
    wbs: Vec<openxer_xer::Wbs>,
    records: RecordSnapshot,
    calendar_names: Vec<String>,
}

impl Step {
    fn take(p: &ProjectData) -> Self {
        Self {
            model: p.model.clone(),
            details: p.details.clone(),
            wbs: p.wbs.clone(),
            records: RecordSnapshot::capture(&p.document),
            calendar_names: p.calendar_names.clone(),
        }
    }
    fn undo(&self, p: &mut ProjectData) {
        p.model = self.model.clone();
        p.details = self.details.clone();
        p.wbs = self.wbs.clone();
        self.records.restore(&mut p.document);
        p.calendar_names = self.calendar_names.clone();
    }
}

/// `TABLE.column` cells that differ between two exported files; a column
/// missing from a table reads as blank.
fn differences(before: &Document, after: &Document) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let names: BTreeSet<String> = before
        .tables
        .iter()
        .chain(&after.tables)
        .map(|t| t.name.to_uppercase())
        .collect();
    for name in names {
        let (Some(a), Some(b)) = (before.table(&name), after.table(&name)) else {
            out.insert(format!("{name} (table)"));
            continue;
        };
        if a.rows.len() != b.rows.len() {
            out.insert(format!("{name} (rows)"));
            continue;
        }
        let fields: BTreeSet<String> = a
            .fields
            .iter()
            .chain(&b.fields)
            .map(|f| f.to_lowercase())
            .collect();
        for (ra, rb) in a.rows.iter().zip(&b.rows) {
            for f in &fields {
                if a.value(ra, f) != b.value(rb, f) {
                    out.insert(format!("{name}.{f}"));
                }
            }
        }
    }
    out
}

/// One case: `setup` runs before the undo step is taken, then `edit`.
struct Case {
    task_id: i64,
    setup: Vec<Edit>,
    edit: Edit,
    /// The edit may be refused for this activity (a locked figure).
    may_refuse: bool,
}

fn case(task_id: i64, setup: Vec<Edit>, edit: Edit) -> Case {
    Case {
        task_id,
        setup,
        edit,
        may_refuse: false,
    }
}

fn cases(p: &ProjectData) -> Vec<Case> {
    // RB-1020: a three-day task; RB-1000: the start milestone.
    let task = 1020;
    let i = index(p, task);
    let s = act::default_actual_start(p, i);
    let day = 1440;
    let started = || vec![Edit::Status(StatusType::InProgress)];
    let mut out = vec![
        case(task, vec![], Edit::ActivityType(TaskType::Resource)),
        case(task, vec![], Edit::Status(StatusType::InProgress)),
        case(task, vec![], Edit::Status(StatusType::Completed)),
        case(task, vec![], Edit::Wbs(40)),
        case(task, vec![], Edit::Calendar(p.model.activities[i].cal)),
        case(1000, vec![], Edit::PrimaryResource(CREW)),
        case(task, vec![], Edit::DurationType("DT_FixedQty")),
        case(task, vec![], Edit::PercentType("CP_Phys")),
        case(task, vec![], Edit::Priority("PT_High")),
        case(task, vec![], Edit::AutoComputeActuals(true)),
        case(task, vec![], Edit::Start(s + day)),
        case(task, vec![], Edit::StartUnheld(s + day)),
        case(task, vec![], Edit::Finish(s + 6 * day)),
        case(task, vec![], Edit::ActualStart(Some(s))),
        case(task, started(), Edit::ActualFinish(Some(s + 2 * day))),
        case(task, vec![], Edit::PlannedStart(s + day)),
        case(task, vec![], Edit::PlannedFinish(s + 6 * day)),
        case(task, started(), Edit::ExpectedFinish(Some(s + 9 * day))),
        case(task, started(), Edit::SuspendDate(Some(s + day))),
        case(
            task,
            vec![
                Edit::Status(StatusType::InProgress),
                Edit::SuspendDate(Some(s + day)),
            ],
            Edit::ResumeDate(Some(s + 3 * day)),
        ),
        case(task, vec![], Edit::OriginalDuration(40 * 60)),
        case(
            task,
            vec![Edit::Status(StatusType::Completed)],
            Edit::ActualDuration(8 * 60),
        ),
        case(task, started(), Edit::RemainingDuration(16 * 60)),
        case(task, started(), Edit::AtCompletionDuration(32 * 60)),
        case(
            task,
            vec![
                Edit::PercentType("CP_Phys"),
                Edit::Status(StatusType::InProgress),
            ],
            Edit::PhysicalPercent(50.),
        ),
        case(task, started(), Edit::ActivityPercent(50.)),
        case(task, started(), Edit::DurationPercent(50.)),
        case(
            task,
            vec![],
            Edit::Constraint {
                secondary: false,
                kind: CstrType::StartOnAfter,
            },
        ),
        case(
            task,
            vec![Edit::Constraint {
                secondary: false,
                kind: CstrType::StartOnAfter,
            }],
            Edit::ConstraintDate {
                secondary: false,
                date: Some(s + 2 * day),
            },
        ),
        case(task, vec![], Edit::MoveStart(s + day)),
        case(task, vec![], Edit::BarDuration(16 * 60)),
    ];
    // Every figure the activity's own quantities take, not started and
    // started; a locked one may be refused.
    for measure in Measure::ALL.into_iter().filter(|m| m.editable()) {
        for phase in Phase::ALL {
            for setup in [vec![], started()] {
                out.push(Case {
                    task_id: task,
                    setup,
                    edit: Edit::Figure {
                        measure,
                        phase,
                        value: 24.,
                    },
                    may_refuse: true,
                });
            }
        }
        out.push(Case {
            task_id: task,
            setup: started(),
            edit: Edit::ThisPeriod { measure, value: 4. },
            may_refuse: true,
        });
    }
    out
}

#[test]
fn undo_restores_every_field_an_activity_edit_writes() {
    let base = project();
    let mut gaps: BTreeSet<String> = BTreeSet::new();
    let mut by_edit: Vec<String> = vec![];
    let mut refused: Vec<String> = vec![];
    let mut applied: BTreeSet<&'static str> = BTreeSet::new();
    for c in cases(&base) {
        let mut p = base.clone();
        let i = index(&p, c.task_id);
        let setup: Result<Vec<_>, String> = c
            .setup
            .iter()
            .map(|e| act::edit_activity(&mut p, i, e.clone()))
            .collect();
        if let Err(e) = setup {
            refused.push(format!("setup of {:?}: {e}", c.edit));
            continue;
        }
        let before = p.updated_document();
        let step = Step::take(&p);
        match act::edit_activity(&mut p, i, c.edit.clone()) {
            Ok(_) => {
                applied.insert(variant(&c.edit));
            }
            Err(_) if c.may_refuse => continue,
            Err(e) => {
                refused.push(format!("{:?}: {e}", c.edit));
                continue;
            }
        }
        step.undo(&mut p);
        let missed = differences(&before, &p.updated_document());
        if !missed.is_empty() {
            by_edit.push(format!("{:?}: {missed:?}", c.edit));
            gaps.extend(missed);
        }
    }
    let mut problems = vec![];
    for (name, table) in SNAPSHOT_TABLES {
        if !records::DETAIL_TABLES.contains(&table) {
            problems.push(format!(
                "{name} edits write {table}, which Undo must keep whole"
            ));
        }
    }
    // The arms of `variant`: every one is applied above or kept whole.
    const VARIANTS: usize = 34;
    if applied.len() + SNAPSHOT_TABLES.len() != VARIANTS {
        problems.push(format!(
            "not every edit was checked (add a case for a new one); applied {applied:?}"
        ));
    }
    if !refused.is_empty() {
        problems.push(format!("edits refused: {refused:#?}"));
    }
    if !gaps.is_empty() {
        problems.push(format!(
            "Undo does not restore {gaps:?}; by edit:\n{}",
            by_edit.join("\n")
        ));
    }
    let report = problems.join("\n\n");
    if !problems.is_empty() {
        // Also kept in the temp folder: a test runner that shows only the
        // first line of a failure still leaves the full list to read.
        let _ = std::fs::write(
            std::env::temp_dir().join("openxer-undo-columns.txt"),
            &report,
        );
    }
    assert!(problems.is_empty(), "{report}");
}
