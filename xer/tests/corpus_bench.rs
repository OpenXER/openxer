//! Import, schedule and export timing for the largest corpus project.
//! Run with `cargo test -p openxer-xer --release --test corpus_bench -- --ignored --nocapture`.
//! `OPENXER_BENCH_XER` selects the file; by default it is `369.xer` in the
//! private corpus (see `corpus_roundtrip.rs`).
use openxer_engine::scheduler;
use openxer_xer::{Document, ProjectData};
use std::{path::PathBuf, time::Instant};

#[test]
#[ignore = "timing run on a private corpus file; run in release with --ignored"]
fn import_schedule_export_timing() {
    let path = std::env::var_os("OPENXER_BENCH_XER")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("OPENXER_CORPUS")
                .map(PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("OPENXER_CORPUS is not set"))
                .join("369.xer")
        });
    let Ok(bytes) = std::fs::read(&path) else {
        println!(
            "SKIPPED: {} not found (set OPENXER_BENCH_XER)",
            path.display()
        );
        return;
    };
    let time = |label: &str, start: Instant| {
        println!("{label}: {:.1} ms", start.elapsed().as_secs_f64() * 1000.0);
    };
    let start = Instant::now();
    let document = Document::from_bytes(&bytes).unwrap();
    time("parse", start);
    let pid = document.projects()[0].0;
    let start = Instant::now();
    let mut project = ProjectData::from_document(document, pid, "bench".into()).unwrap();
    time("map to model", start);
    println!(
        "{} activities, {} relationships",
        project.model.activities.len(),
        project.model.links.len()
    );
    let start = Instant::now();
    project.refresh_derived_durations();
    time("refresh derived durations", start);
    let start = Instant::now();
    scheduler::schedule_with_resources(&mut project.model, &project.resources).unwrap();
    time("schedule", start);
    let start = Instant::now();
    let exported = project.updated_document();
    time("updated_document", start);
    let start = Instant::now();
    let out = exported.to_bytes().unwrap();
    time("encode", start);
    println!("{} bytes written", out.len());
}
