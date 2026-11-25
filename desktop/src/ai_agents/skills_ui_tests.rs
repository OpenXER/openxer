//! AI Agents > Skills through the production UI, on a temporary skills
//! folder: no test here reads or writes the real data folder.
use super::*;
use crate::qa_sweep::Driver;
use egui::Key;
use std::fs;
use std::path::{Path, PathBuf};

/// A temporary folder removed when dropped.
struct Tmp(PathBuf);
impl Tmp {
    fn new() -> Tmp {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "openxer-skills-ui-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Tmp(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn open(name: &str) -> (Driver, Tmp) {
    let dir = Tmp::new();
    let mut d = Driver::new(name, openxer_xer::starter());
    d.app.skills.dir = Some(dir.path().to_path_buf());
    d.home();
    d.click("menu_bar_ai_agents");
    d.click("menu_ai_skills");
    d.frames(2);
    assert!(d.app.skills.list.is_some(), "Skills opened");
    (d, dir)
}

fn select(d: &mut Driver, id: &str) {
    let i = d
        .app
        .skills
        .list
        .as_ref()
        .unwrap()
        .catalog
        .entries
        .iter()
        .position(|e| e.skill.id == id)
        .unwrap_or_else(|| panic!("{id} listed"));
    d.app.skills.list.as_mut().unwrap().selected = Some(i);
    d.frames(2);
}

fn entry(d: &Driver, id: &str) -> Option<Entry> {
    d.app.skills.list.as_ref().unwrap().catalog.get(id).cloned()
}

fn message(d: &Driver) -> String {
    d.app.skills.list.as_ref().unwrap().message.clone()
}

#[test]
fn skills_lists_the_built_ins_and_escape_closes_it() {
    let (mut d, _dir) = open("skills-open");
    let list = d.app.skills.list.as_ref().unwrap();
    assert_eq!(list.catalog.entries.len(), skills::BUILT_IN.len());
    assert!(d.has("ai_skills_list_row_0"));
    let texts: Vec<String> = d.texts().into_iter().map(|(t, _)| t).collect();
    assert!(texts.iter().any(|t| t == "Logic health check"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "Built-in"));
    assert!(texts.iter().any(|t| t == "Check"));
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.skills.list.is_none(), "Escape closes Skills");
}

#[test]
fn new_writes_a_skill_of_yours_from_the_template() {
    let (mut d, dir) = open("skills-new");
    d.click("ai_skills_new");
    d.frames(2);
    let ed = d.app.skills.editor.as_mut().expect("the editor opened");
    assert!(ed.new);
    assert_eq!(ed.id, "my-skill");
    ed.id = "crew-peaks".into();
    ed.name = "Crew peaks".into();
    ed.tools = "openxer_resource_loading".into();
    d.frames(1);
    d.click("ai_skill_edit_save");
    d.frames(2);
    assert!(
        d.app.skills.editor.is_none(),
        "{:?}",
        d.app.skills.editor.as_ref().map(|e| e.error.clone())
    );
    let file = dir.path().join("crew-peaks.md");
    let text = fs::read_to_string(&file).unwrap();
    assert!(skills::parse(&text, Some("crew-peaks")).is_ok());
    let e = entry(&d, "crew-peaks").unwrap();
    assert_eq!(e.source, Source::Yours);
    assert!(message(&d).contains("Saved Crew peaks"));
}

#[test]
fn edit_with_a_problem_shows_it_and_saves_nothing_and_cancel_changes_nothing() {
    let (mut d, dir) = open("skills-edit-error");
    select(&mut d, "look-ahead");
    d.click("ai_skills_edit");
    d.frames(2);
    let ed = d.app.skills.editor.as_mut().unwrap();
    assert!(!ed.new);
    ed.name = String::new();
    d.frames(1);
    d.click("ai_skill_edit_save");
    d.frames(2);
    let ed = d.app.skills.editor.as_ref().expect("still open");
    assert_eq!(ed.error, "Name is required.");
    assert!(d.texts().iter().any(|(t, _)| t == "Name is required."));
    assert!(!dir.path().join("look-ahead.md").exists());
    // A name that is too long says so, in the form's words.
    d.app.skills.editor.as_mut().unwrap().name = "x".repeat(41);
    d.click("ai_skill_edit_save");
    d.frames(2);
    assert_eq!(
        d.app.skills.editor.as_ref().unwrap().error,
        "Name is 41 characters; keep it to 40."
    );
    d.click("ai_skill_edit_cancel");
    d.frames(2);
    assert!(d.app.skills.editor.is_none());
    assert!(
        fs::read_dir(dir.path()).unwrap().next().is_none(),
        "Cancel wrote nothing"
    );
    assert_eq!(entry(&d, "look-ahead").unwrap().source, Source::BuiltIn);
}

#[test]
fn edit_of_a_built_in_customizes_it_and_restore_brings_it_back() {
    let (mut d, dir) = open("skills-customize");
    select(&mut d, "milestone-status");
    d.click("ai_skills_edit");
    d.frames(2);
    d.app.skills.editor.as_mut().unwrap().name = "Our milestones".into();
    d.click("ai_skill_edit_save");
    d.frames(2);
    let e = entry(&d, "milestone-status").unwrap();
    assert_eq!(
        (e.source, e.skill.name.as_str()),
        (Source::Customized, "Our milestones")
    );
    assert!(dir.path().join("milestone-status.md").is_file());
    // Restore asks first; No keeps the file.
    select(&mut d, "milestone-status");
    assert!(d.texts().iter().any(|(t, _)| t == "Restore"));
    d.click("ai_skills_remove");
    d.frames(2);
    d.click("ai_skill_delete_no");
    d.frames(2);
    assert!(dir.path().join("milestone-status.md").is_file());
    d.click("ai_skills_remove");
    d.frames(2);
    d.click("ai_skill_delete_yes");
    d.frames(2);
    assert!(!dir.path().join("milestone-status.md").exists());
    let e = entry(&d, "milestone-status").unwrap();
    assert_eq!(e.source, Source::BuiltIn);
    assert_eq!(e.skill.name, "Milestone status");
}

#[test]
fn duplicate_makes_a_copy_of_yours_under_a_new_id() {
    let (mut d, dir) = open("skills-duplicate");
    select(&mut d, "near-critical-paths");
    d.click("ai_skills_duplicate");
    d.frames(2);
    let ed = d.app.skills.editor.as_ref().unwrap();
    assert!(ed.new);
    assert_eq!(ed.id, "near-critical-paths-copy");
    assert_eq!(ed.name, "Near-critical paths (copy)");
    d.click("ai_skill_edit_save");
    d.frames(2);
    assert!(dir.path().join("near-critical-paths-copy.md").is_file());
    assert_eq!(
        entry(&d, "near-critical-paths-copy").unwrap().source,
        Source::Yours
    );
    assert_eq!(
        entry(&d, "near-critical-paths").unwrap().source,
        Source::BuiltIn
    );
    // A second duplicate takes the next free id; Delete removes a skill of yours.
    select(&mut d, "near-critical-paths-copy");
    d.click("ai_skills_remove");
    d.frames(2);
    d.click("ai_skill_delete_yes");
    d.frames(2);
    assert!(entry(&d, "near-critical-paths-copy").is_none());
}

#[test]
fn copy_request_and_the_text_editor_and_the_folder() {
    let (mut d, dir) = open("skills-copy");
    select(&mut d, "logic-health-check");
    d.click("ai_skills_copy_request");
    d.frames(2);
    assert!(
        message(&d).contains("Run the logic health check on this schedule."),
        "{}",
        message(&d)
    );
    // Edit in Text Editor saves the user's copy and opens that file.
    crate::util::OPENED.with(|o| o.borrow_mut().clear());
    d.click("ai_skills_edit");
    d.frames(2);
    d.click("ai_skill_edit_text_editor");
    d.frames(2);
    let file = dir.path().join("logic-health-check.md");
    assert!(file.is_file());
    crate::util::OPENED.with(|o| assert_eq!(o.borrow().last(), Some(&file)));
    d.click("ai_skills_folder");
    d.frames(2);
    crate::util::OPENED.with(|o| assert_eq!(o.borrow().last(), Some(&dir.path().to_path_buf())));
}

#[test]
fn a_broken_file_is_shown_as_left_out_and_add_tool_fills_the_field() {
    let dir = Tmp::new();
    fs::write(dir.path().join("broken.md"), "no header").unwrap();
    let mut d = Driver::new("skills-broken", openxer_xer::starter());
    d.app.skills.dir = Some(dir.path().to_path_buf());
    d.home();
    d.app.open_skills();
    d.frames(2);
    assert!(d
        .texts()
        .iter()
        .any(|(t, _)| t.starts_with("Left out: broken.md: line 1")));
    d.click("ai_skills_new");
    d.frames(2);
    let first_tool = openxer_automation::skills::known_tools()[0].clone();
    d.app.skills.editor.as_mut().unwrap().tool = Some(0);
    d.frames(1);
    d.click("ai_skill_edit_add_tool");
    d.frames(2);
    let ed = d.app.skills.editor.as_ref().unwrap();
    assert!(
        ed.tools.contains(&first_tool),
        "{} (expected {first_tool})",
        ed.tools
    );
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.skills.editor.is_none(), "Escape closes the editor");
    assert!(d.app.skills.list.is_some(), "and leaves Skills open");
}

/// Renders for review, to the ignored `artifacts/skills/`: the Skills list,
/// the editor on a built-in, and the editor showing a problem.
#[test]
fn render_skills_dialogs() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/skills");
    fs::create_dir_all(&out).unwrap();
    let dir = Tmp::new();
    let mut d = Driver::probed(
        "skills-render",
        openxer_xer::starter(),
        egui::Vec2::new(1366., 768.),
        Some(out.clone()),
    );
    d.app.skills.dir = Some(dir.path().to_path_buf());
    d.home();
    d.click("menu_bar_ai_agents");
    d.frames(2);
    d.screenshot(&out.join("01-ai-agents-menu.png"));
    d.click("menu_ai_skills");
    d.frames(3);
    d.screenshot(&out.join("02-skills.png"));
    select(&mut d, "look-ahead");
    d.click("ai_skills_edit");
    d.frames(3);
    d.app.skills.editor.as_mut().unwrap().tool = Some(9);
    d.frames(3);
    d.screenshot(&out.join("03-edit-skill.png"));
    d.app.skills.editor.as_mut().unwrap().request = String::new();
    d.click("ai_skill_edit_save");
    d.frames(3);
    d.screenshot(&out.join("04-edit-problem.png"));
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.skills.editor.is_none());
}
