//! Link Activities and Unlink Activities of the table context menu and the
//! Edit menu: tie the selected activities together in table order, or cut
//! every tie among them.
use openxer_engine::model::{Link, LinkType, Model};

/// What Link Activities did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LinkOutcome {
    /// New finish to start relationships.
    pub added: usize,
    /// Pairs already tied finish to start.
    pub existing: usize,
    /// Pairs left alone because the tie would close a loop.
    pub circular: usize,
}

/// Ties each activity in `rows` (model indices, in table order) to the next
/// one with a finish to start relationship and no lag. A pair that already
/// has a finish to start tie, or whose tie would make a loop, is skipped.
pub fn link_in_order(model: &mut Model, rows: &[usize]) -> Result<LinkOutcome, String> {
    let mut out = LinkOutcome::default();
    for pair in rows.windows(2) {
        let (pred, succ) = (pair[0], pair[1]);
        if pred == succ {
            continue;
        }
        if model
            .links
            .iter()
            .any(|l| l.pred == pred && l.succ == succ && l.link_type == LinkType::FS)
        {
            out.existing += 1;
            continue;
        }
        if crate::relationships::can_connect(model, pred, succ).is_err() {
            out.circular += 1;
            continue;
        }
        model.links.push(Link {
            pred,
            succ,
            link_type: LinkType::FS,
            lag: 0,
            driving_path: false,
        });
        // Later pairs check loops against the ties added so far.
        model.rebuild_links()?;
        out.added += 1;
    }
    Ok(out)
}

/// Removes every relationship whose predecessor and successor are both in
/// `rows`. Returns how many were removed.
pub fn unlink_among(model: &mut Model, rows: &[usize]) -> Result<usize, String> {
    let before = model.links.len();
    // A membership table, so Select All then Unlink is linear in links.
    let mut chosen = vec![false; model.activities.len()];
    for &r in rows {
        if let Some(c) = chosen.get_mut(r) {
            *c = true;
        }
    }
    let among = |i: usize| chosen.get(i).copied().unwrap_or(false);
    model.links.retain(|l| !(among(l.pred) && among(l.succ)));
    let removed = before - model.links.len();
    if removed > 0 {
        model.rebuild_links()?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bare() -> Model {
        let mut model = openxer_xer::starter().model;
        model.links.clear();
        model.rebuild_links().unwrap();
        model
    }

    #[test]
    fn link_ties_rows_in_the_given_order_and_skips_existing_and_loops() {
        let mut model = bare();
        assert!(model.activities.len() >= 4);
        let out = link_in_order(&mut model, &[2, 0, 1]).unwrap();
        assert_eq!(
            out,
            LinkOutcome {
                added: 2,
                existing: 0,
                circular: 0
            }
        );
        let ties: Vec<(usize, usize, LinkType, i64)> = model
            .links
            .iter()
            .map(|l| (l.pred, l.succ, l.link_type, l.lag))
            .collect();
        assert_eq!(ties, vec![(2, 0, LinkType::FS, 0), (0, 1, LinkType::FS, 0)]);
        assert_eq!(model.activities[1].preds.len(), 1);
        // Again: nothing new. Reversed: 1 -> 0 would close 0 -> 1.
        let again = link_in_order(&mut model, &[2, 0, 1]).unwrap();
        assert_eq!((again.added, again.existing), (0, 2));
        let reversed = link_in_order(&mut model, &[1, 0, 3]).unwrap();
        assert_eq!(
            reversed,
            LinkOutcome {
                added: 1,
                existing: 0,
                circular: 1
            }
        );
        assert_eq!(model.links.len(), 3);
    }

    #[test]
    fn unlink_removes_only_ties_inside_the_selection() {
        let mut model = bare();
        link_in_order(&mut model, &[0, 1, 2, 3]).unwrap();
        assert_eq!(model.links.len(), 3);
        assert_eq!(unlink_among(&mut model, &[1, 2, 3]).unwrap(), 2);
        let ties: Vec<(usize, usize)> = model.links.iter().map(|l| (l.pred, l.succ)).collect();
        assert_eq!(ties, vec![(0, 1)]);
        assert!(model.activities[2].preds.is_empty());
        assert_eq!(unlink_among(&mut model, &[2, 3]).unwrap(), 0);
    }
}
