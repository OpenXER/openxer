//! A printable report: a title block, summary facts and sections of
//! tables. Schedule Comparison, Project Checker and the Report Wizard build
//! one of these; `html`, `pdf`, `csv` and the Excel adapter write it out.
use openxer_engine::calendar::DAY;

/// One table cell. Numbers and dates keep their value for Excel; every cell
/// carries the text shown on screen and in HTML and PDF.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Text(String),
    Number {
        value: f64,
        text: String,
    },
    /// `serial` is days since 1899-12-30 (the spreadsheet date serial).
    Date {
        serial: f64,
        text: String,
    },
}

impl Cell {
    pub fn text(&self) -> &str {
        match self {
            Cell::Text(s) => s,
            Cell::Number { text, .. } | Cell::Date { text, .. } => text,
        }
    }
    pub fn number(value: f64, text: impl Into<String>) -> Self {
        Cell::Number {
            value,
            text: text.into(),
        }
    }
    /// A date cell from minutes after the project's origin day.
    pub fn date(origin_serial: i64, minute: Option<i64>, text: impl Into<String>) -> Self {
        let text = text.into();
        match minute {
            Some(m) => Cell::Date {
                serial: (origin_serial * DAY + m) as f64 / DAY as f64,
                text,
            },
            None => Cell::Text(text),
        }
    }
}

impl From<&str> for Cell {
    fn from(s: &str) -> Self {
        Cell::Text(s.to_string())
    }
}

impl From<String> for Cell {
    fn from(s: String) -> Self {
        Cell::Text(s)
    }
}

/// How a row is marked in every output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tone {
    #[default]
    Normal,
    Added,
    Deleted,
    Changed,
    /// A failed check or a slip.
    Bad,
    /// A passed check or a gain.
    Good,
    /// A grouping band at nesting level 0, 1, ...
    Band(u8),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub title: String,
    /// Relative width in characters.
    pub width: f32,
    pub right: bool,
}

impl Column {
    pub fn left(title: &str, width: f32) -> Self {
        Self {
            title: title.into(),
            width,
            right: false,
        }
    }
    pub fn right(title: &str, width: f32) -> Self {
        Self {
            title: title.into(),
            width,
            right: true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub cells: Vec<Cell>,
    pub tone: Tone,
}

impl Row {
    pub fn new(cells: Vec<Cell>, tone: Tone) -> Self {
        Self { cells, tone }
    }
    /// A band row: its label in the first cell, the other cells blank or
    /// holding totals.
    pub fn band(level: u8, label: impl Into<String>, columns: usize) -> Self {
        let mut cells = vec![Cell::Text(label.into())];
        cells.resize(columns.max(1), Cell::Text(String::new()));
        Self {
            cells,
            tone: Tone::Band(level),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    /// Short name, used for the spreadsheet tab.
    pub name: String,
    /// Line printed above the table.
    pub caption: String,
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    pub heading: String,
    pub paragraphs: Vec<String>,
    pub tables: Vec<Table>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub title: String,
    /// Lines under the title ("Comparing ... to ...", run by, date).
    pub header: Vec<String>,
    pub sections: Vec<Section>,
    /// Printed at the bottom of every page and the end of the HTML.
    pub footer: String,
}

impl Report {
    /// Every table in order, for spreadsheets and CSV.
    pub fn tables(&self) -> impl Iterator<Item = (&Section, &Table)> {
        self.sections
            .iter()
            .flat_map(|s| s.tables.iter().map(move |t| (s, t)))
    }
}

/// The user running the report, for the header line.
pub fn user_name() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "user".into())
}

/// Now in the user's date format, for the header line.
pub fn run_stamp() -> String {
    let now = chrono::Local::now().naive_local();
    let epoch = chrono::NaiveDate::from_ymd_opt(1899, 12, 30)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    crate::format::date(0, Some((now - epoch).num_minutes()))
}

/// The output formats a report can be sent to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    Html,
    Excel,
    Pdf,
    Csv,
}

impl Output {
    pub const ALL: [Output; 4] = [Output::Html, Output::Excel, Output::Pdf, Output::Csv];
    pub fn caption(self) -> &'static str {
        match self {
            Output::Html => "HTML file",
            Output::Excel => "Excel file",
            Output::Pdf => "PDF file",
            Output::Csv => "CSV file",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Output::Html => "html",
            Output::Excel => "xlsx",
            Output::Pdf => "pdf",
            Output::Csv => "csv",
        }
    }
    pub fn key(self) -> &'static str {
        self.extension()
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|o| o.key() == key)
    }
    /// The format a file name asks for, by its extension.
    pub fn from_path(path: &std::path::Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "htm" | "html" => Some(Output::Html),
            "xlsx" => Some(Output::Excel),
            "pdf" => Some(Output::Pdf),
            "csv" | "txt" => Some(Output::Csv),
            _ => None,
        }
    }
}

/// Write `report` to `path` in `format`. The file is written whole or not
/// at all (a temporary file is renamed into place).
pub fn write(report: &Report, format: Output, path: &std::path::Path) -> Result<(), String> {
    let bytes = match format {
        Output::Html => super::html::write(report).into_bytes(),
        Output::Csv => super::csv::file_bytes(super::csv::write(report)),
        Output::Pdf => super::pdf::write(report),
        Output::Excel => super::xlsx::bytes(report)?,
    };
    super::rpt_run::write_file(path, &bytes)
}

/// Default folder for saved reports: the user's Documents folder.
pub fn documents() -> std::path::PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|h| std::path::PathBuf::from(h).join("Documents"))
        .filter(|p| p.is_dir())
        .unwrap_or_else(std::env::temp_dir)
}

/// The short note a results window shows after writing a report.
pub fn saved_note(path: &std::path::Path) -> String {
    format!(
        "Report saved: {}",
        path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().to_string()
        )
    )
}
