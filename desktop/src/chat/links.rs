//! Activity IDs in a reply become links: any word that is the Activity ID
//! of an activity in the open project, with the punctuation around it left
//! out (`PIP-3110.` or `(PIP-3110)`), and never part of a longer ID.
use crate::markdown::Span;
use std::collections::HashSet;

/// The link target of Activity ID `code`.
pub(crate) fn target(code: &str) -> String {
    format!("activity:{code}")
}

/// The Activity ID a link target names.
pub(crate) fn code_of(target: &str) -> Option<&str> {
    target.strip_prefix("activity:")
}

/// Characters that end a word: spaces and the brackets, quotes and marks
/// that do not appear inside an Activity ID.
fn breaks(c: char) -> bool {
    c.is_whitespace() || "()[]{}<>\"'`,;:!?*|".contains(c)
}

/// The Activity IDs in `text`: byte ranges of the words (less their
/// trailing full stops, hyphens or slashes) that are IDs in `codes`.
pub(crate) fn find(text: &str, codes: &HashSet<String>) -> Vec<(usize, usize)> {
    let mut out = vec![];
    if codes.is_empty() {
        return out;
    }
    let mut start = None;
    let word = |a: usize, b: usize, out: &mut Vec<(usize, usize)>| {
        let mut end = b;
        loop {
            let w = &text[a..end];
            if w.is_empty() {
                return;
            }
            if codes.contains(w) {
                out.push((a, end));
                return;
            }
            match w.chars().last() {
                Some(c @ ('.' | '-' | '/' | '_')) => end -= c.len_utf8(),
                _ => return,
            }
        }
    };
    for (i, c) in text.char_indices() {
        if breaks(c) {
            if let Some(a) = start.take() {
                word(a, i, &mut out);
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(a) = start {
        word(a, text.len(), &mut out);
    }
    out
}

/// `spans` with every Activity ID in their plain, bold and italic text
/// turned into a link (a link keeps the plain style).
pub(crate) fn link_ids(spans: Vec<Span>, codes: &HashSet<String>) -> Vec<Span> {
    if codes.is_empty() {
        return spans;
    }
    let mut out = vec![];
    for span in spans {
        let (text, wrap): (String, fn(String) -> Span) = match span {
            Span::Text(t) => (t, Span::Text),
            Span::Bold(t) => (t, Span::Bold),
            Span::Italic(t) => (t, Span::Italic),
            other => {
                out.push(other);
                continue;
            }
        };
        let mut at = 0;
        for (a, b) in find(&text, codes) {
            if a > at {
                out.push(wrap(text[at..a].to_owned()));
            }
            out.push(Span::Link {
                text: text[a..b].to_owned(),
                target: target(&text[a..b]),
            });
            at = b;
        }
        if at < text.len() {
            out.push(wrap(text[at..].to_owned()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(list: &[&str]) -> HashSet<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn words<'a>(text: &'a str, set: &HashSet<String>) -> Vec<&'a str> {
        find(text, set)
            .into_iter()
            .map(|(a, b)| &text[a..b])
            .collect()
    }

    #[test]
    fn ids_next_to_punctuation_are_found_without_it() {
        let set = codes(&["PIP-3110", "INS-5020", "A1"]);
        assert_eq!(
            words(
                "PIP-3110 drives (INS-5020), then A1. Also \"PIP-3110\"; PIP-3110's float, PIP-3110:",
                &set
            ),
            vec!["PIP-3110", "INS-5020", "A1", "PIP-3110", "PIP-3110", "PIP-3110"]
        );
        assert_eq!(
            words("PIP-3110- and PIP-3110/", &set),
            vec!["PIP-3110", "PIP-3110"]
        );
    }

    #[test]
    fn an_id_inside_a_longer_word_or_id_is_not_a_link() {
        let set = codes(&["A1", "A10", "PIP-31"]);
        assert_eq!(
            words("A10 and A1 and A100 and A1x", &set),
            vec!["A10", "A1"]
        );
        assert_eq!(words("PIP-3110 is not PIP-31", &set), vec!["PIP-31"]);
        // Case matters: IDs are matched as typed.
        assert_eq!(words("a1", &set), Vec::<&str>::new());
    }

    #[test]
    fn with_no_project_nothing_is_a_link() {
        let none = HashSet::new();
        assert!(find("PIP-3110", &none).is_empty());
        let spans = vec![Span::Text("PIP-3110".into())];
        assert_eq!(link_ids(spans.clone(), &none), spans);
    }

    #[test]
    fn links_keep_the_text_around_them_and_skip_code() {
        let set = codes(&["A1"]);
        assert_eq!(
            link_ids(
                vec![
                    Span::Text("see A1.".into()),
                    Span::Bold("A1".into()),
                    Span::Code("A1".into())
                ],
                &set
            ),
            vec![
                Span::Text("see ".into()),
                Span::Link {
                    text: "A1".into(),
                    target: "activity:A1".into()
                },
                Span::Text(".".into()),
                Span::Link {
                    text: "A1".into(),
                    target: "activity:A1".into()
                },
                Span::Code("A1".into()),
            ]
        );
        assert_eq!(code_of(&target("X-1")), Some("X-1"));
    }
}
