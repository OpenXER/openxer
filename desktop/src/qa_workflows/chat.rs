//! Workflow: ask the agent about the schedule in Chat, follow a linked
//! Activity ID to the table, let the agent propose an edit and apply it,
//! roll the window up while working in the table, and find it where it was
//! after a restart. Driven through the AI Agents menu, F8 and the window's
//! own controls with the scripted chat backend; the project itself must
//! come through unchanged (the window writes nothing on its own).
use super::*;
use crate::chat::backend::{AgentEvent, ChangeRow, FakeBackend, Mode, Usage};

fn shown(d: &mut Driver, needle: &str) -> bool {
    d.texts().iter().any(|(t, _)| t.contains(needle))
}

#[test]
fn workflow_chat_ask_follow_a_link_apply_a_proposal_and_come_back() {
    let mut d = fresh("chat-workflow");
    let (fake, log) = FakeBackend::new();
    d.app.chat.backend = Box::new(fake);
    let before = d.app.project.model.clone();
    let undo = d.app.history.counts().0;
    let last = d.app.project.model.activities.len() - 1;
    let code = d.app.project.model.activities[last].task_code.clone();

    // Open from the menu: Claude Code is ready, with starter questions.
    d.home();
    d.click("menu_bar_ai_agents");
    d.click(crate::chat::COMMAND);
    d.frames(3);
    assert!(d.app.chat.open);
    assert!(shown(&mut d, "Claude Code is ready."));

    // A starter question; the reply streams with tool rows and a link.
    let reply = format!(
        "**{code}** is the last activity. It finishes the project.\n\n- No successor\n- Zero float"
    );
    let mut batches = vec![
        vec![AgentEvent::Ready],
        vec![AgentEvent::ToolStarted {
            call: "1".into(),
            tool: "inspect".into(),
            target: d.app.project.model.project.name.clone(),
        }],
        vec![AgentEvent::ToolFinished {
            call: "1".into(),
            ok: true,
            millis: 250,
        }],
    ];
    for piece in reply.as_bytes().chunks(9) {
        batches.push(vec![AgentEvent::TextDelta(
            String::from_utf8(piece.to_vec()).unwrap(),
        )]);
    }
    batches.push(vec![AgentEvent::TurnFinished {
        usage: Some(Usage {
            input: 900,
            output: 60,
        }),
    }]);
    log.lock().unwrap().replies.push_back(batches);
    d.click("chat_starter_row_0");
    assert_eq!(
        log.lock().unwrap().sent[0].0,
        crate::ai_agents::THINGS_TO_ASK[0]
    );
    d.frames(30);
    assert!(!d.app.chat.conv.running);
    assert!(shown(&mut d, "is the last activity."));
    assert!(shown(&mut d, "0.3 s") || shown(&mut d, "0.2 s"));
    assert!(shown(&mut d, "960 tokens"));

    // Follow the link: the activity is selected in the table.
    d.click(&format!("chat_link_{code}"));
    assert_eq!(d.app.selected, Some(last));
    assert!(shown(&mut d, &format!("{code} selected")));

    // Edit mode: the agent proposes, the user applies once.
    d.click("chat_mode");
    d.click("chat_mode_item_1");
    log.lock().unwrap().replies.push_back(vec![vec![
        AgentEvent::ToolStarted {
            call: "2".into(),
            tool: "set progress".into(),
            target: code.clone(),
        },
        AgentEvent::Proposed {
            proposal: 5,
            rows: vec![ChangeRow {
                activity: code.clone(),
                field: "Remaining Duration".into(),
                old: "5d".into(),
                new: "2d".into(),
            }],
        },
        AgentEvent::TurnFinished { usage: None },
    ]]);
    d.click("chat_message");
    d.type_text("Cut its remaining duration to 2d");
    d.key(Key::Enter);
    d.frames(3);
    assert_eq!(log.lock().unwrap().sent[1].1, Mode::Edit);
    assert!(shown(
        &mut d,
        "1 change ready. Nothing is written until you apply it."
    ));

    // Roll it up and work in the table; the title says a change waits.
    d.click("chat_roll");
    assert!(shown(&mut d, "Chat - 1 change ready"));
    d.key(Key::ArrowUp);
    assert!(d.app.chat.open);
    d.click("chat_roll");
    d.click("chat_apply");
    assert_eq!(log.lock().unwrap().applied, vec![5]);
    assert!(shown(&mut d, "Applied."));

    // New Chat empties it; Escape closes it.
    d.click("chat_new");
    assert!(d.app.chat.conv.is_empty());
    d.key(Key::Escape);
    assert!(!d.app.chat.open);

    // The window wrote nothing: no undo step, the same project.
    assert_eq!(d.app.history.counts().0, undo);
    assert!(d.app.project.model == before);

    // After a restart, F8 opens it where it was, in Edit mode.
    let place = crate::dialog_kit::geometry(&d.ctx, crate::chat::window_id());
    d.frames(2);
    restart(&mut d);
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let store = Store::open(d.app.store.path.clone()).unwrap();
    let project = store.last().unwrap().unwrap();
    d.app = App::new(&ctx, store, project);
    d.ctx = ctx;
    let (fake, _log) = FakeBackend::new();
    d.app.chat.backend = Box::new(fake);
    d.frames(3);
    d.key(Key::F8);
    d.frames(3);
    assert!(d.app.chat.open);
    assert_eq!(d.app.chat.mode, Mode::Edit);
    assert_eq!(
        crate::dialog_kit::geometry(&d.ctx, crate::chat::window_id()).pos,
        place.pos
    );
    let saved = stored(&d, d.app.open_set.active.unwrap());
    assert!(as_saved(&saved.model) == as_saved(&before));
}

fn as_saved(m: &Model) -> Model {
    crate::qa_sweep::as_saved(m)
}

/// Workflow on the real backend over fakes (a scripted Codex installed by
/// npm, the recorded transcripts, nothing else): the Chat window from the
/// AI Agents menu asks Codex about the sample project, applies Codex's
/// proposal as one undo step, undoes and redoes it, and closing the window leaves no process and no files.
#[test]
fn workflow_chat_on_the_real_backend_asks_applies_and_cleans_up() {
    use crate::chat::live::fakes::{Behaviour, Rig, Shape};
    use crate::chat::live::{LiveBackend, CODEX};
    use openxer_automation::agent_host::Agent;
    let mut rig = Rig::bare("workflow codex");
    rig.profile.install(Agent::Codex, Shape::Npm);
    rig.agents.set(Agent::Codex, Behaviour::of(Agent::Codex));
    let mut p = openxer_xer::sample_project::sample_project().current;
    schedule_model(&mut p.model, &p.resources).unwrap();
    let mut d = Driver::new("chat-workflow-live", p);
    d.app.chat.opener = rig.opener.clone();
    d.app.chat.backend = Box::new(LiveBackend::new(rig.config()));
    let wait = |d: &mut Driver, what: &str, check: &dyn Fn(&Driver) -> bool| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while !check(d) {
            assert!(
                std::time::Instant::now() < deadline,
                "{what}: {:?} / {:?} / {}",
                d.app.chat.conv.items,
                d.app
                    .chat
                    .backend
                    .agents()
                    .iter()
                    .map(|a| (a.id.clone(), a.state.clone()))
                    .collect::<Vec<_>>(),
                d.app.status
            );
            d.frames(1);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    };
    d.home();
    d.click("menu_bar_ai_agents");
    d.click(crate::chat::COMMAND);
    d.frames(3);
    // Codex is the one ready route: the window goes straight to it.
    wait(&mut d, "Codex is ready", &|d| {
        d.app
            .chat
            .backend
            .agents()
            .iter()
            .any(|a| a.id == CODEX && a.state.usable())
    });
    d.frames(2);
    assert!(shown(&mut d, "Codex is ready."));
    d.click("chat_starter_row_0");
    wait(&mut d, "the answer", &|d| !d.app.chat.conv.running);
    assert!(shown(&mut d, "PM0920"));
    // Edit: Codex proposes renaming PM0910; Apply is one undo step.
    d.click("chat_mode");
    d.click("chat_mode_item_1");
    let undo = d.app.history.counts().0;
    let before = d.app.project.model.clone();
    d.click("chat_message");
    d.type_text("Rename PM0910 to Submit as-built drawings and O&M manuals");
    d.key(Key::Enter);
    wait(&mut d, "the proposal", &|d| {
        d.app.chat.conv.pending().is_some() && !d.app.chat.conv.running
    });
    d.click("chat_apply");
    d.frames(2);
    let name = |d: &Driver| {
        let a = &d.app.project.model.activities[d
            .app
            .project
            .model
            .activities
            .iter()
            .position(|a| a.task_code == "PM0910")
            .unwrap()];
        d.app.project.details[&a.task_id].name.clone()
    };
    assert_eq!(name(&d), "Submit as-built drawings and O&M manuals");
    assert_eq!(d.app.history.counts().0, undo + 1);
    d.click("undo");
    d.frames(2);
    assert!(d.app.project.model == before);
    d.click("redo");
    d.frames(2);
    assert_eq!(name(&d), "Submit as-built drawings and O&M manuals");
    // Closing the window ends it all.
    d.click("chat_title_close");
    assert!(!d.app.chat.open);
    wait(&mut d, "no process left", &|_| rig.agents.alive() == 0);
    wait(&mut d, "no work folder left", &|_| {
        rig.work_dirs().is_empty()
    });
}
