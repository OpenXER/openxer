//! The one reader and text builder for OpenXER's light Markdown subset,
//! shared by the help topics and the Chat window's agent replies.
//!
//! Blocks: paragraphs (lines joined until a blank line), `#` headings,
//! `-` / `*` bullets, `1.` / `1)` numbered steps, `>` notes and simple
//! `| a | b |` tables (a header row, a `|---|---|` row, then body rows).
//! Inline: `**bold**`, `*italic*` / `_italic_`, `` `code` `` and, where
//! the dialect allows it, `[text](target)` links. Anything else is shown
//! as typed.
//!
//! Text that is still arriving (a streaming reply) is read with
//! [`Dialect::streaming`]: markup left open at its end runs to the end
//! (`**bol` is bold "bol"), and a last line that may still become a list
//! marker, a heading or a table row is held back until it is complete, so
//! what shows only ever grows.
use eframe::egui::{
    self,
    text::{CCursor, LayoutJob, TextFormat},
    Color32, FontId, Pos2, Rect, Stroke, Vec2,
};

/// A run of text inside a paragraph.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Span {
    Text(String),
    Bold(String),
    Italic(String),
    /// A key, field or menu path, `like this`.
    Code(String),
    Link {
        text: String,
        target: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Block {
    /// `## Heading`
    Heading(String),
    Para(Vec<Span>),
    /// `- item`
    Bullet(Vec<Span>),
    /// `1. step`
    Step(u32, Vec<Span>),
    /// `> note`: a gotcha, drawn with a mark in the margin.
    Note(Vec<Span>),
    /// The header row, then the body rows; each cell is its spans.
    Table(Vec<Vec<Vec<Span>>>),
}

/// How a text is read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Dialect {
    /// `[text](target)` is a link (help topics); otherwise it shows as typed.
    pub links: bool,
    /// The text is still arriving; see the module comment.
    pub streaming: bool,
}

/// A block before its inline markup is read.
enum Raw {
    Heading(String),
    Para(String),
    Bullet(String),
    Step(u32, String),
    Note(String),
    Table(Vec<Vec<String>>),
}

/// The blocks of `source`.
pub(crate) fn blocks(source: &str, dialect: Dialect) -> Vec<Block> {
    let mut lines: Vec<&str> = source
        .split('\n')
        .map(|l| l.trim_end_matches('\r'))
        .collect();
    if source.ends_with('\n') {
        lines.pop();
    }
    if dialect.streaming && !source.ends_with('\n') {
        if let Some(last) = lines.last() {
            if held_back(last) {
                lines.pop();
            }
        }
    }
    if dialect.streaming {
        // Table lines at the end without their `|---|` row yet may still
        // become a table.
        let tail = lines
            .iter()
            .rev()
            .take_while(|l| l.trim_start().starts_with('|'))
            .count();
        if tail > 0 && !is_table(&lines[lines.len() - tail..]) {
            lines.truncate(lines.len() - tail);
        }
    }
    let mut raw: Vec<Raw> = vec![];
    let mut para: Vec<&str> = vec![];
    let mut open: Option<Raw> = None;
    fn flush(para: &mut Vec<&str>, raw: &mut Vec<Raw>) {
        if !para.is_empty() {
            raw.push(Raw::Para(para.join(" ")));
            para.clear();
        }
    }
    fn close(open: &mut Option<Raw>, raw: &mut Vec<Raw>) {
        if let Some(b) = open.take() {
            raw.push(b);
        }
    }
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim_end();
        let trimmed = line.trim_start();
        i += 1;
        if trimmed.is_empty() {
            flush(&mut para, &mut raw);
            close(&mut open, &mut raw);
            continue;
        }
        let indented = line.starts_with("  ");
        if indented && !trimmed.starts_with('|') {
            if let Some(Raw::Bullet(s) | Raw::Step(_, s) | Raw::Note(s)) = open.as_mut() {
                s.push(' ');
                s.push_str(trimmed);
                continue;
            }
        }
        if trimmed.starts_with('|') {
            let start = i - 1;
            let mut end = start;
            while end < lines.len() && lines[end].trim_start().starts_with('|') {
                end += 1;
            }
            if is_table(&lines[start..end]) {
                flush(&mut para, &mut raw);
                close(&mut open, &mut raw);
                let rows: Vec<Vec<String>> = lines[start..end]
                    .iter()
                    .enumerate()
                    .filter(|(k, _)| *k != 1)
                    .map(|(_, l)| cells(l))
                    .collect();
                raw.push(Raw::Table(rows));
                i = end;
                continue;
            }
        }
        if let Some(h) = heading(trimmed) {
            flush(&mut para, &mut raw);
            close(&mut open, &mut raw);
            raw.push(Raw::Heading(h.to_owned()));
        } else if let Some(b) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            flush(&mut para, &mut raw);
            close(&mut open, &mut raw);
            open = Some(Raw::Bullet(b.trim().to_owned()));
        } else if let Some(n) = trimmed.strip_prefix("> ") {
            flush(&mut para, &mut raw);
            close(&mut open, &mut raw);
            open = Some(Raw::Note(n.trim().to_owned()));
        } else if let Some((n, rest)) = step_number(trimmed) {
            flush(&mut para, &mut raw);
            close(&mut open, &mut raw);
            open = Some(Raw::Step(n, rest.to_owned()));
        } else {
            close(&mut open, &mut raw);
            para.push(trimmed);
        }
    }
    flush(&mut para, &mut raw);
    close(&mut open, &mut raw);
    let count = raw.len();
    raw.into_iter()
        .enumerate()
        .map(|(k, b)| {
            // Only the last block can still be growing.
            let open_end = dialect.streaming && k + 1 == count;
            let sp = |s: &str| spans_with(s, dialect.links, open_end);
            match b {
                Raw::Heading(h) => Block::Heading(span_text(&sp(&h))),
                Raw::Para(s) => Block::Para(sp(&s)),
                Raw::Bullet(s) => Block::Bullet(sp(&s)),
                Raw::Step(n, s) => Block::Step(n, sp(&s)),
                Raw::Note(s) => Block::Note(sp(&s)),
                Raw::Table(rows) => Block::Table(
                    rows.iter()
                        .map(|r| {
                            r.iter()
                                .map(|c| spans_with(c, dialect.links, false))
                                .collect()
                        })
                        .collect(),
                ),
            }
        })
        .collect()
}

/// A last line still being written that may yet turn into markup of its
/// own: a list marker, a heading marker, a note marker or a table row.
fn held_back(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() {
        return false;
    }
    let digits = t.trim_end_matches(['.', ')']);
    t.chars().all(|c| c == '#')
        || matches!(t, "-" | "*" | ">")
        || t.starts_with('|')
        || (!digits.is_empty()
            && digits.chars().all(|c| c.is_ascii_digit())
            && t.len() <= digits.len() + 1)
}

/// `# Heading` (any level): its text.
fn heading(line: &str) -> Option<&str> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    line[hashes..].strip_prefix(' ').map(str::trim)
}

fn step_number(line: &str) -> Option<(u32, &str)> {
    let end = line.find(|c: char| !c.is_ascii_digit())?;
    if end == 0 {
        return None;
    }
    let rest = &line[end..];
    let rest = rest
        .strip_prefix(". ")
        .or_else(|| rest.strip_prefix(") "))?;
    Some((line[..end].parse().ok()?, rest.trim()))
}

/// The cells of a table line, without the outer pipes.
fn cells(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| c.trim().to_owned()).collect()
}

/// Lines that make a table: a header row and a `|---|` row of as many cells.
fn is_table(lines: &[&str]) -> bool {
    if lines.len() < 2 {
        return false;
    }
    let rule = cells(lines[1]);
    rule.len() == cells(lines[0]).len()
        && rule.iter().all(|c| {
            let c = c.trim_start_matches(':').trim_end_matches(':');
            !c.is_empty() && c.chars().all(|ch| ch == '-')
        })
}

/// Inline markup of one paragraph, `[text](target)` read as a link.
#[cfg(test)]
pub(crate) fn spans(s: &str) -> Vec<Span> {
    spans_with(s, true, false)
}

/// Inline markup; `open_end`: markup left open runs to the end.
pub(crate) fn spans_with(s: &str, links: bool, open_end: bool) -> Vec<Span> {
    let mut out = vec![];
    let mut text = String::new();
    let mut rest = s;
    let mut prev: Option<char> = None;
    fn push_text(text: &mut String, out: &mut Vec<Span>) {
        if !text.is_empty() {
            out.push(Span::Text(std::mem::take(text)));
        }
    }
    fn push(out: &mut Vec<Span>, span: Span) {
        let empty =
            matches!(&span, Span::Bold(t) | Span::Italic(t) | Span::Code(t) if t.is_empty());
        if !empty {
            out.push(span);
        }
    }
    while let Some(c) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix("**") {
            if let Some(end) = after.find("**").filter(|e| *e > 0) {
                push_text(&mut text, &mut out);
                push(&mut out, Span::Bold(after[..end].to_owned()));
                prev = Some('*');
                rest = &after[end + 2..];
                continue;
            }
            if open_end && !after.contains("**") {
                push_text(&mut text, &mut out);
                push(&mut out, Span::Bold(after.trim_end_matches('*').to_owned()));
                rest = "";
                continue;
            }
        }
        if c == '*' || c == '_' {
            let after = &rest[1..];
            let next = after.chars().next();
            let opens = next.is_some_and(|n| !n.is_whitespace() && n != c)
                && !(c == '_' && prev.is_some_and(char::is_alphanumeric));
            if opens {
                if let Some(end) = closer(after, c) {
                    push_text(&mut text, &mut out);
                    push(&mut out, Span::Italic(after[..end].to_owned()));
                    prev = Some(c);
                    rest = &after[end + 1..];
                    continue;
                }
                if open_end {
                    push_text(&mut text, &mut out);
                    push(&mut out, Span::Italic(after.trim_end_matches(c).to_owned()));
                    rest = "";
                    continue;
                }
            } else if open_end
                && next.is_none()
                && !(c == '_' && prev.is_some_and(char::is_alphanumeric))
            {
                // A lone marker at the very end may still open something.
                rest = "";
                continue;
            }
        }
        if let Some(after) = rest.strip_prefix('`') {
            if let Some(end) = after.find('`') {
                push_text(&mut text, &mut out);
                push(&mut out, Span::Code(after[..end].to_owned()));
                prev = Some('`');
                rest = &after[end + 1..];
                continue;
            }
            if open_end {
                push_text(&mut text, &mut out);
                push(&mut out, Span::Code(after.to_owned()));
                rest = "";
                continue;
            }
        }
        if links {
            if let Some(after) = rest.strip_prefix('[') {
                if let Some(mid) = after.find("](") {
                    if let Some(end) = after[mid + 2..].find(')') {
                        push_text(&mut text, &mut out);
                        out.push(Span::Link {
                            text: after[..mid].to_owned(),
                            target: after[mid + 2..mid + 2 + end].to_owned(),
                        });
                        prev = Some(')');
                        rest = &after[mid + 2 + end + 1..];
                        continue;
                    }
                }
            }
        }
        text.push(c);
        prev = Some(c);
        rest = &rest[c.len_utf8()..];
    }
    push_text(&mut text, &mut out);
    out
}

/// Where the italic run that `after` starts ends: the next `c` that follows
/// a non-space and, for `_`, is not followed by a letter or digit (and for
/// `*`, is not half of a `**`).
fn closer(after: &str, c: char) -> Option<usize> {
    let mut last: Option<char> = None;
    for (i, ch) in after.char_indices() {
        if ch == c && i > 0 && last.is_some_and(|l| !l.is_whitespace()) {
            let next = after[i + 1..].chars().next();
            let ok = match c {
                '_' => !next.is_some_and(char::is_alphanumeric),
                _ => next != Some('*'),
            };
            if ok {
                return Some(i);
            }
        }
        last = Some(ch);
    }
    None
}

pub(crate) fn span_text(spans: &[Span]) -> String {
    spans
        .iter()
        .map(|s| match s {
            Span::Text(t) | Span::Bold(t) | Span::Italic(t) | Span::Code(t) => t.as_str(),
            Span::Link { text, .. } => text.as_str(),
        })
        .collect()
}

pub(crate) fn block_text(b: &Block) -> String {
    match b {
        Block::Heading(h) => h.clone(),
        Block::Para(s) | Block::Bullet(s) | Block::Step(_, s) | Block::Note(s) => span_text(s),
        Block::Table(rows) => rows
            .iter()
            .map(|r| {
                r.iter()
                    .map(|c| span_text(c))
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// The text of every block, one block per line (marks left out).
#[cfg(test)]
pub(crate) fn plain_text(blocks: &[Block]) -> String {
    blocks.iter().map(block_text).collect::<Vec<_>>().join("\n")
}

// ----------------------------------------------------------- laying out

/// The fonts and colors spans are laid out in.
#[derive(Clone)]
pub(crate) struct Style {
    pub regular: FontId,
    pub bold: FontId,
    pub ink: Color32,
    /// Font and color of `code`.
    pub code: (FontId, Color32),
    /// Fill behind `code` (transparent for none).
    pub code_fill: Color32,
    pub link: Color32,
}

/// Spans laid out as one wrapped text, with the character ranges of its
/// links and their targets.
pub(crate) struct Laid {
    pub job: LayoutJob,
    pub links: Vec<(usize, usize, String)>,
}

/// `spans` as a text wrapped at `width`.
pub(crate) fn layout(spans: &[Span], style: &Style, width: f32) -> Laid {
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    let mut links = vec![];
    let mut chars = 0;
    for span in spans {
        let (text, format) = match span {
            Span::Text(t) => (
                t.as_str(),
                TextFormat::simple(style.regular.clone(), style.ink),
            ),
            Span::Bold(t) => (
                t.as_str(),
                TextFormat::simple(style.bold.clone(), style.ink),
            ),
            Span::Italic(t) => {
                let mut f = TextFormat::simple(style.regular.clone(), style.ink);
                f.italics = true;
                (t.as_str(), f)
            }
            Span::Code(t) => {
                let mut f = TextFormat::simple(style.code.0.clone(), style.code.1);
                f.background = style.code_fill;
                (t.as_str(), f)
            }
            Span::Link { text, target } => {
                let n = text.chars().count();
                links.push((chars, chars + n, target.clone()));
                let mut f = TextFormat::simple(style.regular.clone(), style.link);
                f.underline = Stroke::new(1_f32, style.link);
                (text.as_str(), f)
            }
        };
        chars += text.chars().count();
        job.append(text, 0., format);
    }
    Laid { job, links }
}

// ------------------------------------------------------------------ tables

/// Space between a table cell's lines and its text (left and right, top
/// and bottom).
pub(crate) const CELL_PAD: Vec2 = Vec2::new(4., 2.);
/// The narrowest a column is squeezed to before the table scrolls sideways
/// (a column whose text is narrower keeps its own width).
pub(crate) const MIN_COLUMN: f32 = 48.;

/// One laid-out table cell: its rectangle (lines included) and its wrapped
/// text, placed at `pos`, both relative to the table's top left.
pub(crate) struct TableCell {
    pub rect: Rect,
    pub pos: Pos2,
    pub galley: std::sync::Arc<egui::Galley>,
    pub links: Vec<(usize, usize, String)>,
}

/// A Markdown table laid out for a width: column widths, row heights and
/// each cell's wrapped text. `size` is wider than the width given only
/// when the columns cannot shrink to [`MIN_COLUMN`] and fit.
pub(crate) struct TableLaid {
    pub columns: Vec<f32>,
    pub rows: Vec<f32>,
    pub cells: Vec<Vec<TableCell>>,
    pub size: Vec2,
}

/// Column widths for a room: the natural widths when they fit; otherwise
/// the room shared in proportion to them, no column below [`MIN_COLUMN`]
/// (or its natural width, when that is less). Whole pixels.
pub(crate) fn column_widths(natural: &[f32], room: f32) -> Vec<f32> {
    let total: f32 = natural.iter().sum();
    if total <= room {
        return natural.iter().map(|w| w.ceil()).collect();
    }
    let floor = |w: f32| w.min(MIN_COLUMN);
    let mut fixed: Vec<Option<f32>> = vec![None; natural.len()];
    loop {
        let taken: f32 = fixed.iter().flatten().sum();
        let free: f32 = natural
            .iter()
            .zip(&fixed)
            .filter(|(_, f)| f.is_none())
            .map(|(w, _)| *w)
            .sum();
        let scale = ((room - taken) / free.max(1.)).max(0.);
        let mut changed = false;
        for (k, w) in natural.iter().enumerate() {
            if fixed[k].is_none() && w * scale < floor(*w) {
                fixed[k] = Some(floor(*w));
                changed = true;
            }
        }
        if !changed {
            return natural
                .iter()
                .zip(&fixed)
                .map(|(w, f)| f.unwrap_or((w * scale).floor()).max(1.))
                .collect();
        }
    }
}

/// Lay out a table (`rows[0]` is the header, drawn bold) for `width`:
/// columns sized by [`column_widths`], each cell's text wrapped inside its
/// column (a word too long for the column breaks between letters), each
/// row as tall as its tallest cell and never under the kit's 18 px row.
pub(crate) fn table_layout(
    ctx: &egui::Context,
    rows: &[Vec<Vec<Span>>],
    style: &Style,
    width: f32,
) -> TableLaid {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0).max(1);
    let header = Style {
        regular: style.bold.clone(),
        ..style.clone()
    };
    let style_of = |r: usize| if r == 0 { &header } else { style };
    let galley = |job: LayoutJob| ctx.fonts_mut(|f| f.layout_job(job));
    let mut natural = vec![2. * CELL_PAD.x; columns];
    for (r, row) in rows.iter().enumerate() {
        for (k, cell) in row.iter().enumerate() {
            let laid = layout(cell, style_of(r), f32::INFINITY);
            let w = galley(laid.job).size().x + 2. * CELL_PAD.x;
            natural[k] = natural[k].max(w);
        }
    }
    let widths = column_widths(&natural, width);
    let mut cells = vec![];
    let mut heights = vec![];
    let mut top = 0.;
    for (r, row) in rows.iter().enumerate() {
        let mut left = 0.;
        let mut laid_row = vec![];
        let mut height = crate::dialog_kit::ROW;
        for (k, w) in widths.iter().enumerate() {
            let empty = vec![];
            let spans = row.get(k).unwrap_or(&empty);
            let Laid { job, links } = layout(spans, style_of(r), w - 2. * CELL_PAD.x);
            let g = galley(job);
            height = height.max((g.size().y + 2. * CELL_PAD.y).ceil());
            laid_row.push(TableCell {
                rect: Rect::from_min_size(Pos2::new(left, top), Vec2::new(*w, 0.)),
                pos: Pos2::new(left + CELL_PAD.x, top + CELL_PAD.y),
                galley: g,
                links,
            });
            left += w;
        }
        for cell in &mut laid_row {
            cell.rect.set_height(height);
        }
        cells.push(laid_row);
        heights.push(height);
        top += height;
    }
    let size = Vec2::new(widths.iter().sum::<f32>(), top);
    TableLaid {
        columns: widths,
        rows: heights,
        cells,
        size,
    }
}

/// Paint a laid-out table with its top left at `origin`: the header row on
/// the dialog face, the kit list's grid lines around every cell, and each
/// cell's text clipped to its cell.
pub(crate) fn paint_table(painter: &egui::Painter, origin: Pos2, laid: &TableLaid, ink: Color32) {
    let line = crate::dialog_kit::ROW_LINE;
    for (r, row) in laid.cells.iter().enumerate() {
        for cell in row {
            let rect = cell.rect.translate(origin.to_vec2());
            if r == 0 {
                painter.rect_filled(rect, 0., crate::chrome::pal::FACE);
            }
            painter
                .with_clip_rect(painter.clip_rect().intersect(rect))
                .galley(origin + cell.pos.to_vec2(), cell.galley.clone(), ink);
        }
    }
    // The grid: a line at the left and top, and at the right and bottom
    // edge of every column and row (the last pixel inside the cell).
    let (w, h) = (laid.size.x, laid.size.y);
    let mut lines = vec![
        Rect::from_min_size(origin, Vec2::new(w, 1.)),
        Rect::from_min_size(origin, Vec2::new(1., h)),
    ];
    let mut x = origin.x;
    for width in &laid.columns {
        x += width;
        let at = Pos2::new(x - 1., origin.y);
        lines.push(Rect::from_min_size(at, Vec2::new(1., h)));
    }
    let mut y = origin.y;
    for height in &laid.rows {
        y += height;
        let at = Pos2::new(origin.x, y - 1.);
        lines.push(Rect::from_min_size(at, Vec2::new(w, 1.)));
    }
    for rect in lines {
        painter.rect_filled(rect, 0., line);
    }
}

/// What a click on a table did, and where its links are.
#[derive(Default)]
pub(crate) struct TableOut {
    /// The target of the link clicked.
    pub clicked: Option<String>,
    /// Each link's target and rectangle on screen.
    pub links: Vec<(String, Rect)>,
}

/// A Markdown table in a flow layout `width` wide: laid out by
/// [`table_layout`], painted by [`paint_table`], with a sideways scroll
/// only when its columns cannot shrink enough to fit.
pub(crate) fn table_ui(
    ui: &mut egui::Ui,
    id: egui::Id,
    rows: &[Vec<Vec<Span>>],
    style: &Style,
    width: f32,
) -> TableOut {
    let laid = table_layout(ui.ctx(), rows, style, width);
    let mut out = TableOut::default();
    let mut draw = |ui: &mut egui::Ui| {
        let (rect, response) = ui.allocate_exact_size(laid.size, egui::Sense::click());
        paint_table(ui.painter(), rect.min, &laid, style.ink);
        for cell in laid.cells.iter().flatten() {
            if cell.links.is_empty() {
                continue;
            }
            let pos = rect.min + cell.pos.to_vec2();
            out.links.extend(link_rects(&cell.galley, pos, &cell.links));
            let hit = |p: Pos2| link_at(&cell.galley, pos, &cell.links, p);
            if response.hover_pos().and_then(hit).is_some() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if response.clicked() {
                if let Some(target) = response.interact_pointer_pos().and_then(hit) {
                    out.clicked = Some(target.to_owned());
                }
            }
        }
    };
    if laid.size.x > width {
        egui::ScrollArea::horizontal()
            .id_salt(id)
            .max_width(width)
            .show(ui, &mut draw);
    } else {
        draw(ui);
    }
    out
}

/// The link target under `p` of a galley drawn at `pos`.
pub(crate) fn link_at<'a>(
    galley: &egui::Galley,
    pos: Pos2,
    links: &'a [(usize, usize, String)],
    p: Pos2,
) -> Option<&'a str> {
    if !galley.rect.translate(pos.to_vec2()).contains(p) {
        return None;
    }
    let at = galley.cursor_from_pos(p - pos).index;
    links
        .iter()
        .find(|(a, b, _)| at >= *a && at < *b)
        .map(|l| l.2.as_str())
}

/// Each link's rectangle (the part on its first row) in a galley drawn at
/// `pos`.
pub(crate) fn link_rects(
    galley: &egui::Galley,
    pos: Pos2,
    links: &[(usize, usize, String)],
) -> Vec<(String, Rect)> {
    links
        .iter()
        .map(|(a, b, target)| {
            let start = galley.pos_from_cursor(CCursor::new(*a));
            let end = galley.pos_from_cursor(CCursor::new(*b));
            let right = if (end.top() - start.top()).abs() < 1. {
                end.left()
            } else {
                galley.rect.right()
            };
            let r = Rect::from_min_max(
                pos + start.min.to_vec2(),
                pos + Vec2::new(right.max(start.left() + 2.), start.bottom()),
            );
            (target.clone(), r)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chat(s: &str) -> Vec<Block> {
        blocks(s, Dialect::default())
    }

    fn stream(s: &str) -> Vec<Block> {
        blocks(
            s,
            Dialect {
                links: false,
                streaming: true,
            },
        )
    }

    #[test]
    fn the_subset_reads_paragraphs_lists_tables_and_inline_markup() {
        let b = chat("First line\nsame paragraph **bold** and *it* and _it2_ and `code`.\n\n- one\n* two\n1. step\n2) next\n\n| ID | Float |\n|---|--:|\n| A1 | 0d |\n\n## Head\n> watch");
        assert_eq!(
            b[0],
            Block::Para(vec![
                Span::Text("First line same paragraph ".into()),
                Span::Bold("bold".into()),
                Span::Text(" and ".into()),
                Span::Italic("it".into()),
                Span::Text(" and ".into()),
                Span::Italic("it2".into()),
                Span::Text(" and ".into()),
                Span::Code("code".into()),
                Span::Text(".".into()),
            ])
        );
        assert_eq!(b[1], Block::Bullet(vec![Span::Text("one".into())]));
        assert_eq!(b[2], Block::Bullet(vec![Span::Text("two".into())]));
        assert_eq!(b[3], Block::Step(1, vec![Span::Text("step".into())]));
        assert_eq!(b[4], Block::Step(2, vec![Span::Text("next".into())]));
        assert_eq!(block_text(&b[5]), "ID\tFloat\nA1\t0d");
        assert_eq!(b[6], Block::Heading("Head".into()));
        assert_eq!(b[7], Block::Note(vec![Span::Text("watch".into())]));
    }

    #[test]
    fn unknown_or_unfinished_markup_shows_as_typed() {
        // Snake case names, a product sign, an unclosed run, a link in chat.
        assert_eq!(
            plain_text(&chat("set_activity_code and 3 * 4 and **open and [x](y)")),
            "set_activity_code and 3 * 4 and **open and [x](y)"
        );
        // A table line with no rule row is a paragraph.
        assert_eq!(plain_text(&chat("| a | b |")), "| a | b |");
        // Help links still read.
        assert_eq!(
            spans("see [Topic](topic-id)"),
            vec![
                Span::Text("see ".into()),
                Span::Link {
                    text: "Topic".into(),
                    target: "topic-id".into()
                }
            ]
        );
    }

    /// A reply arriving one character at a time never panics, never shows
    /// a stray marker, and what shows only ever grows towards the final text.
    #[test]
    fn every_split_point_of_a_streaming_reply_shows_a_prefix_of_the_final_text() {
        let reply = "**PIP-3110** is on the *longest path*; it drives `INS-5020`.\n\nWhy:\n\n1. The rack piping finishes 07-Apr-26.\n2. Wiring and loop checks follow with _no slack_.\n- PIP-3170 has 13d of float\n* set_activity is the tool\n\n| Activity | Float |\n|---|---|\n| PIP-3110 | 0d |\n| PIP-3170 | 13d |\n\n## Next\n> Apply nothing yet.\nDone.";
        let full = plain_text(&chat(reply));
        let boundaries: Vec<usize> = reply
            .char_indices()
            .map(|(i, _)| i)
            .chain([reply.len()])
            .collect();
        for &at in &boundaries {
            let part = plain_text(&stream(&reply[..at]));
            let shown: String = part.split_whitespace().collect::<Vec<_>>().join(" ");
            let whole: String = full.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(
                whole.starts_with(&shown),
                "at {at} ({:?}): {shown:?} is not the start of the final text",
                &reply[..at]
            );
            for marker in ["**", "`", "|"] {
                assert!(
                    !part.contains(marker),
                    "at {at}: marker {marker:?} shows in {part:?}"
                );
            }
        }
        assert_eq!(plain_text(&stream(reply)), full);
        // Bold left open runs to the end while streaming.
        assert_eq!(
            stream("a **bol"),
            vec![Block::Para(vec![
                Span::Text("a ".into()),
                Span::Bold("bol".into())
            ])]
        );
    }

    /// The rows of the one table in `source`.
    fn table_rows(source: &str) -> Vec<Vec<Vec<Span>>> {
        match chat(source).into_iter().next() {
            Some(Block::Table(rows)) => rows,
            other => panic!("not a table: {other:?}"),
        }
    }

    /// Tables of a reply at three widths: every cell's text lies inside its
    /// cell, and inside the message area unless the table has too many
    /// columns to fit even at the narrowest column width (then, and only
    /// then, it scrolls sideways).
    #[test]
    fn table_text_stays_inside_its_cell_and_the_message_area() {
        let ctx = egui::Context::default();
        crate::chrome::configure(&ctx);
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let style = Style {
            regular: crate::dialog_kit::body_font(),
            bold: crate::chrome::bold_font(),
            ink: Color32::BLACK,
            code: (crate::dialog_kit::body_font(), Color32::BLACK),
            code_fill: Color32::TRANSPARENT,
            link: Color32::BLUE,
        };
        let long = "The rack piping finishes before the wiring crew arrives, \
            so the loop checks can start on the Monday after";
        let short = "| ID | Float |\n|---|---|\n| PIP-3110 | 0d |\n| PIP-3170 | 13d |";
        let wordy = format!(
            "| Activity | Why it matters | What to do |\n|---|---|---|\n\
            | PIP-3110 | {long} | {long} |\n| INS-5020 | Short | {long} |"
        );
        let path = r"C:\Schedules\Riverbend\Exports\RIVERBEND-2026-11-24-final-issued-v3.xer";
        let number = "123456789012345678901234567890";
        let token = format!("| File | Total |\n|---|---|\n| {path} | {number} |");
        let titles: Vec<String> = (1..=12).map(|k| format!("Column {k}")).collect();
        let values: Vec<String> = (1..=12).map(|k| format!("value {k}")).collect();
        let many = format!(
            "| {} |\n|{}|\n| {} |",
            titles.join(" | "),
            ["---"; 12].join("|"),
            values.join(" | ")
        );
        for source in [short, wordy.as_str(), token.as_str(), many.as_str()] {
            let rows = table_rows(source);
            let columns = rows[0].len();
            for width in [250., 400., 700.] {
                let laid = table_layout(&ctx, &rows, &style, width);
                let fits = columns as f32 * MIN_COLUMN <= width;
                if fits {
                    assert!(laid.size.x <= width, "{width}: {} wide", laid.size.x);
                } else {
                    assert!(laid.columns.iter().all(|w| *w >= MIN_COLUMN));
                }
                let area = Rect::from_min_size(Pos2::ZERO, Vec2::new(width, laid.size.y));
                for (r, row) in laid.cells.iter().enumerate() {
                    for (k, cell) in row.iter().enumerate() {
                        let text = Rect::from_min_size(cell.pos, cell.galley.size());
                        let at = format!("{width} px, row {r}, column {k} of {source:?}");
                        let inside = cell.rect.expand(0.5).contains_rect(text);
                        assert!(inside, "{at}: off its cell");
                        let tall = text.height() + 2. * CELL_PAD.y;
                        assert!(tall <= laid.rows[r] + 0.5, "{at}: the row is too short");
                        if fits {
                            let shown = area.expand(0.5).contains_rect(text);
                            assert!(shown, "{at}: off the area");
                        }
                    }
                }
            }
        }
        // Short columns keep their natural width, one line each.
        let laid = table_layout(&ctx, &table_rows(short), &style, 700.);
        assert!(laid.size.x < 200., "natural width: {}", laid.size.x);
        let row = crate::dialog_kit::ROW;
        for cell in laid.cells.iter().flatten() {
            assert_eq!(cell.galley.rows.len(), 1);
        }
        assert!(laid.rows.iter().all(|h| *h == row));
        // Long text wraps and its row grows.
        let laid = table_layout(&ctx, &table_rows(&wordy), &style, 250.);
        assert!(laid.rows[1] > row * 2., "{:?}", laid.rows);
        // A long path or number breaks inside its cell.
        let laid = table_layout(&ctx, &table_rows(&token), &style, 250.);
        assert!(laid.cells[1].iter().all(|c| c.galley.rows.len() > 1));
        // Many columns scroll sideways only when they cannot fit.
        let many = table_rows(&many);
        assert!(table_layout(&ctx, &many, &style, 250.).size.x > 250.);
        assert!(table_layout(&ctx, &many, &style, 700.).size.x <= 700.);
    }
}
