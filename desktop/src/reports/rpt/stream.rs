//! The report definition text: a stream of nested components, each
//! `object Class` ... `end` with `Name = value` properties and child
//! components. The stream is kept lossless: every line keeps its own
//! indentation, property values keep their original text (including string
//! lists and wrapped strings), and text after the root component is kept as
//! is, so reading a definition and writing it back gives the same bytes.
//! Edits replace whole property values or whole components.
use std::fmt::Write as _;

/// Components nested deeper than this are refused (real definitions use
/// about ten levels).
pub const MAX_DEPTH: usize = 128;

/// A whole definition.
#[derive(Clone, Debug, PartialEq)]
pub struct Stream {
    pub root: Component,
    /// Line ending of the text, `"\r\n"` or `"\n"`.
    pub eol: String,
    /// Everything after the root component's closing `end`, verbatim.
    pub trailer: String,
    /// The text was not UTF-8 and is held one byte per character.
    pub latin1: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Component {
    /// Leading spaces of the `object` line.
    pub indent: usize,
    pub class: String,
    /// The instance name of `object Name: Class`, when present.
    pub name: Option<String>,
    pub items: Vec<Item>,
    /// Leading spaces of the closing `end` line.
    pub end_indent: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Prop(Prop),
    Child(Component),
    /// A line that is neither (kept verbatim).
    Raw(String),
}

/// One property: its first line's value text and any continuation lines
/// (string lists, wrapped strings, binary data), verbatim.
#[derive(Clone, Debug, PartialEq)]
pub struct Prop {
    pub indent: usize,
    pub name: String,
    pub first: String,
    pub rest: Vec<String>,
}

impl Prop {
    pub fn single(indent: usize, name: &str, value: &str) -> Self {
        Self {
            indent,
            name: name.into(),
            first: value.into(),
            rest: vec![],
        }
    }

    /// The text value of a quoted string property (concatenations and `#n`
    /// codes decoded).
    pub fn string(&self) -> Option<String> {
        let mut text = self.first.clone();
        for line in &self.rest {
            text.push('\n');
            text.push_str(line);
        }
        let joined = text.replace("+\n", " ");
        pascal_decode(&joined)
    }

    /// The items of a string list property.
    pub fn list(&self) -> Option<Vec<String>> {
        if !self.first.starts_with('(') {
            return None;
        }
        let mut lines: Vec<&str> = vec![self.first.strip_prefix('(').unwrap_or("")];
        lines.extend(self.rest.iter().map(String::as_str));
        let mut items = vec![];
        let mut current = String::new();
        let last = lines.len() - 1;
        for (i, line) in lines.iter().enumerate() {
            let mut line = line.trim();
            if i == last {
                line = line.strip_suffix(')')?.trim_end();
            }
            if line.is_empty() {
                continue;
            }
            let continues = line.ends_with('+');
            let body = if continues {
                line.trim_end_matches('+').trim_end()
            } else {
                line
            };
            current.push_str(body);
            current.push(' ');
            if !continues {
                items.push(pascal_decode(&current)?);
                current.clear();
            }
        }
        if !current.is_empty() {
            return None;
        }
        Some(items)
    }

    /// A string list value in the layout definitions use: `(` on the
    /// property line, one quoted item per line two spaces deeper, `)` after
    /// the last item.
    pub fn from_list(indent: usize, name: &str, items: &[String]) -> Self {
        if items.is_empty() {
            return Self::single(indent, name, "()");
        }
        let pad = " ".repeat(indent + 2);
        let n = items.len();
        Self {
            indent,
            name: name.into(),
            first: "(".into(),
            rest: items
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    format!(
                        "{pad}{}{}",
                        pascal_encode(s),
                        if i + 1 == n { ")" } else { "" }
                    )
                })
                .collect(),
        }
    }
}

impl Component {
    pub fn new(indent: usize, class: &str) -> Self {
        Self {
            indent,
            class: class.into(),
            name: None,
            items: vec![],
            end_indent: indent,
        }
    }

    pub fn prop(&self, name: &str) -> Option<&Prop> {
        self.items.iter().find_map(|i| match i {
            Item::Prop(p) if p.name == name => Some(p),
            _ => None,
        })
    }

    pub fn prop_mut(&mut self, name: &str) -> Option<&mut Prop> {
        self.items.iter_mut().find_map(|i| match i {
            Item::Prop(p) if p.name == name => Some(p),
            _ => None,
        })
    }

    /// The first-line value text of a property.
    pub fn raw(&self, name: &str) -> Option<&str> {
        self.prop(name).map(|p| p.first.as_str())
    }

    pub fn string(&self, name: &str) -> Option<String> {
        self.prop(name).and_then(Prop::string)
    }

    pub fn int(&self, name: &str) -> Option<i64> {
        self.raw(name)?.trim().parse().ok()
    }

    pub fn flag(&self, name: &str) -> Option<bool> {
        match self.raw(name)?.trim() {
            "True" => Some(true),
            "False" => Some(false),
            _ => None,
        }
    }

    pub fn list(&self, name: &str) -> Option<Vec<String>> {
        self.prop(name).and_then(Prop::list)
    }

    /// Indentation for a property added to this component: that of the
    /// existing properties, else two deeper than the component.
    fn prop_indent(&self) -> usize {
        self.items
            .iter()
            .find_map(|i| match i {
                Item::Prop(p) => Some(p.indent),
                _ => None,
            })
            .unwrap_or(self.indent + 2)
    }

    /// Replaces a property (or adds it after the last property, before the
    /// children).
    pub fn set(&mut self, prop: Prop) {
        if let Some(p) = self.prop_mut(&prop.name) {
            *p = prop;
            return;
        }
        let at = self
            .items
            .iter()
            .rposition(|i| matches!(i, Item::Prop(_)))
            .map_or(0, |i| i + 1);
        self.items.insert(at, Item::Prop(prop));
    }

    pub fn set_raw(&mut self, name: &str, value: &str) {
        let indent = self.prop_indent();
        self.set(Prop::single(indent, name, value));
    }

    pub fn set_string(&mut self, name: &str, value: &str) {
        self.set_raw(name, &pascal_encode(value));
    }

    pub fn set_int(&mut self, name: &str, value: i64) {
        self.set_raw(name, &value.to_string());
    }

    pub fn set_flag(&mut self, name: &str, value: bool) {
        self.set_raw(name, if value { "True" } else { "False" });
    }

    pub fn set_list(&mut self, name: &str, items: &[String]) {
        let indent = self.prop_indent();
        self.set(Prop::from_list(indent, name, items));
    }

    pub fn remove(&mut self, name: &str) {
        self.items
            .retain(|i| !matches!(i, Item::Prop(p) if p.name == name));
    }

    pub fn children(&self) -> impl Iterator<Item = &Component> {
        self.items.iter().filter_map(|i| match i {
            Item::Child(c) => Some(c),
            _ => None,
        })
    }

    /// Moves this component and everything in it `by` columns right (left
    /// when negative), for pasting at another depth.
    pub fn shift(&mut self, by: isize) {
        let f = |n: &mut usize| *n = (*n as isize + by).max(0) as usize;
        f(&mut self.indent);
        f(&mut self.end_indent);
        for item in &mut self.items {
            match item {
                Item::Prop(p) => {
                    f(&mut p.indent);
                    for line in &mut p.rest {
                        if by >= 0 {
                            line.insert_str(0, &" ".repeat(by as usize));
                        } else {
                            let cut = line
                                .bytes()
                                .take_while(|b| *b == b' ')
                                .count()
                                .min(by.unsigned_abs());
                            line.drain(..cut);
                        }
                    }
                }
                Item::Child(c) => c.shift(by),
                Item::Raw(_) => {}
            }
        }
    }
}

// ---------------------------------------------------------------- strings

/// Decodes a Pascal string expression: quoted runs (`''` is a quote) and
/// `#n` character codes in any order, joined. `None` if it is not one.
pub fn pascal_decode(text: &str) -> Option<String> {
    let mut out = String::new();
    let b: Vec<char> = text.trim().chars().collect();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            '\'' => {
                i += 1;
                loop {
                    match b.get(i) {
                        None => return None,
                        Some('\'') if b.get(i + 1) == Some(&'\'') => {
                            out.push('\'');
                            i += 2;
                        }
                        Some('\'') => {
                            i += 1;
                            break;
                        }
                        Some(c) => {
                            out.push(*c);
                            i += 1;
                        }
                    }
                }
            }
            '#' => {
                i += 1;
                let hex = b.get(i) == Some(&'$');
                if hex {
                    i += 1;
                }
                let start = i;
                while b
                    .get(i)
                    .is_some_and(|c| c.is_digit(if hex { 16 } else { 10 }))
                {
                    i += 1;
                }
                if start == i {
                    return None;
                }
                let digits: String = b[start..i].iter().collect();
                let code = u32::from_str_radix(&digits, if hex { 16 } else { 10 }).ok()?;
                out.push(char::from_u32(code)?);
            }
            ' ' | '\t' => i += 1,
            _ => return None,
        }
    }
    Some(out)
}

/// A Pascal string expression for `text`: quoted runs of printable
/// characters and `#n` for control characters.
pub fn pascal_encode(text: &str) -> String {
    let mut out = String::new();
    let mut quoted = false;
    for c in text.chars() {
        if (c as u32) < 32 || c as u32 == 127 {
            if quoted {
                out.push('\'');
                quoted = false;
            }
            let _ = write!(out, "#{}", c as u32);
        } else {
            if !quoted {
                out.push('\'');
                quoted = true;
            }
            if c == '\'' {
                out.push('\'');
            }
            out.push(c);
        }
    }
    if quoted {
        out.push('\'');
    }
    if out.is_empty() {
        out.push_str("''");
    }
    out
}

// ---------------------------------------------------------------- parsing

impl Stream {
    /// Reads a definition from bytes: UTF-8 when valid, else one byte per
    /// character.
    pub fn parse_bytes(bytes: &[u8]) -> Result<Stream, String> {
        if bytes.starts_with(b"TPF0") {
            return Err("This is a compiled form, not a text definition".into());
        }
        let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
        match std::str::from_utf8(bytes) {
            Ok(text) => Self::parse(text),
            Err(_) => {
                let text: String = bytes.iter().map(|&b| char::from(b)).collect();
                let mut s = Self::parse(&text)?;
                s.latin1 = true;
                Ok(s)
            }
        }
    }

    /// The definition as bytes, the inverse of [`Stream::parse_bytes`].
    pub fn to_bytes(&self) -> Vec<u8> {
        let text = self.write();
        if self.latin1 {
            text.chars()
                .map(|c| u8::try_from(c as u32).unwrap_or(b'?'))
                .collect()
        } else {
            text.into_bytes()
        }
    }

    pub fn parse(text: &str) -> Result<Stream, String> {
        let eol = if text.find('\n').is_some_and(|i| text[..i].ends_with('\r')) {
            "\r\n"
        } else {
            "\n"
        };
        // Lines with their end-of-line text; the root's closing line ends the
        // component and the rest is the trailer.
        let mut lines: Vec<(&str, &str)> = vec![];
        let mut rest = text;
        while !rest.is_empty() {
            match rest.find('\n') {
                Some(i) => {
                    let (line, tail) = rest.split_at(i + 1);
                    let body = line[..i].strip_suffix('\r').unwrap_or(&line[..i]);
                    lines.push((body, &line[body.len()..]));
                    rest = tail;
                }
                None => {
                    lines.push((rest, ""));
                    rest = "";
                }
            }
        }
        let mut at = 0;
        let mut stack: Vec<Component> = vec![];
        let mut root: Option<Component> = None;
        let mut trailer_from = 0usize;
        while at < lines.len() {
            let (line, _) = lines[at];
            let indent = line.bytes().take_while(|b| *b == b' ').count();
            let body = &line[indent..];
            at += 1;
            if root.is_none() && stack.is_empty() {
                // Before the root: blank lines only.
                if body.is_empty() {
                    return Err("The definition starts with an empty line".into());
                }
                let c = header(indent, body)
                    .ok_or_else(|| format!("Line {at}: the definition must start with `object`"))?;
                stack.push(c);
                continue;
            }
            let Some(top) = stack.last_mut() else { break };
            if let Some(c) = header(indent, body) {
                if stack.len() >= MAX_DEPTH {
                    return Err(format!("Line {at}: components are nested too deeply"));
                }
                stack.push(c);
            } else if body == "end" {
                let mut done = stack.pop().expect("a component is open");
                done.end_indent = indent;
                match stack.last_mut() {
                    Some(parent) => parent.items.push(Item::Child(done)),
                    None => {
                        let consumed: usize =
                            lines[..at].iter().map(|(l, e)| l.len() + e.len()).sum();
                        trailer_from = consumed - lines[at - 1].1.len();
                        root = Some(done);
                        break;
                    }
                }
            } else if let Some((name, value)) = property(body) {
                let mut prop = Prop {
                    indent,
                    name: name.into(),
                    first: value.into(),
                    rest: vec![],
                };
                let mut depth = continuation_start(value);
                while let Some(open) = &mut depth {
                    if at >= lines.len() {
                        return Err(format!("Line {at}: property {name} is not finished"));
                    }
                    let next = lines[at].0;
                    at += 1;
                    prop.rest.push(next.to_string());
                    if open.finish(next) {
                        break;
                    }
                }
                top.items.push(Item::Prop(prop));
            } else {
                top.items.push(Item::Raw(line.to_string()));
            }
        }
        let root = root.ok_or("The definition has no closing `end`")?;
        let stream = Stream {
            root,
            eol: eol.into(),
            trailer: text[trailer_from..].to_string(),
            latin1: false,
        };
        if stream.write() != text {
            return Err("The text has irregular line endings or spacing".into());
        }
        Ok(stream)
    }

    pub fn write(&self) -> String {
        let mut out = String::new();
        // Iterative: (component, next item).
        let mut stack: Vec<(&Component, usize)> = vec![];
        let open = |out: &mut String, c: &Component, eol: &str| {
            out.push_str(&" ".repeat(c.indent));
            out.push_str("object ");
            if let Some(n) = &c.name {
                out.push_str(n);
                out.push_str(": ");
            }
            out.push_str(&c.class);
            out.push_str(eol);
        };
        open(&mut out, &self.root, &self.eol);
        stack.push((&self.root, 0));
        while let Some((c, next)) = stack.last().copied() {
            match c.items.get(next) {
                Some(item) => {
                    stack.last_mut().expect("stack is not empty").1 += 1;
                    match item {
                        Item::Prop(p) => {
                            out.push_str(&" ".repeat(p.indent));
                            out.push_str(&p.name);
                            out.push_str(" = ");
                            out.push_str(&p.first);
                            out.push_str(&self.eol);
                            for line in &p.rest {
                                out.push_str(line);
                                out.push_str(&self.eol);
                            }
                        }
                        Item::Child(child) => {
                            open(&mut out, child, &self.eol);
                            stack.push((child, 0));
                        }
                        Item::Raw(line) => {
                            out.push_str(line);
                            out.push_str(&self.eol);
                        }
                    }
                }
                None => {
                    out.push_str(&" ".repeat(c.end_indent));
                    out.push_str("end");
                    stack.pop();
                    if !stack.is_empty() {
                        out.push_str(&self.eol);
                    }
                }
            }
        }
        out.push_str(&self.trailer);
        out
    }
}

fn header(indent: usize, body: &str) -> Option<Component> {
    let rest = body.strip_prefix("object ")?;
    let (name, class) = match rest.split_once(':') {
        Some((n, c)) => (Some(n.trim().to_string()), c.trim()),
        None => (None, rest.trim()),
    };
    let ident = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_');
    if !ident(class) || name.as_deref().is_some_and(|n| !ident(n)) {
        return None;
    }
    let mut c = Component::new(indent, class);
    c.name = name;
    Some(c)
}

/// `Name = value` with a dotted identifier name.
fn property(body: &str) -> Option<(&str, &str)> {
    let (name, value) = body.split_once(" = ")?;
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
    {
        return None;
    }
    Some((name, value))
}

/// How the continuation lines of a multi-line value end.
enum Open {
    /// A parenthesized list: depth outside quotes.
    List(i32),
    Binary,
    Collection(i32),
    /// A string wrapped with `+` at the end of each line.
    Wrapped,
}

fn continuation_start(first: &str) -> Option<Open> {
    let t = first.trim_end();
    if t.starts_with('(') {
        let mut open = Open::List(0);
        // The first line may already close the list (`()`).
        return (!open.finish(t)).then_some(open);
    }
    if t == "{" {
        return Some(Open::Binary);
    }
    if t == "<" {
        return Some(Open::Collection(0));
    }
    if t.ends_with('+') && (t.starts_with('\'') || t.starts_with('#')) {
        return Some(Open::Wrapped);
    }
    None
}

impl Open {
    /// Reads one more line; true when it completes the value.
    fn finish(&mut self, line: &str) -> bool {
        match self {
            Open::List(depth) => {
                let mut quoted = false;
                for c in line.chars() {
                    match c {
                        '\'' => quoted = !quoted,
                        '(' if !quoted => *depth += 1,
                        ')' if !quoted => *depth -= 1,
                        _ => {}
                    }
                }
                *depth <= 0
            }
            Open::Binary => line.trim_end().ends_with('}'),
            Open::Collection(depth) => {
                let t = line.trim();
                if t == "item" || t.starts_with("item ") {
                    *depth += 1;
                }
                if t == "end" || t == "end>" {
                    *depth -= 1;
                }
                t.ends_with('>') && *depth <= 0
            }
            Open::Wrapped => !line.trim_end().ends_with('+'),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "object TCVReport\r\n  Width = 768\r\n  Title = 'Plan''s'#13#10'x'\r\n  PageSettingsList.Strings = (\r\n    '(0||()('\r\n    '  (0||page(orientation|1)())))')\r\n  Empty.Strings = ()\r\n  object TCVReportRow\r\n    Height = 20\r\n  end\r\nend\r\n";

    #[test]
    fn reads_and_writes_exactly() {
        let s = Stream::parse(SAMPLE).unwrap();
        assert_eq!(s.eol, "\r\n");
        assert_eq!(s.write(), SAMPLE);
        assert_eq!(s.root.class, "TCVReport");
        assert_eq!(s.root.int("Width"), Some(768));
        assert_eq!(s.root.string("Title").unwrap(), "Plan's\r\nx");
        assert_eq!(
            s.root.list("PageSettingsList.Strings").unwrap(),
            ["(0||()(", "  (0||page(orientation|1)())))"]
        );
        assert_eq!(s.root.list("Empty.Strings").unwrap(), Vec::<String>::new());
        assert_eq!(s.root.children().count(), 1);
        assert_eq!(s.trailer, "\r\n");
    }

    #[test]
    fn keeps_odd_indents_and_trailing_garbage() {
        let text =
            "object R\n   A = 1\n   object C\n     B = 2\n   end\n end\n nt = False\n d\n end\n ";
        let s = Stream::parse(text).unwrap();
        assert_eq!(s.write(), text);
        assert!(s.trailer.contains("nt = False"));
        let text = "object R\n  A = 'x'";
        assert!(Stream::parse(text).is_err());
    }

    #[test]
    fn wrapped_strings_and_lists_decode() {
        let text = "object R\n  T = 'abc' +\n    'def'\n  L.Strings = (\n    'one' +\n    'two'\n    'three')\n  F = [fsBold]\nend\n";
        let s = Stream::parse(text).unwrap();
        assert_eq!(s.write(), text);
        assert_eq!(s.root.string("T").unwrap(), "abcdef");
        assert_eq!(s.root.list("L.Strings").unwrap(), ["onetwo", "three"]);
        assert_eq!(s.root.raw("F"), Some("[fsBold]"));
    }

    #[test]
    fn edits_write_and_read_back() {
        let mut s = Stream::parse(SAMPLE).unwrap();
        s.root.set_string("Title", "A 'quoted'\tname");
        s.root.set_int("Height", 99);
        s.root
            .set_list("Filter.Strings", &["a = 1".into(), "b = 'x'".into()]);
        s.root.remove("Width");
        let again = Stream::parse(&s.write()).unwrap();
        assert_eq!(again, s);
        assert_eq!(again.root.string("Title").unwrap(), "A 'quoted'\tname");
        assert_eq!(again.root.int("Height"), Some(99));
        assert_eq!(again.root.int("Width"), None);
        assert_eq!(
            again.root.list("Filter.Strings").unwrap(),
            ["a = 1", "b = 'x'"]
        );
    }

    #[test]
    fn pascal_strings() {
        assert_eq!(pascal_encode(""), "''");
        assert_eq!(pascal_encode("a'b"), "'a''b'");
        assert_eq!(pascal_encode("a\r\nb"), "'a'#13#10'b'");
        assert_eq!(pascal_decode("'a'#13#10'b'").unwrap(), "a\r\nb");
        assert_eq!(pascal_decode("#$41").unwrap(), "A");
        assert!(pascal_decode("'open").is_none());
        assert!(pascal_decode("bare").is_none());
        for s in ["", "x", "it's", "\u{1}\u{2}", "'", "''", "a\tb", "é"] {
            assert_eq!(pascal_decode(&pascal_encode(s)).unwrap(), s);
        }
    }

    #[test]
    fn shift_moves_everything() {
        let mut s = Stream::parse(SAMPLE).unwrap();
        let mut row = s.root.children().next().unwrap().clone();
        row.shift(4);
        assert_eq!(row.indent, 6);
        assert_eq!(row.end_indent, 6);
        row.shift(-4);
        assert_eq!(&row, s.root.children().next().unwrap());
        s.root.items.push(Item::Child(row));
    }

    #[test]
    fn binary_and_bad_text_are_errors() {
        assert!(Stream::parse_bytes(b"TPF0\x0dTCVReport").is_err());
        assert!(Stream::parse("hello").is_err());
        assert!(Stream::parse("").is_err());
        assert!(Stream::parse("object A\n  X = (\n    'a'\n").is_err());
    }
}
