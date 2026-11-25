//! Assembling several open projects: the read-only display composite,
//! resolving cross-project relationships and grouping linked projects.
use openxer_engine::model::Link;
use openxer_xer::{ProjectData, Wbs};
use std::collections::{BTreeSet, HashMap, HashSet};

/// The project among `projects` that a relationship of `projects[from]`
/// names as `proj_id`: the one from the same source file first, otherwise
/// the only open project with that ID.
pub fn resolve(projects: &[&ProjectData], from: usize, proj_id: i64) -> Option<usize> {
    let candidates: Vec<usize> = (0..projects.len())
        .filter(|&k| k != from && projects[k].model.project.proj_id == proj_id)
        .collect();
    let source = &projects[from].source_name;
    candidates
        .iter()
        .copied()
        .find(|&k| &projects[k].source_name == source)
        .or_else(|| (candidates.len() == 1).then(|| candidates[0]))
}

/// A resolved cross-project relationship: (predecessor project, its
/// activity index, successor project, its activity index, the row).
pub type Resolved = (usize, usize, usize, usize, openxer_xer::ExternalLink);

/// Every cross-project relationship between the given projects whose two
/// activities exist.
pub fn cross_links(projects: &[&ProjectData]) -> Vec<Resolved> {
    let index: Vec<HashMap<i64, usize>> = projects
        .iter()
        .map(|p| {
            p.model
                .activities
                .iter()
                .enumerate()
                .map(|(i, a)| (a.task_id, i))
                .collect()
        })
        .collect();
    let mut out = vec![];
    for (s, p) in projects.iter().enumerate() {
        for x in p.external_links() {
            let Some(ps) = resolve(projects, s, x.pred_project) else {
                continue;
            };
            let (Some(&pi), Some(&si)) = (index[ps].get(&x.pred_task), index[s].get(&x.succ_task))
            else {
                continue;
            };
            out.push((ps, pi, s, si, x));
        }
    }
    out
}

/// Projects joined by cross-project relationships (connected groups), each
/// in ascending order; groups ordered by their first project.
pub fn link_groups(projects: &[&ProjectData]) -> Vec<Vec<usize>> {
    let mut parent: Vec<usize> = (0..projects.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for (a, _, b, _, _) in cross_links(projects) {
        let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
    }
    let mut groups: Vec<Vec<usize>> = vec![];
    let mut at: HashMap<usize, usize> = HashMap::new();
    for i in 0..projects.len() {
        let r = root(&mut parent, i);
        match at.get(&r) {
            Some(&g) => groups[g].push(i),
            None => {
                at.insert(r, groups.len());
                groups.push(vec![i]);
            }
        }
    }
    groups
}

fn num(s: &str) -> i64 {
    s.trim().parse().unwrap_or(0)
}

/// Sequence of the first project band node of a composite (the others follow).
/// The shared WBS rules know it (`openxer_xer::edit::wbs::is_project_band`),
/// so full codes name each band by itself.
const BAND_SEQUENCE: i64 = openxer_xer::edit::wbs::PROJECT_BAND_SEQUENCE;

/// Replace the rows `base` holds for project `pid` with the rows `from` holds
/// for it (tables with a `proj_id` column), and add dictionary rows `base`
/// lacks.
fn merge_owned_rows(base: &mut openxer_xer::Document, from: &openxer_xer::Document, pid: i64) {
    for t in &mut base.tables {
        if let Some(col) = t.field("proj_id") {
            t.rows.retain(|r| {
                r.get(col)
                    .is_none_or(|v| v.trim().is_empty() || num(v) != pid)
            });
        }
    }
    for t in &from.tables {
        let owned = t.field("proj_id");
        let target = base.table_mut(&t.name);
        if target.fields.is_empty() {
            target.fields = t.fields.clone();
        }
        let key = t.fields.first().cloned().unwrap_or_default();
        let existing: HashSet<String> = target
            .rows
            .iter()
            .map(|r| target.value(r, &key).to_string())
            .collect();
        for r in &t.rows {
            let take = match owned {
                Some(c) => r.get(c).is_some_and(|v| num(v) == pid),
                None => !existing.contains(t.value(r, &key)),
            };
            if !take {
                continue;
            }
            let mut values = vec![String::new(); target.fields.len()];
            for (k, field) in t.fields.iter().enumerate() {
                if let Some(c) = target.field(field) {
                    values[c] = r.get(k).cloned().unwrap_or_default();
                }
            }
            target.rows.push(values);
        }
    }
}

/// The display composite of open projects; `projects[0]` is the active one.
/// Its activities come first at their own indices, the others follow; every
/// date is counted from the active project's origin. Activity and WBS ids of
/// other projects keep their values unless they would collide or the project
/// comes from another source file (then only the composite renumbers them).
/// Each project gets a band node at the top of its WBS. Returns the composite
/// and, per composite activity, its (project, activity index).
pub fn compose(projects: &[&ProjectData]) -> (ProjectData, Vec<(usize, usize)>) {
    let active = projects[0];
    let origin = active.model.origin_serial;
    let mut out = active.clone();
    let mut map: Vec<(usize, usize)> = (0..active.model.activities.len()).map(|i| (0, i)).collect();
    let doc_ids = |table: &str, field: &str| -> Vec<i64> {
        active.document.table(table).map_or(vec![], |t| {
            t.rows.iter().map(|r| num(t.value(r, field))).collect()
        })
    };
    let mut task_ids: HashSet<i64> = active.model.activities.iter().map(|a| a.task_id).collect();
    task_ids.extend(doc_ids("TASK", "task_id"));
    let mut wbs_ids: HashSet<i64> = active.wbs.iter().map(|w| w.id).collect();
    wbs_ids.extend(doc_ids("PROJWBS", "wbs_id"));
    let mut next_task = task_ids.iter().copied().max().unwrap_or(0) + 1;
    let mut next_wbs = wbs_ids.iter().copied().max().unwrap_or(0) + 1;
    let mut starts = vec![0];
    let mut wbs_maps: Vec<HashMap<i64, i64>> =
        vec![active.wbs.iter().map(|w| (w.id, w.id)).collect()];
    for (slot, q) in projects.iter().enumerate().skip(1) {
        let same = q.source_name == active.source_name;
        let mut model = q.model.clone();
        model.rebase(origin);
        let cal_offset = out.model.calendars.len();
        out.model.calendars.append(&mut model.calendars);
        out.calendar_names.extend(q.calendar_names.iter().cloned());
        let mut wbs_map = HashMap::new();
        for w in &q.wbs {
            let id = if same && !wbs_ids.contains(&w.id) {
                w.id
            } else {
                while wbs_ids.contains(&next_wbs) {
                    next_wbs += 1;
                }
                next_wbs
            };
            wbs_ids.insert(id);
            wbs_map.insert(w.id, id);
        }
        for w in &q.wbs {
            out.wbs.push(Wbs {
                id: wbs_map[&w.id],
                parent: w.parent.and_then(|p| wbs_map.get(&p).copied()),
                code: w.code.clone(),
                name: w.name.clone(),
                sequence: w.sequence,
            });
        }
        let base = out.model.activities.len();
        starts.push(base);
        for (k, mut a) in model.activities.into_iter().enumerate() {
            let old = a.task_id;
            let id = if same && !task_ids.contains(&old) {
                old
            } else {
                while task_ids.contains(&next_task) {
                    next_task += 1;
                }
                next_task
            };
            task_ids.insert(id);
            a.task_id = id;
            a.cal += cal_offset;
            for asg in &mut a.assignments {
                if let Some(c) = &mut asg.cal {
                    *c += cal_offset;
                }
            }
            a.preds.clear();
            a.succs.clear();
            if let Some(d) = q.details.get(&old) {
                let mut d = d.clone();
                d.wbs = wbs_map.get(&d.wbs).copied().unwrap_or(d.wbs);
                out.details.insert(id, d);
            }
            map.push((slot, k));
            out.model.activities.push(a);
        }
        for l in &model.links {
            out.model.links.push(Link {
                pred: base + l.pred,
                succ: base + l.succ,
                ..l.clone()
            });
        }
        for (id, name) in &q.resource_names {
            out.resource_names
                .entry(*id)
                .or_insert_with(|| name.clone());
        }
        if same {
            merge_owned_rows(&mut out.document, &q.document, q.model.project.proj_id);
        }
        wbs_maps.push(wbs_map);
    }
    // One band node per project at the top of its WBS, ordered by project ID.
    let mut order: Vec<usize> = (0..projects.len()).collect();
    order.sort_by(|&a, &b| {
        projects[a]
            .model
            .project
            .name
            .cmp(&projects[b].model.project.name)
            .then(projects[a].source_name.cmp(&projects[b].source_name))
    });
    for (rank, &slot) in order.iter().enumerate() {
        let p = projects[slot];
        while wbs_ids.contains(&next_wbs) {
            next_wbs += 1;
        }
        let band = next_wbs;
        wbs_ids.insert(band);
        let members: BTreeSet<i64> = wbs_maps[slot].values().copied().collect();
        for w in &mut out.wbs {
            if members.contains(&w.id) && w.parent.is_none_or(|p| !members.contains(&p)) {
                w.parent = Some(band);
            }
        }
        out.wbs.push(Wbs {
            id: band,
            parent: None,
            code: p.model.project.name.clone(),
            name: crate::views::project_title(p),
            sequence: BAND_SEQUENCE + rank as i64,
        });
    }
    let refs: Vec<&ProjectData> = projects.to_vec();
    for (ps, pi, s, si, x) in cross_links(&refs) {
        out.model.links.push(Link {
            pred: starts[ps] + pi,
            succ: starts[s] + si,
            link_type: x.link_type,
            lag: x.lag,
            driving_path: false,
        });
    }
    let _ = out.model.rebuild_links();
    (out, map)
}
