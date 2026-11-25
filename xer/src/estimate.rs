//! Top-down estimation: a labor-unit total distributed down the WBS by
//! estimation weights and written to activities' budgeted and remaining
//! labor units.
//!
//! Weights live where XER keeps them: `PROJWBS.est_wt` for WBS elements and
//! `TASK.est_wt` for activities (blank reads as 1, the value new rows get).
//!
//! Distribution, from the chosen WBS element (or the project node) down: at
//! each element the amount is shared among its child elements that hold
//! receiving activities, each by its WBS weight, and its own receiving
//! activities, each by its activity weight. Shares are proportional to the
//! weights; when every weight at an element is zero the shares are equal, so
//! no units are lost. Receiving activities are Task Dependent and Resource
//! Dependent ones (optionally not started only).
//!
//! Writing: an activity with labor assignments spreads its units over them
//! in proportion to their budgeted units (equally when those are zero), at
//! units/time that keep each assignment's duration, so the schedule does not
//! change; costs follow. An activity without labor assignments takes the
//! units on the activity itself. `TASK.target_work_qty` and
//! `remain_work_qty` hold the activity totals either way. Remaining units
//! are the budget less actual units, not below zero.
use super::assignments::{float, quantity_text, set_task_column, task_column};
use super::*;

/// Weight of a weight cell: blank reads as 1, negative as 0.
fn weight(text: &str) -> f64 {
    if text.trim().is_empty() {
        1.
    } else {
        float(text).max(0.)
    }
}

/// Shares of `amount` by `weights`, equal when they sum to zero.
fn shares(amount: f64, weights: &[f64]) -> Vec<f64> {
    let sum: f64 = weights.iter().sum();
    if weights.is_empty() {
        vec![]
    } else if sum > 0. {
        weights.iter().map(|w| amount * w / sum).collect()
    } else {
        vec![amount / weights.len() as f64; weights.len()]
    }
}

impl ProjectData {
    fn projwbs_row(&self, wbs_id: i64) -> Option<usize> {
        let pid = self.model.project.proj_id;
        let t = self.document.table("PROJWBS")?;
        t.rows
            .iter()
            .position(|r| num(t.value(r, "wbs_id")) == wbs_id && num(t.value(r, "proj_id")) == pid)
    }

    /// A WBS element's estimation weight (`PROJWBS.est_wt`).
    pub fn wbs_estimation_weight(&self, wbs_id: i64) -> f64 {
        let Some(row) = self.projwbs_row(wbs_id) else {
            return 1.;
        };
        let t = self.document.table("PROJWBS").expect("row found");
        weight(t.value(&t.rows[row], "est_wt"))
    }

    /// Sets a WBS element's estimation weight; false when it has no row or
    /// already holds the value.
    pub fn set_wbs_estimation_weight(&mut self, wbs_id: i64, value: f64) -> bool {
        let Some(row) = self.projwbs_row(wbs_id) else {
            return false;
        };
        let value = value.max(0.);
        if (self.wbs_estimation_weight(wbs_id) - value).abs() < 1e-12 {
            return false;
        }
        self.document
            .table_mut("PROJWBS")
            .set(row, "est_wt", quantity_text(value));
        true
    }

    /// An activity's estimated weight (`TASK.est_wt`).
    pub fn activity_estimation_weight(&self, task_id: i64) -> f64 {
        let pid = self.model.project.proj_id;
        weight(&task_column(&self.document, pid, task_id, "est_wt"))
    }

    /// The project's own WBS node (`proj_node_flag`), if the file has one.
    pub fn project_wbs_node(&self) -> Option<i64> {
        let pid = self.model.project.proj_id;
        let t = self.document.table("PROJWBS")?;
        t.rows
            .iter()
            .find(|r| num(t.value(r, "proj_id")) == pid && t.value(r, "proj_node_flag") == "Y")
            .map(|r| num(t.value(r, "wbs_id")))
            .filter(|id| self.wbs.iter().any(|w| w.id == *id))
    }

    /// Labor units per receiving activity (task id, units) for `total`
    /// units spread from `root` (a WBS id; `None` is the whole project).
    /// `weights` overrides WBS weights by id (draft values of a dialog).
    pub fn top_down_distribution(
        &self,
        total: f64,
        root: Option<i64>,
        not_started_only: bool,
        weights: &HashMap<i64, f64>,
    ) -> Vec<(i64, f64)> {
        if !total.is_finite() || total <= 0. {
            return vec![];
        }
        let known: HashMap<i64, Option<i64>> = self.wbs.iter().map(|w| (w.id, w.parent)).collect();
        // Child elements and receiving activities of each element; `None`
        // collects the top level (elements without a known parent and
        // activities outside the WBS).
        let mut order: Vec<&Wbs> = self.wbs.iter().collect();
        order.sort_by_key(|w| (w.sequence, w.id));
        let mut children: HashMap<Option<i64>, Vec<i64>> = HashMap::new();
        for w in &order {
            let parent = w.parent.filter(|p| known.contains_key(p));
            children.entry(parent).or_default().push(w.id);
        }
        let mut activities: HashMap<Option<i64>, Vec<i64>> = HashMap::new();
        for a in &self.model.activities {
            if !matches!(a.task_type, TaskType::Task | TaskType::Resource)
                || (not_started_only && a.status != StatusType::NotStarted)
            {
                continue;
            }
            let wbs = self
                .details
                .get(&a.task_id)
                .map(|d| d.wbs)
                .filter(|w| known.contains_key(w));
            activities.entry(wbs).or_default().push(a.task_id);
        }
        // Whether an element's subtree holds a receiving activity.
        fn holds(
            at: Option<i64>,
            children: &HashMap<Option<i64>, Vec<i64>>,
            activities: &HashMap<Option<i64>, Vec<i64>>,
            memo: &mut HashMap<Option<i64>, bool>,
            depth: usize,
        ) -> bool {
            if let Some(&v) = memo.get(&at) {
                return v;
            }
            let v = activities.get(&at).is_some_and(|a| !a.is_empty())
                || (depth < 10_000
                    && children.get(&at).is_some_and(|c| {
                        c.iter()
                            .any(|&k| holds(Some(k), children, activities, memo, depth + 1))
                    }));
            memo.insert(at, v);
            v
        }
        let mut memo = HashMap::new();
        // Weights read once (a scan per activity made this quadratic).
        let pid = self.model.project.proj_id;
        let column = |table: &str, key: &str| -> HashMap<i64, f64> {
            self.document.table(table).map_or(HashMap::new(), |t| {
                let mut m = HashMap::new();
                for r in &t.rows {
                    if num(t.value(r, "proj_id")) == pid {
                        m.entry(num(t.value(r, key)))
                            .or_insert_with(|| weight(t.value(r, "est_wt")));
                    }
                }
                m
            })
        };
        let wbs_weights = column("PROJWBS", "wbs_id");
        let task_weights = column("TASK", "task_id");
        let start = match root {
            Some(id) if known.contains_key(&id) => Some(id),
            Some(_) => return vec![],
            None => self.project_wbs_node(),
        };
        let mut out = vec![];
        let mut stack = vec![(start, total, 0usize)];
        while let Some((at, amount, depth)) = stack.pop() {
            let kids: Vec<i64> = children
                .get(&at)
                .map(|c| {
                    c.iter()
                        .copied()
                        .filter(|&k| holds(Some(k), &children, &activities, &mut memo, 0))
                        .collect()
                })
                .unwrap_or_default();
            let own: &[i64] = activities.get(&at).map_or(&[], |v| v.as_slice());
            let w: Vec<f64> = kids
                .iter()
                .map(|k| {
                    weights
                        .get(k)
                        .copied()
                        .unwrap_or_else(|| wbs_weights.get(k).copied().unwrap_or(1.))
                        .max(0.)
                })
                .chain(
                    own.iter()
                        .map(|t| task_weights.get(t).copied().unwrap_or(1.)),
                )
                .collect();
            let parts = shares(amount, &w);
            for (k, part) in kids.iter().zip(&parts) {
                if depth < 10_000 {
                    stack.push((Some(*k), *part, depth + 1));
                }
            }
            for (t, part) in own.iter().zip(&parts[kids.len()..]) {
                out.push((*t, *part));
            }
        }
        // Report in activity order.
        let rank: HashMap<i64, usize> = self
            .model
            .activities
            .iter()
            .enumerate()
            .map(|(i, a)| (a.task_id, i))
            .collect();
        out.sort_by_key(|(t, _)| rank.get(t).copied().unwrap_or(usize::MAX));
        out
    }

    /// Writes the labor units of `lines` (task id, budgeted units) to the
    /// activities and their labor assignments. Returns the activities
    /// changed. Durations and dates are left alone.
    pub fn apply_labor_estimate(&mut self, lines: &[(i64, f64)]) -> usize {
        let pid = self.model.project.proj_id;
        let rsrc_types: HashMap<i64, String> =
            self.document.table("RSRC").map_or_else(HashMap::new, |t| {
                t.rows
                    .iter()
                    .map(|r| {
                        (
                            num(t.value(r, "rsrc_id")),
                            t.value(r, "rsrc_type").to_string(),
                        )
                    })
                    .collect()
            });
        let index: HashMap<i64, usize> = self
            .model
            .activities
            .iter()
            .enumerate()
            .map(|(i, a)| (a.task_id, i))
            .collect();
        let mut changed = 0;
        let mut assignments_changed = false;
        for &(task_id, units) in lines {
            let Some(&i) = index.get(&task_id) else {
                continue;
            };
            let units = units.max(0.);
            let (rem_hours, original_hours) = (
                self.model.activities[i].rem as f64 / 60.,
                self.details
                    .get(&task_id)
                    .map_or(0., |d| d.original_duration as f64 / 60.),
            );
            let labor: Vec<usize> = self
                .assignment_rows(task_id)
                .into_iter()
                .filter(|&row| {
                    let t = self.document.table("TASKRSRC").expect("rows exist");
                    let r = &t.rows[row];
                    let mut kind = t.value(r, "rsrc_type").to_string();
                    if kind.is_empty() {
                        kind = rsrc_types
                            .get(&num(t.value(r, "rsrc_id")))
                            .cloned()
                            .unwrap_or_default();
                    }
                    !matches!(kind.as_str(), "RT_Equip" | "RT_Mat")
                })
                .collect();
            let before = (
                task_column(&self.document, pid, task_id, "target_work_qty"),
                task_column(&self.document, pid, task_id, "remain_work_qty"),
            );
            let mut touched = false;
            let (budget_total, remain_total) = if labor.is_empty() {
                let actual = float(&task_column(&self.document, pid, task_id, "act_work_qty"));
                (units, (units - actual).max(0.))
            } else {
                let budgets: Vec<f64> = labor
                    .iter()
                    .map(|&row| {
                        let t = self.document.table("TASKRSRC").expect("rows exist");
                        float(t.value(&t.rows[row], "target_qty")).max(0.)
                    })
                    .collect();
                let parts = shares(units, &budgets);
                let (mut b, mut r) = (0., 0.);
                for (&row, &part) in labor.iter().zip(&parts) {
                    let t = self.document.table_mut("TASKRSRC");
                    let cell = |t: &Table, k: &str| float(t.value(&t.rows[row], k));
                    let actual = cell(t, "act_reg_qty") + cell(t, "act_ot_qty");
                    let remaining = (part - actual).max(0.);
                    // Hours each side lasts now; kept by the new units/time.
                    let hours = |qty: f64, rate: f64, fallback: f64| {
                        if qty > 0. && rate > 0. {
                            qty / rate
                        } else {
                            fallback
                        }
                    };
                    let budget_hours = hours(
                        cell(t, "target_qty"),
                        cell(t, "target_qty_per_hr"),
                        original_hours,
                    );
                    let remain_hours = hours(
                        cell(t, "remain_qty"),
                        cell(t, "remain_qty_per_hr"),
                        rem_hours,
                    );
                    let rate = |qty: f64, h: f64| if h > 0. { qty / h } else { 0. };
                    for (k, v) in [
                        ("target_qty", part),
                        ("remain_qty", remaining),
                        ("target_qty_per_hr", rate(part, budget_hours)),
                        ("remain_qty_per_hr", rate(remaining, remain_hours)),
                    ] {
                        if (cell(t, k) - v).abs() > 1e-9 * v.abs().max(1.) {
                            t.set(row, k, quantity_text(v));
                            touched = true;
                        }
                    }
                    self.update_costs(row);
                    b += part;
                    r += remaining;
                }
                (b, r)
            };
            for (k, v) in [
                ("target_work_qty", budget_total),
                ("remain_work_qty", remain_total),
            ] {
                if (float(&task_column(&self.document, pid, task_id, k)) - v).abs()
                    > 1e-9 * v.abs().max(1.)
                {
                    set_task_column(&mut self.document, pid, task_id, k, quantity_text(v));
                }
            }
            let after = (
                task_column(&self.document, pid, task_id, "target_work_qty"),
                task_column(&self.document, pid, task_id, "remain_work_qty"),
            );
            if after != before || touched {
                changed += 1;
            }
            assignments_changed |= touched;
        }
        if assignments_changed {
            self.rebuild_assignments();
        }
        changed
    }
}
