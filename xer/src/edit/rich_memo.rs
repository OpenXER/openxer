//! Notebook text with its formatting. A notebook topic, a step description
//! and the other memo columns are stored as HTML in the file. This module
//! reads the part of that HTML OpenXER can show and edit faithfully (bold,
//! italic and underlined text, bulleted and numbered lists, line breaks and
//! paragraphs) into a plain text with one style per character and one kind
//! per line, and writes it back as HTML of the same subset. A memo that is
//! not HTML is read as plain text, one line per line break.
//!
//! What is kept: `b`/`strong`, `i`/`em`, `u`, `ul`, `ol`, `li`, `br`, `p`,
//! `div`, and the text inside `span` and `font` (their attributes, such as a
//! face or a colour, are not kept). A memo holding anything else (a picture,
//! a table, a hyperlink, a heading) is not [`editable`]: rewriting it would
//! lose that content.

/// The character formatting of one character.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

/// One of the character formats a command switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Bold,
    Italic,
    Underline,
}

impl Style {
    pub fn has(self, f: Format) -> bool {
        match f {
            Format::Bold => self.bold,
            Format::Italic => self.italic,
            Format::Underline => self.underline,
        }
    }
    pub fn with(mut self, f: Format, on: bool) -> Self {
        match f {
            Format::Bold => self.bold = on,
            Format::Italic => self.italic = on,
            Format::Underline => self.underline = on,
        }
        self
    }
}

/// What a line is: plain text or an item of a bulleted or numbered list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineKind {
    #[default]
    Plain,
    Bullet,
    Numbered,
}

/// Formatted memo text: `text` with one [`Style`] per character (line breaks
/// included) and one [`LineKind`] per line (`text` holds one more line than
/// it has `\n` characters).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RichMemo {
    pub text: String,
    pub styles: Vec<Style>,
    pub lines: Vec<LineKind>,
}

impl Default for RichMemo {
    fn default() -> Self {
        Self::plain("")
    }
}

/// Tags whose content is kept and whose markup the editor can rewrite.
const KEPT: &[&str] = &[
    "html", "body", "head", "title", "style", "meta", "p", "div", "br", "b", "strong", "i", "em",
    "u", "ul", "ol", "li", "span", "font",
];
/// Tags whose text is not part of the note.
const HIDDEN: &[&str] = &["head", "title", "style", "script"];

/// Whether `stored` can be edited without losing content: plain text, or
/// HTML that uses only the tags this module keeps.
pub fn editable(stored: &str) -> bool {
    if !super::records::looks_like_html(stored) {
        return true;
    }
    tags(stored).all(|(name, _)| name.starts_with('!') || KEPT.contains(&name.as_str()))
}

/// The lower-case names of the tags in `html`, with whether each closes.
fn tags(html: &str) -> impl Iterator<Item = (String, bool)> + '_ {
    let mut rest = html;
    std::iter::from_fn(move || {
        let start = rest.find('<')?;
        let after = &rest[start + 1..];
        let end = after.find('>').unwrap_or(after.len());
        let tag = &after[..end];
        rest = after.get(end + 1..).unwrap_or("");
        Some(tag_name(tag))
    })
}

fn tag_name(tag: &str) -> (String, bool) {
    let closing = tag.trim_start().starts_with('/');
    let name = tag
        .trim_start()
        .trim_start_matches('/')
        .split(|c: char| c.is_whitespace() || c == '/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    (name, closing)
}

fn decode_entity(name: &str) -> Option<char> {
    match name {
        "nbsp" => Some(' '),
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            match char::from_u32(code)? {
                '\u{a0}' => Some(' '),
                c => Some(c),
            }
        }
    }
}

/// The reader's state while it walks the HTML.
struct Reader {
    memo: RichMemo,
    /// Characters on the line being read.
    line_len: usize,
    /// The last character read collapses with following white space.
    soft_space: bool,
    kind: LineKind,
    lists: Vec<LineKind>,
    /// A list item is open and has not ended a line yet.
    item_open: bool,
    bold: usize,
    italic: usize,
    underline: usize,
    hidden: usize,
}

impl Reader {
    fn style(&self) -> Style {
        Style {
            bold: self.bold > 0,
            italic: self.italic > 0,
            underline: self.underline > 0,
        }
    }
    fn push(&mut self, c: char, soft: bool) {
        self.memo.text.push(c);
        self.memo.styles.push(self.style());
        self.line_len += 1;
        self.soft_space = soft;
    }
    fn white(&mut self) {
        if self.line_len > 0 && !self.soft_space {
            self.push(' ', true);
        }
    }
    fn end_line(&mut self) {
        self.memo.text.push('\n');
        self.memo.styles.push(Style::default());
        self.memo.lines.push(self.kind);
        self.line_len = 0;
        self.soft_space = false;
        self.item_open = false;
    }
    fn end_open_line(&mut self) {
        if self.line_len > 0 {
            self.end_line();
        }
    }
    fn tag(&mut self, name: &str, closing: bool) {
        let count = |n: &mut usize| {
            if closing {
                *n = n.saturating_sub(1);
            } else {
                *n += 1;
            }
        };
        match name {
            "br" => self.end_line(),
            "p" | "div" => self.end_open_line(),
            "b" | "strong" => count(&mut self.bold),
            "i" | "em" => count(&mut self.italic),
            "u" => count(&mut self.underline),
            "ul" | "ol" => {
                self.end_open_line();
                if closing {
                    self.lists.pop();
                    self.kind = LineKind::Plain;
                } else {
                    self.lists.push(if name == "ul" {
                        LineKind::Bullet
                    } else {
                        LineKind::Numbered
                    });
                }
            }
            "li" if closing => {
                if self.line_len > 0 || self.item_open {
                    self.end_line();
                }
                self.kind = LineKind::Plain;
            }
            "li" => {
                self.end_open_line();
                self.kind = self.lists.last().copied().unwrap_or(LineKind::Bullet);
                self.item_open = true;
            }
            n if HIDDEN.contains(&n) => count(&mut self.hidden),
            _ => {}
        }
    }
    fn finish(mut self) -> RichMemo {
        if self.line_len == 0 && !self.memo.lines.is_empty() {
            // A break or a block that ends the memo adds no empty line.
            self.memo.text.pop();
            self.memo.styles.pop();
        } else {
            self.memo.lines.push(self.kind);
        }
        self.memo
    }
}

impl RichMemo {
    /// Unformatted text, one line per `\n`.
    pub fn plain(text: &str) -> Self {
        let text: String = text.replace('\t', " ");
        let lines = text.matches('\n').count() + 1;
        Self {
            styles: vec![Style::default(); text.chars().count()],
            lines: vec![LineKind::Plain; lines],
            text,
        }
    }

    /// Read a memo as stored: HTML, or plain text with `\r\n`, `\n` or the
    /// file's `0x7f` line breaks.
    pub fn parse(stored: &str) -> Self {
        if !super::records::looks_like_html(stored) {
            let text = stored
                .replace("\r\n", "\n")
                .replace("\u{7f}\u{7f}", "\n")
                .replace(['\r', '\u{7f}'], "\n");
            return Self::plain(&text);
        }
        let mut r = Reader {
            memo: RichMemo {
                text: String::new(),
                styles: vec![],
                lines: vec![],
            },
            line_len: 0,
            soft_space: false,
            kind: LineKind::Plain,
            lists: vec![],
            item_open: false,
            bold: 0,
            italic: 0,
            underline: 0,
            hidden: 0,
        };
        let mut chars = stored.char_indices().peekable();
        while let Some((at, c)) = chars.next() {
            match c {
                '<' => {
                    let rest = &stored[at + 1..];
                    let end = rest.find('>').unwrap_or(rest.len());
                    let (name, closing) = tag_name(&rest[..end]);
                    let stop = at + 1 + end;
                    while chars.peek().is_some_and(|(i, _)| *i <= stop) {
                        chars.next();
                    }
                    if !name.starts_with('!') {
                        r.tag(&name, closing);
                    }
                }
                _ if r.hidden > 0 => {}
                '&' => {
                    let rest = &stored[at + 1..];
                    let entity = rest
                        .find(';')
                        .filter(|&n| n <= 10)
                        .and_then(|n| decode_entity(&rest[..n]).map(|c| (n, c)));
                    match entity {
                        Some((n, decoded)) => {
                            let stop = at + 1 + n;
                            while chars.peek().is_some_and(|(i, _)| *i <= stop) {
                                chars.next();
                            }
                            r.push(decoded, false);
                        }
                        None => r.push('&', false),
                    }
                }
                ' ' | '\t' | '\n' | '\r' | '\u{7f}' => r.white(),
                c => r.push(c, false),
            }
        }
        r.finish()
    }

    /// The text without formatting.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The memo as stored: HTML of the kept subset. Plain lines are joined
    /// by `<br>`, list items sit in `<ul>` or `<ol>`, and a space that HTML
    /// would fold away (at a line start or after another space) is written
    /// `&nbsp;`.
    pub fn to_html(&self) -> String {
        let mut out = String::from("<html><body>");
        let chars: Vec<char> = self.text.chars().collect();
        let mut lines: Vec<(usize, usize)> = vec![];
        let mut start = 0;
        for (i, c) in chars.iter().enumerate() {
            if *c == '\n' {
                lines.push((start, i));
                start = i + 1;
            }
        }
        lines.push((start, chars.len()));
        let kind = |k: usize| self.lines.get(k).copied().unwrap_or_default();
        let count = lines.len();
        for (k, &(a, b)) in lines.iter().enumerate() {
            let this = kind(k);
            let before = (k > 0).then(|| kind(k - 1));
            let after = (k + 1 < count).then(|| kind(k + 1));
            let body = self.inline(&chars, a, b);
            match this {
                LineKind::Plain => {
                    out.push_str(&body);
                    let last = k + 1 == count;
                    if !last || (body.is_empty() && count > 1) {
                        out.push_str("<br>");
                    }
                }
                LineKind::Bullet | LineKind::Numbered => {
                    let tag = if this == LineKind::Bullet { "ul" } else { "ol" };
                    if before != Some(this) {
                        out.push_str(&format!("<{tag}>"));
                    }
                    out.push_str("<li>");
                    out.push_str(&body);
                    out.push_str("</li>");
                    if after != Some(this) {
                        out.push_str(&format!("</{tag}>"));
                    }
                }
            }
        }
        out.push_str("</body></html>");
        out
    }

    fn inline(&self, chars: &[char], a: usize, b: usize) -> String {
        let mut out = String::new();
        let mut open = Style::default();
        let close = |out: &mut String, s: Style| {
            if s.underline {
                out.push_str("</u>");
            }
            if s.italic {
                out.push_str("</i>");
            }
            if s.bold {
                out.push_str("</b>");
            }
        };
        for i in a..b {
            let style = self.styles.get(i).copied().unwrap_or_default();
            if style != open {
                close(&mut out, open);
                if style.bold {
                    out.push_str("<b>");
                }
                if style.italic {
                    out.push_str("<i>");
                }
                if style.underline {
                    out.push_str("<u>");
                }
                open = style;
            }
            match chars[i] {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                ' ' | '\t' if i == a || chars[i - 1] == ' ' || chars[i - 1] == '\t' => {
                    out.push_str("&nbsp;")
                }
                '\t' => out.push(' '),
                c => out.push(c),
            }
        }
        close(&mut out, open);
        out
    }

    /// Characters in the text.
    pub fn len(&self) -> usize {
        self.styles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.styles.is_empty()
    }

    /// The line holding character `at` (a line break belongs to the line it
    /// ends).
    pub fn line_of(&self, at: usize) -> usize {
        self.text.chars().take(at).filter(|c| *c == '\n').count()
    }

    /// The memo after its text was changed to `new` in place (typed, pasted
    /// or deleted): characters kept keep their style, new ones take
    /// `typed` or else the style of the character before them, and new lines
    /// take the kind of the line they were split from.
    pub fn after_edit(&self, new: &str, typed: Option<Style>) -> Self {
        let old: Vec<char> = self.text.chars().collect();
        let fresh: Vec<char> = new.chars().collect();
        let prefix = old.iter().zip(&fresh).take_while(|(a, b)| a == b).count();
        let room = old.len().min(fresh.len()) - prefix;
        let suffix = old
            .iter()
            .rev()
            .zip(fresh.iter().rev())
            .take(room)
            .take_while(|(a, b)| a == b)
            .count();
        let inserted = fresh.len() - prefix - suffix;
        let inherit = typed.unwrap_or_else(|| {
            prefix
                .checked_sub(1)
                .and_then(|i| self.styles.get(i).copied())
                .filter(|_| old.get(prefix - 1) != Some(&'\n'))
                .or_else(|| self.styles.get(prefix).copied())
                .unwrap_or_default()
        });
        let mut styles: Vec<Style> = self.styles[..prefix.min(self.styles.len())].to_vec();
        styles.extend(fresh[prefix..prefix + inserted].iter().map(|c| {
            if *c == '\n' {
                Style::default()
            } else {
                inherit
            }
        }));
        let tail = old.len() - suffix;
        styles.extend_from_slice(self.styles.get(tail..).unwrap_or(&[]));
        let breaks = |s: &[char]| s.iter().filter(|c| **c == '\n').count();
        let first = breaks(&old[..prefix]);
        let old_last = breaks(&old[..tail]);
        let new_last = breaks(&fresh[..prefix + inserted]);
        let split = self.lines.get(first).copied().unwrap_or_default();
        let mut lines: Vec<LineKind> = self.lines[..first.min(self.lines.len())].to_vec();
        lines.extend(std::iter::repeat_n(split, new_last - first + 1));
        lines.extend_from_slice(self.lines.get(old_last + 1..).unwrap_or(&[]));
        Self {
            text: new.to_string(),
            styles,
            lines,
        }
    }

    /// Switch `format` on characters `from..to`: off when every one of them
    /// has it, on otherwise. Line breaks are left alone.
    pub fn toggle(&mut self, from: usize, to: usize, format: Format) {
        let chars: Vec<char> = self.text.chars().collect();
        let to = to.min(self.styles.len());
        let range = from.min(to)..to;
        let text = |i: &usize| chars[*i] != '\n';
        let all = range
            .clone()
            .filter(text)
            .all(|i| self.styles[i].has(format));
        for i in range.filter(text) {
            self.styles[i] = self.styles[i].with(format, !all);
        }
    }

    /// Whether every character of `from..to` has `format` (for an empty
    /// range, the character before `from`).
    pub fn has(&self, from: usize, to: usize, format: Format) -> bool {
        let chars: Vec<char> = self.text.chars().collect();
        if from >= to {
            return from
                .checked_sub(1)
                .filter(|i| chars.get(*i) != Some(&'\n'))
                .and_then(|i| self.styles.get(i))
                .is_some_and(|s| s.has(format));
        }
        let mut seen = false;
        let end = to.min(chars.len());
        let styles = self.styles.get(from..).unwrap_or(&[]);
        for (c, style) in chars[from.min(end)..end].iter().zip(styles) {
            if *c != '\n' {
                if !style.has(format) {
                    return false;
                }
                seen = true;
            }
        }
        seen
    }

    /// Make the lines holding characters `from..to` items of a `kind` list,
    /// or plain lines again when they all are already.
    pub fn toggle_lines(&mut self, from: usize, to: usize, kind: LineKind) {
        if self.lines.is_empty() {
            return;
        }
        let max = self.lines.len() - 1;
        let first = self.line_of(from.min(to)).min(max);
        let last = self.line_of(to.max(from)).min(max);
        let span = &mut self.lines[first..=last];
        let all = span.iter().all(|k| *k == kind);
        let new = if all { LineKind::Plain } else { kind };
        span.iter_mut().for_each(|k| *k = new);
    }

    /// Plain text again: every character unformatted, every line plain.
    pub fn clear_formatting(&mut self) {
        self.styles.iter_mut().for_each(|s| *s = Style::default());
        self.lines.iter_mut().for_each(|k| *k = LineKind::Plain);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bold() -> Style {
        Style {
            bold: true,
            ..Default::default()
        }
    }

    #[test]
    fn plain_memos_read_as_lines() {
        let m = RichMemo::parse("First\u{7f}\u{7f}Second");
        assert_eq!(m.text, "First\nSecond");
        assert_eq!(m.lines.len(), 2);
        let m = RichMemo::parse(&super::super::records::text_to_memo("a <b>\nc"));
        assert_eq!(m.text, "a <b>\nc");
        assert!(m.styles.iter().all(|s| *s == Style::default()));
    }

    #[test]
    fn reference_style_html_reads_formatting_and_lists() {
        let html = "<HTML><HEAD><TITLE>x</TITLE></HEAD><BODY>\u{7f}\u{7f}<P>Pour <B>after</B> \
                    <FONT face=Arial><I>inspection</I></FONT></P><UL><LI>Crew of 6</LI>\
                    <LI><U>Pump</U></LI></UL><OL><LI>One</LI></OL><P>&nbsp;</P><P>End &amp; done</P></BODY></HTML>";
        let m = RichMemo::parse(html);
        assert_eq!(
            m.text,
            "Pour after inspection\nCrew of 6\nPump\nOne\n \nEnd & done"
        );
        assert_eq!(
            m.lines,
            vec![
                LineKind::Plain,
                LineKind::Bullet,
                LineKind::Bullet,
                LineKind::Numbered,
                LineKind::Plain,
                LineKind::Plain
            ]
        );
        assert!(m.styles[5].bold && !m.styles[4].bold);
        assert!(m.styles[11].italic);
        assert!(m.styles[32].underline);
        assert!(editable(html));
    }

    #[test]
    fn pictures_tables_and_links_are_not_editable() {
        for html in [
            "<html><body><img src=a.png></body></html>",
            "<html><body><table><tr><td>a</td></tr></table></body></html>",
            "<html><body><a href=x>a</a></body></html>",
        ] {
            assert!(!editable(html), "{html}");
        }
        assert!(editable("<5 days of float"));
    }

    #[test]
    fn formatting_survives_a_write_and_read() {
        let mut m = RichMemo::plain("Pour after inspection\nCrew of 6\nPump\n\nOne  two\n");
        m.toggle(5, 10, Format::Bold);
        m.toggle(0, 21, Format::Italic);
        m.toggle(22, 31, Format::Underline);
        m.toggle_lines(23, 33, LineKind::Bullet);
        m.toggle_lines(36, 36, LineKind::Numbered);
        let html = m.to_html();
        assert!(html.contains("<ul><li>"), "{html}");
        assert!(html.contains("&nbsp;"), "{html}");
        assert_eq!(RichMemo::parse(&html), m, "{html}");
        for text in ["", "a", "a\n", "\n\na", " lead", "x\n\ny"] {
            let m = RichMemo::plain(text);
            assert_eq!(RichMemo::parse(&m.to_html()), m, "{text:?}");
        }
        let mut list = RichMemo::plain("x\n\ny");
        list.toggle_lines(0, 2, LineKind::Numbered);
        assert_eq!(RichMemo::parse(&list.to_html()), list);
        let mut tail = RichMemo::plain("x\n");
        tail.toggle_lines(0, 0, LineKind::Bullet);
        assert_eq!(RichMemo::parse(&tail.to_html()), tail);
    }

    #[test]
    fn typing_keeps_styles_and_list_lines() {
        let mut m = RichMemo::plain("ab\ncd");
        m.toggle(0, 2, Format::Bold);
        m.toggle_lines(0, 0, LineKind::Bullet);
        let typed = m.after_edit("abX\nY\ncd", None);
        assert_eq!(typed.styles[2], bold(), "typed after bold text");
        assert_eq!(
            typed.lines,
            vec![LineKind::Bullet, LineKind::Bullet, LineKind::Plain]
        );
        let cut = typed.after_edit("acd", None);
        assert_eq!(cut.lines, vec![LineKind::Bullet]);
        assert_eq!(cut.styles.len(), 3);
        let plain = m.after_edit("abZ\ncd", Some(Style::default()));
        assert_eq!(plain.styles[2], Style::default());
    }
}
