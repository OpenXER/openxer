//! The spelling dictionary: the built-in English list (derived from SCOWL,
//! see `THIRD_PARTY.md`), a small construction supplement and the user's
//! own words. Also the word tokenizer and the suggestion search.
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// Sorted, front-coded word list: each line is one character giving the
/// number of leading characters shared with the previous word (0-9, then
/// a-z for 10-35), then the rest of the word.
const WORDS: &str = include_str!("../../assets/spell/words.txt");
const SUPPLEMENT: &str = include_str!("../../assets/spell/construction.txt");

/// Longest word the checker looks up or suggests for, in characters. No
/// English word comes close; a longer run of letters (a pasted blob in a
/// notebook from an untrusted file) is reported as unknown, without
/// suggestions.
pub const MAX_WORD: usize = 40;
/// Most second-order candidates tried for one suggestion search.
const MAX_SECOND_ORDER: usize = 60_000;

/// `word` cut to `MAX_WORD` characters, for display and for the Ignore All
/// and Change All keys.
pub fn clip(word: &str) -> &str {
    match word.char_indices().nth(MAX_WORD) {
        Some((i, _)) => &word[..i],
        None => word,
    }
}

fn too_long(word: &str) -> bool {
    word.chars().nth(MAX_WORD).is_some()
}

pub struct Dictionary {
    /// Every word as listed (proper nouns keep their capital).
    exact: HashSet<String>,
    /// Lowercase form of every word -> the listed form (for suggestions and
    /// all-caps text).
    folded: HashMap<String, String>,
    /// Words listed in lowercase: any capitalization of these is fine.
    lower: HashSet<String>,
}

fn decode(text: &str) -> Vec<String> {
    const DIGITS: &str = "0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    let mut prev = String::new();
    for line in text.lines() {
        let mut chars = line.chars();
        let Some(n) = chars.next().and_then(|c| DIGITS.find(c)) else {
            continue;
        };
        let keep: String = prev.chars().take(n).collect();
        let word = format!("{keep}{}", chars.as_str());
        out.push(word.clone());
        prev = word;
    }
    out
}

impl Dictionary {
    pub fn from_words(words: impl IntoIterator<Item = String>) -> Self {
        let mut d = Self {
            exact: HashSet::new(),
            folded: HashMap::new(),
            lower: HashSet::new(),
        };
        for w in words {
            let low = w.to_lowercase();
            if w == low {
                d.lower.insert(low.clone());
            }
            d.folded.entry(low).or_insert_with(|| w.clone());
            d.exact.insert(w);
        }
        d
    }

    #[cfg(test)]
    pub fn word_count(&self) -> usize {
        self.exact.len()
    }

    /// Is `word` spelled right? `user` holds the user's words in lowercase.
    pub fn known(&self, word: &str, user: &HashSet<String>) -> bool {
        if too_long(word) {
            return false;
        }
        let word = word
            .strip_suffix("'s")
            .or_else(|| word.strip_suffix("'S"))
            .unwrap_or(word)
            .trim_end_matches('\'');
        let letters = word.chars().filter(|c| c.is_alphabetic()).count();
        if letters <= 1 || word.chars().any(|c| c.is_ascii_digit()) {
            return true;
        }
        let low = word.to_lowercase();
        if self.exact.contains(word) || self.lower.contains(&low) || user.contains(&low) {
            return true;
        }
        let all_caps = word.chars().all(|c| !c.is_lowercase());
        if all_caps {
            // An acronym, or a capitalized heading word.
            return letters <= 5 || self.folded.contains_key(&low);
        }
        // "Monday" at any capitalization that starts with the capital.
        let mut chars = word.chars();
        let title =
            chars.next().is_some_and(char::is_uppercase) && chars.all(|c| !c.is_uppercase());
        title
            && self
                .folded
                .get(&low)
                .is_some_and(|w| w.to_lowercase() == low)
    }

    fn present(&self, low: &str, user: &HashSet<String>) -> Option<String> {
        self.folded
            .get(low)
            .cloned()
            .or_else(|| user.contains(low).then(|| low.to_string()))
    }

    /// Up to `limit` replacements for `word`, closest first, in the word's
    /// capitalization (Title or UPPER).
    pub fn suggest(&self, word: &str, user: &HashSet<String>, limit: usize) -> Vec<String> {
        if too_long(word) {
            return vec![];
        }
        let low = word.to_lowercase();
        let first = edits(&low);
        let mut found: Vec<(usize, String)> = vec![];
        let mut seen = HashSet::new();
        for e in &first {
            if let Some(w) = self.present(e, user) {
                if seen.insert(w.clone()) {
                    found.push((1, w));
                }
            }
        }
        if found.len() < limit && low.chars().count() <= 14 {
            let mut tried = 0;
            'outer: for e in &first {
                for e2 in edits(e) {
                    tried += 1;
                    if tried > MAX_SECOND_ORDER {
                        break 'outer;
                    }
                    if let Some(w) = self.present(&e2, user) {
                        if seen.insert(w.clone()) {
                            found.push((2, w));
                        }
                    }
                }
            }
        }
        let len = low.chars().count() as i64;
        let initial = low.chars().next();
        found.sort_by(|a, b| {
            let key = |(d, w): &(usize, String)| {
                (
                    *d,
                    // A word cut short is the likeliest slip.
                    !w.to_lowercase().starts_with(&low),
                    w.to_lowercase().chars().next() != initial,
                    (w.chars().count() as i64 - len).abs(),
                    w.to_lowercase(),
                )
            };
            key(a).cmp(&key(b))
        });
        found.truncate(limit);
        let caps = word
            .chars()
            .filter(|c| c.is_alphabetic())
            .all(char::is_uppercase)
            && word.chars().count() > 1;
        let title = word.chars().next().is_some_and(char::is_uppercase);
        found
            .into_iter()
            .map(|(_, w)| {
                if caps {
                    w.to_uppercase()
                } else if title {
                    let mut c = w.chars();
                    c.next()
                        .map(|f| f.to_uppercase().chain(c).collect())
                        .unwrap_or_default()
                } else {
                    w
                }
            })
            .collect()
    }
}

/// Every string one edit away: a deletion, a transposition, a replacement
/// or an insertion of a letter or apostrophe.
fn edits(word: &str) -> Vec<String> {
    const LETTERS: &str = "abcdefghijklmnopqrstuvwxyz'";
    let chars: Vec<char> = word.chars().collect();
    let n = chars.len();
    let mut out = Vec::with_capacity(54 * n + 27);
    let join = |v: &[char]| v.iter().collect::<String>();
    for i in 0..n {
        let mut v = chars.clone();
        v.remove(i);
        out.push(join(&v));
    }
    for i in 0..n.saturating_sub(1) {
        let mut v = chars.clone();
        v.swap(i, i + 1);
        out.push(join(&v));
    }
    for i in 0..n {
        for c in LETTERS.chars() {
            if c != chars[i] {
                let mut v = chars.clone();
                v[i] = c;
                out.push(join(&v));
            }
        }
    }
    for i in 0..=n {
        for c in LETTERS.chars() {
            let mut v = chars.clone();
            v.insert(i, c);
            out.push(join(&v));
        }
    }
    out
}

/// The built-in dictionary, decoded once.
pub fn builtin() -> &'static Dictionary {
    static D: OnceLock<Dictionary> = OnceLock::new();
    D.get_or_init(|| {
        let mut words = decode(WORDS);
        words.extend(
            SUPPLEMENT
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(String::from),
        );
        Dictionary::from_words(words)
    })
}

/// Byte ranges of the words in `text`: letters with apostrophes inside.
/// In notebook HTML, tags and character entities are skipped. A run of
/// letters stays one word however long it is (so Change replaces all of
/// it); callers look up and show only its first `MAX_WORD` characters.
pub fn words(text: &str, html: bool) -> Vec<(usize, usize)> {
    let mut out = vec![];
    let mut start: Option<usize> = None;
    let mut in_tag = false;
    let mut in_entity = false;
    let bytes: Vec<(usize, char)> = text.char_indices().collect();
    let close = |start: &mut Option<usize>, end: usize, out: &mut Vec<(usize, usize)>| {
        if let Some(s) = start.take() {
            let mut e = end;
            while e > s && text[..e].ends_with('\'') {
                e -= 1;
            }
            if e > s {
                out.push((s, e));
            }
        }
    };
    for (k, &(i, c)) in bytes.iter().enumerate() {
        if html {
            if in_tag {
                in_tag = c != '>';
                continue;
            }
            if in_entity {
                in_entity = c != ';' && (c.is_alphanumeric() || c == '#');
                continue;
            }
            if c == '<' {
                close(&mut start, i, &mut out);
                in_tag = true;
                continue;
            }
            if c == '&' {
                close(&mut start, i, &mut out);
                in_entity = true;
                continue;
            }
        }
        let next_is_letter = bytes.get(k + 1).is_some_and(|(_, n)| n.is_alphabetic());
        let part = c.is_alphabetic() || (c == '\'' && start.is_some() && next_is_letter);
        if part {
            if start.is_none() {
                start = Some(i);
            }
        } else {
            close(&mut start, i, &mut out);
        }
    }
    close(&mut start, text.len(), &mut out);
    // A token touching digits is a code (RB1000A), not a word.
    out.retain(|&(s, e)| {
        let before = text[..s].chars().next_back();
        let after = text[e..].chars().next();
        !before.is_some_and(|c| c.is_ascii_digit()) && !after.is_some_and(|c| c.is_ascii_digit())
    });
    out
}
