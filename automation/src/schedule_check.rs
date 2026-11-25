//! Pure schedule diagnostics live in `openxer_xer::schedule_check`; this
//! module re-exports them so `analysis` and external callers share one copy.
pub use openxer_xer::schedule_check::*;
