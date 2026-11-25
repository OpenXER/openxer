//! OpenXER's schedule engine.
//!
//! Calendar arithmetic and the CPM passes are checked against stored results
//! of real projects and saved probe projects (`oxdiff`, see `engine/RULES.md`
//! for every rule, its test and its evidence). `features` adds leveling,
//! generalized LOE spans and multiple float paths; `RULES.md` states which of
//! these have no independent evidence.
//!
//! Dates and durations use `i64` minutes from a shared origin; nullable
//! dates use `Option<i64>`. This crate uses only the standard library.

pub mod calendar;
pub mod cpm;
pub mod features;
pub mod fixture;
pub mod model;
pub mod options;
pub mod scheduler;
