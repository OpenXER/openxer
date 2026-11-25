use openxer_engine::model::{Link, LinkType, Model};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Start,
    Finish,
}

pub fn kind(from: Edge, to: Edge) -> LinkType {
    match (from, to) {
        (Edge::Finish, Edge::Start) => LinkType::FS,
        (Edge::Start, Edge::Start) => LinkType::SS,
        (Edge::Finish, Edge::Finish) => LinkType::FF,
        (Edge::Start, Edge::Finish) => LinkType::SF,
    }
}

pub fn can_connect(model: &Model, pred: usize, succ: usize) -> Result<(), String> {
    if pred >= model.activities.len() || succ >= model.activities.len() {
        return Err("The activity is no longer available.".into());
    }
    if pred == succ {
        return Err("An activity cannot be its own predecessor.".into());
    }
    let mut visited = vec![false; model.activities.len()];
    let mut successors = vec![Vec::new(); model.activities.len()];
    for link in &model.links {
        successors[link.pred].push(link.succ);
    }
    let mut pending = vec![succ];
    while let Some(index) = pending.pop() {
        if index == pred {
            return Err("This relationship would create a circular path.".into());
        }
        if visited[index] {
            continue;
        }
        visited[index] = true;
        pending.extend(successors[index].iter().copied());
    }
    Ok(())
}

pub fn add(
    model: &mut Model,
    pred: usize,
    succ: usize,
    link_type: LinkType,
    lag: i64,
) -> Result<(), String> {
    can_connect(model, pred, succ)?;
    if model
        .links
        .iter()
        .any(|l| l.pred == pred && l.succ == succ && l.link_type == link_type)
    {
        return Err("This relationship type already exists between these activities.".into());
    }
    model.links.push(Link {
        pred,
        succ,
        link_type,
        lag,
        driving_path: false,
    });
    model.rebuild_links()
}

/// Whether the relationships of `model` plus the `extra` (predecessor,
/// successor) pairs form no loop. One pass over the whole network, so a
/// caller adding many relationships checks them all at once.
pub fn acyclic_with(model: &Model, extra: &[(usize, usize)]) -> bool {
    let n = model.activities.len();
    let mut successors = vec![Vec::new(); n];
    let mut waiting = vec![0usize; n];
    for (pred, succ) in model
        .links
        .iter()
        .map(|l| (l.pred, l.succ))
        .chain(extra.iter().copied())
    {
        if pred >= n || succ >= n {
            return false;
        }
        successors[pred].push(succ);
        waiting[succ] += 1;
    }
    let mut ready: Vec<usize> = (0..n).filter(|&i| waiting[i] == 0).collect();
    let mut done = 0;
    while let Some(i) = ready.pop() {
        done += 1;
        for &s in &successors[i] {
            waiting[s] -= 1;
            if waiting[s] == 0 {
                ready.push(s);
            }
        }
    }
    done == n
}

/// Add several relationships as [`add`] does, with the links rebuilt once.
/// Each one is checked as [`add`] checks it, in order, against the ones
/// before it; on a refusal nothing is added. Imports use it so that adding
/// many relationships is not one network walk per relationship.
pub fn add_many(model: &mut Model, links: &[(usize, usize, LinkType, i64)]) -> Result<(), String> {
    let n = model.activities.len();
    let mut taken: std::collections::HashSet<(usize, usize, &'static str)> = model
        .links
        .iter()
        .map(|l| (l.pred, l.succ, l.link_type.as_str()))
        .collect();
    for &(pred, succ, link_type, _) in links {
        if pred >= n || succ >= n {
            return Err("The activity is no longer available.".into());
        }
        if pred == succ {
            return Err("An activity cannot be its own predecessor.".into());
        }
        if !taken.insert((pred, succ, link_type.as_str())) {
            return Err("This relationship type already exists between these activities.".into());
        }
    }
    let pairs: Vec<(usize, usize)> = links.iter().map(|&(p, s, _, _)| (p, s)).collect();
    if !acyclic_with(model, &pairs) {
        // Find the first one that closes a loop, as `add` would.
        let mut trial = model.clone();
        for &(pred, succ, link_type, lag) in links {
            can_connect(&trial, pred, succ)?;
            trial.links.push(Link {
                pred,
                succ,
                link_type,
                lag,
                driving_path: false,
            });
        }
    }
    model
        .links
        .extend(links.iter().map(|&(pred, succ, link_type, lag)| Link {
            pred,
            succ,
            link_type,
            lag,
            driving_path: false,
        }));
    model.rebuild_links()
}

/// Add a relationship as [`add`] does, but keep one that closes a loop:
/// the reference scheduler stores those and its schedule decides (a loop
/// through a level of effort schedules). Used when replaying stored data.
pub fn add_keeping_loops(
    model: &mut Model,
    pred: usize,
    succ: usize,
    link_type: LinkType,
    lag: i64,
) -> Result<(), String> {
    if pred >= model.activities.len() || succ >= model.activities.len() || pred == succ {
        return Err("An activity cannot be its own predecessor.".into());
    }
    if model
        .links
        .iter()
        .any(|l| l.pred == pred && l.succ == succ && l.link_type == link_type)
    {
        return Err("This relationship type already exists between these activities.".into());
    }
    model.links.push(Link {
        pred,
        succ,
        link_type,
        lag,
        driving_path: false,
    });
    model.rebuild_links()
}

pub fn edit(
    model: &mut Model,
    index: usize,
    link_type: LinkType,
    hours: f64,
) -> Result<(), String> {
    if !hours.is_finite() || !(-100000.0..=100000.0).contains(&hours) {
        return Err("Relationship lag must be between -100000 and 100000 hours.".into());
    }
    let link = model
        .links
        .get(index)
        .ok_or("The relationship no longer exists.")?;
    if model.links.iter().enumerate().any(|(i, l)| {
        i != index && l.pred == link.pred && l.succ == link.succ && l.link_type == link_type
    }) {
        return Err("This relationship type already exists between these activities.".into());
    }
    let link = &mut model.links[index];
    link.link_type = link_type;
    link.lag = (hours * 60.).round() as i64;
    Ok(())
}

pub fn remove(model: &mut Model, index: usize) -> Result<(), String> {
    if index >= model.links.len() {
        return Err("The relationship no longer exists.".into());
    }
    model.links.remove(index);
    model.rebuild_links()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn add_many_checks_like_add_and_rebuilds_once() {
        let base = crate::starter().model;
        let mut model = base.clone();
        add_many(
            &mut model,
            &[(0, 13, LinkType::SS, 60), (1, 13, LinkType::FF, 0)],
        )
        .unwrap();
        assert_eq!(model.links.len(), base.links.len() + 2);
        assert!(model.activities[13].preds.len() >= 2);
        for bad in [
            vec![(13, 0, LinkType::FS, 0)],
            vec![(2, 2, LinkType::FS, 0)],
            vec![(0, 1, LinkType::FS, 0)],
            vec![(0, 5, LinkType::SS, 0), (0, 5, LinkType::SS, 0)],
        ] {
            let mut m = base.clone();
            assert!(add_many(&mut m, &bad).is_err(), "{bad:?}");
            assert_eq!(m.links, base.links, "nothing added on a refusal");
        }
        assert!(acyclic_with(&base, &[(0, 13)]));
        assert!(!acyclic_with(&base, &[(13, 0)]));
    }
    #[test]
    fn endpoint_gestures_create_all_four_types() {
        let mut model = crate::starter().model;
        model.links.clear();
        model.rebuild_links().unwrap();
        for (i, (a, b, t)) in [
            (Edge::Finish, Edge::Start, LinkType::FS),
            (Edge::Start, Edge::Start, LinkType::SS),
            (Edge::Finish, Edge::Finish, LinkType::FF),
            (Edge::Start, Edge::Finish, LinkType::SF),
        ]
        .into_iter()
        .enumerate()
        {
            add(&mut model, i, i + 1, kind(a, b), 0).unwrap();
            assert_eq!(model.links[i].link_type, t);
        }
        assert_eq!(model.activities[2].preds, vec![1]);
        assert_eq!(model.activities[2].succs, vec![2]);
    }
    #[test]
    fn invalid_relationships_never_change_the_network() {
        let mut model = crate::starter().model;
        let l = model.links[0].clone();
        add(&mut model, l.pred, l.succ, LinkType::SS, 0).unwrap();
        let before = model.clone();
        assert!(add(&mut model, 0, 0, LinkType::FS, 0).is_err());
        assert!(add(&mut model, l.pred, l.succ, LinkType::SS, 0).is_err());
        assert!(add(&mut model, l.succ, l.pred, LinkType::FF, 0).is_err());
        assert_eq!(model, before);
        assert!(edit(&mut model, 0, LinkType::SS, 0.).is_err());
        assert_eq!(model, before);
        assert!(edit(&mut model, 0, LinkType::FS, f64::NAN).is_err());
        assert_eq!(model, before);
    }
}
