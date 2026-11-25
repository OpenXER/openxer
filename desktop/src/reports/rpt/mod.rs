//! Report definitions in the `.rpt` text format: the component
//! stream, the report model, data binding, the generation engine and the
//! writers.
pub mod data;
pub mod edit;
pub mod engine;
pub mod filter;
pub mod library;
#[cfg(test)]
mod library_gen;
pub mod model;
pub mod page;
#[cfg(test)]
mod run_tests;
pub mod store;
pub mod stream;
#[cfg(test)]
mod tests;
pub mod writers;

use crate::grid::columns::Index;
use openxer_xer::ProjectData;

/// Runs a definition on a project.
pub fn generate(
    stream: &stream::Stream,
    p: &ProjectData,
    ix: &Index,
    opts: &engine::Options,
) -> Result<engine::Generated, String> {
    let report = model::Report::from_stream(stream)?;
    let data = data::Data::new(p, ix, opts.now);
    Ok(engine::run(&report, &data, opts))
}
