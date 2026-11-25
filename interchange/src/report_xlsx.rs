//! Generic report workbooks: one sheet per report table, title lines on
//! top, a bold header row with frozen panes and an autofilter, typed numbers
//! and real dates, and a fill per marked row. Used by the desktop reports
//! (Schedule Comparison, Project Checker, Report Wizard).
use rust_xlsxwriter::{Color, Format, FormatBorder, Workbook, XlsxError};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq)]
pub enum ReportCell {
    Empty,
    Text(String),
    Number(f64),
    /// Days since 1899-12-30, written as a date.
    Date(f64),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReportRow {
    pub cells: Vec<ReportCell>,
    /// Background fill as 0xRRGGBB.
    pub fill: Option<u32>,
    /// Text color as 0xRRGGBB.
    pub ink: Option<u32>,
    pub bold: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReportSheet {
    pub name: String,
    /// Lines above the table; the first is written bold and larger.
    pub title: Vec<String>,
    pub header: Vec<String>,
    /// Column widths in characters.
    pub widths: Vec<f64>,
    pub rows: Vec<ReportRow>,
}

fn err(e: XlsxError) -> String {
    format!("The workbook could not be written: {e}")
}

/// A sheet name Excel accepts: no `[]:*?/\`, at most 31 characters, unique
/// in the workbook.
fn sheet_name(name: &str, used: &mut BTreeSet<String>) -> String {
    let clean: String = name
        .chars()
        .map(|c| if "[]:*?/\\".contains(c) { ' ' } else { c })
        .collect::<String>()
        .trim()
        .trim_matches('\'')
        .to_string();
    let base: String = if clean.is_empty() {
        "Sheet".into()
    } else {
        clean.chars().take(31).collect()
    };
    let mut candidate = base.clone();
    let mut n = 2;
    while used.contains(&candidate.to_lowercase()) {
        let suffix = format!(" ({n})");
        let keep = 31 - suffix.chars().count();
        candidate = format!("{}{suffix}", base.chars().take(keep).collect::<String>());
        n += 1;
    }
    used.insert(candidate.to_lowercase());
    candidate
}

/// The workbook as .xlsx bytes.
pub fn report_bytes(sheets: &[ReportSheet], date_format: &str) -> Result<Vec<u8>, String> {
    let mut book = Workbook::new();
    let mut used = BTreeSet::new();
    let header = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0xF0F0F0))
        .set_border_bottom(FormatBorder::Thin)
        .set_text_wrap();
    let title = Format::new().set_bold().set_font_size(14);
    let sub = Format::new().set_font_color(Color::RGB(0x404040));
    for sheet in sheets {
        let ws = book.add_worksheet();
        ws.set_name(sheet_name(&sheet.name, &mut used))
            .map_err(err)?;
        let mut row = 0u32;
        for (i, line) in sheet.title.iter().enumerate() {
            ws.write_string_with_format(row, 0, line, if i == 0 { &title } else { &sub })
                .map_err(err)?;
            row += 1;
        }
        if !sheet.title.is_empty() {
            row += 1;
        }
        let header_row = row;
        for (c, h) in sheet.header.iter().enumerate() {
            ws.write_string_with_format(row, c as u16, h, &header)
                .map_err(err)?;
        }
        for (c, w) in sheet.widths.iter().enumerate() {
            ws.set_column_width(c as u16, *w).map_err(err)?;
        }
        row += 1;
        for r in &sheet.rows {
            let mut base = Format::new();
            if let Some(fill) = r.fill {
                base = base.set_background_color(Color::RGB(fill));
            }
            if let Some(ink) = r.ink {
                base = base.set_font_color(Color::RGB(ink));
            }
            if r.bold {
                base = base.set_bold();
            }
            let date = base.clone().set_num_format(date_format);
            // "#,##0.##" would show a whole number as "50." in Excel.
            let whole = base.clone().set_num_format("#,##0");
            let number = base.clone().set_num_format("#,##0.0#");
            let width = sheet.header.len().max(r.cells.len());
            for c in 0..width {
                let col = c as u16;
                match r.cells.get(c).unwrap_or(&ReportCell::Empty) {
                    ReportCell::Empty => {
                        if r.fill.is_some() {
                            ws.write_blank(row, col, &base).map_err(err)?;
                        }
                    }
                    ReportCell::Text(s) => {
                        ws.write_string_with_format(row, col, s, &base)
                            .map_err(err)?;
                    }
                    ReportCell::Number(v) => {
                        let format = if v.fract() == 0.0 { &whole } else { &number };
                        ws.write_number_with_format(row, col, *v, format)
                            .map_err(err)?;
                    }
                    ReportCell::Date(v) => {
                        ws.write_number_with_format(row, col, *v, &date)
                            .map_err(err)?;
                    }
                }
            }
            row += 1;
        }
        if !sheet.header.is_empty() {
            ws.set_freeze_panes(header_row + 1, 0).map_err(err)?;
            ws.autofilter(
                header_row,
                0,
                row.saturating_sub(1).max(header_row + 1),
                sheet.header.len() as u16 - 1,
            )
            .map_err(err)?;
        }
    }
    if sheets.is_empty() {
        book.add_worksheet();
    }
    book.save_to_buffer().map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_workbook_reads_back_with_types_and_unique_names() {
        let sheet = |name: &str| ReportSheet {
            name: name.into(),
            title: vec!["Title".into(), "Sub".into()],
            header: vec!["Text".into(), "Number".into(), "Date".into()],
            widths: vec![20., 10., 12.],
            rows: vec![ReportRow {
                cells: vec![
                    ReportCell::Text("a".into()),
                    ReportCell::Number(2.5),
                    ReportCell::Date(46000.5),
                ],
                fill: Some(0xDFF5DF),
                ink: None,
                bold: false,
            }],
        };
        let bytes = report_bytes(
            &[sheet("Changes: a/b"), sheet("Changes: a/b")],
            "dd-mmm-yy hh:mm",
        )
        .unwrap();
        let path = std::env::temp_dir().join(format!("report-xlsx-{}.xlsx", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        let wb = crate::xlsx_import::read_workbook(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(wb.sheets.len(), 2);
        assert_eq!(wb.sheets[0].name, "Changes  a b");
        assert_eq!(wb.sheets[1].name, "Changes  a b (2)");
        let s = &wb.sheets[0];
        assert_eq!(s.cell(0, 0).text(), "Title");
        assert_eq!(s.cell(3, 1).text(), "Number");
        assert_eq!(s.cell(4, 0).text(), "a");
        assert_eq!(*s.cell(4, 1), crate::xlsx_import::Cell::Number(2.5));
        assert_eq!(*s.cell(4, 2), crate::xlsx_import::Cell::Date(46000.5));
    }

    /// Regression: numbers used `#,##0.##`, which Excel shows as "50." for a
    /// whole number. Whole numbers now get a format without decimals.
    #[test]
    fn whole_numbers_have_no_trailing_decimal_point() {
        use calamine::Xlsx;
        let sheet = ReportSheet {
            name: "Numbers".into(),
            header: vec!["N".into()],
            rows: [50.0, 2.5]
                .iter()
                .map(|v| ReportRow {
                    cells: vec![ReportCell::Number(*v)],
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let bytes = report_bytes(&[sheet], "dd-mmm-yy").unwrap();
        let path =
            std::env::temp_dir().join(format!("report-xlsx-fmt-{}.xlsx", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        let mut book: Xlsx<_> = calamine::open_workbook(&path).unwrap();
        let styles = book.worksheet_style("Numbers").unwrap();
        drop(book);
        std::fs::remove_file(&path).unwrap();
        let format = |row: u32| {
            styles
                .get((row, 0))
                .and_then(|s| s.number_format.clone())
                .map(|f| f.format_code)
                .unwrap_or_default()
        };
        assert_eq!(format(1), "#,##0");
        assert!(!format(2).ends_with(".##"), "{}", format(2));
    }

    /// Text that looks like a formula (names come from untrusted XER files)
    /// is written as a plain string cell, never as a formula.
    #[test]
    fn formula_like_text_stays_a_string() {
        use calamine::{Reader, Xlsx};
        let texts = ["=HYPERLINK(\"http://x\",\"y\")", "+1+1", "-2", "@SUM(A1)"];
        let sheet = ReportSheet {
            name: "Injection".into(),
            title: vec![texts[0].into()],
            header: vec![texts[1].into()],
            widths: vec![20.],
            rows: texts
                .iter()
                .map(|t| ReportRow {
                    cells: vec![ReportCell::Text(t.to_string())],
                    ..Default::default()
                })
                .collect(),
        };
        let bytes = report_bytes(&[sheet], "dd-mmm-yy").unwrap();
        let path =
            std::env::temp_dir().join(format!("report-xlsx-inj-{}.xlsx", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        let mut book: Xlsx<_> = calamine::open_workbook(&path).unwrap();
        let formulas = book.worksheet_formula("Injection").unwrap();
        assert!(
            formulas.used_cells().all(|(_, _, f)| f.is_empty()),
            "no formula cells"
        );
        drop(book);
        let wb = crate::xlsx_import::read_workbook(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        let s = &wb.sheets[0];
        for (n, t) in texts.iter().enumerate() {
            assert_eq!(
                *s.cell(3 + n, 0),
                crate::xlsx_import::Cell::Text(t.to_string())
            );
        }
    }
}
