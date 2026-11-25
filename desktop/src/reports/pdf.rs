//! A report as a PDF: US Letter landscape, the two standard Helvetica
//! faces (no embedded fonts), tables with a repeated header row on every
//! page, colored row marks and "Page n of N" footers. Text that does not fit
//! a column is cut with "...". The report is laid out in points into the
//! print display list and written by the print module's PDF writer.
use super::doc::{Report, Tone};
use crate::print::ops::{Clipped, Op, Page};
#[cfg(test)]
pub use crate::print::pdf::check;
use crate::print::pdf::{win_ansi, PT};
use eframe::egui::{Color32, Pos2, Rect, Vec2};

const PAGE_W: f32 = 792.;
const PAGE_H: f32 = 612.;
const MARGIN: f32 = 36.;
const ROW: f32 = 12.;
const BODY: f32 = 7.5;
const BOTTOM: f32 = PAGE_H - MARGIN - 14.;

/// Advance widths (1/1000 em) of ASCII 32 to 126 in Helvetica.
const REGULAR: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];
/// Advance widths of ASCII 32 to 126 in Helvetica-Bold.
const BOLD: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611, 975, 722, 722, 722, 722, 667,
    611, 778, 722, 278, 556, 722, 611, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 333, 278, 333, 584, 556, 333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556,
    278, 889, 611, 611, 611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584,
];

/// Report text as the print writer will encode it: arrows, curly quotes
/// and dashes become plain ASCII, tabs spaces, other controls are dropped.
fn plain(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\u{2192}' => out.push_str("->"),
            '\u{2018}' | '\u{2019}' => out.push('\''),
            '\u{201c}' | '\u{201d}' => out.push('"'),
            '\u{2013}' | '\u{2014}' => out.push('-'),
            '\t' => out.push(' '),
            _ if c.is_control() => {}
            _ => out.push(c),
        }
    }
    out
}

/// The text as single-byte WinAnsi codes.
fn ansi(s: &str) -> Vec<u8> {
    win_ansi(&plain(s))
}

pub fn text_width(s: &str, size: f32, bold: bool) -> f32 {
    let table = if bold { &BOLD } else { &REGULAR };
    ansi(s)
        .iter()
        .map(|&b| {
            if (32..=126).contains(&b) {
                table[(b - 32) as usize] as f32
            } else {
                556.
            }
        })
        .sum::<f32>()
        * size
        / 1000.
}

/// `s` cut to fit `width` points, ending in "..." when cut.
fn fit(s: &str, width: f32, size: f32, bold: bool) -> String {
    if text_width(s, size, bold) <= width {
        return s.to_string();
    }
    let mut out = String::new();
    for c in s.chars() {
        let next = format!("{out}{c}...");
        if text_width(&next, size, bold) > width {
            break;
        }
        out.push(c);
    }
    format!("{}...", out.trim_end())
}

/// Words of `s` broken into lines no wider than `width`.
fn wrap(s: &str, width: f32, size: f32) -> Vec<String> {
    let mut lines = vec![];
    let mut line = String::new();
    for word in s.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if text_width(&candidate, size, false) > width && !line.is_empty() {
            lines.push(std::mem::take(&mut line));
            line = word.to_string();
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

type Rgb = (f32, f32, f32);

fn tone_colors(tone: Tone) -> Option<(Rgb, Rgb)> {
    let black = (0., 0., 0.);
    match tone {
        Tone::Normal => None,
        Tone::Added | Tone::Good => Some(((0.875, 0.961, 0.875), black)),
        Tone::Deleted | Tone::Bad => Some(((0.976, 0.855, 0.855), black)),
        Tone::Changed => Some(((1., 0.969, 0.8), black)),
        Tone::Band(level) => {
            let (background, text) = crate::theme::report_band(level);
            let unit = |hex: u32| {
                let [r, g, b] = crate::theme::bytes(hex);
                (
                    f32::from(r) / 255.,
                    f32::from(g) / 255.,
                    f32::from(b) / 255.,
                )
            };
            Some((unit(background), unit(text)))
        }
    }
}

/// A color from 0..1 components.
fn color(c: Rgb) -> Color32 {
    let v = |x: f32| (x * 255.).round().clamp(0., 255.) as u8;
    Color32::from_rgb(v(c.0), v(c.1), v(c.2))
}

/// Page pixels (96 per inch) from points.
fn px(points: f32) -> f32 {
    points / PT
}

struct Layout<'r> {
    report: &'r Report,
    pages: Vec<Vec<Clipped>>,
    page: Vec<Clipped>,
    /// Distance of the next line's top from the top of the page.
    y: f32,
}

impl Layout<'_> {
    fn push(&mut self, op: Op) {
        let clip = Rect::from_min_size(Pos2::ZERO, Vec2::new(px(PAGE_W), px(PAGE_H)));
        self.page.push(Clipped { clip, op });
    }
    fn text(&mut self, x: f32, top: f32, size: f32, bold: bool, ink: Rgb, s: &str) {
        let s = plain(s);
        let width = px(text_width(&s, size, bold));
        self.push(Op::Text {
            pos: Pos2::new(px(x), px(top + size)),
            text: s,
            size: px(size),
            bold,
            color: color(ink),
            width,
        });
    }
    fn rect(&mut self, x: f32, top: f32, w: f32, h: f32, fill: Rgb) {
        self.push(Op::Fill {
            rect: Rect::from_min_size(Pos2::new(px(x), px(top)), Vec2::new(px(w), px(h))),
            color: color(fill),
        });
    }
    fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, gray: f32) {
        self.push(Op::Stroke {
            points: vec![Pos2::new(px(x0), px(y0)), Pos2::new(px(x1), px(y1))],
            closed: false,
            width: px(0.5),
            color: color((gray, gray, gray)),
        });
    }
    fn new_page(&mut self) {
        if !self.page.is_empty() {
            self.pages.push(std::mem::take(&mut self.page));
        }
        self.y = MARGIN;
        let title = self.report.title.clone();
        self.text(MARGIN, self.y, 8., true, (0.3, 0.3, 0.3), &title);
        self.y += 12.;
        self.line(MARGIN, self.y, PAGE_W - MARGIN, self.y, 0.6);
        self.y += 6.;
    }
    fn room(&mut self, h: f32) -> bool {
        if self.y + h > BOTTOM {
            self.new_page();
            false
        } else {
            true
        }
    }
    fn table_header(&mut self, xs: &[f32], widths: &[f32], cols: &[super::doc::Column]) {
        let width: f32 = widths.iter().sum();
        self.rect(MARGIN, self.y, width, ROW + 2., (0.86, 0.86, 0.86));
        for (k, c) in cols.iter().enumerate() {
            let s = fit(&c.title, widths[k] - 4., BODY, true);
            let x = if c.right {
                xs[k] + widths[k] - 2. - text_width(&s, BODY, true)
            } else {
                xs[k] + 2.
            };
            self.text(x, self.y + 3., BODY, true, (0., 0., 0.), &s);
        }
        self.y += ROW + 2.;
        self.line(MARGIN, self.y, MARGIN + width, self.y, 0.4);
    }
}

pub fn write(report: &Report) -> Vec<u8> {
    crate::print::pdf::write(&pages(report), &report.title)
}

/// The report laid out as pages, for the PDF writer and the viewer.
pub fn pages(report: &Report) -> Vec<Page> {
    pages_with(report, None)
}

/// Width in page pixels a figure under the title block may take.
pub fn figure_width() -> f32 {
    px(PAGE_W - 2. * MARGIN)
}

/// The report laid out as pages with a figure (print ops in page pixels,
/// with its size) under the title block, before the sections.
pub fn pages_with(report: &Report, figure: Option<(&[Clipped], Vec2)>) -> Vec<Page> {
    let mut l = Layout {
        report,
        pages: vec![],
        page: vec![],
        y: MARGIN,
    };
    let usable = PAGE_W - 2. * MARGIN;
    // Title block.
    let title = report.title.clone();
    l.text(MARGIN, l.y, 16., true, (0., 0., 0.), &title);
    l.y += 22.;
    for h in &report.header {
        l.text(MARGIN, l.y, 9., false, (0.2, 0.2, 0.2), h);
        l.y += 12.;
    }
    l.y += 4.;
    if let Some((ops, size)) = figure {
        let height = size.y * PT;
        l.room(height);
        crate::print::ops::place(
            ops,
            Rect::from_min_size(Pos2::ZERO, size),
            1.,
            Pos2::new(px(MARGIN), px(l.y)),
            &mut l.page,
        );
        l.y += height + 10.;
    }
    for section in &report.sections {
        l.room(18. + ROW * 3.);
        l.rect(MARGIN, l.y, usable, 16., (0.816, 0.816, 0.816));
        l.rect(MARGIN, l.y, 3., 16., (0., 0., 1.));
        l.text(
            MARGIN + 8.,
            l.y + 3.,
            10.,
            true,
            (0., 0., 0.),
            &section.heading,
        );
        l.y += 22.;
        for p in &section.paragraphs {
            for line in wrap(p, usable, 9.) {
                l.room(12.);
                l.text(MARGIN, l.y, 9., false, (0., 0., 0.), &line);
                l.y += 12.;
            }
            l.y += 2.;
        }
        for table in &section.tables {
            if table.columns.is_empty() {
                continue;
            }
            let total: f32 = table.columns.iter().map(|c| c.width.max(1.)).sum();
            let widths: Vec<f32> = table
                .columns
                .iter()
                .map(|c| usable * c.width.max(1.) / total)
                .collect();
            let mut xs = vec![MARGIN];
            for w in &widths[..widths.len() - 1] {
                xs.push(xs.last().unwrap() + w);
            }
            l.room(14. + (ROW + 2.) * 2.);
            if !table.caption.is_empty() {
                l.text(MARGIN, l.y, 8.5, true, (0., 0., 0.), &table.caption);
                l.y += 13.;
            }
            l.table_header(&xs, &widths, &table.columns);
            for row in &table.rows {
                if !l.room(ROW) {
                    l.table_header(&xs, &widths, &table.columns);
                }
                let colors = tone_colors(row.tone);
                let band = matches!(row.tone, Tone::Band(_));
                if let Some((fill, _)) = colors {
                    l.rect(MARGIN, l.y, usable, ROW, fill);
                }
                let ink = colors.map_or((0., 0., 0.), |c| c.1);
                let spans = band && row.cells.iter().skip(1).all(|c| c.text().is_empty());
                for (k, cell) in row.cells.iter().enumerate().take(table.columns.len()) {
                    let width = if spans { usable } else { widths[k] };
                    let s = fit(cell.text(), width - 4., BODY, band);
                    let right = table.columns[k].right && !spans;
                    let x = if right {
                        xs[k] + widths[k] - 2. - text_width(&s, BODY, band)
                    } else {
                        xs[k] + 2.
                    };
                    l.text(x, l.y + 2., BODY, band, ink, &s);
                    if spans {
                        break;
                    }
                }
                l.y += ROW;
                l.line(MARGIN, l.y, MARGIN + usable, l.y, 0.8);
            }
            l.y += 8.;
        }
    }
    if !l.page.is_empty() {
        l.pages.push(std::mem::take(&mut l.page));
    }
    if l.pages.is_empty() {
        l.pages.push(vec![]);
    }
    // Footers, now that the page count is known.
    let count = l.pages.len();
    l.pages
        .iter()
        .enumerate()
        .map(|(i, body)| {
            let mut f = Layout {
                report,
                pages: vec![],
                page: body.clone(),
                y: 0.,
            };
            let foot = PAGE_H - MARGIN + 4.;
            f.line(MARGIN, foot - 4., PAGE_W - MARGIN, foot - 4., 0.6);
            if !report.footer.is_empty() {
                let s = fit(&report.footer, usable - 80., 7., false);
                f.text(MARGIN, foot, 7., false, (0.3, 0.3, 0.3), &s);
            }
            let n = format!("Page {} of {count}", i + 1);
            let x = PAGE_W - MARGIN - text_width(&n, 7., false);
            f.text(x, foot, 7., false, (0.3, 0.3, 0.3), &n);
            Page {
                size: Vec2::new(px(PAGE_W), px(PAGE_H)),
                ops: f.page,
            }
        })
        .collect()
}
