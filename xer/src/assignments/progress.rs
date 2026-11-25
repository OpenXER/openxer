//! Progress by units on the activity itself.
//!
//! An activity with resource assignments takes its units from them (the
//! assignment edits in the parent module). An activity without an assignment
//! of a resource type carries its own units of that type in its `TASK` row:
//! work quantities for labor and equipment quantities for nonlabor, each as
//! budgeted (target), actual to date, remaining and actual this period. This
//! module edits those, follows the project's calculation options, and keeps
//! the `TASK` totals equal to the sum of the assignments after an assignment
//! edit, as export files carry them.
use super::*;

/// The three resource types an activity's units and costs are split by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceSlot {
    Labor,
    Nonlabor,
    Material,
}

impl ResourceSlot {
    pub fn caption(self) -> &'static str {
        match self {
            ResourceSlot::Labor => "labor",
            ResourceSlot::Nonlabor => "nonlabor",
            ResourceSlot::Material => "material",
        }
    }

    /// `TASK` columns of the activity's own units: budgeted, actual,
    /// remaining and actual this period. Material has none.
    fn task_columns(self) -> Option<[&'static str; 4]> {
        match self {
            ResourceSlot::Labor => Some([
                "target_work_qty",
                "act_work_qty",
                "remain_work_qty",
                "act_this_per_work_qty",
            ]),
            ResourceSlot::Nonlabor => Some([
                "target_equip_qty",
                "act_equip_qty",
                "remain_equip_qty",
                "act_this_per_equip_qty",
            ]),
            ResourceSlot::Material => None,
        }
    }
}

/// Which of the activity's own units a unit edit sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitsPhase {
    Budgeted,
    Actual,
    Remaining,
    ThisPeriod,
}

impl UnitsPhase {
    fn caption(self) -> &'static str {
        match self {
            UnitsPhase::Budgeted => "Budgeted units",
            UnitsPhase::Actual => "Actual units",
            UnitsPhase::Remaining => "Remaining units",
            UnitsPhase::ThisPeriod => "Actual units this period",
        }
    }
}

impl ProjectData {
    /// The resource type of a `TASKRSRC` row: its own column, else the
    /// resource's.
    fn row_slot(&self, t: &Table, r: &[String]) -> ResourceSlot {
        let mut kind = t.value(r, "rsrc_type").to_string();
        if kind.is_empty() {
            kind = lookup(
                &self.document,
                "RSRC",
                "rsrc_id",
                num(t.value(r, "rsrc_id")),
            )
            .map(|(rt, rr)| rt.value(rr, "rsrc_type").to_string())
            .unwrap_or_default();
        }
        match kind.as_str() {
            "RT_Equip" => ResourceSlot::Nonlabor,
            "RT_Mat" => ResourceSlot::Material,
            _ => ResourceSlot::Labor,
        }
    }

    /// The `TASKRSRC` rows of the activity whose resource is of this type.
    pub fn assignment_rows_of(&self, task_id: i64, slot: ResourceSlot) -> Vec<usize> {
        let Some(t) = self.document.table("TASKRSRC") else {
            return vec![];
        };
        self.assignment_rows(task_id)
            .into_iter()
            .filter(|&row| self.row_slot(t, &t.rows[row]) == slot)
            .collect()
    }

    /// A column of the open project's `PROJECT` row (the Calculations
    /// options are Y/N flags there).
    pub fn project_option(&self, column: &str) -> String {
        project_flag(&self.document, self.model.project.proj_id, column)
    }

    /// The activity's own units of this type from its `TASK` row:
    /// budgeted, actual, remaining and actual this period.
    pub fn activity_own_units(&self, task_id: i64, slot: ResourceSlot) -> [f64; 4] {
        let pid = self.model.project.proj_id;
        let Some(columns) = slot.task_columns() else {
            return [0.; 4];
        };
        columns.map(|c| float(&task_column(&self.document, pid, task_id, c)))
    }

    /// Set the units the activity carries itself (no assignment of this
    /// type), with the rules the assignment edits follow: budgeted and
    /// remaining move together while the activity has not started and the
    /// project links them; actual units need a started activity and, with
    /// "Subtract Actual from At Completion", take the change off the
    /// remaining units; actual this period follows the actual to date when
    /// the project links them. Returns whether anything changed; on `Err`
    /// nothing did.
    pub fn edit_activity_units(
        &mut self,
        i: usize,
        slot: ResourceSlot,
        phase: UnitsPhase,
        value: f64,
    ) -> Result<bool, String> {
        let Some(columns) = slot.task_columns() else {
            return Err("Material resources have no units on the activity itself".into());
        };
        let a = self.model.activities.get(i).ok_or("No such activity")?;
        let (task_id, completed) = (a.task_id, a.is_completed());
        let started = a.status != StatusType::NotStarted;
        let pid = self.model.project.proj_id;
        if !(0. ..1.0e12).contains(&value) {
            return Err(format!("{} must be zero or more", phase.caption()));
        }
        let now = self.activity_own_units(task_id, slot);
        let linked = self.linked(i);
        let subtract = project_flag(&self.document, pid, "add_act_remain_flag") == "N";
        let this_linked = project_flag(&self.document, pid, "act_this_per_link_flag") == "Y";
        let mut next = now;
        match phase {
            UnitsPhase::Budgeted => {
                next[0] = value;
                if linked {
                    next[2] = value;
                }
            }
            UnitsPhase::Remaining => {
                if completed && value > 0. {
                    return Err("A completed activity has no remaining units".into());
                }
                next[2] = value;
                if linked {
                    next[0] = value;
                }
            }
            UnitsPhase::Actual | UnitsPhase::ThisPeriod => {
                if !started {
                    return Err("Actual units need a started activity".into());
                }
                let actual = if phase == UnitsPhase::Actual {
                    value
                } else if this_linked {
                    let actual = now[1] + (value - now[3]);
                    if actual < -1e-9 {
                        return Err("The actual units cannot go below zero".into());
                    }
                    actual.max(0.)
                } else {
                    now[1]
                };
                let delta = actual - now[1];
                next[1] = actual;
                if subtract && !completed && delta.abs() > 1e-9 {
                    next[2] = (now[2] - delta).max(0.);
                }
                if phase == UnitsPhase::ThisPeriod && !this_linked {
                    next[3] = value;
                } else if this_linked {
                    next[3] = (now[3] + delta).max(0.);
                }
            }
        }
        let mut changed = false;
        for (n, column) in columns.iter().enumerate() {
            if (next[n] - now[n]).abs() > 1e-9 {
                set_task_column(
                    &mut self.document,
                    pid,
                    task_id,
                    column,
                    quantity_text(next[n]),
                );
                changed = true;
            }
        }
        Ok(changed)
    }

    /// Make the `TASK` totals of each resource type the activity has an
    /// assignment of equal to the sum of those assignments (budgeted,
    /// actual, remaining and actual this period). A type without an
    /// assignment keeps the activity's own units. A completed activity's
    /// remaining total is zero.
    pub fn sync_task_rollups(&mut self, task_id: i64) {
        let pid = self.model.project.proj_id;
        for slot in [ResourceSlot::Labor, ResourceSlot::Nonlabor] {
            let rows = self.assignment_rows_of(task_id, slot);
            let Some(columns) = slot.task_columns().filter(|_| !rows.is_empty()) else {
                continue;
            };
            let Some(t) = self.document.table("TASKRSRC") else {
                return;
            };
            let mut sum = [0.; 4];
            for &row in &rows {
                let r = &t.rows[row];
                let v = |k: &str| float(t.value(r, k));
                sum[0] += v("target_qty");
                sum[1] += v("act_reg_qty") + v("act_ot_qty");
                sum[2] += v("remain_qty");
                sum[3] += v("act_this_per_qty");
            }
            // A completed activity has no remaining units, even when an
            // assignment still carries some (the reference keeps those on the row).
            if self
                .model
                .activities
                .iter()
                .any(|a| a.task_id == task_id && a.is_completed())
            {
                sum[2] = 0.;
            }
            for (n, column) in columns.iter().enumerate() {
                let current = float(&task_column(&self.document, pid, task_id, column));
                if (current - sum[n]).abs() > 1e-9 {
                    set_task_column(
                        &mut self.document,
                        pid,
                        task_id,
                        column,
                        quantity_text(sum[n]),
                    );
                }
            }
        }
    }

    /// The project option "Recalculate actual units and cost when duration
    /// % complete changes" (`act_pct_link_flag`), as observed in the
    /// reference: after a Duration % edit every assignment of the activity
    /// has actual units of its budget times `percent`, actual cost at its
    /// price and its budget's rest remaining, spread over the activity's
    /// remaining duration. With "link actual to date and actual this
    /// period" the actual this period moves by the change. Returns whether
    /// a row changed.
    pub fn actuals_from_duration_percent(&mut self, task_id: i64, percent: f64) -> bool {
        let places = cost_places(&self.document);
        let this_period = self.project_option("act_this_per_link_flag") == "Y";
        let Ok(i) = self.activity_index(task_id) else {
            return false;
        };
        let hours = self.model.activities[i].rem as f64 / 60.;
        let mut changed = false;
        for row in self.assignment_rows(task_id) {
            let Some(view) = self.assignment_view(row) else {
                continue;
            };
            let t = self.document.table_mut("TASKRSRC");
            let linked = t.value(&t.rows[row], "cost_qty_link_flag") != "N";
            let actual = view.budgeted.units * percent / 100.;
            let regular = (actual - view.actual_overtime_units).max(0.);
            let left = (view.budgeted.units - actual).max(0.);
            let delta = actual - view.actual_units;
            let mut sets = vec![
                ("act_reg_qty", quantity_text(regular)),
                ("remain_qty", quantity_text(left)),
            ];
            if view.drives_dates && hours > 0. {
                sets.push(("remain_qty_per_hr", quantity_text(left / hours)));
            }
            if linked {
                sets.push(("act_reg_cost", cost_in(places, regular * view.price)));
            }
            if this_period && delta.abs() > 1e-9 {
                let units = (view.this_period_units + delta).max(0.);
                sets.push(("act_this_per_qty", quantity_text(units)));
                if linked {
                    let cost = (view.this_period_cost + delta * view.price).max(0.);
                    sets.push(("act_this_per_cost", cost_in(places, cost)));
                }
            }
            for (column, value) in sets {
                if t.value(&t.rows[row], column) != value {
                    t.set(row, column, value);
                    changed = true;
                }
            }
            self.update_costs(row);
        }
        if changed {
            self.sync_task_rollups(task_id);
            self.rebuild_assignments();
        }
        changed
    }

    /// Reset actual this period to zero on every activity and assignment of
    /// the open project (Store Period Performance). Returns how many rows
    /// changed.
    pub fn reset_actual_this_period(&mut self) -> usize {
        let pid = self.model.project.proj_id;
        reset_this_period(&mut self.document, pid)
    }
}

/// Zero the actual-this-period columns of the project's `TASK` and `TASKRSRC`
/// rows. Returns how many rows changed.
pub fn reset_this_period(doc: &mut Document, pid: i64) -> usize {
    let mut changed = 0;
    for (table, columns) in [
        (
            "TASK",
            &["act_this_per_work_qty", "act_this_per_equip_qty"][..],
        ),
        ("TASKRSRC", &["act_this_per_qty", "act_this_per_cost"][..]),
    ] {
        let Some(t) = doc
            .tables
            .iter_mut()
            .find(|t| t.name.eq_ignore_ascii_case(table))
        else {
            continue;
        };
        for row in 0..t.rows.len() {
            if num(t.value(&t.rows[row], "proj_id")) != pid {
                continue;
            }
            let mut touched = false;
            for column in columns {
                if t.field(column).is_some() && float(t.value(&t.rows[row], column)) != 0. {
                    t.set(
                        row,
                        column,
                        if table == "TASK" { "0" } else { "0.0000" }.into(),
                    );
                    touched = true;
                }
            }
            changed += usize::from(touched);
        }
    }
    changed
}
