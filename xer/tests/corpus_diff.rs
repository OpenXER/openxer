//! End to end: each corpus XER is read, scheduled, and compared with the
//! expected results its model fixture holds (the fixtures `oxdiff engine`
//! compares).
use openxer_engine::{
    fixture,
    model::{Activity, TaskType},
    scheduler,
};
use openxer_xer::{Document, ProjectData};
use std::fs;
fn is_open_loe(a: &Activity) -> bool {
    a.task_type == TaskType::Loe && !a.is_completed()
}
#[test]
fn xer_corpus_matches_the_model_fixtures() {
    let Some(dir) = fixture::fixture_path("models") else {
        return;
    };
    // The corpus is client data outside the repository.
    let Some(corpus) = std::env::var_os("OPENXER_CORPUS").map(std::path::PathBuf::from) else {
        println!("SKIPPED: set OPENXER_CORPUS to the XER corpus folder");
        return;
    };
    let mut total = 0;
    let mut failed = vec![];
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|x| x != "txt") {
            continue;
        }
        let dump = fixture::parse_model(&fs::read_to_string(&path).unwrap()).unwrap();
        let xer = corpus.join(format!("{}.xer", dump.name));
        let mut loaded = ProjectData::from_document(
            Document::read(&xer).unwrap(),
            dump.model.project.proj_id,
            dump.name.clone(),
        )
        .unwrap();
        scheduler::schedule(&mut loaded.model).unwrap();
        let delta = (loaded.model.origin_serial - dump.model.origin_serial) * 1440;
        for e in &dump.expected {
            let a = loaded
                .model
                .activities
                .iter()
                .find(|a| a.task_code == e.key)
                .unwrap();
            for (k, want) in &e.fields {
                let (got, is_date) = match k.as_str() {
                    "es" => (a.early_start, true),
                    "ef" => (a.early_finish, true),
                    "ls" => (a.late_start, true),
                    "lf" => (a.late_finish, true),
                    "tf" => (a.total_float, false),
                    "ff" => (a.free_float, false),
                    "drive" => (Some(i64::from(a.driving_path)), false),
                    // The model fixtures leave an open LOE at zero; the
                    // application stores the span of its remaining dates
                    // (checked against the reference in corpus_parity).
                    "rem" if is_open_loe(a) => {
                        let span = a
                            .restart
                            .zip(a.reend)
                            .map(|(s, e)| loaded.model.calendars[a.cal].between(s, e));
                        assert_eq!(
                            (*want, Some(a.rem)),
                            (Some(0), span),
                            "{} {}",
                            dump.name,
                            e.key
                        );
                        continue;
                    }
                    "rem" => (Some(a.rem), false),
                    _ => continue,
                };
                let got = got.map(|v| v + if is_date { delta } else { 0 });
                total += 1;
                if got != *want && failed.len() < 30 {
                    failed.push(format!(
                        "{} {} {k}: fixture {want:?}, from XER {got:?}",
                        dump.name, e.key
                    ));
                }
            }
        }
    }
    println!("Checked {total} XER-scheduled outputs");
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
