//! The S-curve and earned hours workbooks read back: sheet names, headers,
//! totals, the stored values of the formulas and the chart part. On the
//! synthetic crews sample (`openxer_xer::workspace::sample::crews`).
use openxer_interchange::scurve_xlsx::{self, Style};
use openxer_interchange::xlsx_import::{read_workbook, Cell, Workbook};
use openxer_xer::scurve::{self, Breakdown, Group, Measure, Options, Period, Source};
use openxer_xer::ProjectData;

fn sample() -> ProjectData {
    let doc = openxer_xer::workspace::sample::crews(24., true);
    let mut p = ProjectData::from_document(doc, 1, "Crews".into()).unwrap();
    openxer_engine::scheduler::schedule_with_resources(&mut p.model, &p.resources).unwrap();
    p
}

fn read_back(name: &str, bytes: &[u8]) -> Workbook {
    let path = std::env::temp_dir().join(format!("scurve-{name}-{}.xlsx", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    let wb = read_workbook(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    wb
}

fn number(c: &Cell) -> f64 {
    match c {
        Cell::Number(v) | Cell::Date(v) => *v,
        other => panic!("not a number: {other:?}"),
    }
}

/// The zip package holds a part of this name (names are stored as plain
/// text in the archive's headers).
fn has_part(bytes: &[u8], part: &str) -> bool {
    bytes.windows(part.len()).any(|w| w == part.as_bytes())
}

#[test]
fn the_s_curve_workbook_has_its_sheets_totals_and_chart() {
    let p = sample();
    let c = scurve::curve(
        &[Source::new(&p)],
        Options {
            measure: Measure::Labor,
            period: Period::Week,
            breakdown: Breakdown::Resource,
            late: true,
            week_starts_sunday: true,
        },
    )
    .unwrap();
    let bytes = scurve_xlsx::curve_bytes(
        &c,
        &[("Generated".into(), "test".into())],
        &Style::default(),
    )
    .unwrap();
    assert!(has_part(&bytes, "xl/charts/chart1.xml"), "a native chart");
    let wb = read_back("curve", &bytes);
    let names: Vec<&str> = wb.sheets.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "S-Curve",
            "By Resource - Budgeted",
            "By Resource - Actual",
            "By Resource - Remaining",
            "Info"
        ]
    );
    let s = &wb.sheets[0];
    assert!(s.cell(0, 0).text().starts_with("S-Curve: "));
    let header: Vec<String> = (0..5).map(|k| s.cell(4, k).text()).collect();
    assert_eq!(
        header,
        [
            "Period Start",
            "Period End",
            "Budgeted Hours",
            "Actual Hours",
            "Remaining Hours"
        ]
    );
    let n = c.periods.len();
    let (first, last) = (5, 4 + n);
    // Period values are numbers; the running sums end at the totals.
    assert!(
        (number(s.cell(last, 5)) - 104.).abs() < 1e-6,
        "cum budgeted"
    );
    assert!((number(s.cell(last, 6)) - 24.).abs() < 1e-6, "cum actual");
    assert!(
        (number(s.cell(last, 8)) - 104.).abs() < 1e-6,
        "actual + remaining"
    );
    // A percent cell reads back as its shown number.
    assert!(
        (number(s.cell(last, 9)) - 100.).abs() < 1e-6,
        "100% budgeted: {:?}",
        s.cell(last, 9)
    );
    assert_eq!(s.cell(last + 1, 0).text(), "Total");
    assert!((number(s.cell(last + 1, 2)) - 104.).abs() < 1e-6);
    let sum: f64 = (first..=last).map(|r| number(s.cell(r, 2))).sum();
    assert!((sum - 104.).abs() < 1e-6);
    // Earned: one value, in the row of the data date.
    let k = first + c.earned_period.unwrap();
    assert!((number(s.cell(k, 12)) - 24.).abs() < 1e-6);
    let earned_cells = (first..=last)
        .filter(|r| *s.cell(*r, 12) != Cell::Empty)
        .count();
    assert_eq!(earned_cells, 1);
    // The budgeted matrix: a row per labor resource and the grand total.
    let m = &wb.sheets[1];
    assert_eq!(m.cell(3, 0).text(), "Resource ID");
    assert_eq!(m.cell(4, 0).text(), "ELEC");
    assert_eq!(m.cell(5, 0).text(), "PIPE");
    assert_eq!(m.cell(6, 0).text(), "Total");
    assert!(
        (number(m.cell(6, 2 + n)) - 104.).abs() < 1e-6,
        "grand total"
    );
    assert!((number(m.cell(5, 2 + n)) - 64.).abs() < 1e-6);
    let info = &wb.sheets[4];
    let rows: Vec<String> = (1..8).map(|r| info.cell(r, 0).text()).collect();
    assert!(rows.contains(&"Budgeted from".to_string()), "{rows:?}");
}

#[test]
fn the_total_only_curve_has_no_breakdown_sheets() {
    let p = sample();
    let c = scurve::curve(
        &[Source::new(&p)],
        Options {
            measure: Measure::Duration,
            period: Period::Month,
            breakdown: Breakdown::Total,
            late: false,
            week_starts_sunday: true,
        },
    )
    .unwrap();
    let bytes = scurve_xlsx::curve_bytes(&c, &[], &Style::default()).unwrap();
    let wb = read_back("duration", &bytes);
    let names: Vec<&str> = wb.sheets.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["S-Curve", "Info"]);
    assert_eq!(wb.sheets[0].cell(4, 2).text(), "Budgeted Days");
}

#[test]
fn the_earned_hours_workbook_lines_up_rows_for_subtraction() {
    let p = sample();
    let e = scurve::earned_hours(&Source::new(&p), Group::Resource).unwrap();
    let bytes = scurve_xlsx::earned_bytes(&e, &[], &Style::default()).unwrap();
    let wb = read_back("earned", &bytes);
    let names: Vec<&str> = wb.sheets.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Earned to date", "By activity", "Info"]);
    let s = &wb.sheets[0];
    let header: Vec<String> = (0..8).map(|k| s.cell(3, k).text()).collect();
    assert_eq!(
        header,
        [
            "Resource ID",
            "Resource Name",
            "Budgeted Units",
            "Earned Units",
            "Actual Units",
            "Remaining Units",
            "At Completion Units",
            "% Earned"
        ]
    );
    assert_eq!(s.cell(4, 0).text(), "ELEC");
    assert_eq!(s.cell(5, 0).text(), "PIPE");
    assert_eq!(s.cell(6, 0).text(), "Total");
    assert!((number(s.cell(6, 2)) - 104.).abs() < 1e-6);
    assert!((number(s.cell(6, 3)) - 24.).abs() < 1e-6, "earned");
    assert!((number(s.cell(6, 6)) - 104.).abs() < 1e-6, "at completion");
    let d = &wb.sheets[1];
    assert_eq!(d.cell(3, 0).text(), "Activity ID");
    let ids: Vec<String> = (4..7).map(|r| d.cell(r, 0).text()).collect();
    assert_eq!(ids, ["RB-1010", "RB-1070", "RB-1100"]);
    assert!((number(d.cell(4, 7)) - 24.).abs() < 1e-6);
}

#[test]
fn currency_formats_follow_the_base_currency() {
    assert_eq!(scurve_xlsx::excel_currency("$", 2), "\"$\"#,##0.00");
    assert_eq!(scurve_xlsx::excel_currency("", 0), "#,##0");
}
