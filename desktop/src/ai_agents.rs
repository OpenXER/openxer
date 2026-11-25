//! The AI Agents menu: connect Claude Desktop, ChatGPT desktop and Codex,
//! and Claude Code (and any other MCP client) to OpenXER's MCP server,
//! `openxer-cli mcp`.
//!
//! All config work is `openxer_automation::connect`, the module behind
//! `openxer-cli connect`, so the menu and the CLI back up, merge and check
//! files the same way, and both offer the same one action per app
//! ([`connect::Action`]). This file is only the menu, the dialogs and the
//! status bar hint. Where it looks for clients is an injected
//! [`connect::Env`]; a test build that injects none sees an empty profile,
//! never the real one.
use crate::chrome::{self, Item, INK};
use crate::dialog_kit as kit;
use crate::schedule_dialogs::{dialog_font, modal, Canvas};
use crate::App;
use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use openxer_automation::connect::{self, Action, Client, ClientStatus, Entry, Env};
use std::sync::mpsc;

pub(crate) mod row;

/// The user preference that hides the status bar hint.
pub(crate) const HINT_KEY: &str = "ai_agents_hint_dismissed_v1";

/// The intro of Connect AI Agents.
const INTRO: &str = "Click Connect next to an app on this PC, restart that app, and ask it to work on your schedule. Changes are written to new files. For these apps OpenXER itself sends nothing off this computer; only Chat does, when you use it with ChatGPT or with an API key.";
/// The example first request.
const FIRST_PROMPT: &str = "Try this first: \"Use OpenXER to inspect my schedule.\"";
/// The default button when more than one app can be connected: it connects
/// every app found that is not connected (fits the standard 108 x 25).
pub(crate) const CONNECT_ALL: &str = "Connect &All";

/// Questions worth asking an agent about a schedule: listed (quoted) in
/// What Can Agents Do? and offered as the Chat window's starters.
pub(crate) const THINGS_TO_ASK: &[&str] = &[
    "Which activities drive the finish date, and why?",
    "List activities with no predecessor or no successor.",
    "What is late against the baseline?",
    "Summarize this week's progress for the team.",
    "Mark the foundation activities complete as of Friday and reschedule.",
    "Level the schedule and export it to Excel for the team.",
];

/// The first request to an agent outside OpenXER, before the things to ask.
const FIRST_ASK: &str = "Use OpenXER to inspect my schedule.";

/// The title of What Can Agents Do?'s group that lists [`THINGS_TO_ASK`].
const ASK_TITLE: &str = "Things to ask";

/// What Can Agents Do?: headed groups of lines (the things to ask come from
/// [`THINGS_TO_ASK`]).
const ABOUT: &[(&str, &[&str])] = &[
    (
        "What an agent can do",
        &[
            "Inspect: read projects, activities, relationships, calendars and resources.",
            "Progress: set actual dates, remaining durations and percent complete.",
            "Logic: add, change and remove relationships and their lags.",
            "Schedule and level: run the schedule or resource leveling and report the dates.",
            "Documents: list them and read their local text, Word and PDF files.",
            "Export: write XER, Excel and PDF files.",
        ],
    ),
    (ASK_TITLE, &[]),
    (
        "Safety",
        &[
            "Input files are never changed: every edit is written to a new file.",
            "A batch of edits runs all or nothing; nothing is written if one fails.",
            "Ask for a dry run to see every change before anything is written.",
            "The server runs on this PC only. What your agent reads goes to its own service.",
        ],
    ),
];

/// Whether the status bar hint shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Hint {
    /// Not worked out yet: on the first status bar draw.
    #[default]
    Unknown,
    Shown,
    Hidden,
}

#[derive(Default)]
pub(crate) struct AgentsState {
    /// Where to look for clients; None is the process environment (an empty
    /// profile in a test build).
    pub(crate) env: Option<Env>,
    /// Connect AI Agents, while open.
    pub(crate) connect: Option<ConnectDialog>,
    /// What Can Agents Do?, while open.
    pub(crate) about: bool,
    /// Connection Status, while open: its lines.
    pub(crate) status_box: Option<String>,
    pub(crate) hint: Hint,
    /// The hint's words, naming the app found.
    pub(crate) hint_text: String,
}

pub(crate) struct ConnectDialog {
    pub(crate) statuses: Vec<ClientStatus>,
    /// The last action's result, with the next step after a connect.
    pub(crate) message: String,
    /// A running connection test.
    test: Option<mpsc::Receiver<Result<usize, String>>>,
    /// Whether the running test's result also goes to `message` (a test
    /// asked for; the one after a connect only fills the rows).
    test_to_message: bool,
    /// The last test's result, shown on connected rows.
    pub(crate) tested: Option<Result<usize, String>>,
}

impl ConnectDialog {
    pub(crate) fn testing(&self) -> bool {
        self.test.is_some()
    }

    /// The apps Connect All would connect.
    pub(crate) fn connect_all_targets(&self) -> Vec<Client> {
        connect::connect_all_targets(&self.statuses)
    }
}

/// The action key of a client's row button.
pub(crate) fn row_key(client: Client, what: &str) -> String {
    format!("ai_connect_{}_{what}", client.id().replace('-', "_"))
}

impl App {
    /// The environment the menu looks in.
    pub(crate) fn agent_env(&self) -> Env {
        if let Some(env) = &self.ai_agents.env {
            return env.clone();
        }
        let cli = connect::resolve_cli(std::env::current_exe().ok().as_deref());
        if cfg!(test) {
            // Tests never see the real profile unless they inject one.
            return Env {
                cli,
                ..Env::default()
            };
        }
        Env::from_process(cli)
    }

    /// The AI Agents menu.
    pub(crate) fn ai_agents_menu(&mut self, ui: &mut egui::Ui) {
        if self.item(ui, "menu_ai_connect", Item::new("&Connect AI Agents...")) {
            self.open_connect_agents();
        }
        if self.item(ui, "menu_ai_status", Item::new("Connection &Status...")) {
            self.open_connection_status();
        }
        self.chat_menu_item(ui);
        if self.item(
            ui,
            "menu_ai_copy_setup",
            Item::new("Copy Setup for &Other Agents"),
        ) {
            self.copy_agent_setup(ui.ctx());
        }
        chrome::menu_separator(ui);
        if self.item(ui, "menu_ai_skills", Item::new("S&kills...")) {
            self.open_skills();
        }
        if self.item(ui, "menu_ai_about", Item::new("&What Can Agents Do?")) {
            self.ai_agents.about = true;
        }
    }

    pub(crate) fn open_connect_agents(&mut self) {
        let env = self.agent_env();
        self.ai_agents.connect = Some(ConnectDialog {
            statuses: connect::status(&env),
            message: String::new(),
            test: None,
            test_to_message: false,
            tested: None,
        });
    }

    pub(crate) fn open_connection_status(&mut self) {
        let env = self.agent_env();
        let mut lines: Vec<String> = connect::status(&env)
            .iter()
            .map(|s| {
                let (state, detail) = row_text(s, None);
                let state = state.to_lowercase();
                match s.action {
                    Action::UseThisCopy | Action::Repair => {
                        format!("{}: {state} ({detail})", s.name)
                    }
                    _ => format!("{}: {state}", s.name),
                }
            })
            .collect();
        lines.push(match env.cli() {
            Ok(cli) => format!("Server: {}", cli.display()),
            Err(e) => e.message,
        });
        self.ai_agents.status_box = Some(lines.join("\n"));
    }

    /// Copy Setup for Other Agents: the generic MCP JSON on the clipboard.
    pub(crate) fn copy_agent_setup(&mut self, ctx: &egui::Context) {
        match self.agent_env().cli() {
            Ok(cli) => {
                ctx.copy_text(connect::setup_snippet(cli));
                self.status =
                    "MCP setup copied to the clipboard; paste it into your agent's MCP settings"
                        .into();
            }
            Err(e) => self.status = e.message,
        }
    }

    /// Copy Setup Prompt: the prompt any AI tool that can run commands
    /// follows to set itself up with this copy (`connect::setup_prompt`,
    /// the text `openxer-cli connect prompt` prints).
    pub(crate) fn copy_setup_prompt(&mut self, ctx: &egui::Context) {
        match self.agent_env().cli() {
            Ok(cli) => {
                ctx.copy_text(connect::setup_prompt(cli));
                self.status = "Setup prompt copied. Paste it into an AI tool that can run commands on this PC.".into();
            }
            Err(e) => self.status = e.message,
        }
    }

    /// Re-reads every client's state into the open dialog.
    fn refresh_connect(&mut self) {
        let statuses = connect::status(&self.agent_env());
        if statuses.iter().any(|s| s.entry.works()) {
            self.hide_agent_hint(true);
        }
        if let Some(d) = &mut self.ai_agents.connect {
            d.statuses = statuses;
        }
    }

    /// A row's button: Connect, Use This Copy or Repair (all connect this
    /// copy), or Disconnect. A connect ends with the app's next step and
    /// starts a connection test, whose result the row shows.
    pub(crate) fn agent_apply(&mut self, client: Client, disconnect: bool) {
        let env = self.agent_env();
        let (message, connected) = match connect::apply(&env, client, disconnect, false) {
            Ok(changes) => (
                connect::finish_message(client, disconnect, &changes, false),
                !disconnect,
            ),
            Err(e) => (e.message, false),
        };
        self.refresh_connect();
        if let Some(d) = &mut self.ai_agents.connect {
            d.message = message;
        }
        if connected {
            self.start_agent_test_quietly();
        }
    }

    /// Connect All: every found app that is not connected or
    /// needs repair; one connected to another copy is left as it is.
    pub(crate) fn agent_connect_all(&mut self) {
        let env = self.agent_env();
        let results = connect::connect_all(&env, false);
        let message = match results.as_slice() {
            [] => "Nothing to connect.".to_string(),
            [one] => one.message.clone(),
            many => many
                .iter()
                .map(|r| {
                    let text = r.message.strip_prefix("Connected. ").unwrap_or(&r.message);
                    format!("{}: {text}", r.client.name())
                })
                .collect::<Vec<_>>()
                .join(" "),
        };
        let any = results.iter().any(|r| r.ok);
        self.refresh_connect();
        if let Some(d) = &mut self.ai_agents.connect {
            d.message = message;
        }
        if any {
            self.start_agent_test_quietly();
        }
    }

    /// Test Connection (the strip button or a row's Test): the result goes
    /// to the rows and the message line.
    pub(crate) fn start_agent_test(&mut self) {
        self.spawn_agent_test(true);
    }

    /// The test after a connect: the result goes to the rows only, so the
    /// next step stays in view.
    fn start_agent_test_quietly(&mut self) {
        self.spawn_agent_test(false);
    }

    fn spawn_agent_test(&mut self, to_message: bool) {
        let env = self.agent_env();
        let Some(d) = &mut self.ai_agents.connect else {
            return;
        };
        let cli = match env.cli() {
            Ok(cli) => cli.to_path_buf(),
            Err(e) => {
                d.tested = Some(Err(e.message.clone()));
                if to_message {
                    d.message = e.message;
                }
                return;
            }
        };
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let result =
                connect::test_connection(&cli, connect::TEST_TIMEOUT).map_err(|e| e.message);
            let _ = send.send(result);
        });
        d.test = Some(receive);
        d.test_to_message = to_message;
        d.tested = None;
        if to_message {
            d.message = "Testing the connection...".into();
        }
    }

    fn poll_agent_test(&mut self, ctx: &egui::Context) {
        let Some(d) = &mut self.ai_agents.connect else {
            return;
        };
        let Some(receive) = &d.test else {
            return;
        };
        let result = match receive.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err("the test stopped".into()),
        };
        if d.test_to_message {
            d.message = test_text(&result);
        }
        d.tested = Some(result);
        d.test = None;
    }

    /// The menu's dialogs, each frame.
    pub(crate) fn ai_agents_dialogs(&mut self, ctx: &egui::Context) {
        self.connect_agents_dialog(ctx);
        self.agents_about_dialog(ctx);
        if let Some(text) = self.ai_agents.status_box.clone() {
            let mut rects = vec![];
            let answer = crate::kit_window::message_box(
                ctx,
                "ai_status",
                "Connection Status",
                crate::kit_window::MessageIcon::Info,
                &text,
                &["OK"],
                Some(0),
                &mut rects,
            );
            self.record_dialog_actions(rects);
            let escape = crate::kit_window::escape_goes_to(
                ctx,
                egui::Id::new(("kit_message", "ai_status")),
                egui::Order::Foreground,
            );
            if answer.is_some() || escape {
                self.ai_agents.status_box = None;
            }
        }
    }

    /// Connect AI Agents: the intro, one row per app with its state and the
    /// one thing to do (Connect, Use This Copy, Repair, a test result when
    /// connected, plain text when not installed), Disconnect beneath when
    /// there is an entry, the last result with the next step, and on the
    /// right Connect All (the default when more than one app can
    /// be connected), Close, Test Connection, and Copy Setup and Copy Setup
    /// Prompt for other agents.
    fn connect_agents_dialog(&mut self, ctx: &egui::Context) {
        self.poll_agent_test(ctx);
        let Some(d) = self.ai_agents.connect.take() else {
            return;
        };
        const W: f32 = 680.;
        const MAIN: f32 = W - kit::STRIP_WIDTH - 12.;
        const ROW: f32 = 62.;
        const ROWS_Y: f32 = 78.;
        const MESSAGE_LINES: usize = 6;
        // Prose wraps at a readable width (the dialog sweep's long line rule).
        const TEXT_W: f32 = 470.;
        let message_y = ROWS_Y + ROW * d.statuses.len() as f32 + 12.;
        let h = message_y + 15. * MESSAGE_LINES as f32 + 10.;
        let cli_ok = self.agent_env().cli().is_ok();
        let all = d.connect_all_targets().len();
        #[derive(Default)]
        struct Clicks {
            close: bool,
            test: bool,
            setup: bool,
            about: bool,
            all: bool,
            row: Option<(Client, bool)>,
            copy_command: bool,
            prompt: bool,
        }
        let (k, closed, actions) = modal(
            ctx,
            "ai_connect",
            "Connect AI Agents",
            Vec2::new(W, h),
            |c| {
                let mut k = Clicks::default();
                c.fill(0., 0., W, h, kit::FACE);
                let intro = wrap(c, INTRO, TEXT_W);
                for (i, line) in intro.iter().enumerate() {
                    kit::text(c, 10., 10. + 15. * i as f32, line, true);
                }
                kit::text(
                    c,
                    10.,
                    10. + 15. * intro.len() as f32 + 4.,
                    FIRST_PROMPT,
                    true,
                );
                // The app rows on a white panel.
                let panel_h = ROW * d.statuses.len() as f32 + 2.;
                c.fill(6., ROWS_Y - 1., MAIN, panel_h, Color32::WHITE);
                c.lowered(6., ROWS_Y - 1., MAIN, panel_h);
                for (i, s) in d.statuses.iter().enumerate() {
                    let y = ROWS_Y + ROW * i as f32;
                    if i > 0 {
                        c.hline(7., y, MAIN - 2., kit::SHADOW);
                    }
                    let (state, detail) = row_text(s, tested_for(&d, s));
                    let detail = [fit(c, &detail, row::text_width(MAIN - 6.))];
                    // The one thing to do.
                    let primary = match s.action {
                        Action::Connect => Some(("connect", "Connect", cli_ok)),
                        Action::UseThisCopy => Some(("connect", "Use This Copy", cli_ok)),
                        Action::Repair => Some(("connect", "Repair", cli_ok)),
                        Action::Connected => Some(("test", "Test", cli_ok && !d.testing())),
                        Action::FixFile => Some(("connect", "Copy Setup", cli_ok)),
                        Action::CopyCommand => Some(("copy", "Copy Command", s.command.is_some())),
                        Action::NotInstalled => None,
                    };
                    let primary_key = primary.map(|(what, _, _)| row_key(s.client, what));
                    // Disconnect stays, beneath, whenever there is an entry.
                    let has_entry = matches!(
                        s.action,
                        Action::Connected | Action::UseThisCopy | Action::Repair
                    );
                    let disconnect_key = row_key(s.client, "disconnect");
                    let clicked = row::paint(
                        c,
                        6.,
                        y,
                        MAIN - 6.,
                        &row::Row {
                            name: s.name,
                            mark: mark_of(s.action),
                            state: &state,
                            state_dim: s.action == Action::NotInstalled,
                            detail: &detail,
                            primary: primary.map(|(_, label, enabled)| row::Button {
                                key: primary_key.as_deref().unwrap_or_default(),
                                label,
                                enabled,
                            }),
                            secondary: has_entry.then(|| row::Button {
                                key: &disconnect_key,
                                label: "Disconnect",
                                enabled: true,
                            }),
                        },
                    );
                    match (clicked, primary) {
                        (Some(k2), _) if k2 == disconnect_key => k.row = Some((s.client, true)),
                        (Some(_), Some((what, _, _))) => match (s.action, what) {
                            (_, "test") => k.test = true,
                            (_, "copy") => k.copy_command = true,
                            (Action::FixFile, _) => k.setup = true,
                            _ => k.row = Some((s.client, false)),
                        },
                        _ => {}
                    }
                }
                let message = wrap(c, &d.message, TEXT_W);
                for (i, line) in message.iter().take(MESSAGE_LINES).enumerate() {
                    kit::text(c, 10., message_y + 15. * i as f32, line, true);
                }
                // The standard strip: 108 x 25 buttons, 30 apart, 10 more
                // before a new group. Connect All is the default when shown.
                kit::strip(c, W, h);
                let x = W - kit::STRIP_WIDTH;
                let (bw, bh) = (kit::BUTTON.x, kit::BUTTON.y);
                let mut y = kit::COLUMN_TOP;
                if all > 1 {
                    k.all =
                        kit::default_button(c, "ai_connect_all", x, y, bw, bh, CONNECT_ALL, cli_ok);
                    y += kit::BUTTON_PITCH;
                }
                k.close = if all > 1 {
                    kit::button(c, "ai_connect_close", x, y, bw, bh, "Close", true)
                } else {
                    kit::default_button(c, "ai_connect_close", x, y, bw, bh, "Close", true)
                };
                y += kit::BUTTON_PITCH + kit::GROUP_GAP;
                k.test |= kit::button(
                    c,
                    "ai_connect_test",
                    x,
                    y,
                    bw,
                    bh,
                    "&Test Connection",
                    cli_ok && !d.testing(),
                );
                y += kit::BUTTON_PITCH + kit::GROUP_GAP;
                kit::secondary_text(c, x + 2., y, "Other agents:");
                y += 17.;
                k.setup |= kit::button(c, "ai_connect_setup", x, y, bw, bh, "Copy &Setup", cli_ok);
                y += kit::BUTTON_PITCH;
                k.prompt = kit::button(
                    c,
                    "ai_connect_prompt",
                    x,
                    y,
                    bw,
                    bh,
                    "Copy Setup &Prompt",
                    cli_ok,
                );
                y += kit::BUTTON_PITCH + kit::GROUP_GAP;
                k.about = kit::button(
                    c,
                    "ai_connect_about",
                    x,
                    y,
                    bw,
                    bh,
                    "&About Agents...",
                    true,
                );
                k
            },
        );
        self.record_dialog_actions(actions);
        if k.close || closed {
            return;
        }
        self.ai_agents.connect = Some(d);
        if k.prompt {
            self.copy_setup_prompt(ctx);
            let status = self.status.clone();
            if let Some(d) = &mut self.ai_agents.connect {
                d.message = status;
            }
        } else if k.setup {
            self.copy_agent_setup(ctx);
            let status = self.status.clone();
            if let Some(d) = &mut self.ai_agents.connect {
                d.message = status;
            }
        } else if let Some((client, disconnect)) = k.row {
            self.agent_apply(client, disconnect);
        } else if k.all {
            self.agent_connect_all();
        }
        if k.copy_command {
            let command = self
                .ai_agents
                .connect
                .as_ref()
                .and_then(|d| d.statuses.iter().find_map(|s| s.command.clone()));
            if let Some(command) = command {
                ctx.copy_text(command);
                if let Some(d) = &mut self.ai_agents.connect {
                    d.message = format!("Command copied. {}", Client::ClaudeCode.next_step());
                }
            }
        }
        if k.test {
            self.start_agent_test();
        }
        if k.about {
            self.ai_agents.about = true;
        }
    }

    /// What Can Agents Do?: read-only.
    fn agents_about_dialog(&mut self, ctx: &egui::Context) {
        if !self.ai_agents.about {
            return;
        }
        const W: f32 = 520.;
        let lines: usize = ABOUT.iter().map(|(t, l)| about_lines(t, l).len() + 2).sum();
        let h = 12. + 15. * lines as f32 + 40.;
        let (close, closed, actions) = modal(
            ctx,
            "ai_about",
            "What Can Agents Do?",
            Vec2::new(W, h),
            |c| {
                c.fill(0., 0., W, h, kit::FACE);
                let mut y = 10.;
                for (title, items) in ABOUT {
                    c.bold(10., y, 15., title);
                    y += 17.;
                    for item in about_lines(title, items) {
                        kit::text(c, 22., y, &item, true);
                        y += 15.;
                    }
                    y += 13.;
                }
                kit::default_button(
                    c,
                    "ai_about_close",
                    W - kit::BUTTON.x - 10.,
                    h - kit::BUTTON.y - 8.,
                    kit::BUTTON.x,
                    kit::BUTTON.y,
                    "Close",
                    true,
                )
            },
        );
        self.record_dialog_actions(actions);
        if close || closed {
            self.ai_agents.about = false;
        }
    }

    // ---------------------------------------------------------------- hint

    /// Works out whether the hint shows: until dismissed, or until a client
    /// is connected. A test build keeps it hidden unless a test asks.
    pub(crate) fn restore_agent_hint(&mut self) {
        if cfg!(test) {
            self.ai_agents.hint = Hint::Hidden;
        } else {
            self.load_agent_hint();
        }
    }

    /// The hint shows once an app it can connect is found on this PC and
    /// nothing is connected, until it is dismissed; it names the app.
    pub(crate) fn load_agent_hint(&mut self) {
        let dismissed = self.store.preference(HINT_KEY).ok().flatten().is_some();
        let statuses = if dismissed {
            vec![]
        } else {
            connect::status(&self.agent_env())
        };
        let connected = statuses.iter().any(|s| s.entry.works());
        let found: Vec<&ClientStatus> = statuses.iter().filter(|s| s.found).collect();
        self.ai_agents.hint_text = hint_text(&found);
        self.ai_agents.hint = if dismissed || connected || found.is_empty() {
            Hint::Hidden
        } else {
            Hint::Shown
        };
    }

    /// Hides the hint; `store` keeps it hidden from now on.
    pub(crate) fn hide_agent_hint(&mut self, store: bool) {
        self.ai_agents.hint = Hint::Hidden;
        if store {
            if let Err(e) = self.store.set_preference(HINT_KEY, "1") {
                self.status = e;
            }
        }
    }

    /// The hint at the right end of the status bar's message panel, when it
    /// fits beside the message: clicking it opens Connect AI Agents, its x
    /// dismisses it for good.
    pub(crate) fn agent_hint(&mut self, ui: &mut egui::Ui, panel: Rect) {
        if self.ai_agents.hint == Hint::Unknown {
            self.load_agent_hint();
        }
        if self.ai_agents.hint != Hint::Shown {
            return;
        }
        let font = chrome::ui_font();
        let painter = ui.painter().clone();
        let width = |s: &str| {
            painter
                .layout_no_wrap(s.to_string(), font.clone(), INK)
                .size()
                .x
        };
        let label = self.ai_agents.hint_text.clone();
        let text_w = width(&label);
        let close = Rect::from_min_size(
            Pos2::new(panel.right() - 30., panel.top() + 2.),
            Vec2::new(12., 12.),
        );
        let text = Rect::from_min_max(
            Pos2::new(close.left() - 6. - text_w - 4., panel.top() + 1.),
            Pos2::new(close.left() - 4., panel.bottom() - 1.),
        );
        if text.left() < panel.left() + 3. + width(&self.status) + 16. {
            return;
        }
        let hint = ui.interact(text, egui::Id::new("ai_agents_hint"), Sense::click());
        let dismiss = ui.interact(
            close,
            egui::Id::new("ai_agents_hint_dismiss"),
            Sense::click(),
        );
        let ink = Color32::from_rgb(0x0D, 0x47, 0xA1);
        painter.text(
            Pos2::new(text.left() + 2., panel.top() - 1.),
            Align2::LEFT_TOP,
            &label,
            font,
            ink,
        );
        if hint.hovered() {
            painter.hline(
                text.left() + 2.0..=text.right() - 2.,
                text.bottom() - 2.,
                Stroke::new(1_f32, ink),
            );
        }
        let x_ink = if dismiss.hovered() { INK } else { kit::SHADOW };
        let m = close.center();
        for (a, b) in [
            (Vec2::new(-3., -3.), Vec2::new(3., 3.)),
            (Vec2::new(-3., 3.), Vec2::new(3., -3.)),
        ] {
            painter.line_segment([m + a, m + b], Stroke::new(1.5_f32, x_ink));
        }
        let hint = hint.on_hover_text("Open Connect AI Agents");
        let dismiss = dismiss.on_hover_text("Hide this hint");
        self.action("status_ai_hint", &hint);
        self.action("status_ai_hint_dismiss", &dismiss);
        if dismiss.clicked() {
            self.hide_agent_hint(true);
        } else if hint.clicked() {
            self.open_connect_agents();
        }
    }
}

/// The lines of one What Can Agents Do? group: its own, or the first
/// request and the things to ask, quoted.
fn about_lines(title: &str, items: &[&str]) -> Vec<String> {
    if title != ASK_TITLE {
        return items.iter().map(|s| s.to_string()).collect();
    }
    std::iter::once(FIRST_ASK)
        .chain(THINGS_TO_ASK.iter().copied())
        .map(|s| format!("\"{s}\""))
        .collect()
}

/// The status bar hint's words: the app found, or "your AI apps" for
/// several.
pub(crate) fn hint_text(found: &[&ClientStatus]) -> String {
    match found {
        [one] if one.client != Client::Codex => format!("Connect {} to OpenXER", one.name),
        [_] => "Connect ChatGPT or Codex to OpenXER".to_string(),
        _ => "Connect your AI apps to OpenXER".to_string(),
    }
}

/// A test result in words.
fn test_text(result: &Result<usize, String>) -> String {
    match result {
        Ok(n) => format!("Working: {n} tools"),
        Err(e) => format!("Not working: {e}"),
    }
}

/// The test result a connected row shows (the test runs this copy's
/// server, which every connected app starts).
fn tested_for<'a>(d: &'a ConnectDialog, s: &ClientStatus) -> Option<Result<&'a usize, &'a String>> {
    (s.action == Action::Connected)
        .then(|| d.tested.as_ref().map(|r| r.as_ref()))
        .flatten()
}

/// A row's state line and its grey detail line, in plain words.
fn row_text(s: &ClientStatus, tested: Option<Result<&usize, &String>>) -> (String, String) {
    let file = || {
        s.files
            .first()
            .map(|f| f.path.display().to_string())
            .unwrap_or_default()
    };
    match (s.action, &s.entry) {
        (Action::NotInstalled, _) => (
            "Not installed".into(),
            "Install it, then come back here.".into(),
        ),
        (Action::CopyCommand, _) => (
            "Found: connect it with a command".into(),
            s.command.clone().unwrap_or_default(),
        ),
        (Action::Connect, _) => ("Found, not connected".into(), file()),
        (Action::Connected, _) => (
            "Connected".into(),
            match tested {
                Some(Ok(n)) => format!("Working: {n} tools"),
                Some(Err(e)) => format!("Not working: {e}"),
                None => file(),
            },
        ),
        (Action::UseThisCopy, Entry::OtherCopy { command, installed }) => (
            s.entry.label().into(),
            format!(
                "at {command}{}",
                if *installed {
                    " (the installed copy)"
                } else {
                    ""
                }
            ),
        ),
        (Action::Repair, Entry::Stale { command }) if !command.is_empty() => (
            s.entry.label().into(),
            format!("It runs {command}, which no longer works."),
        ),
        _ => (s.entry.label().into(), file()),
    }
}

/// The mark before a row's state: a tick when connected, a warning when it
/// needs a look, a ring when not.
fn mark_of(action: Action) -> row::Mark {
    match action {
        Action::Connected => row::Mark::Ready,
        Action::UseThisCopy | Action::Repair | Action::FixFile => row::Mark::Attention,
        _ => row::Mark::Off,
    }
}

fn text_width(c: &Canvas, s: &str) -> f32 {
    c.ui.painter()
        .layout_no_wrap(s.to_string(), dialog_font(), INK)
        .size()
        .x
}

/// `s` broken into lines no wider than `width`.
pub(crate) fn wrap(c: &Canvas, s: &str, width: f32) -> Vec<String> {
    let mut lines = vec![];
    let mut line = String::new();
    for word in s.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if text_width(c, &candidate) <= width {
            line = candidate;
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        // A word wider than the line (a long path) is broken by characters.
        for ch in word.chars() {
            line.push(ch);
            if text_width(c, &line) > width {
                let last = line.pop().expect("one char");
                lines.push(std::mem::take(&mut line));
                line.push(last);
            }
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// `s` shortened in the middle to fit `width`.
pub(crate) fn fit(c: &Canvas, s: &str, width: f32) -> String {
    if text_width(c, s) <= width {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut keep = chars.len();
    while keep > 4 {
        keep -= 1;
        let head = keep / 3;
        let tail = keep - head;
        let candidate: String = chars[..head]
            .iter()
            .chain(['.', '.', '.'].iter())
            .chain(chars[chars.len() - tail..].iter())
            .collect();
        if text_width(c, &candidate) <= width {
            return candidate;
        }
    }
    String::new()
}

/// An empty client profile under `dir` (APPDATA, LOCALAPPDATA, home and
/// PATH folders, nothing installed), for test drivers.
#[cfg(test)]
pub(crate) fn temp_profile(dir: &std::path::Path) -> Env {
    let root = dir.join("agent-profile");
    for sub in ["Roaming", "Local", "home", "bin"] {
        let _ = std::fs::create_dir_all(root.join(sub));
    }
    Env {
        appdata: Some(root.join("Roaming")),
        local_appdata: Some(root.join("Local")),
        home: Some(root.join("home")),
        codex_home: None,
        path: Some(root.join("bin").into_os_string()),
        cli: connect::resolve_cli(std::env::current_exe().ok().as_deref()),
    }
}

pub(crate) mod skills_ui;
#[cfg(test)]
mod tests;
