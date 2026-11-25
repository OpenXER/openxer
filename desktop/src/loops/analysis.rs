//! Circular relationship analysis: pure functions over a [`Model`].
//!
//! Every strongly connected group of activities is found with an iterative
//! Tarjan pass (no recursion, linear in activities plus relationships). Each
//! group is shown as its shortest loop, a chain of relationships, and gets a
//! suggested relationship to remove with the reason in words. A greedy plan
//! then lists a small set of relationships whose removal clears every loop.
use openxer_engine::calendar::DAY;
use openxer_engine::model::{Link, LinkType, Model, TaskType};
use std::collections::HashMap;

/// Most relationships the greedy plan will list.
const PLAN_CAP: usize = 50;
/// Groups up to this size search for the shortest loop from every activity;
/// larger groups sample start points.
const EXACT_BELOW: usize = 300;
const SAMPLE_STARTS: usize = 24;
/// Most chain relationships tried when counting what a removal frees.
const TRIAL_CAP: usize = 40;

/// A relationship named by the Task IDs of its ends and its type, which
/// stays valid when relationship indices shift.
pub type LinkKey = (i64, i64, u8);

pub fn link_key(model: &Model, link: &Link) -> LinkKey {
    let kind = match link.link_type {
        LinkType::FS => 0,
        LinkType::SS => 1,
        LinkType::FF => 2,
        LinkType::SF => 3,
        LinkType::None => 4,
    };
    (
        model.activities[link.pred].task_id,
        model.activities[link.succ].task_id,
        kind,
    )
}

/// What is known about when relationships were made.
#[derive(Clone, Debug, Default)]
pub struct Hints {
    /// Relationships added since the oldest undo step kept; a larger number
    /// is a later step.
    pub added: HashMap<LinkKey, u32>,
    /// The relationship's update date as stored, sortable text.
    pub updated: HashMap<LinkKey, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainTie {
    /// Index into `Model::links`.
    pub link: usize,
    pub pred: usize,
    pub succ: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    /// Index into `Model::links`.
    pub link: usize,
    pub reason: String,
    /// Reversing the tie (successor drives predecessor) is the likely repair.
    pub reversible: bool,
    /// Activities of the group that no longer sit in a loop once it is gone.
    pub frees: usize,
}

#[derive(Clone, Debug)]
pub struct LoopGroup {
    /// Every activity in the group, ascending.
    pub members: Vec<usize>,
    /// The shortest loop, one tie per row, in relationship order.
    pub chain: Vec<ChainTie>,
    pub suggestion: Option<Suggestion>,
    /// Position of the suggested tie in `chain`.
    pub suggested_row: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct LoopReport {
    /// Largest group first.
    pub groups: Vec<LoopGroup>,
    /// Relationships that clear every loop when all are removed.
    pub plan: Vec<Suggestion>,
}

impl LoopReport {
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }
}

struct Graph {
    n: usize,
    start: Vec<usize>,
    /// (successor, link index), grouped by predecessor.
    adj: Vec<(usize, usize)>,
}

impl Graph {
    fn new(model: &Model) -> Self {
        let n = model.activities.len();
        let mut start = vec![0usize; n + 1];
        for l in &model.links {
            if l.pred < n && l.succ < n {
                start[l.pred + 1] += 1;
            }
        }
        for i in 0..n {
            start[i + 1] += start[i];
        }
        let mut fill = start.clone();
        let mut adj = vec![(0, 0); start[n]];
        for (i, l) in model.links.iter().enumerate() {
            if l.pred < n && l.succ < n {
                adj[fill[l.pred]] = (l.succ, i);
                fill[l.pred] += 1;
            }
        }
        Self { n, start, adj }
    }

    fn edges(&self, v: usize) -> &[(usize, usize)] {
        &self.adj[self.start[v]..self.start[v + 1]]
    }

    /// Strongly connected groups that hold a loop: more than one activity,
    /// or one activity tied to itself. `scope` limits the walk to those
    /// activities. Sorted largest first, then by lowest activity.
    fn groups(&self, removed: &[bool], scope: Option<&[usize]>) -> Vec<Vec<usize>> {
        const NONE: usize = usize::MAX;
        let mut allowed = vec![scope.is_none(); self.n];
        if let Some(nodes) = scope {
            for &v in nodes {
                allowed[v] = true;
            }
        }
        let mut index = vec![NONE; self.n];
        let mut low = vec![0usize; self.n];
        let mut on_stack = vec![false; self.n];
        let mut stack: Vec<usize> = vec![];
        let mut calls: Vec<(usize, usize)> = vec![];
        let mut next = 0usize;
        let mut out: Vec<Vec<usize>> = vec![];
        let roots: Box<dyn Iterator<Item = usize>> = match scope {
            Some(nodes) => Box::new(nodes.iter().copied()),
            None => Box::new(0..self.n),
        };
        for root in roots {
            if index[root] != NONE {
                continue;
            }
            calls.push((root, 0));
            index[root] = next;
            low[root] = next;
            next += 1;
            stack.push(root);
            on_stack[root] = true;
            while let Some(top) = calls.len().checked_sub(1) {
                let (v, at) = calls[top];
                let edges = self.edges(v);
                if at < edges.len() {
                    let (w, link) = edges[at];
                    calls[top].1 += 1;
                    if removed[link] || !allowed[w] {
                        continue;
                    }
                    if index[w] == NONE {
                        index[w] = next;
                        low[w] = next;
                        next += 1;
                        stack.push(w);
                        on_stack[w] = true;
                        calls.push((w, 0));
                    } else if on_stack[w] {
                        low[v] = low[v].min(index[w]);
                    }
                } else {
                    calls.pop();
                    if let Some(&(parent, _)) = calls.last() {
                        low[parent] = low[parent].min(low[v]);
                    }
                    if low[v] == index[v] {
                        let mut group = vec![];
                        loop {
                            let w = stack.pop().expect("component member on the stack");
                            on_stack[w] = false;
                            group.push(w);
                            if w == v {
                                break;
                            }
                        }
                        let looped = group.len() > 1
                            || self
                                .edges(v)
                                .iter()
                                .any(|&(w, link)| w == v && !removed[link]);
                        if looped {
                            group.sort_unstable();
                            out.push(group);
                        }
                    }
                }
            }
        }
        out.sort_by(|a, b| b.len().cmp(&a.len()).then(a[0].cmp(&b[0])));
        out
    }

    /// The shortest loop inside `group` as relationship indices from the
    /// start activity round to it.
    fn shortest_loop(&self, model: &Model, removed: &[bool], group: &[usize]) -> Vec<usize> {
        let mut in_group = vec![false; self.n];
        for &v in group {
            in_group[v] = true;
        }
        for &v in group {
            if let Some(&(_, link)) = self
                .edges(v)
                .iter()
                .find(|&&(w, link)| w == v && !removed[link])
            {
                return vec![link];
            }
        }
        let mut starts: Vec<usize> = vec![];
        if group.len() <= EXACT_BELOW {
            starts.extend_from_slice(group);
        } else {
            // Effort-driven activities sit on the typical bad tie; try them
            // first, then spread the rest over the group.
            starts.extend(
                group
                    .iter()
                    .copied()
                    .filter(|&v| derived_dates(model.activities[v].task_type))
                    .take(SAMPLE_STARTS),
            );
            let step = (group.len() / SAMPLE_STARTS).max(1);
            starts.extend(group.iter().copied().step_by(step));
        }
        let mut best: Vec<usize> = vec![];
        let mut stamp = vec![usize::MAX; self.n];
        let mut dist = vec![0usize; self.n];
        let mut via = vec![0usize; self.n];
        let mut queue: Vec<usize> = vec![];
        for (round, &s) in starts.iter().enumerate() {
            queue.clear();
            queue.push(s);
            stamp[s] = round;
            dist[s] = 0;
            let mut head = 0;
            'bfs: while head < queue.len() {
                let u = queue[head];
                head += 1;
                if !best.is_empty() && dist[u] + 1 >= best.len() {
                    break;
                }
                for &(v, link) in self.edges(u) {
                    if removed[link] || !in_group[v] {
                        continue;
                    }
                    if v == s {
                        let mut ring = vec![link];
                        let mut at = u;
                        while at != s {
                            ring.push(via[at]);
                            at = model.links[via[at]].pred;
                        }
                        ring.reverse();
                        best = ring;
                        break 'bfs;
                    }
                    if stamp[v] != round {
                        stamp[v] = round;
                        dist[v] = dist[u] + 1;
                        via[v] = link;
                        queue.push(v);
                    }
                }
            }
            if !best.is_empty() && best.len() <= 2 {
                break;
            }
        }
        best
    }
}

/// Activity types whose dates come from the activities around them.
fn derived_dates(kind: TaskType) -> bool {
    matches!(kind, TaskType::Loe | TaskType::Wbs | TaskType::Hamwk)
}

fn derived_name(kind: TaskType) -> &'static str {
    match kind {
        TaskType::Loe => "Level of Effort",
        TaskType::Wbs => "WBS Summary",
        _ => "Hammock",
    }
}

/// Whether any loop exists. Cheap: one linear pass.
pub fn has_loop(model: &Model) -> bool {
    let g = Graph::new(model);
    !g.groups(&vec![false; model.links.len()], None).is_empty()
}

/// Days the stored dates put the predecessor's driving point after the
/// successor's driven point, when they do by a day or more.
fn days_backwards(model: &Model, link: &Link) -> Option<i64> {
    let (p, s) = (&model.activities[link.pred], &model.activities[link.succ]);
    let from_start = matches!(link.link_type, LinkType::SS | LinkType::SF);
    let to_start = matches!(link.link_type, LinkType::FS | LinkType::SS);
    let from = if from_start {
        p.early_start
    } else {
        p.early_finish
    }?;
    let to = if to_start {
        s.early_start
    } else {
        s.early_finish
    }?;
    let late = from + link.lag - to;
    (late >= DAY).then_some(late / DAY)
}

fn end_word(start: bool) -> &'static str {
    if start {
        "start"
    } else {
        "finish"
    }
}

struct Candidate {
    row: usize,
    rank: (u8, u32, usize),
    reasons: Vec<String>,
    reversible: bool,
    frees: usize,
}

fn suggest(
    model: &Model,
    graph: &Graph,
    removed: &mut [bool],
    group: &[usize],
    chain: &[usize],
    hints: &Hints,
    label: &dyn Fn(usize) -> String,
) -> Option<(usize, Suggestion)> {
    if chain.is_empty() {
        return None;
    }
    let keys: Vec<LinkKey> = chain
        .iter()
        .map(|&l| link_key(model, &model.links[l]))
        .collect();
    let newest_update = {
        let stamps: Vec<Option<&String>> = keys.iter().map(|k| hints.updated.get(k)).collect();
        let max = stamps.iter().flatten().max().copied();
        max.filter(|m| stamps.iter().filter(|s| **s == Some(*m)).count() == 1 && chain.len() > 1)
    };
    let mut best: Option<Candidate> = None;
    for (row, &l) in chain.iter().enumerate() {
        let link = &model.links[l];
        let p = &model.activities[link.pred];
        let mut reasons = vec![];
        let mut category = 0u8;
        let mut recency = 0u32;
        let reversible = derived_dates(p.task_type) && link.pred != link.succ;
        if reversible {
            category = 4;
            reasons.push(format!(
                "{} is a {} activity, so it takes its dates from the activities around it and should not drive {}. This tie is probably reversed.",
                label(link.pred),
                derived_name(p.task_type),
                label(link.succ)
            ));
        }
        if let Some(days) = days_backwards(model, link) {
            category = category.max(3);
            reasons.push(format!(
                "It points backwards in time against the last schedule: the predecessor's {} is about {days} day{} after the successor's {}.",
                end_word(matches!(link.link_type, LinkType::SS | LinkType::SF)),
                if days == 1 { "" } else { "s" },
                end_word(matches!(link.link_type, LinkType::FS | LinkType::SS)),
            ));
        }
        if let Some(&step) = hints.added.get(&keys[row]) {
            if category < 3 {
                category = category.max(2);
            }
            recency = step;
            let newest = keys
                .iter()
                .filter_map(|k| hints.added.get(k))
                .max()
                .is_some_and(|m| *m == step);
            reasons.push(if newest {
                "It was added most recently in this session.".to_string()
            } else {
                "It was added in this session.".to_string()
            });
        }
        if newest_update == hints.updated.get(&keys[row]) && newest_update.is_some() {
            category = category.max(1);
            reasons.push(
                "It has the most recent update date of the relationships in this loop.".into(),
            );
        }
        // What removing it frees.
        let stride = chain.len().div_ceil(TRIAL_CAP);
        let frees = if row % stride == 0 {
            removed[l] = true;
            let left: usize = graph
                .groups(removed, Some(group))
                .iter()
                .map(Vec::len)
                .sum();
            removed[l] = false;
            group.len() - left
        } else {
            0
        };
        let candidate = Candidate {
            row,
            rank: (category, recency, frees),
            reasons,
            reversible,
            frees,
        };
        let better = match &best {
            None => true,
            Some(b) => {
                // Category first, then recency within it, then what it frees.
                let key =
                    |c: &Candidate| (c.rank.0, if c.rank.0 == 2 { c.rank.1 } else { 0 }, c.rank.2);
                key(&candidate) > key(b)
            }
        };
        if better {
            best = Some(candidate);
        }
    }
    let c = best?;
    let mut reasons = c.reasons;
    reasons.push(if c.frees >= group.len() {
        "Deleting it clears this loop group.".to_string()
    } else if c.frees == 0 {
        "Deleting it does not clear the group by itself; more ties must change.".to_string()
    } else {
        format!(
            "Deleting it frees {} of {} activities; other loops remain in the group.",
            c.frees,
            group.len()
        )
    });
    if reasons.len() == 1 {
        reasons.insert(
            0,
            "No tie in this loop stands out by type, date or history; this one breaks the most loops at once.".into(),
        );
    }
    Some((
        c.row,
        Suggestion {
            link: chain[c.row],
            reason: reasons.join(" "),
            reversible: c.reversible,
            frees: c.frees,
        },
    ))
}

fn tie(model: &Model, link: usize) -> ChainTie {
    ChainTie {
        link,
        pred: model.links[link].pred,
        succ: model.links[link].succ,
    }
}

/// All loops of `model`. `label` names an activity in reasons.
pub fn analyze(model: &Model, hints: &Hints, label: &dyn Fn(usize) -> String) -> LoopReport {
    let graph = Graph::new(model);
    let mut removed = vec![false; model.links.len()];
    let mut report = LoopReport::default();
    for group in graph.groups(&removed, None) {
        let mut ring = graph.shortest_loop(model, &removed, &group);
        // Start the chain at the lowest activity so the list reads steadily.
        if let Some(at) = (0..ring.len()).min_by_key(|&i| model.links[ring[i]].pred) {
            ring.rotate_left(at);
        }
        let found = suggest(model, &graph, &mut removed, &group, &ring, hints, label);
        let chain: Vec<ChainTie> = ring.iter().map(|&l| tie(model, l)).collect();
        let (row, suggestion) = match found {
            Some((row, s)) => (Some(row), Some(s)),
            None => (None, None),
        };
        report.groups.push(LoopGroup {
            members: group,
            chain,
            suggestion,
            suggested_row: row,
        });
    }
    // The plan: each group's suggestion, then repeat on what remains.
    for s in report.groups.iter().filter_map(|g| g.suggestion.as_ref()) {
        removed[s.link] = true;
        report.plan.push(s.clone());
    }
    while report.plan.len() < PLAN_CAP {
        let groups = graph.groups(&removed, None);
        if groups.is_empty() {
            break;
        }
        let before = report.plan.len();
        for group in groups {
            let ring = graph.shortest_loop(model, &removed, &group);
            if let Some((_, s)) = suggest(model, &graph, &mut removed, &group, &ring, hints, label)
            {
                removed[s.link] = true;
                report.plan.push(s);
            }
            if report.plan.len() >= PLAN_CAP {
                break;
            }
        }
        if report.plan.len() == before {
            break;
        }
    }
    report
}
