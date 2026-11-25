//! The engine sources state every rule as observable behavior and pin it with
//! a test (`engine/RULES.md`). This guard keeps two things out of them: the
//! words of a private list, and long hexadecimal numbers in comments.
//!
//! The word list is not published. It is the fixture `internal-refs.txt` in
//! the `OPENXER_FIXTURES` folder, one word per line (`#` starts a comment),
//! matched in any case. Without it the word check prints a SKIPPED line; the
//! hexadecimal check always runs.
use openxer_engine::fixture;
use std::{fs, path::Path};

fn sources(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Every engine source file with its text.
fn engine_sources() -> Vec<(std::path::PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![];
    sources(&root, &mut files);
    assert!(files.len() >= 8, "engine sources not found");
    files
        .into_iter()
        .map(|file| {
            let text = fs::read_to_string(&file).unwrap();
            (file, text)
        })
        .collect()
}

/// True when `text` holds `0x` followed by at least four hex digits.
fn has_address(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'0' && bytes[i + 1] == b'x' {
            let digits = bytes[i + 2..]
                .iter()
                .take_while(|b| b.is_ascii_hexdigit())
                .count();
            if digits >= 4 {
                return true;
            }
        }
        i += 1;
    }
    false
}

#[test]
fn engine_sources_contain_no_listed_word() {
    let Some(lines) = fixture::fixture_list("internal-refs.txt") else {
        return;
    };
    let words: Vec<String> = lines
        .iter()
        .map(|fields| fields.join(" ").to_ascii_lowercase())
        .collect();
    assert!(!words.is_empty(), "internal-refs.txt lists no words");
    let mut problems = vec![];
    for (file, text) in engine_sources() {
        for (number, line) in text.lines().enumerate() {
            let lower = line.to_ascii_lowercase();
            for word in &words {
                if lower.contains(word.as_str()) {
                    problems.push(format!(
                        "{}:{}: contains `{word}`",
                        file.display(),
                        number + 1
                    ));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn engine_comments_hold_no_long_hex_numbers() {
    let mut problems = vec![];
    for (file, text) in engine_sources() {
        for (number, line) in text.lines().enumerate() {
            if let Some(at) = line.find("//") {
                if has_address(&line[at..]) {
                    problems.push(format!(
                        "{}:{}: a comment holds a long hex number",
                        file.display(),
                        number + 1
                    ));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn the_hex_check_recognizes_long_hex_numbers() {
    assert!(has_address("// see 0xabcd1234"));
    assert!(has_address("/// bitmask 0x03aa"));
    assert!(!has_address("// the byte 0xff and 0x1a are short"));
    assert!(!has_address("// plain text"));
}
