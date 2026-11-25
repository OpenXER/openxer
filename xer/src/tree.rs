//! The nested `(0||name(key|value|key|value)(children))` text dialect shared by
//! calendar data and report definition payloads.
//!
//! A node is `(0||` name `(` words `)` `(` children `)` `)`. Words are the
//! `|`-separated items of the first parenthesis group, normally key/value
//! pairs. White space is skipped between nodes only; inside words it is data.
//! Parsing is iterative with an explicit depth limit, so hostile input cannot
//! overflow the stack. Values use `\XX` hex escapes for the characters the
//! dialect itself uses (see [`escape`] and [`unescape`]).

/// Depth limit used by [`Tree::parse`]. Calendar trees are a few levels deep
/// and report payloads about six.
pub const MAX_DEPTH: usize = 64;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tree {
    pub name: String,
    /// The `|`-separated words of the attribute group, as written.
    pub words: Vec<String>,
    pub children: Vec<Tree>,
}

impl Tree {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            ..Self::default()
        }
    }

    /// A node with `pairs` as its key/value words.
    pub fn with(name: &str, pairs: &[(&str, &str)], children: Vec<Tree>) -> Self {
        let mut t = Self::new(name);
        for (k, v) in pairs {
            t.words.push((*k).to_string());
            t.words.push((*v).to_string());
        }
        t.children = children;
        t
    }

    /// The key/value pairs (a trailing unpaired word is not a pair).
    pub fn pairs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.words
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[k, v]| (k.as_str(), v.as_str()))
    }

    /// The raw value of `key`.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.pairs().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    /// The value of `key` with `\XX` escapes decoded.
    pub fn text(&self, key: &str) -> Option<String> {
        self.get(key).map(unescape)
    }

    /// Sets the raw value of `key`, appending the pair when it is missing.
    pub fn set(&mut self, key: &str, value: &str) {
        let n = self.words.len() / 2 * 2;
        for i in (0..n).step_by(2) {
            if self.words[i] == key {
                self.words[i + 1] = value.to_string();
                return;
            }
        }
        self.words.truncate(n);
        self.words.push(key.to_string());
        self.words.push(value.to_string());
    }

    pub fn child(&self, name: &str) -> Option<&Tree> {
        self.children.iter().find(|c| c.name == name)
    }

    pub fn child_mut(&mut self, name: &str) -> Option<&mut Tree> {
        self.children.iter_mut().find(|c| c.name == name)
    }

    /// Parses one node from the start of `text` (leading white space is
    /// skipped; text after the node is ignored).
    pub fn parse(text: &str) -> Result<Tree, String> {
        Self::parse_at(text, &mut 0, MAX_DEPTH)
    }

    /// Parses the node at `*pos` and leaves `*pos` after it. Nodes nested
    /// deeper than `max_depth` levels are an error.
    pub fn parse_at(data: &str, pos: &mut usize, max_depth: usize) -> Result<Tree, String> {
        let bytes = data.as_bytes();
        let skip = |pos: &mut usize| {
            while bytes.get(*pos).is_some_and(u8::is_ascii_whitespace) {
                *pos += 1;
            }
        };
        // Nodes whose children are still being read, innermost last.
        let mut open: Vec<Tree> = vec![];
        loop {
            // Read the next node's head.
            skip(pos);
            if !data.get(*pos..).is_some_and(|s| s.starts_with("(0||")) {
                return Err(format!("Invalid tree near byte {pos}"));
            }
            if open.len() >= max_depth {
                return Err("The tree is nested too deeply".into());
            }
            *pos += 4;
            let name_end = data[*pos..].find('(').ok_or("A tree node is incomplete")? + *pos;
            let name = data[*pos..name_end].to_string();
            *pos = name_end + 1;
            let words_end = data[*pos..]
                .find(')')
                .ok_or("The words of a tree node are incomplete")?
                + *pos;
            let words: Vec<String> = data[*pos..words_end].split('|').map(String::from).collect();
            // `()` holds no words, not one empty word.
            let words = if words.len() == 1 && words[0].is_empty() {
                vec![]
            } else {
                words
            };
            *pos = words_end + 1;
            if bytes.get(*pos) != Some(&b'(') {
                return Err("A tree node has no children group".into());
            }
            *pos += 1;
            open.push(Tree {
                name,
                words,
                children: vec![],
            });
            // Close finished nodes until another child starts.
            loop {
                skip(pos);
                match bytes.get(*pos) {
                    Some(b')') => {
                        *pos += 1;
                        skip(pos);
                        if bytes.get(*pos) != Some(&b')') {
                            return Err("A tree node is not closed".into());
                        }
                        *pos += 1;
                        let done = open.pop().expect("a node is open");
                        match open.last_mut() {
                            Some(parent) => parent.children.push(done),
                            None => return Ok(done),
                        }
                    }
                    Some(_) => break,
                    None => return Err("A tree node is not closed".into()),
                }
            }
        }
    }

    /// The canonical indented text: each child on its own line, two spaces
    /// per level, the closing parentheses of a node run together after its
    /// last child. Lines are joined with `eol`.
    pub fn write(&self, eol: &str) -> String {
        let mut out = String::new();
        self.write_at(&mut out, eol);
        out
    }

    fn write_at(&self, out: &mut String, eol: &str) {
        // Iterative: (node, next child index, depth).
        let mut stack: Vec<(&Tree, usize)> = vec![];
        let head = |out: &mut String, t: &Tree| {
            out.push_str("(0||");
            out.push_str(&t.name);
            out.push('(');
            out.push_str(&t.words.join("|"));
            out.push_str(")(");
        };
        head(out, self);
        stack.push((self, 0));
        while let Some((node, next)) = stack.last().copied() {
            if let Some(child) = node.children.get(next) {
                stack.last_mut().expect("stack is not empty").1 += 1;
                out.push_str(eol);
                out.push_str(&"  ".repeat(stack.len()));
                head(out, child);
                stack.push((child, 0));
            } else {
                out.push_str("))");
                stack.pop();
            }
        }
    }

    /// The text without any white space between nodes.
    pub fn write_compact(&self) -> String {
        let mut out = String::new();
        let mut stack: Vec<(&Tree, usize)> = vec![(self, 0)];
        out.push_str(&format!("(0||{}({})(", self.name, self.words.join("|")));
        while let Some((node, next)) = stack.last().copied() {
            if let Some(child) = node.children.get(next) {
                stack.last_mut().expect("stack is not empty").1 += 1;
                out.push_str(&format!("(0||{}({})(", child.name, child.words.join("|")));
                stack.push((child, 0));
            } else {
                out.push_str("))");
                stack.pop();
            }
        }
        out
    }
}

/// Decodes `\XX` (two hex digits, a Latin-1 character) escapes; a backslash
/// not followed by two hex digits stays as it is.
pub fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let mut plain = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push_str(&s[plain..i]);
                out.push(char::from(h * 16 + l));
                i += 3;
                plain = i;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&s[plain..]);
    out
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Encodes a text for use as a word: the characters the dialect uses
/// (`\`, `|`, `(`, `)`), control characters and characters above Latin-1
/// become `\XX` (the latter as `?`, since the escape holds one byte).
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' | '|' | '(' | ')' => out.push_str(&format!("\\{:02X}", c as u32)),
            c if (c as u32) < 32 || (0x7f..0xa0).contains(&(c as u32)) => {
                out.push_str(&format!("\\{:02X}", c as u32))
            }
            c if (c as u32) >= 0xa0 && (c as u32) < 0x100 => {
                out.push_str(&format!("\\{:02X}", c as u32))
            }
            c if (c as u32) >= 0x100 => out.push('?'),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "(0||()(\n  (0||page(orientation|1|paper_size|256)())\n  (0||header()(\n    (0||left(text||mode|T)(\n      (0||font(font_name|Arial|size|10)())))\n    (0||right(text|[date] [time]|mode|T)())))))";

    #[test]
    fn parses_and_writes_the_indented_form_exactly() {
        let t = Tree::parse(SAMPLE).unwrap();
        assert_eq!(t.name, "");
        assert_eq!(t.children.len(), 2);
        assert_eq!(t.children[0].get("paper_size"), Some("256"));
        let header = t.child("header").unwrap();
        assert_eq!(header.children[1].get("text"), Some("[date] [time]"));
        assert_eq!(
            header.children[0].children[0].get("font_name"),
            Some("Arial")
        );
        assert_eq!(t.write("\n"), SAMPLE);
        assert_eq!(Tree::parse(&t.write_compact()).unwrap(), t);
    }

    #[test]
    fn deep_nesting_is_an_error_not_a_crash() {
        let mut text = String::new();
        for _ in 0..100_000 {
            text.push_str("(0||a()(");
        }
        assert!(Tree::parse(&text)
            .unwrap_err()
            .contains("nested too deeply"));
        // Exactly at the limit is fine.
        let mut ok = String::new();
        for _ in 0..MAX_DEPTH {
            ok.push_str("(0||a()(");
        }
        for _ in 0..MAX_DEPTH {
            ok.push_str("))");
        }
        assert!(Tree::parse(&ok).is_ok());
    }

    #[test]
    fn malformed_text_is_an_error() {
        for bad in [
            "",
            "(",
            "(0||",
            "(0||a",
            "(0||a(",
            "(0||a()",
            "(0||a()(",
            "(0||a()()",
            "(0||a()(x)",
            "(0||a()((0||b()())",
            "junk",
        ] {
            assert!(Tree::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn escapes_round_trip() {
        assert_eq!(unescape("A\\0D\\0AB \\A9 x"), "A\r\nB \u{a9} x");
        assert_eq!(unescape("\\4"), "\\4");
        assert_eq!(unescape("\\zz"), "\\zz");
        assert_eq!(unescape("tail\\"), "tail\\");
        let s = "a|b(c)d\\e\r\n\u{a9}";
        assert_eq!(unescape(&escape(s)), s);
        assert!(!escape(s).contains(['|', '(', ')']));
        let mut t = Tree::with("n", &[("k", "v")], vec![]);
        t.set("text", &escape("x (1) | y"));
        let back = Tree::parse(&t.write("\n")).unwrap();
        assert_eq!(back.text("text").unwrap(), "x (1) | y");
    }

    #[test]
    fn set_replaces_or_appends() {
        let mut t = Tree::with("n", &[("a", "1")], vec![]);
        t.set("a", "2");
        t.set("b", "3");
        assert_eq!(t.words, ["a", "2", "b", "3"]);
    }
}
