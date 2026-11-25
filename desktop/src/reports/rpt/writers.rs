//! Output of a generated report: an HTML file, an ASCII text file, and
//! pages for the print module (PDF file and Print Preview). All of them read
//! the same paint list, so a page looks the same in every format.
use super::engine::{Generated, Kind, Line, PageOut};
use super::model::{Align, Font, Rgb};
use crate::print::ops::{Clipped, Op, Page as PrintPage};
use eframe::egui::{Color32, Pos2, Rect, Vec2};

/// One thing to draw on a page, in layout pixels from the paper's top left.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Fill {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Rgb,
    },
    Line {
        from: (f32, f32),
        to: (f32, f32),
        color: Rgb,
    },
    Text {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        text: String,
        align: Align,
        font: Font,
    },
    Image {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        file: String,
    },
}

/// Paper size in layout pixels.
pub fn paper(g: &Generated) -> (f32, f32) {
    (
        g.settings.width * 96. / g.scale,
        g.settings.length * 96. / g.scale,
    )
}

fn line_items(l: &Line, x0: f32, y: f32, out: &mut Vec<Item>) {
    if let Some(bg) = l.background {
        let w = l.cells.iter().map(|c| c.x + c.w).fold(0., f32::max);
        out.push(Item::Fill {
            x: x0,
            y,
            w,
            h: l.height,
            color: bg,
        });
    }
    for c in &l.cells {
        let (x, w, h) = (x0 + c.x, c.w, c.h.min(l.height).max(1.));
        if let Some(bg) = c.background {
            out.push(Item::Fill {
                x,
                y,
                w,
                h,
                color: bg,
            });
        }
        match &c.kind {
            Kind::Text => {
                if !c.text.trim().is_empty() {
                    out.push(Item::Text {
                        x,
                        y,
                        w,
                        h,
                        text: c.text.clone(),
                        align: c.align,
                        font: c.font.clone(),
                    });
                }
            }
            Kind::Line(n) => {
                let mid = y + (h / 2.).floor();
                for k in 0..*n {
                    let yy = mid + 3. * k as f32;
                    out.push(Item::Line {
                        from: (x, yy),
                        to: (x + w, yy),
                        color: Rgb::BLACK,
                    });
                }
            }
            Kind::Image(file) => out.push(Item::Image {
                x,
                y,
                w,
                h,
                file: file.clone(),
            }),
        }
        let b = c.borders;
        for (on, a, z) in [
            (b.left, (x, y), (x, y + h)),
            (b.right, (x + w, y), (x + w, y + h)),
            (b.top, (x, y), (x + w, y)),
            (b.bottom, (x, y + h), (x + w, y + h)),
        ] {
            if on {
                out.push(Item::Line {
                    from: a,
                    to: z,
                    color: Rgb::BLACK,
                });
            }
        }
    }
}

/// The paint list of one page.
pub fn items(g: &Generated, page: &PageOut) -> Vec<Item> {
    let mut out = vec![];
    let m = &g.settings.margins;
    let k = 96. / g.scale;
    let (left, top) = (m.left * k, m.top * k);
    let head_h: f32 = page.header.iter().map(|p| p.line.height).sum();
    for p in &page.header {
        line_items(&p.line, left, top + p.y, &mut out);
    }
    for p in &page.body {
        line_items(&p.line, left, top + head_h + p.y, &mut out);
    }
    let foot_h: f32 = page.footer.iter().map(|p| p.line.height).sum();
    for p in &page.footer {
        line_items(&p.line, left, top + g.height - foot_h + p.y, &mut out);
    }
    // Header and footer bands sit in the page margins.
    let (paper_w, paper_h) = paper(g);
    let content_w = paper_w - (m.left + m.right) * k;
    for (parts, texts, y) in [
        (&g.settings.header, &page.band_header, m.header * k),
        (
            &g.settings.footer,
            &page.band_footer,
            paper_h - m.footer * k - 14.,
        ),
    ] {
        for (i, (part, text)) in parts.iter().zip(texts).enumerate() {
            let text = text.replace(['\r', '\n'], " ");
            if text.trim().is_empty() {
                continue;
            }
            let font = Font {
                name: part.font_name.clone(),
                px: (part.size * 96. / 72.).clamp(4., 60.),
                bold: part.bold,
                italic: part.italic,
                underline: false,
                color: Rgb::parse(&part.color.to_string()).unwrap_or(Rgb::BLACK),
            };
            out.push(Item::Text {
                x: left,
                y,
                w: content_w,
                h: font.px * 1.4,
                text,
                align: [Align::Left, Align::Center, Align::Right][i],
                font,
            });
        }
    }
    out
}

// ------------------------------------------------------------------ HTML

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if c.is_control() && c != '\n' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

fn css_font(f: &Font) -> String {
    let family: String = f
        .name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
        .collect();
    format!(
        "font-family:'{family}',Arial,sans-serif;font-size:{}px;color:{};{}{}{}",
        trim(f.px),
        f.color.css(),
        if f.bold { "font-weight:bold;" } else { "" },
        if f.italic { "font-style:italic;" } else { "" },
        if f.underline {
            "text-decoration:underline;"
        } else {
            ""
        },
    )
}

fn trim(v: f32) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The report as one HTML document, a positioned page per printed page.
pub fn html(g: &Generated) -> String {
    let (pw, ph) = paper(g);
    let mut out = String::new();
    out.push_str("<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n");
    out.push_str(&format!("<title>{}</title>\n", esc(&g.title)));
    out.push_str("<style>\nbody{background:#808080;margin:0;padding:12px}\n");
    out.push_str(".page{position:relative;background:#fff;margin:0 auto 12px;overflow:hidden;box-shadow:2px 3px 6px rgba(0,0,0,.4)}\n");
    out.push_str(
        ".c,.l,.f{position:absolute;overflow:hidden;white-space:nowrap;box-sizing:border-box}\n",
    );
    out.push_str(".c{padding:0 2px}\n@media print{body{background:#fff;padding:0}.page{margin:0;box-shadow:none;page-break-after:always}}\n</style>\n</head>\n<body>\n");
    for page in &g.pages {
        out.push_str(&format!(
            "<div class=\"page\" style=\"width:{}px;height:{}px\">\n",
            trim(pw),
            trim(ph)
        ));
        for item in items(g, page) {
            match item {
                Item::Fill { x, y, w, h, color } => out.push_str(&format!(
                    "<div class=\"f\" style=\"left:{}px;top:{}px;width:{}px;height:{}px;background:{}\"></div>\n",
                    trim(x), trim(y), trim(w), trim(h), color.css()
                )),
                Item::Line { from, to, color } => {
                    let (x, y) = (from.0.min(to.0), from.1.min(to.1));
                    let (w, h) = ((to.0 - from.0).abs().max(1.), (to.1 - from.1).abs().max(1.));
                    out.push_str(&format!(
                        "<div class=\"f\" style=\"left:{}px;top:{}px;width:{}px;height:{}px;background:{}\"></div>\n",
                        trim(x), trim(y), trim(w), trim(h), color.css()
                    ));
                }
                Item::Text { x, y, w, h, text, align, font } => out.push_str(&format!(
                    "<div class=\"c\" style=\"left:{}px;top:{}px;width:{}px;height:{}px;line-height:{}px;text-align:{};{}\">{}</div>\n",
                    trim(x), trim(y), trim(w), trim(h), trim(h),
                    match align { Align::Left => "left", Align::Right => "right", Align::Center => "center" },
                    css_font(&font), esc(&text)
                )),
                Item::Image { x, y, w, h, file } => out.push_str(&format!(
                    "<div class=\"c\" style=\"left:{}px;top:{}px;width:{}px;height:{}px;border:1px solid #888;color:#666;font:11px Arial\">{}</div>\n",
                    trim(x), trim(y), trim(w), trim(h), esc(&file)
                )),
            }
        }
        out.push_str("</div>\n");
    }
    out.push_str("</body>\n</html>\n");
    out
}

/// A rough check that tags open and close in order (for tests).
#[cfg(test)]
pub fn html_is_balanced(html: &str) -> Result<usize, String> {
    let mut stack: Vec<String> = vec![];
    let mut tags = 0;
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        let after = &rest[open + 1..];
        let close = after.find('>').ok_or("unterminated tag")?;
        let tag = &after[..close];
        rest = &after[close + 1..];
        if tag.starts_with('!') || tag.starts_with('?') {
            continue;
        }
        tags += 1;
        if let Some(name) = tag.strip_prefix('/') {
            if stack.pop().as_deref() != Some(name.trim()) {
                return Err(format!("unexpected </{name}>"));
            }
        } else {
            let name = tag
                .split(|c: char| c.is_whitespace() || c == '/')
                .next()
                .unwrap_or("")
                .to_string();
            if !matches!(name.as_str(), "meta" | "br" | "hr" | "img") {
                stack.push(name);
            }
        }
    }
    if stack.is_empty() {
        Ok(tags)
    } else {
        Err(format!("unclosed {stack:?}"))
    }
}

// ------------------------------------------------------------------ text

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextOptions {
    pub delimiter: String,
    pub qualifier: String,
}

impl Default for TextOptions {
    fn default() -> Self {
        TextOptions {
            delimiter: "\t".into(),
            qualifier: "\"".into(),
        }
    }
}

/// The delimiters offered by Run Report: (caption, text).
pub const DELIMITERS: [(&str, &str); 5] = [
    ("Tab", "\t"),
    ("Comma", ","),
    ("Semicolon", ";"),
    ("Space", " "),
    ("Pipe", "|"),
];

/// The qualifiers offered by Run Report: (caption, text).
pub const QUALIFIERS: [(&str, &str); 3] =
    [("Double quote", "\""), ("Single quote", "'"), ("None", "")];

/// The report as delimited text: one record per row that has text, fields in
/// cell order, each wrapped in the qualifier (doubled inside). Page header and
/// footer rows appear once.
pub fn text(g: &Generated, o: &TextOptions) -> String {
    let mut out = String::new();
    for line in g.all_lines() {
        let mut cells: Vec<_> = line.cells.iter().filter(|c| c.kind == Kind::Text).collect();
        if cells.is_empty() {
            continue;
        }
        cells.sort_by(|a, b| a.x.total_cmp(&b.x));
        if cells.iter().all(|c| c.text.trim().is_empty()) {
            continue;
        }
        let fields: Vec<String> = cells
            .iter()
            .map(|c| {
                let t = super::super::csv::guard(c.text.trim_end());
                if o.qualifier.is_empty() {
                    super::super::csv::field_with(&t, &o.delimiter, "")
                } else {
                    format!(
                        "{q}{}{q}",
                        t.replace(&o.qualifier, &format!("{}{}", o.qualifier, o.qualifier)),
                        q = o.qualifier
                    )
                }
            })
            .collect();
        out.push_str(&fields.join(&o.delimiter));
        out.push_str("\r\n");
    }
    out
}

/// The report as a workbook with one sheet: every row that has text, each
/// cell in the column its left edge falls in, so the cells of a table line
/// up from one row to the next.
pub fn excel(g: &Generated) -> Result<Vec<u8>, String> {
    openxer_interchange::report_xlsx::report_bytes(&[excel_sheet(g)], "dd-mmm-yy hh:mm")
}

/// The sheet `excel` writes.
pub fn excel_sheet(g: &Generated) -> openxer_interchange::report_xlsx::ReportSheet {
    use openxer_interchange::report_xlsx::{ReportCell, ReportRow, ReportSheet};
    let lines: Vec<Vec<&super::engine::Out>> = g
        .all_lines()
        .into_iter()
        .filter_map(|line| {
            let mut cells: Vec<&super::engine::Out> = line
                .cells
                .iter()
                .filter(|c| c.kind == Kind::Text && !c.text.trim().is_empty())
                .collect();
            cells.sort_by(|a, b| a.x.total_cmp(&b.x));
            (!cells.is_empty()).then_some(cells)
        })
        .collect();
    let mut edges: Vec<i64> = lines.iter().flatten().map(|c| c.x.round() as i64).collect();
    edges.sort_unstable();
    edges.dedup();
    let mut widths = vec![6.0_f64; edges.len()];
    let mut rows = vec![];
    for cells in &lines {
        let mut row = vec![ReportCell::Empty; edges.len()];
        for c in cells {
            let column = edges
                .iter()
                .position(|e| *e == c.x.round() as i64)
                .unwrap_or(0);
            let text = c.text.trim_end().to_string();
            widths[column] = widths[column].max(text.chars().count() as f64 + 2.);
            // Numbers and dates are typed cells, so they sort and sum.
            row[column] = match c.typed {
                Some(super::engine::Typed::Number(x)) => ReportCell::Number(x),
                Some(super::engine::Typed::Date(d)) => ReportCell::Date(d),
                None => ReportCell::Text(text),
            };
        }
        rows.push(ReportRow {
            cells: row,
            ..Default::default()
        });
    }
    for w in &mut widths {
        *w = w.clamp(6., 80.);
    }
    ReportSheet {
        name: g.title.clone(),
        title: vec![g.title.clone()],
        header: vec![],
        widths,
        rows,
    }
}

// ------------------------------------------------------------ print pages

fn color(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

fn fit_text(s: &str, width: f32, size: f32, bold: bool) -> String {
    if super::super::pdf::text_width(s, size, bold) <= width {
        return s.to_string();
    }
    let mut out = String::new();
    for c in s.chars() {
        let next = format!("{out}{c}...");
        if super::super::pdf::text_width(&next, size, bold) > width {
            break;
        }
        out.push(c);
    }
    format!("{}...", out.trim_end())
}

/// The report as pages for the print module's PDF writer and Print Preview.
pub fn print_pages(g: &Generated) -> Vec<PrintPage> {
    let k = g.scale;
    let (pw, ph) = paper(g);
    g.pages
        .iter()
        .map(|page| {
            let mut ops: Vec<Clipped> = vec![];
            let whole = Rect::from_min_size(Pos2::ZERO, Vec2::new(pw * k, ph * k));
            let mut push = |clip: Rect, op: Op| ops.push(Clipped { clip, op });
            for item in items(g, page) {
                match item {
                    Item::Fill {
                        x,
                        y,
                        w,
                        h,
                        color: c,
                    } => {
                        let r =
                            Rect::from_min_size(Pos2::new(x * k, y * k), Vec2::new(w * k, h * k));
                        push(
                            whole,
                            Op::Fill {
                                rect: r,
                                color: color(c),
                            },
                        );
                    }
                    Item::Line { from, to, color: c } => push(
                        whole,
                        Op::Stroke {
                            points: vec![
                                Pos2::new(from.0 * k, from.1 * k),
                                Pos2::new(to.0 * k, to.1 * k),
                            ],
                            closed: false,
                            width: (k).max(0.5),
                            color: color(c),
                        },
                    ),
                    Item::Text {
                        x,
                        y,
                        w,
                        h,
                        text,
                        align,
                        font,
                    } => {
                        let text = fit_text(&text, (w - 4.) * k, font.px * k, font.bold);
                        let width = super::super::pdf::text_width(&text, font.px * k, font.bold);
                        let left = match align {
                            Align::Left => x * k + 2. * k,
                            Align::Right => (x + w) * k - 2. * k - width,
                            Align::Center => x * k + (w * k - width) / 2.,
                        };
                        let base = y * k + (h * k - font.px * k) / 2. + font.px * k * 0.8;
                        let clip =
                            Rect::from_min_size(Pos2::new(x * k, y * k), Vec2::new(w * k, h * k));
                        push(
                            clip.intersect(whole)
                                .expand2(Vec2::new(0., font.px * k * 0.25)),
                            Op::Text {
                                pos: Pos2::new(left, base),
                                text: text.clone(),
                                size: font.px * k,
                                bold: font.bold,
                                color: color(font.color),
                                width,
                            },
                        );
                        if font.underline {
                            push(
                                whole,
                                Op::Stroke {
                                    points: vec![
                                        Pos2::new(left, base + 1.5 * k),
                                        Pos2::new(left + width, base + 1.5 * k),
                                    ],
                                    closed: false,
                                    width: k.max(0.5),
                                    color: color(font.color),
                                },
                            );
                        }
                    }
                    Item::Image { x, y, w, h, file } => {
                        let r =
                            Rect::from_min_size(Pos2::new(x * k, y * k), Vec2::new(w * k, h * k));
                        push(
                            whole,
                            Op::Stroke {
                                points: vec![
                                    r.left_top(),
                                    r.right_top(),
                                    r.right_bottom(),
                                    r.left_bottom(),
                                ],
                                closed: true,
                                width: k.max(0.5),
                                color: Color32::from_gray(136),
                            },
                        );
                        let label = fit_text(&file, w * k - 4., 10. * k, false);
                        let width = super::super::pdf::text_width(&label, 10. * k, false);
                        push(
                            r,
                            Op::Text {
                                pos: Pos2::new(r.left() + 2. * k, r.top() + 12. * k),
                                text: label,
                                size: 10. * k,
                                bold: false,
                                color: Color32::from_gray(102),
                                width,
                            },
                        );
                    }
                }
            }
            PrintPage {
                size: Vec2::new(pw * k, ph * k),
                ops,
            }
        })
        .collect()
}

/// The report as a PDF file.
pub fn pdf(g: &Generated) -> Vec<u8> {
    crate::print::pdf::write(&print_pages(g), &g.title)
}
