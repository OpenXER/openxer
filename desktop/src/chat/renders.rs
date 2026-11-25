//! Pictures of the Chat window over the README's synthetic sample schedule,
//! in the states of the approved mockup and the others it can be in.
//! `cargo test -p openxer-desktop --bin OpenXER chat_renders -- --ignored`
//! writes each as a full screen and as the window alone to the ignored
//! `artifacts/chat/`.
use super::backend::*;
use super::model::*;
use super::*;
use crate::readme_hero::{write_png, Shot};
use crate::DetailTab;
use std::sync::{Arc, Mutex};

fn out_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/chat")
}

/// The hero picture's screen with Chat open at the mockup's place.
fn shot(name: &str, at: Pos2) -> (Shot, Arc<Mutex<FakeLog>>) {
    let mut s = Shot::new(name);
    s.bottom(crate::panes::BottomView::Details, 240.);
    s.app.detail.tab = DetailTab::Relationships;
    s.fold(&["Electrical", "Structural steel"]);
    s.select("PIP-3110");
    let (fake, log) = FakeBackend::new();
    s.app.chat.backend = Box::new(fake);
    kit::set_geometry(
        &s.ctx,
        window_id(),
        kit::Geometry {
            pos: Some([at.x, at.y]),
            client: None,
            rolled: false,
        },
    );
    s.app.chat.restore = None;
    s.app.chat.open = true;
    s.frames(3);
    (s, log)
}

const MOCKUP_AT: Pos2 = Pos2::new(1196., 142.);

/// Writes the screen and the window alone.
fn save(s: &mut Shot, name: &str) {
    let picture = s.picture(name);
    let dir = out_dir();
    write_png(&picture, &dir.join(format!("{name}.png")));
    let close = s.app.action_rects["chat_title_close"];
    let client = kit::noted_client(&s.ctx, SALT);
    let left = close[2] - CLIENT.x - 2.;
    let top = close[1] - 1.;
    let bottom = client.map_or(close[3] + 1., |c| c.bottom() + 1.);
    let w = (close[2] + 1. - left).round() as u32;
    let h = (bottom - top).round().max(1.) as u32;
    let crop = image::imageops::crop_imm(
        &picture,
        left.max(0.).round() as u32,
        top.max(0.).round() as u32,
        w,
        h,
    )
    .to_image();
    write_png(&crop, &dir.join(format!("{name}-window.png")));
}

fn ev_delta(s: &str) -> AgentEvent {
    AgentEvent::TextDelta(s.into())
}

/// The mockup's conversation: a question answered with tool activity and
/// linked Activity IDs, then an edit waiting to be applied.
fn mid_conversation(c: &mut Conversation) {
    c.send("Why does PIP-3110 have zero float?");
    for e in [
        AgentEvent::ToolStarted {
            call: "1".into(),
            tool: "inspect".into(),
            target: "UNIT4-EXP".into(),
        },
        AgentEvent::ToolFinished {
            call: "1".into(),
            ok: true,
            millis: 300,
        },
        AgentEvent::ToolStarted {
            call: "2".into(),
            tool: "trace logic".into(),
            target: "PIP-3110".into(),
        },
        AgentEvent::ToolFinished {
            call: "2".into(),
            ok: true,
            millis: 100,
        },
        AgentEvent::Message(
            "PIP-3110 Erect pipe rack piping - Area 2 is on the longest path. It finishes 07-Apr-26 17:00 and drives INS-5020 Install field instruments - Area 2 (FS, 0d lag), which starts the next morning.\n\nFrom there the chain runs through instrument wiring and loop checks to the 09-Jun-26 finish with no slack. Its other successor, PIP-3170, has 13d of float."
                .into(),
        ),
        AgentEvent::TurnFinished {
            usage: Some(Usage {
                input: 1180,
                output: 142,
            }),
        },
    ] {
        c.apply(e);
    }
    c.send("Mark it 80% complete as of the data date");
    for e in [
        AgentEvent::ToolStarted {
            call: "3".into(),
            tool: "set progress".into(),
            target: "PIP-3110".into(),
        },
        AgentEvent::Proposed {
            proposal: 1,
            rows: vec![
                ChangeRow {
                    activity: "PIP-3110".into(),
                    field: "Remaining Duration".into(),
                    old: "7d".into(),
                    new: "3d".into(),
                },
                ChangeRow {
                    activity: "PIP-3110".into(),
                    field: "Duration % Complete".into(),
                    old: "53%".into(),
                    new: "80%".into(),
                },
            ],
        },
        AgentEvent::TurnFinished { usage: None },
    ] {
        c.apply(e);
    }
}

#[test]
#[ignore = "writes artifacts/chat; run explicitly"]
fn chat_renders() {
    // Main: mid-conversation, an edit waiting.
    let (mut s, _log) = shot("chat-main", MOCKUP_AT);
    s.app.chat.mode = Mode::Edit;
    mid_conversation(&mut s.app.chat.conv);
    s.frames(4);
    save(&mut s, "chat-main");

    // Empty: a new chat with the starter questions.
    let (mut s, _log) = shot("chat-empty", MOCKUP_AT);
    save(&mut s, "chat-empty");

    // Rolled up to its title bar, with the change waiting.
    let (mut s, _log) = shot("chat-rolled", Pos2::new(1196., 590.));
    mid_conversation(&mut s.app.chat.conv);
    let mut g = kit::geometry(&s.ctx, window_id());
    g.rolled = true;
    kit::set_geometry(&s.ctx, window_id(), g);
    save(&mut s, "chat-rolled");

    // A reply streaming: Stop in Send's place, half-finished markup.
    let (mut s, _log) = shot("chat-streaming", MOCKUP_AT);
    let c = &mut s.app.chat.conv;
    c.send("What is late against the baseline?");
    c.apply(ev_delta(
        "Three activities are behind the baseline:\n\n| Activity | Finish variance |\n|---|---|\n| PIP-3110 | -3d |\n| PIP-3120 | -2d |\n| ELE-4020 | -2d |\n\n**PIP-3110** is the one that moves the fi",
    ));
    save(&mut s, "chat-streaming");

    // The usage limit and an error, as status lines.
    let (mut s, _log) = shot("chat-error", MOCKUP_AT);
    let c = &mut s.app.chat.conv;
    c.send("Level the schedule and export it to Excel for the team.");
    c.apply(ev_delta("Leveling the pipefitters first"));
    c.apply(AgentEvent::Error {
        kind: ErrorKind::UsageLimit,
        message: "Your plan's limit resets at 3:00 PM.".into(),
    });
    c.send("Summarize this week's progress for the team.");
    c.apply(AgentEvent::Error {
        kind: ErrorKind::Failed,
        message: "the agent stopped responding.".into(),
    });
    s.frames(2);
    c_stop_line(&mut s);
    save(&mut s, "chat-error");

    // No project open.
    let (mut s, _log) = shot("chat-no-project", MOCKUP_AT);
    s.app.project.model.project.name.clear();
    save(&mut s, "chat-no-project");

    // ChatGPT: sign-in needed, the consent box, signing in, signed in.
    let (mut s, log) = shot("chat-signin", MOCKUP_AT);
    s.app.chat.agent = Some("chatgpt".into());
    s.app.chat.picked = true;
    s.frames(3);
    save(&mut s, "chat-sign-in");
    s.click("chat_sign_in");
    save(&mut s, "chat-consent");
    s.click("chat_consent_continue with chatgpt");
    save(&mut s, "chat-signing-in");
    log.lock()
        .unwrap()
        .finish_sign_in("chatgpt", "scheduler@example.com");
    s.frames(3);
    // OpenAI's welcome box, once, after the first sign-in.
    save(&mut s, "chat-plan-welcome");
    s.click("chat_welcome_got it");
    s.frames(2);
    save(&mut s, "chat-signed-in");
    s.click("chat_options");
    save(&mut s, "chat-options");

    // ChatGPT answering: the plan label and Manage usage by the composer,
    // then the usage limit with Manage usage as its action.
    let (mut s, log) = shot("chat-plan", MOCKUP_AT);
    log.lock()
        .unwrap()
        .finish_sign_in("chatgpt", "scheduler@example.com");
    s.app.chat.agent = Some("chatgpt".into());
    s.app.chat.picked = true;
    s.app.chat.welcomed = Some(true);
    s.frames(3);
    let c = &mut s.app.chat.conv;
    c.send("Which activities are in progress?");
    for e in [
        AgentEvent::ToolStarted {
            call: "1".into(),
            tool: "read schedule".into(),
            target: "activities".into(),
        },
        AgentEvent::ToolFinished {
            call: "1".into(),
            ok: true,
            millis: 40,
        },
        AgentEvent::Message(
            "Three are in progress: PIP-3110, PIP-3120 and ELE-4020. PIP-3110 is on the longest path."
                .into(),
        ),
        AgentEvent::TurnFinished {
            usage: Some(Usage {
                input: 2410,
                output: 64,
            }),
        },
    ] {
        c.apply(e);
    }
    s.frames(3);
    save(&mut s, "chat-plan-label");
    let c = &mut s.app.chat.conv;
    c.send("And which of them are late?");
    c.apply(AgentEvent::Error {
        kind: ErrorKind::UsageLimit,
        message: openxer_chat::ui::USAGE_LIMIT_BODY.into(),
    });
    s.frames(3);
    save(&mut s, "chat-plan-usage-limit");

    // A fresh PC: nothing installed, ChatGPT signed out.
    let (mut s, log) = shot("chat-setup", MOCKUP_AT);
    {
        let mut l = log.lock().unwrap();
        l.agents.rotate_right(1);
        l.agents.insert(
            1,
            AgentInfo {
                id: super::live::endpoint::ENDPOINT.into(),
                name: super::live::endpoint::ENDPOINT_NAME.into(),
                state: AgentState::NotSetUp,
                privacy: String::new(),
                consent: None,
                can_sign_out: false,
                models: vec![],
                lower_cost: None,
                setup: super::live::endpoint::ENDPOINT_SETUP.into(),
                install_url: None,
                plan: false,
                notice: None,
                configurable: true,
                ask_only: false,
                use_consent: None,
            },
        );
        for a in &mut l.agents {
            match a.id.as_str() {
                "chatgpt" => a.setup = super::live::CHATGPT_SETUP.into(),
                "claude-code" => {
                    a.state = AgentState::NotInstalled;
                    a.setup = super::live::CLAUDE_CODE_SETUP.into();
                    a.install_url = Some(super::live::CLAUDE_CODE_URL.into());
                }
                "codex" => {
                    a.setup = super::live::CODEX_SETUP.into();
                    a.install_url = Some(super::live::CODEX_URL.into());
                }
                _ => {}
            }
        }
    }
    s.frames(3);
    save(&mut s, "chat-setup-fresh");
    // Mixed: signing in to ChatGPT, Claude Code installed but signed out
    // and old, Codex not installed.
    {
        let mut l = log.lock().unwrap();
        for a in &mut l.agents {
            match a.id.as_str() {
                "chatgpt" => a.state = AgentState::SigningIn,
                "claude-code" => {
                    a.state = AgentState::SignInNeeded;
                    a.install_url = None;
                    a.notice = Some(
                        "Claude Code 2.0.4 is older than the version OpenXER was tested with (2.1.330). It may not work; update Claude Code if it does not."
                            .into(),
                    );
                }
                _ => {}
            }
        }
    }
    s.frames(3);
    save(&mut s, "chat-setup-mixed");
    // Check Setup with mixed results.
    s.click("chat_check_setup");
    s.frames(3);
    s.click("chat_checks_list_row_5");
    s.frames(2);
    let picture = s.picture("chat-check-setup");
    write_png(&picture, &out_dir().join("chat-check-setup.png"));
    s.app.chat.check = None;
    s.frames(2);

    // Set Up... of the API key or local model: a key typed (masked).
    s.app.open_endpoint_dialog();
    if let Some(d) = s.app.chat.endpoint.as_mut() {
        let info = openxer_chat::compat::Preset::OpenAi.info();
        d.conn.preset = info.preset;
        d.conn.name = info.label.into();
        d.conn.base_url = info.base_url.into();
        d.conn.needs_key = info.needs_key;
        d.conn.model = "model-a".into();
        d.key = "sk-render-not-a-real-key".into();
        d.models = vec!["model-a".into(), "model-b".into()];
        d.result = Some("Connected. The server lists 2 model(s).".into());
    }
    s.frames(4);
    let picture = s.picture("chat-endpoint-setup");
    write_png(&picture, &out_dir().join("chat-endpoint-setup.png"));
    // A model on this PC: no key, Ask only.
    if let Some(d) = s.app.chat.endpoint.as_mut() {
        let info = openxer_chat::compat::Preset::Ollama.info();
        d.conn.preset = info.preset;
        d.conn.name = info.label.into();
        d.conn.base_url = info.base_url.into();
        d.conn.needs_key = info.needs_key;
        d.key.clear();
        d.result = None;
    }
    s.frames(4);
    let picture = s.picture("chat-endpoint-local");
    write_png(&picture, &out_dir().join("chat-endpoint-local.png"));
}

/// A stopped reply's divider under the error lines.
fn c_stop_line(s: &mut Shot) {
    let c = &mut s.app.chat.conv;
    c.send("Try again with fewer activities.");
    c.apply(ev_delta("Working on it"));
    c.stop();
}
