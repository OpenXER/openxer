#[test]
fn engine_fixtures() {
    let Some(dir) = openxer_engine::fixture::fixture_path("models") else {
        return;
    };
    let r = openxer_engine::fixture::run_models(dir, "engine").unwrap();
    assert_eq!(r.exact, r.total, "{:?}", r.mismatches);
}
#[test]
fn malformed_input_is_an_error() {
    for input in ["", "model", "cal x 0 0\nwk x 7 -\n", "op add x 1 = 2"] {
        assert!(openxer_engine::fixture::parse_model(input).is_err());
    }
}

#[test]
fn corrected_mapping_regressions() {
    let Some(dir) = openxer_engine::fixture::fixture_path("regressions") else {
        return;
    };
    let report = openxer_engine::fixture::run_models(dir, "engine").unwrap();
    assert!(report.total > 0);
    assert_eq!(report.exact, report.total, "{:?}", report.mismatches);
}

#[test]
fn corpus_models_round_trip_exactly() {
    let Some(dir) = openxer_engine::fixture::fixture_path("models") else {
        return;
    };
    let report = openxer_engine::fixture::run_models(dir, "roundtrip").unwrap();
    assert_eq!(report.exact, report.total, "{:?}", report.mismatches);
}
