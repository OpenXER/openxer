//! AI Agents > Chat (F8): ask an AI agent on this PC about the open
//! schedule. Runs either as a non-modal tool window on the kit's shell
//! (`kit::shell_with` with roll-up, resizing and tool layering) or docked
//! as a right-hand split pane beside the schedule, so the table and Gantt
//! chart keep working while it is open. Its place, size, rolled state,
//! docked state, agent, model and mode are kept in the user's preferences.
//!
//! The window is driven by a [`backend::ChatBackend`] polled once a frame;
//! [`model::Conversation`] turns the backend's events into what the message
//! list ([`list`]) shows. The app's backend is [`live::LiveBackend`] (made
//! when Chat first opens; [`backend::OfflineBackend`] stands in until
//! then). With no assistant ready the list shows how to connect one, and
//! Check Setup ([`checks`]) says what is missing.
use crate::chrome::{self, Item, INK};
use crate::dialog_kit::{self as kit, Canvas};
use crate::App;
use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use std::collections::{HashMap, HashSet};

pub(crate) mod backend;
pub(crate) mod checks;
pub(crate) mod endpoint_dialog;
pub(crate) mod history;
pub(crate) mod links;
pub(crate) mod list;
pub(crate) mod live;
pub(crate) mod model;
pub(crate) mod prompts;
pub(crate) mod ui_actions;

use backend::{
    AgentInfo, AgentState, Applied, ChatBackend, ChatContext, Mode, ModelChoice, OfflineBackend,
    Selection, Turn,
};
use model::{Conversation, ProposalState};

#[cfg(test)]
mod renders;
#[cfg(test)]
mod tests;

/// The window's kit name (its client area, controls and help topic).
pub(crate) const SALT: &str = "chat";
/// The window's title.
pub(crate) const TITLE: &str = "Chat";
/// The menu command.
pub(crate) const COMMAND: &str = "menu_ai_chat";
/// Menu command to open Chat pre-loaded for the selected activities.
pub(crate) const ASK_SELECTION_COMMAND: &str = "menu_ai_ask_selection";
/// The client area it opens with: a 364 x 486 window.
pub(crate) const CLIENT: Vec2 = Vec2::new(362., 453.);
/// The smallest it can be made.
pub(crate) const MIN_CLIENT: Vec2 = Vec2::new(300., 330.);
/// Default docked right-pane width.
pub(crate) const DEFAULT_DOCK_WIDTH: f32 = 380.;
/// Minimum docked right-pane width.
pub(crate) const MIN_DOCK_WIDTH: f32 = 300.;
/// The user preference holding its place, agent and mode.
pub(crate) const PREF_KEY: &str = "chat_window_v1";
/// The user preference noting that the consent for `agent` was given.
fn consent_key(agent: &str) -> String {
    format!("chat_consent_{agent}")
}
/// The user preference noting that the ChatGPT plan welcome was shown.
pub(crate) const WELCOME_KEY: &str = "chat_plan_welcome_v1";
/// The Agent drop-down's last item: the setup list.
pub(crate) const SETUP_ITEM: &str = "Choose how to connect...";
/// The composer's placeholder.
pub(crate) const HINT: &str = "Ask about this schedule... (/ for playbooks, @ for activity)";
/// The two modes, as the Mode drop-down names them.
const MODES: [(&str, Mode); 2] = [("Ask only", Mode::Ask), ("Edit", Mode::Edit)];

/// The window's shell id (where the kit keeps its place).
pub(crate) fn window_id() -> egui::Id {
    egui::Id::new(("dialog_window", SALT))
}

/// What is kept between sessions.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Prefs {
    #[serde(default)]
    pub geometry: kit::Geometry,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub model: ModelChoice,
    #[serde(default)]
    pub docked: bool,
    #[serde(default)]
    pub dock_width: Option<f32>,
}

/// In-app dialog for saving a reusable user prompt snippet.
pub(crate) struct SavePromptDialog {
    pub name: String,
    pub text: String,
}

/// In-app dialog for editing per-project standing rules.
pub(crate) struct ProjectRulesDialog {
    pub text: String,
}

pub(crate) struct ChatState {
    pub open: bool,
    /// When true, Chat docks on the right edge of the main window instead of floating.
    pub docked: bool,
    /// Width of the docked right pane (pixels).
    pub dock_width: f32,
    pub backend: Box<dyn ChatBackend>,
    pub conv: Conversation,
    /// The chosen agent's id.
    pub agent: Option<String>,
    pub mode: Mode,
    pub model: ModelChoice,
    /// The text being written.
    pub draft: String,
    /// The agent whose consent box is up.
    pub consent: Option<String>,
    /// Chat Options while open: the Model drop-down's choice.
    pub options: Option<usize>,
    /// The stored place, given to the window when it is first shown.
    restore: Option<kit::Geometry>,
    /// What was last written to the preferences.
    saved: Prefs,
    /// The composer takes the keyboard on the next frame.
    focus: bool,
    /// The open project's Activity IDs, hover tooltips, and autocomplete pairs.
    codes: HashSet<String>,
    tips: HashMap<String, String>,
    activities_index: Vec<(String, String)>,
    codes_of: (usize, u64, String),
    /// Opens links and web pages (a recording fake in tests).
    pub opener: live::Opener,
    /// The user chose the agent in this session (a remembered choice is
    /// used only while it is ready).
    pub picked: bool,
    /// The setup list is shown at the user's request.
    pub setup: bool,
    /// Check Setup while open: the selected row.
    pub check: Option<Option<usize>>,
    /// The ChatGPT plan welcome was shown (read once).
    welcomed: Option<bool>,
    /// The project the conversation is about.
    project_key: Option<String>,
    /// The user's own chat instructions file (`chat/AGENTS.md` in the data
    /// folder; a temp folder in a test build).
    pub instructions: std::path::PathBuf,
    /// Per-project standing rules loaded from the workspace store.
    pub project_rules: String,
    /// In-app Project Rules dialog while open.
    pub project_rules_dialog: Option<ProjectRulesDialog>,
    /// In-app Save Prompt dialog while open.
    pub save_prompt_dialog: Option<SavePromptDialog>,
    /// Saved reusable user prompt snippets (loaded from store).
    pub saved_prompts: Vec<history::SavedPrompt>,
    /// Archived conversations for the active project (newest first).
    pub history: Vec<history::SavedChat>,
    /// ID of the active conversation inside `history`, if archived.
    pub active_history_id: Option<u64>,
    /// True after hiding/restoring a conversation so the next message sends a
    /// concise recap to the fresh backend session.
    pub needs_recap: bool,
    /// Set Up... of the API key or local model, while open.
    pub endpoint: Option<endpoint_dialog::EndpointDialog>,
    /// The box before the first message to a service: its preference
    /// key, its text and the message held back.
    pub use_consent: Option<(String, String, String)>,
    /// A message to send on the next frame (after that box).
    pending_send: Option<String>,
    /// Whether the open project / selection context bar is sent with messages.
    pub include_context: bool,
    /// Expanded finished multi-tool blocks (by item index).
    pub expanded_tools: HashSet<usize>,
    /// Expanded large proposal blocks (by item index).
    pub expanded_proposals: HashSet<usize>,
    /// Proposal ID + history change count right after Apply (for inline Undo).
    pub last_applied_proposal: Option<(u64, u64)>,
    /// Previously sent prompts in this session, for Up/Down recall.
    pub prompt_history: Vec<String>,
    /// Current cursor inside `prompt_history` (`None` = typing fresh draft).
    prompt_cursor: Option<usize>,
    /// Draft held aside while browsing `prompt_history`.
    prompt_stash: String,
    /// Highlighted row in the `/` slash popup or `@` mention popup.
    popup_pick: usize,
    /// Suppress popup for this exact draft string after pressing Escape.
    popup_dismissed: Option<String>,
    /// Frame time (`ctx.input.time`) when the current running turn started.
    turn_started_at: Option<f64>,
    /// Frame number on which `poll_chat_frame` already ran (avoids double polling).
    polled_frame: usize,
    /// Frame number on which `chat_docked_panel` rendered (prevents floating
    /// window from also painting in the same frame when Dock -> Float is clicked).
    dock_rendered_frame: usize,
    /// User messages queued while the agent is still running; sent automatically
    /// when the current turn finishes (or immediately via "Send now").
    pub queued_messages: Vec<String>,
    /// True after a turn finishes naturally (or after Apply) so queued messages
    /// or apply continuations dispatch automatically without restarting after Stop.
    auto_dequeue: bool,
    /// Status summary from a newly applied proposal, waiting to notify the agent
    /// so it sees the applied edit and continues whatever it was working on.
    pending_apply_continue: Option<String>,
}

impl Default for ChatState {
    fn default() -> Self {
        Self {
            open: false,
            docked: false,
            dock_width: DEFAULT_DOCK_WIDTH,
            backend: Box::new(OfflineBackend),
            conv: Conversation::default(),
            agent: None,
            mode: Mode::Ask,
            model: ModelChoice::AgentDefault,
            draft: String::new(),
            consent: None,
            options: None,
            restore: None,
            saved: Prefs::default(),
            focus: false,
            codes: HashSet::new(),
            tips: HashMap::new(),
            activities_index: Vec::new(),
            codes_of: (usize::MAX, u64::MAX, String::new()),
            opener: live::system_opener(),
            picked: false,
            setup: false,
            check: None,
            welcomed: None,
            project_key: None,
            project_rules: String::new(),
            project_rules_dialog: None,
            save_prompt_dialog: None,
            saved_prompts: Vec::new(),
            history: Vec::new(),
            active_history_id: None,
            needs_recap: false,
            endpoint: None,
            use_consent: None,
            pending_send: None,
            include_context: true,
            expanded_tools: HashSet::new(),
            expanded_proposals: HashSet::new(),
            last_applied_proposal: None,
            prompt_history: Vec::new(),
            prompt_cursor: None,
            prompt_stash: String::new(),
            popup_pick: 0,
            popup_dismissed: None,
            turn_started_at: None,
            polled_frame: usize::MAX,
            dock_rendered_frame: usize::MAX,
            queued_messages: Vec::new(),
            auto_dequeue: false,
            pending_apply_continue: None,
            instructions: if cfg!(test) {
                live::test_instructions_path()
            } else {
                openxer_automation::chat_prompt::instructions_path(
                    &openxer_automation::chat_prompt::data_folder(),
                )
            },
        }
    }
}

/// What the user did in the window this frame.
#[derive(Default)]
struct Clicks {
    send: Option<String>,
    queue: Option<String>,
    send_now: Option<String>,
    send_now_queued: Option<usize>,
    drop_queued: Option<usize>,
    stop: bool,
    new_chat: bool,
    agent: Option<usize>,
    model: Option<ModelChoice>,
    mode: Option<Mode>,
    options: bool,
    manage_usage: bool,
    toggle_dock: bool,
    load_history: Option<u64>,
    clear_history: bool,
    run_playbook: Option<String>,
    run_saved_prompt: Option<String>,
    open_save_prompt: bool,
    delete_saved_prompt: Option<usize>,
    open_playbooks_folder: bool,
    open_project_rules: bool,
    list: list::ListOut,
}

impl App {
    /// The AI Agents menu's Chat command (and Ask Chat about Selection).
    pub(crate) fn chat_menu_item(&mut self, ui: &mut egui::Ui) {
        if self.item(
            ui,
            COMMAND,
            Item::new("C&hat").key("F8").checked(self.chat.open),
        ) {
            self.toggle_chat();
        }
        let has_sel = self.chat_has_project() && !self.selected_in_order().is_empty();
        if self.item(
            ui,
            ASK_SELECTION_COMMAND,
            Item::new("&Ask Chat about Selection").enabled(has_sel),
        ) {
            self.ask_chat_about_selection(ui.ctx());
        }
    }

    /// Open Chat ready to ask about the currently selected activities.
    pub(crate) fn ask_chat_about_selection(&mut self, ctx: &egui::Context) {
        self.ensure_chat_backend();
        self.chat.open = true;
        self.chat.focus = true;
        // Unroll if the floating window was rolled up.
        let mut g = kit::geometry(ctx, window_id());
        if g.rolled {
            g.rolled = false;
            kit::set_geometry(ctx, window_id(), g);
        }
        let m = &self.project.model;
        let ids: Vec<String> = self
            .selected_in_order()
            .into_iter()
            .filter_map(|i| m.activities.get(i).map(|a| a.task_code.clone()))
            .collect();
        self.chat.draft = match ids.len() {
            0 => String::new(),
            1 => format!(
                "Explain what drives {} and flag any logic, float or progress issues.",
                ids[0]
            ),
            n => format!(
                "Analyze the {n} selected activities ({}) and explain their logic ties, float and key drivers.",
                ids.iter().take(6).cloned().collect::<Vec<_>>().join(", ")
            ),
        };
    }

    /// Opens Chat, or hides it when it is open (preserving the visible
    /// conversation while cleanly stopping any running backend/temp folder).
    pub(crate) fn toggle_chat(&mut self) {
        if self.chat.open {
            self.hide_chat();
        } else {
            self.ensure_chat_backend();
            self.chat.open = true;
            self.chat.focus = true;
        }
    }

    /// Hides Chat: stops any active backend process and deletes temp folders,
    /// archives the conversation in history, and keeps it visible on reopen.
    pub(crate) fn hide_chat(&mut self) {
        self.archive_current_chat();
        self.chat.open = false;
        self.chat.options = None;
        self.chat.check = None;
        self.chat.project_rules_dialog = None;
        self.chat.save_prompt_dialog = None;
        self.chat.backend.end();
        self.chat.conv.sanitize_restored();
        self.chat.needs_recap = !self.chat.conv.is_empty();
        self.chat.turn_started_at = None;
        self.chat.auto_dequeue = false;
        self.chat.pending_apply_continue = None;
    }

    /// Closes Chat completely: its conversation ends, resets and its files
    /// are removed (used on project switch and app shutdown).
    pub(crate) fn close_chat(&mut self) {
        self.archive_current_chat();
        self.chat.open = false;
        self.chat.options = None;
        self.chat.check = None;
        self.chat.project_rules_dialog = None;
        self.chat.save_prompt_dialog = None;
        self.chat.backend.end();
        self.chat.conv.reset();
        self.chat.active_history_id = None;
        self.chat.needs_recap = false;
        self.chat.expanded_tools.clear();
        self.chat.expanded_proposals.clear();
        self.chat.last_applied_proposal = None;
        self.chat.turn_started_at = None;
        self.chat.queued_messages.clear();
        self.chat.auto_dequeue = false;
        self.chat.pending_apply_continue = None;
    }

    /// The real backend, made when Chat first opens (detection starts then,
    /// not at every start of the app).
    pub(crate) fn ensure_chat_backend(&mut self) {
        if self.chat.backend.is_offline() {
            let mut config = live::Config::system(self.agent_env(), self.chat.opener.clone());
            config.instructions = self.chat.instructions.clone();
            self.chat.backend = Box::new(live::LiveBackend::new(config));
            self.restore_endpoint();
        }
    }

    /// Opens `url` in the browser, or says why not.
    pub(crate) fn chat_open_url(&mut self, url: &str) {
        if let Err(e) = (self.chat.opener)(url) {
            self.status = e;
        }
    }

    /// Changes whenever the open project changes: an edit, an undo or redo,
    /// another project.
    pub(crate) fn chat_revision(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (
            self.history.changes(),
            self.records_revision,
            self.project.model.project.proj_id,
            &self.project.source_name,
        )
            .hash(&mut h);
        h.finish()
    }

    /// Which project is open (a conversation ends with it).
    fn chat_project_key(&self) -> String {
        if !self.chat_has_project() {
            return String::new();
        }
        format!(
            "{}\u{1}{}",
            self.project.source_name, self.project.model.project.proj_id
        )
    }

    /// Reads the stored window place, agent, mode, dock state and saved prompts (at start).
    pub(crate) fn restore_chat(&mut self) {
        let prefs: Prefs = self
            .store
            .preference(PREF_KEY)
            .ok()
            .flatten()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        self.chat.restore = Some(prefs.geometry);
        self.chat.agent = prefs.agent.clone();
        self.chat.mode = prefs.mode;
        self.chat.model = prefs.model.clone();
        self.chat.docked = prefs.docked;
        self.chat.dock_width = prefs
            .dock_width
            .unwrap_or(DEFAULT_DOCK_WIDTH)
            .max(MIN_DOCK_WIDTH);
        self.chat.saved = prefs;
        self.chat.saved_prompts = self
            .store
            .preference(history::SAVED_PROMPTS_KEY)
            .ok()
            .flatten()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
    }

    fn load_project_chat_state(&mut self, key: &str) {
        if key.is_empty() {
            self.chat.history.clear();
            self.chat.project_rules.clear();
            return;
        }
        self.chat.history = self
            .store
            .preference(&history::history_pref_key(key))
            .ok()
            .flatten()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        self.chat.project_rules = self
            .store
            .preference(&history::project_rules_pref_key(key))
            .ok()
            .flatten()
            .unwrap_or_default();
    }

    fn save_chat_history(&mut self) {
        let Some(key) = self.chat.project_key.as_deref().filter(|k| !k.is_empty()) else {
            return;
        };
        if let Ok(text) = serde_json::to_string(&self.chat.history) {
            if let Err(e) = self
                .store
                .set_preference(&history::history_pref_key(key), &text)
            {
                self.status = e;
            }
        }
    }

    fn archive_current_chat(&mut self) {
        if self.chat.conv.is_empty() {
            return;
        }
        let id = history::archive(
            &mut self.chat.history,
            self.chat.active_history_id,
            self.chat.agent.clone(),
            &self.chat.conv,
        );
        self.chat.active_history_id = id;
        self.save_chat_history();
    }

    fn save_saved_prompts(&mut self) {
        if let Ok(text) = serde_json::to_string(&self.chat.saved_prompts) {
            if let Err(e) = self.store.set_preference(history::SAVED_PROMPTS_KEY, &text) {
                self.status = e;
            }
        }
    }

    fn save_project_rules(&mut self) {
        let Some(key) = self.chat.project_key.as_deref().filter(|k| !k.is_empty()) else {
            return;
        };
        if let Err(e) = self.store.set_preference(
            &history::project_rules_pref_key(key),
            self.chat.project_rules.trim(),
        ) {
            self.status = e;
        }
    }

    /// Writes the window's place, agent, mode and dock state when they changed
    /// and the mouse is up (not on every pixel of a drag).
    fn persist_chat(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        let mut geometry = kit::geometry(ctx, window_id());
        if self.chat.restore.is_some() && !self.chat.docked {
            return;
        }
        if geometry.pos.is_none() {
            geometry = self.chat.saved.geometry;
        }
        let dock_width =
            if self.chat.docked || (self.chat.dock_width - DEFAULT_DOCK_WIDTH).abs() > 0.5 {
                Some(self.chat.dock_width.round())
            } else {
                self.chat.saved.dock_width
            };
        let prefs = Prefs {
            geometry,
            agent: self.chat.agent.clone(),
            mode: self.chat.mode,
            model: self.chat.model.clone(),
            docked: self.chat.docked,
            dock_width,
        };
        if prefs != self.chat.saved {
            if let Ok(text) = serde_json::to_string(&prefs) {
                if let Err(e) = self.store.set_preference(PREF_KEY, &text) {
                    self.status = e;
                }
            }
            self.chat.saved = prefs;
        }
    }

    /// F8 opens and hides Chat when no other dialog has the keyboard.
    pub(crate) fn chat_keys(&mut self, ctx: &egui::Context) {
        let pressed = ctx.input(|i| i.key_pressed(Key::F8) && i.modifiers.is_none());
        if !pressed {
            return;
        }
        let open = kit::open_dialogs(ctx);
        let only_chat = open.iter().all(|n| n == SALT);
        if only_chat && (open.iter().any(|n| n == SALT) || !self.modal_open(ctx)) {
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::F8));
            self.toggle_chat();
        }
    }

    /// There is a project to ask about.
    pub(crate) fn chat_has_project(&self) -> bool {
        !self.project.model.project.name.trim().is_empty()
    }

    /// What the agent is told with every message: project, selection, data
    /// date, plus active baseline, filter and open Details tab.
    pub(crate) fn chat_context(&self) -> Option<ChatContext> {
        if !self.chat_has_project() {
            return None;
        }
        let m = &self.project.model;
        let ids: Vec<String> = self
            .selected_in_order()
            .into_iter()
            .filter_map(|i| m.activities.get(i).map(|a| a.task_code.clone()))
            .collect();
        let selection = match ids.len() {
            0 => Selection::None,
            1 => Selection::One(ids[0].clone()),
            _ => Selection::Many(ids),
        };
        let bl = self.project_baseline_name();
        let baseline = (!bl.is_empty() && bl != "<Current Project>").then_some(bl);
        let flt = self.filter_bar_name();
        let filter = (!flt.is_empty() && flt != "All Activities").then_some(flt);
        let detail_tab = (self.panes.bottom == crate::panes::BottomView::Details)
            .then(|| self.detail.tab.caption().to_string());
        Some(ChatContext {
            project_id: m.project.name.clone(),
            selection,
            data_date: crate::format::date(m.origin_serial, m.project.data_date),
            baseline,
            filter,
            detail_tab,
        })
    }

    /// Schedule-aware starter questions: keeps the driving-path starter first
    /// and a selected-activity starter second, then adapts the rest to the
    /// open schedule's health and the active mode.
    pub(crate) fn chat_starters(&self) -> Vec<String> {
        let mut out: Vec<String> = vec![crate::ai_agents::THINGS_TO_ASK[0].to_string()];
        let m = &self.project.model;
        let selected_ids = self.selected_in_order();
        if selected_ids.len() > 1 {
            out.push(format!(
                "Trace logic and float across the {} selected activities.",
                selected_ids.len()
            ));
        } else if let Some(a) = self.selected.and_then(|i| m.activities.get(i)) {
            let q = if a.total_float.is_some_and(|f| f <= 0) {
                format!("Why does {} have zero or negative float?", a.task_code)
            } else {
                format!("What drives the dates of {}?", a.task_code)
            };
            out.push(q);
        }
        let has_neg_float = m
            .activities
            .iter()
            .any(|a| a.total_float.is_some_and(|f| f < 0));
        if has_neg_float {
            out.push("Why is there negative float, and how can we recover time?".into());
        }
        out.push("Run Check Schedule and prioritize the top fixes.".into());
        if self.chat.mode == Mode::Edit {
            out.push("Propose fixes for open ends and missing logic ties.".into());
        } else {
            out.push("Build a 3-week field lookahead with readiness risks.".into());
        }
        out.push("Draft a weekly executive schedule narrative.".into());
        out.truncate(5);
        out
    }

    fn refresh_chat_codes(&mut self) {
        let m = &self.project.model;
        let key = (
            m.activities.len(),
            self.records_revision,
            m.project.name.clone(),
        );
        if key == self.chat.codes_of {
            return;
        }
        if self.chat_has_project() {
            self.chat.codes = m.activities.iter().map(|a| a.task_code.clone()).collect();
            self.chat.tips.clear();
            self.chat.activities_index.clear();
            for (idx, a) in m.activities.iter().enumerate() {
                let periods = crate::format::activity_periods(&self.project, idx);
                let name = self
                    .project
                    .details
                    .get(&a.task_id)
                    .map(|d| d.name.as_str())
                    .unwrap_or("");
                let status = crate::status_label(a.status);
                let es = crate::format::date(
                    m.origin_serial,
                    crate::displayed_start(a, m.options.do_level),
                );
                let ef = crate::format::date(
                    m.origin_serial,
                    crate::displayed_finish(a, m.options.do_level),
                );
                let tf = a
                    .total_float
                    .map(|f| crate::format::duration(f, periods))
                    .unwrap_or_else(|| "-".into());
                let title = if name.is_empty() {
                    a.task_code.clone()
                } else {
                    format!("{} - {name}", a.task_code)
                };
                self.chat.tips.insert(
                    a.task_code.clone(),
                    format!("{title}\n{status} \u{b7} {es} to {ef} \u{b7} TF {tf}"),
                );
                self.chat
                    .activities_index
                    .push((a.task_code.clone(), name.to_string()));
            }
        } else {
            self.chat.codes.clear();
            self.chat.tips.clear();
            self.chat.activities_index.clear();
        }
        self.chat.codes_of = key;
    }

    /// The agent the window uses: the one chosen in this session; else the
    /// remembered one while it is ready (or still being looked for); else
    /// the first ready one; else the first.
    fn chat_agent(&self, agents: &[AgentInfo]) -> Option<usize> {
        let chosen = self
            .chat
            .agent
            .as_ref()
            .and_then(|id| agents.iter().position(|a| &a.id == id));
        match chosen {
            Some(i)
                if self.chat.picked
                    || agents[i].state.usable()
                    || agents[i].state == AgentState::Checking =>
            {
                Some(i)
            }
            _ => agents
                .iter()
                .position(|a| a.state.usable())
                .or(chosen)
                .or((!agents.is_empty()).then_some(0)),
        }
    }

    /// Whether the list shows how to connect: at the user's request, or
    /// when no assistant is ready and none was chosen.
    fn chat_shows_setup(&self, agents: &[AgentInfo], agent: Option<usize>) -> bool {
        if self.chat.setup {
            return true;
        }
        if !self.chat.conv.is_empty() || agents.is_empty() {
            return false;
        }
        match agent.map(|i| &agents[i]) {
            None => true,
            Some(a) if a.state.usable() || a.state == AgentState::Checking => false,
            Some(_) => !self.chat.picked,
        }
    }

    /// Runs once per frame (before docked panel or floating window draws):
    /// polls backend events, handles project changes and repaint cadence.
    fn poll_chat_frame(&mut self, ctx: &egui::Context) {
        if self.chat.polled_frame == self.frame_number {
            return;
        }
        self.chat.polled_frame = self.frame_number;
        let was_running = self.chat.conv.running;
        for event in self.chat.backend.poll() {
            if let backend::AgentEvent::UiAction(payload) = event {
                if payload["action"].as_str() == Some("ask_user") {
                    if let Some((title, prompts)) = parse_ask_user_payload(&payload) {
                        self.chat.conv.ask_question(title, prompts);
                    }
                } else {
                    match self.apply_chat_ui_action(ctx, &payload) {
                        Ok(summary) => self.chat.conv.note(summary),
                        Err(err) => self.chat.conv.note(format!("App action failed: {err}")),
                    }
                }
            } else {
                self.chat.conv.apply(event);
            }
        }
        if was_running && !self.chat.conv.running {
            self.chat.turn_started_at = None;
            self.chat.auto_dequeue = true;
            self.archive_current_chat();
        }
        // A conversation is about one project: it ends when that changes.
        let key = self.chat_project_key();
        if self.chat.project_key.as_ref() != Some(&key) {
            if self.chat.project_key.is_some() {
                self.archive_current_chat();
                self.chat.backend.end();
                self.chat.conv.reset();
                self.chat.active_history_id = None;
                self.chat.needs_recap = false;
                self.chat.expanded_tools.clear();
                self.chat.expanded_proposals.clear();
                self.chat.last_applied_proposal = None;
                self.chat.queued_messages.clear();
                self.chat.auto_dequeue = false;
                self.chat.pending_apply_continue = None;
            }
            self.chat.project_key = Some(key.clone());
            self.load_project_chat_state(&key);
        }
        // Drop inline Undo badge if user made another edit or already undid.
        if let Some((_, change_count)) = self.chat.last_applied_proposal {
            if self.history.changes() != change_count {
                self.chat.last_applied_proposal = None;
            }
        }
        if self.chat.conv.running || self.chat.open {
            // Replies, detection, sign-ins and checks arrive between frames.
            ctx.request_repaint_after(std::time::Duration::from_millis(
                if self.chat.conv.running { 50 } else { 250 },
            ));
        }
    }

    /// Docked right-hand split pane when `self.chat.open && self.chat.docked`.
    /// Called before `view_area(ctx)` so the main table/Gantt resizes cleanly.
    pub(crate) fn chat_docked_panel(&mut self, ctx: &egui::Context) {
        self.poll_chat_frame(ctx);
        if !self.chat.open || !self.chat.docked {
            return;
        }
        self.chat.dock_rendered_frame = self.frame_number;
        self.refresh_chat_codes();
        let agents = self.chat.backend.agents();
        let agent = self.chat_agent(&agents);
        let context = self.chat_context();
        let starters = self.chat_starters();
        let setup_rows = if self.chat_shows_setup(&agents, agent) {
            Some(agents.iter().map(setup_row).collect::<Vec<_>>())
        } else {
            None
        };
        let playbooks = prompts::available_playbooks(&self.chat.instructions);
        let screen = ctx.content_rect();
        let max_w = (screen.width() * 0.55).max(MIN_DOCK_WIDTH + 20.);
        let width = self.chat.dock_width.clamp(MIN_DOCK_WIDTH, max_w).round();
        let focus = std::mem::take(&mut self.chat.focus);
        let time = ctx.input(|i| i.time);
        let elapsed_secs = self
            .chat
            .turn_started_at
            .filter(|_| self.chat.conv.running)
            .map(|t| (time - t).max(0.) as u64);
        let undoable_proposal = self
            .chat
            .last_applied_proposal
            .filter(|(_, c)| *c == self.history.changes())
            .map(|(id, _)| id);
        let mut k = Clicks::default();
        let mut actions = vec![];
        let mut closed = false;
        let mut new_dock_width = None;
        let splitter_w = crate::activities::SPLITTER;
        egui::SidePanel::right("chat_docked_panel")
            .resizable(false)
            .exact_width(width + splitter_w)
            .frame(egui::Frame::NONE.fill(kit::FACE))
            .show(ctx, |ui| {
                let full = ui.max_rect();
                // Left vertical splitter using the shared Activities splitter painter.
                let splitter = Rect::from_min_max(
                    full.min,
                    Pos2::new(full.left() + splitter_w, full.bottom()),
                );
                crate::activities::paint_splitter(ui.painter(), splitter, true);
                let drag = ui
                    .interact(splitter, ui.id().with("chat_dock_splitter"), Sense::drag())
                    .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                actions.push(("chat_dock_splitter".into(), splitter));
                if drag.dragged() {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        new_dock_width = Some(
                            (full.right() - pos.x - splitter_w / 2.).clamp(MIN_DOCK_WIDTH, max_w),
                        );
                    }
                }
                let panel = Rect::from_min_max(Pos2::new(splitter.right(), full.top()), full.max);
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt("chat_docked_body")
                        .max_rect(panel)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                child.set_clip_rect(panel);
                let title_h = 22.;
                let title_rect = Rect::from_min_size(panel.min, Vec2::new(panel.width(), title_h));
                let p = child.painter();
                p.rect_filled(title_rect, 0., chrome::pal::CAPTION_BG);
                p.hline(
                    title_rect.x_range(),
                    title_rect.bottom() - 0.5,
                    Stroke::new(1_f32, chrome::pal::CAPTION_LINE),
                );
                let title_text = match self.chat.conv.status() {
                    Some(s) => format!("{TITLE} - {s}"),
                    None => TITLE.to_string(),
                };
                p.text(
                    Pos2::new(title_rect.left() + 8., title_rect.center().y),
                    Align2::LEFT_CENTER,
                    title_text,
                    chrome::bold_font(),
                    INK,
                );
                // Close button at top-right of the docked caption bar.
                let close_rect = Rect::from_min_size(
                    Pos2::new(title_rect.right() - 20., title_rect.top() + 3.),
                    Vec2::new(16., 16.),
                );
                let close_resp = child.interact(
                    close_rect,
                    child.id().with("chat_dock_close"),
                    Sense::click(),
                );
                if close_resp.hovered() {
                    p.rect_filled(close_rect, 0., chrome::pal::HOT_FILL);
                    p.rect_stroke(
                        close_rect.shrink(0.5),
                        0.,
                        Stroke::new(1_f32, chrome::pal::HOT_BORDER),
                        egui::StrokeKind::Middle,
                    );
                }
                let c0 = close_rect.center();
                p.line_segment(
                    [c0 + Vec2::new(-3.5, -3.5), c0 + Vec2::new(3.5, 3.5)],
                    Stroke::new(1.2_f32, INK),
                );
                p.line_segment(
                    [c0 + Vec2::new(-3.5, 3.5), c0 + Vec2::new(3.5, -3.5)],
                    Stroke::new(1.2_f32, INK),
                );
                actions.push(("chat_title_close".into(), close_rect));
                actions.push(("chat_title".into(), title_rect));
                if close_resp.clicked() {
                    closed = true;
                }

                let body_origin = Pos2::new(panel.left(), title_rect.bottom());
                let body_size = Vec2::new(panel.width(), (panel.height() - title_h).max(40.));
                let mut c = Canvas {
                    ui: &mut child,
                    origin: body_origin,
                    salt: SALT,
                    actions: vec![],
                };
                let view = View {
                    agents: &agents,
                    agent,
                    context: context.as_ref(),
                    starters: &starters,
                    focus,
                    setup: setup_rows.as_deref(),
                    playbooks: &playbooks,
                    undoable_proposal,
                    elapsed_secs,
                };
                body(&mut c, body_size, &view, &mut self.chat, &mut k);
                actions.extend(std::mem::take(&mut c.actions));
            });
        if let Some(w) = new_dock_width {
            self.chat.dock_width = w;
        }
        actions.extend(std::mem::take(&mut k.list.actions));
        self.record_dialog_actions(actions);
        if closed {
            self.hide_chat();
            return;
        }
        self.chat_act(ctx, k, &agents, agent, context, &playbooks, time);
        self.persist_chat(ctx);
    }

    /// The floating window and Chat's child modals, each frame.
    pub(crate) fn chat_window(&mut self, ctx: &egui::Context) {
        self.poll_chat_frame(ctx);
        self.chat_consent_box(ctx);
        self.chat_use_consent_box(ctx);
        self.chat_options_dialog(ctx);
        self.chat_project_rules_dialog(ctx);
        self.chat_save_prompt_dialog(ctx);
        self.chat_welcome_box(ctx);
        self.chat_check_dialog(ctx);
        self.chat_endpoint_dialog(ctx);
        if !self.chat.open || self.chat.docked || self.chat.dock_rendered_frame == self.frame_number
        {
            return;
        }
        if let Some(g) = self.chat.restore.take() {
            kit::set_geometry(ctx, window_id(), g);
        }
        self.refresh_chat_codes();
        let agents = self.chat.backend.agents();
        let agent = self.chat_agent(&agents);
        let context = self.chat_context();
        let starters = self.chat_starters();
        let setup_rows = if self.chat_shows_setup(&agents, agent) {
            Some(agents.iter().map(setup_row).collect::<Vec<_>>())
        } else {
            None
        };
        let playbooks = prompts::available_playbooks(&self.chat.instructions);
        let rolled = kit::geometry(ctx, window_id()).rolled;
        let title = match self.chat.conv.status().filter(|_| rolled) {
            Some(s) => format!("{TITLE} - {s}"),
            None => TITLE.to_string(),
        };
        let screen = ctx.content_rect();
        let outer = CLIENT + Vec2::new(2., kit::TITLE_H + 2.);
        let at = Pos2::new(screen.right() - outer.x - 40., screen.top() + 142.);
        let focus = std::mem::take(&mut self.chat.focus);
        let time = ctx.input(|i| i.time);
        let elapsed_secs = self
            .chat
            .turn_started_at
            .filter(|_| self.chat.conv.running)
            .map(|t| (time - t).max(0.) as u64);
        let undoable_proposal = self
            .chat
            .last_applied_proposal
            .filter(|(_, c)| *c == self.history.changes())
            .map(|(id, _)| id);
        let mut k = Clicks::default();
        let mut actions = vec![];
        let out = kit::shell_with(
            ctx,
            kit::Shell {
                name: SALT,
                id: window_id(),
                title: &title,
                client: CLIENT,
                layering: kit::Layering::Window,
                at: Some(at),
                extras: kit::Extras {
                    roll_up: true,
                    min_client: Some(MIN_CLIENT),
                    tool: true,
                },
            },
            |ui, origin, size| {
                let mut c = Canvas {
                    ui,
                    origin,
                    salt: SALT,
                    actions: vec![],
                };
                let view = View {
                    agents: &agents,
                    agent,
                    context: context.as_ref(),
                    starters: &starters,
                    focus,
                    setup: setup_rows.as_deref(),
                    playbooks: &playbooks,
                    undoable_proposal,
                    elapsed_secs,
                };
                body(&mut c, size, &view, &mut self.chat, &mut k);
                actions = std::mem::take(&mut c.actions);
            },
        );
        actions.insert(0, (format!("{SALT}_title_close"), out.close));
        if let Some(r) = out.roll {
            actions.push((format!("{SALT}_roll"), r));
        }
        actions.push((format!("{SALT}_title"), out.grip));
        if let Some(r) = out.size_grip {
            actions.push((format!("{SALT}_size_grip"), r));
        }
        actions.extend(std::mem::take(&mut k.list.actions));
        self.record_dialog_actions(actions);
        if out.closed {
            self.hide_chat();
            return;
        }
        self.chat_act(ctx, k, &agents, agent, context, &playbooks, time);
        self.persist_chat(ctx);
    }

    /// Runs what the user did in the window.
    #[allow(clippy::too_many_arguments)]
    fn chat_act(
        &mut self,
        ctx: &egui::Context,
        k: Clicks,
        agents: &[AgentInfo],
        agent: Option<usize>,
        context: Option<ChatContext>,
        playbooks: &[openxer_automation::playbooks::PlaybookEntry],
        time: f64,
    ) {
        let current = agent.map(|i| agents[i].clone());
        if k.toggle_dock {
            self.chat.docked = !self.chat.docked;
            self.chat.focus = true;
            return;
        }
        if let Some(hist_id) = k.load_history {
            self.archive_current_chat();
            if let Some(saved) = self.chat.history.iter().find(|s| s.id == hist_id).cloned() {
                if self.chat.conv.running {
                    self.chat.backend.stop();
                }
                self.chat.backend.end();
                self.chat.conv = saved.conv;
                self.chat.conv.sanitize_restored();
                self.chat.active_history_id = Some(saved.id);
                self.chat.needs_recap = !self.chat.conv.is_empty();
                self.chat.expanded_tools.clear();
                self.chat.expanded_proposals.clear();
                self.chat.last_applied_proposal = None;
                self.chat.queued_messages.clear();
                self.chat.auto_dequeue = false;
                self.chat.pending_apply_continue = None;
                self.chat.focus = true;
            }
            return;
        }
        if k.clear_history {
            self.chat.history.clear();
            self.chat.active_history_id = None;
            self.save_chat_history();
            self.status = "Chat history cleared for this project.".into();
            return;
        }
        if let Some(model) = k.model {
            self.chat.model = model;
        }
        if k.open_playbooks_folder {
            self.open_chat_playbooks_folder();
            return;
        }
        if k.open_project_rules {
            self.chat.project_rules_dialog = Some(ProjectRulesDialog {
                text: self.chat.project_rules.clone(),
            });
            return;
        }
        if k.open_save_prompt {
            let trimmed = self.chat.draft.trim();
            let default_name: String = trimmed
                .split_whitespace()
                .take(4)
                .collect::<Vec<_>>()
                .join(" ");
            self.chat.save_prompt_dialog = Some(SavePromptDialog {
                name: default_name,
                text: trimmed.to_string(),
            });
            return;
        }
        if let Some(idx) = k.delete_saved_prompt {
            if idx < self.chat.saved_prompts.len() {
                let removed = self.chat.saved_prompts.remove(idx);
                self.save_saved_prompts();
                self.status = format!("Removed saved prompt \"{}\".", removed.name);
            }
            return;
        }
        if let Some(i) = k.agent {
            if i >= agents.len() {
                self.chat.setup = true;
                return;
            }
            self.chat.setup = false;
            self.chat.picked = true;
            if agents[i].ask_only {
                self.chat.mode = Mode::Ask;
            }
            let same = agent == Some(i);
            // The agent on screen may be a fallback: the pick is what counts.
            self.chat.agent = Some(agents[i].id.clone());
            if !same {
                self.archive_current_chat();
                if self.chat.conv.running {
                    self.chat.backend.stop();
                }
                self.chat.agent = Some(agents[i].id.clone());
                self.chat.model = ModelChoice::AgentDefault;
                self.chat.conv.reset();
                self.chat.active_history_id = None;
                self.chat.needs_recap = false;
                self.chat.expanded_tools.clear();
                self.chat.expanded_proposals.clear();
                self.chat.last_applied_proposal = None;
                self.chat.queued_messages.clear();
                self.chat.auto_dequeue = false;
                self.chat.pending_apply_continue = None;
                self.chat.backend.new_conversation(&agents[i].id);
            }
            return;
        }
        if let Some(mode) = k.mode {
            self.chat.mode = mode;
        }
        if k.new_chat {
            self.archive_current_chat();
            if self.chat.conv.running {
                self.chat.backend.stop();
            }
            self.chat.conv.reset();
            self.chat.active_history_id = None;
            self.chat.needs_recap = false;
            self.chat.expanded_tools.clear();
            self.chat.expanded_proposals.clear();
            self.chat.last_applied_proposal = None;
            self.chat.turn_started_at = None;
            self.chat.queued_messages.clear();
            self.chat.auto_dequeue = false;
            self.chat.pending_apply_continue = None;
            if let Some(a) = &current {
                self.chat.backend.new_conversation(&a.id);
            }
            self.chat.focus = true;
        }
        if k.stop && self.chat.conv.stop() {
            self.chat.backend.stop();
            self.chat.turn_started_at = None;
            self.chat.auto_dequeue = false;
            self.chat.pending_apply_continue = None;
        }
        if let Some(q) = k.queue {
            let trimmed = q.trim();
            if !trimmed.is_empty() {
                let expanded = prompts::expand_slash_send(
                    trimmed,
                    playbooks,
                    &self.chat.saved_prompts,
                    context.as_ref(),
                )
                .unwrap_or_else(|| trimmed.to_string());
                self.chat.queued_messages.push(expanded);
                self.chat.draft.clear();
                self.chat.popup_dismissed = None;
                self.chat.focus = true;
            }
        }
        if let Some(idx) = k.drop_queued {
            if idx < self.chat.queued_messages.len() {
                self.chat.queued_messages.remove(idx);
            }
        }
        let mut interrupt_send: Option<(String, bool)> = None;
        if let Some(text) = k.send_now {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                interrupt_send = Some((trimmed.to_string(), true));
            }
        } else if let Some(idx) = k.send_now_queued {
            if idx < self.chat.queued_messages.len() {
                let text = self.chat.queued_messages.remove(idx);
                interrupt_send = Some((text, false));
            }
        }
        if interrupt_send.is_some() && self.chat.conv.running {
            self.chat.conv.stop();
            self.chat.backend.stop();
            self.chat.turn_started_at = None;
            self.chat.pending_apply_continue = None;
        }
        if let Some((text, status)) = k.list.copy_text {
            ctx.copy_text(text);
            self.status = status.into();
        }
        if let Some(ids) = k.list.select_all {
            self.chat_select_many(&ids);
        }
        if let Some(idx) = k.list.toggle_tools {
            if !self.chat.expanded_tools.remove(&idx) {
                self.chat.expanded_tools.insert(idx);
            }
        }
        if let Some(idx) = k.list.toggle_proposal {
            if !self.chat.expanded_proposals.remove(&idx) {
                self.chat.expanded_proposals.insert(idx);
            }
        }
        if let Some(id) = k.list.undo_proposal {
            if self
                .chat
                .last_applied_proposal
                .is_some_and(|(last_id, c)| last_id == id && c == self.history.changes())
            {
                self.history_move(ctx, false);
                self.chat.last_applied_proposal = None;
                self.chat.pending_apply_continue = None;
                self.chat.conv.note("Undid the last applied proposal.");
            }
        }
        if let Some(qid) = k.list.dismiss_question {
            self.chat
                .conv
                .resolve_question(qid, model::QuestionState::Dismissed);
        }
        let question_reply = if let Some((qid, answer)) = k.list.answer_question {
            self.chat
                .conv
                .resolve_question(qid, model::QuestionState::Answered(answer.clone()));
            Some(answer)
        } else {
            None
        };
        let mut retry_text = None;
        if k.list.switch_edit_retry {
            self.chat.mode = Mode::Edit;
            retry_text = self.chat.conv.last_user_prompt().map(str::to_string);
        }
        let ready = current.as_ref().is_some_and(|a| a.state.usable());
        let from_composer = k.send.is_some() || interrupt_send.as_ref().is_some_and(|(_, fc)| *fc);
        let dequeued = if !self.chat.conv.running
            && self.chat.auto_dequeue
            && ready
            && context.is_some()
            && !self.chat.queued_messages.is_empty()
        {
            self.chat.auto_dequeue = false;
            Some(self.chat.queued_messages.remove(0))
        } else {
            None
        };
        let raw_text = interrupt_send
            .map(|(t, _)| t)
            .or(k.send)
            .or(question_reply)
            .or(dequeued)
            .or_else(|| {
                k.run_playbook
                    .map(|name| prompts::playbook_prompt(&name, ""))
            })
            .or_else(|| {
                k.run_saved_prompt
                    .map(|tpl| history::expand_template(&tpl, context.as_ref()))
            })
            .or(retry_text)
            .or_else(|| {
                k.list
                    .starter
                    .and_then(|n| self.chat_starters().get(n).cloned())
            })
            .or_else(|| self.chat.pending_send.take());
        let text = raw_text.map(|t| {
            prompts::expand_slash_send(&t, playbooks, &self.chat.saved_prompts, context.as_ref())
                .unwrap_or(t)
        });
        // The first message to a service on another computer asks first.
        let ask = |s: &Self, a: &AgentInfo| -> Option<(String, String)> {
            a.use_consent
                .clone()
                .filter(|(key, _)| s.store.preference(key).ok().flatten().is_none())
        };
        let text = match (text, &current) {
            (Some(text), Some(a)) if ready && context.is_some() => match ask(self, a) {
                Some((key, why)) => {
                    self.chat.use_consent = Some((key, why, text));
                    None
                }
                None => Some(text),
            },
            (text, _) => text,
        };
        if let (Some(text), Some(context), Some(a), true) = (text, &context, &current, ready) {
            if self.chat.conv.send(&text) {
                // Record in prompt history for Up/Down recall.
                let trimmed = text.trim().to_string();
                if self.chat.prompt_history.last() != Some(&trimmed) {
                    self.chat.prompt_history.push(trimmed);
                    if self.chat.prompt_history.len() > 50 {
                        self.chat.prompt_history.remove(0);
                    }
                }
                self.chat.prompt_cursor = None;
                self.chat.prompt_stash.clear();
                self.dispatch_chat_turn(a, text.trim(), context, time);
                if from_composer {
                    self.chat.draft.clear();
                    self.chat.popup_dismissed = None;
                }
            }
        }
        if let Some(id) = k.list.apply {
            if let Some(status) = self.chat_apply(id, time) {
                self.chat.pending_apply_continue = Some(status);
                if !self.chat.conv.running {
                    self.chat.auto_dequeue = true;
                }
            }
        }
        if let Some(id) = k.list.discard {
            self.chat.backend.discard(id);
            self.chat.conv.resolve(id, ProposalState::Discarded);
        }
        // Automatically let the agent see an applied proposal and continue whatever
        // workflow it was doing, once the current turn is idle.
        if !self.chat.conv.running
            && self.chat.auto_dequeue
            && ready
            && self.chat.queued_messages.is_empty()
        {
            if let (Some(status), Some(a)) = (self.chat.pending_apply_continue.take(), &current) {
                if let Some(fresh_ctx) = self.chat_context() {
                    if self.chat.conv.begin_followup() {
                        let followup = format!(
                            "I applied the proposed changes ({status}). The open schedule now reflects those changes. Continue with any remaining steps, or briefly confirm if everything requested is done."
                        );
                        self.dispatch_chat_turn(a, &followup, &fresh_ctx, time);
                    }
                }
            }
            self.chat.auto_dequeue = false;
        }
        if let Some(a) = &current {
            if k.list.sign_in {
                self.chat_sign_in(a);
            }
            if k.list.cancel_sign_in {
                self.chat.backend.cancel_sign_in(&a.id);
            }
        }
        if let Some((n, second)) = k.list.setup {
            if let Some(a) = agents.get(n) {
                self.chat_setup_action(a, second, current.as_ref());
            }
        }
        if k.list.check_setup {
            self.open_check_setup();
        }
        if k.list.set_up {
            self.open_endpoint_dialog();
        }
        if k.list.manage_usage || k.manage_usage {
            self.chat_open_url(openxer_chat::USAGE_SETTINGS_URL);
        }
        if k.options {
            let choices = model_choices(current.as_ref());
            let at = choices
                .iter()
                .position(|m| *m == self.chat.model)
                .unwrap_or(0);
            self.chat.options = Some(at);
        }
        if let Some(code) = k.list.link {
            self.chat_go_to(&code);
        }
    }

    /// Dispatch one turn to the active backend (used for user prompts,
    /// queued messages, question answers, and post-Apply continuations).
    fn dispatch_chat_turn(&mut self, a: &AgentInfo, text: &str, context: &ChatContext, time: f64) {
        let recap = if self.chat.needs_recap {
            self.chat.conv.recap_text()
        } else {
            None
        };
        self.chat.needs_recap = false;
        self.chat.turn_started_at = Some(time);
        self.chat.agent = Some(a.id.clone());
        self.chat.picked = true;
        let revision = self.chat_revision();
        let empty_context = ChatContext::default();
        let sent_context = if self.chat.include_context {
            context
        } else {
            &empty_context
        };
        let project_rules = self.chat.project_rules.trim();
        let project_instructions = (!project_rules.is_empty()).then_some(project_rules);
        let turn = Turn {
            agent: &a.id,
            text,
            mode: self.chat.mode,
            context: sent_context,
            model: &self.chat.model,
            project: Some(&self.project),
            revision,
            project_instructions,
            recap: recap.as_deref(),
        };
        self.chat.backend.send(turn);
    }

    /// Apply: the proposal worked out on the open project and put in place
    /// as one undo step; or its refreshed rows when the project changed
    /// since it was made and the change now reads differently. Returns
    /// `Some(status)` when the project actually changed.
    fn chat_apply(&mut self, id: u64, time: f64) -> Option<String> {
        let revision = self.chat_revision();
        match self.chat.backend.apply(id, &self.project, revision) {
            Ok(Applied::Nothing) => {
                self.chat.conv.resolve(id, ProposalState::Applied);
                None
            }
            Ok(Applied::Project(next, status)) => {
                let before = std::mem::replace(&mut self.project, *next);
                let before_revision = self.records_revision;
                self.edit_step(before, before_revision, time);
                self.drop_frame_cache();
                let n = self.project.model.activities.len();
                self.selected = self.selected.filter(|&i| i < n);
                self.status = status.clone();
                self.chat.conv.resolve(id, ProposalState::Applied);
                self.chat.last_applied_proposal = Some((id, self.history.changes()));
                Some(status)
            }
            Ok(Applied::Refreshed(rows)) => {
                self.chat.conv.refresh(id, rows);
                self.chat.conv.note(
                    "The project changed since this was proposed. Look at the changes again and apply them if they are still right.",
                );
                None
            }
            Err(e) => {
                self.chat.conv.resolve(id, ProposalState::Failed(e));
                None
            }
        }
    }

    /// Sign In... for `a`: its consent first, once, when it has one.
    fn chat_sign_in(&mut self, a: &AgentInfo) {
        let asked = a.consent.is_some()
            && self
                .store
                .preference(&consent_key(&a.id))
                .ok()
                .flatten()
                .is_none();
        if asked {
            self.chat.consent = Some(a.id.clone());
        } else {
            self.chat.backend.sign_in(&a.id);
        }
    }

    /// A setup row's button.
    fn chat_setup_action(&mut self, a: &AgentInfo, second: bool, current: Option<&AgentInfo>) {
        if a.configurable && (second || a.state == AgentState::NotSetUp) {
            self.open_endpoint_dialog();
            return;
        }
        if a.ask_only {
            self.chat.mode = Mode::Ask;
        }
        match (&a.state, second) {
            (AgentState::NotInstalled, true) => self.chat.backend.check_again(&a.id),
            (AgentState::NotInstalled, false) => {
                if let Some(url) = a.install_url.clone() {
                    self.chat_open_url(&url);
                }
            }
            (AgentState::SignInNeeded, _) => self.chat_sign_in(a),
            (AgentState::SigningIn, _) => self.chat.backend.cancel_sign_in(&a.id),
            (state, _) if state.usable() => {
                if current.is_none_or(|c| c.id != a.id) {
                    self.archive_current_chat();
                    if self.chat.conv.running {
                        self.chat.backend.stop();
                    }
                    self.chat.conv.reset();
                    self.chat.active_history_id = None;
                    self.chat.needs_recap = false;
                    self.chat.model = ModelChoice::AgentDefault;
                    self.chat.backend.new_conversation(&a.id);
                }
                self.chat.agent = Some(a.id.clone());
                self.chat.picked = true;
                self.chat.setup = false;
                self.chat.focus = true;
            }
            _ => {}
        }
    }

    /// Instructions...: the user's own chat instructions file, made from the
    /// starter when missing (never overwritten), opened in the default
    /// editor. It is read at the start of each conversation.
    pub(crate) fn open_chat_instructions(&mut self) {
        let path = self.chat.instructions.clone();
        match openxer_automation::chat_prompt::create_user_instructions(&path) {
            Ok(path) => {
                let shown = path.display().to_string();
                self.chat_open_url(&shown);
                self.status =
                    format!("Your chat instructions: {shown}. They apply from the next new chat.");
            }
            Err(e) => self.status = format!("Cannot make {}: {e}", path.display()),
        }
    }

    /// Open the user's custom playbooks folder (`chat\playbooks`), creating it
    /// with a starter README if missing.
    pub(crate) fn open_chat_playbooks_folder(&mut self) {
        let dir = self
            .chat
            .instructions
            .parent()
            .map(|p| p.join("playbooks"))
            .unwrap_or_else(|| std::path::PathBuf::from("playbooks"));
        match openxer_automation::playbooks::ensure_playbooks_dir(&dir) {
            Ok(path) => {
                let shown = path.display().to_string();
                self.chat_open_url(&shown);
                self.status = format!(
                    "Custom playbooks folder: {shown}. Drop .md files here to add /commands."
                );
            }
            Err(e) => self.status = format!("Cannot make {}: {e}", dir.display()),
        }
    }

    /// Check Setup: starts the checks and shows their rows.
    pub(crate) fn open_check_setup(&mut self) {
        self.ensure_chat_backend();
        if self.chat.backend.check_setup(true).is_some() {
            self.chat.check = Some(None);
        }
    }

    /// Once, after the first sign-in to the ChatGPT plan: OpenAI's welcome
    /// box, Got it (the default) or Manage usage.
    fn chat_welcome_box(&mut self, ctx: &egui::Context) {
        if !self.chat.open || self.chat.welcomed == Some(true) {
            return;
        }
        let signed_in = self
            .chat
            .backend
            .agents()
            .iter()
            .any(|a| a.plan && matches!(a.state, AgentState::SignedIn { .. }));
        if !signed_in {
            return;
        }
        if self.chat.welcomed.is_none() {
            let seen = self.store.preference(WELCOME_KEY).ok().flatten().is_some();
            self.chat.welcomed = Some(seen);
            if seen {
                return;
            }
        }
        use openxer_chat::ui;
        let mut rects = vec![];
        let answer = kit::message_box(
            ctx,
            "chat_welcome",
            ui::WELCOME_TITLE,
            crate::kit_window::MessageIcon::Info,
            ui::WELCOME_BODY,
            &[ui::WELCOME_BUTTON, ui::MANAGE_USAGE],
            crate::kit_window::Question::ChatPlanWelcome
                .default_in(&[ui::WELCOME_BUTTON, ui::MANAGE_USAGE]),
            &mut rects,
        );
        self.record_dialog_actions(rects);
        let escape = crate::kit_window::escape_goes_to(
            ctx,
            egui::Id::new(("kit_message", "chat_welcome")),
            egui::Order::Foreground,
        );
        if answer.is_some() || escape {
            self.chat.welcomed = Some(true);
            if let Err(e) = self.store.set_preference(WELCOME_KEY, "1") {
                self.status = e;
            }
        }
        if answer == Some(1) {
            self.chat_open_url(openxer_chat::USAGE_SETTINGS_URL);
        }
    }

    /// Check Setup: the rows as they come in on the kit's list, the
    /// selected row's fix, Check Again, Copy Results and Stop while the
    /// checks run.
    fn chat_check_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut pick) = self.chat.check else {
            return;
        };
        let Some(checks) = self.chat.backend.check_setup(false) else {
            self.chat.check = None;
            return;
        };
        checks.poll();
        let rows = checks.rows.clone();
        let running = checks.running();
        let cancelled = checks.cancelled;
        let total = checks.total;
        if running {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        const W: f32 = 760.;
        const H: f32 = 330.;
        let main = W - kit::STRIP_WIDTH;
        let fix = pick.and_then(|i| rows.get(i)).and_then(|r| r.fix.clone());
        let fix_label = fix.as_ref().map(|f| f.label()).unwrap_or("&Fix".into());
        let (clicked, closed, actions) =
            kit::modal(ctx, "chat_checks", "Check Setup", Vec2::new(W, H), |c| {
                c.fill(0., 0., main, H, kit::FACE);
                let passed = rows.iter().filter(|r| r.mark == checks::Mark::Pass).count();
                let failed = rows.iter().filter(|r| r.mark == checks::Mark::Fail).count();
                let line = if running {
                    format!("Checking... {} of about {total} done.", rows.len())
                } else if cancelled {
                    format!("Stopped: {passed} passed, {failed} failed so far.")
                } else {
                    format!("Done: {passed} passed, {failed} failed.")
                };
                kit::text(c, 10., 10., &line, true);
                let cols = [
                    kit::Col {
                        title: "Result",
                        width: 50.,
                    },
                    kit::Col {
                        title: "Area",
                        width: 90.,
                    },
                    kit::Col {
                        title: "Check",
                        width: main - 20. - 2. - 140. - kit::SCROLLBAR,
                    },
                ];
                let out = kit::list(
                    c,
                    "chat_checks_list",
                    (10., 30., main - 20., H - 40.),
                    None,
                    &cols,
                    rows.len(),
                    pick,
                    None,
                    &mut |c, r, col, rect, selected| {
                        let row = &rows[r];
                        let text = match col {
                            0 => row.mark.label().to_string(),
                            1 => row.area.clone(),
                            _ => row.text.clone(),
                        };
                        kit::cell_text(c, rect, &text, selected, 4.);
                    },
                );
                if let Some(r) = out.clicked {
                    pick = Some(r);
                }
                let column = kit::button_column(
                    c,
                    W,
                    &[
                        kit::Entry::close("chat_checks_close"),
                        kit::Entry::Button("chat_checks_fix", &fix_label, fix.is_some(), true),
                        kit::Entry::Button("chat_checks_again", "Check &Again", !running, true),
                        kit::Entry::Button("chat_checks_copy", "&Copy Results", true, false),
                        kit::Entry::Button("chat_checks_stop", "&Stop", running, false),
                    ],
                );
                column.clicked
            });
        self.record_dialog_actions(actions);
        let clicked = clicked.as_deref();
        if closed || clicked == Some("chat_checks_close") {
            if let Some(c) = self.chat.backend.check_setup(false) {
                c.cancel();
            }
            self.chat.check = None;
            return;
        }
        self.chat.check = Some(pick);
        match clicked {
            Some("chat_checks_stop") => {
                if let Some(c) = self.chat.backend.check_setup(false) {
                    c.cancel();
                }
            }
            Some("chat_checks_again") => {
                self.chat.backend.check_setup(true);
                self.chat.check = Some(None);
            }
            Some("chat_checks_copy") => {
                if let Some(c) = self.chat.backend.check_setup(false) {
                    let text = c.text();
                    ctx.copy_text(text);
                    self.status =
                        "Check Setup results copied (no accounts, tokens or profile paths).".into();
                }
            }
            Some("chat_checks_fix") => {
                if let Some(fix) = fix {
                    self.chat_fix(fix);
                }
            }
            _ => {}
        }
    }

    /// A Check Setup row's fix: the setup list's own actions.
    fn chat_fix(&mut self, fix: checks::Fix) {
        match fix {
            checks::Fix::SignIn(id) => {
                let agent = self.chat.backend.agents().into_iter().find(|a| a.id == id);
                if let Some(a) = agent {
                    self.chat.check = None;
                    self.chat_sign_in(&a);
                }
            }
            checks::Fix::Install { url, .. } | checks::Fix::Update(url) => self.chat_open_url(&url),
            checks::Fix::SetUp => {
                self.chat.check = None;
                self.open_endpoint_dialog();
            }
            checks::Fix::CheckAgain(id) => {
                self.chat.backend.check_again(&id);
                self.chat.backend.check_setup(true);
                self.chat.check = Some(None);
            }
        }
    }

    /// Selects the activity a reply links to and shows it in the table.
    pub(crate) fn chat_go_to(&mut self, code: &str) {
        let Some(i) = self
            .project
            .model
            .activities
            .iter()
            .position(|a| a.task_code == code)
        else {
            self.status = format!("{code} is not in this project.");
            return;
        };
        if !self.views.main.is_activities() {
            self.show_view(crate::views::MainView::Activities);
        }
        self.select_activities(&[i]);
        self.jump_to(i);
    }

    /// Selects every mentioned Activity ID in the table and jumps to the first.
    pub(crate) fn chat_select_many(&mut self, codes: &[String]) {
        let indices: Vec<usize> = codes
            .iter()
            .filter_map(|code| {
                self.project
                    .model
                    .activities
                    .iter()
                    .position(|a| a.task_code == *code)
            })
            .collect();
        if indices.is_empty() {
            return;
        }
        if !self.views.main.is_activities() {
            self.show_view(crate::views::MainView::Activities);
        }
        let first = indices[0];
        let count = indices.len();
        self.select_activities(&indices);
        self.jump_to(first);
        self.status = format!(
            "Selected {count} activit{} in the schedule.",
            if count == 1 { "y" } else { "ies" }
        );
    }

    /// Before the first sign-in to an agent that sends data off this PC:
    /// what that means, and Sign In or Cancel (Cancel is the default).
    fn chat_consent_box(&mut self, ctx: &egui::Context) {
        let Some(id) = self.chat.consent.clone() else {
            return;
        };
        let agents = self.chat.backend.agents();
        let Some(agent) = agents.iter().find(|a| a.id == id) else {
            self.chat.consent = None;
            return;
        };
        let text = agent.consent.clone().unwrap_or_default();
        let title = format!("Sign In to {}", agent.name);
        // OpenAI's wording for its sign-in button.
        let sign_in = if agent.plan {
            openxer_chat::ui::SIGN_IN_BUTTON
        } else {
            "Sign In"
        };
        let mut rects = vec![];
        let buttons = [sign_in, "Cancel"];
        let answer = kit::message_box(
            ctx,
            "chat_consent",
            &title,
            crate::kit_window::MessageIcon::Question,
            &text,
            &buttons,
            crate::kit_window::Question::ChatSignIn.default_in(&buttons),
            &mut rects,
        );
        self.record_dialog_actions(rects);
        let escape = crate::kit_window::escape_goes_to(
            ctx,
            egui::Id::new(("kit_message", "chat_consent")),
            egui::Order::Foreground,
        );
        match answer {
            Some(0) => {
                self.chat.consent = None;
                if let Err(e) = self.store.set_preference(&consent_key(&id), "1") {
                    self.status = e;
                }
                self.chat.backend.sign_in(&id);
            }
            Some(_) => self.chat.consent = None,
            None if escape => self.chat.consent = None,
            None => {}
        }
    }

    /// Before the first message to a service on another computer: what is
    /// sent where, Continue or Cancel (Cancel is the default). Remembered
    /// per service.
    fn chat_use_consent_box(&mut self, ctx: &egui::Context) {
        let Some((key, text, message)) = self.chat.use_consent.clone() else {
            return;
        };
        let mut rects = vec![];
        let answer = kit::message_box(
            ctx,
            "chat_use_consent",
            "Send to This Service?",
            crate::kit_window::MessageIcon::Question,
            &text,
            &["Continue", "Cancel"],
            crate::kit_window::Question::ChatSendToService.default_in(&["Continue", "Cancel"]),
            &mut rects,
        );
        self.record_dialog_actions(rects);
        let escape = crate::kit_window::escape_goes_to(
            ctx,
            egui::Id::new(("kit_message", "chat_use_consent")),
            egui::Order::Foreground,
        );
        match answer {
            Some(0) => {
                self.chat.use_consent = None;
                if let Err(e) = self.store.set_preference(&key, "1") {
                    self.status = e;
                }
                self.chat.pending_send = Some(message);
            }
            Some(_) => self.chat.use_consent = None,
            None if escape => self.chat.use_consent = None,
            None => {}
        }
    }

    /// Chat Options: model, active instructions/playbooks summary, per-project
    /// rules, custom playbooks folder, Check Setup and Sign Out.
    fn chat_options_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut pick) = self.chat.options else {
            return;
        };
        let agents = self.chat.backend.agents();
        let agent = self.chat_agent(&agents).map(|i| agents[i].clone());
        let choices = model_choices(agent.as_ref());
        let labels: Vec<String> = choices
            .iter()
            .map(|m| model_label(m, agent.as_ref()))
            .collect();
        let account = agent.as_ref().and_then(|a| match &a.state {
            AgentState::SignedIn { account } if a.can_sign_out => Some(account.clone()),
            _ => None,
        });
        let user_status = match openxer_automation::chat_prompt::read_user_instructions(
            &self.chat.instructions,
        ) {
            Some(u) => format!(
                "Global instructions: active ({} / {} chars)",
                u.text.chars().count(),
                openxer_automation::chat_prompt::USER_LIMIT
            ),
            None => "Global instructions: none (chat\\AGENTS.md)".into(),
        };
        let proj_len = self.chat.project_rules.trim().chars().count();
        let proj_status = if !self.chat_has_project() {
            "Project rules: no project open".into()
        } else if proj_len > 0 {
            format!(
                "Project rules: active ({proj_len} / {} chars)",
                openxer_automation::chat_prompt::PROJECT_LIMIT
            )
        } else {
            "Project rules: none".into()
        };
        let custom_count = prompts::available_playbooks(&self.chat.instructions)
            .iter()
            .filter(|p| p.custom)
            .count();
        let pb_status = format!(
            "Playbooks: {} built-in, {} custom",
            openxer_automation::playbooks::PLAYBOOKS.len(),
            custom_count
        );
        const W: f32 = 430.;
        const H: f32 = 224.;
        let mut check = false;
        let mut instructions = false;
        let mut project_rules = false;
        let mut playbooks_folder = false;
        let has_project = self.chat_has_project();
        let main = W - kit::STRIP_WIDTH;
        let ((ok, cancel, sign_out), closed, actions) =
            kit::modal(ctx, "chat_options", "Chat Options", Vec2::new(W, H), |c| {
                c.fill(0., 0., main, H, kit::FACE);
                let items: Vec<(&str, bool)> = labels.iter().map(|l| (l.as_str(), true)).collect();
                c.label(10., 12., 21., "Model", true);
                c.combo(
                    "chat_model",
                    60.,
                    12.,
                    main - 72.,
                    21.,
                    &items,
                    &mut pick,
                    true,
                );
                kit::secondary_text(c, 10., 46., &user_status);
                kit::secondary_text(c, 10., 66., &proj_status);
                kit::secondary_text(c, 10., 86., &pb_status);
                let mut sign_out = false;
                if let Some(account) = &account {
                    kit::text(c, 10., 116., &format!("Signed in as {account}"), true);
                    sign_out = kit::button(
                        c,
                        "chat_sign_out",
                        10.,
                        136.,
                        kit::BUTTON.x,
                        kit::BUTTON.y,
                        "Sign &Out",
                        true,
                    );
                }
                let column = kit::button_column(
                    c,
                    W,
                    &[
                        kit::Entry::ok("chat_options_ok"),
                        kit::Entry::cancel("chat_options_cancel"),
                        kit::Entry::Button("chat_options_check", "Check Set&up...", true, true),
                        kit::Entry::Button(
                            "chat_options_instructions",
                            "&Instructions...",
                            true,
                            false,
                        ),
                        kit::Entry::Button(
                            "chat_options_project_rules",
                            "&Project Rules...",
                            has_project,
                            false,
                        ),
                        kit::Entry::Button("chat_options_playbooks", "Play&books...", true, false),
                    ],
                );
                let clicked = column.clicked.as_deref();
                if clicked == Some("chat_options_check") {
                    check = true;
                }
                if clicked == Some("chat_options_instructions") {
                    instructions = true;
                }
                if clicked == Some("chat_options_project_rules") {
                    project_rules = true;
                }
                if clicked == Some("chat_options_playbooks") {
                    playbooks_folder = true;
                }
                (
                    clicked == Some("chat_options_ok"),
                    clicked == Some("chat_options_cancel"),
                    sign_out,
                )
            });
        self.record_dialog_actions(actions);
        if check {
            self.chat.options = None;
            self.open_check_setup();
            return;
        }
        if instructions {
            self.chat.options = None;
            self.open_chat_instructions();
            return;
        }
        if project_rules {
            self.chat.options = None;
            self.chat.project_rules_dialog = Some(ProjectRulesDialog {
                text: self.chat.project_rules.clone(),
            });
            return;
        }
        if playbooks_folder {
            self.chat.options = None;
            self.open_chat_playbooks_folder();
            return;
        }
        if sign_out {
            if let Some(a) = &agent {
                self.chat.backend.sign_out(&a.id);
            }
            self.chat.options = None;
        } else if ok {
            self.chat.model = choices.get(pick).cloned().unwrap_or_default();
            self.chat.options = None;
        } else if cancel || closed {
            self.chat.options = None;
        } else {
            self.chat.options = Some(pick);
        }
    }

    /// In-app modal to edit per-project standing rules (contract milestones,
    /// owner DCMA thresholds, crew limits) sent alongside global AGENTS.md.
    fn chat_project_rules_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.chat.project_rules_dialog.take() else {
            return;
        };
        const W: f32 = 460.;
        const H: f32 = 240.;
        let main = W - kit::STRIP_WIDTH;
        let limit = openxer_automation::chat_prompt::PROJECT_LIMIT;
        let count = dialog.text.trim().chars().count();
        let (clicked, closed, actions) = kit::modal(
            ctx,
            "chat_project_rules",
            "Project Chat Rules",
            Vec2::new(W, H),
            |c| {
                c.fill(0., 0., main, H, kit::FACE);
                kit::text(
                    c,
                    10.,
                    10.,
                    "Standing rules for this project only (applied on every turn):",
                    true,
                );
                kit::text_area_with(
                    c,
                    "chat_project_rules_text",
                    (10., 30., main - 20., H - 62.),
                    &mut dialog.text,
                    kit::body_font(),
                    INK,
                    kit::TextArea {
                        hint: "Example: Milestone MS-900 has $25k/day LDs. Keep lags <= 5d.",
                        enter_is_callers: false,
                        enabled: true,
                        scroll: true,
                    },
                );
                kit::secondary_text(c, 10., H - 24., &format!("{count} / {limit} characters"));
                let col = kit::button_column(
                    c,
                    W,
                    &[
                        kit::Entry::ok("chat_project_rules_ok"),
                        kit::Entry::cancel("chat_project_rules_cancel"),
                    ],
                );
                col.clicked
            },
        );
        self.record_dialog_actions(actions);
        match clicked.as_deref() {
            Some("chat_project_rules_ok") => {
                self.chat.project_rules = dialog.text.trim().chars().take(limit).collect();
                self.save_project_rules();
                self.status = "Project chat rules saved.".into();
            }
            Some("chat_project_rules_cancel") => {}
            _ if closed => {}
            _ => self.chat.project_rules_dialog = Some(dialog),
        }
    }

    /// In-app modal to save a reusable prompt snippet (with optional
    /// `{selection}`, `{project}`, `{data_date}` placeholders).
    fn chat_save_prompt_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.chat.save_prompt_dialog.take() else {
            return;
        };
        const W: f32 = 440.;
        const H: f32 = 220.;
        let main = W - kit::STRIP_WIDTH;
        let can_save = !dialog.name.trim().is_empty() && !dialog.text.trim().is_empty();
        let (clicked, closed, actions) = kit::modal(
            ctx,
            "chat_save_prompt",
            "Save Prompt",
            Vec2::new(W, H),
            |c| {
                c.fill(0., 0., main, H, kit::FACE);
                c.label(10., 12., 21., "Name", true);
                c.edit(
                    "chat_save_prompt_name",
                    58.,
                    12.,
                    main - 68.,
                    21.,
                    &mut dialog.name,
                    true,
                    egui::Align::LEFT,
                );
                kit::secondary_text(
                    c,
                    10.,
                    40.,
                    "Prompt text (optional tokens: {selection}, {project}, {data_date}):",
                );
                kit::text_area_with(
                    c,
                    "chat_save_prompt_text",
                    (10., 58., main - 20., H - 70.),
                    &mut dialog.text,
                    kit::body_font(),
                    INK,
                    kit::TextArea {
                        hint: "Write your reusable scheduling prompt...",
                        enter_is_callers: false,
                        enabled: true,
                        scroll: true,
                    },
                );
                let col = kit::button_column(
                    c,
                    W,
                    &[
                        kit::Entry::Button("chat_save_prompt_ok", "OK", can_save, false),
                        kit::Entry::cancel("chat_save_prompt_cancel"),
                    ],
                );
                col.clicked
            },
        );
        self.record_dialog_actions(actions);
        match clicked.as_deref() {
            Some("chat_save_prompt_ok") if can_save => {
                let name = dialog.name.trim().to_string();
                let text = dialog.text.trim().to_string();
                if let Some(existing) = self
                    .chat
                    .saved_prompts
                    .iter_mut()
                    .find(|p| p.name.eq_ignore_ascii_case(&name))
                {
                    existing.text = text;
                } else {
                    self.chat.saved_prompts.push(history::SavedPrompt {
                        name: name.clone(),
                        text,
                    });
                }
                self.save_saved_prompts();
                self.status = format!("Saved prompt \"{name}\".");
            }
            Some("chat_save_prompt_cancel") => {}
            _ if closed => {}
            _ => self.chat.save_prompt_dialog = Some(dialog),
        }
    }
}

/// The models an agent offers: its default, a lower cost one, and the
/// names it lists.
fn model_choices(agent: Option<&AgentInfo>) -> Vec<ModelChoice> {
    let mut out = vec![ModelChoice::AgentDefault, ModelChoice::LowerCost];
    if let Some(a) = agent {
        out.extend(a.models.iter().cloned().map(ModelChoice::Named));
    }
    out
}

fn model_label(m: &ModelChoice, agent: Option<&AgentInfo>) -> String {
    match m {
        ModelChoice::AgentDefault => "Agent default".into(),
        ModelChoice::LowerCost => agent
            .and_then(|a| a.lower_cost.clone())
            .unwrap_or_else(|| "Lower cost".into()),
        ModelChoice::Named(n) => n.clone(),
    }
}

/// An agent as the setup list shows it: its state, what it needs, and its
/// one action (with Check Again beneath Get for a program not installed).
fn setup_row(a: &AgentInfo) -> list::SetupRow {
    use crate::ai_agents::row::Mark;
    let (mark, state, dim, primary, secondary) = match &a.state {
        AgentState::Ready => (
            Mark::Ready,
            "Ready".to_string(),
            false,
            ("&Use".to_string(), true),
            None,
        ),
        AgentState::SignedIn { account } => (
            Mark::Ready,
            format!("Signed in as {account}"),
            false,
            ("&Use".to_string(), true),
            None,
        ),
        AgentState::NotInstalled => (
            Mark::Off,
            "Not installed".to_string(),
            true,
            (format!("Get {}...", a.name), a.install_url.is_some()),
            Some(("Check &Again".to_string(), true)),
        ),
        AgentState::SignInNeeded => (
            Mark::Attention,
            "Sign-in needed".to_string(),
            false,
            ("Sign &In...".to_string(), true),
            None,
        ),
        AgentState::SigningIn => (
            Mark::Attention,
            "Signing in in your browser...".to_string(),
            false,
            ("Cancel".to_string(), true),
            None,
        ),
        AgentState::Checking => (
            Mark::Off,
            "Looking for it on this PC...".to_string(),
            true,
            ("&Use".to_string(), false),
            None,
        ),
        AgentState::NotSetUp => (
            Mark::Off,
            "Not set up".to_string(),
            false,
            ("Set &Up...".to_string(), true),
            None,
        ),
    };
    // A route set up in OpenXER can be set up again beneath Use.
    let secondary = if a.configurable && a.state.usable() {
        Some(("Set &Up...".to_string(), true))
    } else {
        secondary
    };
    let detail = match &a.notice {
        Some(n) => format!("{} {n}", a.setup),
        None => a.setup.clone(),
    };
    list::SetupRow {
        name: a.name.clone(),
        mark,
        state,
        dim,
        detail,
        primary: Some(primary),
        secondary,
    }
}

/// Takes a specific unmodified key press from this frame's input events.
fn take_plain_key(ctx: &egui::Context, target: Key) -> bool {
    ctx.input_mut(|i| {
        let at = i.events.iter().position(|e| {
            matches!(
                e,
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } if *key == target && modifiers.is_none()
            )
        });
        at.map(|at| i.events.remove(at)).is_some()
    })
}

/// Takes an Enter pressed without Shift, Ctrl or Alt from this frame's
/// input (Shift+Enter is left to the composer's new line).
fn take_plain_enter(ctx: &egui::Context) -> bool {
    take_plain_key(ctx, Key::Enter)
}

/// Takes Ctrl+Enter / Cmd+Enter from this frame's input (used to interrupt
/// and send immediately while a reply is still running).
fn take_cmd_enter(ctx: &egui::Context) -> bool {
    let mut took = false;
    ctx.input_mut(|i| {
        i.events.retain(|e| {
            let hit = matches!(
                e,
                egui::Event::Key {
                    key: Key::Enter,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.command && !modifiers.shift && !modifiers.alt
            );
            took |= hit;
            !hit
        });
    });
    took
}

/// Parse an `openxer_ask_user` JSON payload into `(title, prompts)`.
fn parse_ask_user_payload(payload: &serde_json::Value) -> Option<(String, Vec<model::AskPrompt>)> {
    let title = payload["title"].as_str().unwrap_or("").trim().to_string();
    let arr = payload["questions"].as_array()?;
    let mut prompts = Vec::new();
    for q in arr {
        let question = q["question"].as_str().unwrap_or("").trim().to_string();
        if question.is_empty() {
            continue;
        }
        let multi_select = q["multi_select"].as_bool().unwrap_or(false);
        let mut options = Vec::new();
        if let Some(opts) = q["options"].as_array() {
            for o in opts {
                if let Some(s) = o.as_str() {
                    let label = s.trim().to_string();
                    if !label.is_empty() {
                        options.push(model::AskOption {
                            label,
                            description: String::new(),
                        });
                    }
                } else if let Some(label) = o["label"].as_str() {
                    let label = label.trim().to_string();
                    if !label.is_empty() {
                        options.push(model::AskOption {
                            label,
                            description: o["description"].as_str().unwrap_or("").trim().to_string(),
                        });
                    }
                }
            }
        }
        if !options.is_empty() {
            prompts.push(model::AskPrompt {
                question,
                multi_select,
                options,
            });
        }
    }
    (!prompts.is_empty()).then_some((title, prompts))
}

/// What the body is drawn from.
struct View<'a> {
    agents: &'a [AgentInfo],
    agent: Option<usize>,
    context: Option<&'a ChatContext>,
    starters: &'a [String],
    focus: bool,
    /// The setup list, when it shows.
    setup: Option<&'a [list::SetupRow]>,
    playbooks: &'a [openxer_automation::playbooks::PlaybookEntry],
    undoable_proposal: Option<u64>,
    elapsed_secs: Option<u64>,
}

fn text_width(c: &Canvas, s: &str) -> f32 {
    c.ui.painter()
        .layout_no_wrap(s.to_owned(), kit::body_font(), INK)
        .size()
        .x
        .ceil()
}

/// An agent as the Agent drop-down lists it: its name, and its state when
/// it cannot be used yet.
fn agent_label(a: &AgentInfo) -> String {
    match &a.state {
        AgentState::Ready | AgentState::SignedIn { .. } => a.name.clone(),
        AgentState::NotInstalled => format!("{} (not installed)", a.name),
        AgentState::SignInNeeded => format!("{} (sign-in needed)", a.name),
        AgentState::SigningIn => format!("{} (signing in)", a.name),
        AgentState::Checking => format!("{} (checking)", a.name),
        AgentState::NotSetUp => format!("{} (not set up)", a.name),
    }
}

/// Paint a standard kit button and return its `egui::Response` so a native
/// popup menu (`egui::Popup::menu`) can attach directly to it.
#[allow(clippy::too_many_arguments)]
fn menu_button(
    c: &mut Canvas,
    key: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    caption: &str,
    enabled: bool,
) -> egui::Response {
    let rect = c.r(x, y, w, h);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let resp = c.ui.interact(rect, c.id(key), sense);
    let p = c.ui.painter();
    let hot = enabled && resp.hovered();
    let down = enabled && resp.is_pointer_button_down_on();
    let fill = if down {
        chrome::pal::PRESSED_FILL
    } else if hot {
        chrome::pal::HOT_FILL
    } else {
        kit::FACE
    };
    let border = if hot || down {
        chrome::pal::HOT_BORDER
    } else {
        kit::SHADOW
    };
    p.rect_filled(rect, 0., fill);
    p.rect_stroke(
        rect.shrink(0.5),
        0.,
        Stroke::new(1_f32, border),
        egui::StrokeKind::Middle,
    );
    let color = if enabled {
        INK
    } else {
        crate::theme::color(crate::theme::SECONDARY_TEXT)
    };
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        caption,
        kit::body_font(),
        color,
    );
    c.act(key, rect);
    resp
}

/// The client area: Agent row, Model/History/Dock row, message list,
/// context + Prompts row, composer (with `/` & `@` popups) and Mode/Send row.
fn body(c: &mut Canvas, size: Vec2, v: &View, chat: &mut ChatState, k: &mut Clicks) {
    const PAD: f32 = 7.;
    const GAP: f32 = 6.;
    const FIELD: f32 = 21.;
    let (w, h) = (size.x, size.y);
    c.fill(0., 0., w, h, kit::FACE);
    let agent = v.agent.map(|i| &v.agents[i]);
    let project = v.context.is_some();
    let usable = project && agent.is_some_and(|a| a.state.usable());

    // Row 1: Agent, Options..., New Chat.
    let y = GAP;
    let label_w = text_width(c, "Agent");
    c.label(PAD, y, FIELD, "Agent", !v.agents.is_empty());
    let new_w = 72.;
    let new_x = w - PAD - new_w;
    let options = agent.is_some();
    let options_w = 72.;
    let combo_x = PAD + label_w + GAP;
    let combo_right = if options {
        new_x - GAP - options_w - GAP
    } else {
        new_x - GAP
    };
    let mut labels: Vec<String> = v.agents.iter().map(agent_label).collect();
    if !labels.is_empty() {
        labels.push(SETUP_ITEM.into());
    }
    let items: Vec<(&str, bool)> = labels.iter().map(|l| (l.as_str(), true)).collect();
    let mut pick = if v.setup.is_some() && chat.setup {
        v.agents.len()
    } else {
        v.agent.unwrap_or(0)
    };
    if c.combo(
        "chat_agent",
        combo_x,
        y,
        (combo_right - combo_x).max(60.),
        FIELD,
        &items,
        &mut pick,
        !v.agents.is_empty(),
    ) {
        k.agent = Some(pick);
    }
    if options {
        k.options = kit::button(
            c,
            "chat_options",
            new_x - GAP - options_w,
            y,
            options_w,
            FIELD,
            "&Options...",
            true,
        );
    }
    k.new_chat = kit::button(
        c,
        "chat_new",
        new_x,
        y,
        new_w,
        FIELD,
        "&New Chat",
        usable || !chat.conv.is_empty(),
    );

    // Row 2: Inline Model selector, History menu, Dock/Float toggle.
    // Hidden while the Choose-how-to-connect setup list is shown so the setup
    // rows keep full vertical room.
    let list_y = if v.setup.is_some() {
        y + FIELD + GAP
    } else {
        let row2_y = y + FIELD + 4.;
        let model_w = text_width(c, "Model");
        c.label(PAD, row2_y, FIELD, "Model", agent.is_some());
        let dock_w = 46.;
        let dock_x = w - PAD - dock_w;
        let hist_w = 58.;
        let hist_x = dock_x - GAP - hist_w;
        let mcombo_x = PAD + model_w + GAP;
        let mcombo_w = (hist_x - GAP - mcombo_x).max(60.);
        let choices = model_choices(agent);
        let mlabels: Vec<String> = choices.iter().map(|m| model_label(m, agent)).collect();
        let mitems: Vec<(&str, bool)> = mlabels.iter().map(|l| (l.as_str(), true)).collect();
        let mut mpick = choices.iter().position(|m| *m == chat.model).unwrap_or(0);
        if c.combo(
            "chat_inline_model",
            mcombo_x,
            row2_y,
            mcombo_w,
            FIELD,
            &mitems,
            &mut mpick,
            agent.is_some(),
        ) {
            if let Some(chosen) = choices.get(mpick) {
                k.model = Some(chosen.clone());
            }
        }
        let has_history = !chat.history.is_empty();
        let hist_resp = menu_button(
            c,
            "chat_history",
            hist_x,
            row2_y,
            hist_w,
            FIELD,
            "History",
            has_history,
        );
        if has_history {
            egui::Popup::menu(&hist_resp)
                .style(chrome::popup_menu_style)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| {
                    ui.set_min_width(210.);
                    let mut m = crate::context_menu::Menu::on(ui, "chat_history");
                    for (idx, saved) in chat.history.iter().enumerate() {
                        let active = chat.active_history_id == Some(saved.id);
                        if m.item(
                            &format!("item_{idx}"),
                            Item::new(&saved.title).checked(active),
                        ) {
                            k.load_history = Some(saved.id);
                        }
                    }
                    m.separator();
                    if m.item("clear", Item::new("Clear History for This Project")) {
                        k.clear_history = true;
                    }
                });
        }
        let dock_label = if chat.docked { "Float" } else { "Dock" };
        if kit::button(
            c,
            "chat_dock_toggle",
            dock_x,
            row2_y,
            dock_w,
            FIELD,
            dock_label,
            true,
        ) {
            k.toggle_dock = true;
        }
        row2_y + FIELD + GAP
    };

    // Composer height auto-grows cleanly from 50 px up to 96 px for multi-line drafts.
    let line_count = chat.draft.lines().count().max(1);
    let composer_h = (50. + (line_count.saturating_sub(3) as f32) * 15.).min(96.);

    // The bottom rows, from the bottom up.
    let send_y = h - PAD - kit::BUTTON.y;
    let composer_y = send_y - GAP - composer_h;
    let context_y = composer_y - GAP - FIELD;
    // OpenAI's wording while the ChatGPT plan answers: above the context.
    let plan = agent.is_some_and(|a| a.plan && a.state.usable());
    let plan_y = context_y - GAP - 15.;
    let mut list_bottom = if plan { plan_y } else { context_y };
    // Compact strip of queued messages (waiting to send after the current turn,
    // or click "Send now" to interrupt and send immediately).
    let queued_shown = chat.queued_messages.len().min(3);
    let queue_h = if queued_shown > 0 {
        queued_shown as f32 * 20. + 4.
    } else {
        0.
    };
    let queue_y = if queued_shown > 0 {
        let y = list_bottom - GAP - queue_h;
        list_bottom = y;
        Some(y)
    } else {
        None
    };
    let list_h = (list_bottom - GAP - list_y).max(40.);

    // The message list.
    let empty = match (v.setup, v.context, agent) {
        (Some(rows), context, _) => list::Empty::Setup {
            rows,
            note: context
                .is_none()
                .then_some("No project is open. Open a project to ask about its schedule."),
        },
        (None, context, agent) => empty_of(context, agent, v.starters),
    };
    let composer_id = c.id("chat_message");
    let composer_focused = c.ui.ctx().memory(|m| m.has_focus(composer_id));
    let list_view = list::ListView {
        codes: &chat.codes,
        tips: &chat.tips,
        plan: agent.is_some_and(|a| a.plan),
        mode: chat.mode,
        expanded_tools: &chat.expanded_tools,
        expanded_proposals: &chat.expanded_proposals,
        undoable_proposal: v.undoable_proposal,
        elapsed_secs: v.elapsed_secs,
        composer_focused,
    };
    k.list = list::show(
        c,
        (PAD, list_y, w - 2. * PAD, list_h),
        &chat.conv,
        empty,
        &list_view,
    );
    if k.list.text_interacted {
        c.ui.ctx().memory_mut(|m| m.surrender_focus(composer_id));
    }
    if let Some(qy) = queue_y {
        let qw = w - 2. * PAD;
        let qrect = c.r(PAD, qy, qw, queue_h);
        c.fill(PAD, qy, qw, queue_h, chrome::pal::READONLY);
        c.ui.painter().rect_stroke(
            qrect.shrink(0.5),
            0.,
            Stroke::new(1_f32, chrome::pal::CAPTION_LINE),
            egui::StrokeKind::Middle,
        );
        for (i, msg) in chat.queued_messages.iter().take(queued_shown).enumerate() {
            let ry = qy + 2. + i as f32 * 20.;
            let drop_w = 14.;
            let now_w = text_width(c, "Send now").ceil() + 2.;
            let drop_x = PAD + qw - 6. - drop_w;
            let now_x = drop_x - 6. - now_w;
            let text_w = (now_x - (PAD + 6.) - 6.).max(40.);
            let single = msg.split_whitespace().collect::<Vec<_>>().join(" ");
            let label = format!("Queued: {single}");
            let cell = c.r(PAD + 6., ry, text_w, 18.);
            kit::cell_text(c, cell, &label, false, 0.);
            if kit::link(
                c,
                &format!("chat_queue_now_{i}"),
                now_x,
                ry + 2.,
                "Send now",
            ) {
                k.send_now_queued = Some(i);
            }
            if kit::link(
                c,
                &format!("chat_queue_drop_{i}"),
                drop_x + 2.,
                ry + 2.,
                "x",
            ) {
                k.drop_queued = Some(i);
            }
        }
    }
    if plan {
        let label = openxer_chat::ui::PLAN_INDICATOR;
        kit::secondary_text(c, PAD, plan_y, label);
        let lx = PAD + text_width(c, label) + 10.;
        k.manage_usage = kit::link(
            c,
            "chat_plan_manage_usage",
            lx,
            plan_y,
            openxer_chat::ui::MANAGE_USAGE,
        );
    }
    body_rest(
        c, size, v, chat, k, list_y, context_y, composer_y, composer_h, send_y, usable,
    );
}

/// The empty state of the agent in use.
fn empty_of<'a>(
    context: Option<&ChatContext>,
    agent: Option<&'a AgentInfo>,
    starters: &'a [String],
) -> list::Empty<'a> {
    match (context, agent) {
        (None, _) => list::Empty::NoProject,
        (_, None) => list::Empty::NoAgent,
        (_, Some(a)) => match &a.state {
            AgentState::NotInstalled => list::Empty::NotInstalled { name: &a.name },
            AgentState::SignInNeeded => list::Empty::SignIn {
                name: &a.name,
                privacy: &a.privacy,
            },
            AgentState::SigningIn => list::Empty::SigningIn { name: &a.name },
            AgentState::Ready => list::Empty::Ready {
                line: format!("{} is ready. {}", a.name, a.privacy),
                starters,
            },
            AgentState::SignedIn { account } => list::Empty::Ready {
                line: format!("Signed in to {} as {account}. {}", a.name, a.privacy),
                starters,
            },
            AgentState::Checking => list::Empty::Checking { name: &a.name },
            AgentState::NotSetUp => list::Empty::SetUp {
                name: &a.name,
                privacy: &a.privacy,
            },
        },
    }
}

/// The context + Prompts row, composer (with `/` & `@` autocomplete popups and
/// Up/Down history) and Mode/Usage/Send row.
#[allow(clippy::too_many_arguments)]
fn body_rest(
    c: &mut Canvas,
    size: Vec2,
    v: &View,
    chat: &mut ChatState,
    k: &mut Clicks,
    list_y: f32,
    context_y: f32,
    composer_y: f32,
    composer_h: f32,
    send_y: f32,
    usable: bool,
) {
    const PAD: f32 = 7.;
    const GAP: f32 = 6.;
    const FIELD: f32 = 21.;
    let w = size.x;

    // Context line + Prompts menu button.
    let ctx_label = "Context";
    let cw = text_width(c, ctx_label) + 20.;
    c.checkbox(
        "chat_include_context",
        PAD,
        context_y,
        cw,
        FIELD,
        ctx_label,
        &mut chat.include_context,
        v.context.is_some(),
    );
    let prompts_w = 64.;
    let prompts_x = w - PAD - prompts_w;
    let line = match v.context {
        Some(ctx) if chat.include_context => model::context_line(ctx),
        Some(_) => "Not included".into(),
        None => "No project open".into(),
    };
    let fx = PAD + cw + GAP;
    let fw = (prompts_x - GAP - fx).max(40.);
    kit::field(c, fx, context_y, fw, FIELD, &line, false);
    if let Some(ctx) = v.context.filter(|_| chat.include_context) {
        let mut tip_lines = vec![line.clone()];
        if let Some(b) = &ctx.baseline {
            tip_lines.push(format!("Baseline: {b}"));
        }
        if let Some(f) = &ctx.filter {
            tip_lines.push(format!("Filter: {f}"));
        }
        if let Some(t) = &ctx.detail_tab {
            tip_lines.push(format!("Details tab: {t}"));
        }
        if !chat.project_rules.trim().is_empty() {
            tip_lines.push("Project chat rules: active".into());
        }
        kit::tooltip(
            c,
            "chat_context_tip",
            fx,
            context_y,
            fw,
            FIELD,
            &tip_lines.join("\n"),
        );
    }
    let prompts_resp = menu_button(
        c,
        "chat_prompts",
        prompts_x,
        context_y,
        prompts_w,
        FIELD,
        "Prompts",
        v.context.is_some(),
    );
    if v.context.is_some() {
        egui::Popup::menu(&prompts_resp)
            .style(chrome::popup_menu_style)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_min_width(220.);
                let mut m = crate::context_menu::Menu::on(ui, "chat_prompts");
                for p in v.playbooks {
                    let label = if p.custom {
                        format!("/{} (custom)", p.name)
                    } else {
                        format!("/{}", p.name)
                    };
                    if m.item(
                        &format!("pb_{}", p.name),
                        Item::new(&label).enabled(usable && !chat.conv.running),
                    ) {
                        k.run_playbook = Some(p.name.clone());
                    }
                }
                m.separator();
                if chat.saved_prompts.is_empty() {
                    m.item("no_saved", Item::new("No saved prompts yet").enabled(false));
                } else {
                    for (idx, s) in chat.saved_prompts.iter().enumerate() {
                        if m.item(
                            &format!("saved_{idx}"),
                            Item::new(&s.name).enabled(usable && !chat.conv.running),
                        ) {
                            k.run_saved_prompt = Some(s.text.clone());
                        }
                    }
                }
                if m.item(
                    "save_draft",
                    Item::new("Save Draft as Prompt...").enabled(!chat.draft.trim().is_empty()),
                ) {
                    k.open_save_prompt = true;
                }
                if !chat.saved_prompts.is_empty() {
                    m.submenu("remove_saved", Item::new("Remove Saved Prompt"), |sub| {
                        for (idx, s) in chat.saved_prompts.iter().enumerate() {
                            if sub.item(&format!("del_{idx}"), Item::new(&s.name)) {
                                k.delete_saved_prompt = Some(idx);
                            }
                        }
                    });
                }
                m.separator();
                if m.item("project_rules", Item::new("Project Chat Rules...")) {
                    k.open_project_rules = true;
                }
                if m.item(
                    "playbooks_dir",
                    Item::new("Open Custom Playbooks Folder..."),
                ) {
                    k.open_playbooks_folder = true;
                }
            });
    }

    // Check active `/` slash or `@` mention popup state before handling keys.
    let dismissed = chat.popup_dismissed.as_deref() == Some(chat.draft.as_str());
    let slash_items = if usable && !dismissed {
        prompts::slash_query(&chat.draft)
            .map(|q| prompts::slash_matches(q, v.playbooks, &chat.saved_prompts))
            .filter(|m| !m.is_empty())
    } else {
        None
    };
    let mention_items = if usable && !dismissed && slash_items.is_none() {
        prompts::at_mention_query(&chat.draft).and_then(|(at, q)| {
            let ql = q.to_ascii_lowercase();
            let matches: Vec<(String, String)> = chat
                .activities_index
                .iter()
                .filter(|(code, name)| {
                    ql.is_empty()
                        || code.to_ascii_lowercase().contains(&ql)
                        || name.to_ascii_lowercase().contains(&ql)
                })
                .take(8)
                .cloned()
                .collect();
            (!matches.is_empty()).then_some((at, matches))
        })
    } else {
        None
    };

    // Composer: Enter sends (or completes popup), Shift+Enter starts a new line,
    // Up/Down recalls sent prompt history when empty or navigates popups.
    let composer_id = c.id("chat_message");
    if chat.use_consent.is_some() {
        c.ui.ctx().memory_mut(|m| m.surrender_focus(composer_id));
    }
    let focused = c.ui.ctx().memory(|m| m.has_focus(composer_id));
    let popup_len = slash_items
        .as_ref()
        .map(Vec::len)
        .or_else(|| mention_items.as_ref().map(|(_, m)| m.len()))
        .unwrap_or(0);
    if popup_len > 0 {
        chat.popup_pick = chat.popup_pick.min(popup_len - 1);
    } else {
        chat.popup_pick = 0;
    }

    if focused && usable {
        if popup_len > 0 {
            if take_plain_key(c.ui.ctx(), Key::ArrowDown) {
                chat.popup_pick = (chat.popup_pick + 1) % popup_len;
            } else if take_plain_key(c.ui.ctx(), Key::ArrowUp) {
                chat.popup_pick = (chat.popup_pick + popup_len - 1) % popup_len;
            } else if take_plain_key(c.ui.ctx(), Key::Escape) {
                chat.popup_dismissed = Some(chat.draft.clone());
            } else if take_plain_key(c.ui.ctx(), Key::Tab) {
                if let Some(items) = &slash_items {
                    chat.draft = format!("/{} ", items[chat.popup_pick].command);
                } else if let Some((at, items)) = &mention_items {
                    prompts::insert_at_mention(&mut chat.draft, *at, &items[chat.popup_pick].0);
                }
            } else if take_plain_enter(c.ui.ctx()) && !chat.conv.running {
                if let Some(items) = &slash_items {
                    match &items[chat.popup_pick].kind {
                        prompts::SlashKind::Playbook { name, .. } => {
                            k.run_playbook = Some(name.clone());
                            chat.draft.clear();
                        }
                        prompts::SlashKind::SavedPrompt { text } => {
                            k.run_saved_prompt = Some(text.clone());
                            chat.draft.clear();
                        }
                    }
                } else if let Some((at, items)) = &mention_items {
                    prompts::insert_at_mention(&mut chat.draft, *at, &items[chat.popup_pick].0);
                }
            }
        } else {
            // Prompt history recall with Up/Down when draft is empty or already browsing.
            let can_recall = !chat.prompt_history.is_empty()
                && (chat.draft.is_empty() || chat.prompt_cursor.is_some());
            if can_recall && !chat.draft.contains('\n') {
                if take_plain_key(c.ui.ctx(), Key::ArrowUp) {
                    let next = match chat.prompt_cursor {
                        None => {
                            chat.prompt_stash = chat.draft.clone();
                            chat.prompt_history.len() - 1
                        }
                        Some(0) => 0,
                        Some(i) => i - 1,
                    };
                    chat.prompt_cursor = Some(next);
                    chat.draft = chat.prompt_history[next].clone();
                } else if chat.prompt_cursor.is_some() && take_plain_key(c.ui.ctx(), Key::ArrowDown)
                {
                    let next = chat.prompt_cursor.unwrap() + 1;
                    if next >= chat.prompt_history.len() {
                        chat.prompt_cursor = None;
                        chat.draft = std::mem::take(&mut chat.prompt_stash);
                    } else {
                        chat.prompt_cursor = Some(next);
                        chat.draft = chat.prompt_history[next].clone();
                    }
                }
            }
            if chat.conv.running {
                if take_cmd_enter(c.ui.ctx()) && !chat.draft.trim().is_empty() {
                    k.send_now = Some(chat.draft.clone());
                } else if take_plain_enter(c.ui.ctx()) && !chat.draft.trim().is_empty() {
                    k.queue = Some(chat.draft.clone());
                }
            } else if take_plain_enter(c.ui.ctx()) {
                k.send = Some(chat.draft.clone());
            }
        }
    }

    let prev_draft = chat.draft.clone();
    let response = kit::text_area_with(
        c,
        "chat_message",
        (PAD, composer_y, w - 2. * PAD, composer_h),
        &mut chat.draft,
        kit::body_font(),
        INK,
        kit::TextArea {
            hint: HINT,
            enter_is_callers: true,
            enabled: usable,
            scroll: true,
        },
    );
    if chat.draft != prev_draft {
        chat.prompt_cursor = None;
        if chat.popup_dismissed.as_deref() != Some(chat.draft.as_str()) {
            chat.popup_dismissed = None;
        }
    }
    if v.focus && usable {
        response.request_focus();
    }

    // Inline `/` slash popup or `@` Activity ID mention popup painted over the
    // lower edge of the message list, right above the Context/Composer area.
    if let Some(items) = &slash_items {
        let shown = items.len().min(7);
        let pop_h = (shown as f32) * kit::ROW + 2.;
        let pop_y = (context_y - GAP - pop_h).max(list_y);
        let pop_w = w - 2. * PAD;
        c.fill(PAD, pop_y, pop_w, pop_h, Color32::WHITE);
        let frame = c.r(PAD, pop_y, pop_w, pop_h);
        c.ui.painter().rect_stroke(
            frame.shrink(0.5),
            0.,
            Stroke::new(1_f32, chrome::pal::HOT_BORDER),
            egui::StrokeKind::Middle,
        );
        let body = frame.shrink(1.);
        let hover = c.ui.ctx().pointer_hover_pos();
        let picked_row = chat.popup_pick.min(shown - 1);
        let picked = kit::rows_in(
            c,
            "chat_slash",
            body,
            kit::ROW,
            shown,
            Some(picked_row),
            &mut |c, r, row, _| {
                let hot = r == picked_row || hover.is_some_and(|p| row.contains(p));
                if hot {
                    c.ui.painter().rect_filled(row, 0., chrome::pal::HOT_FILL);
                }
                let item = &items[r];
                let line = format!("{} - {}", item.label, item.detail);
                kit::cell_text(c, row, &line, false, 6.);
            },
        )
        .clicked;
        if let Some(r) = picked {
            match &items[r].kind {
                prompts::SlashKind::Playbook { name, .. } => {
                    k.run_playbook = Some(name.clone());
                    chat.draft.clear();
                }
                prompts::SlashKind::SavedPrompt { text } => {
                    k.run_saved_prompt = Some(text.clone());
                    chat.draft.clear();
                }
            }
        }
    } else if let Some((at, items)) = &mention_items {
        let shown = items.len().min(7);
        let pop_h = (shown as f32) * kit::ROW + 2.;
        let pop_y = (context_y - GAP - pop_h).max(list_y);
        let pop_w = w - 2. * PAD;
        c.fill(PAD, pop_y, pop_w, pop_h, Color32::WHITE);
        let frame = c.r(PAD, pop_y, pop_w, pop_h);
        c.ui.painter().rect_stroke(
            frame.shrink(0.5),
            0.,
            Stroke::new(1_f32, chrome::pal::HOT_BORDER),
            egui::StrokeKind::Middle,
        );
        let body = frame.shrink(1.);
        let hover = c.ui.ctx().pointer_hover_pos();
        let picked_row = chat.popup_pick.min(shown - 1);
        let picked = kit::rows_in(
            c,
            "chat_mention",
            body,
            kit::ROW,
            shown,
            Some(picked_row),
            &mut |c, r, row, _| {
                let hot = r == picked_row || hover.is_some_and(|p| row.contains(p));
                if hot {
                    c.ui.painter().rect_filled(row, 0., chrome::pal::HOT_FILL);
                }
                let (code, name) = &items[r];
                let line = if name.is_empty() {
                    code.clone()
                } else {
                    format!("{code} - {name}")
                };
                kit::cell_text(c, row, &line, false, 6.);
            },
        )
        .clicked;
        if let Some(r) = picked {
            prompts::insert_at_mention(&mut chat.draft, *at, &items[r].0);
            chat.focus = true;
        }
    }

    // Mode row with Usage and Send or Stop.
    let mode_w = text_width(c, "Mode");
    c.label(PAD, send_y + 2., FIELD, "Mode", usable);
    let mut mode = MODES.iter().position(|(_, m)| *m == chat.mode).unwrap_or(0);
    let mode_items: Vec<(&str, bool)> = MODES.iter().map(|(l, _)| (*l, true)).collect();
    if c.combo(
        "chat_mode",
        PAD + mode_w + GAP,
        send_y + 2.,
        96.,
        FIELD,
        &mode_items,
        &mut mode,
        usable,
    ) {
        k.mode = Some(MODES[mode].1);
    }
    let send_x = w - PAD - kit::BUTTON.x;
    let usage = match chat.conv.usage {
        Some(u) => {
            let total = chat.conv.total_usage;
            let session_tip = if chat.conv.turns > 1 {
                format!(
                    "\nSession total ({} turns): {} in, {} out",
                    chat.conv.turns, total.input, total.output
                )
            } else {
                String::new()
            };
            Some((
                format!("{} tokens", model::tokens(u.input + u.output)),
                format!(
                    "Last reply: {} tokens in, {} out{session_tip}",
                    u.input, u.output
                ),
            ))
        }
        None if chat.conv.usage_unknown => Some((
            "tokens not reported".to_string(),
            "The server did not report the tokens of the last reply.".to_string(),
        )),
        None => None,
    };
    let show_queue_btn = chat.conv.running && usable && !chat.draft.trim().is_empty();
    let usage_right = if show_queue_btn {
        send_x - GAP - 60.
    } else {
        send_x
    };
    if let Some((text, tip)) = usage {
        let tx = usage_right - GAP - text_width(c, &text);
        let ux = PAD + mode_w + GAP + 96. + GAP;
        if tx >= ux {
            kit::secondary_text(c, tx, send_y + 6., &text);
            kit::tooltip(
                c,
                "chat_usage",
                tx,
                send_y + 4.,
                usage_right - GAP - tx,
                17.,
                &tip,
            );
        }
    }
    let pending = chat.conv.pending().is_some();
    if chat.conv.running {
        if show_queue_btn {
            let qw = 60.;
            let qx = send_x - GAP - qw;
            if kit::button(
                c,
                "chat_queue",
                qx,
                send_y,
                qw,
                kit::BUTTON.y,
                "&Queue",
                true,
            ) {
                k.queue = Some(chat.draft.clone());
            }
        }
        k.stop = kit::button(
            c,
            "chat_stop",
            send_x,
            send_y,
            kit::BUTTON.x,
            kit::BUTTON.y,
            "&Stop",
            true,
        );
    } else {
        let can_send = usable && !chat.draft.trim().is_empty();
        // Apply in the list is the default while a proposal waits; Send
        // otherwise. Enter in the composer always sends.
        let clicked = if pending {
            kit::button(
                c,
                "chat_send",
                send_x,
                send_y,
                kit::BUTTON.x,
                kit::BUTTON.y,
                "&Send",
                can_send,
            )
        } else {
            kit::default_button(
                c,
                "chat_send",
                send_x,
                send_y,
                kit::BUTTON.x,
                kit::BUTTON.y,
                "&Send",
                can_send,
            )
        };
        if clicked {
            k.send = Some(chat.draft.clone());
        }
    }
}
