//! Project editing rules shared by every front end: the desktop app, the
//! command line and the MCP server call these same functions, so an edit
//! behaves the same wherever it is made. Nothing here draws anything or keeps
//! user interface state; callers do their own follow-ups (undo history,
//! prompts) from what the functions return.
pub mod activity;
pub mod calendars;
pub mod curves;
pub mod dissolve;
pub mod documents;
pub mod earned_value;
pub mod fields;
pub mod financial_periods;
pub mod funding;
pub mod global_change;
pub mod new_activity;
pub mod notebooks;
pub mod period_units;
pub mod progress;
pub mod project_codes;
pub mod project_dates;
pub mod project_records;
pub mod records;
pub mod relationships;
pub mod resources;
pub mod rich_memo;
pub mod steps;
pub mod tables;
pub mod thresholds;
pub mod udf;
pub mod wbs;
