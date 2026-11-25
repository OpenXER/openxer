//! A report as an Excel workbook: a Summary sheet with the title lines and
//! paragraphs, then one sheet per table with typed numbers and dates.
use super::doc::{Cell, Report, Tone};
use openxer_interchange::report_xlsx::{report_bytes, ReportCell, ReportRow, ReportSheet};

fn fill(tone: Tone) -> (Option<u32>, Option<u32>, bool) {
    match tone {
        Tone::Normal => (None, None, false),
        Tone::Added | Tone::Good => (Some(0xDFF5DF), None, false),
        Tone::Deleted | Tone::Bad => (Some(0xF9DADA), None, false),
        Tone::Changed => (Some(0xFFF7CC), None, false),
        Tone::Band(level) => {
            let (background, text) = crate::theme::report_band(level);
            (Some(background), Some(text), true)
        }
    }
}

fn cell(c: &Cell) -> ReportCell {
    match c {
        Cell::Text(s) if s.is_empty() => ReportCell::Empty,
        Cell::Text(s) => ReportCell::Text(s.clone()),
        Cell::Number { value, .. } => ReportCell::Number(*value),
        Cell::Date { serial, .. } => ReportCell::Date(*serial),
    }
}

pub fn sheets(report: &Report) -> Vec<ReportSheet> {
    let mut summary = ReportSheet {
        name: "Summary".into(),
        title: std::iter::once(report.title.clone())
            .chain(report.header.iter().cloned())
            .collect(),
        header: vec!["Section".into(), "Notes".into()],
        widths: vec![36., 100.],
        rows: vec![],
    };
    for s in &report.sections {
        let tables: Vec<String> = s
            .tables
            .iter()
            .map(|t| format!("{} ({} rows)", t.name, t.rows.len()))
            .collect();
        let mut notes = s.paragraphs.clone();
        if !tables.is_empty() {
            notes.push(format!("Sheets: {}", tables.join(", ")));
        }
        summary.rows.push(ReportRow {
            cells: vec![
                ReportCell::Text(s.heading.clone()),
                ReportCell::Text(notes.join(" ")),
            ],
            ..Default::default()
        });
    }
    let mut out = vec![summary];
    for (section, table) in report.tables() {
        let mut title = vec![section.heading.clone()];
        if !table.caption.is_empty() {
            title.push(table.caption.clone());
        }
        out.push(ReportSheet {
            name: if table.name.is_empty() {
                section.heading.clone()
            } else {
                table.name.clone()
            },
            title,
            header: table.columns.iter().map(|c| c.title.clone()).collect(),
            widths: table
                .columns
                .iter()
                .map(|c| (c.width as f64).clamp(6., 80.))
                .collect(),
            rows: table
                .rows
                .iter()
                .map(|r| {
                    let (fill, ink, bold) = fill(r.tone);
                    ReportRow {
                        cells: r.cells.iter().map(cell).collect(),
                        fill,
                        ink,
                        bold,
                    }
                })
                .collect(),
        });
    }
    out
}

pub fn bytes(report: &Report) -> Result<Vec<u8>, String> {
    report_bytes(&sheets(report), "dd-mmm-yy hh:mm")
}
