//! S-Curve and Earned Hours on screen: the figures of `openxer_xer::scurve`
//! as a report document the report tab shows, prints and exports. The
//! figures are computed once and feed both the pages and the workbook, so
//! what the tab shows is what Export > Excel writes.
//!
//! The S-curve's chart is drawn by the usage profiles' histogram painter
//! (`panes::histogram`) and the timescale bands under it by theirs, on the
//! print canvas, and placed on the first page above the tables.
use super::doc::{Cell, Column, Report, Row, Section, Table, Tone};
use super::rpt_entries::Builtin;
use crate::panes::histogram::{self, Curve as CurveLine, Marker, Plot, Segment};
use crate::panes::usage::{self, ACTUAL, BUDGET, REMAINING, TIER_H};
use crate::print::ops::Clipped;
use crate::theme;
use crate::timescale::{self, LabelOptions, TimeScale, Unit};
use eframe::egui::{self, Align2, Color32, Pos2, Rect, Stroke, Vec2};
use openxer_engine::calendar::DAY;
use openxer_interchange::scurve_xlsx::{self, Style};
use openxer_xer::scurve::{Breakdown, Curve, EarnedHours, Group, Measure, Options, Period};

/// The chart's height on the page, in page pixels.
pub const CHART_HEIGHT: f32 = 420.;
/// Room left of the plot for the Y axis labels.
const AXIS_W: f32 = 70.;
/// Room right of the plot for the legend, so it never covers a bar.
const LEGEND_W: f32 = 250.;
/// Periods in one table of a breakdown matrix.
const MATRIX_PERIODS: usize = 10;
/// The earned marker's fill.
pub const EARNED: Color32 = theme::color(theme::AMBER);

/// What a S-Curve or Earned Hours tab runs again on Refresh: the options
/// its dialog chose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Spec {
    Scurve { options: Options, filtered: bool },
    Earned { group: Group },
}

impl Spec {
    pub fn builtin(self) -> Builtin {
        match self {
            Spec::Scurve { .. } => Builtin::Scurve,
            Spec::Earned { .. } => Builtin::EarnedHours,
        }
    }
}

/// The figures behind a tab.
pub enum Figures {
    Curve(Box<Curve>),
    Earned(Box<EarnedHours>),
}

/// A built-in report's document: its tables, its chart as print ops (page
/// pixels, with its size), and what its workbook is written from.
pub struct ProgressDoc {
    pub report: Report,
    pub chart: Option<(Vec<Clipped>, Vec2)>,
    pub figures: Figures,
    /// Info rows only the app knows (the scope of an S-curve).
    pub extra: Vec<(String, String)>,
    pub style: Style,
}

impl ProgressDoc {
    /// The workbook: the same writer the command line and MCP use.
    pub fn workbook(&self) -> Result<Vec<u8>, String> {
        match &self.figures {
            Figures::Curve(c) => scurve_xlsx::curve_bytes(c, &self.extra, &self.style),
            Figures::Earned(e) => scurve_xlsx::earned_bytes(e, &self.extra, &self.style),
        }
    }

    pub fn warnings(&self) -> Vec<String> {
        match &self.figures {
            Figures::Curve(c) => c.warnings.clone(),
            Figures::Earned(e) => e.warnings.clone(),
        }
    }

    /// The pages: the chart under the title block, then the tables.
    pub fn pages(&self) -> Vec<crate::print::ops::Page> {
        super::pdf::pages_with(
            &self.report,
            self.chart
                .as_ref()
                .map(|(ops, size)| (ops.as_slice(), *size)),
        )
    }
}

/// A value as the tables show it: money in the currency format, other
/// measures with two decimals.
fn value_text(measure: Measure, v: f64) -> String {
    match measure {
        Measure::Cost => crate::format::cost(v),
        _ => grouped(v, 2),
    }
}

/// `v` with `decimals` decimals and thousands separators.
fn grouped(v: f64, decimals: usize) -> String {
    let text = format!("{:.*}", decimals, v.abs());
    let (whole, frac) = text.split_once('.').unwrap_or((&text, ""));
    let mut out = String::new();
    for (k, ch) in whole.chars().enumerate() {
        if k > 0 && (whole.len() - k) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if !frac.is_empty() {
        out.push('.');
        out.push_str(frac);
    }
    if v < 0. && out.chars().any(|c| c.is_ascii_digit() && c != '0') {
        out.insert(0, '-');
    }
    out
}

fn number(measure: Measure, v: f64) -> Cell {
    Cell::number(v, value_text(measure, v))
}

fn percent(share: f64) -> Cell {
    Cell::number(share, format!("{:.1}%", share * 100.))
}

/// A calendar day of model minute `m` in the user's date format.
fn day(origin: i64, m: i64) -> Cell {
    let epoch = chrono::NaiveDate::from_ymd_opt(1899, 12, 30).expect("a valid date");
    let text = epoch
        .checked_add_signed(chrono::Duration::days(origin + m.div_euclid(DAY)))
        .map(crate::format::day)
        .unwrap_or_default();
    Cell::date(origin, Some(m.div_euclid(DAY) * DAY), text)
}

fn day_text(origin: i64, m: i64) -> String {
    day(origin, m).text().to_string()
}

fn period_word(o: &Options) -> &'static str {
    match o.period {
        Period::Day => "day",
        Period::Week => "week",
        Period::Month => "month",
        Period::Financial => "financial period",
    }
}

fn share(a: f64, b: f64) -> f64 {
    if b.abs() > 1e-12 {
        a / b
    } else {
        0.
    }
}

fn total_row(cells: Vec<Cell>) -> Row {
    Row::new(cells, Tone::Band(1))
}

/// The S-curve's tables: the periods with their running totals, and for a
/// breakdown its totals and its matrices by period.
pub fn curve_report(c: &Curve, extra: &[(String, String)]) -> Report {
    let m = c.options.measure;
    let unit = m.unit();
    let mut header = vec![
        c.projects.join(", "),
        format!(
            "{} per {}; budgeted from {}",
            m.label(),
            period_word(&c.options),
            c.budget_source
        ),
        c.data_date.map_or("No data date".into(), |d| {
            format!("Data date {}", day_text(c.origin, d))
        }),
    ];
    header.extend(extra.iter().map(|(k, v)| format!("{k}: {v}")));
    if let Some(e) = c.earned {
        header.push(format!("Earned to the data date: {}", value_text(m, e)));
    }
    let late = c.options.late;
    let mut columns = vec![
        Column::left("Period Start", 9.),
        Column::left("Period End", 9.),
        Column::right("Budgeted", 8.),
        Column::right("Actual", 8.),
        Column::right("Remaining", 8.),
        Column::right("Cum Budgeted", 9.),
        Column::right("Cum Actual", 9.),
        Column::right("Cum Remaining", 9.),
        Column::right("Cum Act + Rem", 9.),
        Column::right("Cum Bud %", 7.),
        Column::right("Cum Act %", 7.),
        Column::right("Earned", 8.),
    ];
    if late {
        columns.push(Column::right("Late Dates", 8.));
        columns.push(Column::right("Cum Late", 9.));
    }
    let t = &c.total;
    let budget_total: f64 = t.budget.iter().sum();
    let mut running = [0.; 4];
    let mut rows = vec![];
    for (k, &(s, e)) in c.periods.iter().enumerate() {
        running[0] += t.budget[k];
        running[1] += t.actual[k];
        running[2] += t.remaining[k];
        let mut cells = vec![
            day(c.origin, s),
            day(c.origin, e - DAY),
            number(m, t.budget[k]),
            number(m, t.actual[k]),
            number(m, t.remaining[k]),
            number(m, running[0]),
            number(m, running[1]),
            number(m, running[2]),
            number(m, running[1] + running[2]),
            percent(share(running[0], budget_total)),
            percent(share(running[1], budget_total)),
            match (c.earned_period == Some(k), c.earned) {
                (true, Some(v)) => number(m, v),
                _ => Cell::Text(String::new()),
            },
        ];
        if late {
            running[3] += t.late[k];
            cells.push(number(m, t.late[k]));
            cells.push(number(m, running[3]));
        }
        rows.push(Row::new(cells, Tone::Normal));
    }
    let blank = || Cell::Text(String::new());
    let mut total = vec![
        Cell::Text("Total".into()),
        blank(),
        number(m, budget_total),
        number(m, t.actual.iter().sum()),
        number(m, t.remaining.iter().sum()),
    ];
    total.resize_with(12, blank);
    if late {
        total.push(number(m, t.late.iter().sum()));
        total.push(blank());
    }
    rows.push(total_row(total));
    let mut sections = vec![Section {
        heading: format!("{unit} per {}", period_word(&c.options)),
        paragraphs: vec![],
        tables: vec![Table {
            name: "S-Curve".into(),
            caption: String::new(),
            columns,
            rows,
        }],
    }];
    if c.options.breakdown != Breakdown::Total {
        sections.push(breakdown_section(c));
    }
    Report {
        title: format!("S-Curve: {}", m.label()),
        header,
        sections,
        footer: "OpenXER S-Curve".into(),
    }
}

/// The breakdown: each row's totals, then its values by period (budgeted,
/// actual, remaining), a table for every few periods.
fn breakdown_section(c: &Curve) -> Section {
    let m = c.options.measure;
    let (word, key_title, name_title) = match c.options.breakdown {
        Breakdown::Resource => ("resource", "Resource ID", "Resource Name"),
        Breakdown::Wbs1 => ("WBS level 1", "WBS", "WBS Name"),
        _ => ("WBS level 2", "WBS", "WBS Name"),
    };
    let sum = |v: &[f64]| v.iter().sum::<f64>();
    let mut rows: Vec<Row> = c
        .lines
        .iter()
        .map(|l| {
            let s = &l.series;
            Row::new(
                vec![
                    Cell::Text(l.key.clone()),
                    Cell::Text(l.name.clone()),
                    number(m, sum(&s.budget)),
                    number(m, sum(&s.actual)),
                    number(m, sum(&s.remaining)),
                    number(m, sum(&s.actual) + sum(&s.remaining)),
                    percent(share(sum(&s.actual), sum(&s.budget))),
                ],
                Tone::Normal,
            )
        })
        .collect();
    let all = |pick: fn(&openxer_xer::scurve::Series) -> &Vec<f64>| -> f64 {
        c.lines.iter().map(|l| sum(pick(&l.series))).sum()
    };
    let (b, a, r) = (
        all(|s| &s.budget),
        all(|s| &s.actual),
        all(|s| &s.remaining),
    );
    rows.push(total_row(vec![
        Cell::Text("Total".into()),
        Cell::Text(String::new()),
        number(m, b),
        number(m, a),
        number(m, r),
        number(m, a + r),
        percent(share(a, b)),
    ]));
    let mut tables = vec![Table {
        name: format!("By {word}"),
        caption: format!("Totals by {word}"),
        columns: vec![
            Column::left(key_title, 12.),
            Column::left(name_title, 24.),
            Column::right("Budgeted", 10.),
            Column::right("Actual", 10.),
            Column::right("Remaining", 10.),
            Column::right("Actual + Remaining", 12.),
            Column::right("Actual % of Budget", 10.),
        ],
        rows,
    }];
    let n = c.periods.len();
    for (which, pick) in [("Budgeted", 0usize), ("Actual", 1), ("Remaining", 2)] {
        let values = |l: &openxer_xer::scurve::Line| -> Vec<f64> {
            match pick {
                0 => l.series.budget.clone(),
                1 => l.series.actual.clone(),
                _ => l.series.remaining.clone(),
            }
        };
        let mut from = 0;
        while from < n.max(1) {
            let to = (from + MATRIX_PERIODS).min(n);
            let last = to == n;
            let mut columns = vec![Column::left(key_title, 14.)];
            for &(s, _) in &c.periods[from..to] {
                columns.push(Column::right(&day_text(c.origin, s), 9.));
            }
            if last {
                columns.push(Column::right("Total", 10.));
            }
            let mut rows = vec![];
            let mut column_totals = vec![0.; to - from];
            for l in &c.lines {
                let v = values(l);
                let mut cells = vec![Cell::Text(l.key.clone())];
                for k in from..to {
                    cells.push(number(m, v[k]));
                    column_totals[k - from] += v[k];
                }
                if last {
                    cells.push(number(m, sum(&v)));
                }
                rows.push(Row::new(cells, Tone::Normal));
            }
            let mut cells = vec![Cell::Text("Total".into())];
            cells.extend(column_totals.iter().map(|v| number(m, *v)));
            if last {
                cells.push(number(m, c.lines.iter().map(|l| sum(&values(l))).sum()));
            }
            rows.push(total_row(cells));
            tables.push(Table {
                name: format!("By {word} - {which}"),
                caption: format!(
                    "{which} by {word}, periods {} to {} of {n}",
                    from + 1,
                    to.max(from + 1)
                ),
                columns,
                rows,
            });
            from = to.max(from + 1);
        }
    }
    Section {
        heading: format!("By {word}"),
        paragraphs: vec![],
        tables,
    }
}

/// The earned hours tables: earned to date by the chosen rows, and by
/// labor assignment.
pub fn earned_report(e: &EarnedHours) -> Report {
    let m = Measure::Labor;
    let (key_title, name_title) = match e.group {
        Group::Resource => ("Resource ID", "Resource Name"),
        Group::Wbs => ("WBS", "WBS Name"),
        Group::Activity => ("Activity ID", "Activity Name"),
    };
    let mut rows: Vec<Row> = e
        .rows
        .iter()
        .map(|r| {
            Row::new(
                vec![
                    Cell::Text(r.key.clone()),
                    Cell::Text(r.name.clone()),
                    number(m, r.budgeted),
                    number(m, r.earned),
                    number(m, r.actual),
                    number(m, r.remaining),
                    number(m, r.actual + r.remaining),
                    percent(share(r.earned, r.budgeted)),
                ],
                Tone::Normal,
            )
        })
        .collect();
    let sum =
        |pick: fn(&openxer_xer::scurve::EarnedRow) -> f64| e.rows.iter().map(pick).sum::<f64>();
    let (b, ea, a, r) = (
        sum(|r| r.budgeted),
        sum(|r| r.earned),
        sum(|r| r.actual),
        sum(|r| r.remaining),
    );
    rows.push(total_row(vec![
        Cell::Text("Total".into()),
        Cell::Text(String::new()),
        number(m, b),
        number(m, ea),
        number(m, a),
        number(m, r),
        number(m, a + r),
        percent(share(ea, b)),
    ]));
    let details: Vec<Row> = e
        .details
        .iter()
        .map(|d| {
            Row::new(
                vec![
                    Cell::Text(d.activity_id.clone()),
                    Cell::Text(d.activity_name.clone()),
                    Cell::Text(d.wbs.clone()),
                    Cell::Text(d.resource_id.clone()),
                    Cell::Text(d.resource_name.clone()),
                    number(m, d.budgeted),
                    percent(d.percent / 100.),
                    number(m, d.earned),
                    number(m, d.actual),
                    number(m, d.remaining),
                ],
                Tone::Normal,
            )
        })
        .collect();
    Report {
        title: "Earned Hours".into(),
        header: vec![
            e.project.clone(),
            e.data_date.map_or("No data date".into(), |d| {
                format!("Cumulative to the data date {}", day_text(e.origin, d))
            }),
            format!("Budgeted from {}", e.budget_source),
            e.percent_rule.clone(),
        ],
        sections: vec![
            Section {
                heading: "Earned to date".into(),
                paragraphs: vec![],
                tables: vec![Table {
                    name: scurve_xlsx::EARNED_SHEET.into(),
                    caption: format!("One row per {}", e.group.label().to_lowercase()),
                    columns: vec![
                        Column::left(key_title, 12.),
                        Column::left(name_title, 26.),
                        Column::right("Budgeted Units", 10.),
                        Column::right("Earned Units", 10.),
                        Column::right("Actual Units", 10.),
                        Column::right("Remaining Units", 10.),
                        Column::right("At Completion Units", 11.),
                        Column::right("% Earned", 7.),
                    ],
                    rows,
                }],
            },
            Section {
                heading: "By activity".into(),
                paragraphs: vec![],
                tables: vec![Table {
                    name: scurve_xlsx::DETAIL_SHEET.into(),
                    caption: "Hours, one row per labor assignment, by Activity ID then Resource ID"
                        .into(),
                    columns: vec![
                        Column::left("Activity ID", 10.),
                        Column::left("Activity Name", 22.),
                        Column::left("WBS", 12.),
                        Column::left("Resource ID", 9.),
                        Column::left("Resource Name", 14.),
                        Column::right("Budgeted", 9.),
                        Column::right("% Complete", 8.),
                        Column::right("Earned", 9.),
                        Column::right("Actual", 9.),
                        Column::right("Remaining", 9.),
                    ],
                    rows: details,
                }],
            },
        ],
        footer: "OpenXER Earned Hours".into(),
    }
}

/// The timescale units under the chart: the period's (finer) above a
/// coarser one.
fn tier_units(p: Period) -> (Unit, Unit) {
    match p {
        Period::Day => (Unit::Day, Unit::Month),
        Period::Week => (Unit::Week, Unit::Month),
        Period::Month | Period::Financial => (Unit::Month, Unit::Year),
    }
}

/// The S-curve chart in `rect`: bars per period (budgeted; actual with
/// remaining on top), the cumulative budgeted, actual and actual plus
/// remaining curves (and the late dates one) on the right scale, the
/// earned marker at the data date, the data date line, the timescale and
/// the legend, all with the usage profiles' painters.
pub fn paint_curve_chart(ui: &egui::Ui, rect: Rect, c: &Curve, labels: &LabelOptions) {
    let p = ui.painter().with_clip_rect(rect);
    p.rect_filled(rect, 0., Color32::WHITE);
    let m = c.options.measure;
    let (Some(&(first, _)), Some(&(_, last))) = (c.periods.first(), c.periods.last()) else {
        p.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "No periods to chart",
            crate::chrome::table_font(),
            crate::chrome::INK,
        );
        return;
    };
    let tiers = Rect::from_min_max(
        Pos2::new(rect.left(), rect.bottom() - 2. * TIER_H),
        Pos2::new(rect.right() - LEGEND_W, rect.bottom()),
    );
    let body = Rect::from_min_max(
        Pos2::new(rect.left() + AXIS_W, rect.top()),
        Pos2::new(rect.right() - LEGEND_W, tiers.top()),
    );
    let weeks = (last - first) as f64 / (7 * DAY) as f64;
    let axis = TimeScale::new(
        first as f64,
        f64::from(body.width()) / weeks.max(1e-6),
        body.left(),
    );
    let t = &c.total;
    let bars: Vec<Vec<Vec<Segment>>> = (0..c.periods.len())
        .map(|k| {
            let (a, r) = (t.actual[k], t.remaining[k]);
            vec![
                vec![Segment {
                    from: 0.,
                    to: t.budget[k],
                    colors: BUDGET,
                }],
                vec![
                    Segment {
                        from: 0.,
                        to: a,
                        colors: ACTUAL,
                    },
                    Segment {
                        from: a,
                        to: a + r,
                        colors: REMAINING,
                    },
                ],
            ]
        })
        .collect();
    let both: Vec<f64> = (0..c.periods.len())
        .map(|k| t.actual[k] + t.remaining[k])
        .collect();
    let mut curves = vec![
        CurveLine {
            totals: usage::running(0., &t.budget),
            color: BUDGET.1,
            dashed: false,
        },
        CurveLine {
            totals: usage::running(0., &t.actual),
            color: ACTUAL.1,
            dashed: false,
        },
        CurveLine {
            totals: usage::running(0., &both),
            color: REMAINING.1,
            dashed: true,
        },
    ];
    let late = c.options.late && t.late.len() == c.periods.len();
    if late {
        curves.push(CurveLine {
            totals: usage::running(0., &t.late),
            color: crate::chrome::INK,
            dashed: true,
        });
    }
    let markers: Vec<Marker> = match (c.data_date, c.earned) {
        (Some(at), Some(value)) => vec![Marker {
            at,
            value,
            color: EARNED,
        }],
        _ => vec![],
    };
    let quantity = |v: f64| match m {
        Measure::Cost => crate::format::cost(v),
        Measure::Labor => crate::format::units(v),
        Measure::Duration => grouped(v, 0),
    };
    histogram::paint_plot(
        &p,
        body,
        &Plot {
            axis: &axis,
            periods: &c.periods,
            bars: &bars,
            limit: None,
            curves: &curves,
            markers: &markers,
            picked: None,
            data_date: c.data_date,
            quantity: &quantity,
            style: Default::default(),
        },
    );
    // What each scale counts.
    let unit = m.unit();
    p.text(
        Pos2::new(rect.left() + 4., rect.top() + 4.),
        Align2::LEFT_TOP,
        format!("{unit} per {}", period_word(&c.options)),
        crate::chrome::table_font(),
        crate::chrome::INK,
    );
    p.text(
        Pos2::new(body.right() - 4., rect.top() + 4.),
        Align2::RIGHT_TOP,
        format!("Cumulative {}", unit.to_lowercase()),
        crate::chrome::table_font(),
        crate::chrome::INK,
    );
    // The timescale: the periods' unit above a coarser one.
    let (fine, coarse) = tier_units(c.options.period);
    let cells = |unit: Unit, upper: bool| {
        timescale::band_cells(unit, upper, c.origin, first as f64, last as f64, labels)
    };
    let tp = p.with_clip_rect(Rect::from_min_max(
        Pos2::new(body.left(), tiers.top()),
        tiers.max,
    ));
    usage::paint_tier(
        &tp,
        Rect::from_min_size(tiers.min, Vec2::new(tiers.width(), TIER_H)),
        &axis,
        fine,
        &cells(fine, false),
    );
    usage::paint_tier(
        &tp,
        Rect::from_min_max(Pos2::new(tiers.left(), tiers.top() + TIER_H), tiers.max),
        &axis,
        coarse,
        &cells(coarse, true),
    );
    crate::chrome::hline(
        &tp,
        tiers.left(),
        tiers.right(),
        tiers.top(),
        crate::chrome::INK,
    );
    let line = |color: Color32| (color, color);
    let mut legend: Vec<histogram::LegendEntry> = vec![
        ("Budgeted per period", BUDGET, false),
        ("Actual per period", ACTUAL, false),
        ("Remaining per period", REMAINING, false),
        ("Cum Budgeted (right scale)", line(BUDGET.1), true),
        ("Cum Actual (right scale)", line(ACTUAL.1), true),
        ("Cum Actual + Remaining (dashed)", line(REMAINING.1), true),
    ];
    if late {
        legend.push(("Cum Late Dates (dashed)", line(crate::chrome::INK), true));
    }
    if !markers.is_empty() {
        legend.push((
            "Earned to the data date",
            (EARNED, crate::chrome::INK),
            false,
        ));
    }
    histogram::paint_legend(
        &p,
        Pos2::new(body.right() + 12., body.top() + 24.),
        "",
        &legend,
    );
    p.rect_stroke(
        rect,
        0.,
        Stroke::new(1_f32, usage::LINE),
        egui::StrokeKind::Inside,
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn grouped_numbers() {
        assert_eq!(super::grouped(1234567.891, 2), "1,234,567.89");
        assert_eq!(super::grouped(-12.5, 2), "-12.50");
        assert_eq!(super::grouped(999., 0), "999");
        assert_eq!(super::grouped(-0.001, 2), "0.00");
    }
}
