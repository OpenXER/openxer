//! The core of a schedule comparison, shared by the app's Schedule
//! Comparison (Tools > Schedule Comparison, which shows field changes
//! through its column display on top) and the agents' `compare` answer:
//! which activities match by Activity ID, which were added or deleted, and
//! how the logic changed between two projects. Pure: reads two projects,
//! changes nothing.
use crate::ProjectData;
use openxer_engine::model::LinkType;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// What happened to an item between the original and the revised project.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Change {
    Added,
    Deleted,
    Changed,
}

/// Activities of the two projects matched by Activity ID (the first
/// activity holding an ID counts; a repeated ID is not matched again).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Matches {
    /// (original index, revised index), in the revised project's order.
    pub pairs: Vec<(usize, usize)>,
    /// Revised activities the original lacks, in the revised order.
    pub added: Vec<usize>,
    /// Original activities the revised project lacks, in the original order.
    pub deleted: Vec<usize>,
}

/// Each Activity ID's first activity.
pub fn by_code(p: &ProjectData) -> HashMap<&str, usize> {
    let mut out = HashMap::new();
    for (i, a) in p.model.activities.iter().enumerate() {
        out.entry(a.task_code.as_str()).or_insert(i);
    }
    out
}

/// Match `old`'s activities to `new`'s by Activity ID.
pub fn match_activities(old: &ProjectData, new: &ProjectData) -> Matches {
    let a = by_code(old);
    let b = by_code(new);
    let mut out = Matches::default();
    for (j, act) in new.model.activities.iter().enumerate() {
        if b.get(act.task_code.as_str()) != Some(&j) {
            continue;
        }
        match a.get(act.task_code.as_str()) {
            Some(&i) => out.pairs.push((i, j)),
            None => out.added.push(j),
        }
    }
    for (i, act) in old.model.activities.iter().enumerate() {
        if a.get(act.task_code.as_str()) == Some(&i) && !b.contains_key(act.task_code.as_str()) {
            out.deleted.push(i);
        }
    }
    out
}

/// One relationship as compared: its type, lag (working minutes) and its
/// index in its own project's links.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TieSide {
    pub link_type: LinkType,
    pub lag: i64,
    pub link: usize,
}

/// A relationship added, deleted or changed (type or lag), between the
/// activities with these IDs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TieChange {
    pub change: Change,
    pub pred: String,
    pub succ: String,
    pub old: Option<TieSide>,
    pub new: Option<TieSide>,
}

fn ties(p: &ProjectData) -> BTreeMap<(String, String), Vec<TieSide>> {
    let m = &p.model;
    let mut out: BTreeMap<(String, String), Vec<TieSide>> = BTreeMap::new();
    for (k, l) in m.links.iter().enumerate() {
        let (Some(pred), Some(succ)) = (m.activities.get(l.pred), m.activities.get(l.succ)) else {
            continue;
        };
        out.entry((pred.task_code.clone(), succ.task_code.clone()))
            .or_default()
            .push(TieSide {
                link_type: l.link_type,
                lag: l.lag,
                link: k,
            });
    }
    out
}

/// How the logic changed: ties are keyed by predecessor and successor
/// Activity ID. Between one pair, ties of the same type are paired first (a
/// lag change is a change), then what is left in order (a type change);
/// the rest were added or deleted. Unchanged ties are left out. In
/// predecessor, then successor order.
pub fn tie_changes(old: &ProjectData, new: &ProjectData) -> Vec<TieChange> {
    let a = ties(old);
    let b = ties(new);
    let keys: BTreeSet<&(String, String)> = a.keys().chain(b.keys()).collect();
    let mut out = vec![];
    for key in keys {
        let mut olds = a.get(key).cloned().unwrap_or_default();
        let mut news = b.get(key).cloned().unwrap_or_default();
        let mut pairs: Vec<(Option<TieSide>, Option<TieSide>)> = vec![];
        olds.retain(|o| {
            if let Some(k) = news.iter().position(|n| n.link_type == o.link_type) {
                let n = news.remove(k);
                pairs.push((Some(*o), Some(n)));
                false
            } else {
                true
            }
        });
        while !olds.is_empty() && !news.is_empty() {
            pairs.push((Some(olds.remove(0)), Some(news.remove(0))));
        }
        pairs.extend(olds.into_iter().map(|o| (Some(o), None)));
        pairs.extend(news.into_iter().map(|n| (None, Some(n))));
        for (o, n) in pairs {
            let change = match (&o, &n) {
                (Some(o), Some(n)) if o.link_type == n.link_type && o.lag == n.lag => continue,
                (Some(_), Some(_)) => Change::Changed,
                (Some(_), None) => Change::Deleted,
                _ => Change::Added,
            };
            out.push(TieChange {
                change,
                pred: key.0.clone(),
                succ: key.1.clone(),
                old: o,
                new: n,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_and_logic_changes_on_the_starter() {
        let old = crate::starter();
        let mut new = crate::starter();
        // Drop RB-1130's last tie, change a lag, and a type.
        let last = new.model.links.len() - 1;
        new.model.links.remove(last);
        new.model.links[0].lag = 480;
        new.model.links[1].link_type = LinkType::SS;
        new.model.activities[13].task_code = "RB-9999".into();
        new.model.rebuild_links().unwrap();
        let m = match_activities(&old, &new);
        assert_eq!(m.pairs.len(), 13);
        assert_eq!(m.added, vec![13]);
        assert_eq!(m.deleted, vec![13]);
        let t = tie_changes(&old, &new);
        let kinds: Vec<Change> = t.iter().map(|c| c.change).collect();
        assert_eq!(kinds.iter().filter(|k| **k == Change::Changed).count(), 2);
        // The renamed finish milestone's tie is deleted under its old ID.
        assert!(t
            .iter()
            .any(|c| c.change == Change::Deleted && c.succ == "RB-1130"));
    }
}
