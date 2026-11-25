//! Spreadsheet export in two flavors.
//!
//! Modern (OpenXER default): what you see in the activity layout, as a real
//! spreadsheet: typed dates, numeric durations in a stated unit, WBS outline
//! levels, frozen panes, autofilter, conditional formats, plus Relationships,
//! Resource Assignments and an About sheet that makes reimport exact.
//!
//! Tables layout: sheets TASK, TASKPRED, TASKRSRC and USERDATA with XER field names in
//! row 1, titles in row 2, `(*)` on calculated columns and a trailing Delete column.
use crate::columns::{
    displayed_finish, displayed_start, value, Column, DurationUnit, Kind, Lookup, Value,
};
use crate::util::{num, Col};
use openxer_engine::calendar::DAY;
use openxer_xer::{is_critical, ProjectData};
use rust_xlsxwriter::{
    Color, ConditionalFormatFormula, ExcelDateTime, Format, FormatAlign, FormatBorder, Workbook,
    Worksheet, XlsxError,
};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

pub const FORMAT_MARK: &str = "OpenXER activity export";
pub const FORMAT_VERSION: f64 = 1.0;
pub const ACTIVITY_SHEET: &str = "Activities";
pub const LINK_SHEET: &str = "Relationships";
pub const ASSIGNMENT_SHEET: &str = "Resource Assignments";
pub const ABOUT_SHEET: &str = "About";
/// Hidden helper columns at the right edge of the modern Activities sheet.
pub const KEY_HEADER: &str = "OpenXER Key";
pub const CRITICAL_HEADER: &str = "OpenXER Critical";
pub const FLOAT_HEADER: &str = "OpenXER Total Float";
pub const DELETE_HEADER: &str = "Delete";
/// Text date format used where dates must be text (tables-layout sheets).
/// Four-digit years; the import also reads the two-digit form written by
/// earlier versions.
pub const TABLES_TEXT_DATE: &str = "%d-%b-%Y %H:%M";

#[derive(Clone, Debug)]
pub struct ExportOptions {
    pub columns: Vec<Column>,
    pub unit: DurationUnit,
    /// Excel number format for date cells, for example `dd-mmm-yy hh:mm`.
    pub date_format: String,
    pub group_by_wbs: bool,
    /// Activities to export in display order; `None` exports every activity.
    pub rows: Option<Vec<usize>>,
    /// Caller-computed columns that `Column::Extra(n)` in `columns` names.
    pub extra: Vec<crate::columns::Extra>,
}
impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            columns: vec![
                Column::ActivityId,
                Column::ActivityName,
                Column::RemainingDuration,
                Column::Start,
                Column::Finish,
                Column::TotalFloat,
            ],
            unit: DurationUnit::Days,
            date_format: "dd-mmm-yy hh:mm".into(),
            group_by_wbs: true,
            rows: None,
            extra: vec![],
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExportSummary {
    pub activities: usize,
    pub bands: usize,
    pub relationships: usize,
    pub assignments: usize,
}

fn err(e: XlsxError) -> String {
    format!("Spreadsheet: {e}")
}

fn excel_date(minutes: i64) -> Result<ExcelDateTime, String> {
    ExcelDateTime::from_serial_datetime(minutes as f64 / DAY as f64).map_err(err)
}

struct Formats {
    header: Format,
    band: Format,
    band_date: Format,
    date: Format,
    number: Format,
    percent: Format,
    /// A whole percentage: `0.##` would show 50 as "50." in Excel.
    percent_whole: Format,
    text: Format,
}
impl Formats {
    fn new(date_format: &str) -> Self {
        let grey = Color::RGB(0xE2E2E2);
        let header = Format::new()
            .set_bold()
            .set_background_color(Color::RGB(0xF0F0F0))
            .set_border_bottom(FormatBorder::Thin)
            .set_text_wrap()
            .set_align(FormatAlign::Top);
        Self {
            header,
            band: Format::new().set_bold().set_background_color(grey),
            band_date: Format::new()
                .set_bold()
                .set_background_color(grey)
                .set_num_format(date_format),
            date: Format::new().set_num_format(date_format),
            number: Format::new().set_num_format("0.0#"),
            percent: Format::new().set_num_format("0.0#"),
            percent_whole: Format::new().set_num_format("0"),
            text: Format::new(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn write_value(
    ws: &mut Worksheet,
    row: u32,
    col: u16,
    v: &Value,
    kind: Kind,
    unit: DurationUnit,
    hours_per_day: f64,
    f: &Formats,
) -> Result<(), String> {
    match v {
        Value::Empty => {}
        Value::Text(s) => {
            ws.write_string_with_format(row, col, s, &f.text)
                .map_err(err)?;
        }
        Value::Date(m) => {
            ws.write_datetime_with_format(row, col, excel_date(*m)?, &f.date)
                .map_err(err)?;
        }
        Value::Duration(m) => {
            ws.write_number_with_format(row, col, unit.from_minutes(*m, hours_per_day), &f.number)
                .map_err(err)?;
        }
        Value::Number(n) => {
            let format = if kind == Kind::Percent {
                if n.fract() == 0.0 {
                    &f.percent_whole
                } else {
                    &f.percent
                }
            } else {
                &f.number
            };
            ws.write_number_with_format(row, col, *n, format)
                .map_err(err)?;
        }
        Value::Bool(b) => {
            ws.write_boolean(row, col, *b).map_err(err)?;
        }
    }
    Ok(())
}

fn col_letter(mut col: u16) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (col % 26) as u8) as char);
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    s
}

/// Order of activity rows with WBS bands: depth-first over the WBS tree.
enum Line {
    Band {
        wbs: i64,
        depth: usize,
        members: Vec<usize>,
    },
    Activity {
        index: usize,
        depth: usize,
    },
}

fn lines(p: &ProjectData, lookup: &Lookup, chosen: &[usize], group: bool) -> Vec<Line> {
    if !group {
        return chosen
            .iter()
            .map(|&index| Line::Activity { index, depth: 0 })
            .collect();
    }
    let wbs_of = |i: usize| {
        p.details
            .get(&p.model.activities[i].task_id)
            .map_or(0, |d| d.wbs)
    };
    let mut by_wbs: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    for &i in chosen {
        by_wbs.entry(wbs_of(i)).or_default().push(i);
    }
    let known: std::collections::HashSet<i64> = p.wbs.iter().map(|w| w.id).collect();
    let mut children: BTreeMap<Option<i64>, Vec<&openxer_xer::Wbs>> = BTreeMap::new();
    for w in &p.wbs {
        let parent = w.parent.filter(|id| known.contains(id));
        children.entry(parent).or_default().push(w);
    }
    for list in children.values_mut() {
        list.sort_by_key(|w| (w.sequence, w.id));
    }
    fn subtree_has(
        id: i64,
        children: &BTreeMap<Option<i64>, Vec<&openxer_xer::Wbs>>,
        by_wbs: &BTreeMap<i64, Vec<usize>>,
    ) -> bool {
        by_wbs.contains_key(&id)
            || children
                .get(&Some(id))
                .is_some_and(|c| c.iter().any(|w| subtree_has(w.id, children, by_wbs)))
    }
    fn members(
        id: i64,
        children: &BTreeMap<Option<i64>, Vec<&openxer_xer::Wbs>>,
        by_wbs: &BTreeMap<i64, Vec<usize>>,
        out: &mut Vec<usize>,
    ) {
        out.extend(by_wbs.get(&id).into_iter().flatten().copied());
        for w in children.get(&Some(id)).into_iter().flatten() {
            members(w.id, children, by_wbs, out);
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn walk(
        parent: Option<i64>,
        depth: usize,
        children: &BTreeMap<Option<i64>, Vec<&openxer_xer::Wbs>>,
        by_wbs: &BTreeMap<i64, Vec<usize>>,
        out: &mut Vec<Line>,
    ) {
        for w in children.get(&parent).into_iter().flatten() {
            if !subtree_has(w.id, children, by_wbs) {
                continue;
            }
            let mut all = vec![];
            members(w.id, children, by_wbs, &mut all);
            out.push(Line::Band {
                wbs: w.id,
                depth,
                members: all,
            });
            for &index in by_wbs.get(&w.id).into_iter().flatten() {
                out.push(Line::Activity {
                    index,
                    depth: depth + 1,
                });
            }
            walk(Some(w.id), depth + 1, children, by_wbs, out);
        }
    }
    let mut out = vec![];
    walk(None, 0, &children, &by_wbs, &mut out);
    let _ = lookup;
    // Activities whose WBS is not in the project keep their place at the end.
    for (wbs, list) in &by_wbs {
        if !known.contains(wbs) {
            out.extend(list.iter().map(|&index| Line::Activity { index, depth: 0 }));
        }
    }
    out
}

/// Build the modern workbook in memory.
pub fn modern_workbook(
    p: &ProjectData,
    options: &ExportOptions,
) -> Result<(Workbook, ExportSummary), String> {
    if options.columns.is_empty() {
        return Err("Choose at least one column to export".into());
    }
    let mut lookup = Lookup::new(p);
    lookup.extras = options.extra.clone();
    let hpd = lookup.hours_per_day;
    let unit = options.unit;
    let f = Formats::new(&options.date_format);
    let mut workbook = Workbook::new();
    let mut summary = ExportSummary::default();
    let all: Vec<usize> = (0..p.model.activities.len()).collect();
    let chosen = options.rows.clone().unwrap_or(all);
    let lines = lines(p, &lookup, &chosen, options.group_by_wbs);
    let columns = &options.columns;
    let ncol = columns.len() as u16;
    let key_col = ncol;
    let crit_col = ncol + 1;
    let tf_col = ncol + 2;
    {
        let ws = workbook.add_worksheet();
        ws.set_name(ACTIVITY_SHEET).map_err(err)?;
        for (c, column) in columns.iter().enumerate() {
            ws.write_string_with_format(0, c as u16, column.header(&lookup, unit), &f.header)
                .map_err(err)?;
            ws.set_column_width(c as u16, column.width(&lookup))
                .map_err(err)?;
        }
        for (c, h) in [
            (key_col, KEY_HEADER),
            (crit_col, CRITICAL_HEADER),
            (tf_col, FLOAT_HEADER),
        ] {
            ws.write_string_with_format(0, c, h, &f.header)
                .map_err(err)?;
            ws.set_column_hidden(c).map_err(err)?;
        }
        ws.set_row_height(0, 30).map_err(err)?;
        ws.group_symbols_above(true);
        let mut row = 1u32;
        for line in &lines {
            match line {
                Line::Band {
                    wbs,
                    depth,
                    members,
                } => {
                    summary.bands += 1;
                    let label = p
                        .wbs
                        .iter()
                        .find(|w| w.id == *wbs)
                        .map(|w| {
                            format!(
                                "{}  {}",
                                lookup.wbs_path.get(wbs).cloned().unwrap_or_default(),
                                w.name
                            )
                        })
                        .unwrap_or_default();
                    for c in 0..ncol {
                        ws.write_blank(row, c, &f.band).map_err(err)?;
                    }
                    ws.write_string_with_format(row, 0, &label, &f.band)
                        .map_err(err)?;
                    // Roll-up dates on the band, like the activity table's group rows.
                    let leveled = p.model.options.do_level;
                    for (c, column) in columns.iter().enumerate() {
                        let pick = match column {
                            Column::Start => members
                                .iter()
                                .filter_map(|&i| displayed_start(&p.model.activities[i], leveled))
                                .min(),
                            Column::Finish => members
                                .iter()
                                .filter_map(|&i| displayed_finish(&p.model.activities[i], leveled))
                                .max(),
                            _ => None,
                        };
                        if let Some(m) = pick {
                            ws.write_datetime_with_format(
                                row,
                                c as u16,
                                excel_date(m + lookup.origin_minutes)?,
                                &f.band_date,
                            )
                            .map_err(err)?;
                        }
                    }
                    ws.write_string(row, key_col, format!("W:{wbs}"))
                        .map_err(err)?;
                    for _ in 0..(*depth).min(7) {
                        ws.group_rows(row, row).map_err(err)?;
                    }
                }
                Line::Activity { index, depth } => {
                    summary.activities += 1;
                    let a = &p.model.activities[*index];
                    for (c, column) in columns.iter().enumerate() {
                        let v = value(p, &lookup, *index, *column);
                        write_value(ws, row, c as u16, &v, column.kind(&lookup), unit, hpd, &f)?;
                    }
                    ws.write_string(row, key_col, format!("A:{}", a.task_id))
                        .map_err(err)?;
                    ws.write_boolean(row, crit_col, is_critical(a, lookup.critical))
                        .map_err(err)?;
                    if let Some(tf) = a.total_float {
                        ws.write_number(row, tf_col, unit.from_minutes(tf, hpd))
                            .map_err(err)?;
                    }
                    for _ in 0..(*depth).min(7) {
                        ws.group_rows(row, row).map_err(err)?;
                    }
                }
            }
            row += 1;
        }
        let last = row.saturating_sub(1).max(1);
        let frozen_cols = columns
            .iter()
            .take_while(|c| matches!(c, Column::ActivityId | Column::ActivityName))
            .count()
            .min(2) as u16;
        ws.set_freeze_panes(1, frozen_cols).map_err(err)?;
        ws.autofilter(0, 0, last, ncol - 1).map_err(err)?;
        if row > 1 {
            let crit = col_letter(crit_col);
            let critical = ConditionalFormatFormula::new()
                .set_rule(format!("=${crit}2=TRUE").as_str())
                .set_format(Format::new().set_font_color(Color::RGB(0xC00000)));
            ws.add_conditional_format(1, 0, last, ncol - 1, &critical)
                .map_err(err)?;
            for (c, column) in columns.iter().enumerate() {
                if matches!(column, Column::TotalFloat | Column::FreeFloat) {
                    let l = col_letter(c as u16);
                    let negative = ConditionalFormatFormula::new()
                        .set_rule(format!("=AND(ISNUMBER({l}2),{l}2<0)").as_str())
                        .set_format(
                            Format::new()
                                .set_background_color(Color::RGB(0xFFC7CE))
                                .set_font_color(Color::RGB(0x9C0006)),
                        );
                    ws.add_conditional_format(1, c as u16, last, c as u16, &negative)
                        .map_err(err)?;
                }
            }
        }
    }
    summary.relationships = relationships_sheet(&mut workbook, p, &lookup, unit, &f)?;
    summary.assignments = assignments_sheet(&mut workbook, p, &lookup, &f)?;
    about_sheet(&mut workbook, p, &lookup, options, &f)?;
    Ok((workbook, summary))
}

fn relationships_sheet(
    workbook: &mut Workbook,
    p: &ProjectData,
    lookup: &Lookup,
    unit: DurationUnit,
    f: &Formats,
) -> Result<usize, String> {
    let ws = workbook.add_worksheet();
    ws.set_name(LINK_SHEET).map_err(err)?;
    let headers = [
        ("Predecessor ID".to_string(), 16.0),
        ("Predecessor Name".into(), 36.0),
        ("Successor ID".into(), 16.0),
        ("Successor Name".into(), 36.0),
        ("Relationship Type".into(), 12.0),
        (format!("Lag ({})", unit.suffix()), 10.0),
        ("Driving".into(), 9.0),
        (DELETE_HEADER.into(), 9.0),
    ];
    for (c, (h, w)) in headers.iter().enumerate() {
        ws.write_string_with_format(0, c as u16, h, &f.header)
            .map_err(err)?;
        ws.set_column_width(c as u16, *w).map_err(err)?;
    }
    let mut links: Vec<_> = p.model.links.iter().collect();
    let code = |i: usize| p.model.activities[i].task_code.as_str();
    links.sort_by(|a, b| (code(a.pred), code(a.succ)).cmp(&(code(b.pred), code(b.succ))));
    for (n, l) in links.iter().enumerate() {
        let row = n as u32 + 1;
        ws.write_string(row, 0, code(l.pred)).map_err(err)?;
        ws.write_string(row, 1, p.name(l.pred)).map_err(err)?;
        ws.write_string(row, 2, code(l.succ)).map_err(err)?;
        ws.write_string(row, 3, p.name(l.succ)).map_err(err)?;
        ws.write_string(row, 4, l.link_type.as_str().trim_start_matches("PR_"))
            .map_err(err)?;
        ws.write_number_with_format(
            row,
            5,
            unit.from_minutes(l.lag, lookup.hours_per_day),
            &f.number,
        )
        .map_err(err)?;
        ws.write_boolean(row, 6, l.driving_path).map_err(err)?;
    }
    ws.set_freeze_panes(1, 0).map_err(err)?;
    ws.autofilter(0, 0, links.len().max(1) as u32, headers.len() as u16 - 1)
        .map_err(err)?;
    Ok(links.len())
}

fn assignments_sheet(
    workbook: &mut Workbook,
    p: &ProjectData,
    lookup: &Lookup,
    f: &Formats,
) -> Result<usize, String> {
    let ws = workbook.add_worksheet();
    ws.set_name(ASSIGNMENT_SHEET).map_err(err)?;
    let headers = [
        ("Activity ID", 16.0),
        ("Activity Name", 36.0),
        ("Resource ID", 16.0),
        ("Resource Name", 28.0),
        ("Budgeted Units (h)", 12.0),
        ("Remaining Units (h)", 12.0),
        ("Actual Units (h)", 12.0),
        ("Start", 15.0),
        ("Finish", 15.0),
    ];
    for (c, (h, w)) in headers.iter().enumerate() {
        ws.write_string_with_format(0, c as u16, *h, &f.header)
            .map_err(err)?;
        ws.set_column_width(c as u16, *w).map_err(err)?;
    }
    let pid = p.model.project.proj_id;
    let by_id: BTreeMap<i64, usize> = p
        .model
        .activities
        .iter()
        .enumerate()
        .map(|(i, a)| (a.task_id, i))
        .collect();
    // Resource ID and name by resource id, built once (the first row of an
    // id wins, as a scan from the top found).
    let mut rsrc: HashMap<&str, (&str, &str)> = HashMap::new();
    if let Some(rt) = p.document.table("RSRC") {
        let (id, short, name) = (
            Col::of(rt, "rsrc_id"),
            Col::of(rt, "rsrc_short_name"),
            Col::of(rt, "rsrc_name"),
        );
        for x in &rt.rows {
            rsrc.entry(id.get(x)).or_insert((short.get(x), name.get(x)));
        }
    }
    let mut row = 1u32;
    if let Some(t) = p.document.table("TASKRSRC") {
        for r in &t.rows {
            if num(t.value(r, "proj_id")) != pid {
                continue;
            }
            let Some(&i) = by_id.get(&num(t.value(r, "task_id"))) else {
                continue;
            };
            let rid = t.value(r, "rsrc_id");
            let (short, name) = rsrc.get(rid).copied().unwrap_or((rid, ""));
            ws.write_string(row, 0, &p.model.activities[i].task_code)
                .map_err(err)?;
            ws.write_string(row, 1, p.name(i)).map_err(err)?;
            ws.write_string(row, 2, short).map_err(err)?;
            ws.write_string(row, 3, name).map_err(err)?;
            for (c, field) in [(4u16, "target_qty"), (5, "remain_qty"), (6, "act_reg_qty")] {
                if let Ok(v) = t.value(r, field).trim().parse::<f64>() {
                    ws.write_number_with_format(row, c, v, &f.number)
                        .map_err(err)?;
                }
            }
            for (c, fields) in [
                (
                    7u16,
                    ["act_start_date", "restart_date", "target_start_date"],
                ),
                (8, ["act_end_date", "reend_date", "target_end_date"]),
            ] {
                if let Some(m) = fields
                    .iter()
                    .find_map(|k| openxer_xer::parse_absolute(t.value(r, k)))
                {
                    ws.write_datetime_with_format(row, c, excel_date(m)?, &f.date)
                        .map_err(err)?;
                }
            }
            row += 1;
        }
    }
    let _ = lookup;
    ws.set_freeze_panes(1, 0).map_err(err)?;
    ws.autofilter(0, 0, row.saturating_sub(1).max(1), headers.len() as u16 - 1)
        .map_err(err)?;
    Ok(row as usize - 1)
}

fn about_sheet(
    workbook: &mut Workbook,
    p: &ProjectData,
    lookup: &Lookup,
    options: &ExportOptions,
    f: &Formats,
) -> Result<(), String> {
    let ws = workbook.add_worksheet();
    ws.set_name(ABOUT_SHEET).map_err(err)?;
    ws.set_column_width(0, 22).map_err(err)?;
    ws.set_column_width(1, 34).map_err(err)?;
    ws.set_column_width(2, 30).map_err(err)?;
    ws.set_column_width(3, 22).map_err(err)?;
    ws.set_column_width(4, 10).map_err(err)?;
    let bold = Format::new().set_bold();
    let pairs: Vec<(&str, Value)> = vec![
        ("Format", Value::Text(FORMAT_MARK.into())),
        ("Format Version", Value::Number(FORMAT_VERSION)),
        ("Project ID", Value::Text(p.model.project.name.clone())),
        ("Project Key", Value::Number(p.model.project.proj_id as f64)),
        (
            "Data Date",
            p.model
                .project
                .data_date
                .map_or(Value::Empty, |d| Value::Date(d + lookup.origin_minutes)),
        ),
        (
            "Exported",
            Value::Date(
                chrono::Local::now()
                    .naive_local()
                    .signed_duration_since(
                        chrono::NaiveDate::from_ymd_opt(1899, 12, 30)
                            .expect("valid")
                            .and_hms_opt(0, 0, 0)
                            .expect("valid"),
                    )
                    .num_minutes(),
            ),
        ),
        ("Duration Unit", Value::Text(options.unit.label().into())),
        ("Hours per Day", Value::Number(lookup.hours_per_day)),
        ("Date Format", Value::Text(options.date_format.clone())),
        ("Source", Value::Text(p.source_name.clone())),
        (
            "How to edit",
            Value::Text(
                "Change names, durations, actual dates, % complete, constraints, calendar, WBS, \
                 activity codes and UDF values, add rows for new activities, then import the file \
                 in OpenXER (File > Import, Spreadsheet). Type D in a Delete column to remove an \
                 activity or relationship. Calculated columns (planned dates, float, status) are \
                 ignored on import. A blank cell changes nothing; type <none> to clear a value. \
                 Every change is previewed before it is applied."
                    .into(),
            ),
        ),
    ];
    for (r, (k, v)) in pairs.iter().enumerate() {
        ws.write_string_with_format(r as u32, 0, *k, &bold)
            .map_err(err)?;
        write_value(
            ws,
            r as u32,
            1,
            v,
            Kind::Number,
            options.unit,
            lookup.hours_per_day,
            f,
        )?;
    }
    let start = pairs.len() as u32 + 1;
    for (c, h) in ["Sheet", "Column", "Header", "Key", "Editable"]
        .iter()
        .enumerate()
    {
        ws.write_string_with_format(start, c as u16, *h, &f.header)
            .map_err(err)?;
    }
    for (n, column) in options.columns.iter().enumerate() {
        let r = start + 1 + n as u32;
        ws.write_string(r, 0, ACTIVITY_SHEET).map_err(err)?;
        ws.write_number(r, 1, (n + 1) as f64).map_err(err)?;
        ws.write_string(r, 2, column.header(lookup, options.unit))
            .map_err(err)?;
        ws.write_string(r, 3, column.key()).map_err(err)?;
        ws.write_string(r, 4, if column.editable() { "Yes" } else { "No" })
            .map_err(err)?;
    }
    Ok(())
}

/// Write the workbook beside `path` and rename it over the file there, so a
/// failure never leaves a cut-short workbook in place of the previous one.
fn save(workbook: &mut Workbook, path: &Path) -> Result<(), String> {
    let bytes = workbook.save_to_buffer().map_err(err)?;
    openxer_xer::write_file_atomic(path, &bytes).map_err(|e| format!("Spreadsheet: {e}"))
}

pub fn write_modern(
    p: &ProjectData,
    options: &ExportOptions,
    path: &Path,
) -> Result<ExportSummary, String> {
    let (mut workbook, summary) = modern_workbook(p, options)?;
    save(&mut workbook, path)?;
    Ok(summary)
}

/// Column set of the tables-layout TASK sheet.
pub fn tables_columns(lookup: &Lookup) -> Vec<Column> {
    let mut columns = vec![
        Column::ActivityId,
        Column::ActivityStatus,
        Column::WbsCode,
        Column::ActivityName,
        Column::Calendar,
        Column::OriginalDuration,
        Column::RemainingDuration,
        Column::PercentComplete,
        Column::Start,
        Column::Finish,
        Column::ActualStart,
        Column::ActualFinish,
        Column::PrimaryConstraint,
        Column::PrimaryConstraintDate,
        Column::SecondaryConstraint,
        Column::SecondaryConstraintDate,
        Column::TotalFloat,
    ];
    columns.extend(
        lookup
            .catalog()
            .into_iter()
            .filter(|c| matches!(c, Column::ActivityCode(_) | Column::Udf(_))),
    );
    columns
}

fn text_date(minutes: i64) -> String {
    openxer_xer::day_label(0, minutes, TABLES_TEXT_DATE)
}

/// Tables-layout workbook: XER field names in row 1, titles in row 2, data from row 3.
pub fn tables_workbook(
    p: &ProjectData,
    unit: DurationUnit,
) -> Result<(Workbook, ExportSummary), String> {
    let lookup = Lookup::new(p);
    let hpd = lookup.hours_per_day;
    let mut workbook = Workbook::new();
    let mut summary = ExportSummary::default();
    let title_format = Format::new().set_bold();
    let columns = tables_columns(&lookup);
    {
        let ws = workbook.add_worksheet();
        ws.set_name("TASK").map_err(err)?;
        for (c, column) in columns.iter().enumerate() {
            let title = column.title(&lookup);
            let title = if column.editable() || *column == Column::ActivityStatus {
                title
            } else {
                format!("(*){title}")
            };
            ws.write_string(0, c as u16, column.field()).map_err(err)?;
            ws.write_string_with_format(1, c as u16, title, &title_format)
                .map_err(err)?;
            ws.set_column_width(c as u16, column.width(&lookup))
                .map_err(err)?;
        }
        let delete_col = columns.len() as u16;
        ws.write_string(0, delete_col, "delete_record_flag")
            .map_err(err)?;
        ws.write_string_with_format(1, delete_col, "Delete This Row", &title_format)
            .map_err(err)?;
        let mut order: Vec<usize> = (0..p.model.activities.len()).collect();
        order.sort_by(|&a, &b| {
            p.model.activities[a]
                .task_code
                .cmp(&p.model.activities[b].task_code)
        });
        for (n, &i) in order.iter().enumerate() {
            let row = n as u32 + 2;
            for (c, column) in columns.iter().enumerate() {
                let c = c as u16;
                match value(p, &lookup, i, *column) {
                    Value::Empty => {}
                    Value::Text(s) => {
                        ws.write_string(row, c, s).map_err(err)?;
                    }
                    Value::Date(m) => {
                        ws.write_string(row, c, text_date(m)).map_err(err)?;
                    }
                    Value::Duration(m) => {
                        ws.write_number(row, c, round6(unit.from_minutes(m, hpd)))
                            .map_err(err)?;
                    }
                    Value::Number(v) => {
                        ws.write_number(row, c, v).map_err(err)?;
                    }
                    Value::Bool(b) => {
                        ws.write_string(row, c, if b { "Yes" } else { "No" })
                            .map_err(err)?;
                    }
                }
            }
            summary.activities += 1;
        }
        ws.set_freeze_panes(2, 1).map_err(err)?;
    }
    {
        let ws = workbook.add_worksheet();
        ws.set_name("TASKPRED").map_err(err)?;
        let cols = [
            ("pred_task_id", "Predecessor"),
            ("task_id", "Successor"),
            ("pred_type", "Relationship Type"),
            ("lag_hr_cnt", "Lag"),
            ("delete_record_flag", "Delete This Row"),
        ];
        for (c, (field, title)) in cols.iter().enumerate() {
            ws.write_string(0, c as u16, *field).map_err(err)?;
            ws.write_string_with_format(1, c as u16, *title, &title_format)
                .map_err(err)?;
            ws.set_column_width(c as u16, 16).map_err(err)?;
        }
        for (n, l) in p.model.links.iter().enumerate() {
            let row = n as u32 + 2;
            ws.write_string(row, 0, &p.model.activities[l.pred].task_code)
                .map_err(err)?;
            ws.write_string(row, 1, &p.model.activities[l.succ].task_code)
                .map_err(err)?;
            ws.write_string(row, 2, l.link_type.as_str().trim_start_matches("PR_"))
                .map_err(err)?;
            ws.write_number(row, 3, round6(unit.from_minutes(l.lag, hpd)))
                .map_err(err)?;
            summary.relationships += 1;
        }
        ws.set_freeze_panes(2, 0).map_err(err)?;
    }
    {
        let ws = workbook.add_worksheet();
        ws.set_name("TASKRSRC").map_err(err)?;
        let cols = [
            ("task_id", "Activity ID"),
            ("rsrc_id", "Resource ID"),
            ("target_qty", "Budgeted Units"),
            ("remain_qty", "Remaining Units"),
            ("delete_record_flag", "Delete This Row"),
        ];
        for (c, (field, title)) in cols.iter().enumerate() {
            ws.write_string(0, c as u16, *field).map_err(err)?;
            ws.write_string_with_format(1, c as u16, *title, &title_format)
                .map_err(err)?;
            ws.set_column_width(c as u16, 16).map_err(err)?;
        }
        let pid = p.model.project.proj_id;
        let codes: BTreeMap<i64, &str> = p
            .model
            .activities
            .iter()
            .map(|a| (a.task_id, a.task_code.as_str()))
            .collect();
        let rsrc = p.document.table("RSRC");
        let mut row = 2u32;
        if let Some(t) = p.document.table("TASKRSRC") {
            for r in &t.rows {
                if num(t.value(r, "proj_id")) != pid {
                    continue;
                }
                let Some(code) = codes.get(&num(t.value(r, "task_id"))) else {
                    continue;
                };
                let rid = t.value(r, "rsrc_id");
                let short = rsrc
                    .and_then(|rt| {
                        rt.rows
                            .iter()
                            .find(|x| rt.value(x, "rsrc_id") == rid)
                            .map(|x| rt.value(x, "rsrc_short_name"))
                    })
                    .unwrap_or(rid);
                ws.write_string(row, 0, *code).map_err(err)?;
                ws.write_string(row, 1, short).map_err(err)?;
                for (c, field) in [(2u16, "target_qty"), (3, "remain_qty")] {
                    if let Ok(v) = t.value(r, field).trim().parse::<f64>() {
                        ws.write_number(row, c, v).map_err(err)?;
                    }
                }
                row += 1;
                summary.assignments += 1;
            }
        }
    }
    {
        let ws = workbook.add_worksheet();
        ws.set_name("USERDATA").map_err(err)?;
        ws.write_string(0, 0, "user_data").map_err(err)?;
        ws.write_string(1, 0, "UserSettings Do Not Edit")
            .map_err(err)?;
        let qty = match unit {
            DurationUnit::Days => "QT_Day",
            DurationUnit::Hours => "QT_Hour",
        };
        ws.write_string(
            2,
            0,
            format!(
                "DurationQtyType={qty}\nShowAsPercentage=0\nSmallScaleQtyType=QT_Hour\nDateFormat=dd-mmm-yy hh:mm\nHoursPerDay={hpd}\nExportedBy=OpenXER"
            ),
        )
        .map_err(err)?;
    }
    Ok((workbook, summary))
}

/// Six decimals: enough that the import, which rounds to whole minutes,
/// reads back the exact duration on any calendar (two decimals of a day
/// lose up to 2.4 minutes per hour of day length), without float noise.
fn round6(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

pub fn write_tables(
    p: &ProjectData,
    unit: DurationUnit,
    path: &Path,
) -> Result<ExportSummary, String> {
    let (mut workbook, summary) = tables_workbook(p, unit)?;
    save(&mut workbook, path)?;
    Ok(summary)
}
