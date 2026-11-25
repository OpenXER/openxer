//! The help content and its wiring: every topic is well formed and in the
//! contents, every link and every dialog or view context lands on a topic of
//! its own, and F1, Ctrl+F1, the Help buttons, search, Back and Forward work
//! through the real UI.
use super::*;
use crate::dialog_sweep::SMALL_SCREEN;
use crate::qa_sweep::Driver;
use std::collections::BTreeSet;
use std::path::PathBuf;

fn help_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("help")
}

fn src_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// The words of the private list `internal-refs.txt` in `OPENXER_FIXTURES`
/// (the list `engine/tests/no_internal_refs.rs` checks the engine against),
/// lowercased; `None` after a SKIPPED line when the list is not available.
fn listed_words() -> Option<Vec<String>> {
    let lines = openxer_engine::fixture::fixture_list("internal-refs.txt")?;
    let words: Vec<String> = lines
        .iter()
        .map(|fields| fields.join(" ").to_lowercase())
        .collect();
    assert!(!words.is_empty(), "internal-refs.txt lists no words");
    Some(words)
}

fn starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

#[test]
fn every_topic_is_well_formed() {
    let lib = library();
    assert!(lib.topics.len() >= 90, "{} topics", lib.topics.len());
    let mut ids = BTreeSet::new();
    for t in &lib.topics {
        assert!(ids.insert(t.id), "{} is listed twice", t.id);
        assert!(!t.title.is_empty(), "{} has no # title line", t.id);
        assert!(t.blocks.len() >= 3, "{} is too short to help anyone", t.id);
        assert!(
            !t.text.contains("placeholder") && !t.text.contains("todo"),
            "{} is not written yet",
            t.id
        );
    }
}

#[test]
fn every_help_file_is_compiled_in_and_follows_the_text_rules() {
    let lib = library();
    let words = listed_words();
    let mut files = BTreeSet::new();
    for entry in std::fs::read_dir(help_dir()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let Some(stem) = name.strip_suffix(".md") else {
            panic!("{name}: only .md files belong in desktop/help");
        };
        let text = std::fs::read_to_string(&path).unwrap();
        for (n, line) in text.lines().enumerate() {
            assert!(
                !line.contains('\u{2014}') && !line.contains('\u{2013}'),
                "{name}:{}: em or en dash",
                n + 1
            );
            let lower = line.to_lowercase();
            for word in words.iter().flatten() {
                assert!(
                    !lower.contains(word.as_str()),
                    "{name}:{}: help text contains the listed word `{word}`",
                    n + 1
                );
            }
        }
        if stem != "contents" {
            files.insert(stem.to_owned());
        }
    }
    // What's New is made from CHANGELOG.md, not a help file.
    let compiled: BTreeSet<String> = lib
        .topics
        .iter()
        .map(|t| t.id.to_owned())
        .filter(|id| id != WHATS_NEW)
        .collect();
    assert_eq!(
        files, compiled,
        "help files and the compiled topic list differ"
    );
}

#[test]
fn every_topic_is_in_the_contents_once() {
    let lib = library();
    let mut seen = BTreeSet::new();
    for section in &lib.contents {
        assert!(!section.topics.is_empty(), "{} is empty", section.title);
        for id in &section.topics {
            assert!(lib.topic(id).is_some(), "contents lists unknown topic {id}");
            assert!(seen.insert(id.clone()), "{id} is in the contents twice");
        }
    }
    for t in &lib.topics {
        assert!(
            seen.contains(t.id),
            "{} is not reachable from the contents",
            t.id
        );
    }
}

#[test]
fn every_link_lands_on_a_topic() {
    let lib = library();
    for t in &lib.topics {
        for b in &t.blocks {
            let spans = match b {
                Block::Para(s) | Block::Bullet(s) | Block::Step(_, s) | Block::Note(s) => s,
                Block::Heading(_) | Block::Table(_) => continue,
            };
            for s in spans {
                if let Span::Link { target, .. } = s {
                    assert!(
                        lib.topic(target).is_some(),
                        "{}: link to unknown {target}",
                        t.id
                    );
                }
            }
        }
    }
}

/// Every dialog and view context resolves to a topic, and no dialog with a
/// Help button lands on the overview or on a view's page.
#[test]
fn every_context_has_a_topic_of_its_own() {
    let lib = library();
    let mut names = BTreeSet::new();
    for (name, topic) in CONTEXTS {
        assert!(names.insert(*name), "{name} is mapped twice");
        assert!(lib.topic(topic).is_some(), "{name} maps to unknown {topic}");
        assert_ne!(*topic, OVERVIEW, "{name} falls through to the overview");
    }
    use views::MainView as V;
    let report = crate::reports::report_tab::ReportTabId(1);
    for v in [
        V::Activities,
        V::Activities2,
        V::Wbs,
        V::Projects,
        V::Resources,
        V::Reports,
        V::Assignments,
        V::Expenses,
        V::Documents,
        V::Risks,
        V::Issues,
        V::Thresholds,
        V::Report(report),
    ] {
        let t = view_topic(v);
        assert!(lib.topic(t).is_some(), "{v:?} maps to unknown {t}");
        assert!(t.starts_with("view-"), "{v:?} opens {t}, not a view topic");
    }
}

/// Every `open_help_for("...")` in the source names a dialog the table
/// knows, and no Help button still sets the old one-page flag.
#[test]
fn every_help_button_in_the_source_has_a_topic() {
    fn visit(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                out.push((path.display().to_string(), text));
            }
        }
    }
    let mut files = vec![];
    visit(&src_dir(), &mut files);
    let mut calls = 0;
    for (path, text) in &files {
        if path.ends_with("tests.rs") {
            continue;
        }
        assert!(!text.contains("show_help"), "{path} still uses show_help");
        for (at, _) in text.match_indices("open_help_for(\"") {
            let rest = &text[at + "open_help_for(\"".len()..];
            let name = &rest[..rest.find('"').unwrap()];
            assert!(
                dialog_topic(name).is_some(),
                "{path}: open_help_for(\"{name}\") has no topic"
            );
            calls += 1;
        }
    }
    assert!(calls >= 60, "only {calls} Help buttons found");
}

#[test]
fn markup_parses_into_blocks_and_spans() {
    let t = parse(
        "x",
        "# Title\n\nSome **bold** and `F9` and [a link](overview).\n\n## Steps\n1. First\n   continued\n2. Second\n- Bullet\n> Careful\n",
    );
    assert_eq!(t.title, "Title");
    assert_eq!(
        t.blocks[0],
        Block::Para(vec![
            Span::Text("Some ".into()),
            Span::Bold("bold".into()),
            Span::Text(" and ".into()),
            Span::Code("F9".into()),
            Span::Text(" and ".into()),
            Span::Link {
                text: "a link".into(),
                target: "overview".into()
            },
            Span::Text(".".into()),
        ])
    );
    assert_eq!(t.blocks[1], Block::Heading("Steps".into()));
    assert_eq!(
        t.blocks[2],
        Block::Step(1, vec![Span::Text("First continued".into())])
    );
    assert!(matches!(t.blocks[3], Block::Step(2, _)));
    assert!(matches!(t.blocks[4], Block::Bullet(_)));
    assert!(matches!(t.blocks[5], Block::Note(_)));
}

#[test]
fn search_finds_topics_by_every_word_titles_first() {
    let lib = library();
    let hits = lib.search("data date");
    assert!(!hits.is_empty());
    assert!(hits
        .iter()
        .all(|t| t.text.contains("data") && t.text.contains("date")));
    let hits = lib.search("baseline");
    assert!(
        hits[0].title.to_lowercase().contains("baseline"),
        "{}",
        hits[0].title
    );
    assert!(lib.search("zzqx nothing").is_empty());
    assert!(lib.search("  ").is_empty());
}

#[test]
fn back_and_forward_walk_the_history() {
    let mut h = HelpState::default();
    h.go(OVERVIEW);
    h.go("schedule");
    h.go("calendars");
    h.back();
    assert_eq!(h.topic, "schedule");
    h.back();
    assert_eq!(h.topic, OVERVIEW);
    h.forward();
    assert_eq!(h.topic, "schedule");
    h.go("bars");
    assert!(h.forward.is_empty(), "a new topic drops the forward list");
    assert_eq!(h.back, vec![OVERVIEW.to_owned(), "schedule".to_owned()]);
}

// ------------------------------------------------------------ through the UI

fn driver(name: &str) -> Driver {
    let mut d = Driver::probed(name, starter(), SMALL_SCREEN, None);
    d.frames(3);
    d.home();
    d
}

#[test]
fn ctrl_f1_and_help_topics_open_the_overview() {
    let mut d = driver("help-ctrl-f1");
    d.key_with(Key::F1, egui::Modifiers::COMMAND);
    d.frames(2);
    assert!(d.app.help.open);
    assert_eq!(d.app.help.topic, OVERVIEW);
    assert!(d.has("help_toc_getting-started"), "the contents show");
    d.click("help_close");
    assert!(!d.app.help.open);
    d.click("menu_bar_help");
    d.click("menu_local_help");
    d.frames(2);
    assert_eq!(d.app.help.topic, OVERVIEW);
    d.key(Key::Escape);
    d.frames(2);
    assert!(!d.app.help.open, "Escape closes Help");
}

#[test]
fn f1_opens_the_current_view_topic() {
    let mut d = driver("help-f1-view");
    d.key(Key::F1);
    d.frames(2);
    assert!(d.app.help.open);
    assert_eq!(d.app.help.topic, "view-activities");
    d.key(Key::Escape);
    d.click("directory_wbs");
    d.frames(2);
    d.key(Key::F1);
    d.frames(2);
    assert_eq!(d.app.help.topic, "view-wbs");
}

#[test]
fn alt_f1_still_toggles_cell_tips_and_opens_no_help() {
    let mut d = driver("help-alt-f1");
    let before = d.app.grid.layout.table.hint_help;
    d.key_with(Key::F1, egui::Modifiers::ALT);
    d.frames(2);
    assert_ne!(
        d.app.grid.layout.table.hint_help, before,
        "Alt+F1 is Cell Tips"
    );
    assert!(!d.app.help.open, "Alt+F1 is not help");
}

/// Page Setup is modal: F1 and its Help button open the same topic of its
/// own, Help sits above it and Escape closes Help but leaves Page Setup.
#[test]
fn f1_and_the_help_button_agree_over_a_modal_dialog() {
    let mut d = driver("help-modal");
    d.click("menu_bar_file");
    d.click("menu_page_setup");
    d.frames(3);
    d.key(Key::F1);
    d.frames(3);
    assert!(d.app.help.open);
    assert_eq!(d.app.help.topic, "page-setup");
    d.key(Key::Escape);
    d.frames(3);
    assert!(!d.app.help.open, "Escape closes Help first");
    assert!(d.has("page_setup_ok"), "Page Setup is still open");
    d.click("page_setup_help");
    d.frames(3);
    assert_eq!(d.app.help.topic, "page-setup");
    let size = |d: &Driver| {
        d.probe
            .snaps
            .iter()
            .find(|s| s.name == WINDOW_TITLE)
            .map(|s| s.area.size())
            .expect("the Help window is drawn")
    };
    let before = size(&d);
    d.frames(4);
    assert_eq!(size(&d), before, "the Help window keeps its size");
    assert!(before.x < 1000. && before.y < 600., "{before:?}");
    d.click("help_close");
    assert!(!d.app.help.open);
    assert!(d.has("page_setup_ok"), "Close leaves Page Setup open");
}

/// A non-modal dialog (Columns): F1 opens its topic, not the view's.
#[test]
fn f1_over_a_kit_window_opens_its_topic() {
    let mut d = driver("help-columns");
    d.app.open_columns_dialog();
    d.frames(3);
    d.key(Key::F1);
    d.frames(2);
    assert_eq!(d.app.help.topic, "columns");
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.has("columns_help"), "Columns stays open");
}

/// The four Help buttons that used to be drawn disabled now open topics.
#[test]
fn the_formerly_disabled_help_buttons_work() {
    type Case = (&'static str, fn(&mut App), &'static str, &'static str);
    let cases: [Case; 4] = [
        (
            "admin_help",
            |a| a.prefs.admin.open = true,
            "admin-preferences",
            "admin_close",
        ),
        (
            "prefs_help",
            |a| a.open_user_preferences(),
            "user-preferences",
            "prefs_close",
        ),
        (
            "monitor_help",
            |a| a.open_monitor_thresholds(),
            "monitor-thresholds",
            "monitor_cancel",
        ),
        (
            "navigator_help",
            |a| a.open_issue_navigator(),
            "issue-navigator",
            "navigator_close",
        ),
    ];
    for (key, open, topic, close) in cases {
        let mut d = driver("help-enabled");
        open(&mut d.app);
        d.frames(3);
        assert!(d.has(key), "{key} is drawn");
        d.click(key);
        d.frames(2);
        assert!(d.app.help.open, "{key} opens Help");
        assert_eq!(d.app.help.topic, topic, "{key}");
        d.click("help_close");
        assert!(d.has(close), "{key}: the dialog stays open");
    }
}

#[test]
fn search_links_and_history_work_in_the_window() {
    let mut d = driver("help-search");
    d.app.open_help_contents();
    d.frames(2);
    d.click("help_toc_scheduling-basics");
    d.frames(2);
    assert_eq!(d.app.help.topic, "scheduling-basics");
    d.click("help_link_schedule");
    d.frames(2);
    assert_eq!(d.app.help.topic, "schedule");
    d.click("help_back");
    d.frames(2);
    assert_eq!(d.app.help.topic, "scheduling-basics");
    d.click("help_forward");
    d.frames(2);
    assert_eq!(d.app.help.topic, "schedule");
    d.click("help_search");
    d.type_text("baseline");
    d.frames(2);
    assert!(d.has("help_result_0"), "results are listed");
    assert!(!d.has("help_toc_overview"), "results replace the contents");
    d.key(Key::Enter);
    d.frames(2);
    assert!(d.app.help.open, "Enter in the search field keeps Help open");
    d.click("help_result_0");
    d.frames(2);
    assert!(
        d.app.help.topic.contains("baseline"),
        "{}",
        d.app.help.topic
    );
    d.click("help_contents");
    d.frames(2);
    assert_eq!(d.app.help.topic, OVERVIEW);
    assert!(d.has("help_toc_overview"));
}

#[test]
fn whats_new_shows_the_changelog_newest_release_first() {
    let lib = library();
    let t = lib.topic(WHATS_NEW).expect("What's New is compiled in");
    assert_eq!(t.title, "What's New");
    let headings: Vec<&str> = t
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Heading(h) => Some(h.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(headings.first(), Some(&"Unreleased"), "{headings:?}");
    let bullets = t
        .blocks
        .iter()
        .filter(|b| matches!(b, Block::Bullet(_)))
        .count();
    assert!(bullets >= 20, "{bullets} bullets");
    // Kinds of change are bold lines, and links to the web keep their text.
    assert!(t
        .blocks
        .iter()
        .any(|b| matches!(b, Block::Para(s) if s == &vec![Span::Bold("Added".into())])));
    assert!(!t.text.contains("https://"));
    // The changelog follows the help text rules.
    let changelog = std::fs::read_to_string(src_dir().join("../../CHANGELOG.md")).unwrap();
    // The private list's words, and the AI clients, which the app names in
    // its AI Agents dialog but release notes do not.
    let mut words = listed_words().unwrap_or_default();
    words.extend(["claude".to_owned(), "codex".to_owned()]);
    for (n, line) in changelog.lines().enumerate() {
        assert!(
            !line.contains('\u{2014}') && !line.contains('\u{2013}'),
            "CHANGELOG.md:{}: em or en dash",
            n + 1
        );
        let lower = line.to_lowercase();
        for word in &words {
            assert!(
                !lower.contains(word.as_str()),
                "CHANGELOG.md:{}: contains the listed word `{word}`",
                n + 1
            );
        }
    }
    // Releases in order: a second release goes under the first.
    let two = whats_new_source(
        "# Changelog\n\nIntro [link](https://x.y).\n\n## [0.2.0] - 2027-01-01\n### Fixed\n- B\n\n## [0.1.0] - 2026-12-01\n### Added\n- A, see [notes](https://x.y/n)\n\n[0.2.0]: https://x.y/0.2.0\n",
    );
    let topic = parse("two", &two);
    let order: Vec<String> = topic.blocks.iter().map(block_text).collect();
    assert_eq!(
        order,
        [
            "The changes in each release of OpenXER, the newest release first.",
            "0.2.0 - 2027-01-01",
            "Fixed",
            "B",
            "0.1.0 - 2026-12-01",
            "Added",
            "A, see notes",
        ]
    );
}

#[test]
fn help_whats_new_and_its_toolbar_button_open_the_release_notes() {
    let mut d = driver("help-whats-new");
    d.click("menu_bar_help");
    d.click("menu_whats_new");
    d.frames(2);
    assert!(d.app.help.open);
    assert_eq!(d.app.help.topic, WHATS_NEW);
    assert!(d.has("help_toc_whats-new"), "listed in the contents");
    d.key(Key::Escape);
    d.frames(2);
    assert!(!d.app.help.open);
    d.app.set_button_shown("tool_whats_new", true);
    d.frames(3);
    assert!(d.has("tool_whats_new"));
    d.click("tool_whats_new");
    d.frames(2);
    assert!(d.app.help.open);
    assert_eq!(d.app.help.topic, WHATS_NEW);
}

/// Renders of the help window to the ignored `artifacts/help/`: the
/// contents, a dialog topic over its dialog, and search results.
#[test]
fn render_help_window() {
    use crate::headless_render::SoftwarePainter;
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/help");
    std::fs::create_dir_all(&out).unwrap();
    let path = std::env::temp_dir().join(format!("openxer-help-render-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let project = starter();
    let mut store = Store::open(path.clone()).unwrap();
    store.save(&project).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    let mut time = 0.;
    let mut painter = SoftwarePainter::default();
    painter.screen = Some(SMALL_SCREEN);
    let mut shot = |app: &mut App, name: &str| {
        for _ in 0..3 {
            let _ = painter.frame(app, &ctx, &mut time, vec![], false);
        }
        let (image, triangles) = painter.frame(app, &ctx, &mut time, vec![], true).unwrap();
        assert!(triangles > 300, "{name}");
        image.save(out.join(format!("{name}.png"))).unwrap();
    };
    app.open_help_contents();
    shot(&mut app, "help-contents");
    app.close_help();
    app.open_page_setup();
    app.open_help_for("page_setup");
    shot(&mut app, "help-page-setup-topic");
    app.close_help();
    app.print.dialog = None;
    app.help.go("scheduling-basics");
    shot(&mut app, "help-scheduling-basics");
    app.help.query = "baseline".into();
    shot(&mut app, "help-search-results");
    app.help.query.clear();
    app.open_whats_new();
    shot(&mut app, "help-whats-new");
    let _ = std::fs::remove_file(&path);
}

// --------------------------------------- keys and menu paths the help names

/// Every `code` span of every topic, with its topic.
fn code_spans() -> Vec<(&'static str, String)> {
    let mut out = vec![];
    for t in &library().topics {
        for b in &t.blocks {
            let spans = match b {
                Block::Para(s) | Block::Bullet(s) | Block::Step(_, s) | Block::Note(s) => s,
                Block::Heading(_) | Block::Table(_) => continue,
            };
            for s in spans {
                if let Span::Code(c) = s {
                    out.push((t.id, c.clone()));
                }
            }
        }
    }
    out
}

const MODIFIERS: [&str; 3] = ["Ctrl", "Shift", "Alt"];

/// Key names the help may use, as written.
const KEY_NAMES: [&str; 18] = [
    "Tab",
    "Insert",
    "Delete",
    "Enter",
    "Escape",
    "Esc",
    "Home",
    "End",
    "Page Up",
    "Page Down",
    "Left",
    "Right",
    "Up",
    "Down",
    "Space",
    "Backspace",
    "Menu",
    "F",
];

/// Whether a code span names a key or key combination, such as `F9`,
/// `Shift+F9`, `Ctrl+Z`, `Page Up` or a bare `Ctrl`. A lone letter is not a
/// key (`A` is the actual-date mark).
fn is_shortcut(s: &str) -> bool {
    let parts: Vec<&str> = s.split('+').collect();
    let (key, mods) = parts.split_last().expect("split gives one part");
    if !mods.iter().all(|m| MODIFIERS.contains(m)) {
        return false;
    }
    if mods.is_empty() && MODIFIERS.contains(key) {
        return true;
    }
    let function = key
        .strip_prefix('F')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=12).contains(&n));
    let letter = key.len() == 1 && key.chars().all(|c| c.is_ascii_uppercase());
    function || (KEY_NAMES.contains(key) && *key != "F") || (letter && !mods.is_empty())
}

#[test]
fn shortcut_spans_are_recognized() {
    for s in [
        "F9",
        "Shift+F9",
        "Ctrl+Z",
        "Ctrl+Shift+Tab",
        "Page Up",
        "Ctrl",
        "Alt+Left",
    ] {
        assert!(is_shortcut(s), "{s}");
    }
    for s in [
        "A",
        " A",
        "Tools > Schedule ...",
        "%xer%",
        "SCHEDLOG.TXT",
        "2d",
        "F99",
    ] {
        assert!(!is_shortcut(s), "{s}");
    }
}

/// The Activities view of the starter project with activity 0 selected.
fn on_activities(name: &str) -> Driver {
    let mut d = driver(name);
    d.select(0);
    d
}

fn dialog_open(d: &Driver, name: &str) -> bool {
    crate::dialog_kit::open_dialogs(&d.ctx)
        .iter()
        .any(|n| n == name)
}

fn press(d: &mut Driver, s: &str) {
    let parts: Vec<&str> = s.split('+').collect();
    let (key, mods) = parts.split_last().unwrap();
    let mut m = egui::Modifiers::NONE;
    for x in mods {
        match *x {
            "Ctrl" => m |= egui::Modifiers::COMMAND,
            "Shift" => m |= egui::Modifiers::SHIFT,
            "Alt" => m |= egui::Modifiers::ALT,
            _ => unreachable!(),
        }
    }
    let key = match *key {
        "Page Up" => Key::PageUp,
        "Page Down" => Key::PageDown,
        "Left" => Key::ArrowLeft,
        "Right" => Key::ArrowRight,
        k => Key::from_name(k).unwrap_or_else(|| panic!("no key {k}")),
    };
    d.key_with(key, m);
    d.frames(2);
}

fn activity_count(d: &Driver) -> usize {
    d.app.project.model.activities.len()
}

/// What each key the help names does, checked through the real UI; a key
/// that is not here fails the test until it is added with its check.
enum Check {
    /// Press the key in a fresh driver and check its effect.
    Run(fn(&str) -> Result<(), String>),
    /// Keys whose behavior other tests already pin, with where.
    Covered(&'static str),
}

fn expect(ok: bool, what: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(what.to_owned())
    }
}

fn opens_dialog(name: &'static str) -> impl Fn(&str) -> Result<(), String> {
    move |key| {
        let mut d = on_activities("help-key");
        press(&mut d, key);
        expect(
            dialog_open(&d, name),
            &format!(
                "does not open {name}; open: {:?}",
                crate::dialog_kit::open_dialogs(&d.ctx)
            ),
        )
    }
}

fn shortcut_checks() -> Vec<(&'static str, Check)> {
    use Check::*;
    vec![
        (
            "F1",
            Run(|k| {
                let mut d = driver("help-key-f1");
                press(&mut d, k);
                expect(
                    d.app.help.open && d.app.help.topic == "view-activities",
                    "F1",
                )
            }),
        ),
        (
            "Ctrl+F1",
            Run(|k| {
                let mut d = driver("help-key-cf1");
                press(&mut d, k);
                expect(d.app.help.open && d.app.help.topic == OVERVIEW, "overview")
            }),
        ),
        (
            "Alt+F1",
            Run(|k| {
                let mut d = driver("help-key-af1");
                let before = d.app.grid.layout.table.hint_help;
                press(&mut d, k);
                expect(d.app.grid.layout.table.hint_help != before, "Cell Tips")
            }),
        ),
        (
            "Alt+Left",
            Run(|k| {
                let mut d = driver("help-key-back");
                d.app.help.go("schedule");
                d.app.help.go("calendars");
                d.frames(2);
                press(&mut d, k);
                expect(d.app.help.topic == "schedule", "Back")
            }),
        ),
        (
            "Alt+Right",
            Run(|k| {
                let mut d = driver("help-key-fwd");
                d.app.help.go("schedule");
                d.app.help.go("calendars");
                d.app.help.back();
                d.frames(2);
                press(&mut d, k);
                expect(d.app.help.topic == "calendars", "Forward")
            }),
        ),
        ("Ctrl+N", Run(|k| opens_dialog("New Project")(k))),
        ("Ctrl+O", Run(|k| opens_dialog("open_project")(k))),
        ("Ctrl+F", Run(|k| opens_dialog("Find")(k))),
        ("Ctrl+H", Run(|k| opens_dialog("Replace")(k))),
        ("Ctrl+R", Run(|k| opens_dialog("Replace")(k))),
        ("F7", Run(|k| opens_dialog("spell")(k))),
        ("F9", Run(|k| opens_dialog("schedule")(k))),
        ("Shift+F9", Run(|k| opens_dialog("level")(k))),
        ("F8", Run(|k| opens_dialog(crate::chat::SALT)(k))),
        (
            "Shift+Enter",
            Covered("chat::tests: Shift+Enter starts a new line in Chat's message box"),
        ),
        (
            "Ctrl+W",
            Run(|k| {
                let mut d = on_activities("help-key-w");
                press(&mut d, k);
                expect(
                    d.app.status == "Only the default project is open",
                    &d.app.status,
                )
            }),
        ),
        (
            "Ctrl+S",
            Run(|k| {
                let mut d = on_activities("help-key-s");
                d.app.dirty = true;
                press(&mut d, k);
                expect(!d.app.dirty, "saves")
            }),
        ),
        (
            "F10",
            Run(|k| {
                let mut d = on_activities("help-key-f10");
                d.app.dirty = true;
                press(&mut d, k);
                expect(!d.app.dirty, "saves")
            }),
        ),
        (
            "Ctrl+P",
            Run(|k| {
                let mut d = on_activities("help-key-p");
                press(&mut d, k);
                expect(d.app.print.last_pdf.is_some(), "prints")
            }),
        ),
        (
            "F5",
            Run(|k| {
                let mut d = on_activities("help-key-f5");
                press(&mut d, k);
                expect(d.app.status == "View refreshed", &d.app.status)
            }),
        ),
        (
            "Insert",
            Run(|k| {
                let mut d = on_activities("help-key-ins");
                let n = activity_count(&d);
                press(&mut d, k);
                expect(activity_count(&d) == n + 1, "adds an activity")
            }),
        ),
        (
            "Delete",
            Run(|k| {
                let mut d = on_activities("help-key-del");
                press(&mut d, k);
                expect(d.app.delete_confirm.is_some(), "asks to delete")
            }),
        ),
        (
            "F2",
            Run(|k| {
                let mut d = on_activities("help-key-f2");
                press(&mut d, k);
                expect(d.app.edit_cell.is_some(), "edits the cell")
            }),
        ),
        (
            "Ctrl+Z",
            Run(|k| {
                let mut d = on_activities("help-key-z");
                let n = activity_count(&d);
                press(&mut d, "Insert");
                // The new activity's name editor holds the keys until Escape.
                press(&mut d, "Escape");
                press(&mut d, k);
                expect(activity_count(&d) == n, "undoes")
            }),
        ),
        (
            "Ctrl+Y",
            Run(|k| {
                let mut d = on_activities("help-key-y");
                let n = activity_count(&d);
                press(&mut d, "Insert");
                press(&mut d, "Escape");
                press(&mut d, "Ctrl+Z");
                press(&mut d, k);
                expect(activity_count(&d) == n + 1, "redoes")
            }),
        ),
        (
            "Ctrl+E",
            Run(|k| {
                let mut d = on_activities("help-key-e");
                press(&mut d, k);
                expect(d.app.status.contains("fill down"), &d.app.status)
            }),
        ),
        (
            "F3",
            Run(|k| {
                let mut d = on_activities("help-key-f3");
                let code = d.app.project.model.activities[1].task_code.clone();
                d.app.grid.dialogs.last_find = Some(crate::grid::dialogs::FindState {
                    what: code,
                    with: String::new(),
                    column: None,
                    match_case: false,
                    replace: false,
                });
                press(&mut d, k);
                expect(d.app.selected == Some(1), "finds the next match")
            }),
        ),
        (
            "Ctrl+Tab",
            Run(|k| {
                let mut d = driver("help-key-tab");
                d.click("directory_wbs");
                d.click("directory_activities");
                press(&mut d, k);
                expect(
                    d.app.views.groups.current() != views::MainView::Activities,
                    "next tab",
                )
            }),
        ),
        (
            "Ctrl+Shift+Tab",
            Run(|k| {
                let mut d = driver("help-key-stab");
                d.click("directory_wbs");
                d.click("directory_activities");
                press(&mut d, k);
                expect(
                    d.app.views.groups.current() != views::MainView::Activities,
                    "previous tab",
                )
            }),
        ),
        (
            "Ctrl+F4",
            Run(|k| {
                let mut d = driver("help-key-f4");
                d.click("directory_wbs");
                press(&mut d, k);
                expect(
                    !d.app.views.groups.tabs().contains(&views::MainView::Wbs),
                    "closes the tab",
                )
            }),
        ),
        (
            "Shift+F10",
            Run(|k| {
                let mut d = on_activities("help-key-sf10");
                press(&mut d, k);
                expect(
                    egui::Popup::is_any_open(&d.ctx),
                    "opens the right-click menu",
                )
            }),
        ),
        (
            "Alt",
            Run(|_| {
                let mut d = driver("help-key-alt");
                let _ = d.frame_with(vec![], egui::Modifiers::ALT);
                d.frames(2);
                expect(
                    crate::menu_keys::bar_title(&d.ctx).is_some(),
                    "reaches the menu bar",
                )
            }),
        ),
        (
            "Home",
            Run(|k| {
                let mut d = on_activities("help-key-home");
                let rows = d.visible_activities();
                d.app.selected = rows.get(2).copied();
                d.frames(2);
                press(&mut d, k);
                expect(d.app.selected == rows.first().copied(), "first row")
            }),
        ),
        (
            "End",
            Run(|k| {
                let mut d = on_activities("help-key-end");
                let rows = d.visible_activities();
                press(&mut d, k);
                expect(d.app.selected == rows.last().copied(), "last row")
            }),
        ),
        (
            "Page Down",
            Run(|k| {
                let mut d = on_activities("help-key-pgdn");
                let rows = d.visible_activities();
                press(&mut d, k);
                let want = rows.get(20.min(rows.len() - 1)).copied();
                expect(d.app.selected == want, "twenty rows down")
            }),
        ),
        (
            "Page Up",
            Run(|k| {
                let mut d = on_activities("help-key-pgup");
                let rows = d.visible_activities();
                d.app.selected = rows.last().copied();
                d.frames(2);
                press(&mut d, k);
                let want = rows.get(rows.len().saturating_sub(21)).copied();
                expect(d.app.selected == want, "twenty rows up")
            }),
        ),
        (
            "Ctrl+C",
            Run(|_| {
                // The platform turns Ctrl+C into a Copy event.
                let mut d = on_activities("help-key-c");
                let _ = d.frame(vec![egui::Event::Copy]);
                d.frames(1);
                expect(d.app.editing.clipboard.is_some(), "copies")
            }),
        ),
        (
            "Ctrl+X",
            Run(|_| {
                // The platform turns Ctrl+X into a Cut event: it marks the
                // selection for a move, asks nothing and removes nothing.
                let mut d = on_activities("help-key-x");
                let n = activity_count(&d);
                let _ = d.frame(vec![egui::Event::Cut]);
                d.frames(2);
                let cut = d.app.editing.clipboard.as_ref().is_some_and(|c| c.cut);
                expect(
                    cut && activity_count(&d) == n && d.app.delete_confirm.is_none(),
                    "marks the cut",
                )
            }),
        ),
        (
            "Ctrl+V",
            Run(|_| {
                // The platform turns Ctrl+V into a Paste event.
                let mut d = on_activities("help-key-v");
                let _ = d.frame(vec![egui::Event::Copy]);
                let text = d.app.editing.clipboard.as_ref().unwrap().text.clone();
                let _ = d.frame(vec![egui::Event::Paste(text)]);
                d.frames(2);
                expect(
                    dialog_open(&d, "copy_activity_options"),
                    &format!("{:?}", crate::dialog_kit::open_dialogs(&d.ctx)),
                )
            }),
        ),
        (
            "Enter",
            Covered("dialog_formatting sweep: Enter runs each dialog's default button"),
        ),
        (
            "Escape",
            Covered("full_ui_sweep: every dialog closes on Escape"),
        ),
        (
            "Ctrl",
            Covered("a modifier for click selection, not a key of its own"),
        ),
        (
            "Shift",
            Covered("a modifier for click selection, not a key of its own"),
        ),
    ]
}

/// Every key or key combination a help topic names is a real binding: each
/// is pressed through the real UI and its effect checked.
#[test]
fn every_shortcut_named_in_help_is_bound() {
    let checks = shortcut_checks();
    let named: BTreeSet<String> = code_spans()
        .into_iter()
        .filter(|(_, s)| is_shortcut(s))
        .map(|(_, s)| s)
        .collect();
    let mut problems = vec![];
    for (topic, s) in code_spans() {
        if is_shortcut(&s) && !checks.iter().any(|(k, _)| *k == s) {
            problems.push(format!("{topic}: `{s}` has no check (or is misspelled)"));
        }
    }
    for (key, check) in &checks {
        if !named.contains(*key) {
            problems.push(format!("`{key}` is checked but no topic names it"));
            continue;
        }
        match check {
            Check::Run(run) => {
                if let Err(e) = run(key) {
                    problems.push(format!("`{key}`: {e}"));
                }
            }
            Check::Covered(why) => assert!(!why.is_empty(), "{key}: say where it is covered"),
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The menu bar titles and their action names.
const MENU_TITLES: [(&str, &str); 9] = [
    ("File", "file"),
    ("Edit", "edit"),
    ("View", "view"),
    ("Project", "project"),
    ("Enterprise", "enterprise"),
    ("Tools", "tools"),
    ("AI Agents", "ai_agents"),
    ("Admin", "admin"),
    ("Help", "help"),
];

/// A label as the help writes it matches the drawn one; the help may leave
/// off a trailing `...`.
fn same_label(help: &str, drawn: &str) -> bool {
    let trim = |s: &str| s.trim_end_matches("...").trim().to_owned();
    help == drawn || trim(help) == trim(drawn)
}

/// Every menu path a help topic names (`Tools > Schedule ...`) can be
/// followed in the real menus: the title opens, each submenu opens and the
/// last item is drawn. A label counts only when the open menu adds it to the
/// screen (the directory bar also shows "WBS"). Paths are tried on the
/// Activities view with the Gantt chart on top, then with the Activity
/// Network on top (the Activity Network submenu needs it).
#[test]
fn every_menu_path_named_in_help_exists() {
    fn follow(d: &mut Driver, action: &str, parts: &[&str]) -> Result<(), String> {
        let count = |texts: &[(String, Rect)], part: &str| {
            texts.iter().filter(|(t, _)| same_label(part, t)).count()
        };
        let before = d.texts();
        d.open_menu(&format!("menu_bar_{action}"));
        for (n, part) in parts.iter().enumerate().skip(1) {
            let texts = d.texts();
            let drawn = texts
                .iter()
                .rev()
                .find(|(t, _)| same_label(part, t))
                .map(|(t, _)| t.clone());
            match drawn {
                Some(drawn) if count(&texts, part) > count(&before, part) => {
                    if n + 1 < parts.len() {
                        d.hover_text(&drawn);
                    }
                }
                _ => return Err(format!("no {part:?} under {:?}", parts[..n].join(" > "))),
            }
        }
        Ok(())
    }
    let mut problems = vec![];
    let mut checked = 0;
    let mut paths: Vec<(&str, String)> = code_spans()
        .into_iter()
        .filter(|(_, s)| s.contains(" > "))
        .collect();
    paths.sort_by(|a, b| a.1.cmp(&b.1));
    paths.dedup_by(|a, b| a.1 == b.1);
    let mut d = driver("help-menu-paths");
    for (topic, path) in paths {
        let parts: Vec<&str> = path.split(" > ").collect();
        let Some((_, action)) = MENU_TITLES.iter().find(|(t, _)| *t == parts[0]) else {
            // A right-click or view menu path, not the menu bar.
            continue;
        };
        checked += 1;
        let mut result = Err(String::new());
        for top in [panes::TopView::Gantt, panes::TopView::Network] {
            d.home();
            d.app.panes.top = top;
            d.frames(2);
            result = follow(&mut d, action, &parts);
            d.key(Key::Escape);
            d.key(Key::Escape);
            if result.is_ok() {
                break;
            }
        }
        if let Err(e) = result {
            problems.push(format!("{topic}: `{path}`: {e}"));
        }
    }
    d.home();
    assert!(checked >= 60, "only {checked} menu paths found");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
