//! The AI Agents menu through the production UI (`App::update_ui` driven by
//! synthetic input), on fake profiles in temporary folders only: no test
//! here reads or writes a real client config.
use super::*;
use crate::headless_render::SoftwarePainter;
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
            "openxer-ai-agents-{}-{}",
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

/// A fake profile under `dir`; `cli` is the executable clients should run
/// (a stand-in file unless a test needs the real server).
fn profile(dir: &Path, cli: Option<PathBuf>) -> Env {
    for sub in ["Roaming", "Local", "home", "bin", "app"] {
        fs::create_dir_all(dir.join(sub)).unwrap();
    }
    let cli = cli.unwrap_or_else(|| {
        let stand_in = dir.join("app").join(connect::CLI_NAME);
        fs::write(&stand_in, b"stand-in").unwrap();
        stand_in
    });
    Env {
        appdata: Some(dir.join("Roaming")),
        local_appdata: Some(dir.join("Local")),
        home: Some(dir.join("home")),
        codex_home: None,
        path: Some(dir.join("bin").into_os_string()),
        cli: Some(cli),
    }
}

fn driver(name: &str) -> (Driver, Tmp, Env) {
    let dir = Tmp::new();
    let env = profile(dir.path(), None);
    let mut d = Driver::new(name, openxer_xer::starter());
    d.app.ai_agents.env = Some(env.clone());
    (d, dir, env)
}

fn codex_file(env: &Env) -> PathBuf {
    env.codex_file().unwrap()
}

fn claude_file(env: &Env) -> PathBuf {
    env.appdata
        .as_ref()
        .unwrap()
        .join("Claude")
        .join(connect::CLAUDE_DESKTOP_FILE)
}

/// Claude Desktop connected, Codex pointing at an old copy, Claude Code on
/// PATH.
fn mixed(env: &Env) {
    let cli = env.cli.as_ref().unwrap().display().to_string();
    let claude = claude_file(env);
    fs::create_dir_all(claude.parent().unwrap()).unwrap();
    fs::write(
        &claude,
        serde_json::json!({"mcpServers":{"openxer":{"command":cli,"args":["mcp"]}}}).to_string(),
    )
    .unwrap();
    let codex = codex_file(env);
    fs::create_dir_all(codex.parent().unwrap()).unwrap();
    fs::write(
        &codex,
        "[mcp_servers.openxer]\ncommand = 'D:\\Old OpenXER\\openxer-cli.exe'\nargs = [\"mcp\"]\n",
    )
    .unwrap();
    let claude_code = if cfg!(windows) {
        "claude.cmd"
    } else {
        "claude"
    };
    fs::write(
        env.path
            .as_ref()
            .map(PathBuf::from)
            .unwrap()
            .join(claude_code),
        b"",
    )
    .unwrap();
}

fn open_dialog(d: &mut Driver) {
    d.home();
    d.click("menu_bar_ai_agents");
    d.click("menu_ai_connect");
    d.frames(2);
    assert!(
        d.app.ai_agents.connect.is_some(),
        "Connect AI Agents opened"
    );
}

fn message(d: &Driver) -> String {
    d.app.ai_agents.connect.as_ref().unwrap().message.clone()
}

fn text_shown(d: &mut Driver, needle: &str) -> bool {
    d.texts().iter().any(|(t, _)| t.contains(needle))
}

#[test]
fn menu_sits_between_tools_and_admin_with_its_four_commands() {
    let (mut d, _dir, _env) = driver("ai-menu");
    d.home();
    let tools = d.rect("menu_bar_tools");
    let agents = d.rect("menu_bar_ai_agents");
    let admin = d.rect("menu_bar_admin");
    assert!(tools.right() <= agents.left() + 0.5 && agents.right() <= admin.left() + 0.5);
    assert!(crate::qa_sweep::MENUS
        .iter()
        .any(|(m, _)| *m == "ai_agents"));
    d.click("menu_bar_ai_agents");
    for item in [
        "menu_ai_connect",
        "menu_ai_status",
        "menu_ai_copy_setup",
        "menu_ai_about",
    ] {
        assert!(d.has(item), "{item}");
    }
    let order: Vec<f32> = [
        "menu_ai_connect",
        "menu_ai_status",
        "menu_ai_copy_setup",
        "menu_ai_about",
    ]
    .iter()
    .map(|k| d.rect(k).top())
    .collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
    d.key(Key::Escape);
    d.key(Key::Escape);
    d.frames(2);
    assert!(!d.has("menu_ai_connect"));
}

#[test]
fn alt_i_opens_the_menu_and_its_letters_run_the_commands() {
    let (mut d, _dir, _env) = driver("ai-keys");
    d.home();
    d.key_with(Key::I, egui::Modifiers::ALT);
    d.frames(2);
    assert!(d.has("menu_ai_connect"), "Alt+I opens AI Agents");
    d.key(Key::C);
    d.frames(2);
    assert!(
        d.app.ai_agents.connect.is_some(),
        "C runs Connect AI Agents..."
    );
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.ai_agents.connect.is_none());
    d.home();
    d.key_with(Key::I, egui::Modifiers::ALT);
    d.frames(2);
    d.key(Key::W);
    d.frames(2);
    assert!(d.app.ai_agents.about, "W runs What Can Agents Do?");
    d.key(Key::Escape);
    d.frames(2);
    assert!(!d.app.ai_agents.about);
    d.home();
    d.key_with(Key::I, egui::Modifiers::ALT);
    d.frames(2);
    d.key(Key::S);
    d.frames(2);
    assert!(
        d.app.ai_agents.status_box.is_some(),
        "S runs Connection Status"
    );
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.ai_agents.status_box.is_none());
}

/// Claude Desktop and the ChatGPT/Codex app installed (their folders), with
/// no config files yet.
fn install_both(env: &Env) {
    fs::create_dir_all(claude_file(env).parent().unwrap()).unwrap();
    fs::create_dir_all(codex_file(env).parent().unwrap()).unwrap();
}

/// Another copy of OpenXER in the profile; `installed` puts the installer's
/// marker beside it.
fn other_copy(env: &Env, installed: bool) -> PathBuf {
    let dir = env.home.as_ref().unwrap().join("Other OpenXER");
    fs::create_dir_all(&dir).unwrap();
    let cli = dir.join(connect::CLI_NAME);
    fs::write(&cli, b"another copy").unwrap();
    if installed {
        fs::write(dir.join(connect::INSTALLED_MARKER), b"").unwrap();
    }
    cli
}

fn statuses(d: &Driver) -> Vec<ClientStatus> {
    d.app.ai_agents.connect.as_ref().unwrap().statuses.clone()
}

#[test]
fn nothing_installed_shows_plain_text_and_no_dead_buttons() {
    let (mut d, _dir, _env) = driver("ai-rows-empty");
    open_dialog(&mut d);
    let s = statuses(&d);
    assert!(s.iter().all(|s| s.action == Action::NotInstalled));
    assert!(text_shown(&mut d, "Not installed"));
    assert!(text_shown(&mut d, "Use OpenXER to inspect my schedule."));
    for client in Client::ALL {
        for what in ["connect", "copy", "test", "disconnect"] {
            assert!(!d.has(&row_key(client, what)), "{client:?} {what}");
        }
    }
    // Nothing to connect: no Connect All, Close is the default.
    assert!(!d.has("ai_connect_all"));
    d.key(Key::Enter);
    d.frames(2);
    assert!(d.app.ai_agents.connect.is_none(), "Enter runs Close");
}

#[test]
fn rows_show_connected_repair_and_the_command() {
    let (mut d, _dir, env) = driver("ai-rows");
    mixed(&env);
    open_dialog(&mut d);
    let s = statuses(&d);
    assert_eq!(s[0].action, Action::Connected);
    assert_eq!(
        s[1].name, "ChatGPT desktop and Codex",
        "right after Claude Desktop"
    );
    assert!(matches!(&s[1].entry, Entry::Stale { command } if command.contains("Old OpenXER")));
    assert_eq!(s[1].action, Action::Repair);
    assert_eq!(s[2].action, Action::CopyCommand);
    assert!(text_shown(&mut d, "Needs repair"));
    assert!(text_shown(&mut d, "which no longer works"));
    assert!(text_shown(&mut d, "Connected"));
    assert!(text_shown(&mut d, "Found: connect it with a command"));
    assert!(text_shown(&mut d, "Repair"));
    // Connected: Test, and Disconnect beneath; no Connect.
    assert!(d.has(&row_key(Client::ClaudeDesktop, "test")));
    assert!(d.has(&row_key(Client::ClaudeDesktop, "disconnect")));
    assert!(!d.has(&row_key(Client::ClaudeDesktop, "connect")));
    assert!(d.has(&row_key(Client::ClaudeCode, "copy")));
    assert!(!d.has(&row_key(Client::ClaudeCode, "disconnect")));
    // One app to connect: no Connect All.
    assert!(!d.has("ai_connect_all"));
    d.key(Key::Escape);
    d.frames(2);
    assert!(
        d.app.ai_agents.connect.is_none(),
        "Escape closes Connect AI Agents"
    );
}

#[test]
fn connect_shows_the_next_step_and_disconnect_round_trips() {
    let (mut d, _dir, env) = driver("ai-round-trip");
    install_both(&env);
    let codex = codex_file(&env);
    let original = "# mine\nmodel = \"m\"\n";
    fs::write(&codex, original).unwrap();
    open_dialog(&mut d);
    d.click(&row_key(Client::Codex, "connect"));
    d.frames(1);
    let next = message(&d);
    assert!(next.starts_with("Connected. Restart the app"), "{next}");
    assert!(next.contains("Work or Codex mode"), "{next}");
    assert!(text_shown(&mut d, "Restart the app"));
    let written = fs::read_to_string(&codex).unwrap();
    assert!(written.starts_with(original) && written.contains("[mcp_servers.openxer]"));
    assert_eq!(statuses(&d)[1].action, Action::Connected);
    assert_eq!(
        d.app.ai_agents.hint,
        Hint::Hidden,
        "a connection ends the hint"
    );
    // The test after the connect runs by itself; its result is the row's.
    let dialog = d.app.ai_agents.connect.as_ref().unwrap();
    assert!(dialog.testing() || dialog.tested.is_some());
    let backups = || {
        fs::read_dir(codex.parent().unwrap())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains(".openxer-backup-")
            })
            .count()
    };
    assert_eq!(backups(), 1);
    // Claude Desktop: installed, no config yet; Connect creates it.
    d.click(&row_key(Client::ClaudeDesktop, "connect"));
    d.frames(1);
    let next = message(&d);
    assert!(
        next.starts_with("Connected. Quit Claude Desktop from its tray icon"),
        "{next}"
    );
    assert!(
        next.ends_with("then ask: \"Use OpenXER to inspect my schedule.\""),
        "{next}"
    );
    assert!(claude_file(&env).is_file());
    d.click(&row_key(Client::Codex, "disconnect"));
    d.frames(1);
    assert_eq!(
        message(&d),
        "Disconnected. Restart ChatGPT desktop and Codex to finish."
    );
    assert_eq!(fs::read_to_string(&codex).unwrap(), original);
    assert_eq!(backups(), 2);
    assert_eq!(statuses(&d)[1].action, Action::Connect);
    d.click("ai_connect_close");
    assert!(d.app.ai_agents.connect.is_none());
}

#[test]
fn connect_all_is_the_default_when_two_apps_can_be_connected() {
    let (mut d, _dir, env) = driver("ai-connect-all");
    install_both(&env);
    open_dialog(&mut d);
    assert_eq!(
        d.app
            .ai_agents
            .connect
            .as_ref()
            .unwrap()
            .connect_all_targets(),
        vec![Client::ClaudeDesktop, Client::Codex]
    );
    assert!(d.has("ai_connect_all"));
    assert!(text_shown(&mut d, "Connect All"));
    // Enter runs the default button.
    d.key(Key::Enter);
    d.frames(2);
    assert!(d.app.ai_agents.connect.is_some(), "the dialog stays open");
    let s = statuses(&d);
    assert_eq!(s[0].action, Action::Connected);
    assert_eq!(s[1].action, Action::Connected);
    let m = message(&d);
    assert!(
        m.contains("Claude Desktop: Quit Claude Desktop from its tray icon"),
        "{m}"
    );
    assert!(
        m.contains("ChatGPT desktop and Codex: Restart the app"),
        "{m}"
    );
    // Nothing left to connect: the button goes.
    d.frames(1);
    assert!(!d.has("ai_connect_all"));
    d.key(Key::Escape);
    d.frames(1);
    assert!(d.app.ai_agents.connect.is_none());
}

#[test]
fn use_this_copy_switches_only_when_asked() {
    let (mut d, _dir, env) = driver("ai-use-this-copy");
    install_both(&env);
    let other = other_copy(&env, true);
    let claude = claude_file(&env);
    let text = format!(
        "{{\n  \"mcpServers\": {{\n    \"openxer\": {{\n      \"command\": {},\n      \"args\": [\"mcp\"]\n    }}\n  }}\n}}\n",
        serde_json::json!(other.display().to_string())
    );
    fs::write(&claude, &text).unwrap();
    open_dialog(&mut d);
    let s = statuses(&d);
    assert_eq!(s[0].action, Action::UseThisCopy);
    assert!(text_shown(&mut d, "Connected to another copy of OpenXER"));
    assert!(text_shown(&mut d, "(the installed copy)"));
    assert!(text_shown(&mut d, "Use This Copy"));
    // Connect All is not offered for it: only Codex is a target, so there
    // is no Connect All button, and connecting all leaves it alone.
    assert_eq!(
        d.app
            .ai_agents
            .connect
            .as_ref()
            .unwrap()
            .connect_all_targets(),
        vec![Client::Codex]
    );
    assert!(!d.has("ai_connect_all"));
    d.app.agent_connect_all();
    d.frames(1);
    assert_eq!(
        fs::read_to_string(&claude).unwrap(),
        text,
        "never repointed"
    );
    // Use This Copy switches it.
    d.click(&row_key(Client::ClaudeDesktop, "connect"));
    d.frames(1);
    assert_eq!(statuses(&d)[0].action, Action::Connected);
    assert!(message(&d).contains("tray icon"), "{}", message(&d));
    let now = fs::read_to_string(&claude).unwrap();
    assert!(now
        .contains(&serde_json::json!(env.cli.as_ref().unwrap().display().to_string()).to_string()));
    d.key(Key::Escape);
}

#[test]
fn repair_points_a_broken_entry_at_this_copy() {
    let (mut d, _dir, env) = driver("ai-repair");
    mixed(&env);
    open_dialog(&mut d);
    assert_eq!(statuses(&d)[1].action, Action::Repair);
    d.click(&row_key(Client::Codex, "connect"));
    d.frames(1);
    assert_eq!(statuses(&d)[1].action, Action::Connected);
    assert!(message(&d).starts_with("Connected. Restart the app"));
    let text = fs::read_to_string(codex_file(&env)).unwrap();
    assert!(
        text.contains(&format!(
            "command = '{}'",
            env.cli.as_ref().unwrap().display()
        )),
        "{text}"
    );
    d.key(Key::Escape);
}

#[test]
fn malformed_config_is_left_alone_and_offers_copy_setup() {
    let (mut d, _dir, env) = driver("ai-malformed");
    let claude = claude_file(&env);
    fs::create_dir_all(claude.parent().unwrap()).unwrap();
    fs::write(&claude, "{ \"mcpServers\": [").unwrap();
    open_dialog(&mut d);
    assert!(matches!(statuses(&d)[0].entry, Entry::Malformed { .. }));
    assert_eq!(statuses(&d)[0].action, Action::FixFile);
    assert!(text_shown(&mut d, "Its settings file cannot be read"));
    d.click(&row_key(Client::ClaudeDesktop, "connect"));
    d.frames(1);
    assert!(
        message(&d).starts_with("MCP setup copied"),
        "{}",
        message(&d)
    );
    assert_eq!(fs::read_to_string(&claude).unwrap(), "{ \"mcpServers\": [");
    d.key(Key::Escape);
}

#[test]
fn copy_setup_and_connection_status_from_the_menu() {
    let (mut d, _dir, env) = driver("ai-copy");
    mixed(&env);
    d.home();
    d.click("menu_bar_ai_agents");
    d.click("menu_ai_copy_setup");
    d.frames(1);
    assert!(
        d.app
            .status
            .starts_with("MCP setup copied to the clipboard"),
        "{}",
        d.app.status
    );
    d.home();
    d.click("menu_bar_ai_agents");
    d.click("menu_ai_status");
    d.frames(2);
    let text = d.app.ai_agents.status_box.clone().unwrap();
    assert!(text.contains("Claude Desktop: connected"), "{text}");
    assert!(
        text.contains("ChatGPT desktop and Codex: needs repair (It runs D:\\Old OpenXER"),
        "{text}"
    );
    assert!(
        text.contains("Claude Code: found: connect it with a command"),
        "{text}"
    );
    assert!(text.contains("Server: "), "{text}");
    d.click("ai_status_ok");
    d.frames(1);
    assert!(d.app.ai_agents.status_box.is_none());
}

#[test]
fn what_can_agents_do_is_read_only_and_closes() {
    let (mut d, _dir, _env) = driver("ai-about");
    let model = d.app.project.model.clone();
    d.home();
    d.click("menu_bar_ai_agents");
    d.click("menu_ai_about");
    d.frames(2);
    assert!(d.app.ai_agents.about);
    assert!(text_shown(&mut d, "all or nothing"));
    assert!(text_shown(&mut d, "dry run"));
    d.key(Key::Enter);
    d.frames(2);
    assert!(!d.app.ai_agents.about, "Enter runs Close");
    assert_eq!(d.app.project.model, model);
    // From the Connect dialog too.
    open_dialog(&mut d);
    d.click("ai_connect_about");
    d.frames(2);
    assert!(d.app.ai_agents.about);
    d.click("ai_about_close");
    d.frames(1);
    assert!(!d.app.ai_agents.about);
    assert!(d.app.ai_agents.connect.is_some());
}

#[test]
fn hint_names_the_app_found_and_opens_the_dialog_in_one_click() {
    let (mut d, _dir, env) = driver("ai-hint");
    assert_eq!(
        d.app.ai_agents.hint,
        Hint::Hidden,
        "test builds start hidden"
    );
    // Nothing found: no hint.
    d.app.load_agent_hint();
    assert_eq!(d.app.ai_agents.hint, Hint::Hidden);
    // Claude Desktop found, nothing connected: the hint names it.
    fs::create_dir_all(claude_file(&env).parent().unwrap()).unwrap();
    d.app.load_agent_hint();
    assert_eq!(d.app.ai_agents.hint, Hint::Shown);
    assert_eq!(
        d.app.ai_agents.hint_text,
        "Connect Claude Desktop to OpenXER"
    );
    d.frames(2);
    assert!(d.has("status_ai_hint") && d.has("status_ai_hint_dismiss"));
    // One click opens the dialog, with Connect on the app's row.
    d.click("status_ai_hint");
    d.frames(1);
    assert!(d.app.ai_agents.connect.is_some());
    assert!(d.has(&row_key(Client::ClaudeDesktop, "connect")));
    d.key(Key::Escape);
    d.frames(1);
    // Its x dismisses it for good.
    d.click("status_ai_hint_dismiss");
    d.frames(2);
    assert!(!d.has("status_ai_hint"));
    assert_eq!(
        d.app.store.preference(HINT_KEY).unwrap().as_deref(),
        Some("1")
    );
    d.app.load_agent_hint();
    assert_eq!(d.app.ai_agents.hint, Hint::Hidden);

    // Two apps found: one hint for both.
    let (mut d, _dir2, env2) = driver("ai-hint-two");
    install_both(&env2);
    d.app.load_agent_hint();
    assert_eq!(d.app.ai_agents.hint_text, "Connect your AI apps to OpenXER");
    // A working connection, even to another copy, hides it.
    let other = other_copy(&env2, false);
    fs::write(
        codex_file(&env2),
        format!(
            "[mcp_servers.openxer]\ncommand = '{}'\nargs = [\"mcp\"]\n",
            other.display()
        ),
    )
    .unwrap();
    d.app.load_agent_hint();
    assert_eq!(d.app.ai_agents.hint, Hint::Hidden);
    let (mut d, _dir3, env3) = driver("ai-hint-connected");
    mixed(&env3);
    d.app.load_agent_hint();
    assert_eq!(d.app.ai_agents.hint, Hint::Hidden);
}

#[test]
fn test_builds_never_look_at_the_real_profile() {
    let d = Driver::new("ai-default-env", openxer_xer::starter());
    // The QA driver injects a temporary profile.
    let env = d.app.agent_env();
    let tmp = std::env::temp_dir();
    for path in [&env.appdata, &env.local_appdata, &env.home] {
        assert!(
            path.as_ref().is_some_and(|p| p.starts_with(&tmp)),
            "{path:?}"
        );
    }
    assert_eq!(env.codex_home, None);
    // An app with nothing injected sees an empty profile.
    let mut d = d;
    d.app.ai_agents.env = None;
    let env = d.app.agent_env();
    assert_eq!(
        (env.appdata, env.local_appdata, env.home, env.path),
        (None, None, None, None)
    );
}

/// The real server, built beside the test runner (`cargo test` of the
/// automation crate builds it), or `OPENXER_CLI`.
fn real_cli() -> Option<PathBuf> {
    let cli = connect::resolve_cli(std::env::current_exe().ok().as_deref())?;
    cli.is_file().then_some(cli)
}

#[test]
fn test_connection_runs_the_real_server() {
    let Some(cli) = real_cli() else {
        println!("skipped: openxer-cli is not built next to the test runner; build -p openxer-automation or set OPENXER_CLI");
        return;
    };
    let dir = Tmp::new();
    let mut d = Driver::new("ai-test-connection", openxer_xer::starter());
    d.app.ai_agents.env = Some(profile(dir.path(), Some(cli)));
    open_dialog(&mut d);
    let c = d.app.ai_agents.connect.as_ref().unwrap();
    assert!(
        !c.testing() && c.tested.is_none(),
        "no test before the click"
    );
    d.click("ai_connect_test");
    // The click starts the test; the server can answer within the frames
    // the click runs (it does on Linux), so it may have finished already.
    let c = d.app.ai_agents.connect.as_ref().unwrap();
    assert!(
        c.testing() || c.tested.is_some(),
        "the click started a test"
    );
    let start = std::time::Instant::now();
    while d.app.ai_agents.connect.as_ref().unwrap().testing() {
        assert!(
            start.elapsed() < std::time::Duration::from_secs(30),
            "the test finishes"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
        d.frames(1);
    }
    let expected = openxer_automation::mcp::tool_definitions().len();
    assert_eq!(message(&d), format!("Working: {expected} tools"));

    // Connecting runs the test by itself: the row says it works, and the
    // next step stays in the message.
    let env = d.app.agent_env();
    fs::create_dir_all(claude_file(&env).parent().unwrap()).unwrap();
    d.app.refresh_connect();
    d.frames(1);
    d.click(&row_key(Client::ClaudeDesktop, "connect"));
    let start = std::time::Instant::now();
    while d.app.ai_agents.connect.as_ref().unwrap().testing() {
        assert!(start.elapsed() < std::time::Duration::from_secs(30));
        std::thread::sleep(std::time::Duration::from_millis(50));
        d.frames(1);
    }
    d.frames(1);
    assert!(message(&d).contains("tray icon"), "{}", message(&d));
    assert!(text_shown(&mut d, &format!("Working: {expected} tools")));

    // A missing executable is a clear error, and the button is disabled.
    d.app.ai_agents.env = Some(Env {
        cli: Some(dir.path().join("gone").join(connect::CLI_NAME)),
        ..profile(dir.path(), None)
    });
    d.app.start_agent_test();
    assert!(message(&d).contains("was not found"), "{}", message(&d));
}

// ------------------------------------------------------------- renders

fn out_dir() -> PathBuf {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/ai-agents");
    fs::create_dir_all(&out).unwrap();
    out
}

/// The first start's sample note is the message showing when the hint
/// matters most. On a 1366 px wide window the hint fits beside it with each
/// of its wordings (the hint only draws when the whole message ends left of
/// it), and it ends inside the window.
#[test]
fn hint_fits_beside_the_first_run_note() {
    for (name, claude, codex, words) in [
        (
            "ai-hint-note-claude",
            true,
            false,
            "Connect Claude Desktop to OpenXER",
        ),
        (
            "ai-hint-note-codex",
            false,
            true,
            "Connect ChatGPT or Codex to OpenXER",
        ),
        (
            "ai-hint-note-both",
            true,
            true,
            "Connect your AI apps to OpenXER",
        ),
    ] {
        let dir = Tmp::new();
        let env = profile(dir.path(), None);
        let mut d = Driver::probed(name, openxer_xer::starter(), Vec2::new(1366., 768.), None);
        d.app.ai_agents.env = Some(env.clone());
        if claude {
            fs::create_dir_all(claude_file(&env).parent().unwrap()).unwrap();
        }
        if codex {
            fs::create_dir_all(codex_file(&env).parent().unwrap()).unwrap();
        }
        d.frames(2);
        assert_eq!(
            d.app.status,
            crate::first_run::SAMPLE_NOTE,
            "the first start shows the sample note"
        );
        d.app.load_agent_hint();
        assert_eq!(d.app.ai_agents.hint, Hint::Shown);
        assert_eq!(d.app.ai_agents.hint_text, words);
        d.frames(2);
        assert!(
            d.has("status_ai_hint") && d.has("status_ai_hint_dismiss"),
            "{words:?} beside the sample note"
        );
        assert!(d.rect("status_ai_hint_dismiss").right() <= 1366.);
        assert_eq!(d.app.status, crate::first_run::SAMPLE_NOTE);
    }
}

#[test]
fn headless_render_ai_agents_menu_dialogs_and_hint() {
    let dir = Tmp::new();
    // The real server when it is built (the rows then say "Working"), else
    // a stand-in file so the buttons are live.
    let cli = real_cli().unwrap_or_else(|| {
        let p = dir
            .path()
            .join("Program Files")
            .join("OpenXER")
            .join(connect::CLI_NAME);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, b"stand-in").unwrap();
        p
    });
    let env = profile(dir.path(), Some(cli));
    // Claude Desktop installed, nothing connected: the hint names it.
    fs::create_dir_all(claude_file(&env).parent().unwrap()).unwrap();
    let (mut app, ctx) =
        crate::reports::test_support::app(&crate::reports::test_support::temp("ai-agents-render"));
    app.ai_agents.env = Some(env.clone());
    app.load_agent_hint();
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let out = out_dir();
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    let h = image.height();
    image::imageops::crop_imm(&image, 0, h - 60, image.width(), 60)
        .to_image()
        .save(out.join("status-bar-hint.png"))
        .unwrap();
    painter.click_action(&mut app, &ctx, &mut time, "menu_bar_ai_agents");
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image::imageops::crop_imm(&image, 0, 0, 900, 200)
        .to_image()
        .save(out.join("menu.png"))
        .unwrap();
    painter.click_action(&mut app, &ctx, &mut time, "menu_ai_connect");
    let mut shot = |app: &mut crate::App, name: &str, salt: &str| {
        // Let a running connection test finish first.
        let start = std::time::Instant::now();
        while app.ai_agents.connect.as_ref().is_some_and(|d| d.testing())
            && start.elapsed() < std::time::Duration::from_secs(30)
        {
            std::thread::sleep(std::time::Duration::from_millis(50));
            let _ = painter.frame(app, &ctx, &mut time, vec![], false);
        }
        let _ = painter.frame(app, &ctx, &mut time, vec![], false);
        let (image, _) = painter.frame(app, &ctx, &mut time, vec![], true).unwrap();
        crate::reports::test_support::crop_dialog(&image, &ctx, salt)
            .save(out.join(name))
            .unwrap();
    };
    shot(&mut app, "connect-one-found.png", "ai_connect");
    // Both apps found: Connect All is the default.
    fs::create_dir_all(codex_file(&env).parent().unwrap()).unwrap();
    app.refresh_connect();
    shot(&mut app, "connect-two-found.png", "ai_connect");
    app.agent_connect_all();
    shot(&mut app, "connect-after-connect-all.png", "ai_connect");
    // Claude Desktop on another copy, ChatGPT/Codex broken, Claude Code on
    // PATH.
    mixed(&env);
    let other = other_copy(&env, true);
    fs::write(
        claude_file(&env),
        serde_json::json!({"mcpServers":{"openxer":{"command":other.display().to_string(),"args":["mcp"]}}})
            .to_string(),
    )
    .unwrap();
    app.refresh_connect();
    if let Some(d) = &mut app.ai_agents.connect {
        d.message.clear();
    }
    shot(&mut app, "connect-other-copy-repair.png", "ai_connect");
    fs::write(claude_file(&env), "{ broken").unwrap();
    app.refresh_connect();
    app.agent_apply(Client::Codex, false);
    shot(&mut app, "connect-malformed-after-repair.png", "ai_connect");
    app.ai_agents.connect = None;
    app.ai_agents.about = true;
    shot(&mut app, "what-can-agents-do.png", "ai_about");
    app.ai_agents.about = false;
    app.open_connection_status();
    let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("connection-status.png")).unwrap();
}
