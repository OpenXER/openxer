//! Vector PDF output of printed pages. Text uses the standard Helvetica
//! fonts every PDF reader carries (nothing is embedded); the screen draws
//! the same text in Arial, whose advance widths match Helvetica, so the
//! printed columns line up with the measured layout.
use super::ops::{Clipped, Op, Page};
use eframe::egui::{Color32, Pos2, Rect};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect as PdfRect, Ref, Str, TextStr};

/// Points per page pixel (pixels are 1/96 inch).
pub const PT: f32 = 0.75;

/// Colors are blended over white paper (egui colors are premultiplied).
fn rgb(color: Color32) -> [f32; 3] {
    let a = 1. - color.a() as f32 / 255.;
    let c = |v: u8| ((v as f32 / 255.) + a).min(1.);
    [c(color.r()), c(color.g()), c(color.b())]
}

/// Encodes text for the WinAnsi encoding (Windows code page 1252): every
/// character that code page holds prints. Others (Greek, Cyrillic, CJK and
/// most of Latin Extended-A, such as the Polish l with stroke or the Czech c
/// with caron) print `?`: the standard fonts are not embedded.
pub fn win_ansi(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| match c {
            ' '..='~' => c as u8,
            '\u{a0}'..='\u{ff}' => c as u32 as u8,
            '\u{20ac}' => 0x80,
            '\u{201a}' => 0x82,
            '\u{0192}' => 0x83,
            '\u{201e}' => 0x84,
            '\u{2026}' => 0x85,
            '\u{2020}' => 0x86,
            // The double dagger of a constrained Start or Finish.
            '\u{2021}' => 0x87,
            '\u{02c6}' => 0x88,
            '\u{2030}' => 0x89,
            '\u{0160}' => 0x8a,
            '\u{2039}' => 0x8b,
            '\u{0152}' => 0x8c,
            '\u{017d}' => 0x8e,
            '\u{2018}' => 0x91,
            '\u{2019}' => 0x92,
            '\u{201c}' => 0x93,
            '\u{201d}' => 0x94,
            '\u{2022}' => 0x95,
            '\u{2013}' => 0x96,
            '\u{2014}' => 0x97,
            '\u{02dc}' => 0x98,
            '\u{2122}' => 0x99,
            '\u{0161}' => 0x9a,
            '\u{203a}' => 0x9b,
            '\u{0153}' => 0x9c,
            '\u{017e}' => 0x9e,
            '\u{0178}' => 0x9f,
            _ => b'?',
        })
        .collect()
}

struct Writer<'a> {
    content: &'a mut Content,
    height: f32,
    /// Fill color of rectangles added to the path and not yet filled.
    pending: Option<Color32>,
}

impl Writer<'_> {
    /// Fills the rectangles gathered so far.
    fn flush(&mut self) {
        if self.pending.take().is_some() {
            self.content.fill_nonzero();
        }
    }
    fn x(&self, x: f32) -> f32 {
        x * PT
    }
    fn y(&self, y: f32) -> f32 {
        self.height - y * PT
    }
    fn path(&mut self, points: &[Pos2]) {
        for (k, p) in points.iter().enumerate() {
            let (x, y) = (self.x(p.x), self.y(p.y));
            if k == 0 {
                self.content.move_to(x, y);
            } else {
                self.content.line_to(x, y);
            }
        }
    }
    fn rect(&mut self, r: Rect) {
        let (x, y) = (self.x(r.left()), self.y(r.bottom()));
        self.content.rect(x, y, r.width() * PT, r.height() * PT);
    }
    fn op(&mut self, op: &Op) {
        if !matches!(op, Op::Fill { .. }) {
            self.flush();
        }
        match op {
            Op::Fill { rect, color } => {
                if self.pending != Some(*color) {
                    self.flush();
                    let [r, g, b] = rgb(*color);
                    self.content.set_fill_rgb(r, g, b);
                    self.pending = Some(*color);
                }
                self.rect(*rect);
            }
            Op::Polygon { points, color } => {
                let [r, g, b] = rgb(*color);
                self.content.set_fill_rgb(r, g, b);
                self.path(points);
                self.content.close_path();
                self.content.fill_nonzero();
            }
            Op::Stroke {
                points,
                closed,
                width,
                color,
            } => {
                let [r, g, b] = rgb(*color);
                self.content.set_stroke_rgb(r, g, b);
                self.content.set_line_width(width * PT);
                self.path(points);
                if *closed {
                    self.content.close_and_stroke();
                } else {
                    self.content.stroke();
                }
            }
            Op::Dash {
                from,
                to,
                width,
                on,
                off,
                color,
            } => {
                let [r, g, b] = rgb(*color);
                self.content.set_stroke_rgb(r, g, b);
                self.content.set_line_width(width * PT);
                self.content.set_dash_pattern([on * PT, off * PT], 0.);
                self.path(&[*from, *to]);
                self.content.stroke();
                self.content.set_dash_pattern([], 0.);
            }
            Op::Text {
                pos,
                text,
                size,
                bold,
                color,
                ..
            } => {
                let [r, g, b] = rgb(*color);
                self.content.set_fill_rgb(r, g, b);
                self.content.begin_text();
                let font = if *bold { Name(b"F2") } else { Name(b"F1") };
                self.content.set_font(font, size * PT);
                let (x, y) = (self.x(pos.x), self.y(pos.y));
                self.content.next_line(x, y);
                self.content.show(Str(&win_ansi(text)));
                self.content.end_text();
            }
        }
    }
}

fn content_stream(page: &Page) -> Vec<u8> {
    let height = page.size.y * PT;
    let mut content = Content::new();
    let mut current: Option<Rect> = None;
    {
        let mut w = Writer {
            content: &mut content,
            height,
            pending: None,
        };
        for Clipped { clip, op } in &page.ops {
            if current != Some(*clip) {
                w.flush();
                if current.is_some() {
                    w.content.restore_state();
                }
                w.content.save_state();
                w.rect(*clip);
                w.content.clip_nonzero();
                w.content.end_path();
                current = Some(*clip);
            }
            w.op(op);
        }
        w.flush();
        if current.is_some() {
            w.content.restore_state();
        }
    }
    content.finish().to_vec()
}

/// Writes the pages as a PDF document.
pub fn write(pages: &[Page], title: &str) -> Vec<u8> {
    let mut pdf = Pdf::new();
    let catalog = Ref::new(1);
    let tree = Ref::new(2);
    let regular = Ref::new(3);
    let bold = Ref::new(4);
    let info = Ref::new(5);
    let mut next = 6;
    let mut ids = vec![];
    for _ in pages {
        ids.push((Ref::new(next), Ref::new(next + 1)));
        next += 2;
    }
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree)
        .kids(ids.iter().map(|(page, _)| *page))
        .count(pages.len() as i32);
    for (page, (page_id, content_id)) in pages.iter().zip(&ids) {
        let mut p = pdf.page(*page_id);
        p.media_box(PdfRect::new(0., 0., page.size.x * PT, page.size.y * PT));
        p.parent(tree);
        p.contents(*content_id);
        let mut resources = p.resources();
        let mut fonts = resources.fonts();
        fonts.pair(Name(b"F1"), regular);
        fonts.pair(Name(b"F2"), bold);
        fonts.finish();
        resources.finish();
        p.finish();
        let raw = content_stream(page);
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);
        pdf.stream(*content_id, &packed).filter(Filter::FlateDecode);
    }
    for (id, name) in [(regular, &b"Helvetica"[..]), (bold, &b"Helvetica-Bold"[..])] {
        pdf.type1_font(id)
            .base_font(Name(name))
            .encoding_predefined(Name(b"WinAnsiEncoding"));
    }
    pdf.document_info(info)
        .title(TextStr(title))
        .producer(TextStr("OpenXER"))
        .creator(TextStr("OpenXER"));
    pdf.finish()
}

/// Structure check used by the tests: header, every xref offset pointing at
/// its object, every stream's length, the page count against the page
/// objects, and the trailer. Returns the page count.
#[cfg(test)]
pub fn check(bytes: &[u8]) -> Result<usize, String> {
    // One char per byte so string offsets are file offsets.
    let text: String = bytes
        .iter()
        .map(|&b| if b < 128 { b as char } else { '?' })
        .collect();
    if !text.starts_with("%PDF-") || !text.trim_end().ends_with("%%EOF") {
        return Err("missing header or trailer".into());
    }
    let start = text.rfind("startxref").ok_or("no startxref")?;
    let xref: usize = text[start + 9..]
        .split_whitespace()
        .next()
        .and_then(|l| l.parse().ok())
        .ok_or("bad startxref")?;
    if !text[xref..].starts_with("xref") {
        return Err("startxref does not point at the table".into());
    }
    let mut lines = text[xref..].lines().skip(1);
    let count: usize = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
        .ok_or("bad xref header")?;
    lines.next();
    for n in 1..count {
        let offset: usize = lines
            .next()
            .and_then(|l| l.split_whitespace().next()?.parse().ok())
            .ok_or("short xref")?;
        if !text[offset..].starts_with(&format!("{n} 0 obj")) {
            return Err(format!("object {n} is not at {offset}"));
        }
    }
    let mut rest = &text[..xref];
    while let Some(at) = rest.find("/Length ") {
        let chunk = &rest[at + 8..];
        let length: usize = chunk
            .split_whitespace()
            .next()
            .and_then(|v| v.parse().ok())
            .ok_or("bad length")?;
        let (_, body) = chunk.split_once("stream\n").ok_or("no stream")?;
        if !body.as_bytes()[length..].starts_with(b"\nendstream") {
            return Err("stream length is wrong".into());
        }
        rest = &body[length..];
    }
    let pages = text
        .split("/Count ")
        .nth(1)
        .and_then(|s| s.split_whitespace().next()?.parse().ok())
        .ok_or("no page count")?;
    let objects = text.matches("/Type /Page").count() - text.matches("/Type /Pages").count();
    if objects != pages {
        return Err("page count does not match the page objects".into());
    }
    Ok(pages)
}
