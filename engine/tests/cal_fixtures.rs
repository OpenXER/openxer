#[test]
fn calendar_fixtures() {
    let Some(path) = openxer_engine::fixture::fixture_path("cal.txt") else {
        return;
    };
    let r = openxer_engine::fixture::run_file(path).unwrap();
    assert_eq!(r.exact, r.total, "{:?}", r.mismatches);
}
