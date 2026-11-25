//! Circular relationships made understandable: the analysis (`analysis`),
//! the Circular Relationships dialog (`dialog`) and the pieces that open it
//! from scheduling, Check Schedule and file open.
use super::*;

pub(crate) mod analysis;
mod dialog;
#[cfg(test)]
mod tests;

use analysis::LoopReport;

/// The loop shown by Show Loop: its activities replace the table rows and
/// its relationship lines are drawn heavy on the Gantt chart.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LoopView {
    pub members: BTreeSet<usize>,
    /// (predecessor, successor) activity pairs of the loop's ties.
    pub links: BTreeSet<(usize, usize)>,
}

#[derive(Default)]
pub(crate) struct State {
    pub open: bool,
    pub report: LoopReport,
    /// Fingerprint of the relationships the report was made from.
    pub fingerprint: u64,
    pub group: usize,
    pub row: usize,
    /// Go To uses the successor end of the selected row.
    pub go_succ: bool,
    pub view: Option<LoopView>,
    /// The non-blocking offer after a file with loops opens.
    pub prompt: bool,
    /// A line under the lists: what the last action did.
    pub note: String,
    /// The loops as text, from the last Check Schedule run.
    pub check_lines: Vec<String>,
}

/// A cheap fingerprint of the relationship list, to notice changes made
/// outside the dialog (undo, redo, edits in the table).
pub(crate) fn fingerprint(model: &Model) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |v: u64| {
        h ^= v;
        h = h.wrapping_mul(0x0100_0000_01b3);
    };
    mix(model.links.len() as u64);
    for l in &model.links {
        mix(l.pred as u64);
        mix(l.succ as u64);
        mix(l.link_type as u64);
        mix(l.lag as u64);
    }
    h
}
