//! The S-curve and earned hours workbooks, written from the figures of
//! `openxer_xer::scurve` (the app, the command line and MCP all write them
//! here).
//!
//! S-curve workbook:
//! - **S-Curve**: one row per period (start, end) with the period's
//!   budgeted, actual and remaining values (numbers), their running sums
//!   and percents of the budget (formulas, with their values stored), the
//!   earned figure at the data date, and a native chart: period budgeted
//!   and actual as columns, cumulative budgeted, actual and actual plus
//!   remaining (and the late dates curve) as lines on the secondary axis,
//!   earned as one marker.
//! - **By <breakdown> - Budgeted / Actual / Remaining**: one row per
//!   resource or WBS element, one column per period, a total row and a
//!   total column (formulas). Left out for the total only.
//! - **Info**: what the curve is and where its numbers come from.
//!
//! Earned hours workbook: **Earned to date** (one row per resource, WBS
//! element or activity), **By activity** (one row per labor assignment, by
//! Activity ID then resource) and **Info**.
use openxer_engine::calendar::DAY;
use openxer_xer::scurve::{Breakdown, Curve, EarnedHours, Group, Measure, Period};
use openxer_xer::{date_string, Document};
use rust_xlsxwriter::{
    utility::{row_col_to_cell, row_col_to_cell_absolute},
    Chart, ChartFormat, ChartLegendPosition, ChartMarker, ChartMarkerType, ChartType, Color,
    Format, FormatBorder, Formula, Workbook, Worksheet, XlsxError,
};

/// Sheet names the readers and tests look for.
pub const CURVE_SHEET: &str = "S-Curve";
pub const INFO_SHEET: &str = "Info";
pub const EARNED_SHEET: &str = "Earned to date";
pub const DETAIL_SHEET: &str = "By activity";

/// How dates and costs are shown.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// Excel number format of dates.
    pub date_format: String,
    /// Excel number format of costs; `None` for plain numbers with two
    /// decimals.
    pub currency_format: Option<String>,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            date_format: "dd-mmm-yy".into(),
            currency_format: None,
        }
    }
}

/// An Excel cost format from the base currency of `doc` (its CURRTYPE row
/// with the lowest id): the symbol before the number and its decimals.
pub fn currency_format(doc: &Document) -> Option<String> {
    let t = doc.table("CURRTYPE")?;
    let r = t.rows.iter().min_by_key(|r| {
        t.value(r, "curr_id")
            .trim()
            .parse::<i64>()
            .unwrap_or(i64::MAX)
    })?;
    let symbol = t.value(r, "curr_symbol").trim();
    let decimals = t
        .value(r, "decimal_digit_cnt")
        .trim()
        .parse::<usize>()
        .unwrap_or(2)
        .min(6);
    Some(excel_currency(symbol, decimals))
}

/// `"$"#,##0.00` for symbol `$` and two decimals.
pub fn excel_currency(symbol: &str, decimals: usize) -> String {
    let number = if decimals == 0 {
        "#,##0".to_string()
    } else {
        format!("#,##0.{}", "0".repeat(decimals))
    };
    if symbol.is_empty() {
        number
    } else {
        format!("\"{}\"{number}", symbol.replace('"', ""))
    }
}

/// When the workbook was made, local time.
fn generated() -> (String, String) {
    (
        "Generated".into(),
        chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
    )
}

fn err(e: XlsxError) -> String {
    format!("The workbook could not be written: {e}")
}

/// Days since 1899-12-30 of model minute `m`.
fn serial(origin: i64, m: i64) -> f64 {
    origin as f64 + m as f64 / DAY as f64
}

fn cell(row: u32, col: u16) -> String {
    row_col_to_cell(row, col)
}

struct Formats {
    title: Format,
    sub: Format,
    header: Format,
    date: Format,
    date_header: Format,
    value: Format,
    value_bold: Format,
    percent: Format,
    percent_bold: Format,
    bold: Format,
    text: Format,
}

fn formats(style: &Style, measure: Measure) -> Formats {
    let number = match measure {
        Measure::Cost => style
            .currency_format
            .clone()
            .unwrap_or_else(|| "#,##0.00".into()),
        _ => "#,##0.0".into(),
    };
    let header = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0xF0F0F0))
        .set_border_bottom(FormatBorder::Thin)
        .set_text_wrap();
    Formats {
        title: Format::new().set_bold().set_font_size(14),
        sub: Format::new().set_font_color(Color::RGB(0x404040)),
        date: Format::new().set_num_format(&style.date_format),
        date_header: header.clone().set_num_format(&style.date_format),
        header,
        value: Format::new().set_num_format(&number),
        value_bold: Format::new().set_bold().set_num_format(&number),
        percent: Format::new().set_num_format("0.0%"),
        percent_bold: Format::new().set_bold().set_num_format("0.0%"),
        bold: Format::new().set_bold(),
        text: Format::new(),
    }
}

/// A formula with its value stored, so readers that do not calculate see
/// the number too.
fn formula(text: String, value: f64) -> Formula {
    Formula::new(text).set_result(format!("{value}"))
}

fn titles(ws: &mut Worksheet, f: &Formats, lines: &[String]) -> Result<(), String> {
    for (i, line) in lines.iter().enumerate() {
        ws.write_string_with_format(i as u32, 0, line, if i == 0 { &f.title } else { &f.sub })
            .map_err(err)?;
    }
    Ok(())
}

fn info_sheet(
    book: &mut Workbook,
    rows: &[(String, String)],
    warnings: &[String],
) -> Result<(), String> {
    let ws = book.add_worksheet();
    ws.set_name(INFO_SHEET).map_err(err)?;
    let bold = Format::new().set_bold();
    let header = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0xF0F0F0))
        .set_border_bottom(FormatBorder::Thin);
    ws.write_string_with_format(0, 0, "Item", &header)
        .map_err(err)?;
    ws.write_string_with_format(0, 1, "Value", &header)
        .map_err(err)?;
    let mut row = 1;
    for (k, v) in rows {
        ws.write_string_with_format(row, 0, k, &bold).map_err(err)?;
        ws.write_string(row, 1, v).map_err(err)?;
        row += 1;
    }
    if !warnings.is_empty() {
        row += 1;
        ws.write_string_with_format(row, 0, "Warnings", &bold)
            .map_err(err)?;
        for w in warnings {
            ws.write_string(row, 1, w).map_err(err)?;
            row += 1;
        }
    }
    ws.set_column_width(0, 24).map_err(err)?;
    ws.set_column_width(1, 110).map_err(err)?;
    ws.set_freeze_panes(1, 0).map_err(err)?;
    Ok(())
}

fn period_label(c: &Curve) -> String {
    match c.options.period {
        Period::Week if c.options.week_starts_sunday => "Week (from Sunday)".into(),
        Period::Week => "Week (from Monday)".into(),
        p => p.label().into(),
    }
}

fn breakdown_word(b: Breakdown) -> &'static str {
    match b {
        Breakdown::Total => "Total",
        Breakdown::Resource => "Resource",
        Breakdown::Wbs1 => "WBS level 1",
        Breakdown::Wbs2 => "WBS level 2",
    }
}

/// The Info rows of an S-curve: the caller adds what only it knows (when
/// it was made, a filter).
pub fn curve_info(c: &Curve) -> Vec<(String, String)> {
    let mut rows = vec![
        ("Project".to_string(), c.projects.join(", ")),
        (
            "Data date".into(),
            c.data_date
                .map_or("(none)".into(), |d| date_string(c.origin, Some(d))),
        ),
        ("Measure".into(), c.options.measure.label().into()),
        ("Found".into(), c.detection.clone()),
        ("Period".into(), period_label(c)),
        ("Break down by".into(), c.options.breakdown.label().into()),
        ("Budgeted from".into(), c.budget_source.clone()),
        (
            "Actual and remaining".into(),
            "Actual to the data date; remaining from it on the early dates (leveled dates when the project levels).".into(),
        ),
        (
            "Earned to data date".into(),
            c.earned.map_or("(no data date)".into(), |e| format!("{e:.2}")),
        ),
    ];
    if c.options.late {
        rows.push((
            "Late dates".into(),
            "Actual plus remaining spread over the late dates.".into(),
        ));
    }
    let rule = match c.options.measure {
        Measure::Labor => "Labor assignments' budgeted, actual and remaining units, spread over their own dates on their calendars as the usage views spread them (resource curves included).",
        Measure::Cost => "Every assignment's costs plus expenses; an expense spreads evenly over its activity's dates.",
        Measure::Duration => "Each activity weighs its planned (original) duration in working days on its own calendar, spread evenly over its planned working days; actual is that weight times the activity % complete over the actual dates, remaining the rest. Milestones weigh nothing.",
    };
    rows.push(("Rule".into(), rule.into()));
    rows
}

/// The S-curve workbook.
pub fn curve_bytes(
    c: &Curve,
    extra: &[(String, String)],
    style: &Style,
) -> Result<Vec<u8>, String> {
    let mut book = Workbook::new();
    let f = formats(style, c.options.measure);
    let n = c.periods.len() as u32;
    let unit = c.options.measure.unit();
    let late = c.options.late;
    let ws = book.add_worksheet();
    ws.set_name(CURVE_SHEET).map_err(err)?;
    titles(
        ws,
        &f,
        &[
            format!("S-Curve: {}", c.projects.join(", ")),
            format!(
                "{} per {}; budgeted from {}",
                c.options.measure.label(),
                period_label(c).to_lowercase(),
                c.budget_source
            ),
            c.data_date.map_or("No data date".into(), |d| {
                format!("Data date {}", date_string(c.origin, Some(d)))
            }),
        ],
    )?;
    let head = 4u32;
    let first = head + 1;
    let last = head + n;
    let mut header: Vec<String> = vec![
        "Period Start".into(),
        "Period End".into(),
        format!("Budgeted {unit}"),
        format!("Actual {unit}"),
        format!("Remaining {unit}"),
        "Cum Budgeted".into(),
        "Cum Actual".into(),
        "Cum Remaining".into(),
        "Cum Actual + Remaining".into(),
        "Cum Budgeted % of Budget".into(),
        "Cum Actual % of Budget".into(),
        "Cum Actual + Remaining % of Budget".into(),
        "Earned (cumulative)".into(),
    ];
    if late {
        header.push(format!("Late Dates {unit}"));
        header.push("Cum Late Dates".into());
    }
    for (k, h) in header.iter().enumerate() {
        ws.write_string_with_format(head, k as u16, h, &f.header)
            .map_err(err)?;
    }
    ws.set_row_height(head, 45).map_err(err)?;
    let budget_total: f64 = c.total.budget.iter().sum();
    let total_ref = format!(
        "SUM({}:{})",
        row_col_to_cell_absolute(first, 2),
        row_col_to_cell_absolute(last.max(first), 2)
    );
    let mut running = [0.; 4];
    for (k, &(s, e)) in c.periods.iter().enumerate() {
        let row = first + k as u32;
        let t = &c.total;
        ws.write_number_with_format(row, 0, serial(c.origin, s), &f.date)
            .map_err(err)?;
        ws.write_number_with_format(row, 1, serial(c.origin, e - DAY), &f.date)
            .map_err(err)?;
        for (col, v) in [(2, t.budget[k]), (3, t.actual[k]), (4, t.remaining[k])] {
            ws.write_number_with_format(row, col, v, &f.value)
                .map_err(err)?;
        }
        running[0] += t.budget[k];
        running[1] += t.actual[k];
        running[2] += t.remaining[k];
        // A running sum: $C$6:C9.
        let run = |col: u16| {
            format!(
                "SUM({}:{})",
                row_col_to_cell_absolute(first, col),
                cell(row, col)
            )
        };
        for (col, src, v) in [
            (5u16, 2u16, running[0]),
            (6, 3, running[1]),
            (7, 4, running[2]),
        ] {
            ws.write_formula_with_format(row, col, formula(format!("={}", run(src)), v), &f.value)
                .map_err(err)?;
        }
        ws.write_formula_with_format(
            row,
            8,
            formula(
                format!("={}+{}", cell(row, 6), cell(row, 7)),
                running[1] + running[2],
            ),
            &f.value,
        )
        .map_err(err)?;
        let share = |v: f64| {
            if budget_total.abs() > 1e-12 {
                v / budget_total
            } else {
                0.
            }
        };
        for (col, of, v) in [
            (9u16, 5u16, running[0]),
            (10, 6, running[1]),
            (11, 8, running[1] + running[2]),
        ] {
            ws.write_formula_with_format(
                row,
                col,
                formula(
                    format!("=IF({total_ref}=0,0,{}/{total_ref})", cell(row, of)),
                    share(v),
                ),
                &f.percent,
            )
            .map_err(err)?;
        }
        if c.earned_period == Some(k) {
            if let Some(earned) = c.earned {
                ws.write_number_with_format(row, 12, earned, &f.value)
                    .map_err(err)?;
            }
        }
        if late {
            ws.write_number_with_format(row, 13, t.late[k], &f.value)
                .map_err(err)?;
            running[3] += t.late[k];
            ws.write_formula_with_format(
                row,
                14,
                formula(format!("={}", run(13)), running[3]),
                &f.value,
            )
            .map_err(err)?;
        }
    }
    // The total row.
    let total_row = last + 1;
    ws.write_string_with_format(total_row, 0, "Total", &f.bold)
        .map_err(err)?;
    let sums = [
        (2u16, c.total.budget.iter().sum::<f64>()),
        (3, c.total.actual.iter().sum()),
        (4, c.total.remaining.iter().sum()),
    ];
    let mut sums = sums.to_vec();
    if late {
        sums.push((13, c.total.late.iter().sum()));
    }
    for (col, v) in sums {
        ws.write_formula_with_format(
            total_row,
            col,
            formula(
                format!("=SUM({}:{})", cell(first, col), cell(last.max(first), col)),
                v,
            ),
            &f.value_bold,
        )
        .map_err(err)?;
    }
    ws.set_column_width(0, 12).map_err(err)?;
    ws.set_column_width(1, 12).map_err(err)?;
    for col in 2..header.len() as u16 {
        ws.set_column_width(col, 14).map_err(err)?;
    }
    ws.set_freeze_panes(first, 2).map_err(err)?;
    if n > 0 {
        let range = |col: u16| (CURVE_SHEET, first, col, last, col);
        let mut chart = Chart::new(ChartType::Column);
        for (col, name) in [(2u16, "Budgeted"), (3, "Actual")] {
            chart
                .add_series()
                .set_categories(range(1))
                .set_values(range(col))
                .set_name(&format!("{name} per period"));
        }
        let mut lines = Chart::new(ChartType::Line);
        let mut series: Vec<(u16, &str)> = vec![
            (5, "Cum Budgeted"),
            (6, "Cum Actual"),
            (8, "Cum Actual + Remaining"),
        ];
        if late {
            series.push((14, "Cum Late Dates"));
        }
        for (col, name) in series {
            lines
                .add_series()
                .set_categories(range(1))
                .set_values(range(col))
                .set_name(name)
                .set_secondary_axis(true);
        }
        if c.earned.is_some() && c.earned_period.is_some() {
            lines
                .add_series()
                .set_categories(range(1))
                .set_values(range(12))
                .set_name("Earned (cumulative)")
                .set_secondary_axis(true)
                .set_format(ChartFormat::new().set_no_line())
                .set_marker(
                    ChartMarker::new()
                        .set_type(ChartMarkerType::Diamond)
                        .set_size(9),
                );
        }
        chart.combine(&lines);
        chart
            .title()
            .set_name(&format!("S-Curve ({})", c.options.measure.label()));
        chart.x_axis().set_name("Period end");
        chart.x_axis().set_num_format(&style.date_format);
        chart.y_axis().set_name(&format!("{unit} per period"));
        chart
            .y2_axis()
            .set_name(&format!("Cumulative {}", unit.to_lowercase()));
        chart.legend().set_position(ChartLegendPosition::Bottom);
        chart.set_width(960).set_height(480);
        ws.insert_chart_with_offset(head, header.len() as u16 + 1, &chart, 10, 0)
            .map_err(err)?;
    }
    // The breakdown matrices.
    if c.options.breakdown != Breakdown::Total {
        let word = breakdown_word(c.options.breakdown);
        let key_title = match c.options.breakdown {
            Breakdown::Resource => "Resource ID",
            _ => "WBS",
        };
        let name_title = match c.options.breakdown {
            Breakdown::Resource => "Resource Name",
            _ => "WBS Name",
        };
        for (which, pick) in [("Budgeted", 0usize), ("Actual", 1), ("Remaining", 2)] {
            let ws = book.add_worksheet();
            ws.set_name(format!("By {word} - {which}")).map_err(err)?;
            titles(
                ws,
                &f,
                &[
                    format!(
                        "{which} {} by {} and period",
                        c.options.measure.label(),
                        word.to_lowercase()
                    ),
                    c.projects.join(", "),
                ],
            )?;
            let head = 3u32;
            ws.write_string_with_format(head, 0, key_title, &f.header)
                .map_err(err)?;
            ws.write_string_with_format(head, 1, name_title, &f.header)
                .map_err(err)?;
            for (k, &(s, _)) in c.periods.iter().enumerate() {
                ws.write_number_with_format(
                    head,
                    2 + k as u16,
                    serial(c.origin, s),
                    &f.date_header,
                )
                .map_err(err)?;
            }
            let total_col = 2 + n as u16;
            ws.write_string_with_format(head, total_col, "Total", &f.header)
                .map_err(err)?;
            let first = head + 1;
            let lines_n = c.lines.len() as u32;
            for (i, line) in c.lines.iter().enumerate() {
                let row = first + i as u32;
                let values = match pick {
                    0 => &line.series.budget,
                    1 => &line.series.actual,
                    _ => &line.series.remaining,
                };
                ws.write_string_with_format(row, 0, &line.key, &f.text)
                    .map_err(err)?;
                ws.write_string_with_format(row, 1, &line.name, &f.text)
                    .map_err(err)?;
                for (k, v) in values.iter().enumerate() {
                    ws.write_number_with_format(row, 2 + k as u16, *v, &f.value)
                        .map_err(err)?;
                }
                let sum: f64 = values.iter().sum();
                ws.write_formula_with_format(
                    row,
                    total_col,
                    formula(
                        format!("=SUM({}:{})", cell(row, 2), cell(row, total_col.max(3) - 1)),
                        sum,
                    ),
                    &f.value_bold,
                )
                .map_err(err)?;
            }
            let total_row = first + lines_n;
            ws.write_string_with_format(total_row, 0, "Total", &f.bold)
                .map_err(err)?;
            let column = |k: usize| -> f64 {
                c.lines
                    .iter()
                    .map(|l| match pick {
                        0 => l.series.budget[k],
                        1 => l.series.actual[k],
                        _ => l.series.remaining[k],
                    })
                    .sum()
            };
            let mut grand = 0.;
            for k in 0..n as usize {
                let v = column(k);
                grand += v;
                let col = 2 + k as u16;
                ws.write_formula_with_format(
                    total_row,
                    col,
                    formula(
                        format!(
                            "=SUM({}:{})",
                            cell(first, col),
                            cell(total_row.max(first + 1) - 1, col)
                        ),
                        v,
                    ),
                    &f.value_bold,
                )
                .map_err(err)?;
            }
            ws.write_formula_with_format(
                total_row,
                total_col,
                formula(
                    format!(
                        "=SUM({}:{})",
                        cell(first, total_col),
                        cell(total_row.max(first + 1) - 1, total_col)
                    ),
                    grand,
                ),
                &f.value_bold,
            )
            .map_err(err)?;
            ws.set_column_width(0, 18).map_err(err)?;
            ws.set_column_width(1, 30).map_err(err)?;
            for k in 0..=n as u16 {
                ws.set_column_width(2 + k, 11).map_err(err)?;
            }
            ws.set_freeze_panes(first, 2).map_err(err)?;
        }
    }
    let mut info = curve_info(c);
    info.extend(extra.iter().cloned());
    info.push(generated());
    info_sheet(&mut book, &info, &c.warnings)?;
    book.save_to_buffer().map_err(err)
}

/// The Info rows of an earned hours export.
pub fn earned_info(e: &EarnedHours) -> Vec<(String, String)> {
    vec![
        ("Project".to_string(), e.project.clone()),
        (
            "Data date".into(),
            e.data_date
                .map_or("(none)".into(), |d| date_string(e.origin, Some(d))),
        ),
        ("Measure".into(), "Labor units (hours), cumulative to the data date".into()),
        ("Budgeted from".into(), e.budget_source.clone()),
        ("Percent complete".into(), e.percent_rule.clone()),
        (
            "Earned".into(),
            "Budgeted units times the activity's performance % complete, as the earned value columns compute it.".into(),
        ),
        ("Rows".into(), format!("One per {}", e.group.label().to_lowercase())),
        (
            "Comparing updates".into(),
            "Rows are sorted by their ID and the columns keep their order, so the period's earned hours are this file's Earned Units minus the last update's.".into(),
        ),
    ]
}

/// The earned hours workbook.
pub fn earned_bytes(
    e: &EarnedHours,
    extra: &[(String, String)],
    style: &Style,
) -> Result<Vec<u8>, String> {
    let mut book = Workbook::new();
    let f = formats(style, Measure::Labor);
    let ws = book.add_worksheet();
    ws.set_name(EARNED_SHEET).map_err(err)?;
    let subtitle = e.data_date.map_or("No data date".into(), |d| {
        format!(
            "Cumulative to the data date {}",
            date_string(e.origin, Some(d))
        )
    });
    titles(ws, &f, &[format!("Earned hours: {}", e.project), subtitle])?;
    let (key_title, name_title) = match e.group {
        Group::Resource => ("Resource ID", "Resource Name"),
        Group::Wbs => ("WBS", "WBS Name"),
        Group::Activity => ("Activity ID", "Activity Name"),
    };
    let header = [
        key_title,
        name_title,
        "Budgeted Units",
        "Earned Units",
        "Actual Units",
        "Remaining Units",
        "At Completion Units",
        "% Earned",
    ];
    let head = 3u32;
    for (k, h) in header.iter().enumerate() {
        ws.write_string_with_format(head, k as u16, *h, &f.header)
            .map_err(err)?;
    }
    let first = head + 1;
    let share = |a: f64, b: f64| if b.abs() > 1e-12 { a / b } else { 0. };
    for (i, r) in e.rows.iter().enumerate() {
        let row = first + i as u32;
        ws.write_string(row, 0, &r.key).map_err(err)?;
        ws.write_string(row, 1, &r.name).map_err(err)?;
        for (col, v) in [
            (2u16, r.budgeted),
            (3, r.earned),
            (4, r.actual),
            (5, r.remaining),
        ] {
            ws.write_number_with_format(row, col, v, &f.value)
                .map_err(err)?;
        }
        ws.write_formula_with_format(
            row,
            6,
            formula(
                format!("={}+{}", cell(row, 4), cell(row, 5)),
                r.actual + r.remaining,
            ),
            &f.value,
        )
        .map_err(err)?;
        ws.write_formula_with_format(
            row,
            7,
            formula(
                format!("=IF({b}=0,0,{}/{b})", cell(row, 3), b = cell(row, 2)),
                share(r.earned, r.budgeted),
            ),
            &f.percent,
        )
        .map_err(err)?;
    }
    let total_row = first + e.rows.len() as u32;
    ws.write_string_with_format(total_row, 0, "Total", &f.bold)
        .map_err(err)?;
    let sum =
        |pick: fn(&openxer_xer::scurve::EarnedRow) -> f64| e.rows.iter().map(pick).sum::<f64>();
    let totals = [
        sum(|r| r.budgeted),
        sum(|r| r.earned),
        sum(|r| r.actual),
        sum(|r| r.remaining),
    ];
    let bottom = total_row.max(first + 1) - 1;
    for (k, v) in totals.iter().enumerate() {
        let col = 2 + k as u16;
        ws.write_formula_with_format(
            total_row,
            col,
            formula(
                format!("=SUM({}:{})", cell(first, col), cell(bottom, col)),
                *v,
            ),
            &f.value_bold,
        )
        .map_err(err)?;
    }
    ws.write_formula_with_format(
        total_row,
        6,
        formula(
            format!("={}+{}", cell(total_row, 4), cell(total_row, 5)),
            totals[2] + totals[3],
        ),
        &f.value_bold,
    )
    .map_err(err)?;
    ws.write_formula_with_format(
        total_row,
        7,
        formula(
            format!(
                "=IF({b}=0,0,{}/{b})",
                cell(total_row, 3),
                b = cell(total_row, 2)
            ),
            share(totals[1], totals[0]),
        ),
        &f.percent_bold,
    )
    .map_err(err)?;
    for (col, w) in [
        (0u16, 18.),
        (1, 34.),
        (2, 14.),
        (3, 14.),
        (4, 14.),
        (5, 14.),
        (6, 16.),
        (7, 10.),
    ] {
        ws.set_column_width(col, w).map_err(err)?;
    }
    ws.set_freeze_panes(first, 2).map_err(err)?;

    let ws = book.add_worksheet();
    ws.set_name(DETAIL_SHEET).map_err(err)?;
    titles(
        ws,
        &f,
        &[
            format!("Earned hours by activity: {}", e.project),
            "One row per labor assignment, by Activity ID then Resource ID".into(),
        ],
    )?;
    let header = [
        "Activity ID",
        "Activity Name",
        "WBS",
        "Resource ID",
        "Resource Name",
        "Budgeted Units",
        "% Complete Used",
        "Earned Units",
        "Actual Units",
        "Remaining Units",
    ];
    for (k, h) in header.iter().enumerate() {
        ws.write_string_with_format(head, k as u16, *h, &f.header)
            .map_err(err)?;
    }
    for (i, d) in e.details.iter().enumerate() {
        let row = first + i as u32;
        for (col, t) in [
            (0u16, &d.activity_id),
            (1, &d.activity_name),
            (2, &d.wbs),
            (3, &d.resource_id),
            (4, &d.resource_name),
        ] {
            ws.write_string(row, col, t).map_err(err)?;
        }
        ws.write_number_with_format(row, 5, d.budgeted, &f.value)
            .map_err(err)?;
        ws.write_number_with_format(row, 6, d.percent / 100., &f.percent)
            .map_err(err)?;
        ws.write_formula_with_format(
            row,
            7,
            formula(format!("={}*{}", cell(row, 5), cell(row, 6)), d.earned),
            &f.value,
        )
        .map_err(err)?;
        ws.write_number_with_format(row, 8, d.actual, &f.value)
            .map_err(err)?;
        ws.write_number_with_format(row, 9, d.remaining, &f.value)
            .map_err(err)?;
    }
    for (col, w) in [
        (0u16, 16.),
        (1, 36.),
        (2, 16.),
        (3, 14.),
        (4, 24.),
        (5, 14.),
        (6, 12.),
        (7, 14.),
        (8, 14.),
        (9, 14.),
    ] {
        ws.set_column_width(col, w).map_err(err)?;
    }
    ws.set_freeze_panes(first, 2).map_err(err)?;
    let mut info = earned_info(e);
    info.extend(extra.iter().cloned());
    info.push(generated());
    info_sheet(&mut book, &info, &e.warnings)?;
    book.save_to_buffer().map_err(err)
}
