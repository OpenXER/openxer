//! Deleting activities: the scheduling model and every XER row that refers
//! to them, so an export carries no orphaned assignments, codes or notes.
use super::*;
use std::collections::HashSet;

/// Tables whose rows belong to one activity through `task_id`.
pub const ACTIVITY_TABLES: [&str; 12] = [
    "TASKRSRC", "TASKACTV", "TASKMEMO", "TASKPROC", "PROJCOST", "TASKNOTE", "TASKDOC", "TASKFDBK",
    "TASKRISK", "TASKUSER", "TASKFIN", "TRSRCFIN",
];

/// Remove the rows that belong to the assignments `taskrsrc_ids`: their
/// assignment codes (ASGNMNTACAT), stored period actuals (TRSRCFIN) and
/// values of assignment user defined fields (UDFVALUE). The TASKRSRC rows
/// themselves are left to the caller. Returns the rows removed.
pub fn delete_assignment_children(doc: &mut Document, taskrsrc_ids: &HashSet<i64>) -> usize {
    if taskrsrc_ids.is_empty() {
        return 0;
    }
    let id = |t: &Table, r: &[String], k: &str| num(t.value(r, k).trim());
    let mut removed = 0;
    for table in ["ASGNMNTACAT", "TRSRCFIN"] {
        removed += retain_rows(doc, table, |t, r| {
            !taskrsrc_ids.contains(&id(t, r, "taskrsrc_id"))
        });
    }
    let udfs = udf_types_of(doc, "TASKRSRC");
    if !udfs.is_empty() {
        removed += retain_rows(doc, "UDFVALUE", |t, r| {
            !(udfs.contains(&id(t, r, "udf_type_id")) && taskrsrc_ids.contains(&id(t, r, "fk_id")))
        });
    }
    removed
}

/// UDFTYPE ids of user defined fields on `table`.
fn udf_types_of(doc: &Document, table: &str) -> HashSet<i64> {
    doc.table("UDFTYPE")
        .map(|t| {
            t.rows
                .iter()
                .filter(|r| t.value(r, "table_name").eq_ignore_ascii_case(table))
                .map(|r| num(t.value(r, "udf_type_id").trim()))
                .collect()
        })
        .unwrap_or_default()
}

fn retain_rows(doc: &mut Document, table: &str, keep: impl Fn(&Table, &[String]) -> bool) -> usize {
    let Some(t) = doc
        .tables
        .iter_mut()
        .find(|t| t.name.eq_ignore_ascii_case(table))
    else {
        return 0;
    };
    let before = t.rows.len();
    let kept: Vec<Vec<String>> = t.rows.iter().filter(|r| keep(t, r)).cloned().collect();
    t.rows = kept;
    before - t.rows.len()
}

/// Remove activities of project `pid` from `doc`: their TASK rows,
/// relationships in both directions (also from other projects, since activity
/// IDs are unique across a database), every row in `ACTIVITY_TABLES`, and
/// UDFVALUE rows of activity user defined fields. Returns the rows removed.
pub fn delete_activity_rows(doc: &mut Document, pid: i64, task_ids: &[i64]) -> usize {
    if task_ids.is_empty() {
        return 0;
    }
    let ids: HashSet<i64> = task_ids.iter().copied().collect();
    let id = |t: &Table, r: &[String], k: &str| num(t.value(r, k).trim());
    let mut removed = retain_rows(doc, "TASK", |t, r| {
        !(id(t, r, "proj_id") == pid && ids.contains(&id(t, r, "task_id")))
    });
    // Assignment codes, period actuals and user field values follow the
    // assignments they belong to.
    let assignments: HashSet<i64> = doc
        .table("TASKRSRC")
        .map(|t| {
            t.rows
                .iter()
                .filter(|r| ids.contains(&id(t, r, "task_id")))
                .map(|r| id(t, r, "taskrsrc_id"))
                .collect()
        })
        .unwrap_or_default();
    removed += delete_assignment_children(doc, &assignments);
    removed += retain_rows(doc, "TASKPRED", |t, r| {
        !ids.contains(&id(t, r, "task_id")) && !ids.contains(&id(t, r, "pred_task_id"))
    });
    for table in ACTIVITY_TABLES {
        removed += retain_rows(doc, table, |t, r| !ids.contains(&id(t, r, "task_id")));
    }
    let task_udfs = udf_types_of(doc, "TASK");
    removed += retain_rows(doc, "UDFVALUE", |t, r| {
        !(task_udfs.contains(&id(t, r, "udf_type_id")) && ids.contains(&id(t, r, "fk_id")))
    });
    // The project's schedule options no longer end multiple float paths
    // with a deleted activity.
    if let Some(t) = doc
        .tables
        .iter_mut()
        .find(|t| t.name.eq_ignore_ascii_case("SCHEDOPTIONS"))
    {
        let key = "key_activity_for_multiple_longest_paths";
        if t.field(key).is_some() {
            for row in 0..t.rows.len() {
                let r = &t.rows[row];
                if id(t, r, "proj_id") == pid && ids.contains(&num(t.value(r, key).trim())) {
                    t.set(row, key, String::new());
                }
            }
        }
    }
    // An issue keeps its row but no longer points at the deleted activity,
    // so a later activity that reuses the ID does not inherit it.
    if let Some(t) = doc
        .tables
        .iter_mut()
        .find(|t| t.name.eq_ignore_ascii_case("PROJISSU"))
    {
        if t.field("task_id").is_some() {
            for row in 0..t.rows.len() {
                if ids.contains(&num(t.value(&t.rows[row], "task_id").trim())) {
                    t.set(row, "task_id", String::new());
                }
            }
        }
    }
    removed
}

impl ProjectData {
    /// Remove activities from the scheduling model only: activities, their
    /// relationships (indices renumbered) and details.
    pub fn remove_model_activities(&mut self, task_ids: &[i64]) -> Result<(), String> {
        let gone: HashSet<i64> = task_ids.iter().copied().collect();
        let old = std::mem::take(&mut self.model.activities);
        let mut remap = vec![None; old.len()];
        for (i, a) in old.into_iter().enumerate() {
            if gone.contains(&a.task_id) {
                self.details.remove(&a.task_id);
            } else {
                remap[i] = Some(self.model.activities.len());
                self.model.activities.push(a);
            }
        }
        let links = std::mem::take(&mut self.model.links);
        self.model.links = links
            .into_iter()
            .filter_map(|mut l| {
                l.pred = remap.get(l.pred).copied().flatten()?;
                l.succ = remap.get(l.succ).copied().flatten()?;
                Some(l)
            })
            .collect();
        // Multiple float paths cannot end with an activity that is gone (the
        // engine refuses it); the option becomes blank in the same edit.
        let options = &mut self.model.options;
        if options
            .float_paths_end_activity
            .is_some_and(|id| gone.contains(&id))
        {
            options.float_paths_end_activity = None;
        }
        self.model.rebuild_links()
    }

    /// Delete activities and everything that refers to them, in the model and
    /// in the retained document, then rebuild the assignment inputs. Returns
    /// the number of document rows removed.
    pub fn delete_activities(&mut self, task_ids: &[i64]) -> Result<usize, String> {
        self.remove_model_activities(task_ids)?;
        let pid = self.model.project.proj_id;
        let removed = delete_activity_rows(&mut self.document, pid, task_ids);
        self.rebuild_assignments();
        Ok(removed)
    }
}
