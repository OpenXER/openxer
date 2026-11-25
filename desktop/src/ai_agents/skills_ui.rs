//! AI Agents > Skills: the skills connected agents follow to analyze and
//! build schedules, built-in and the user's own, and an editor for them.
//!
//! Everything about the files is `openxer_automation::skills`, the module
//! behind `openxer-cli skills` and the MCP skill tools, so the dialog, the
//! CLI and every agent see the same catalog. The user's skills are `.md`
//! files in `<data folder>\skills\`; a test build that injects no folder
//! uses a temporary one, never the real data folder.
use crate::dialog_kit::{self as kit, Col, Entry as B};
use crate::schedule_dialogs::{dialog_font, modal, Canvas};
use crate::App;
use eframe::egui::{self, Align, Vec2};
use openxer_automation::skills::{self, Catalog, Entry, Kind, Skill, Source};
use std::path::PathBuf;

/// The Skills dialog's intro.
const INTRO: &str = "Skills teach a connected AI agent how to analyze or build a schedule with OpenXER's tools. Agents look for a matching skill on their own; Copy Request gives you the sentence that asks for one.";

#[derive(Default)]
pub(crate) struct SkillsState {
    /// The folder of the user's skills; None is the data folder's (a
    /// temporary folder in a test build).
    pub(crate) dir: Option<PathBuf>,
    /// The Skills dialog, while open.
    pub(crate) list: Option<ListDialog>,
    /// The skill editor, while open.
    pub(crate) editor: Option<Editor>,
    /// Delete or Restore waiting for Yes or No: the skill id.
    pub(crate) confirm: Option<String>,
}

pub(crate) struct ListDialog {
    pub(crate) catalog: Catalog,
    pub(crate) selected: Option<usize>,
    /// The last action's result.
    pub(crate) message: String,
}

/// The editor's fields.
pub(crate) struct Editor {
    /// New (or a duplicate): the id may be changed and must be free.
    pub(crate) new: bool,
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) request: String,
    pub(crate) use_when: String,
    pub(crate) tools: String,
    pub(crate) kind: Kind,
    pub(crate) instructions: String,
    /// Header values the form does not show, kept as they were.
    keep: Skill,
    /// The first problem Save found, in plain words.
    pub(crate) error: String,
    /// The text whose unknown tools the user was warned about; a second
    /// Save of the same text keeps them.
    warned: Option<String>,
    /// The tool picked in "Tools you can name".
    pub(crate) tool: Option<usize>,
}

impl Editor {
    fn from(skill: &Skill, new: bool) -> Editor {
        Editor {
            new,
            id: skill.id.clone(),
            name: skill.name.clone(),
            description: skill.description.clone(),
            request: skill.request.clone(),
            use_when: skill.use_when.clone(),
            tools: skill.tools.join(", "),
            kind: skill.kind,
            instructions: skill.instructions.clone(),
            keep: skill.clone(),
            error: String::new(),
            warned: None,
            tool: None,
        }
    }

    /// The skill the fields describe.
    fn skill(&self) -> Skill {
        Skill {
            id: self.id.trim().to_string(),
            name: self.name.trim().to_string(),
            description: self.description.trim().to_string(),
            request: self.request.trim().to_string(),
            use_when: self.use_when.trim().to_string(),
            instructions: self.instructions.clone(),
            tools: self
                .tools
                .split([',', '\n'])
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(String::from)
                .collect(),
            kind: self.kind,
            ..self.keep.clone()
        }
    }
}

/// A parse problem in the words of the form: which field, what is wrong.
fn plain(e: &skills::SkillError) -> String {
    let m = &e.message;
    fn field(key: &str) -> &str {
        match key {
            "use_when" => "Use when",
            "id" => "Id",
            "name" => "Name",
            "description" => "Description",
            "request" => "Request",
            "tools" => "Tools",
            "kind" => "Kind",
            "order" => "Order",
            other => other,
        }
    }
    if let Some(key) = m.strip_prefix("the header has no ") {
        return format!("{} is required.", field(key));
    }
    if let Some(key) = m.strip_suffix(" is empty") {
        return format!("{} is required.", field(key));
    }
    if m.contains("no instructions") {
        return "Instructions are required: write the steps the agent follows.".into();
    }
    let mut out = m.clone();
    for key in ["use_when", "description", "request", "name", "id"] {
        if let Some(rest) = out.strip_prefix(key) {
            out = format!("{}{rest}", field(key));
            break;
        }
    }
    let mut chars = out.chars();
    let first = chars
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    format!("{first}{}.", chars.as_str().trim_end_matches('.'))
}

/// `s` cut with "..." to fit `width` pixels.
fn fit(c: &Canvas, s: &str, width: f32) -> String {
    let w = |t: &str| {
        c.ui.painter()
            .layout_no_wrap(t.to_owned(), dialog_font(), egui::Color32::BLACK)
            .size()
            .x
    };
    if w(s) <= width {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut n = chars.len();
    while n > 0 {
        let t: String = chars[..n].iter().collect::<String>() + "...";
        if w(&t) <= width {
            return t;
        }
        n -= 1;
    }
    String::new()
}

/// `s` broken into lines of at most `width` pixels.
fn wrap(c: &Canvas, s: &str, width: f32) -> Vec<String> {
    let w = |t: &str| {
        c.ui.painter()
            .layout_no_wrap(t.to_owned(), dialog_font(), egui::Color32::BLACK)
            .size()
            .x
    };
    let mut lines = vec![];
    let mut line = String::new();
    for word in s.split_whitespace() {
        let next = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if w(&next) > width && !line.is_empty() {
            lines.push(std::mem::take(&mut line));
            line = word.to_string();
        } else {
            line = next;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// The tools a skill can name, with the first sentence of each description.
fn tool_list() -> Vec<(String, String)> {
    openxer_automation::mcp::tool_definitions()
        .iter()
        .filter_map(|t| {
            let name = t["name"].as_str()?.to_string();
            let d = t["description"].as_str().unwrap_or_default();
            let first = d.split(". ").next().unwrap_or(d).trim_end_matches('.');
            Some((name, format!("{first}.")))
        })
        .collect()
}

impl App {
    /// The folder of the user's skills.
    pub(crate) fn skills_dir(&self) -> PathBuf {
        if let Some(dir) = &self.skills.dir {
            return dir.clone();
        }
        if cfg!(test) {
            // Tests never touch the real data folder unless they inject one.
            return std::env::temp_dir()
                .join(format!("openxer-test-skills-{}", std::process::id()));
        }
        skills::user_dir()
    }

    pub(crate) fn open_skills(&mut self) {
        let catalog = skills::catalog_in(Some(&self.skills_dir()));
        let selected = (!catalog.entries.is_empty()).then_some(0);
        self.skills.list = Some(ListDialog {
            catalog,
            selected,
            message: String::new(),
        });
    }

    /// Re-reads the folder, keeping the selection on `id` when it is there.
    fn reload_skills(&mut self, id: Option<&str>) {
        let catalog = skills::catalog_in(Some(&self.skills_dir()));
        if let Some(d) = &mut self.skills.list {
            let keep = id.map(String::from).or_else(|| {
                d.selected
                    .and_then(|i| d.catalog.entries.get(i))
                    .map(|e| e.skill.id.clone())
            });
            d.selected = keep
                .and_then(|id| catalog.entries.iter().position(|e| e.skill.id == id))
                .or((!catalog.entries.is_empty()).then_some(0));
            d.catalog = catalog;
        }
    }

    fn selected_skill(&self) -> Option<Entry> {
        let d = self.skills.list.as_ref()?;
        d.selected.and_then(|i| d.catalog.entries.get(i)).cloned()
    }

    fn skills_message(&mut self, message: impl Into<String>) {
        if let Some(d) = &mut self.skills.list {
            d.message = message.into();
        }
    }

    /// New: the template under a free id.
    pub(crate) fn new_skill(&mut self) {
        let id = skills::free_id(&self.skills_dir(), "my-skill");
        match skills::parse(&skills::template(&id), Some(&id)) {
            Ok(skill) => self.skills.editor = Some(Editor::from(&skill, true)),
            Err(e) => self.skills_message(e.to_string()),
        }
    }

    /// Edit: a built-in opens as the user's copy under the same id.
    pub(crate) fn edit_skill(&mut self) {
        if let Some(e) = self.selected_skill() {
            self.skills.editor = Some(Editor::from(&e.skill, false));
        }
    }

    /// Duplicate: a copy under a new free id, for the user to change.
    pub(crate) fn duplicate_skill(&mut self) {
        let Some(e) = self.selected_skill() else {
            return;
        };
        let mut copy = e.skill.clone();
        copy.id = skills::free_id(&self.skills_dir(), &format!("{}-copy", e.skill.id));
        let name: String = format!("{} (copy)", e.skill.name);
        copy.name = if name.chars().count() <= skills::NAME_MAX {
            name
        } else {
            e.skill.name.clone()
        };
        self.skills.editor = Some(Editor::from(&copy, true));
    }

    /// Copy Request: the skill's request sentence on the clipboard.
    pub(crate) fn copy_skill_request(&mut self, ctx: &egui::Context) {
        if let Some(e) = self.selected_skill() {
            ctx.copy_text(e.skill.request.clone());
            self.skills_message(format!(
                "Copied \"{}\". Paste it into your agent.",
                e.skill.request
            ));
        }
    }

    pub(crate) fn open_skills_folder(&mut self) {
        let dir = self.skills_dir();
        let result = std::fs::create_dir_all(&dir)
            .map_err(|e| e.to_string())
            .and_then(|()| crate::util::open_in_default_app(&dir));
        if let Err(e) = result {
            self.skills_message(format!("Cannot open {}: {e}", dir.display()));
        }
    }

    /// Delete (yours) or Restore (customized), after Yes.
    pub(crate) fn remove_skill_file(&mut self, id: &str) {
        let Some(entry) = self
            .skills
            .list
            .as_ref()
            .and_then(|d| d.catalog.get(id).cloned())
        else {
            return;
        };
        let Some(path) = entry.path.clone() else {
            return;
        };
        match std::fs::remove_file(&path) {
            Ok(()) => {
                self.reload_skills(Some(id));
                self.skills_message(if entry.source == Source::Customized {
                    format!("{} is back to the built-in skill.", entry.skill.name)
                } else {
                    format!("{} deleted.", entry.skill.name)
                });
            }
            Err(e) => self.skills_message(format!("Cannot delete {}: {e}", path.display())),
        }
    }

    /// Save: validate the fields and write the file. Returns the path when
    /// it was written.
    pub(crate) fn save_skill(&mut self) -> Option<PathBuf> {
        let dir = self.skills_dir();
        let ed = self.skills.editor.as_mut()?;
        let skill = ed.skill();
        let text = skill.to_text();
        let (parsed, warnings) = match skills::validate(&text, Some(&skill.id)) {
            Ok(v) => v,
            Err(e) => {
                ed.error = plain(&e);
                return None;
            }
        };
        let taken = dir.join(format!("{}.md", parsed.id)).exists();
        if ed.new && taken {
            ed.error = format!(
                "Id {} is already one of your skills. Pick another id.",
                parsed.id
            );
            return None;
        }
        if ed.new && skills::BUILT_IN.iter().any(|(b, _)| *b == parsed.id) {
            ed.error = format!(
                "Id {} is a built-in skill. Pick another id, or use Edit on the built-in to change it.",
                parsed.id
            );
            return None;
        }
        if let Some(first) = warnings.first() {
            if ed.warned.as_deref() != Some(text.as_str()) {
                ed.error = format!(
                    "{}. Check the name in Tools you can name, or Save again to keep it.",
                    first.replacen("tools: ", "Tools: ", 1)
                );
                ed.warned = Some(text);
                return None;
            }
        }
        match skills::write_user(&dir, &parsed.id, &text, !ed.new) {
            Ok(path) => {
                let id = parsed.id.clone();
                self.skills.editor = None;
                self.reload_skills(Some(&id));
                self.skills_message(format!("Saved {} in {}.", parsed.name, path.display()));
                Some(path)
            }
            Err(e) => {
                ed.error = format!("Cannot save: {e}");
                None
            }
        }
    }

    /// The Skills dialogs, each frame.
    pub(crate) fn skills_dialogs(&mut self, ctx: &egui::Context) {
        self.skills_list_dialog(ctx);
        self.skill_editor_dialog(ctx);
        self.skill_confirm(ctx);
    }

    /// Skills: the list, and New, Edit, Duplicate, Delete or Restore, Copy
    /// Request, Open Skills Folder, Help and Close in the right-hand strip.
    fn skills_list_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.skills.list.take() else {
            return;
        };
        const W: f32 = 800.;
        const H: f32 = 470.;
        const MAIN: f32 = W - kit::STRIP_WIDTH - 12.;
        const LIST_Y: f32 = 46.;
        const LIST_H: f32 = 290.;
        let cols = [
            Col {
                title: "Name",
                width: 176.,
            },
            Col {
                title: "Description",
                width: 288.,
            },
            Col {
                title: "Kind",
                width: 70.,
            },
            Col {
                title: "Source",
                width: 84.,
            },
        ];
        let selected = d.selected.and_then(|i| d.catalog.entries.get(i)).cloned();
        let blocked = self.skills.editor.is_some() || self.skills.confirm.is_some();
        #[derive(Default)]
        struct Clicks {
            close: bool,
            new: bool,
            edit: bool,
            duplicate: bool,
            remove: bool,
            copy: bool,
            folder: bool,
            help: bool,
        }
        let (k, closed, actions) = modal(ctx, "ai_skills", "Skills", Vec2::new(W, H), |c| {
            let mut k = Clicks::default();
            c.fill(0., 0., W, H, kit::FACE);
            let intro =
                c.ui.painter()
                    .layout(INTRO.into(), dialog_font(), crate::chrome::INK, MAIN - 8.);
            c.ui.painter()
                .galley(c.origin + Vec2::new(10., 8.), intro, crate::chrome::INK);
            let entries = &d.catalog.entries;
            let out = kit::list(
                c,
                "ai_skills_list",
                (6., LIST_Y, MAIN, LIST_H),
                None,
                &cols,
                entries.len(),
                d.selected,
                None,
                &mut |c, r, col, rect, sel| {
                    let e = &entries[r];
                    let s = match col {
                        0 => e.skill.name.clone(),
                        1 => e.skill.description.clone(),
                        2 => e.skill.kind.label().to_string(),
                        _ => e.source.label().to_string(),
                    };
                    let s = fit(c, &s, rect.width() - 8.);
                    kit::cell_text(c, rect, &s, sel, 3.);
                },
            );
            if let Some(r) = out.clicked {
                d.selected = Some(r);
            }
            if out.double.is_some() {
                k.edit = true;
            }
            // The selected skill's details and the files left out.
            let mut y = LIST_Y + LIST_H + 8.;
            if let Some(e) = &selected {
                let use_when = c.ui.painter().layout(
                    format!("Use when: {}", e.skill.use_when),
                    dialog_font(),
                    crate::chrome::INK,
                    MAIN - 8.,
                );
                let h = use_when.size().y.min(30.);
                c.ui.painter()
                    .with_clip_rect(egui::Rect::from_min_size(
                        c.origin + Vec2::new(10., y),
                        Vec2::new(MAIN - 8., h + 2.),
                    ))
                    .galley(c.origin + Vec2::new(10., y), use_when, crate::chrome::INK);
                y += h + 3.;
                let request = fit(c, &format!("Request: {}", e.skill.request), MAIN - 8.);
                kit::text(c, 10., y, &request, true);
                y += 15.;
                for w in e.warnings.iter().take(1) {
                    kit::error_text(c, 10., y, MAIN - 8., w);
                    y += 15.;
                }
            }
            if let Some(p) = d.catalog.problems.first() {
                let name = p
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let more = d.catalog.problems.len() - 1;
                let text = if more > 0 {
                    format!("Left out: {name}: {} (and {more} more)", p.error)
                } else {
                    format!("Left out: {name}: {}", p.error)
                };
                let text = fit(c, &text, MAIN - 8.);
                kit::error_text(c, 10., y, MAIN - 8., &text);
                y += 15.;
            }
            if !d.message.is_empty() {
                let message = fit(c, &d.message, MAIN - 8.);
                kit::text(c, 10., y, &message, true);
            }
            kit::strip(c, W, H);
            let has = selected.is_some() && !blocked;
            let (label, can) = match selected.as_ref().map(|e| e.source) {
                Some(Source::Customized) => ("&Restore", true),
                Some(Source::Yours) => ("De&lete", true),
                _ => ("De&lete", false),
            };
            let column = kit::button_column(
                c,
                W,
                &[
                    B::close("ai_skills_close"),
                    B::Button("ai_skills_new", "&New...", !blocked, true),
                    B::Button("ai_skills_edit", "&Edit...", has, false),
                    B::Button("ai_skills_duplicate", "&Duplicate...", has, false),
                    B::Button("ai_skills_remove", label, can && !blocked, false),
                    B::Button("ai_skills_copy_request", "&Copy Request", has, true),
                    B::Button("ai_skills_folder", "Open &Folder", !blocked, false),
                    B::help("ai_skills_help"),
                ],
            );
            match column.clicked.as_deref() {
                Some("ai_skills_close") => k.close = true,
                Some("ai_skills_new") => k.new = true,
                Some("ai_skills_edit") => k.edit = true,
                Some("ai_skills_duplicate") => k.duplicate = true,
                Some("ai_skills_remove") => k.remove = true,
                Some("ai_skills_copy_request") => k.copy = true,
                Some("ai_skills_folder") => k.folder = true,
                Some("ai_skills_help") => k.help = true,
                _ => {}
            }
            k
        });
        self.record_dialog_actions(actions);
        if (k.close || closed) && !blocked {
            return;
        }
        self.skills.list = Some(d);
        if k.new {
            self.new_skill();
        } else if k.edit {
            self.edit_skill();
        } else if k.duplicate {
            self.duplicate_skill();
        } else if k.remove {
            self.skills.confirm = self.selected_skill().map(|e| e.skill.id);
        } else if k.copy {
            self.copy_skill_request(ctx);
        } else if k.folder {
            self.open_skills_folder();
        }
        if k.help {
            self.open_help_for("ai_skills");
        }
    }

    /// Delete or Restore: Yes or No.
    fn skill_confirm(&mut self, ctx: &egui::Context) {
        let Some(id) = self.skills.confirm.clone() else {
            return;
        };
        let entry = self
            .skills
            .list
            .as_ref()
            .and_then(|d| d.catalog.get(&id).cloned());
        let Some(entry) = entry else {
            self.skills.confirm = None;
            return;
        };
        let text = if entry.source == Source::Customized {
            format!(
                "Restore the built-in {}? Your changed copy is deleted.",
                entry.skill.name
            )
        } else {
            format!(
                "Delete your skill {}? Its file is deleted.",
                entry.skill.name
            )
        };
        let mut rects = vec![];
        let answer = crate::kit_window::message_box(
            ctx,
            "ai_skill_delete",
            if entry.source == Source::Customized {
                "Restore Skill"
            } else {
                "Delete Skill"
            },
            crate::kit_window::MessageIcon::Question,
            &text,
            &["Yes", "No"],
            Some(1),
            &mut rects,
        );
        self.record_dialog_actions(rects);
        let escape = crate::kit_window::escape_goes_to(
            ctx,
            egui::Id::new(("kit_message", "ai_skill_delete")),
            egui::Order::Foreground,
        );
        if answer == Some(0) {
            self.skills.confirm = None;
            self.remove_skill_file(&id);
        } else if answer.is_some() || escape {
            self.skills.confirm = None;
        }
    }

    /// The skill editor: the header fields, Check or Workflow, the
    /// instructions, and the tools a skill can name.
    fn skill_editor_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut ed) = self.skills.editor.take() else {
            return;
        };
        const W: f32 = 860.;
        const H: f32 = 590.;
        const LEFT: f32 = 540.;
        const LABEL: f32 = 92.;
        let tools = tool_list();
        let title = if ed.new { "New Skill" } else { "Edit Skill" };
        #[derive(Default)]
        struct Clicks {
            save: bool,
            cancel: bool,
            text_editor: bool,
            help: bool,
            add: bool,
        }
        let (k, closed, actions) = modal(ctx, "ai_skill_edit", title, Vec2::new(W, H), |c| {
            let mut k = Clicks::default();
            c.fill(0., 0., W, H, kit::FACE);
            let mut y = 10.;
            let fields: [(&str, &str, &mut String, bool); 6] = [
                ("Id", "ai_skill_edit_id", &mut ed.id, ed.new),
                ("Name", "ai_skill_edit_name", &mut ed.name, true),
                (
                    "Description",
                    "ai_skill_edit_description",
                    &mut ed.description,
                    true,
                ),
                ("Request", "ai_skill_edit_request", &mut ed.request, true),
                ("Use when", "ai_skill_edit_use_when", &mut ed.use_when, true),
                ("Tools", "ai_skill_edit_tools", &mut ed.tools, true),
            ];
            for (label, key, value, enabled) in fields {
                kit::text(c, 10., y + 4., label, true);
                c.edit(
                    key,
                    LABEL,
                    y,
                    LEFT - LABEL - 10.,
                    22.,
                    value,
                    enabled,
                    Align::Min,
                );
                y += 28.;
            }
            kit::text(c, 10., y + 4., "Kind", true);
            if kit::radio(
                c,
                "ai_skill_edit_check",
                LABEL,
                y,
                170.,
                22.,
                "&Check (reads only)",
                ed.kind == Kind::Check,
                true,
            ) {
                ed.kind = Kind::Check;
            }
            if kit::radio(
                c,
                "ai_skill_edit_workflow",
                LABEL + 180.,
                y,
                250.,
                22.,
                "&Workflow (builds or changes)",
                ed.kind == Kind::Workflow,
                true,
            ) {
                ed.kind = Kind::Workflow;
            }
            y += 30.;
            kit::text(c, 10., y, "Instructions", true);
            y += 18.;
            let box_h = H - y - 76.;
            kit::text_area_with(
                c,
                "ai_skill_edit_instructions",
                (10., y, LEFT - 20., box_h),
                &mut ed.instructions,
                kit::body_font(),
                crate::chrome::INK,
                kit::TextArea {
                    scroll: true,
                    ..Default::default()
                },
            );
            // The first problem, under the box.
            let error_y = y + box_h + 6.;
            if !ed.error.is_empty() {
                for (i, line) in wrap(c, &ed.error, LEFT - 20.).iter().take(2).enumerate() {
                    kit::error_text(c, 10., error_y + 15. * i as f32, LEFT - 20., line);
                }
            }
            // Tools you can name.
            let tx = LEFT + 6.;
            let tw = W - tx - 10.;
            kit::text(c, tx, 10., "Tools you can name", true);
            let list_h = 330.;
            let out = kit::list(
                c,
                "ai_skill_edit_tool_list",
                (tx, 28., tw, list_h),
                None,
                &[Col {
                    title: "Tool",
                    width: tw - 2.,
                }],
                tools.len(),
                ed.tool,
                None,
                &mut |c, r, _, rect, sel| {
                    let s = fit(c, &tools[r].0, rect.width() - 8.);
                    kit::cell_text(c, rect, &s, sel, 3.);
                },
            );
            if let Some(r) = out.clicked {
                ed.tool = Some(r);
            }
            if out.double.is_some() {
                k.add = true;
            }
            let mut ty = 28. + list_h + 6.;
            if let Some((_, about)) = ed.tool.and_then(|i| tools.get(i)) {
                for line in wrap(c, about, tw).iter().take(5) {
                    kit::text(c, tx, ty, line, true);
                    ty += 15.;
                }
            } else {
                for line in wrap(
                    c,
                    "Pick a tool to see what it gives; Add Tool puts it in the Tools field.",
                    tw,
                )
                .iter()
                .take(2)
                {
                    kit::secondary_text(c, tx, ty, line);
                    ty += 15.;
                }
            }
            k.add |= kit::button(
                c,
                "ai_skill_edit_add_tool",
                tx,
                28. + list_h + 6. + 15. * 5. + 4.,
                kit::BUTTON.x,
                kit::BUTTON.y,
                "&Add Tool",
                ed.tool.is_some(),
            );
            // Save (Enter), Cancel, Edit in Text Editor, Help: the kit's
            // bottom row, with Save as the default button at its head.
            let by = H - 33.;
            let right = W - 10.;
            let rest = [
                B::cancel("ai_skill_edit_cancel"),
                B::Button("ai_skill_edit_text_editor", "&Text Editor...", true, true),
                B::help("ai_skill_edit_help"),
            ];
            let rest_w = 3. * kit::BUTTON.x + 2. * (kit::ROW_GAP + kit::GROUP_GAP);
            k.save = kit::default_button(
                c,
                "ai_skill_edit_save",
                right - rest_w - kit::ROW_GAP - kit::BUTTON.x,
                by,
                kit::BUTTON.x,
                kit::BUTTON.y,
                "&Save",
                true,
            );
            match kit::button_row(c, right, by, &rest).clicked.as_deref() {
                Some("ai_skill_edit_cancel") => k.cancel = true,
                Some("ai_skill_edit_text_editor") => k.text_editor = true,
                Some("ai_skill_edit_help") => k.help = true,
                _ => {}
            }
            k
        });
        self.record_dialog_actions(actions);
        if k.cancel || closed {
            return;
        }
        if k.add {
            if let Some((name, _)) = ed.tool.and_then(|i| tools.get(i)) {
                let mut list: Vec<String> = ed
                    .tools
                    .split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
                if !list.contains(name) {
                    list.push(name.clone());
                }
                ed.tools = list.join(", ");
            }
        }
        self.skills.editor = Some(ed);
        if k.help {
            self.open_help_for("ai_skill_edit");
        }
        if k.save {
            self.save_skill();
        } else if k.text_editor {
            if let Some(path) = self.save_skill() {
                if let Err(e) = crate::util::open_in_default_app(&path) {
                    self.skills_message(format!("Cannot open {}: {e}", path.display()));
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "skills_ui_tests.rs"]
mod tests;
