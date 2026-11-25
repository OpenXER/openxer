//! Spell Check: the dictionary, word splitting, suggestions, the dialog
//! actions, one-step undo of every change, and a render of the dialog.
use super::dict::{self, Dictionary};
use super::*;
use crate::headless_render::SoftwarePainter;
use crate::reports::test_support::{artifacts, crop_dialog, scheduled_starter, temp};
use crate::store::Store;

fn none() -> HashSet<String> {
    HashSet::new()
}

#[test]
fn builtin_list_knows_common_and_construction_words() {
    let d = dict::builtin();
    assert!(d.word_count() > 100_000, "{}", d.word_count());
    for w in [
        "commissioning",
        "Install",
        "foundations",
        "Monday",
        "contractor's",
        "hydrotest",
        "Precommissioning",
        "don't",
        "RB",
        "HVAC",
        "FOUNDATIONS",
        "x",
    ] {
        assert!(d.known(w, &none()), "{w}");
    }
    for w in ["fondation", "Instal", "monday", "MECHANCIAL", "revew"] {
        assert!(!d.known(w, &none()), "{w}");
    }
    let user: HashSet<String> = ["revew".to_string()].into();
    assert!(d.known("Revew", &user));
}

#[test]
fn suggestions_come_closest_first_in_the_words_case() {
    let d = dict::builtin();
    assert_eq!(d.suggest("foundaton", &none(), 5)[0], "foundation");
    assert!(d
        .suggest("Instal", &none(), 8)
        .contains(&"Install".to_string()));
    assert_eq!(d.suggest("MECHANCIAL", &none(), 3)[0], "MECHANICAL");
    assert_eq!(d.suggest("Concret", &none(), 3)[0], "Concrete");
    let small = Dictionary::from_words(["grout".to_string()]);
    assert_eq!(small.suggest("gruot", &none(), 3), vec!["grout"]);
    assert!(small.suggest("qqqqqqqq", &none(), 3).is_empty());
}

#[test]
fn words_skip_markup_entities_and_codes() {
    let text = "<p class=\"x\">Pour&nbsp;concrete, don't</p> RB1000A A-12 it's";
    let found: Vec<&str> = dict::words(text, true)
        .into_iter()
        .map(|(s, e)| &text[s..e])
        .collect();
    assert_eq!(found, vec!["Pour", "concrete", "don't", "A", "it's"]);
    let plain = "Fabricate 'spools' 4x";
    let found: Vec<&str> = dict::words(plain, false)
        .into_iter()
        .map(|(s, e)| &plain[s..e])
        .collect();
    assert_eq!(found, vec!["Fabricate", "spools"]);
}

/// The starter with misspelled names, a step and a notebook.
fn misspelled() -> openxer_xer::ProjectData {
    let mut p = scheduled_starter();
    let ids: Vec<i64> = p.model.activities.iter().map(|a| a.task_id).collect();
    p.details.get_mut(&ids[0]).unwrap().name = "Notise to proceed".into();
    p.details.get_mut(&ids[1]).unwrap().name = "Site investigaton & survey".into();
    records::add_row(
        &mut p.document,
        "TASKPROC",
        &[
            ("proc_id", "1".into()),
            ("task_id", ids[2].to_string()),
            ("proj_id", "1".into()),
            ("seq_num", "1".into()),
            ("proc_name", "Revew drawings".into()),
        ],
    );
    records::add_row(
        &mut p.document,
        "MEMOTYPE",
        &[
            ("memo_type_id", "1".into()),
            ("memo_type", "Field Notes".into()),
        ],
    );
    records::add_row(
        &mut p.document,
        "TASKMEMO",
        &[
            ("memo_id", "1".into()),
            ("task_id", ids[3].to_string()),
            ("memo_type_id", "1".into()),
            ("proj_id", "1".into()),
            (
                "task_memo",
                "<p>Concret pour, then <b>Concret</b> cure</p>".into(),
            ),
        ],
    );
    p
}

fn app_with(p: openxer_xer::ProjectData, dir: &std::path::Path) -> (App, egui::Context) {
    let store = Store::open(dir.join("workspace.db")).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let app = App::new(&ctx, store, p);
    (app, ctx)
}

#[test]
fn dialog_actions_apply_as_one_undo_step() {
    let dir = temp("spell");
    let (mut app, ctx) = app_with(misspelled(), &dir);
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let original = app.project.clone();
    let (undo_before, _) = app.history.counts();
    app.selected = Some(0);
    painter.click_action(&mut app, &ctx, &mut time, "menu_bar_edit");
    painter.click_action(&mut app, &ctx, &mut time, "menu_spell_check");
    let word = |app: &App| {
        app.spell
            .as_ref()
            .unwrap()
            .current
            .as_ref()
            .map(|m| m.word.clone())
    };
    assert_eq!(word(&app).as_deref(), Some("Notise"));
    assert!(app
        .spell
        .as_ref()
        .unwrap()
        .suggestions
        .contains(&"Notice".to_string()));
    let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    crop_dialog(&image, &ctx, "spell")
        .save(artifacts().join("spell-check.png"))
        .unwrap();
    app.spell.as_mut().unwrap().change_to = "Notice".into();
    painter.click_action(&mut app, &ctx, &mut time, "spell_change");
    assert_eq!(word(&app).as_deref(), Some("investigaton"));
    assert_eq!(app.selected, Some(1), "the checked activity is selected");
    painter.click_action(&mut app, &ctx, &mut time, "spell_ignore");
    assert_eq!(word(&app).as_deref(), Some("Revew"));
    painter.click_action(&mut app, &ctx, &mut time, "spell_add");
    assert_eq!(word(&app).as_deref(), Some("Concret"));
    app.spell.as_mut().unwrap().change_to = "Concrete".into();
    painter.click_action(&mut app, &ctx, &mut time, "spell_change_all");
    assert_eq!(
        word(&app),
        None,
        "both notebook words changed, nothing left"
    );
    // Nothing reaches the project until the dialog closes.
    assert_eq!(app.project.details, original.details);
    painter.click_action(&mut app, &ctx, &mut time, "spell_close");
    assert!(app.spell.is_none());
    let name = |app: &App, i: usize| app.project.name(i).to_string();
    assert_eq!(name(&app, 0), "Notice to proceed");
    assert_eq!(name(&app, 1), "Site investigaton & survey");
    let memo = |app: &App| records::get(&app.project.document, "TASKMEMO", 0, "task_memo");
    assert_eq!(
        memo(&app),
        "<p>Concrete pour, then <b>Concrete</b> cure</p>"
    );
    assert!(app
        .store
        .preference(USER_DICTIONARY_KEY)
        .unwrap()
        .unwrap()
        .lines()
        .any(|l| l == "revew"));
    let (undo_after, _) = app.history.counts();
    assert_eq!(undo_after, undo_before + 1, "one undo step");
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    assert_eq!(
        app.history.counts().0,
        undo_before + 1,
        "no extra steps recorded"
    );
    assert!(app.history.undo(&mut app.project));
    assert_eq!(name(&app, 0), "Notise to proceed");
    assert_eq!(memo(&app), "<p>Concret pour, then <b>Concret</b> cure</p>");
    assert!(app.history.redo(&mut app.project));
    assert_eq!(name(&app, 0), "Notice to proceed");
    // The added word is now known: a second run stops only at the ignored one.
    app.open_spell_check();
    assert_eq!(word(&app).as_deref(), Some("investigaton"));
    app.spell.as_mut().unwrap().ignore_all();
    assert!(app.spell.as_ref().unwrap().done());
    app.close_spell_check(9.);
    assert_eq!(app.status, "Spell Check: no changes");
}

/// A huge run of letters from an untrusted file: flagged once, no
/// suggestions, and quick.
#[test]
fn very_long_token_is_flagged_without_suggestions_quickly() {
    // Processor time, so a busy machine cannot fail the bound.
    let started = crate::util::CpuTimer::start();
    let blob = "x".repeat(100_000);
    let d = dict::builtin();
    assert!(!d.known(&blob, &none()));
    assert!(d.suggest(&blob, &none(), 8).is_empty());
    let text = format!("<p>{blob} at</p>");
    assert_eq!(
        dict::words(&text, true),
        vec![(3, 100_003), (100_004, 100_006)]
    );
    let item = Item {
        source: Source::Notebook(0, "1".into()),
        activity: 0,
        label: "Notebook".into(),
        text,
        changed: false,
    };
    let mut s = SpellState::new(vec![item], none());
    let m = s.current.clone().unwrap();
    assert_eq!((m.start, m.end), (3, 100_003));
    assert_eq!(m.word.len(), dict::MAX_WORD);
    assert!(s.suggestions.is_empty());
    assert_eq!(s.change_to.len(), dict::MAX_WORD);
    s.change_to = "blob".into();
    s.change();
    assert!(s.done());
    assert_eq!(s.items[0].text, "<p>blob at</p>");
    // A long but bounded word still gets a capped search.
    let long = "qwertyuiopasdf";
    let _ = d.suggest(long, &none(), 8);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
}

/// Escape, the close box and Cancel end the check with the project as it
/// was; only Close applies the held changes.
#[test]
fn escape_and_cancel_leave_the_project_unchanged() {
    let dir = temp("spell_escape");
    let (mut app, ctx) = app_with(misspelled(), &dir);
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let original = app.project.clone();
    let (undo_before, _) = app.history.counts();
    let escape = egui::Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    };
    for how in ["escape", "cancel", "title_close"] {
        app.selected = Some(0);
        painter.click_action(&mut app, &ctx, &mut time, "menu_bar_edit");
        painter.click_action(&mut app, &ctx, &mut time, "menu_spell_check");
        assert!(app.spell.is_some(), "{how}");
        app.spell.as_mut().unwrap().change_to = "Notice".into();
        painter.click_action(&mut app, &ctx, &mut time, "spell_change");
        assert_eq!(app.spell.as_ref().unwrap().changes, 1, "{how}");
        match how {
            "escape" => {
                let _ = painter.frame(&mut app, &ctx, &mut time, vec![escape.clone()], false);
            }
            "cancel" => painter.click_action(&mut app, &ctx, &mut time, "spell_cancel"),
            _ => painter.click_action(&mut app, &ctx, &mut time, "spell_title_close"),
        }
        for _ in 0..2 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        assert!(app.spell.is_none(), "{how} closes the dialog");
        assert_eq!(app.project.details, original.details, "{how}");
        assert_eq!(app.project.name(0), "Notise to proceed", "{how}");
        assert_eq!(app.history.counts().0, undo_before, "{how}: no undo step");
        assert!(!app.dirty, "{how}");
    }
}
