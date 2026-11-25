//! F9 over a group of open projects joined by cross-project relationships:
//! one combined network.
use super::compose::{cross_links, resolve};
use openxer_engine::{
    calendar::DAY,
    model::{Link, Model, TaskType},
    options::ScheduleOptions,
    scheduler::{schedule_together, Partition},
};
use openxer_xer::ProjectData;
use std::collections::HashSet;

fn num(s: &str) -> i64 {
    s.trim().parse().unwrap_or(0)
}

/// Schedule `projects` (one linked group, already refreshed) together with
/// the run's `options`. On success each project holds its new dates, its
/// finish and the external limits of its cross-project relationships; on
/// error nothing is changed.
pub fn schedule_group(
    projects: &mut [ProjectData],
    options: &ScheduleOptions,
) -> Result<(), String> {
    if projects.len() < 2 {
        return Err("A combined schedule needs two or more projects".into());
    }
    if options.do_level {
        return Err("Leveling resources of projects linked to each other is not supported yet. Clear 'Level resources during scheduling' or schedule the projects with 'Ignore relationships to and from other projects'.".into());
    }
    if projects.iter().any(|p| {
        p.model
            .activities
            .iter()
            .any(|a| a.task_type == TaskType::Resource)
    }) {
        return Err("Resource Dependent activities require resource leveling, which linked projects do not support yet.".into());
    }
    let refs: Vec<&ProjectData> = projects.iter().collect();
    let links = cross_links(&refs);
    let origin = refs
        .iter()
        .map(|p| p.model.origin_serial)
        .min()
        .expect("two or more projects");
    // Activities whose relationships to other projects are not all in this
    // group keep their stored external dates for the closed ends.
    let mut closed_pred: HashSet<(usize, i64)> = HashSet::new();
    let mut closed_succ: HashSet<(usize, i64)> = HashSet::new();
    for (k, p) in refs.iter().enumerate() {
        let pid = p.model.project.proj_id;
        for x in p.external_links() {
            let open = resolve(&refs, k, x.pred_project).is_some_and(|q| {
                refs[q]
                    .model
                    .activities
                    .iter()
                    .any(|a| a.task_id == x.pred_task)
            });
            if !open {
                closed_pred.insert((k, x.succ_task));
            }
        }
        if let Some(t) = p.document.table("TASKPRED") {
            for r in &t.rows {
                let owner = num(t.value(r, "proj_id"));
                if num(t.value(r, "pred_proj_id")) == pid
                    && owner != pid
                    && resolve(&refs, k, owner).is_none()
                {
                    closed_succ.insert((k, num(t.value(r, "pred_task_id"))));
                }
            }
        }
    }
    let mut model = Model {
        origin_serial: origin,
        project_calendar: 0,
        calendars: vec![],
        activities: vec![],
        links: vec![],
        project: Default::default(),
        options: options.clone(),
    };
    let mut partition = Partition::default();
    let mut starts = vec![];
    let mut cal_offsets = vec![];
    for (k, p) in refs.iter().enumerate() {
        let mut m = p.model.clone();
        m.rebase(origin);
        let base = model.activities.len();
        let cal_offset = model.calendars.len();
        starts.push(base);
        cal_offsets.push(cal_offset);
        if k == 0 {
            model.project_calendar = m.project_calendar + cal_offset;
            model.project = m.project.clone();
        }
        for mut a in m.activities {
            a.cal += cal_offset;
            for asg in &mut a.assignments {
                if let Some(c) = &mut asg.cal {
                    *c += cal_offset;
                }
            }
            a.preds.clear();
            a.succs.clear();
            model.activities.push(a);
            partition.of.push(k);
        }
        for l in &m.links {
            model.links.push(Link {
                pred: base + l.pred,
                succ: base + l.succ,
                ..l.clone()
            });
        }
        model.calendars.append(&mut m.calendars);
        partition.projects.push(m.project);
    }
    let mut cross_pred: HashSet<(usize, usize)> = HashSet::new();
    let mut cross_succ: HashSet<(usize, usize)> = HashSet::new();
    for (ps, pi, s, si, x) in &links {
        model.links.push(Link {
            pred: starts[*ps] + pi,
            succ: starts[*s] + si,
            link_type: x.link_type,
            lag: x.lag,
            driving_path: false,
        });
        cross_pred.insert((*s, *si));
        cross_succ.insert((*ps, *pi));
    }
    for (k, p) in refs.iter().enumerate() {
        for (j, a) in p.model.activities.iter().enumerate() {
            let c = &mut model.activities[starts[k] + j];
            if cross_pred.contains(&(k, j)) && !closed_pred.contains(&(k, a.task_id)) {
                c.ext_early_start = None;
            }
            if cross_succ.contains(&(k, j)) && !closed_succ.contains(&(k, a.task_id)) {
                c.ext_late_end = None;
            }
        }
    }
    model.rebuild_links()?;
    let out = schedule_together(&mut model, partition)?;
    for (k, p) in projects.iter_mut().enumerate() {
        let delta = (p.model.origin_serial - origin) * DAY;
        for j in 0..p.model.activities.len() {
            let index = starts[k] + j;
            let stored = (
                p.model.activities[j].ext_early_start,
                p.model.activities[j].ext_late_end,
            );
            let mut a = model.activities[index].clone();
            a.shift_dates(-delta);
            a.cal -= cal_offsets[k];
            for asg in &mut a.assignments {
                if let Some(c) = &mut asg.cal {
                    *c -= cal_offsets[k];
                }
            }
            a.preds.clear();
            a.succs.clear();
            let task = a.task_id;
            if cross_pred.contains(&(k, j)) {
                let limit = out.external_start[index].map(|v| v - delta);
                a.ext_early_start = if closed_pred.contains(&(k, task)) {
                    stored.0.max(limit)
                } else {
                    limit
                };
            }
            if cross_succ.contains(&(k, j)) {
                let limit = out.external_finish[index].map(|v| v - delta);
                a.ext_late_end = if closed_succ.contains(&(k, task)) {
                    match (stored.1, limit) {
                        (Some(s), Some(l)) => Some(s.min(l)),
                        (s, l) => s.or(l),
                    }
                } else {
                    limit
                };
            }
            p.model.activities[j] = a;
        }
        p.model.project.scd_end = out.projects[k].scd_end.map(|v| v - delta);
        p.model.rebuild_links()?;
    }
    Ok(())
}
