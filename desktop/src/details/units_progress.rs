//! Progress by units and costs on the activity: the Units section of the
//! Status tab and the table's Budgeted, Actual, Remaining and Actual This
//! Period unit and cost columns. Both come here through `Edit::Figure` and
//! `Edit::ThisPeriod`, so a typed value lands in the same rows from either
//! place and from Paste, Fill Down, Global Change and Find/Replace.
//!
//! Where a value goes depends on the activity's assignments of that
//! resource type (`openxer_xer::edit::activity::units_route`): none sends
//! units to the activity's own `TASK` quantities; one goes through the
//! assignment setters (the Resources tab's rules and the project's
//! Calculations options); several are read-only, since the split between
//! them is not defined here.
use super::*;
use crate::grid::columns::{Measure, Phase};
use openxer_xer::edit::activity as shared;

impl App {
    /// Why a unit or cost figure cannot be edited on activity `i`. `phase`
    /// is `None` for the Actual This Period figure.
    pub(crate) fn figure_lock(
        &self,
        i: usize,
        measure: Measure,
        phase: Option<Phase>,
    ) -> Result<(), String> {
        shared::figure_lock(&self.project, i, measure, phase)
    }
}
