//! Spreadsheet import: read a workbook, detect its format (OpenXER modern, the
//! tables layout with XER field names, or a plain sheet with a header row), map columns by
//! header text, build a preview of every row, then apply it in one step.
//!
//! Nothing is applied while planning. Every non-empty row ends up in the plan as
//! unchanged, changed, new, delete, skipped (band rows) or error with a reason.
use crate::columns::{
    header_unit, match_header, normalize_header, parse_constraint, value, Column, DurationUnit,
    Kind, Lookup, Value,
};
use crate::field_edits::{CrateSetters, FieldEdit, Setters};
use crate::util::num;
use crate::xer_merge::delete_activities;
use crate::xlsx_export::{
    ABOUT_SHEET, ACTIVITY_SHEET, ASSIGNMENT_SHEET, CRITICAL_HEADER, FLOAT_HEADER, FORMAT_MARK,
    KEY_HEADER, LINK_SHEET,
};
use calamine::{DataRef, Reader, Xlsx};
use chrono::{NaiveDate, NaiveDateTime};
use openxer_engine::calendar::DAY;
use openxer_engine::model::{CstrType, Link, LinkType, StatusType};
use openxer_xer::edit::{activity, relationships};
use openxer_xer::{constraint_label, ProjectData};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Empty,
    Text(String),
    Number(f64),
    /// Excel serial date (days since 1899-12-30, fraction is the time of day).
    Date(f64),
    Bool(bool),
    /// An Excel error value such as `#N/A`, as Excel shows it.
    Error(String),
}
impl Cell {
    pub fn is_empty(&self) -> bool {
        match self {
            Cell::Empty => true,
            Cell::Text(s) => s.trim().is_empty(),
            _ => false,
        }
    }
    pub fn text(&self) -> String {
        match self {
            Cell::Empty => String::new(),
            Cell::Text(s) => s.trim().to_string(),
            Cell::Number(n) => format_number(*n),
            Cell::Date(d) => format_minutes((d * DAY as f64).round() as i64),
            Cell::Bool(b) => if *b { "TRUE" } else { "FALSE" }.into(),
            Cell::Error(e) => e.clone(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Sheet {
    pub name: String,
    pub rows: Vec<Vec<Cell>>,
}
impl Sheet {
    pub fn cell(&self, row: usize, col: usize) -> &Cell {
        self.rows
            .get(row)
            .and_then(|r| r.get(col))
            .unwrap_or(&Cell::Empty)
    }
    fn row_texts(&self, row: usize) -> Vec<String> {
        self.rows
            .get(row)
            .map(|r| r.iter().map(Cell::text).collect())
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Workbook {
    pub sheets: Vec<Sheet>,
}
impl Workbook {
    pub fn sheet(&self, name: &str) -> Option<usize> {
        self.sheets
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case(name))
    }
}

/// Free text bound for an XER field: tab, CR and LF would break the tab-separated
/// row framing, so they (and other control characters) become spaces.
pub fn clean_text(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{n:.0}")
    } else {
        let s = format!("{n:.4}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

pub fn format_minutes(m: i64) -> String {
    openxer_xer::day_label(0, m, "%d-%b-%y %H:%M")
}

/// Most cells one sheet may hold once laid out as a grid (last used row by
/// last used column). A stray value far from the data would otherwise make
/// the grid billions of cells.
const MAX_SHEET_CELLS: u64 = 10_000_000;
/// Days between the 1904 and the 1900 date systems' day zero.
const DAYS_1904: f64 = 1462.0;

/// The spreadsheet name of a cell, such as `D7` (zero-based row and column).
pub fn cell_name(row: usize, col: usize) -> String {
    let mut letters = String::new();
    let mut c = col;
    loop {
        letters.insert(0, (b'A' + (c % 26) as u8) as char);
        if c < 26 {
            break;
        }
        c = c / 26 - 1;
    }
    format!("{letters}{}", row + 1)
}

/// Whether an Excel number format shows its value as a percentage: a `%`
/// outside quoted text, escapes and bracketed parts.
fn is_percent_format(code: &str) -> bool {
    let mut chars = code.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                for q in chars.by_ref() {
                    if q == '"' {
                        break;
                    }
                }
            }
            '[' => {
                for q in chars.by_ref() {
                    if q == ']' {
                        break;
                    }
                }
            }
            '\\' | '_' | '*' => {
                chars.next();
            }
            '%' => return true,
            _ => {}
        }
    }
    false
}

/// Read every sheet of an .xlsx file.
///
/// Cells are read as the user sees them: a percentage cell (`50%`, stored
/// as 0.5) is 50, a date of a workbook in the 1904 date system is moved to
/// the 1900 day numbers the rest of the import uses, and an error value
/// (`#N/A`) is kept as an error so the row that uses it can say so.
pub fn read_workbook(path: &Path) -> Result<Workbook, String> {
    let mut book: Xlsx<_> = calamine::open_workbook(path).map_err(|e| {
        format!(
            "{} could not be read as an Excel workbook (.xlsx): {e}",
            path.display()
        )
    })?;
    let shift = if book.has_1904_epoch() {
        DAYS_1904
    } else {
        0.0
    };
    let mut sheets = vec![];
    for name in book.sheet_names() {
        let mut reader = match book.worksheet_cells_reader(&name) {
            Ok(r) => r,
            // A chart sheet holds no cells.
            Err(calamine::XlsxError::NotAWorksheet(_)) => {
                sheets.push(Sheet { name, rows: vec![] });
                continue;
            }
            Err(e) => return Err(format!("Sheet {name}: {e}")),
        };
        let percent: Vec<bool> = reader
            .styles()
            .iter()
            .map(|s| {
                s.number_format
                    .as_ref()
                    .is_some_and(|f| is_percent_format(&f.format_code))
            })
            .collect();
        let number = |v: f64, style: usize| {
            if percent.get(style).copied().unwrap_or(false) {
                // Rounded so 0.07 reads as 7, not 7.000000000000001.
                Cell::Number((v * 100.0 * 1e9).round() / 1e9)
            } else {
                Cell::Number(v)
            }
        };
        let mut cells: Vec<(usize, usize, Cell)> = vec![];
        let (mut first_row, mut last_row, mut last_col) = (usize::MAX, 0usize, 0usize);
        let (mut deepest, mut widest) = ((0, 0), (0, 0));
        loop {
            let next = reader
                .next_cell_with_style_id()
                .map_err(|e| format!("Sheet {name}: {e}"))?;
            let Some((c, style)) = next else {
                break;
            };
            let (r, col) = c.get_position();
            let (r, col) = (r as usize, col as usize);
            let cell = match c.get_value() {
                DataRef::Empty => continue,
                DataRef::Error(e) => Cell::Error(e.to_string()),
                DataRef::String(s) => Cell::Text(s.clone()),
                DataRef::SharedString(s) => Cell::Text(s.to_string()),
                DataRef::Int(i) => number(*i as f64, style),
                DataRef::Float(f) => number(*f, style),
                DataRef::Bool(b) => Cell::Bool(*b),
                DataRef::DateTime(dt) if dt.is_datetime() => Cell::Date(dt.as_f64() + shift),
                DataRef::DateTime(dt) => Cell::Date(dt.as_f64()),
                DataRef::DateTimeIso(s) | DataRef::DurationIso(s) => Cell::Text(s.clone()),
            };
            first_row = first_row.min(r);
            if r >= last_row {
                (last_row, deepest) = (r, (r, col));
            }
            if col >= last_col {
                (last_col, widest) = (col, (r, col));
            }
            cells.push((r, col, cell));
        }
        if cells.is_empty() {
            sheets.push(Sheet { name, rows: vec![] });
            continue;
        }
        let size = (last_row - first_row + 1) as u64 * (last_col + 1) as u64;
        if size > MAX_SHEET_CELLS {
            return Err(format!(
                "Sheet {name} is too large to import: it has values as far as cell {} and cell {}, \
                 {} rows by {} columns, more than the {MAX_SHEET_CELLS} cells a sheet may hold. \
                 Delete the stray cells or copy the data to a new workbook.",
                cell_name(deepest.0, deepest.1),
                cell_name(widest.0, widest.1),
                last_row - first_row + 1,
                last_col + 1
            ));
        }
        // Rows above the first used row stay empty; the rest are as wide as
        // the last used column, as a sheet range is.
        let mut rows: Vec<Vec<Cell>> = vec![vec![]; first_row];
        rows.extend((first_row..=last_row).map(|_| vec![Cell::Empty; last_col + 1]));
        for (r, col, cell) in cells {
            rows[r][col] = cell;
        }
        sheets.push(Sheet { name, rows });
    }
    if sheets.is_empty() {
        return Err("The workbook has no sheets".into());
    }
    Ok(Workbook { sheets })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Format {
    /// Written by OpenXER's modern export (has an About sheet).
    OpenXer,
    /// Sheets named TASK/TASKPRED with field names in row 1, titles in row 2.
    Tables,
    /// Any sheet with column headers in its first row.
    Plain,
}
impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Format::OpenXer => "OpenXER spreadsheet",
            Format::Tables => "XER tables spreadsheet (TASK / TASKPRED sheets)",
            Format::Plain => "Plain spreadsheet with a header row",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Settings {
    pub unit: DurationUnit,
    pub hours_per_day: f64,
    /// False when the file does not say which unit its durations use.
    pub unit_known: bool,
    pub project_key: Option<i64>,
    pub project_name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SheetLayout {
    pub sheet: usize,
    /// Row holding the display titles.
    pub header_row: usize,
    /// Row holding XER field names (tables layout only).
    pub field_row: Option<usize>,
    pub first_data: usize,
    pub headers: Vec<String>,
    pub fields: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Detected {
    pub format: Format,
    pub settings: Settings,
    pub activities: SheetLayout,
    pub links: Option<SheetLayout>,
    /// Resource assignments (Activity ID, Resource ID and units columns).
    pub assignments: Option<SheetLayout>,
    /// Column keys recorded in the About sheet, by header text.
    pub about_keys: HashMap<String, Column>,
    pub notes: Vec<String>,
}

fn layout(wb: &Workbook, sheet: usize, header_row: usize, field_row: Option<usize>) -> SheetLayout {
    let s = &wb.sheets[sheet];
    let headers = s.row_texts(header_row);
    let fields = field_row.map(|r| s.row_texts(r)).unwrap_or_default();
    SheetLayout {
        sheet,
        header_row,
        field_row,
        first_data: header_row + 1,
        headers,
        fields,
    }
}

/// A tables-layout sheet: field names in row 1 and, usually, display titles
/// in row 2. The second row is a title row when the cell under the key
/// column (`key` tells it by its header) is a header too, not data.
fn tables_layout(wb: &Workbook, sheet: usize, key: &dyn Fn(&str) -> bool) -> SheetLayout {
    let s = &wb.sheets[sheet];
    let fields = s.row_texts(0);
    let titled = match fields.iter().position(|f| key(f)) {
        Some(c) => key(&s.cell(1, c).text()),
        None => true,
    };
    if titled {
        layout(wb, sheet, 1, Some(0))
    } else {
        layout(wb, sheet, 0, Some(0))
    }
}

fn first_nonempty_row(s: &Sheet) -> Option<usize> {
    s.rows.iter().position(|r| r.iter().any(|c| !c.is_empty()))
}

/// Recognize the workbook's layout and the units it was written in.
pub fn detect(wb: &Workbook, lookup: &Lookup) -> Result<Detected, String> {
    let mut settings = Settings {
        unit: DurationUnit::Days,
        hours_per_day: lookup.hours_per_day,
        unit_known: false,
        project_key: None,
        project_name: None,
    };
    let mut notes = vec![];
    // OpenXER modern export.
    if let Some(about) = wb.sheet(ABOUT_SHEET) {
        let s = &wb.sheets[about];
        let pairs: HashMap<String, Cell> = s
            .rows
            .iter()
            .filter_map(|r| Some((r.first()?.text(), r.get(1).cloned().unwrap_or(Cell::Empty))))
            .collect();
        if pairs.get("Format").is_some_and(|c| c.text() == FORMAT_MARK) {
            if let Some(u) = pairs
                .get("Duration Unit")
                .and_then(|c| DurationUnit::parse(&c.text()))
            {
                settings.unit = u;
                settings.unit_known = true;
            }
            if let Some(Cell::Number(h)) = pairs.get("Hours per Day") {
                if *h > 0.0 && *h <= 24.0 {
                    settings.hours_per_day = *h;
                }
            }
            if let Some(Cell::Number(k)) = pairs.get("Project Key") {
                settings.project_key = Some(*k as i64);
            }
            settings.project_name = pairs.get("Project ID").map(Cell::text);
            let mut about_keys = HashMap::new();
            if let Some(start) = s
                .rows
                .iter()
                .position(|r| r.first().is_some_and(|c| c.text() == "Sheet"))
            {
                for r in &s.rows[start + 1..] {
                    let get = |i: usize| r.get(i).map(Cell::text).unwrap_or_default();
                    if get(0) == ACTIVITY_SHEET {
                        if let Some(c) = Column::from_key(&get(3)) {
                            about_keys.insert(get(2), c);
                        }
                    }
                }
            }
            let sheet = wb
                .sheet(ACTIVITY_SHEET)
                .ok_or("This OpenXER spreadsheet has no Activities sheet")?;
            let links = wb.sheet(LINK_SHEET).map(|i| layout(wb, i, 0, None));
            return Ok(Detected {
                format: Format::OpenXer,
                settings,
                activities: layout(wb, sheet, 0, None),
                links,
                assignments: wb.sheet(ASSIGNMENT_SHEET).map(|i| layout(wb, i, 0, None)),
                about_keys,
                notes,
            });
        }
    }
    // Tables layout (XER field names).
    if let Some(task) = wb.sheet("TASK") {
        if let Some(user) = wb.sheet("USERDATA") {
            let text: String = wb.sheets[user]
                .rows
                .iter()
                .flatten()
                .map(Cell::text)
                .collect::<Vec<_>>()
                .join("\n");
            for part in text.split(['\n', '\r', ';']) {
                let Some((k, v)) = part.split_once('=') else {
                    continue;
                };
                match k.trim() {
                    "DurationQtyType" => {
                        if let Some(u) = DurationUnit::parse(v) {
                            settings.unit = u;
                            settings.unit_known = true;
                        }
                    }
                    "HoursPerDay" => {
                        if let Ok(h) = v.trim().parse::<f64>() {
                            if h > 0.0 && h <= 24.0 {
                                settings.hours_per_day = h;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if !settings.unit_known {
            notes.push(
                "The file does not record its duration unit; check the unit on the Columns step."
                    .into(),
            );
        }
        let is_activity = |t: &str| match_header(t, lookup) == Some(Column::ActivityId);
        let is_predecessor = |t: &str| {
            matches!(
                normalize_header(t).as_str(),
                "pred_task_id" | "predecessor" | "predecessor id"
            )
        };
        let is_assigned = |t: &str| assignment_target(t) == AssignTarget::Activity;
        let links = wb
            .sheet("TASKPRED")
            .map(|i| tables_layout(wb, i, &is_predecessor));
        return Ok(Detected {
            format: Format::Tables,
            settings,
            activities: tables_layout(wb, task, &is_activity),
            links,
            assignments: wb
                .sheet("TASKRSRC")
                .map(|i| tables_layout(wb, i, &is_assigned)),
            about_keys: HashMap::new(),
            notes,
        });
    }
    // Plain: the first sheet whose header row names an Activity ID column.
    let mut activities = None;
    let mut links = None;
    let mut assignments = None;
    for (i, s) in wb.sheets.iter().enumerate() {
        let Some(header) = first_nonempty_row(s) else {
            continue;
        };
        let texts = s.row_texts(header);
        let normalized: Vec<String> = texts.iter().map(|t| normalize_header(t)).collect();
        let has = |names: &[&str]| normalized.iter().any(|h| names.contains(&h.as_str()));
        let assignment_sheet = texts
            .iter()
            .any(|t| assignment_target(t) == AssignTarget::Resource)
            && texts
                .iter()
                .any(|t| assignment_target(t) == AssignTarget::Activity);
        if assignments.is_none() && assignment_sheet {
            assignments = Some(layout(wb, i, header, None));
        } else if links.is_none()
            && has(&["predecessor", "predecessor id", "pred_task_id"])
            && has(&["successor", "successor id"])
        {
            links = Some(layout(wb, i, header, None));
        } else if activities.is_none()
            && texts
                .iter()
                .any(|t| match_header(t, lookup) == Some(Column::ActivityId))
        {
            activities = Some(layout(wb, i, header, None));
        }
    }
    // A workbook of assignments alone updates assignments only.
    let activities = match (activities, &assignments) {
        (Some(a), _) => a,
        (None, Some(a)) => SheetLayout {
            sheet: a.sheet,
            header_row: a.header_row,
            field_row: None,
            first_data: wb.sheets[a.sheet].rows.len(),
            headers: vec![],
            fields: vec![],
        },
        (None, None) => return Err(
            "No sheet has an Activity ID column. Put column headers in the first row, including one named Activity ID.".into(),
        ),
    };
    notes.push(format!(
        "Durations are read as {} unless a header says otherwise, for example \"Remaining Duration (h)\".",
        settings.unit.label().to_lowercase()
    ));
    Ok(Detected {
        format: Format::Plain,
        settings,
        activities,
        links,
        assignments,
        about_keys: HashMap::new(),
        notes,
    })
}

/// What a spreadsheet column feeds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Target {
    Ignore,
    Column(Column),
    /// OpenXER's hidden row key (activity id or WBS band).
    Key,
    Delete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum LinkTarget {
    Ignore,
    Predecessor,
    Successor,
    Type,
    Lag,
    Delete,
}
impl LinkTarget {
    pub const ALL: [LinkTarget; 6] = [
        LinkTarget::Ignore,
        LinkTarget::Predecessor,
        LinkTarget::Successor,
        LinkTarget::Type,
        LinkTarget::Lag,
        LinkTarget::Delete,
    ];
    pub fn label(self) -> &'static str {
        match self {
            LinkTarget::Ignore => "(ignore)",
            LinkTarget::Predecessor => "Predecessor ID",
            LinkTarget::Successor => "Successor ID",
            LinkTarget::Type => "Relationship Type",
            LinkTarget::Lag => "Lag",
            LinkTarget::Delete => "Delete",
        }
    }
}

/// What a column of the assignments sheet feeds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum AssignTarget {
    Ignore,
    Activity,
    Resource,
    BudgetedUnits,
    RemainingUnits,
    ActualUnits,
    Delete,
}
impl AssignTarget {
    pub const ALL: [AssignTarget; 7] = [
        AssignTarget::Ignore,
        AssignTarget::Activity,
        AssignTarget::Resource,
        AssignTarget::BudgetedUnits,
        AssignTarget::RemainingUnits,
        AssignTarget::ActualUnits,
        AssignTarget::Delete,
    ];
    pub fn label(self) -> &'static str {
        match self {
            AssignTarget::Ignore => "(ignore)",
            AssignTarget::Activity => "Activity ID",
            AssignTarget::Resource => "Resource ID",
            AssignTarget::BudgetedUnits => "Budgeted Units",
            AssignTarget::RemainingUnits => "Remaining Units",
            AssignTarget::ActualUnits => "Actual Regular Units",
            AssignTarget::Delete => "Delete",
        }
    }
    pub fn is_units(self) -> bool {
        matches!(
            self,
            AssignTarget::BudgetedUnits | AssignTarget::RemainingUnits | AssignTarget::ActualUnits
        )
    }
}

/// An assignments sheet column by its header or field name.
pub fn assignment_target(header: &str) -> AssignTarget {
    if is_delete_header(header) {
        return AssignTarget::Delete;
    }
    match normalize_header(header).as_str() {
        "activity id" | "task_id" | "activity" => AssignTarget::Activity,
        "resource id" | "rsrc_id" | "resource" | "rsrc_short_name" => AssignTarget::Resource,
        "budgeted units" | "planned units" | "target_qty" => AssignTarget::BudgetedUnits,
        "remaining units" | "remaining early units" | "remain_qty" => AssignTarget::RemainingUnits,
        "actual units" | "actual regular units" | "act_reg_qty" => AssignTarget::ActualUnits,
        _ => AssignTarget::Ignore,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mapping {
    pub assignment: Vec<AssignTarget>,
    /// Unit of each assignment units column (hours unless its header says days).
    pub assignment_units: Vec<DurationUnit>,
    pub activity: Vec<Target>,
    /// Unit of each duration column (from its header, else the file's unit).
    pub units: Vec<DurationUnit>,
    pub link: Vec<LinkTarget>,
    pub link_unit: DurationUnit,
    pub hours_per_day: f64,
}

fn is_delete_header(h: &str) -> bool {
    matches!(
        normalize_header(h).as_str(),
        "delete" | "delete this row" | "delete_record_flag" | "delete row" | "delete (type d)"
    )
}

/// Match columns by header text (display titles or field names, case-insensitive).
pub fn auto_map(detected: &Detected, lookup: &Lookup) -> Mapping {
    let a = &detected.activities;
    let unit = detected.settings.unit;
    let mut used = HashSet::new();
    let mut activity = vec![];
    let mut units = vec![];
    for (i, h) in a.headers.iter().enumerate() {
        let field = a.fields.get(i).map(String::as_str).unwrap_or("");
        let target = if h == KEY_HEADER {
            Target::Key
        } else if h == CRITICAL_HEADER || h == FLOAT_HEADER {
            Target::Ignore
        } else if is_delete_header(h) || is_delete_header(field) {
            Target::Delete
        } else {
            detected
                .about_keys
                .get(h)
                .copied()
                .or_else(|| {
                    (!field.is_empty())
                        .then(|| match_header(field, lookup))
                        .flatten()
                })
                .or_else(|| match_header(h, lookup))
                .filter(|c| used.insert(*c))
                .map_or(Target::Ignore, Target::Column)
        };
        activity.push(target);
        units.push(header_unit(h).unwrap_or(unit));
    }
    let mut link = vec![];
    let mut link_unit = unit;
    if let Some(l) = &detected.links {
        for (i, h) in l.headers.iter().enumerate() {
            let field = l.fields.get(i).map(String::as_str).unwrap_or("");
            let n = normalize_header(h);
            let f = field.to_ascii_lowercase();
            let t = if is_delete_header(h) || is_delete_header(field) {
                LinkTarget::Delete
            } else if f == "pred_task_id" || matches!(n.as_str(), "predecessor" | "predecessor id")
            {
                LinkTarget::Predecessor
            } else if f == "task_id" || matches!(n.as_str(), "successor" | "successor id") {
                LinkTarget::Successor
            } else if f == "pred_type" || matches!(n.as_str(), "relationship type" | "type") {
                LinkTarget::Type
            } else if f == "lag_hr_cnt" || n == "lag" {
                if let Some(u) = header_unit(h) {
                    link_unit = u;
                }
                LinkTarget::Lag
            } else {
                LinkTarget::Ignore
            };
            link.push(t);
        }
    }
    let (assignment, assignment_units) =
        detected.assignments.as_ref().map_or((vec![], vec![]), |a| {
            let mut seen = HashSet::new();
            a.headers
                .iter()
                .enumerate()
                .map(|(i, h)| {
                    let field = a.fields.get(i).map(String::as_str).unwrap_or("");
                    let t = match assignment_target(field) {
                        AssignTarget::Ignore => assignment_target(h),
                        t => t,
                    };
                    let t = if t != AssignTarget::Ignore && !seen.insert(t) {
                        AssignTarget::Ignore
                    } else {
                        t
                    };
                    (t, header_unit(h).unwrap_or(DurationUnit::Hours))
                })
                .unzip()
        });
    Mapping {
        assignment,
        assignment_units,
        activity,
        units,
        link,
        link_unit,
        hours_per_day: detected.settings.hours_per_day,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum RowKind {
    Unchanged,
    Changed,
    New,
    Delete,
    Skipped,
    Error,
}
impl RowKind {
    pub fn label(self) -> &'static str {
        match self {
            RowKind::Unchanged => "Unchanged",
            RowKind::Changed => "Changed",
            RowKind::New => "New",
            RowKind::Delete => "Delete",
            RowKind::Skipped => "Skipped",
            RowKind::Error => "Error",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CellChange {
    pub column: String,
    pub old: String,
    pub new: String,
}

#[derive(Clone, Debug, PartialEq)]
enum Edit {
    /// A new Activity ID.
    Id(String),
    Name(String),
    /// A field change for the setters, with the column title it came from.
    Field(FieldEdit, String),
}

#[derive(Clone, Debug, Serialize)]
pub struct RowPlan {
    /// 1-based spreadsheet row number, as Excel shows it.
    pub sheet_row: usize,
    pub activity_id: String,
    pub kind: RowKind,
    pub task_id: Option<i64>,
    pub changes: Vec<CellChange>,
    pub messages: Vec<String>,
    #[serde(skip)]
    edits: Vec<Edit>,
    #[serde(skip)]
    wbs_hint: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
enum LinkOp {
    None,
    Add(LinkType, i64),
    Update {
        old: LinkType,
        new: LinkType,
        lag: i64,
    },
    Remove(LinkType),
}

#[derive(Clone, Debug, Serialize)]
pub struct LinkPlan {
    pub sheet_row: usize,
    pub predecessor: String,
    pub successor: String,
    pub kind: RowKind,
    pub change: String,
    pub message: String,
    #[serde(skip)]
    op: LinkOp,
}

/// Units an assignments row sets, in hours.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct AssignUnits {
    pub budgeted: Option<f64>,
    pub remaining: Option<f64>,
    pub actual: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AssignPlan {
    pub sheet_row: usize,
    pub activity_id: String,
    pub resource_id: String,
    pub kind: RowKind,
    pub changes: Vec<CellChange>,
    pub message: String,
    #[serde(skip)]
    units: AssignUnits,
    /// The resource's `rsrc_id`.
    #[serde(skip)]
    rsrc: i64,
    /// `taskrsrc_id` of the assignment the row updates or deletes. The n-th
    /// row of one activity and resource is its n-th assignment, so an
    /// activity with a resource assigned twice reads back unchanged.
    #[serde(skip)]
    existing: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct PlanCounts {
    pub assignments_new: usize,
    pub assignments_changed: usize,
    pub assignments_deleted: usize,
    pub assignments_unchanged: usize,
    pub assignment_errors: usize,
    pub unchanged: usize,
    pub changed: usize,
    pub new: usize,
    pub delete: usize,
    pub skipped: usize,
    pub errors: usize,
    pub links_new: usize,
    pub links_changed: usize,
    pub links_deleted: usize,
    pub links_unchanged: usize,
    pub link_errors: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct ImportPlan {
    pub format: Format,
    pub settings: Settings,
    pub rows: Vec<RowPlan>,
    pub links: Vec<LinkPlan>,
    pub assignments: Vec<AssignPlan>,
    pub notes: Vec<String>,
}
impl ImportPlan {
    pub fn counts(&self) -> PlanCounts {
        let mut c = PlanCounts::default();
        for r in &self.rows {
            match r.kind {
                RowKind::Unchanged => c.unchanged += 1,
                RowKind::Changed => c.changed += 1,
                RowKind::New => c.new += 1,
                RowKind::Delete => c.delete += 1,
                RowKind::Skipped => c.skipped += 1,
                RowKind::Error => c.errors += 1,
            }
        }
        for l in &self.links {
            match l.kind {
                RowKind::New => c.links_new += 1,
                RowKind::Changed => c.links_changed += 1,
                RowKind::Delete => c.links_deleted += 1,
                RowKind::Unchanged => c.links_unchanged += 1,
                RowKind::Error => c.link_errors += 1,
                RowKind::Skipped => {}
            }
        }
        for a in &self.assignments {
            match a.kind {
                RowKind::New => c.assignments_new += 1,
                RowKind::Changed => c.assignments_changed += 1,
                RowKind::Delete => c.assignments_deleted += 1,
                RowKind::Unchanged => c.assignments_unchanged += 1,
                RowKind::Error => c.assignment_errors += 1,
                RowKind::Skipped => {}
            }
        }
        c
    }
    pub fn has_changes(&self) -> bool {
        let c = self.counts();
        c.changed
            + c.new
            + c.delete
            + c.links_new
            + c.links_changed
            + c.links_deleted
            + c.assignments_new
            + c.assignments_changed
            + c.assignments_deleted
            > 0
    }
}

fn display(v: &Value, unit: DurationUnit, hpd: f64) -> String {
    match v {
        Value::Empty => String::new(),
        Value::Text(s) => s.clone(),
        Value::Date(m) => format_minutes(*m),
        Value::Duration(m) => format!(
            "{}{}",
            format_number(unit.from_minutes(*m, hpd)),
            unit.suffix()
        ),
        Value::Number(n) => format_number(*n),
        Value::Bool(b) => if *b { "Yes" } else { "No" }.into(),
    }
}

/// Text date forms, each two-digit-year form before its four-digit one: the
/// four-digit directive also takes one to four digits, so "1/2/26" would
/// otherwise be read as the year 26.
const DATE_FORMATS: [&str; 12] = [
    "%d-%b-%y %H:%M",
    "%d-%b-%Y %H:%M",
    "%Y-%m-%d %H:%M",
    "%Y-%m-%d %H:%M:%S",
    "%m/%d/%y %H:%M",
    "%m/%d/%Y %H:%M",
    "%d-%b-%y %I:%M %p",
    "%d-%b-%Y %I:%M %p",
    "%m/%d/%y %I:%M %p",
    "%m/%d/%Y %I:%M %p",
    "%Y-%m-%dT%H:%M:%S",
    "%d %b %Y %H:%M",
];
const DAY_FORMATS: [&str; 6] = [
    "%d-%b-%y", "%d-%b-%Y", "%Y-%m-%d", "%m/%d/%y", "%m/%d/%Y", "%d %b %Y",
];
/// Years a date cell may name. Anything outside is a typing or conversion
/// slip (a two-digit year read as the first century, a duration in a date
/// column) and is refused rather than imported.
const FIRST_YEAR: i32 = 1900;
const LAST_YEAR: i32 = 2199;

/// Refuse absolute minutes outside [`FIRST_YEAR`] to [`LAST_YEAR`].
fn in_range(minutes: i64, shown: &str) -> Result<i64, String> {
    let bound = |y: i32| {
        (NaiveDate::from_ymd_opt(y, 1, 1)
            .expect("valid")
            .and_hms_opt(0, 0, 0)
            .expect("valid")
            - epoch())
        .num_minutes()
    };
    if (bound(FIRST_YEAR)..bound(LAST_YEAR + 1)).contains(&minutes) {
        Ok(minutes)
    } else {
        Err(format!(
            "{shown} is outside the years {FIRST_YEAR} to {LAST_YEAR}; check the year"
        ))
    }
}

fn epoch() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(1899, 12, 30)
        .expect("valid")
        .and_hms_opt(0, 0, 0)
        .expect("valid")
}

/// Absolute minutes, and whether the cell carried only a date (no time of day).
fn parse_date(cell: &Cell) -> Result<Option<(i64, bool)>, String> {
    match cell {
        Cell::Empty => Ok(None),
        Cell::Date(d) | Cell::Number(d) => {
            if !d.is_finite() || !(1.0..=2958465.0).contains(d) {
                return Err(format!("{} is not a date", format_number(*d)));
            }
            let m = in_range((d * DAY as f64).round() as i64, &cell.text())?;
            Ok(Some((m, m % DAY == 0)))
        }
        Cell::Bool(_) => Err("TRUE/FALSE is not a date".into()),
        Cell::Error(e) => Err(format!("{e} is an Excel error, not a date")),
        Cell::Text(s) => {
            let mut t = s.trim().to_string();
            if t.is_empty() {
                return Ok(None);
            }
            // Actual dates carry a trailing A; constraint dates a * (older
            // text) or the double dagger the table shows.
            for suffix in [" A", "*", " *", "\u{2021}", " \u{2021}"] {
                if let Some(rest) = t.strip_suffix(suffix) {
                    t = rest.trim().to_string();
                }
            }
            let shown = |d: NaiveDateTime| format!("{t:?} ({})", d.format("%d-%b-%Y"));
            for f in DATE_FORMATS {
                if let Ok(d) = NaiveDateTime::parse_from_str(&t, f) {
                    let m = in_range((d - epoch()).num_minutes(), &shown(d))?;
                    return Ok(Some((m, false)));
                }
            }
            for f in DAY_FORMATS {
                if let Ok(d) = NaiveDate::parse_from_str(&t, f) {
                    let d = d.and_hms_opt(0, 0, 0).expect("midnight");
                    let m = in_range((d - epoch()).num_minutes(), &shown(d))?;
                    return Ok(Some((m, true)));
                }
            }
            Err(format!(
                "{t:?} is not a date; use a date cell or dd-Mon-yy hh:mm"
            ))
        }
    }
}

fn parse_duration(cell: &Cell, unit: DurationUnit, hpd: f64) -> Result<Option<i64>, String> {
    let (value, unit) = match cell {
        Cell::Empty => return Ok(None),
        Cell::Number(n) => (*n, unit),
        Cell::Text(s) => {
            let t = s.trim().to_ascii_lowercase();
            if t.is_empty() {
                return Ok(None);
            }
            let split = t.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(t.len());
            let (n, suffix) = t.split_at(split);
            let u = if suffix.trim().is_empty() {
                unit
            } else {
                DurationUnit::parse(suffix).ok_or_else(|| format!("{s:?} is not a duration"))?
            };
            (
                n.trim()
                    .parse::<f64>()
                    .map_err(|_| format!("{s:?} is not a duration"))?,
                u,
            )
        }
        Cell::Date(_) | Cell::Bool(_) | Cell::Error(_) => {
            return Err(format!("{} is not a duration", cell.text()))
        }
    };
    if !value.is_finite() || !(0.0..=1_000_000.0).contains(&value) {
        return Err(format!("{} is not a valid duration", format_number(value)));
    }
    Ok(Some(unit.to_minutes(value, hpd)))
}

/// A lag: a duration that may be negative, as a number or as text ("-2d").
fn parse_lag(cell: &Cell, unit: DurationUnit, hpd: f64) -> Result<Option<i64>, String> {
    match cell {
        Cell::Number(n) if *n < 0.0 => {
            parse_duration(&Cell::Number(-n), unit, hpd).map(|m| m.map(|m| -m))
        }
        Cell::Text(t) => match t.trim().strip_prefix('-') {
            Some(rest) => parse_duration(&Cell::Text(rest.trim().into()), unit, hpd)
                .map(|m| m.map(|m| -m))
                .map_err(|_| format!("{t:?} is not a duration")),
            None => parse_duration(cell, unit, hpd),
        },
        _ => parse_duration(cell, unit, hpd),
    }
}

fn parse_percent(cell: &Cell) -> Result<f64, String> {
    let v = match cell {
        Cell::Empty => 0.0,
        Cell::Number(n) => *n,
        Cell::Text(s) => s
            .trim()
            .trim_end_matches('%')
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("{s:?} is not a percentage"))?,
        _ => return Err(format!("{} is not a percentage", cell.text())),
    };
    if !(0.0..=100.0).contains(&v) {
        return Err(format!("{} is outside 0 to 100", format_number(v)));
    }
    Ok(v)
}

/// A units cell in hours; a blank cell is no change.
fn parse_units(cell: &Cell) -> Result<Option<f64>, String> {
    let v = match cell {
        Cell::Empty => return Ok(None),
        Cell::Number(n) => *n,
        Cell::Text(s) => {
            if s.trim().is_empty() {
                return Ok(None);
            }
            s.trim()
                .parse::<f64>()
                .map_err(|_| format!("{s:?} is not a number"))?
        }
        _ => return Err(format!("{} is not a number", cell.text())),
    };
    if !v.is_finite() || !(0.0..1.0e12).contains(&v) {
        return Err(format!("{} is not a valid quantity", format_number(v)));
    }
    Ok(Some(v))
}

fn is_delete_flag(cell: &Cell) -> bool {
    match cell {
        Cell::Bool(b) => *b,
        c => matches!(
            c.text().to_ascii_lowercase().as_str(),
            "d" | "delete" | "yes" | "y" | "x" | "true"
        ),
    }
}

/// Cell text that clears a value on purpose. A blank cell changes nothing.
pub const CLEAR_MARK: &str = "<none>";

fn is_clear(cell: &Cell) -> bool {
    matches!(cell, Cell::Text(t) if t.trim().eq_ignore_ascii_case(CLEAR_MARK))
}

/// Constraint types whose date is the end of a day's work.
fn finish_constraint(c: CstrType) -> bool {
    matches!(
        c,
        CstrType::FinishOn
            | CstrType::FinishOnBefore
            | CstrType::FinishOnAfter
            | CstrType::MandatoryFinish
    )
}

fn parse_link_type(s: &str) -> Option<LinkType> {
    let t = s.trim().to_ascii_uppercase().replace([' ', '-'], "");
    Some(match t.trim_start_matches("PR_") {
        "FS" | "FINISHTOSTART" => LinkType::FS,
        "SS" | "STARTTOSTART" => LinkType::SS,
        "FF" | "FINISHTOFINISH" => LinkType::FF,
        "SF" | "STARTTOFINISH" => LinkType::SF,
        _ => return None,
    })
}

struct Context<'a> {
    p: &'a ProjectData,
    lookup: Lookup,
    /// Code value ids by (type id, lower short name).
    code_values: HashMap<(i64, String), i64>,
    code_names: HashMap<i64, String>,
}
impl<'a> Context<'a> {
    fn new(p: &'a ProjectData) -> Self {
        let lookup = Lookup::new(p);
        let mut code_values = HashMap::new();
        let mut code_names = HashMap::new();
        if let Some(t) = p.document.table("ACTVCODE") {
            for r in &t.rows {
                let kind = num(t.value(r, "actv_code_type_id"));
                if lookup.code_types.contains_key(&kind) {
                    let id = num(t.value(r, "actv_code_id"));
                    code_values.insert((kind, t.value(r, "short_name").trim().to_lowercase()), id);
                    code_names.insert(id, t.value(r, "short_name").to_string());
                }
            }
        }
        Self {
            p,
            lookup,
            code_values,
            code_names,
        }
    }
    fn wbs_by_text(&self, text: &str) -> Option<i64> {
        let t = text.trim().to_lowercase();
        let root = self.p.model.project.name.to_lowercase();
        let stripped = t.strip_prefix(&format!("{root}.")).unwrap_or(&t);
        let label = |w: &openxer_xer::Wbs| format!("{}  {}", w.code, w.name).to_lowercase();
        self.lookup
            .wbs_path
            .iter()
            .find(|(_, path)| path.to_lowercase() == stripped || path.to_lowercase() == t)
            .map(|(id, _)| *id)
            .or_else(|| {
                let codes: Vec<_> = self
                    .p
                    .wbs
                    .iter()
                    .filter(|w| w.code.to_lowercase() == t)
                    .collect();
                (codes.len() == 1).then(|| codes[0].id)
            })
            .or_else(|| self.p.wbs.iter().find(|w| label(w) == t).map(|w| w.id))
            .or_else(|| {
                let names: Vec<_> = self
                    .p
                    .wbs
                    .iter()
                    .filter(|w| w.name.to_lowercase() == t)
                    .collect();
                (names.len() == 1).then(|| names[0].id)
            })
    }
    /// Snap a date-only start to the first work time of that day on `cal`.
    fn snap(&self, absolute: i64, date_only: bool, cal: usize, finish: bool) -> i64 {
        if !date_only {
            return absolute;
        }
        let origin = self.lookup.origin_minutes;
        let rel = absolute - origin;
        let day = rel.div_euclid(DAY);
        let Some(c) = self.p.model.calendars.get(cal) else {
            return absolute;
        };
        let shifts = c.cal.shifts_on(day);
        match (shifts.first(), shifts.last()) {
            (Some(first), Some(last)) => origin + day * DAY + if finish { last.1 } else { first.0 },
            _ => absolute,
        }
    }
}

/// Messages for the Excel error values (`#N/A` and the like) in the cells of
/// sheet row `r` whose column `used` says the import reads.
fn error_cells(
    r: usize,
    cells: &[Cell],
    headers: &[String],
    used: impl Fn(usize) -> bool,
) -> Vec<String> {
    cells
        .iter()
        .enumerate()
        .filter(|(c, _)| used(*c))
        .filter_map(|(c, cell)| match cell {
            Cell::Error(e) => Some(format!(
                "{} (cell {}): Excel error {e}; correct the formula or type the value",
                headers.get(c).map_or("", |h| h.trim()),
                cell_name(r, c)
            )),
            _ => None,
        })
        .collect()
}

/// A note when every value of a percent column lies between 0 and 1: the
/// sheet probably holds fractions (0.5 for 50%) in cells not formatted as
/// percentages, which this import reads as written.
fn fraction_notes(sheet: &Sheet, layout: &SheetLayout, mapping: &Mapping) -> Vec<String> {
    let mut notes = vec![];
    for (c, t) in mapping.activity.iter().enumerate() {
        if !matches!(
            t,
            Target::Column(
                Column::PercentComplete | Column::ActivityPercent | Column::DurationPercent
            )
        ) {
            continue;
        }
        let values: Vec<f64> = (layout.first_data..sheet.rows.len())
            .filter_map(|r| match sheet.cell(r, c) {
                Cell::Number(n) => Some(*n),
                Cell::Text(t) if !t.contains('%') => t.trim().parse::<f64>().ok(),
                _ => None,
            })
            .collect();
        if !values.is_empty()
            && values.iter().all(|v| (0.0..=1.0).contains(v))
            && values.iter().any(|v| *v > 0.0 && *v < 1.0)
        {
            notes.push(format!(
                "{}: every value is between 0 and 1. If these are fractions (0.5 for 50%), \
                 multiply them by 100 or format the cells as percentages; as written, 0.5 imports as 0.5%.",
                layout.headers.get(c).map_or("", |h| h.trim())
            ));
        }
    }
    notes
}

/// Build the preview. `project` is not changed.
pub fn plan(
    wb: &Workbook,
    detected: &Detected,
    mapping: &Mapping,
    project: &ProjectData,
) -> ImportPlan {
    plan_with(wb, detected, mapping, project, &mut CrateSetters)
}

/// `plan` with the caller's setters: every changed cell is tried through them
/// on a copy of `project`, and a cell they refuse turns its row into an error
/// row that carries their message.
pub fn plan_with(
    wb: &Workbook,
    detected: &Detected,
    mapping: &Mapping,
    project: &ProjectData,
    setters: &mut dyn Setters,
) -> ImportPlan {
    let ctx = Context::new(project);
    let hpd = mapping.hours_per_day;
    let layout = &detected.activities;
    let sheet = &wb.sheets[layout.sheet];
    let mut notes = detected.notes.clone();
    if let Some(key) = detected.settings.project_key {
        if key != project.model.project.proj_id {
            notes.push(format!(
                "This file was exported from project {} ({key}); it is being imported into {}. Rows match by Activity ID.",
                detected.settings.project_name.clone().unwrap_or_default(),
                project.model.project.name
            ));
        }
    }
    notes.extend(fraction_notes(sheet, layout, mapping));
    let col_of = |c: Column| {
        mapping
            .activity
            .iter()
            .position(|t| *t == Target::Column(c))
    };
    let id_col = col_of(Column::ActivityId);
    let key_col = mapping.activity.iter().position(|t| *t == Target::Key);
    let delete_col = mapping.activity.iter().position(|t| *t == Target::Delete);
    let calculated: Vec<String> = mapping
        .activity
        .iter()
        .enumerate()
        .filter_map(|(i, t)| match t {
            Target::Column(c) if !c.editable() => layout.headers.get(i).cloned(),
            _ => None,
        })
        .collect();
    if !calculated.is_empty() {
        notes.push(format!(
            "Calculated by scheduling, not imported: {}.",
            calculated.join(", ")
        ));
    }
    let by_task: HashMap<i64, usize> = project
        .model
        .activities
        .iter()
        .enumerate()
        .map(|(i, a)| (a.task_id, i))
        .collect();
    let by_code: HashMap<&str, usize> = project
        .model
        .activities
        .iter()
        .enumerate()
        .map(|(i, a)| (a.task_code.as_str(), i))
        .collect();
    // Band texts as the modern export writes them.
    let bands: HashMap<String, i64> = project
        .wbs
        .iter()
        .map(|w| {
            let path = ctx.lookup.wbs_path.get(&w.id).cloned().unwrap_or_default();
            (format!("{path}  {}", w.name), w.id)
        })
        .collect();
    if detected.format == Format::OpenXer && key_col.is_none() {
        notes.push(
            "The hidden OpenXER Key column is missing: rows match by Activity ID, and WBS band rows are recognized by their text."
                .into(),
        );
    }
    let mut rows = vec![];
    let mut seen_codes: HashMap<String, usize> = HashMap::new();
    let mut seen_targets: HashSet<usize> = HashSet::new();
    let mut band_wbs: Option<i64> = None;
    for r in layout.first_data..sheet.rows.len() {
        let cells = &sheet.rows[r];
        if cells.iter().all(Cell::is_empty) {
            continue;
        }
        let cell = |c: Option<usize>| c.map_or(&Cell::Empty, |c| sheet.cell(r, c));
        let key = cell(key_col).text();
        let code = cell(id_col).text();
        let mut row = RowPlan {
            sheet_row: r + 1,
            activity_id: code.clone(),
            kind: RowKind::Unchanged,
            task_id: None,
            changes: vec![],
            messages: vec![],
            edits: vec![],
            wbs_hint: None,
        };
        // A WBS band row: named by the hidden key, or, when the key column
        // is gone, by the band text in the first cell (an export writes no
        // activity whose ID reads like a band).
        let band = key
            .strip_prefix("W:")
            .and_then(|w| w.parse().ok())
            .or_else(|| {
                key_col
                    .is_none()
                    .then(|| cells.first().map(Cell::text))
                    .flatten()
                    .filter(|t| !by_code.contains_key(t.as_str()))
                    .and_then(|t| bands.get(&t).copied())
            });
        if key.starts_with("W:") || band.is_some() {
            band_wbs = band;
            row.kind = RowKind::Skipped;
            row.activity_id = cells.first().map(Cell::text).unwrap_or_default();
            row.messages.push("WBS band row".into());
            rows.push(row);
            continue;
        }
        let broken = error_cells(r, cells, &layout.headers, |c| {
            match mapping.activity.get(c) {
                Some(Target::Column(col)) => col.editable() || *col == Column::ActivityId,
                Some(Target::Key | Target::Delete) => true,
                _ => false,
            }
        });
        if !broken.is_empty() {
            row.kind = RowKind::Error;
            row.activity_id = clean_text(&code);
            row.messages = broken;
            rows.push(row);
            continue;
        }
        if id_col.is_none() {
            row.kind = RowKind::Error;
            row.messages
                .push("No column is mapped to Activity ID".into());
            rows.push(row);
            continue;
        }
        let keyed = key
            .strip_prefix("A:")
            .and_then(|k| k.parse::<i64>().ok())
            .and_then(|id| by_task.get(&id).copied());
        let target = keyed.or_else(|| by_code.get(code.as_str()).copied());
        if code.chars().any(char::is_control) {
            row.kind = RowKind::Error;
            row.activity_id = clean_text(&code);
            row.messages
                .push("Activity ID contains a tab, line break or other control character".into());
            rows.push(row);
            continue;
        }
        if code.is_empty() {
            // Also when the hidden key still names the activity: an empty
            // Activity ID is never written.
            row.kind = RowKind::Error;
            row.messages.push("Activity ID is empty".into());
            rows.push(row);
            continue;
        }
        if let Some(&first) = seen_codes.get(&code) {
            row.kind = RowKind::Error;
            row.messages
                .push(format!("Activity ID {code} already appears on row {first}"));
            rows.push(row);
            continue;
        }
        seen_codes.insert(code.clone(), r + 1);
        if let Some(t) = target {
            if !seen_targets.insert(t) {
                row.kind = RowKind::Error;
                row.messages
                    .push("Another row already updates this activity".into());
                rows.push(row);
                continue;
            }
            row.task_id = Some(project.model.activities[t].task_id);
        }
        if delete_col.is_some_and(|c| is_delete_flag(sheet.cell(r, c))) {
            match target {
                Some(_) => {
                    row.kind = RowKind::Delete;
                    row.messages
                        .push("Deletes the activity and its relationships".into());
                }
                None => {
                    row.kind = RowKind::Error;
                    row.messages
                        .push(format!("Cannot delete {code}: no such activity"));
                }
            }
            rows.push(row);
            continue;
        }
        let mut errors = vec![];
        // A date-only cell snaps to the start or end of the day's work on
        // the calendar and by the constraint type the row leaves, which may
        // come from other cells of the same row.
        let row_text = |c: Column| {
            col_of(c)
                .map(|i| sheet.cell(r, i).text())
                .filter(|t| !t.is_empty())
        };
        let cal = row_text(Column::Calendar)
            .and_then(|t| {
                project
                    .calendar_names
                    .iter()
                    .position(|n| n.trim().eq_ignore_ascii_case(&t))
            })
            .unwrap_or_else(|| {
                target.map_or(project.model.project_calendar, |t| {
                    project.model.activities[t].cal
                })
            });
        let constraint_of = |secondary: bool| {
            let column = if secondary {
                Column::SecondaryConstraint
            } else {
                Column::PrimaryConstraint
            };
            row_text(column)
                .and_then(|t| parse_constraint(&t))
                .unwrap_or_else(|| {
                    target.map_or(CstrType::None, |t| {
                        let a = &project.model.activities[t];
                        if secondary {
                            a.cstr2
                        } else {
                            a.cstr
                        }
                    })
                })
        };
        let current = |c: Column| -> Value {
            match target {
                Some(t) => value(project, &ctx.lookup, t, c),
                None => Value::Empty,
            }
        };
        let push = |row: &mut RowPlan, label: &str, old: String, new: String, edit: Edit| {
            if target.is_none() || old != new {
                row.changes.push(CellChange {
                    column: label.into(),
                    old: if target.is_some() { old } else { String::new() },
                    new,
                });
            }
            row.edits.push(edit);
        };
        for (c, t) in mapping.activity.iter().enumerate() {
            let Target::Column(column) = *t else { continue };
            if !column.editable() {
                continue;
            }
            let src = sheet.cell(r, c);
            let label = column.title(&ctx.lookup);
            // A refusal by the setters names the column and the cell.
            let at_cell = format!("{label} (cell {})", cell_name(r, c));
            let field = |f: FieldEdit| Edit::Field(f, at_cell.clone());
            let unit = mapping
                .units
                .get(c)
                .copied()
                .unwrap_or(detected.settings.unit);
            let old = current(column);
            let old_text = display(&old, unit, hpd);
            match column {
                Column::ActivityId => {
                    if code != old_text {
                        if by_code
                            .get(code.as_str())
                            .is_some_and(|&i| Some(i) != target)
                        {
                            errors.push(format!("Activity ID {code} is used by another activity"));
                        } else {
                            push(
                                &mut row,
                                &label,
                                old_text,
                                code.clone(),
                                Edit::Id(code.clone()),
                            );
                        }
                    }
                }
                Column::ActivityName => {
                    let new = clean_text(&src.text());
                    if new.is_empty() {
                        continue;
                    }
                    if new != old_text {
                        push(&mut row, &label, old_text, new.clone(), Edit::Name(new));
                    }
                }
                Column::WbsCode => {
                    let text = src.text();
                    if text.is_empty() {
                        continue;
                    }
                    match ctx.wbs_by_text(&text) {
                        Some(id) => {
                            let new = ctx.lookup.wbs_path.get(&id).cloned().unwrap_or(text);
                            if new != old_text {
                                push(&mut row, &label, old_text, new, field(FieldEdit::Wbs(id)));
                            }
                        }
                        None => errors.push(format!("WBS {text:?} is not in this project")),
                    }
                }
                Column::Calendar => {
                    let text = src.text();
                    if text.is_empty() {
                        continue;
                    }
                    match project
                        .calendar_names
                        .iter()
                        .position(|n| n.trim().eq_ignore_ascii_case(&text))
                    {
                        Some(i) => {
                            if !project.calendar_names[i].eq(&old_text) {
                                push(
                                    &mut row,
                                    &label,
                                    old_text,
                                    project.calendar_names[i].clone(),
                                    field(FieldEdit::Calendar(i)),
                                );
                            }
                        }
                        None => errors.push(format!("Calendar {text:?} does not exist")),
                    }
                }
                Column::OriginalDuration | Column::RemainingDuration | Column::ActualDuration => {
                    match parse_duration(src, unit, hpd) {
                        Ok(None) => {}
                        Ok(Some(m)) => {
                            let old_m = match old {
                                Value::Duration(o) => Some(o),
                                _ => None,
                            };
                            if old_m != Some(m) {
                                let edit = match column {
                                    Column::OriginalDuration => FieldEdit::OriginalDuration(m),
                                    Column::RemainingDuration => FieldEdit::RemainingDuration(m),
                                    _ => FieldEdit::ActualDuration(m),
                                };
                                push(
                                    &mut row,
                                    &label,
                                    old_text,
                                    display(&Value::Duration(m), unit, hpd),
                                    field(edit),
                                );
                            }
                        }
                        Err(e) => errors.push(format!("{label}: {e}")),
                    }
                }
                Column::ActualStart
                | Column::ActualFinish
                | Column::ExpectedFinish
                | Column::SuspendDate
                | Column::ResumeDate
                | Column::PrimaryConstraintDate
                | Column::SecondaryConstraintDate => match if is_clear(src) {
                    Ok(None)
                } else if src.is_empty() {
                    continue;
                } else {
                    parse_date(src)
                } {
                    Ok(parsed) => {
                        let finish = match column {
                            Column::ActualFinish | Column::ExpectedFinish | Column::SuspendDate => {
                                true
                            }
                            Column::PrimaryConstraintDate => {
                                finish_constraint(constraint_of(false))
                            }
                            Column::SecondaryConstraintDate => {
                                finish_constraint(constraint_of(true))
                            }
                            _ => false,
                        };
                        let new = parsed.map(|(m, date_only)| ctx.snap(m, date_only, cal, finish));
                        let old_m = match old {
                            Value::Date(o) => Some(o),
                            _ => None,
                        };
                        if new != old_m {
                            let rel = new.map(|m| m - ctx.lookup.origin_minutes);
                            let edit = match column {
                                Column::ActualStart => FieldEdit::ActualStart(rel),
                                Column::ActualFinish => FieldEdit::ActualFinish(rel),
                                Column::ExpectedFinish => FieldEdit::ExpectedFinish(rel),
                                Column::SuspendDate => FieldEdit::SuspendDate(rel),
                                Column::ResumeDate => FieldEdit::ResumeDate(rel),
                                Column::PrimaryConstraintDate => FieldEdit::ConstraintDate {
                                    secondary: false,
                                    date: rel,
                                },
                                _ => FieldEdit::ConstraintDate {
                                    secondary: true,
                                    date: rel,
                                },
                            };
                            push(
                                &mut row,
                                &label,
                                old_text,
                                new.map_or(String::new(), format_minutes),
                                field(edit),
                            );
                        }
                    }
                    Err(e) => errors.push(format!("{label}: {e}")),
                },
                Column::PercentComplete | Column::ActivityPercent | Column::DurationPercent => {
                    match parse_percent(src) {
                        Ok(v) => {
                            if src.is_empty() && target.is_some() {
                                continue;
                            }
                            let old_v = match old {
                                Value::Number(o) => o,
                                _ => 0.0,
                            };
                            if (v - old_v).abs() > 1e-9 {
                                let edit = match column {
                                    Column::PercentComplete => FieldEdit::PhysicalPercent(v),
                                    Column::ActivityPercent => FieldEdit::ActivityPercent(v),
                                    _ => FieldEdit::DurationPercent(v),
                                };
                                push(&mut row, &label, old_text, format_number(v), field(edit));
                            }
                        }
                        Err(e) => errors.push(format!("{label}: {e}")),
                    }
                }
                Column::ActualLaborUnits | Column::RemainingLaborUnits => match parse_units(src) {
                    Ok(None) => {}
                    Ok(Some(hours)) => {
                        let old_v = match old {
                            Value::Number(o) => o,
                            _ => 0.0,
                        };
                        if (hours - old_v).abs() > 1e-9 {
                            let edit = FieldEdit::LaborUnits {
                                actual: column == Column::ActualLaborUnits,
                                hours,
                            };
                            push(
                                &mut row,
                                &label,
                                old_text,
                                format_number(hours),
                                field(edit),
                            );
                        }
                    }
                    Err(e) => errors.push(format!("{label}: {e}")),
                },
                Column::PrimaryConstraint | Column::SecondaryConstraint => {
                    if src.is_empty() {
                        continue;
                    }
                    let text = if is_clear(src) {
                        String::from("none")
                    } else {
                        src.text()
                    };
                    match parse_constraint(&text) {
                        Some(c) => {
                            let new = if c == CstrType::None {
                                String::new()
                            } else {
                                constraint_label(c).into()
                            };
                            if new != old_text {
                                push(
                                    &mut row,
                                    &label,
                                    old_text,
                                    new,
                                    field(FieldEdit::Constraint {
                                        secondary: column == Column::SecondaryConstraint,
                                        kind: c,
                                    }),
                                );
                            }
                        }
                        None => errors.push(format!("{label}: {text:?} is not a constraint type")),
                    }
                }
                Column::ActivityCode(kind) => {
                    if src.is_empty() {
                        continue;
                    }
                    let text = src.text();
                    let new_id = if is_clear(src) {
                        None
                    } else {
                        match ctx.code_values.get(&(kind, text.to_lowercase())) {
                            Some(id) => Some(*id),
                            None => {
                                errors.push(format!("{label}: value {text:?} does not exist"));
                                continue;
                            }
                        }
                    };
                    let new = new_id
                        .and_then(|id| ctx.code_names.get(&id).cloned())
                        .unwrap_or_default();
                    if new != old_text {
                        push(
                            &mut row,
                            &label,
                            old_text,
                            new,
                            field(FieldEdit::Code {
                                type_id: kind,
                                value: new_id,
                            }),
                        );
                    }
                }
                Column::Udf(kind) => {
                    if src.is_empty() {
                        continue;
                    }
                    let parsed = match column.kind(&ctx.lookup) {
                        _ if is_clear(src) => Ok(Value::Empty),
                        Kind::Date => {
                            parse_date(src).map(|d| d.map_or(Value::Empty, |(m, _)| Value::Date(m)))
                        }
                        Kind::Number => match src {
                            Cell::Empty => Ok(Value::Empty),
                            c => match c {
                                Cell::Number(n) => Some(*n),
                                c => c.text().parse::<f64>().ok(),
                            }
                            .filter(|n| n.is_finite())
                            .map(Value::Number)
                            .ok_or_else(|| format!("{:?} is not a number", c.text())),
                        },
                        _ => Ok(if src.is_empty() {
                            Value::Empty
                        } else {
                            Value::Text(clean_text(&src.text()))
                        }),
                    };
                    match parsed {
                        Ok(v) => {
                            if v != old {
                                let new = display(&v, unit, hpd);
                                push(
                                    &mut row,
                                    &label,
                                    old_text,
                                    new,
                                    field(FieldEdit::Udf {
                                        type_id: kind,
                                        value: v,
                                    }),
                                );
                            }
                        }
                        Err(e) => errors.push(format!("{label}: {e}")),
                    }
                }
                _ => {}
            }
        }
        // Progress rules: a finish needs a start; percent and durations are applied
        // in a fixed order (durations first) so they can change together.
        let has_start = row
            .edits
            .iter()
            .find_map(|e| match e {
                Edit::Field(FieldEdit::ActualStart(s), _) => Some(s.is_some()),
                _ => None,
            })
            .unwrap_or_else(|| {
                target.is_some_and(|t| project.model.activities[t].act_start.is_some())
            });
        let finish_set = row
            .edits
            .iter()
            .any(|e| matches!(e, Edit::Field(FieldEdit::ActualFinish(Some(_)), _)));
        let finish_kept = target.is_some_and(|t| project.model.activities[t].act_finish.is_some())
            && !row
                .edits
                .iter()
                .any(|e| matches!(e, Edit::Field(FieldEdit::ActualFinish(None), _)));
        if (finish_set || finish_kept) && !has_start {
            errors.push("Actual Finish needs an Actual Start".into());
        }
        if target.is_none() {
            row.wbs_hint = band_wbs;
            if !row.edits.iter().any(|e| matches!(e, Edit::Name(_))) {
                row.messages.push("New activity has no name".into());
            }
        }
        // Clearing a constraint already clears its date, and the setters apply
        // the edits in the order that lets a row start and finish an activity.
        let off = |secondary: bool, edits: &[Edit]| {
            edits.iter().any(|e| match e {
                Edit::Field(FieldEdit::Constraint { secondary: s, kind }, _) => {
                    let clears = matches!(kind, CstrType::None | CstrType::Alap);
                    (*s == secondary && clears) || (secondary && !*s && *kind == CstrType::None)
                }
                _ => false,
            })
        };
        let (primary_off, secondary_off) = (off(false, &row.edits), off(true, &row.edits));
        row.edits.retain(|e| {
            !matches!(
                e,
                Edit::Field(FieldEdit::ConstraintDate { secondary, date: None }, _)
                    if if *secondary { secondary_off } else { primary_off }
            )
        });
        row.edits.sort_by_key(|e| match e {
            Edit::Field(f, _) => f.rank(),
            _ => 0,
        });
        if !errors.is_empty() {
            row.kind = RowKind::Error;
            row.messages.extend(errors);
            row.edits.clear();
        } else if target.is_none() {
            row.kind = RowKind::New;
            if !row.edits.iter().any(|e| matches!(e, Edit::Id(_))) {
                row.edits.insert(0, Edit::Id(code.clone()));
            }
        } else if row.changes.is_empty() {
            row.kind = RowKind::Unchanged;
            row.edits.clear();
        } else {
            row.kind = RowKind::Changed;
        }
        rows.push(row);
    }
    let mut out = ImportPlan {
        format: detected.format,
        settings: detected.settings.clone(),
        rows,
        links: vec![],
        assignments: vec![],
        notes,
    };
    // Relationships are checked against the project as it will be after the
    // activity rows, so they can name new or renamed activities.
    let mut working = project.clone();
    let _ = run_rows(&mut out, &mut working, setters, true);
    if let Some(l) = &detected.links {
        let deleted: HashSet<String> = out
            .rows
            .iter()
            .filter(|r| r.kind == RowKind::Delete)
            .map(|r| r.activity_id.clone())
            .collect();
        out.links = plan_links(wb, l, mapping, &working, hpd, &deleted);
    }
    if let Some(a) = &detected.assignments {
        let deleted: HashSet<String> = out
            .rows
            .iter()
            .filter(|r| r.kind == RowKind::Delete)
            .map(|r| r.activity_id.clone())
            .collect();
        out.assignments = plan_assignments(wb, a, mapping, &working, &deleted);
    }
    out
}

/// TASKRSRC rows of the project, by (`task_id`, `rsrc_id`) in table order,
/// with each row's `taskrsrc_id`.
fn assignment_index(p: &ProjectData) -> HashMap<(i64, i64), Vec<(usize, i64)>> {
    let mut index: HashMap<(i64, i64), Vec<(usize, i64)>> = HashMap::new();
    let pid = p.model.project.proj_id;
    let Some(t) = p.document.table("TASKRSRC") else {
        return index;
    };
    let col = |name: &str| t.field(name);
    let (Some(task), Some(rsrc), Some(proj), Some(id)) = (
        col("task_id"),
        col("rsrc_id"),
        col("proj_id"),
        col("taskrsrc_id"),
    ) else {
        return index;
    };
    let cell = |r: &[String], c: usize| r.get(c).map_or(0, |v| num(v));
    for (i, r) in t.rows.iter().enumerate() {
        if cell(r, proj) == pid {
            index
                .entry((cell(r, task), cell(r, rsrc)))
                .or_default()
                .push((i, cell(r, id)));
        }
    }
    index
}

fn plan_assignments(
    wb: &Workbook,
    layout: &SheetLayout,
    mapping: &Mapping,
    working: &ProjectData,
    deleted: &HashSet<String>,
) -> Vec<AssignPlan> {
    let sheet = &wb.sheets[layout.sheet];
    let col = |t: AssignTarget| mapping.assignment.iter().position(|x| *x == t);
    let (ac, rc, dc) = (
        col(AssignTarget::Activity),
        col(AssignTarget::Resource),
        col(AssignTarget::Delete),
    );
    let by_code: HashMap<&str, usize> = working
        .model
        .activities
        .iter()
        .enumerate()
        .map(|(i, a)| (a.task_code.as_str(), i))
        .collect();
    let resources: HashMap<String, i64> =
        working
            .document
            .table("RSRC")
            .map_or_else(HashMap::new, |t| {
                t.rows
                    .iter()
                    .map(|r| {
                        (
                            t.value(r, "rsrc_short_name").trim().to_lowercase(),
                            num(t.value(r, "rsrc_id")),
                        )
                    })
                    .collect()
            });
    let existing_rows = assignment_index(working);
    let mut seen: HashMap<(usize, i64), (usize, usize)> = HashMap::new();
    let mut out = vec![];
    for r in layout.first_data..sheet.rows.len() {
        if sheet.rows[r].iter().all(Cell::is_empty) {
            continue;
        }
        let get = |c: Option<usize>| c.map_or(Cell::Empty, |c| sheet.cell(r, c).clone());
        let activity = get(ac).text();
        let resource = get(rc).text();
        let mut plan = AssignPlan {
            sheet_row: r + 1,
            activity_id: activity.clone(),
            resource_id: resource.clone(),
            kind: RowKind::Unchanged,
            changes: vec![],
            message: String::new(),
            units: AssignUnits::default(),
            rsrc: 0,
            existing: None,
        };
        let fail = |plan: &mut AssignPlan, m: String| {
            plan.kind = RowKind::Error;
            plan.message = m;
        };
        let broken = error_cells(r, &sheet.rows[r], &layout.headers, |c| {
            mapping
                .assignment
                .get(c)
                .is_some_and(|t| *t != AssignTarget::Ignore)
        });
        if !broken.is_empty() {
            fail(&mut plan, broken.join("; "));
            out.push(plan);
            continue;
        }
        if ac.is_none() || rc.is_none() {
            fail(
                &mut plan,
                "Map the Activity ID and Resource ID columns".into(),
            );
            out.push(plan);
            continue;
        }
        if deleted.contains(&activity) {
            plan.kind = RowKind::Skipped;
            plan.message = format!("Removed with deleted activity {activity}");
            out.push(plan);
            continue;
        }
        let Some(&i) = by_code.get(activity.as_str()) else {
            fail(&mut plan, format!("Activity {activity:?} does not exist"));
            out.push(plan);
            continue;
        };
        let Some(&rsrc) = resources.get(&resource.trim().to_lowercase()) else {
            fail(
                &mut plan,
                format!("Resource {resource:?} is not in this project file"),
            );
            out.push(plan);
            continue;
        };
        plan.rsrc = rsrc;
        let a = &working.model.activities[i];
        let (first, count) = seen.entry((i, rsrc)).or_insert((r + 1, 0));
        let nth = *count;
        *count += 1;
        let matched = existing_rows
            .get(&(a.task_id, rsrc))
            .and_then(|rows| rows.get(nth))
            .copied();
        if nth > 0 && matched.is_none() {
            fail(
                &mut plan,
                format!("Row {first} already sets this assignment"),
            );
            out.push(plan);
            continue;
        }
        plan.existing = matched.map(|(_, id)| id);
        let existing = matched.map(|(row, _)| row);
        if dc.is_some_and(|c| is_delete_flag(sheet.cell(r, c))) {
            if existing.is_some() {
                plan.kind = RowKind::Delete;
                plan.message = "Removes the assignment".into();
            } else {
                fail(&mut plan, "No such assignment to delete".into());
            }
            out.push(plan);
            continue;
        }
        let mut errors = vec![];
        for (target, slot) in [
            (AssignTarget::BudgetedUnits, 0),
            (AssignTarget::RemainingUnits, 1),
            (AssignTarget::ActualUnits, 2),
        ] {
            let Some(c) = col(target) else {
                continue;
            };
            let cell = sheet.cell(r, c);
            if cell.is_empty() {
                continue;
            }
            let factor = match mapping.assignment_units.get(c) {
                Some(DurationUnit::Days) => mapping.hours_per_day,
                _ => 1.,
            };
            let value = match cell {
                Cell::Number(n) => Some(*n),
                other => other
                    .text()
                    .replace(',', "")
                    .trim_end_matches('h')
                    .trim()
                    .parse::<f64>()
                    .ok(),
            }
            .filter(|v| v.is_finite() && *v >= 0.)
            .map(|v| v * factor);
            match value {
                Some(v) => match slot {
                    0 => plan.units.budgeted = Some(v),
                    1 => plan.units.remaining = Some(v),
                    _ => plan.units.actual = Some(v),
                },
                None => errors.push(format!(
                    "{}: {:?} is not a number of zero or more",
                    target.label(),
                    cell.text()
                )),
            }
        }
        if matches!(
            a.task_type,
            openxer_engine::model::TaskType::Mile | openxer_engine::model::TaskType::FinMile
        ) {
            errors.push("Milestones take no resource assignments".into());
        }
        let started = a.status != StatusType::NotStarted;
        let view = existing.and_then(|row| working.assignment_view(row));
        let actual_now = view.as_ref().map_or(0., |v| v.actual_regular_units);
        if plan
            .units
            .actual
            .is_some_and(|v| (v - actual_now).abs() > 1e-6)
            && !started
        {
            errors.push("Actual units need a started activity".into());
        }
        if plan.units.remaining.is_some_and(|v| v > 0.) && a.is_completed() {
            errors.push("A completed activity has no remaining units".into());
        }
        if !errors.is_empty() {
            fail(&mut plan, errors.join("; "));
            out.push(plan);
            continue;
        }
        let mut change = |column: &str, old: Option<f64>, new: Option<f64>| {
            if let Some(n) = new {
                if old.is_none_or(|o| (o - n).abs() > 1e-6) {
                    plan.changes.push(CellChange {
                        column: column.into(),
                        old: old.map_or(String::new(), format_number),
                        new: format_number(n),
                    });
                }
            }
        };
        change(
            "Budgeted Units",
            view.as_ref().map(|v| v.budgeted.units),
            plan.units.budgeted,
        );
        change(
            "Remaining Units",
            view.as_ref().map(|v| v.remaining.units),
            plan.units.remaining,
        );
        change(
            "Actual Regular Units",
            view.as_ref().map(|v| v.actual_regular_units),
            plan.units.actual,
        );
        plan.kind = match (existing, plan.changes.is_empty()) {
            (None, _) => RowKind::New,
            (Some(_), true) => RowKind::Unchanged,
            (Some(_), false) => RowKind::Changed,
        };
        if plan.kind == RowKind::New {
            plan.message = "Assigns the resource".into();
        }
        out.push(plan);
    }
    out
}

/// Apply the assignment rows of a plan; returns (added, changed, removed).
fn apply_assignments(
    plan: &ImportPlan,
    p: &mut ProjectData,
) -> Result<(usize, usize, usize), String> {
    use openxer_xer::assignments::AssignmentEdit;
    let mut counts = (0, 0, 0);
    let by_code: HashMap<String, i64> = p
        .model
        .activities
        .iter()
        .map(|a| (a.task_code.clone(), a.task_id))
        .collect();
    // Rows are found by their id; removals wait until the end so the row
    // numbers of the others stay put.
    let rows: HashMap<i64, usize> = assignment_index(p)
        .into_values()
        .flatten()
        .map(|(row, id)| (id, row))
        .collect();
    let mut doomed = vec![];
    for a in &plan.assignments {
        if !matches!(a.kind, RowKind::New | RowKind::Changed | RowKind::Delete) {
            continue;
        }
        let task_id = *by_code
            .get(&a.activity_id)
            .ok_or_else(|| format!("Activity {} disappeared", a.activity_id))?;
        let found = a.existing.and_then(|id| rows.get(&id).copied());
        let row = match (a.kind, found) {
            (RowKind::Delete, Some(row)) => {
                doomed.push(row);
                continue;
            }
            (RowKind::New, None) => {
                counts.0 += 1;
                p.add_assignment(task_id, Some(a.rsrc), None)?
            }
            (_, Some(row)) => {
                counts.1 += 1;
                row
            }
            _ => continue,
        };
        let u = &a.units;
        let differs =
            |p: &ProjectData, v: f64, f: fn(&openxer_xer::assignments::AssignmentView) -> f64| {
                p.assignment_view(row)
                    .is_none_or(|x| (f(&x) - v).abs() > 1e-9)
            };
        if let Some(v) = u.budgeted.filter(|v| differs(p, *v, |x| x.budgeted.units)) {
            p.edit_assignment(row, AssignmentEdit::BudgetedUnits(v))?;
        }
        if let Some(v) = u
            .remaining
            .filter(|v| differs(p, *v, |x| x.remaining.units))
        {
            p.edit_assignment(row, AssignmentEdit::RemainingUnits(v))?;
        }
        if let Some(v) = u.actual {
            let current = p
                .assignment_view(row)
                .map_or(0., |v| v.actual_regular_units);
            if (v - current).abs() > 1e-9 {
                p.edit_assignment(row, AssignmentEdit::ActualRegularUnits(v))?;
            }
        }
    }
    doomed.sort_unstable();
    doomed.dedup();
    for row in doomed.into_iter().rev() {
        p.remove_assignment(row)?;
        counts.2 += 1;
    }
    Ok(counts)
}

fn plan_links(
    wb: &Workbook,
    layout: &SheetLayout,
    mapping: &Mapping,
    working: &ProjectData,
    hpd: f64,
    deleted: &HashSet<String>,
) -> Vec<LinkPlan> {
    let sheet = &wb.sheets[layout.sheet];
    let col = |t: LinkTarget| mapping.link.iter().position(|x| *x == t);
    let (pc, sc, tc, lc, dc) = (
        col(LinkTarget::Predecessor),
        col(LinkTarget::Successor),
        col(LinkTarget::Type),
        col(LinkTarget::Lag),
        col(LinkTarget::Delete),
    );
    let by_code: HashMap<&str, usize> = working
        .model
        .activities
        .iter()
        .enumerate()
        .map(|(i, a)| (a.task_code.as_str(), i))
        .collect();
    let mut model = working.model.clone();
    let mut out = vec![];
    // Rows that mention each (pred, succ, type), so a changed type is not mistaken
    // for a removal of the old one.
    let mut mentioned: HashSet<(usize, usize, &'static str)> = HashSet::new();
    let mut parsed_rows = vec![];
    for r in layout.first_data..sheet.rows.len() {
        if sheet.rows[r].iter().all(Cell::is_empty) {
            continue;
        }
        let get = |c: Option<usize>| c.map_or(Cell::Empty, |c| sheet.cell(r, c).clone());
        let pred = get(pc).text();
        let succ = get(sc).text();
        let kind = get(tc).text();
        let plan = LinkPlan {
            sheet_row: r + 1,
            predecessor: pred.clone(),
            successor: succ.clone(),
            kind: RowKind::Unchanged,
            change: String::new(),
            message: String::new(),
            op: LinkOp::None,
        };
        let fail = |mut plan: LinkPlan, m: String| {
            plan.kind = RowKind::Error;
            plan.message = m;
            plan
        };
        let broken = error_cells(r, &sheet.rows[r], &layout.headers, |c| {
            mapping
                .link
                .get(c)
                .is_some_and(|t| *t != LinkTarget::Ignore)
        });
        if !broken.is_empty() {
            parsed_rows.push(Err(fail(plan, broken.join("; "))));
            continue;
        }
        if pc.is_none() || sc.is_none() {
            parsed_rows.push(Err(fail(
                plan,
                "Map the Predecessor and Successor columns".into(),
            )));
            continue;
        }
        if let Some(gone) = [&pred, &succ].into_iter().find(|c| deleted.contains(*c)) {
            let mut plan = plan;
            plan.kind = RowKind::Skipped;
            plan.message = format!("Removed with deleted activity {gone}");
            parsed_rows.push(Err(plan));
            continue;
        }
        let (Some(&p), Some(&s)) = (by_code.get(pred.as_str()), by_code.get(succ.as_str())) else {
            let missing = if by_code.contains_key(pred.as_str()) {
                &succ
            } else {
                &pred
            };
            parsed_rows.push(Err(fail(
                plan,
                format!("Activity {missing:?} does not exist"),
            )));
            continue;
        };
        let link_type = if kind.is_empty() {
            Some(LinkType::FS)
        } else {
            parse_link_type(&kind)
        };
        let Some(link_type) = link_type else {
            parsed_rows.push(Err(fail(plan, format!("{kind:?} is not FS, SS, FF or SF"))));
            continue;
        };
        let lag = match parse_lag(&get(lc), mapping.link_unit, hpd) {
            Ok(v) => v,
            Err(e) => {
                parsed_rows.push(Err(fail(plan, format!("Lag: {e}"))));
                continue;
            }
        };
        mentioned.insert((p, s, link_type.as_str()));
        let delete = dc.is_some_and(|c| is_delete_flag(sheet.cell(r, c)));
        parsed_rows.push(Ok((plan, p, s, link_type, lag, delete)));
    }
    // One check of the whole network with every relationship the sheet
    // names: when that has no loop, no relationship added below can close
    // one, and none needs a walk of its own.
    let pairs: Vec<(usize, usize)> = parsed_rows
        .iter()
        .filter_map(|item| match item {
            Ok((_, p, s, _, _, false)) => Some((*p, *s)),
            _ => None,
        })
        .collect();
    let mut loops = (!relationships::acyclic_with(&model, &pairs)).then(|| Loops::new(&model));
    for item in parsed_rows {
        let (mut plan, p, s, link_type, lag, delete) = match item {
            Ok(x) => x,
            Err(plan) => {
                out.push(plan);
                continue;
            }
        };
        let tname = |t: LinkType| t.as_str().trim_start_matches("PR_").to_string();
        let exact = model
            .links
            .iter()
            .position(|l| l.pred == p && l.succ == s && l.link_type == link_type);
        if delete {
            match exact {
                Some(i) => match relationships::remove(&mut model, i) {
                    Ok(()) => {
                        if let Some(l) = loops.as_mut() {
                            l.remove(p, s);
                        }
                        plan.kind = RowKind::Delete;
                        plan.change = format!("Remove {}", tname(link_type));
                        plan.op = LinkOp::Remove(link_type);
                    }
                    Err(e) => {
                        plan.kind = RowKind::Error;
                        plan.message = e;
                    }
                },
                None => {
                    plan.kind = RowKind::Error;
                    plan.message = "No such relationship to delete".into();
                }
            }
            out.push(plan);
            continue;
        }
        let lag_text = |m: i64| {
            format!(
                "{}{}",
                format_number(mapping.link_unit.from_minutes(m, hpd)),
                mapping.link_unit.suffix()
            )
        };
        if let Some(i) = exact {
            let new_lag = lag.unwrap_or(model.links[i].lag);
            if new_lag != model.links[i].lag {
                let old_lag = model.links[i].lag;
                match relationships::edit(&mut model, i, link_type, new_lag as f64 / 60.0) {
                    Ok(()) => {
                        plan.kind = RowKind::Changed;
                        plan.change = format!("Lag {} -> {}", lag_text(old_lag), lag_text(new_lag));
                        plan.op = LinkOp::Update {
                            old: link_type,
                            new: link_type,
                            lag: new_lag,
                        };
                    }
                    Err(e) => {
                        plan.kind = RowKind::Error;
                        plan.message = e;
                    }
                }
            }
            out.push(plan);
            continue;
        }
        // A single existing link of another type that no row mentions: a type change.
        let others: Vec<usize> = model
            .links
            .iter()
            .enumerate()
            .filter(|(_, l)| {
                l.pred == p && l.succ == s && !mentioned.contains(&(p, s, l.link_type.as_str()))
            })
            .map(|(i, _)| i)
            .collect();
        if others.len() == 1 {
            let i = others[0];
            let old = model.links[i].link_type;
            let old_lag = model.links[i].lag;
            let new_lag = lag.unwrap_or(old_lag);
            match relationships::edit(&mut model, i, link_type, new_lag as f64 / 60.0) {
                Ok(()) => {
                    plan.kind = RowKind::Changed;
                    plan.change = format!("Type {} -> {}", tname(old), tname(link_type));
                    if new_lag != old_lag {
                        plan.change.push_str(&format!(
                            ", lag {} -> {}",
                            lag_text(old_lag),
                            lag_text(new_lag)
                        ));
                    }
                    plan.op = LinkOp::Update {
                        old,
                        new: link_type,
                        lag: new_lag,
                    };
                }
                Err(e) => {
                    plan.kind = RowKind::Error;
                    plan.message = e;
                }
            }
            out.push(plan);
            continue;
        }
        let refused = if p == s {
            Some("An activity cannot be its own predecessor")
        } else if loops.as_mut().is_some_and(|l| l.reaches(s, p)) {
            Some("This relationship would create a loop")
        } else {
            None
        };
        match refused {
            None => {
                let lag = lag.unwrap_or(0);
                model.links.push(Link {
                    pred: p,
                    succ: s,
                    link_type,
                    lag,
                    driving_path: false,
                });
                if let Some(l) = loops.as_mut() {
                    l.add(p, s);
                }
                plan.kind = RowKind::New;
                plan.change = format!("Add {} lag {}", tname(link_type), lag_text(lag));
                plan.op = LinkOp::Add(link_type, lag);
            }
            Some(e) => {
                plan.kind = RowKind::Error;
                plan.message = e.into();
            }
        }
        out.push(plan);
    }
    out
}

/// The relationship network as successor lists, kept up to date as the
/// preview adds and removes relationships, for the loop check of a sheet
/// whose relationships together close a loop.
struct Loops {
    successors: Vec<Vec<usize>>,
    seen: Vec<u32>,
    stamp: u32,
}
impl Loops {
    fn new(model: &openxer_engine::model::Model) -> Self {
        let mut successors = vec![Vec::new(); model.activities.len()];
        for l in &model.links {
            successors[l.pred].push(l.succ);
        }
        Self {
            seen: vec![0; successors.len()],
            successors,
            stamp: 0,
        }
    }
    fn add(&mut self, pred: usize, succ: usize) {
        self.successors[pred].push(succ);
    }
    fn remove(&mut self, pred: usize, succ: usize) {
        if let Some(i) = self.successors[pred].iter().position(|&x| x == succ) {
            self.successors[pred].swap_remove(i);
        }
    }
    /// Whether `to` follows `from` along relationships.
    fn reaches(&mut self, from: usize, to: usize) -> bool {
        self.stamp += 1;
        let mut pending = vec![from];
        while let Some(i) = pending.pop() {
            if i == to {
                return true;
            }
            if std::mem::replace(&mut self.seen[i], self.stamp) == self.stamp {
                continue;
            }
            pending.extend(self.successors[i].iter().copied());
        }
        false
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ApplyReport {
    pub assignments_added: usize,
    pub assignments_changed: usize,
    pub assignments_deleted: usize,
    pub changed: usize,
    pub added: usize,
    pub deleted: usize,
    pub links_added: usize,
    pub links_changed: usize,
    pub links_deleted: usize,
    pub skipped_errors: usize,
}

/// Apply the planned activity rows to `p` through `setters`.
///
/// With `lenient` (planning on a copy) a refused cell marks its row as an
/// error and the run goes on; otherwise the first refusal ends the run with
/// its message.
fn run_rows(
    plan: &mut ImportPlan,
    p: &mut ProjectData,
    setters: &mut dyn Setters,
    lenient: bool,
) -> Result<ApplyReport, String> {
    let mut report = ApplyReport::default();
    let pid = p.model.project.proj_id;
    let mut doomed = vec![];
    for row in &mut plan.rows {
        match row.kind {
            RowKind::Changed => {
                let id = row.task_id.ok_or("Changed row without an activity")?;
                let i = p
                    .model
                    .activities
                    .iter()
                    .position(|a| a.task_id == id)
                    .ok_or("Activity disappeared")?;
                match apply_edits(p, i, &row.edits, setters) {
                    Ok(()) => report.changed += 1,
                    Err(e) if lenient => refuse(row, e),
                    Err(e) => return Err(format!("{}: {e}", row.activity_id)),
                }
            }
            RowKind::New => {
                let wbs = row
                    .edits
                    .iter()
                    .find_map(|e| match e {
                        Edit::Field(FieldEdit::Wbs(w), _) => Some(*w),
                        _ => None,
                    })
                    .or(row.wbs_hint)
                    .or_else(|| p.wbs.first().map(|w| w.id))
                    .unwrap_or(0);
                let i = p.add_activity(wbs);
                // New activities start not started with zero progress.
                let id = p.model.activities[i].task_id;
                if !row.edits.iter().any(|e| matches!(e, Edit::Name(_))) {
                    activity::set_activity_name(p, i, "")?;
                }
                match apply_edits(p, i, &row.edits, setters) {
                    Ok(()) => report.added += 1,
                    Err(e) if lenient => {
                        p.remove_model_activities(&[id])?;
                        refuse(row, e);
                    }
                    Err(e) => return Err(format!("{}: {e}", row.activity_id)),
                }
            }
            RowKind::Delete => {
                if let Some(id) = row.task_id {
                    doomed.push(id);
                }
            }
            RowKind::Error => report.skipped_errors += 1,
            _ => {}
        }
    }
    if !doomed.is_empty() {
        report.deleted = doomed.len();
        remove_activities(p, &doomed)?;
        let ids: Vec<String> = doomed.iter().map(i64::to_string).collect();
        delete_activities(&mut p.document, pid, &ids);
    }
    Ok(report)
}

/// Turn a row the setters refused into an error row.
fn refuse(row: &mut RowPlan, message: String) {
    row.kind = RowKind::Error;
    row.messages.push(message);
    row.edits.clear();
}

fn remove_activities(p: &mut ProjectData, ids: &[i64]) -> Result<(), String> {
    p.remove_model_activities(ids)
}

/// Apply one row's edits in order; a refusal names the column. The edits that
/// ran before it stay applied (the caller discards a failed copy).
fn apply_edits(
    p: &mut ProjectData,
    i: usize,
    edits: &[Edit],
    setters: &mut dyn Setters,
) -> Result<(), String> {
    for e in edits {
        match e {
            Edit::Id(c) => {
                activity::set_activity_id(p, i, c).map_err(|m| format!("Activity ID: {m}"))?;
            }
            Edit::Name(n) => {
                activity::set_activity_name(p, i, n).map_err(|m| format!("Activity Name: {m}"))?;
            }
            Edit::Field(edit, label) => {
                setters
                    .apply(p, i, edit)
                    .map_err(|m| format!("{label}: {m}"))?;
            }
        }
    }
    Ok(())
}

fn t_value<'a>(fields: &[String], row: &'a [String], name: &str) -> &'a str {
    fields
        .iter()
        .position(|f| f.eq_ignore_ascii_case(name))
        .and_then(|i| row.get(i))
        .map_or("", String::as_str)
}

pub(crate) fn set_udf(p: &mut ProjectData, task_id: i64, pid: i64, kind: i64, value: &Value) {
    let t = p.document.table_mut("UDFVALUE");
    if t.fields.is_empty() {
        t.fields = [
            "udf_type_id",
            "fk_id",
            "proj_id",
            "udf_date",
            "udf_number",
            "udf_text",
            "udf_code_id",
        ]
        .map(String::from)
        .to_vec();
    }
    t.rows.retain(|r| {
        !(num(t_value(&t.fields, r, "udf_type_id")) == kind
            && num(t_value(&t.fields, r, "fk_id")) == task_id)
    });
    let mut values = vec![
        ("udf_type_id", kind.to_string()),
        ("fk_id", task_id.to_string()),
        ("proj_id", pid.to_string()),
    ];
    match value {
        Value::Empty => return,
        Value::Date(m) => values.push(("udf_date", openxer_xer::date_string(0, Some(*m)))),
        Value::Number(n) => values.push(("udf_number", format_number(*n))),
        v => values.push((
            "udf_text",
            match v {
                Value::Text(s) => clean_text(s),
                _ => String::new(),
            },
        )),
    }
    t.add(&values);
}

/// Apply the plan to the project. Rows with errors are skipped (they are listed in
/// the plan); a failure part way leaves `project` unchanged.
pub fn apply(plan: &ImportPlan, project: &mut ProjectData) -> Result<ApplyReport, String> {
    apply_with(plan, project, &mut CrateSetters)
}

/// `apply` with the caller's setters. The first cell they refuse ends the
/// import and leaves `project` unchanged.
pub fn apply_with(
    plan: &ImportPlan,
    project: &mut ProjectData,
    setters: &mut dyn Setters,
) -> Result<ApplyReport, String> {
    let mut next = project.clone();
    let mut rows = plan.clone();
    let mut report = run_rows(&mut rows, &mut next, setters, false)?;
    let by_code: HashMap<String, usize> = next
        .model
        .activities
        .iter()
        .enumerate()
        .map(|(i, a)| (a.task_code.clone(), i))
        .collect();
    // Relationship changes go through the edit layer, in the order the
    // preview planned them. Updates are found by (predecessor, successor,
    // type); removals are made after the updates and additions after both,
    // all at once, so the network is rebuilt once and not per row.
    let mut index: HashMap<(usize, usize, &'static str), usize> = HashMap::new();
    for (i, l) in next.model.links.iter().enumerate() {
        index
            .entry((l.pred, l.succ, l.link_type.as_str()))
            .or_insert(i);
    }
    let mut removed = vec![];
    let mut added = vec![];
    for l in &plan.links {
        let (Some(&p), Some(&s)) = (by_code.get(&l.predecessor), by_code.get(&l.successor)) else {
            continue;
        };
        match &l.op {
            LinkOp::None => {}
            LinkOp::Add(t, lag) => added.push((p, s, *t, *lag)),
            LinkOp::Update { old, new, lag } => {
                if let Some(i) = index.remove(&(p, s, old.as_str())) {
                    relationships::edit(&mut next.model, i, *new, *lag as f64 / 60.0).map_err(
                        |e| format!("Relationship {} to {}: {e}", l.predecessor, l.successor),
                    )?;
                    index.insert((p, s, new.as_str()), i);
                    report.links_changed += 1;
                }
            }
            LinkOp::Remove(t) => {
                if let Some(i) = index.remove(&(p, s, t.as_str())) {
                    removed.push(i);
                }
            }
        }
    }
    removed.sort_unstable();
    for &i in removed.iter().rev() {
        relationships::remove(&mut next.model, i)?;
        report.links_deleted += 1;
    }
    relationships::add_many(&mut next.model, &added)?;
    report.links_added += added.len();
    report.skipped_errors += plan
        .links
        .iter()
        .filter(|l| l.kind == RowKind::Error)
        .count();
    next.model.rebuild_links()?;
    next.model.validate()?;
    let (added, changed, deleted) = apply_assignments(plan, &mut next)?;
    report.assignments_added = added;
    report.assignments_changed = changed;
    report.assignments_deleted = deleted;
    report.skipped_errors += plan
        .assignments
        .iter()
        .filter(|a| a.kind == RowKind::Error)
        .count();
    *project = next;
    Ok(report)
}

/// Read, detect and plan with automatic column mapping (CLI and tests).
pub fn plan_file(
    path: &Path,
    project: &ProjectData,
) -> Result<(Workbook, Detected, Mapping, ImportPlan), String> {
    let wb = read_workbook(path)?;
    let lookup = Lookup::new(project);
    let detected = detect(&wb, &lookup)?;
    let mapping = auto_map(&detected, &lookup);
    let plan = plan(&wb, &detected, &mapping, project);
    Ok((wb, detected, mapping, plan))
}

/// Catalog choices offered for a column on the mapping step.
pub fn target_choices(lookup: &Lookup) -> Vec<(Target, String)> {
    let mut out = vec![
        (Target::Ignore, "(ignore)".to_string()),
        (Target::Delete, "Delete flag".to_string()),
        (Target::Key, "OpenXER row key".to_string()),
    ];
    out.extend(
        lookup
            .catalog()
            .into_iter()
            .map(|c| (Target::Column(c), c.title(lookup))),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> i64 {
        (NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            - epoch())
        .num_minutes()
    }

    /// Regression: `%m/%d/%Y` came before `%m/%d/%y`, and the four-digit
    /// year directive also takes two digits, so "1/2/26" was 2 January of
    /// the year 26 and the preview, which shows two-digit years, hid it.
    #[test]
    fn two_digit_years_are_this_century_and_far_years_are_refused() {
        let parse = |s: &str| parse_date(&Cell::Text(s.into()));
        assert_eq!(parse("1/2/26"), Ok(Some((day(2026, 1, 2), true))));
        assert_eq!(parse("1/2/2026"), Ok(Some((day(2026, 1, 2), true))));
        assert_eq!(
            parse("1/2/26 08:00"),
            Ok(Some((day(2026, 1, 2) + 8 * 60, false)))
        );
        assert_eq!(parse("02-Jan-26"), Ok(Some((day(2026, 1, 2), true))));
        assert_eq!(
            parse("02-Jan-2026 17:00"),
            Ok(Some((day(2026, 1, 2) + 17 * 60, false)))
        );
        for bad in ["1/2/0026", "0026-01-02", "26-01-02 08:00", "02-Jan-0026"] {
            let e = parse(bad).unwrap_err();
            assert!(e.contains("outside"), "{bad}: {e}");
        }
        let e = parse_date(&Cell::Date(150_000.0)).unwrap_err();
        assert!(e.contains("outside"), "{e}");
        assert!(parse_date(&Cell::Date(46_000.5)).is_ok());
    }

    /// A stored (uncompressed) zip archive of `files`.
    fn zip(files: &[(&str, String)]) -> Vec<u8> {
        fn crc32(data: &[u8]) -> u32 {
            let mut crc = !0u32;
            for &b in data {
                crc ^= b as u32;
                for _ in 0..8 {
                    crc = if crc & 1 != 0 {
                        (crc >> 1) ^ 0xEDB8_8320
                    } else {
                        crc >> 1
                    };
                }
            }
            !crc
        }
        let (mut out, mut central) = (Vec::new(), Vec::new());
        for (name, text) in files {
            let (data, offset, crc) = (text.as_bytes(), out.len() as u32, crc32(text.as_bytes()));
            let header = |sig: u32, central: bool| {
                let mut h = sig.to_le_bytes().to_vec();
                if central {
                    h.extend(20u16.to_le_bytes());
                }
                for v in [20u16, 0, 0, 0, 0] {
                    h.extend(v.to_le_bytes());
                }
                h.extend(crc.to_le_bytes());
                h.extend((data.len() as u32).to_le_bytes());
                h.extend((data.len() as u32).to_le_bytes());
                h.extend((name.len() as u16).to_le_bytes());
                h.extend(0u16.to_le_bytes());
                if central {
                    for v in [0u16, 0, 0] {
                        h.extend(v.to_le_bytes());
                    }
                    h.extend(0u32.to_le_bytes());
                    h.extend(offset.to_le_bytes());
                }
                h.extend(name.as_bytes());
                h
            };
            out.extend(header(0x0403_4b50, false));
            out.extend(data);
            central.extend(header(0x0201_4b50, true));
        }
        let (start, size) = (out.len() as u32, central.len() as u32);
        out.extend(central);
        out.extend(0x0605_4b50u32.to_le_bytes());
        for v in [0u16, 0, files.len() as u16, files.len() as u16] {
            out.extend(v.to_le_bytes());
        }
        out.extend(size.to_le_bytes());
        out.extend(start.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out
    }

    /// A one-sheet workbook written by hand, so a test controls what a
    /// spreadsheet writer would not produce: the 1904 date system, error
    /// cells and stray far-away cells. Style 1 is a date, 2 is `0%`, 3 is
    /// `0.0%` (custom) and 4 is a number with a literal percent sign.
    fn handmade(name: &str, date1904: bool, cells: &str) -> std::path::PathBuf {
        let ns = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
        let rel = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
        let files = [
            (
                "[Content_Types].xml",
                r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/></Types>"#.to_string(),
            ),
            (
                "_rels/.rels",
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{rel}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
                ),
            ),
            (
                "xl/workbook.xml",
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="{ns}" xmlns:r="{rel}"><workbookPr{}/><sheets><sheet name="Updates" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
                    if date1904 { r#" date1904="1""# } else { "" }
                ),
            ),
            (
                "xl/_rels/workbook.xml.rels",
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{rel}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{rel}/styles" Target="styles.xml"/></Relationships>"#
                ),
            ),
            (
                "xl/styles.xml",
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?><styleSheet xmlns="{ns}"><numFmts count="3"><numFmt numFmtId="164" formatCode="dd-mmm-yy hh:mm"/><numFmt numFmtId="165" formatCode="0.0%"/><numFmt numFmtId="166" formatCode="0&quot;%&quot;"/></numFmts><fonts count="1"><font/></fonts><fills count="1"><fill/></fills><borders count="1"><border/></borders><cellXfs count="5"><xf numFmtId="0"/><xf numFmtId="164"/><xf numFmtId="9"/><xf numFmtId="165"/><xf numFmtId="166"/></cellXfs></styleSheet>"#
                ),
            ),
            (
                "xl/worksheets/sheet1.xml",
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="{ns}"><sheetData>{cells}</sheetData></worksheet>"#
                ),
            ),
        ];
        let path = std::env::temp_dir().join(format!(
            "openxer-handmade-{name}-{}.xlsx",
            std::process::id()
        ));
        std::fs::write(&path, zip(&files)).unwrap();
        path
    }

    fn text(r: &str, t: &str) -> String {
        format!(r#"<c r="{r}" t="inlineStr"><is><t>{t}</t></is></c>"#)
    }

    /// Regression: a 1904 workbook stores day numbers 1,462 days smaller
    /// for the same date, and the raw number was imported.
    #[test]
    fn dates_of_a_1904_workbook_are_not_four_years_early() {
        // 2026-01-02 08:00 is day 46024.3333 in the 1900 system.
        let serial_1904 = 46024.0 + 1.0 / 3.0 - 1462.0;
        let cells = format!(
            r#"<row r="1">{}</row><row r="2"><c r="A2" s="1"><v>{serial_1904}</v></c></row>"#,
            text("A1", "Actual Start")
        );
        for (date1904, expected) in [(true, 46024.0 + 1.0 / 3.0), (false, serial_1904)] {
            let path = handmade(&format!("epoch-{date1904}"), date1904, &cells);
            let wb = read_workbook(&path).unwrap();
            std::fs::remove_file(&path).unwrap();
            match wb.sheets[0].cell(1, 0) {
                Cell::Date(d) => assert!((d - expected).abs() < 1e-9, "{d} {expected}"),
                c => panic!("{c:?}"),
            }
        }
    }

    /// Regression: an Excel error cell (#N/A and the like) was read as an
    /// empty cell, so a broken formula silently meant "no value".
    #[test]
    fn error_cells_are_read_as_errors() {
        let cells = format!(
            r#"<row r="1">{}{}</row><row r="2">{}<c r="B2" t="e"><v>#N/A</v></c></row>"#,
            text("A1", "Activity ID"),
            text("B1", "Actual Start"),
            text("A2", "RB-1010")
        );
        let path = handmade("errors", false, &cells);
        let wb = read_workbook(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(*wb.sheets[0].cell(1, 1), Cell::Error("#N/A".into()));
    }

    /// A cell formatted as a percentage holds the fraction (50% is 0.5);
    /// it is read as the number the user sees. A literal percent sign in
    /// quotes is not a percentage format.
    #[test]
    fn percent_formatted_cells_read_as_shown() {
        let cells = format!(
            r#"<row r="1">{}</row><row r="2"><c r="A2" s="2"><v>0.5</v></c><c r="B2" s="3"><v>0.07</v></c><c r="C2" s="4"><v>40</v></c><c r="D2"><v>0.25</v></c></row>"#,
            text("A1", "Physical % Complete")
        );
        let path = handmade("percent", false, &cells);
        let wb = read_workbook(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        let s = &wb.sheets[0];
        assert_eq!(*s.cell(1, 0), Cell::Number(50.0));
        assert_eq!(*s.cell(1, 1), Cell::Number(7.0));
        assert_eq!(*s.cell(1, 2), Cell::Number(40.0));
        assert_eq!(*s.cell(1, 3), Cell::Number(0.25));
    }

    /// A value in a far corner of a sheet would make the grid billions of
    /// cells; it is refused with a message naming the cell.
    #[test]
    fn a_stray_far_away_cell_is_refused_not_allocated() {
        let cells = format!(
            r#"<row r="1">{}</row><row r="1048576">{}</row>"#,
            text("A1", "Activity ID"),
            text("XFD1048576", "x")
        );
        let path = handmade("far", false, &cells);
        let err = read_workbook(&path).unwrap_err();
        std::fs::remove_file(&path).unwrap();
        assert!(err.contains("XFD1048576"), "{err}");
    }
}
