use super::*;

const GOOD: &str = "---
id: my-check
name: My check
description: One line.
request: Run my check.
use_when: The user asks for my check.
kind: check
order: 5
examples:
- Run it  # a comment after the item
tools:
- openxer_load
---
# My check

Steps.
";

fn error(text: &str, stem: Option<&str>) -> String {
    parse(text, stem).unwrap_err().to_string()
}

#[test]
fn a_good_file_parses_with_every_field() {
    let s = parse(GOOD, Some("my-check")).unwrap();
    assert_eq!(s.id, "my-check");
    assert_eq!(s.name, "My check");
    assert_eq!(s.order, 5);
    assert_eq!(s.kind, Kind::Check);
    assert_eq!(s.examples, vec!["Run it"]);
    assert_eq!(s.tools, vec!["openxer_load"]);
    assert!(s.instructions.starts_with("# My check"));
    // A byte order mark and CRLF line ends are fine.
    let crlf = format!("\u{feff}{}", GOOD.replace('\n', "\r\n"));
    assert_eq!(parse(&crlf, Some("my-check")).unwrap(), s);
    // The text a skill writes parses back to the same skill.
    assert_eq!(parse(&s.to_text(), Some("my-check")).unwrap(), s);
}

#[test]
fn each_bad_case_names_its_line_and_problem() {
    let swap = |from: &str, to: &str| GOOD.replacen(from, to, 1);
    let cases: Vec<(String, &str)> = vec![
        (
            "no header".to_string(),
            "line 1: a skill file starts with a --- header line",
        ),
        (
            GOOD.replacen("---\n#", "\n#", 1),
            "the header has no closing --- line",
        ),
        (swap("name: My check\n", ""), "the header has no name"),
        (
            swap("request: Run my check.", "request:"),
            "line 5: request is empty",
        ),
        (
            swap("id: my-check", "id: My_Check"),
            "line 2: id \"My_Check\": use lower-case",
        ),
        (
            swap("id: my-check", "id: other"),
            "line 2: id \"other\" differs from its file name \"my-check\".md",
        ),
        (
            swap("name: My check", &format!("name: {}", "x".repeat(41))),
            "line 3: name is 41 characters; keep it to 40",
        ),
        (
            swap(
                "description: One line.",
                &format!("description: {}", "x".repeat(161)),
            ),
            "line 4: description is 161 characters",
        ),
        (
            swap("order: 5", "order: 1000"),
            "line 8: order \"1000\": give a whole number from 0 to 999",
        ),
        (
            swap("kind: check", "kind: report"),
            "line 7: kind \"report\": use check or workflow",
        ),
        (
            swap("kind: check", "colour: red"),
            "line 7: unknown key \"colour\"",
        ),
        (
            swap("kind: check", "Not a key line"),
            "line 7: expected 'key: value'",
        ),
        (
            swap("order: 5", "name: Again"),
            "line 8: name is given twice",
        ),
        (
            swap("tools:", "tools: openxer_load"),
            "line 11: tools is a list",
        ),
        (
            swap("order: 5", "- stray"),
            "line 8: a list item outside a list key",
        ),
        (
            GOOD.split("---\n#").next().unwrap().to_string() + "---\n",
            "has no instructions after its header",
        ),
        (
            swap("compare: x", "").replacen("kind: check", "compare: sometimes", 1),
            "line 7: compare \"sometimes\"",
        ),
    ];
    for (text, expected) in cases {
        let got = error(&text, Some("my-check"));
        assert!(
            got.starts_with(expected) || got.contains(expected),
            "{expected:?} not in {got:?}"
        );
    }
}

fn user_dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn write(dir: &Path, id: &str, text: &str) {
    std::fs::write(dir.join(format!("{id}.md")), text).unwrap();
}

#[test]
fn every_built_in_parses_names_real_tools_and_matches_the_folder() {
    let known = known_tools();
    let mut problems = vec![];
    for (id, text) in BUILT_IN {
        match parse(text, Some(id)) {
            Err(e) => problems.push(format!("{id}: {e}")),
            Ok(s) => {
                for t in unknown_tools(&s, &known) {
                    problems.push(format!("{id}: names {t}, not an OpenXER tool"));
                }
                if s.tools.is_empty() {
                    problems.push(format!("{id}: names no tools"));
                }
                if !s.cites.is_empty() {
                    problems.push(format!("{id}: a built-in carries no cites"));
                }
                let lines = text.lines().count();
                if lines > 160 {
                    problems.push(format!("{id}: {lines} lines"));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/skills");
    let mut files: Vec<String> = std::fs::read_dir(folder)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            e.file_name()
                .to_str()
                .and_then(|n| n.strip_suffix(".md"))
                .map(String::from)
        })
        .collect();
    files.sort();
    let listed: Vec<String> = BUILT_IN.iter().map(|(id, _)| id.to_string()).collect();
    assert_eq!(
        files, listed,
        "BUILT_IN lists every file in assets/skills, in name order"
    );
    assert_eq!(built_in().len(), BUILT_IN.len());
}

#[test]
fn the_catalog_is_ordered_by_order_then_id() {
    let dir = user_dir();
    write(
        dir.path(),
        "zz-first",
        &GOOD
            .replace("my-check", "zz-first")
            .replace("order: 5", "order: 0"),
    );
    write(
        dir.path(),
        "aa-last",
        &GOOD
            .replace("my-check", "aa-last")
            .replace("order: 5\n", ""),
    );
    let c = catalog_in(Some(dir.path()));
    assert_eq!(c.entries[0].skill.id, "zz-first");
    assert_eq!(c.entries.last().unwrap().skill.id, "aa-last");
    let orders: Vec<(u32, String)> = c
        .entries
        .iter()
        .map(|e| (e.skill.order, e.skill.id.clone()))
        .collect();
    let mut sorted = orders.clone();
    sorted.sort();
    assert_eq!(orders, sorted);
}

#[test]
fn a_user_file_customizes_a_built_in_and_deleting_it_restores_it() {
    let dir = user_dir();
    let base = catalog_in(Some(dir.path()));
    let original = base.get("look-ahead").unwrap();
    assert_eq!(original.source, Source::BuiltIn);
    let mut mine = original.skill.clone();
    mine.name = "Our look-ahead".into();
    write(dir.path(), "look-ahead", &mine.to_text());
    let c = catalog_in(Some(dir.path()));
    let e = c.get("look-ahead").unwrap();
    assert_eq!(e.source, Source::Customized);
    assert_eq!(e.skill.name, "Our look-ahead");
    assert_eq!(c.entries.len(), base.entries.len());
    std::fs::remove_file(dir.path().join("look-ahead.md")).unwrap();
    let back = catalog_in(Some(dir.path()));
    assert_eq!(back.get("look-ahead").unwrap().source, Source::BuiltIn);
    assert_eq!(
        back.get("look-ahead").unwrap().skill.name,
        original.skill.name
    );
}

#[test]
fn an_invalid_user_file_is_skipped_and_reported() {
    let dir = user_dir();
    write(dir.path(), "broken", "no header here");
    write(dir.path(), "my-check", GOOD);
    write(
        dir.path(),
        "odd-tool",
        &GOOD
            .replace("my-check", "odd-tool")
            .replace("openxer_load", "made_up_tool"),
    );
    std::fs::write(dir.path().join("notes.txt"), "not a skill").unwrap();
    let c = catalog_in(Some(dir.path()));
    assert!(c.get("broken").is_none());
    assert_eq!(c.problems.len(), 1, "{:?}", c.problems);
    assert!(c.problems[0].path.ends_with("broken.md"));
    assert!(c.problems[0].error.contains("line 1"));
    assert_eq!(c.get("my-check").unwrap().source, Source::Yours);
    // An unknown tool loads with a warning.
    let odd = c.get("odd-tool").unwrap();
    assert_eq!(
        odd.warnings,
        vec!["tools: made_up_tool is not an OpenXER tool"]
    );
    // The built-ins are all still there.
    assert!(c.entries.len() >= BUILT_IN.len() + 2);
    // A missing folder is an empty one.
    assert_eq!(
        catalog_in(Some(&dir.path().join("none"))).entries.len(),
        BUILT_IN.len()
    );
}

#[test]
fn the_template_parses_and_free_ids_avoid_taken_ones() {
    let dir = user_dir();
    let id = free_id(dir.path(), "my-skill");
    assert_eq!(id, "my-skill");
    let text = template(&id);
    let s = parse(&text, Some(&id)).unwrap();
    assert!(unknown_tools(&s, &known_tools()).is_empty());
    write_user(dir.path(), &id, &text, false).unwrap();
    assert!(write_user(dir.path(), &id, &text, false).is_err());
    assert_eq!(free_id(dir.path(), "my-skill"), "my-skill-2");
    assert_eq!(free_id(dir.path(), "look-ahead"), "look-ahead-2");
    assert_eq!(free_id(dir.path(), "Bad Id"), "my-skill-2");
}

#[test]
fn the_cites_of_a_user_skill_are_never_sent_to_an_agent() {
    let dir = user_dir();
    write(
        dir.path(),
        "my-check",
        &GOOD.replace("tools:", "cites:\n- Some Book, chapter 3\ntools:"),
    );
    let c = catalog_in(Some(dir.path()));
    let e = c.get("my-check").unwrap();
    assert_eq!(e.skill.cites, vec!["Some Book, chapter 3"]);
    for v in [summary_json(e), detail_json(e)] {
        assert!(!v.to_string().contains("Some Book"), "{v}");
    }
    assert!(!prompt_text(&e.skill).contains("Some Book"));
}

/// Words that would say where a built-in skill came from. The pattern list
/// stays in this test: a standards body's acronym and name, the numbering of
/// recommended practices (two or three digits, R-, two digits), the other
/// agencies' acronyms, the earlier tool and other products.
#[test]
fn no_built_in_says_where_it_came_from() {
    let words = [
        "aace",
        "advancement of cost engineering",
        "dcma",
        "gao",
        "pmi",
        "project management institute",
        "nasa",
        "recommended practice",
        "sidecar",
        "primavera",
        "oracle",
        "p6",
        "f9",
        "ported",
        "adapted from",
        "based on the",
        "according to",
    ];
    let mut problems = vec![];
    for (id, text) in BUILT_IN {
        let lower = text.to_lowercase();
        for w in words {
            let found = lower.match_indices(w).any(|(i, _)| {
                let before = lower[..i].chars().next_back();
                let after = lower[i + w.len()..].chars().next();
                let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric());
                !word(before) && !word(after)
            });
            if found {
                problems.push(format!("{id}: {w}"));
            }
        }
        // 24R-03, 105R-19 ...
        let b = text.as_bytes();
        for i in 0..b.len() {
            let digits = b[..i]
                .iter()
                .rev()
                .take_while(|c| c.is_ascii_digit())
                .count();
            if (2..=3).contains(&digits)
                && b[i..].starts_with(b"R-")
                && b.get(i + 2).is_some_and(u8::is_ascii_digit)
                && b.get(i + 3).is_some_and(u8::is_ascii_digit)
            {
                problems.push(format!("{id}: a practice number at byte {i}"));
            }
        }
        for dash in ['\u{2013}', '\u{2014}'] {
            if text.contains(dash) {
                problems.push(format!("{id}: an en or em dash"));
            }
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

/// The private list the engine guard uses (`internal-refs.txt` in
/// `OPENXER_FIXTURES`), read through the same loader; skipped with a printed
/// line when it is absent.
#[test]
fn no_built_in_holds_a_word_of_the_private_list() {
    let Some(lines) = openxer_engine::fixture::fixture_list("internal-refs.txt") else {
        println!(
            "SKIPPED no_built_in_holds_a_word_of_the_private_list: internal-refs.txt not found"
        );
        return;
    };
    let words: Vec<String> = lines
        .iter()
        .map(|f| f.join(" ").to_ascii_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    let mut problems = vec![];
    for (id, text) in BUILT_IN {
        let lower = text.to_ascii_lowercase();
        for w in &words {
            if lower.contains(w.as_str()) {
                problems.push(format!("{id}: contains a listed word"));
            }
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}
