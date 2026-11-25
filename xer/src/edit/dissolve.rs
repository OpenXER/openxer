//! Dissolve: remove an activity and tie each of its predecessors to each of
//! its successors, so the logic through it stays connected.
//!
//! For a predecessor relationship P (A to B) and a successor relationship S
//! (B to D) of the dissolved activity B, the new relationship A to D starts
//! where P starts (the finish of A for FS and FF, its start for SS and SF)
//! and ends where S ends (the start of D for FS and SS, its finish for FF
//! and SF). Every predecessor is tied to every successor. A pair that is
//! already related keeps its relationship as it is and gets no second one.
//! The new lag is zero, or with `retain_lag` the lag of P plus the lag of S
//! (B's own duration is not added). Nothing is rescheduled.
use super::relationships::{kind, Edge};
use crate::ProjectData;
use openxer_engine::model::{Link, LinkType, Model};

/// What a dissolve did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Dissolved {
    /// Activities removed.
    pub activities: usize,
    /// Relationships added.
    pub added: usize,
    /// Predecessor and successor pairs that were already related and kept
    /// their relationship.
    pub kept: usize,
}

/// The end of the predecessor a relationship of this type starts from.
fn from_edge(t: LinkType) -> Edge {
    match t {
        LinkType::SS | LinkType::SF => Edge::Start,
        LinkType::FS | LinkType::FF | LinkType::None => Edge::Finish,
    }
}

/// The end of the successor a relationship of this type drives.
fn to_edge(t: LinkType) -> Edge {
    match t {
        LinkType::FF | LinkType::SF => Edge::Finish,
        LinkType::FS | LinkType::SS | LinkType::None => Edge::Start,
    }
}

/// The type of the relationship that replaces `pred` (A to B) and `succ`
/// (B to D) when B is dissolved.
pub fn joined_type(pred: LinkType, succ: LinkType) -> LinkType {
    kind(from_edge(pred), to_edge(succ))
}

/// Whether activity `index` can be dissolved: it has at least one
/// predecessor and at least one successor.
pub fn can_dissolve(model: &Model, index: usize) -> bool {
    let other = |a: usize, b: usize| a == index && b != index;
    model.links.iter().any(|l| other(l.succ, l.pred))
        && model.links.iter().any(|l| other(l.pred, l.succ))
}

/// Dissolve the activities `task_ids`, one after another, as the module
/// comment describes. Each must have a predecessor and a successor when its
/// turn comes; otherwise the error names it and the caller keeps its copy
/// of the project from before (the activities before it are already
/// dissolved in `p`).
pub fn dissolve(
    p: &mut ProjectData,
    task_ids: &[i64],
    retain_lag: bool,
) -> Result<Dissolved, String> {
    let mut done = Dissolved::default();
    for &id in task_ids {
        let x = p
            .model
            .activities
            .iter()
            .position(|a| a.task_id == id)
            .ok_or("The activity is no longer available.")?;
        if !can_dissolve(&p.model, x) {
            return Err(format!(
                "Activity {} needs at least one predecessor and one successor to be dissolved.",
                p.model.activities[x].task_code
            ));
        }
        let preds: Vec<Link> = p
            .model
            .links
            .iter()
            .filter(|l| l.succ == x && l.pred != x)
            .cloned()
            .collect();
        let succs: Vec<Link> = p
            .model
            .links
            .iter()
            .filter(|l| l.pred == x && l.succ != x)
            .cloned()
            .collect();
        for pl in &preds {
            for sl in &succs {
                let (a, d) = (pl.pred, sl.succ);
                if a == d {
                    continue;
                }
                if p.model.links.iter().any(|l| l.pred == a && l.succ == d) {
                    done.kept += 1;
                    continue;
                }
                p.model.links.push(Link {
                    pred: a,
                    succ: d,
                    link_type: joined_type(pl.link_type, sl.link_type),
                    lag: if retain_lag { pl.lag + sl.lag } else { 0 },
                    driving_path: false,
                });
                done.added += 1;
            }
        }
        p.delete_activities(&[id])?;
        done.activities += 1;
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use LinkType::*;

    const DAY: i64 = 8 * 60;

    /// The starter project with no relationships but `links`, given as
    /// (predecessor, successor, type, lag) by Activity ID.
    fn project(links: &[(&str, &str, LinkType, i64)]) -> ProjectData {
        let mut p = crate::starter();
        let at = |p: &ProjectData, code: &str| index(p, code);
        let links: Vec<Link> = links
            .iter()
            .map(|&(a, b, link_type, lag)| Link {
                pred: at(&p, a),
                succ: at(&p, b),
                link_type,
                lag,
                driving_path: false,
            })
            .collect();
        p.model.links = links;
        p.model.rebuild_links().unwrap();
        p
    }

    fn index(p: &ProjectData, code: &str) -> usize {
        p.model
            .activities
            .iter()
            .position(|a| a.task_code == code)
            .unwrap_or_else(|| panic!("activity {code}"))
    }

    fn task_id(p: &ProjectData, code: &str) -> i64 {
        p.model.activities[index(p, code)].task_id
    }

    /// Every relationship as (predecessor, successor, type, lag), sorted.
    fn links(p: &ProjectData) -> Vec<(String, String, LinkType, i64)> {
        let code = |i: usize| p.model.activities[i].task_code.clone();
        let mut all: Vec<_> = p
            .model
            .links
            .iter()
            .map(|l| (code(l.pred), code(l.succ), l.link_type, l.lag))
            .collect();
        all.sort_by(|x, y| (&x.0, &x.1).cmp(&(&y.0, &y.1)));
        all
    }

    const A: &str = "RB-1010";
    const B: &str = "RB-1020";
    const D: &str = "RB-1030";

    /// Dissolve B in "A `p` B lag `pl`, B `s` D lag `sl`" and return the
    /// one relationship left.
    fn joined(p: LinkType, pl: i64, s: LinkType, sl: i64, retain: bool) -> (LinkType, i64) {
        let mut x = project(&[(A, B, p, pl), (B, D, s, sl)]);
        let id = task_id(&x, B);
        let done = dissolve(&mut x, &[id], retain).unwrap();
        assert_eq!(
            done,
            Dissolved {
                activities: 1,
                added: 1,
                kept: 0
            }
        );
        let left = links(&x);
        assert_eq!(left.len(), 1, "{left:?}");
        assert_eq!((left[0].0.as_str(), left[0].1.as_str()), (A, D));
        (left[0].2, left[0].3)
    }

    #[test]
    fn c1_fs_then_fs_gives_fs() {
        assert_eq!(joined(FS, 0, FS, 0, false), (FS, 0));
        assert_eq!(joined(FS, 0, FS, 0, true), (FS, 0));
    }

    #[test]
    fn c2_ss_then_fs_gives_ss() {
        assert_eq!(joined(SS, 0, FS, 0, false), (SS, 0));
    }

    #[test]
    fn c3_sf_then_fs_gives_ss() {
        assert_eq!(joined(SF, 0, FS, 0, false), (SS, 0));
    }

    #[test]
    fn c4_ff_then_ss_gives_fs() {
        assert_eq!(joined(FF, 0, SS, 0, false), (FS, 0));
    }

    #[test]
    fn c5_lag_is_dropped_without_retain_lag() {
        assert_eq!(joined(FS, 2 * DAY, FS, 3 * DAY, false), (FS, 0));
    }

    #[test]
    fn c6_retain_lag_adds_both_lags_and_not_the_duration() {
        assert_eq!(joined(FS, 2 * DAY, FS, 3 * DAY, true), (FS, 5 * DAY));
    }

    #[test]
    fn c9b_ss_then_ff_gives_sf_without_lag() {
        assert_eq!(joined(SS, 2 * DAY, FF, 3 * DAY, false), (SF, 0));
    }

    #[test]
    fn c9_ss_then_ff_gives_sf_with_both_lags() {
        assert_eq!(joined(SS, 2 * DAY, FF, 3 * DAY, true), (SF, 5 * DAY));
    }

    #[test]
    fn every_type_pair_takes_the_predecessor_start_and_the_successor_end() {
        for (p, s, t) in [
            (FS, FS, FS),
            (FS, SS, FS),
            (FS, FF, FF),
            (FS, SF, FF),
            (SS, FS, SS),
            (SS, SS, SS),
            (SS, FF, SF),
            (SS, SF, SF),
            (FF, FS, FS),
            (FF, SS, FS),
            (FF, FF, FF),
            (FF, SF, FF),
            (SF, FS, SS),
            (SF, SS, SS),
            (SF, FF, SF),
            (SF, SF, SF),
        ] {
            assert_eq!(joined_type(p, s), t, "{p:?} then {s:?}");
        }
    }

    #[test]
    fn c10_two_predecessors_and_two_successors_give_the_cross_product() {
        let (a1, a2, d1, d2) = ("RB-1000", "RB-1010", "RB-1030", "RB-1040");
        let mut p = project(&[
            (a1, B, FS, 0),
            (a2, B, SS, DAY),
            (B, d1, FS, 0),
            (B, d2, FF, 2 * DAY),
        ]);
        let id = task_id(&p, B);
        let done = dissolve(&mut p, &[id], true).unwrap();
        assert_eq!(
            done,
            Dissolved {
                activities: 1,
                added: 4,
                kept: 0
            }
        );
        let s = String::from;
        assert_eq!(
            links(&p),
            vec![
                (s(a1), s(d1), FS, 0),
                (s(a1), s(d2), FF, 2 * DAY),
                (s(a2), s(d1), SS, DAY),
                (s(a2), s(d2), SF, 3 * DAY),
            ]
        );
    }

    #[test]
    fn c11_an_existing_relationship_is_kept_and_not_doubled() {
        let mut p = project(&[(A, B, FS, 0), (B, D, FS, 0), (A, D, FS, DAY)]);
        let id = task_id(&p, B);
        let done = dissolve(&mut p, &[id], true).unwrap();
        assert_eq!(
            done,
            Dissolved {
                activities: 1,
                added: 0,
                kept: 1
            }
        );
        assert_eq!(links(&p), vec![(A.into(), D.into(), FS, DAY)]);
        // An existing relationship of another type is kept too.
        let mut p = project(&[(A, B, FS, 0), (B, D, FS, 0), (A, D, SS, 0)]);
        let id = task_id(&p, B);
        dissolve(&mut p, &[id], false).unwrap();
        assert_eq!(links(&p), vec![(A.into(), D.into(), SS, 0)]);
    }

    #[test]
    fn c7_c8_one_side_only_is_refused_and_changes_nothing() {
        for set in [vec![(A, B, FS, 0)], vec![(B, D, FS, 0)], vec![]] {
            let mut p = project(&set);
            let i = index(&p, B);
            assert!(!can_dissolve(&p.model, i), "{set:?}");
            let before = p.model.clone();
            let id = task_id(&p, B);
            assert!(dissolve(&mut p, &[id], false).is_err());
            assert_eq!(p.model, before);
        }
        let p = project(&[(A, B, FS, 0), (B, D, FS, 0)]);
        assert!(can_dissolve(&p.model, index(&p, B)));
    }

    #[test]
    fn dissolving_leaves_no_rows_of_the_activity_and_no_dates_change() {
        let mut p = project(&[(A, B, FS, 0), (B, D, FS, 0)]);
        let dates: Vec<_> = p
            .model
            .activities
            .iter()
            .filter(|a| a.task_code != B)
            .map(|a| (a.task_id, a.early_start, a.early_finish))
            .collect();
        let id = task_id(&p, B);
        dissolve(&mut p, &[id], false).unwrap();
        assert!(p.model.activities.iter().all(|a| a.task_id != id));
        let after: Vec<_> = p
            .model
            .activities
            .iter()
            .map(|a| (a.task_id, a.early_start, a.early_finish))
            .collect();
        assert_eq!(after, dates, "no reschedule");
        let doc = p.updated_document();
        let reread =
            ProjectData::from_document(crate::Document::parse(&doc.text()).unwrap(), 1, "x".into())
                .unwrap();
        assert_eq!(links(&reread), links(&p));
    }
}
