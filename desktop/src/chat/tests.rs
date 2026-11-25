//! Chat: the view model on its own, then the whole window through the
//! production UI (`App::update_ui` with synthetic input, the scripted
//! [`FakeBackend`]), from the AI Agents menu and F8.
use super::backend::*;
use super::model::*;
use super::*;
use crate::qa_sweep::Driver;
use eframe::egui::Rect;
use model::Item;
use std::sync::{Arc, Mutex};

// ------------------------------------------------------------ view model

fn delta(s: &str) -> AgentEvent {
    AgentEvent::TextDelta(s.into())
}

fn started(call: &str, tool: &str, target: &str) -> AgentEvent {
    AgentEvent::ToolStarted {
        call: call.into(),
        tool: tool.into(),
        target: target.into(),
    }
}

fn finished(call: &str, ok: bool, millis: u64) -> AgentEvent {
    AgentEvent::ToolFinished {
        call: call.into(),
        ok,
        millis,
    }
}

fn change(id: &str, field: &str, old: &str, new: &str) -> ChangeRow {
    ChangeRow {
        activity: id.into(),
        field: field.into(),
        old: old.into(),
        new: new.into(),
    }
}

fn done(usage: Option<Usage>) -> AgentEvent {
    AgentEvent::TurnFinished { usage }
}

#[test]
fn every_event_kind_lands_in_order() {
    let mut c = Conversation::default();
    // Nothing counts before a message is sent.
    c.apply(delta("stray"));
    assert!(c.is_empty());
    assert!(!c.send("   "), "blank messages are not sent");
    assert!(c.send(" Why zero float? "));
    assert!(!c.send("again"), "one message at a time");
    for e in [
        AgentEvent::Ready,
        started("1", "inspect", "UNIT4-EXP"),
        started("2", "trace logic", "PIP-3110"),
        finished("1", true, 300),
        finished("2", false, 100),
        delta("PIP-3110 is "),
        delta("on the path"),
        AgentEvent::Message("PIP-3110 is on the longest path.".into()),
        delta("More."),
        done(Some(Usage {
            input: 1200,
            output: 80,
        })),
    ] {
        c.apply(e);
    }
    assert_eq!(c.items[0], Item::User("Why zero float?".into()));
    let Item::Tools(rows) = &c.items[1] else {
        panic!("tools block: {:?}", c.items[1])
    };
    assert_eq!(rows.len(), 2, "tools in a row share one block");
    assert_eq!(
        (rows[0].status, rows[0].millis),
        (ToolStatus::Done, Some(300))
    );
    assert_eq!(rows[1].status, ToolStatus::Failed);
    assert_eq!(
        c.items[2],
        Item::Reply {
            text: "PIP-3110 is on the longest path.".into(),
            streaming: false
        },
        "a whole message replaces the streamed text"
    );
    assert_eq!(
        c.items[3],
        Item::Reply {
            text: "More.".into(),
            streaming: false
        }
    );
    assert!(!c.running);
    assert_eq!(c.usage.map(|u| u.input + u.output), Some(1280));
    // After the reply ends, events wait for the next message.
    c.apply(delta("late"));
    assert_eq!(c.items.len(), 4);
}

#[test]
fn streamed_text_is_assembled_piece_by_piece() {
    let mut c = Conversation::default();
    c.send("go");
    let text = "**Bold** start, then a list:\n- one\n- two\n";
    for ch in text.chars() {
        c.apply(delta(&ch.to_string()));
        assert!(c.running);
    }
    assert_eq!(
        c.items[1],
        Item::Reply {
            text: text.into(),
            streaming: true
        }
    );
    // A tool between pieces ends the reply; text after it starts a new one.
    c.apply(started("t", "read", "x"));
    c.apply(delta("after"));
    assert_eq!(c.items.len(), 4);
    assert!(matches!(
        &c.items[1],
        Item::Reply {
            streaming: false,
            ..
        }
    ));
    assert!(matches!(
        &c.items[3],
        Item::Reply {
            streaming: true,
            ..
        }
    ));
    assert_eq!(c.status().as_deref(), Some("answering..."));
}

#[test]
fn stop_ends_the_reply_and_drops_what_follows() {
    let mut c = Conversation::default();
    c.send("go");
    c.apply(started("t", "schedule", "UNIT4"));
    c.apply(delta("Half"));
    assert!(c.stop());
    assert!(!c.stop(), "nothing left to stop");
    assert!(!c.running);
    let Item::Tools(rows) = &c.items[1] else {
        panic!()
    };
    assert_eq!(rows[0].status, ToolStatus::Stopped);
    assert_eq!(
        c.items.last(),
        Some(&Item::Status {
            kind: StatusKind::Note,
            text: "Stopped".into()
        })
    );
    let n = c.items.len();
    c.apply(delta(" more"));
    c.apply(done(None));
    assert_eq!(c.items.len(), n, "the stopped reply's rest is dropped");
    assert!(c.send("next"), "a new message can follow");
}

#[test]
fn errors_end_the_turn_with_a_status_line_and_no_retry() {
    for (kind, words) in [
        (ErrorKind::Failed, "Error: it broke"),
        (
            ErrorKind::UsageLimit,
            "Usage limit reached. it broke Nothing was retried.",
        ),
        (ErrorKind::NotSignedIn, "Not signed in. it broke"),
    ] {
        let mut c = Conversation::default();
        c.send("go");
        c.apply(delta("partial"));
        c.apply(AgentEvent::Error {
            kind,
            message: "it broke".into(),
        });
        assert!(!c.running);
        let Some(Item::Status { text, kind: k }) = c.items.last() else {
            panic!()
        };
        assert!(text.starts_with(words), "{text}");
        assert_eq!(*k == StatusKind::Limit, kind == ErrorKind::UsageLimit);
        assert!(matches!(
            &c.items[1],
            Item::Reply {
                streaming: false,
                ..
            }
        ));
        c.apply(delta("retry"));
        assert_eq!(c.items.len(), 3, "nothing more arrives after the error");
    }
    let mut c = Conversation::default();
    c.send("go");
    c.apply(AgentEvent::Error {
        kind: ErrorKind::UsageLimit,
        message: String::new(),
    });
    assert_eq!(c.status().as_deref(), Some("usage limit reached"));
}

#[test]
fn proposals_wait_hold_their_tools_and_replace_each_other() {
    let mut c = Conversation::default();
    c.send("mark it");
    c.apply(started("p", "set progress", "PIP-3110"));
    c.apply(AgentEvent::Proposed {
        proposal: 7,
        rows: vec![change("PIP-3110", "Remaining Duration", "7d", "3d")],
    });
    let Item::Tools(rows) = &c.items[1] else {
        panic!()
    };
    assert_eq!(rows[0].status, ToolStatus::Waiting);
    assert_eq!(c.pending().map(|(id, r)| (id, r.len())), Some((7, 1)));
    assert_eq!(c.status().as_deref(), Some("1 change ready"));
    c.apply(AgentEvent::Proposed {
        proposal: 8,
        rows: vec![
            change("PIP-3110", "Remaining Duration", "7d", "3d"),
            change("PIP-3110", "Duration % Complete", "53%", "80%"),
        ],
    });
    assert!(matches!(
        &c.items[2],
        Item::Proposal {
            state: ProposalState::Replaced,
            ..
        }
    ));
    assert_eq!(c.status().as_deref(), Some("2 changes ready"));
    c.resolve(8, ProposalState::Applied);
    c.resolve(8, ProposalState::Discarded);
    assert!(matches!(
        &c.items[3],
        Item::Proposal {
            state: ProposalState::Applied,
            ..
        }
    ));
    assert!(c.pending().is_none());
    assert_eq!(
        pending_line(1),
        "1 change ready. Nothing is written until you apply it."
    );
    assert!(pending_line(3).starts_with("3 changes ready."));
}

#[test]
fn new_chat_starts_empty() {
    let mut c = Conversation::default();
    c.send("go");
    c.apply(delta("x"));
    c.reset();
    assert_eq!(c, Conversation::default());
    c.apply(delta("y"));
    assert!(c.is_empty());
}

#[test]
fn the_context_line_and_numbers_read_plainly() {
    let mut ctx = ChatContext {
        project_id: "UNIT4-EXP".into(),
        selection: Selection::One("PIP-3110".into()),
        data_date: "30-Mar-26".into(),
        ..Default::default()
    };
    assert_eq!(
        context_line(&ctx),
        "UNIT4-EXP \u{b7} PIP-3110 selected \u{b7} Data Date 30-Mar-26"
    );
    ctx.selection = Selection::Many(vec!["A".into(), "B".into(), "C".into()]);
    assert!(context_line(&ctx).contains("3 activities selected"));
    ctx.selection = Selection::None;
    assert!(context_line(&ctx).contains("No activity selected"));
    assert_eq!(duration(300), "0.3 s");
    assert_eq!(duration(75_000), "1:15 min");
    assert_eq!(
        (tokens(950), tokens(1234), tokens(36_400)),
        ("950".into(), "1.2k".into(), "36k".into())
    );
}

// ------------------------------------------------------- through the UI

type Log = Arc<Mutex<FakeLog>>;

/// The scheduled starter project with the scripted backend.
pub(crate) fn driver(name: &str) -> (Driver, Log) {
    let mut p = openxer_xer::starter();
    crate::scheduling::schedule_model(&mut p.model, &p.resources).unwrap();
    let mut d = Driver::new(name, p);
    let (fake, log) = FakeBackend::new();
    d.app.chat.backend = Box::new(fake);
    d.frames(2);
    (d, log)
}

/// AI Agents > Chat.
pub(crate) fn open_chat(d: &mut Driver) {
    d.home();
    d.click("menu_bar_ai_agents");
    d.click(COMMAND);
    d.frames(3);
    assert!(d.app.chat.open, "the menu command opens Chat");
}

/// What the agent does for the next message: one batch of events per frame.
fn script(log: &Log, batches: Vec<Vec<AgentEvent>>) {
    log.lock().unwrap().replies.push_back(batches);
}

fn shown(d: &mut Driver, needle: &str) -> bool {
    d.texts().iter().any(|(t, _)| t.contains(needle))
}

fn shown_rect(d: &mut Driver, needle: &str) -> Option<Rect> {
    d.texts()
        .into_iter()
        .rev()
        .find(|(t, _)| t.contains(needle))
        .map(|(_, r)| r)
}

/// Types into the composer and presses Enter.
fn ask(d: &mut Driver, text: &str) {
    d.click("chat_message");
    d.type_text(text);
    d.frames(1);
    d.key(Key::Enter);
    d.frames(2);
}

fn window_rect(d: &Driver) -> Rect {
    let close = d.rect("chat_title_close");
    let client = kit::noted_client(&d.ctx, SALT).unwrap();
    Rect::from_min_max(
        Pos2::new(client.left() - 1., close.top() - 1.),
        Pos2::new(close.right() + 1., client.bottom() + 1.),
    )
}

#[test]
fn chat_opens_from_the_menu_and_f8_and_escape_closes_it() {
    let (mut d, _log) = driver("chat-open");
    open_chat(&mut d);
    assert!(d.has("chat_title_close") && d.has("chat_roll"));
    let client = kit::noted_client(&d.ctx, SALT).expect("the window notes its client area");
    assert_eq!(client.size(), CLIENT);
    assert_eq!(
        client.size() + Vec2::new(2., kit::TITLE_H + 2.),
        Vec2::new(364., 486.),
        "a 364 x 486 window"
    );
    assert!(shown(&mut d, "Claude Code is ready."));
    d.key(Key::Escape);
    assert!(!d.app.chat.open, "Escape closes it");
    d.key(Key::F8);
    d.frames(2);
    assert!(d.app.chat.open, "F8 opens it");
    d.key(Key::F8);
    d.frames(2);
    assert!(!d.app.chat.open, "F8 closes it again");
    // The menu shows the shortcut and a check mark while open.
    d.click("menu_bar_ai_agents");
    assert!(shown(&mut d, "F8"));
    d.key(Key::Escape);
    // F8 waits while another dialog is open.
    d.app.ai_agents.about = true;
    d.frames(3);
    d.key(Key::F8);
    assert!(!d.app.chat.open, "F8 does nothing over another dialog");
    d.app.ai_agents.about = false;
    d.frames(2);
    // The close box closes it as well.
    d.key(Key::F8);
    d.frames(2);
    d.click("chat_title_close");
    assert!(!d.app.chat.open);
}

#[test]
fn enter_sends_shift_enter_starts_a_line_and_the_context_goes_along() {
    let (mut d, log) = driver("chat-send");
    let second = d.app.project.model.activities[1].task_code.clone();
    d.app.select_activities(&[1]);
    open_chat(&mut d);
    // The composer has the keyboard as the window opens.
    d.type_text("Why is it");
    d.key_with(Key::Enter, egui::Modifiers::SHIFT);
    d.type_text("late?");
    d.frames(1);
    assert_eq!(d.app.chat.draft, "Why is it\nlate?");
    assert!(
        log.lock().unwrap().sent.is_empty(),
        "Shift+Enter does not send"
    );
    assert!(
        shown(&mut d, &format!("{second} selected")),
        "the context line"
    );
    d.key(Key::Enter);
    d.frames(2);
    let sent = log.lock().unwrap().sent.clone();
    assert_eq!(sent.len(), 1, "Enter sends once");
    let (text, mode, context, model) = &sent[0];
    assert_eq!(text, "Why is it\nlate?");
    assert_eq!(*mode, Mode::Ask);
    assert_eq!(*model, ModelChoice::AgentDefault);
    assert_eq!(context.project_id, d.app.project.model.project.name);
    assert_eq!(context.selection, Selection::One(second.clone()));
    assert!(!context.data_date.is_empty());
    assert!(d.app.chat.draft.is_empty(), "the composer empties");
    assert!(shown(&mut d, "late?"), "the message shows in the list");
    // Several selected: the context says how many.
    d.app.select_activities(&[0, 1, 2]);
    d.frames(2);
    assert!(shown(&mut d, "3 activities selected"));
}

#[test]
fn a_starter_question_sends_itself() {
    let (mut d, log) = driver("chat-starter");
    open_chat(&mut d);
    let starters = d.app.chat_starters();
    assert_eq!(starters.len(), 5);
    assert_eq!(starters[0], crate::ai_agents::THINGS_TO_ASK[0]);
    assert!(starters[1].contains(&d.app.project.model.activities[0].task_code));
    assert!(shown(&mut d, "Try one of these:"));
    let row = d.rect("chat_starter_row_2");
    assert_eq!(row.height(), 28., "22 px rows, 6 px apart");
    d.click("chat_starter_row_2");
    assert_eq!(log.lock().unwrap().sent[0].0, starters[2]);
    assert!(
        !d.has("chat_starter_row_0"),
        "the starters give way to the chat"
    );
}

#[test]
fn a_streaming_reply_shows_and_the_list_stays_on_the_newest_line() {
    let (mut d, log) = driver("chat-stream");
    open_chat(&mut d);
    let mut batches: Vec<Vec<AgentEvent>> = (1..=80)
        .map(|n| vec![delta(&format!("Line {n} of the reply.\n\n"))])
        .collect();
    batches.push(vec![done(Some(Usage {
        input: 3100,
        output: 420,
    }))]);
    script(&log, batches);
    ask(&mut d, "Tell me everything");
    assert!(d.app.chat.conv.running);
    assert!(d.has("chat_stop"), "Send becomes Stop while a reply runs");
    assert!(!d.has("chat_send"));
    d.frames(30);
    assert!(d.app.chat.conv.running, "still streaming");
    let list = d.rect("chat_list");
    let line = |d: &mut Driver, n: usize| shown_rect(d, &format!("Line {n} of"));
    let newest = d.app.chat.conv.items.len();
    assert!(newest >= 2);
    let latest = (1..=80)
        .rev()
        .find(|n| {
            d.app.chat.conv.items.iter().any(
                |i| matches!(i, Item::Reply { text, .. } if text.contains(&format!("Line {n} of"))),
            )
        })
        .unwrap();
    let r = line(&mut d, latest).expect("the newest line is drawn");
    assert!(
        list.contains_rect(r),
        "the newest line is in view: {r:?} in {list:?}"
    );
    assert!(
        line(&mut d, 1).is_none_or(|r| !list.contains_rect(r)),
        "the first scrolled away"
    );
    // The user scrolls up: the list stays where they put it.
    let _ = d.frame(vec![
        egui::Event::PointerMoved(list.center()),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0., 4000.),
            modifiers: Default::default(),
        },
    ]);
    d.frames(60);
    assert!(!d.app.chat.conv.running);
    let first = line(&mut d, 1).expect("scrolled back to the start");
    assert!(list.contains_rect(first), "the first line stays in view");
    assert!(line(&mut d, 80).is_none_or(|r| !list.contains_rect(r)));
    assert!(d.has("chat_send"), "Stop is Send again");
    assert!(shown(&mut d, "3.5k tokens"), "the last reply's tokens");
}

#[test]
fn stop_ends_the_reply_through_the_backend() {
    let (mut d, log) = driver("chat-stop");
    open_chat(&mut d);
    let batches = (0..50).map(|n| vec![delta(&format!("w{n} "))]).collect();
    script(&log, batches);
    ask(&mut d, "go");
    d.click("chat_stop");
    assert_eq!(log.lock().unwrap().stops, 1);
    assert!(!d.app.chat.conv.running);
    assert!(shown(&mut d, "Stopped"));
    assert!(d.has("chat_send"));
}

#[test]
fn tool_activity_shows_as_eighteen_pixel_rows() {
    let (mut d, log) = driver("chat-tools");
    open_chat(&mut d);
    script(
        &log,
        vec![
            vec![started("a", "inspect", "UNIT4-EXP")],
            vec![started("b", "trace logic", "PIP-3110")],
            vec![finished("a", true, 300)],
            vec![finished("b", false, 1200), done(None)],
        ],
    );
    ask(&mut d, "go");
    d.frames(6);
    let row = d.rect("chat_tools_1_row_0");
    assert_eq!(row.height(), 18.);
    assert_eq!(d.rect("chat_tools_1_row_1").top(), row.bottom());
    for t in [
        "inspect",
        "UNIT4-EXP",
        "Done",
        "0.3 s",
        "trace logic",
        "Failed",
        "1.2 s",
    ] {
        assert!(shown(&mut d, t), "{t}");
    }
}

#[test]
fn an_activity_id_in_a_reply_selects_the_activity_in_the_table() {
    let (mut d, log) = driver("chat-link");
    let target = d.app.project.model.activities.len() - 1;
    let code = d.app.project.model.activities[target].task_code.clone();
    script(
        &log,
        vec![vec![
            AgentEvent::Message(format!("**{code}** drives the finish ({code}).")),
            done(None),
        ]],
    );
    open_chat(&mut d);
    ask(&mut d, "what drives it?");
    d.frames(2);
    assert_ne!(d.app.selected, Some(target));
    d.click(&format!("chat_link_{code}"));
    assert_eq!(d.app.selected, Some(target), "the link selects it");
    assert!(d.app.chat.open, "Chat stays open");
    d.frames(3);
    let rows = d.app.rows();
    let k = rows
        .iter()
        .position(|r| *r == crate::grid::Row::Activity(target))
        .unwrap();
    let (top, _) = d.app.grid_row_tops(&rows)[k];
    assert!(
        top > 0. && top < 900.,
        "the row is scrolled into view at {top}"
    );
}

#[test]
fn apply_and_discard_answer_a_proposal_once() {
    let (mut d, log) = driver("chat-proposal");
    open_chat(&mut d);
    d.click("chat_mode");
    d.click("chat_mode_item_1");
    assert_eq!(d.app.chat.mode, Mode::Edit);
    let proposal = |id| {
        vec![vec![
            started("p", "set progress", "RB-1010"),
            AgentEvent::Proposed {
                proposal: id,
                rows: vec![
                    change("RB-1010", "Remaining Duration", "7d", "3d"),
                    change("RB-1010", "Duration % Complete", "53%", "80%"),
                ],
            },
            done(None),
        ]]
    };
    script(&log, proposal(11));
    let undo = d.app.history.counts().0;
    let model = d.app.project.model.clone();
    ask(&mut d, "mark it");
    assert_eq!(
        log.lock().unwrap().sent[0].1,
        Mode::Edit,
        "the mode goes along"
    );
    assert!(shown(
        &mut d,
        "2 changes ready. Nothing is written until you apply them."
    ));
    assert!(shown(&mut d, "53% \u{2192} 80%"));
    assert!(shown(&mut d, "Waiting"));
    assert_eq!(d.rect("chat_apply").size(), kit::BUTTON);
    assert_eq!(d.rect("chat_changes_2_row_0").height(), 18.);
    // Enter in the composer still sends a message, not Apply.
    script(&log, vec![vec![done(None)]]);
    ask(&mut d, "and the next one?");
    assert_eq!(log.lock().unwrap().sent.len(), 2);
    assert!(
        log.lock().unwrap().applied.is_empty(),
        "Enter in the composer did not apply"
    );
    // Outside the composer Enter runs Apply, the default.
    d.click("chat_title");
    d.key(Key::Enter);
    d.frames(2);
    assert_eq!(
        log.lock().unwrap().applied,
        vec![11],
        "Apply calls the hook once"
    );
    assert!(shown(&mut d, "Applied."));
    assert!(!d.has("chat_apply"));
    assert_eq!(
        d.app.history.counts().0,
        undo,
        "nothing is written by the window itself"
    );
    assert!(d.app.project.model == model);
    // Discard.
    script(&log, proposal(12));
    ask(&mut d, "another");
    d.click("chat_discard");
    let l = log.lock().unwrap();
    assert_eq!(l.discarded, vec![12]);
    assert_eq!(l.applied, vec![11]);
    drop(l);
    assert!(shown(&mut d, "Discarded. Nothing was written."));
    // A failed Apply says why.
    script(&log, proposal(13));
    log.lock().unwrap().apply_error = Some("the activity is locked".into());
    ask(&mut d, "third");
    d.click("chat_apply");
    assert!(shown(&mut d, "Not applied: the activity is locked"));
}

#[test]
fn switching_agents_shows_their_state_and_starts_a_new_chat() {
    let (mut d, log) = driver("chat-agents");
    open_chat(&mut d);
    script(
        &log,
        vec![vec![AgentEvent::Message("Hi".into()), done(None)]],
    );
    ask(&mut d, "hello");
    assert!(shown(&mut d, "Hi"));
    d.click("chat_agent");
    d.click("chat_agent_item_1");
    assert_eq!(d.app.chat.agent.as_deref(), Some("codex"));
    assert!(d.app.chat.conv.is_empty(), "a new chat");
    assert_eq!(log.lock().unwrap().conversations, vec!["codex".to_string()]);
    assert!(shown(&mut d, "Codex is not installed on this PC."));
    assert!(shown(&mut d, "Codex (not installed)"));
    d.click("chat_message");
    d.type_text("typed");
    d.frames(1);
    assert!(d.app.chat.draft.is_empty(), "the composer is disabled");
    // New Chat while a conversation runs on the first agent.
    d.click("chat_agent");
    d.click("chat_agent_item_0");
    script(
        &log,
        vec![vec![AgentEvent::Message("Again".into()), done(None)]],
    );
    ask(&mut d, "hello again");
    d.click("chat_new");
    assert!(d.app.chat.conv.is_empty());
    assert!(shown(&mut d, "Claude Code is ready."));
    assert_eq!(log.lock().unwrap().conversations.len(), 3);
}

#[test]
fn chatgpt_asks_consent_once_signs_in_and_offers_models_and_sign_out() {
    let (mut d, log) = driver("chat-signin");
    open_chat(&mut d);
    assert!(
        shown(&mut d, "OpenXER itself sends nothing off this computer."),
        "a local agent says so"
    );
    d.click("chat_agent");
    d.click("chat_agent_item_2");
    assert!(shown(&mut d, "Sign in to ChatGPT to chat with it."));
    assert!(
        !shown(&mut d, "sends nothing off this computer"),
        "the line is the agent's own"
    );
    assert_eq!(d.rect("chat_sign_in").size(), kit::BUTTON);
    d.click("chat_sign_in");
    assert_eq!(d.app.chat.consent.as_deref(), Some("chatgpt"));
    assert!(
        shown(&mut d, "OpenAI"),
        "the consent box says where data goes"
    );
    // Enter is Cancel: it does not sign in.
    d.key(Key::Enter);
    d.frames(2);
    assert!(d.app.chat.consent.is_none());
    assert!(log.lock().unwrap().sign_ins.is_empty());
    // Escape too.
    d.click("chat_sign_in");
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.chat.consent.is_none() && d.app.chat.open);
    assert!(log.lock().unwrap().sign_ins.is_empty());
    d.click("chat_sign_in");
    d.click("chat_consent_continue with chatgpt");
    assert_eq!(log.lock().unwrap().sign_ins, vec!["chatgpt".to_string()]);
    assert!(shown(
        &mut d,
        "Finish signing in to ChatGPT in your browser."
    ));
    d.click("chat_sign_in_cancel");
    assert_eq!(log.lock().unwrap().cancels, vec!["chatgpt".to_string()]);
    // The consent is asked once.
    d.click("chat_sign_in");
    assert!(d.app.chat.consent.is_none());
    assert_eq!(log.lock().unwrap().sign_ins.len(), 2);
    log.lock()
        .unwrap()
        .finish_sign_in("chatgpt", "scheduler@example.com");
    d.frames(2);
    assert!(shown(
        &mut d,
        "Signed in to ChatGPT as scheduler@example.com."
    ));
    // Options...: the model and Sign Out.
    d.click("chat_options");
    assert_eq!(d.app.chat.options, Some(0));
    d.click("chat_model");
    d.click("chat_model_item_2");
    d.click("chat_options_ok");
    assert_eq!(d.app.chat.model, ModelChoice::Named("large-model".into()));
    script(&log, vec![vec![done(None)]]);
    ask(&mut d, "hi");
    assert_eq!(
        log.lock().unwrap().sent[0].3,
        ModelChoice::Named("large-model".into())
    );
    d.click("chat_options");
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.chat.options.is_none(), "Escape closes Chat Options");
    assert!(d.app.chat.open, "and only it");
    d.click("chat_options");
    d.click("chat_sign_out");
    assert_eq!(log.lock().unwrap().sign_outs, vec!["chatgpt".to_string()]);
}

#[test]
fn the_usage_limit_is_a_status_line_and_nothing_is_retried() {
    let (mut d, log) = driver("chat-limit");
    open_chat(&mut d);
    script(
        &log,
        vec![vec![AgentEvent::Error {
            kind: ErrorKind::UsageLimit,
            message: "Your limit resets at 3 PM.".into(),
        }]],
    );
    ask(&mut d, "go");
    d.frames(10);
    assert!(shown(
        &mut d,
        "Usage limit reached. Your limit resets at 3 PM."
    ));
    assert_eq!(log.lock().unwrap().sent.len(), 1, "no retry");
    assert!(!d.app.chat.conv.running);
}

#[test]
fn roll_up_by_button_and_double_click_shows_the_status_in_the_title() {
    let (mut d, log) = driver("chat-roll");
    open_chat(&mut d);
    script(
        &log,
        vec![vec![
            AgentEvent::Proposed {
                proposal: 1,
                rows: vec![change("RB-1010", "Remaining Duration", "7d", "3d")],
            },
            done(None),
        ]],
    );
    ask(&mut d, "mark it");
    let open_height = window_rect(&d).height();
    d.click("chat_roll");
    assert!(kit::geometry(&d.ctx, window_id()).rolled);
    assert!(shown(&mut d, "Chat - 1 change ready"));
    assert!(!d.has("chat_message"), "only the title bar shows");
    let close = d.rect("chat_title_close");
    assert!(close.height() < open_height);
    d.click("chat_roll");
    assert!(!kit::geometry(&d.ctx, window_id()).rolled);
    assert!(d.has("chat_message"));
    assert!(
        !shown(&mut d, "Chat - 1 change ready"),
        "the full title again"
    );
    let grip = d.rect("chat_title").center();
    d.double_click_at(grip);
    assert!(
        kit::geometry(&d.ctx, window_id()).rolled,
        "a double-click rolls it up"
    );
    // While answering, the rolled title says so.
    d.app.chat.conv.resolve(1, ProposalState::Applied);
    script(
        &log,
        (0..40).map(|n| vec![delta(&format!("{n} "))]).collect(),
    );
    d.app.chat.draft = "more".into();
    let ctx = d.app.chat_context().unwrap();
    d.app.chat.conv.send("more");
    d.app.chat.backend.send(Turn {
        agent: "claude-code",
        project: None,
        revision: 0,
        text: "more",
        mode: Mode::Ask,
        context: &ctx,
        model: &ModelChoice::AgentDefault,
        project_instructions: None,
        recap: None,
    });
    d.frames(2);
    assert!(shown(&mut d, "Chat - answering..."));
    let grip = d.rect("chat_title").center();
    d.double_click_at(grip);
    assert!(!kit::geometry(&d.ctx, window_id()).rolled, "and down again");
}

#[test]
fn a_dragged_chat_stays_on_screen() {
    let (mut d, _log) = driver("chat-drag");
    open_chat(&mut d);
    let start = d.rect("chat_title").center();
    let _ = d.frame(vec![
        egui::Event::PointerMoved(start),
        egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        },
    ]);
    for step in 1..=6 {
        let _ = d.frame(vec![egui::Event::PointerMoved(
            start + Vec2::new(500., 700.) * step as f32 / 6.,
        )]);
    }
    let end = start + Vec2::new(500., 700.);
    let _ = d.frame(vec![egui::Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    }]);
    d.frames(3);
    let screen = Rect::from_min_size(Pos2::ZERO, d.screen);
    let r = window_rect(&d);
    assert!(screen.contains_rect(r), "{r:?} on {screen:?}");
    assert!(r.right() >= screen.right() - 1. && r.bottom() >= screen.bottom() - 1.);
}

/// Drags from `from` by `by` with the primary button.
fn drag(d: &mut Driver, from: Pos2, by: Vec2) {
    let _ = d.frame(vec![egui::Event::PointerMoved(from)]);
    let _ = d.frame(vec![
        egui::Event::PointerMoved(from),
        egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        },
    ]);
    for step in 1..=4 {
        let _ = d.frame(vec![egui::Event::PointerMoved(
            from + by * step as f32 / 4.,
        )]);
    }
    let _ = d.frame(vec![egui::Event::PointerButton {
        pos: from + by,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    }]);
    d.frames(3);
}

#[test]
fn place_size_rolled_state_agent_and_mode_survive_reopening_and_a_restart() {
    let (mut d, _log) = driver("chat-persist");
    open_chat(&mut d);
    let before = window_rect(&d);
    let title = d.rect("chat_title").center();
    drag(&mut d, title, Vec2::new(-400., -60.));
    let grip = d.rect("chat_size_grip").center();
    drag(&mut d, grip, Vec2::new(40., 30.));
    let moved = window_rect(&d);
    assert!(
        (moved.left() - (before.left() - 400.)).abs() < 2.,
        "{moved:?}"
    );
    assert!(
        (moved.width() - (before.width() + 40.)).abs() < 2.,
        "resized: {moved:?}"
    );
    d.click("chat_mode");
    d.click("chat_mode_item_1");
    d.click("chat_roll");
    let g = kit::geometry(&d.ctx, window_id());
    assert!(g.rolled);
    // Close and reopen: the same place, size and rolled state.
    d.click("chat_title_close");
    open_chat(&mut d);
    assert_eq!(kit::geometry(&d.ctx, window_id()), g);
    // A restart reads them from the preferences.
    d.frames(3);
    let stored = d.app.store.preference(PREF_KEY).unwrap().expect("stored");
    let prefs: Prefs = serde_json::from_str(&stored).unwrap();
    assert_eq!(prefs.geometry, g);
    assert_eq!(prefs.mode, Mode::Edit);
    let path = d.app.store.path.clone();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let store = crate::Store::open(path).unwrap();
    let project = store.last().unwrap().unwrap();
    d.app = App::new(&ctx, store, project);
    d.ctx = ctx;
    let (fake, _log) = FakeBackend::new();
    d.app.chat.backend = Box::new(fake);
    d.frames(3);
    assert_eq!(d.app.chat.mode, Mode::Edit, "the mode is kept");
    d.key(Key::F8);
    d.frames(3);
    assert_eq!(kit::geometry(&d.ctx, window_id()), g, "the place is kept");
    assert!(shown(&mut d, "Chat"));
    d.click("chat_roll");
    let r = window_rect(&d);
    assert!(
        (r.width() - moved.width()).abs() < 2.,
        "the size is kept: {r:?}"
    );
}

#[test]
fn with_no_project_open_the_window_says_so_and_takes_no_message() {
    let (mut d, log) = driver("chat-no-project");
    d.app.project.model.project.name.clear();
    open_chat(&mut d);
    assert!(shown(&mut d, "No project is open."));
    assert!(shown(&mut d, "No project open"), "the context line");
    d.type_text("hello");
    d.key(Key::Enter);
    d.frames(2);
    assert!(d.app.chat.draft.is_empty(), "the composer is disabled");
    assert!(log.lock().unwrap().sent.is_empty());
    assert!(!d.has("chat_starter_row_0"));
    assert!(d.app.chat_context().is_none());
}

#[test]
fn the_table_takes_clicks_and_keys_while_chat_is_open() {
    let (mut d, _log) = driver("chat-nonmodal");
    open_chat(&mut d);
    assert!(
        d.app.modal_open(&d.ctx),
        "the window has the keyboard as it opens"
    );
    // A click on a table row selects it; Chat stays open.
    d.frames(1);
    let rows = d.app.rows();
    let k = rows
        .iter()
        .position(|r| *r == crate::grid::Row::Activity(2))
        .unwrap();
    let (top, height) = d.app.grid_row_tops(&rows)[k];
    let x = d.rect("grid_header_activity_id").center().x;
    d.click_at(Pos2::new(x, top + height / 2.));
    assert_eq!(d.app.selected, Some(2), "the row click selects");
    assert!(d.app.chat.open);
    assert!(
        !d.app.modal_open(&d.ctx),
        "the main window has the keyboard now"
    );
    // Row keys work with Chat open.
    d.key(Key::ArrowDown);
    assert_eq!(d.app.selected, Some(3), "Down moves the selection");
    // Escape belongs to the table now, not to Chat.
    d.key(Key::Escape);
    assert!(d.app.chat.open, "Escape outside Chat leaves it open");
    let code = d.app.project.model.activities[3].task_code.clone();
    assert!(
        shown(&mut d, &format!("{code} selected")),
        "the context follows"
    );
    // A click back in Chat gives it the keyboard again.
    d.click("chat_title");
    d.key(Key::Escape);
    assert!(!d.app.chat.open);
}

/// The window's controls come from the kit and its shared parts are
/// written once: the roll-up glyph, the Markdown reader and builder, the
/// message list and the Activity ID links. A second copy fails here.
#[test]
fn chat_controls_are_written_once() {
    let src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    fn visit(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                out.push((path.to_string_lossy().replace('\\', "/"), text));
            }
        }
    }
    let mut files = vec![];
    visit(&src, &mut files);
    let home: &[(&str, &str)] = &[
        ("fn paint_roll(", "/dialog_kit.rs"),
        ("fn paint_size_grip(", "/dialog_kit.rs"),
        ("enum Span {", "/markdown.rs"),
        ("enum Block {", "/markdown.rs"),
        ("fn spans_with(", "/markdown.rs"),
        ("fn link_rects(", "/markdown.rs"),
        ("fn table_layout(", "/markdown.rs"),
        ("fn paint_table(", "/markdown.rs"),
        ("struct Conversation {", "/chat/model.rs"),
        ("fn link_ids(", "/chat/links.rs"),
        ("stick_to_bottom(", "/chat/list.rs"),
    ];
    let mut hits = vec![];
    for (path, text) in &files {
        if path.ends_with("/chat/tests.rs") {
            continue;
        }
        for (copy, owner) in home {
            let allowed = path.ends_with(owner)
                // The kit's scrolling text area keeps its newest line too.
                || (*copy == "stick_to_bottom(" && path.ends_with("/dialog_kit.rs"));
            if !allowed
                && text
                    .lines()
                    .any(|l| l.split("//").next().unwrap_or("").contains(copy))
            {
                hits.push(format!("{path}: second copy of {copy}"));
            }
        }
    }
    assert!(hits.is_empty(), "{}", hits.join("\n"));
}

#[test]
fn picking_the_agent_on_screen_keeps_it_when_the_remembered_one_is_not_ready() {
    let (mut d, _log) = driver("chat-pick-fallback");
    // ChatGPT was remembered but needs a sign-in: Claude Code stands in.
    d.app.chat.agent = Some("chatgpt".into());
    d.app.chat.picked = false;
    open_chat(&mut d);
    assert!(shown(&mut d, "Claude Code is ready."));
    // Choosing Claude Code again changes nothing: it stays on screen.
    d.click("chat_agent");
    d.click("chat_agent_item_0");
    d.frames(2);
    assert!(shown(&mut d, "Claude Code is ready."));
    // Choosing another agent and coming back sticks to the choice.
    d.click("chat_agent");
    d.click("chat_agent_item_1");
    d.click("chat_agent");
    d.click("chat_agent_item_0");
    d.frames(2);
    assert_eq!(d.app.chat.agent.as_deref(), Some("claude-code"));
    assert!(shown(&mut d, "Claude Code is ready."));
}

#[test]
fn thinking_wheel_shows_while_running_and_clears_when_done() {
    let (mut d, _log) = driver("chat-thinking-wheel");
    open_chat(&mut d);
    d.app.chat.draft = "Check critical path".into();
    d.frames(1);
    d.click("chat_send");
    d.frames(2);
    assert!(d.app.chat.conv.running);
    assert!(
        shown(&mut d, "Thinking..."),
        "thinking indicator should show while waiting for reply"
    );
    let r = d.rect("chat_thinking");
    assert!(r.width() > 0. && r.height() > 0.);
    d.app.chat.conv.apply(done(None));
    d.frames(2);
    assert!(!d.app.chat.conv.running);
    assert!(!shown(&mut d, "Thinking..."));
}

#[test]
fn context_checkbox_excludes_context_from_sent_turn() {
    let (mut d, log) = driver("chat-context-toggle");
    open_chat(&mut d);
    assert!(d.app.chat.include_context);
    d.click("chat_include_context");
    d.frames(2);
    assert!(!d.app.chat.include_context);
    assert!(shown(&mut d, "Not included"));
    d.app.chat.draft = "General question".into();
    d.frames(1);
    d.click("chat_send");
    d.frames(2);
    {
        let rec = log.lock().unwrap();
        assert_eq!(rec.sent.len(), 1);
        assert!(rec.sent[0].2.project_id.is_empty());
        assert_eq!(rec.sent[0].2.selection, Selection::None);
    }
    d.app.chat.conv.apply(done(None));
    d.frames(2);
    // Re-checking restores project context on the next message.
    d.click("chat_include_context");
    d.frames(2);
    assert!(d.app.chat.include_context);
    d.app.chat.draft = "Now with context".into();
    d.frames(1);
    d.click("chat_send");
    d.frames(2);
    let rec = log.lock().unwrap();
    assert_eq!(rec.sent.len(), 2);
    assert!(!rec.sent[1].2.project_id.is_empty());
}

#[test]
fn dock_and_float_toggle_persists_and_shows_splitter() {
    let (mut d, _log) = driver("chat-dock");
    open_chat(&mut d);
    assert!(!d.app.chat.docked);
    d.click("chat_dock_toggle");
    d.frames(3);
    assert!(d.app.chat.docked);
    assert!(
        d.has("chat_dock_splitter"),
        "docked pane paints the shared splitter"
    );
    assert!(shown(&mut d, "Float"));
    // Closing and reopening preserves docked state.
    d.click("chat_title_close");
    d.frames(2);
    assert!(!d.app.chat.open);
    d.key(Key::F8);
    d.frames(3);
    assert!(d.app.chat.open && d.app.chat.docked);
    // Toggle back to floating.
    d.click("chat_dock_toggle");
    d.frames(3);
    assert!(!d.app.chat.docked);
    assert!(
        d.has("chat_roll"),
        "floating tool window has roll-up button"
    );
}

#[test]
fn hiding_chat_preserves_conversation_ends_backend_and_history_restores_past_chats() {
    let (mut d, log) = driver("chat-hide-history");
    open_chat(&mut d);
    script(
        &log,
        vec![vec![
            AgentEvent::Message("First answer.".into()),
            done(Some(Usage {
                input: 500,
                output: 50,
            })),
        ]],
    );
    ask(&mut d, "First question");
    d.frames(3);
    assert!(shown(&mut d, "First answer."));
    // Hiding via F8 stops the backend cleanly while keeping the visible transcript.
    d.key(Key::F8);
    d.frames(2);
    assert!(!d.app.chat.open);
    assert_eq!(log.lock().unwrap().ends, 1, "backend ended on hide");
    assert!(!d.app.chat.conv.is_empty(), "transcript kept across hide");
    assert!(d.app.chat.needs_recap);
    d.key(Key::F8);
    d.frames(2);
    assert!(d.app.chat.open);
    assert!(shown(&mut d, "First answer."));
    // Starting a new chat archives the previous one in History.
    d.click("chat_new");
    d.frames(2);
    assert!(d.app.chat.conv.is_empty());
    assert_eq!(d.app.chat.history.len(), 1);
    assert_eq!(d.app.chat.history[0].title, "First question");
    // Restore from the History popup menu.
    d.click("chat_history");
    d.frames(2);
    d.click("chat_history_item_0");
    d.frames(2);
    assert!(shown(&mut d, "First answer."));
}

#[test]
fn inline_model_selector_switches_model_directly() {
    let (mut d, log) = driver("chat-inline-model");
    open_chat(&mut d);
    d.click("chat_inline_model");
    d.click("chat_inline_model_item_1");
    d.frames(2);
    assert_eq!(d.app.chat.model, ModelChoice::LowerCost);
    script(&log, vec![vec![done(None)]]);
    ask(&mut d, "Quick check");
    assert_eq!(log.lock().unwrap().sent[0].3, ModelChoice::LowerCost);
}

#[test]
fn slash_commands_at_mentions_and_prompts_menu_work_end_to_end() {
    let (mut d, log) = driver("chat-slash-mentions");
    let code = d.app.project.model.activities[0].task_code.clone();
    open_chat(&mut d);
    // Slash popup filters playbooks and sends on click.
    d.app.chat.draft = "/look".into();
    d.frames(2);
    assert!(
        d.has("chat_slash_row_0"),
        "slash popup shows matching playbook"
    );
    script(&log, vec![vec![done(None)]]);
    d.click("chat_slash_row_0");
    d.frames(2);
    {
        let sent = &log.lock().unwrap().sent;
        assert_eq!(sent.len(), 1);
        assert!(sent[0].0.contains("lookahead"), "{}", sent[0].0);
    }
    // @ mention popup filters project activities and inserts on click.
    let prefix: String = code.chars().take(3).collect();
    d.app.chat.draft = format!("Why is @{prefix}");
    d.frames(2);
    assert!(
        d.has("chat_mention_row_0"),
        "@ mention popup shows activity"
    );
    d.click("chat_mention_row_0");
    d.frames(2);
    assert!(d.app.chat.draft.contains(&code), "{}", d.app.chat.draft);
    // Save Draft as Prompt via Prompts menu.
    d.app.chat.draft = "Check {selection} progress.".into();
    d.frames(1);
    d.click("chat_prompts");
    d.frames(2);
    d.click("chat_prompts_save_draft");
    d.frames(2);
    assert!(d.app.chat.save_prompt_dialog.is_some());
    d.click("chat_save_prompt_ok");
    d.frames(2);
    assert_eq!(d.app.chat.saved_prompts.len(), 1);
    assert_eq!(
        d.app.chat.saved_prompts[0].text,
        "Check {selection} progress."
    );
}

#[test]
fn copy_reply_copy_table_and_select_all_mentioned_activities() {
    let (mut d, log) = driver("chat-copy-select-all");
    let a0 = d.app.project.model.activities[0].task_code.clone();
    let a2 = d.app.project.model.activities[2].task_code.clone();
    let reply = format!(
        "Two drivers: **{a0}** and **{a2}**.\n\n| Activity | Float |\n|---|---|\n| {a0} | 0d |\n| {a2} | 2d |"
    );
    script(&log, vec![vec![AgentEvent::Message(reply), done(None)]]);
    open_chat(&mut d);
    ask(&mut d, "List drivers");
    d.frames(3);
    assert!(d.has("chat_copy_1"));
    assert!(d.has("chat_select_all_1"));
    assert!(d.has("chat_table_1_1_copy"));
    d.click("chat_copy_1");
    d.frames(1);
    assert_eq!(d.app.status, "Reply copied.");
    d.click("chat_table_1_1_copy");
    d.frames(1);
    assert_eq!(d.app.status, "Table copied for Excel.");
    d.click("chat_select_all_1");
    d.frames(2);
    let sel = d.app.selected_in_order();
    assert!(sel.contains(&0) && sel.contains(&2), "selected: {sel:?}");
}

#[test]
fn finished_multi_tool_block_folds_and_expands_on_click() {
    let (mut d, log) = driver("chat-tools-fold");
    open_chat(&mut d);
    script(
        &log,
        vec![vec![
            started("1", "read schedule", "RIVERBEND"),
            started("2", "check schedule", "RIVERBEND"),
            finished("1", true, 200),
            finished("2", true, 300),
            AgentEvent::Message("All clear.".into()),
            done(None),
        ]],
    );
    ask(&mut d, "Check health");
    d.frames(4);
    assert!(
        d.has("chat_tools_toggle_1"),
        "2+ done tools fold into toggle bar"
    );
    assert!(!d.has("chat_tools_1_row_0"), "rows hidden while folded");
    d.click("chat_tools_toggle_1");
    d.frames(2);
    assert!(d.has("chat_tools_1_row_0"), "clicking toggle expands rows");
    d.click("chat_tools_toggle_1");
    d.frames(2);
    assert!(!d.has("chat_tools_1_row_0"), "clicking again folds rows");
}

#[test]
fn proposal_impact_folds_large_diffs_and_links_activity_rows() {
    let (mut d, log) = driver("chat-proposal-impact");
    let target = d.app.project.model.activities.len() - 1;
    let code = d.app.project.model.activities[target].task_code.clone();
    let rows: Vec<ChangeRow> = (0..10)
        .map(|i| change(&code, &format!("Field {i}"), "1d", "2d"))
        .collect();
    script(
        &log,
        vec![vec![
            AgentEvent::Proposed { proposal: 5, rows },
            AgentEvent::ProposalImpact {
                proposal: 5,
                summary: "Finish: 09-Jun-26 \u{2192} 04-Jun-26 \u{b7} 1 activity edited".into(),
            },
            done(None),
        ]],
    );
    open_chat(&mut d);
    d.app.chat.mode = Mode::Edit;
    ask(&mut d, "Propose changes");
    d.frames(3);
    assert!(shown(&mut d, "Finish: 09-Jun-26"));
    assert!(shown(&mut d, "Show all 10 changes"));
    assert!(!d.has("chat_changes_1_row_8"), "folded to 8 rows initially");
    d.click("chat_proposal_more_1");
    d.frames(2);
    assert!(d.has("chat_changes_1_row_9"), "expanded shows all 10 rows");
    // Clicking a proposal row whose activity is in the project jumps to it.
    assert_ne!(d.app.selected, Some(target));
    d.click("chat_changes_1_row_0");
    d.frames(2);
    assert_eq!(d.app.selected, Some(target));
}

#[test]
fn switch_to_edit_retry_and_prompt_history_recall_work() {
    let (mut d, log) = driver("chat-retry-history");
    open_chat(&mut d);
    script(
        &log,
        vec![vec![
            AgentEvent::Message(
                "Ask mode is read-only. Switch to Edit mode to propose that change.".into(),
            ),
            done(None),
        ]],
    );
    ask(&mut d, "Rename RB-1010");
    d.frames(3);
    assert!(d.has("chat_switch_edit_retry"));
    script(&log, vec![vec![done(None)]]);
    d.click("chat_switch_edit_retry");
    d.frames(3);
    assert_eq!(d.app.chat.mode, Mode::Edit);
    {
        let sent = &log.lock().unwrap().sent;
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[1].0, "Rename RB-1010");
        assert_eq!(sent[1].1, Mode::Edit);
    }
    // Up arrow on an empty focused composer recalls the last prompt.
    d.click("chat_message");
    d.frames(1);
    d.key(Key::ArrowUp);
    d.frames(1);
    assert_eq!(d.app.chat.draft, "Rename RB-1010");
    d.key(Key::ArrowDown);
    d.frames(1);
    assert!(d.app.chat.draft.is_empty());
}

#[test]
fn project_rules_dialog_and_ask_about_selection_menu_work() {
    let (mut d, _log) = driver("chat-rules-selection");
    let code = d.app.project.model.activities[1].task_code.clone();
    d.app.select_activities(&[1]);
    d.home();
    d.click("menu_bar_ai_agents");
    d.click(ASK_SELECTION_COMMAND);
    d.frames(3);
    assert!(d.app.chat.open);
    assert!(d.app.chat.draft.contains(&code), "{}", d.app.chat.draft);
    // Open Options -> Project Rules and save.
    d.click("chat_options");
    d.frames(2);
    d.click("chat_options_project_rules");
    d.frames(2);
    assert!(d.app.chat.project_rules_dialog.is_some());
    d.app.chat.project_rules_dialog.as_mut().unwrap().text =
        "Contract finish has $20k/day LDs.".into();
    d.click("chat_project_rules_ok");
    d.frames(2);
    assert_eq!(
        d.app.chat.project_rules,
        "Contract finish has $20k/day LDs."
    );
}

#[test]
fn chat_ui_actions_control_filters_columns_views_paths_and_dialogs() {
    use serde_json::json;
    let (mut d, log) = driver("chat-ui-actions");
    let a0 = d.app.project.model.activities[0].task_code.clone();
    let a1 = d.app.project.model.activities[1].task_code.clone();
    open_chat(&mut d);
    // 1. Standard filter, then clear, then custom saved filter with highlight.
    script(
        &log,
        vec![vec![
            AgentEvent::UiAction(json!({"action": "filter", "filter": "Critical"})),
            AgentEvent::Message("Filtered to Critical.".into()),
            done(None),
        ]],
    );
    ask(&mut d, "Filter to critical");
    d.frames(3);
    assert!(
        !d.app.grid.layout.filter.all_activities
            && d.app
                .grid
                .layout
                .filter
                .selected
                .iter()
                .any(|k| *k == crate::grid::filters::Standard::Critical.key()),
        "Critical filter should be active: {:?}",
        d.app.grid.layout.filter
    );

    script(
        &log,
        vec![vec![
            AgentEvent::UiAction(json!({"action": "clear_filter"})),
            AgentEvent::UiAction(json!({
                "action": "filter",
                "rules": [{"column": "total_float", "op": "le", "value": "5"}],
                "save_as": "Near Critical 5d",
                "highlight": true
            })),
            AgentEvent::UiAction(json!({"action": "select", "activities": [a0, a1]})),
            AgentEvent::UiAction(json!({"action": "columns", "add": ["free_float"]})),
            AgentEvent::UiAction(
                json!({"action": "critical_path", "activity": a1, "direction": "backward", "paths": 2}),
            ),
            AgentEvent::UiAction(
                json!({"action": "view", "detail_tab": "relationships", "bottom": "details"}),
            ),
            AgentEvent::UiAction(json!({"action": "gantt", "relationships": false})),
            AgentEvent::UiAction(json!({"action": "open_dialog", "dialog": "filters"})),
            done(None),
        ]],
    );
    ask(&mut d, "Run multi-step view setup");
    d.frames(4);
    assert!(
        d.app
            .grid
            .user_filters
            .iter()
            .any(|f| f.name == "Near Critical 5d"),
        "saved custom filter should exist"
    );
    assert!(d.app.grid.layout.filter.highlight);
    // `select` picked [0, 1], then `critical_path` targeted `a1` (index 1).
    assert_eq!(d.app.selected_in_order(), vec![1]);
    assert!(d
        .app
        .grid
        .layout
        .columns
        .iter()
        .any(|c| c.id == "free_float"));
    assert!(d.app.critical_path.shown.is_some());
    assert_eq!(d.app.detail.tab, crate::details::DetailTab::Relationships);
    assert!(!d.app.gantt.options.show_relationships);
    assert!(shown(&mut d, "Filters"));
}

#[test]
fn drag_select_and_ctrl_c_copies_chat_text_without_breaking_links() {
    let (mut d, log) = driver("chat-drag-copy");
    let target = d.app.project.model.activities.len() - 1;
    let code = d.app.project.model.activities[target].task_code.clone();
    script(
        &log,
        vec![vec![
            AgentEvent::Message(format!("Activity {code} drives the finish date.")),
            done(None),
        ]],
    );
    open_chat(&mut d);
    ask(&mut d, "Explain the driver");
    d.frames(3);
    // Drag across the reply rectangle and press Ctrl+C.
    let rect = d.rect("chat_reply_1");
    let from = Pos2::new(rect.left() + 4., rect.center().y);
    let to = Pos2::new(rect.left() + 140., rect.center().y);
    let _ = d.frame(vec![
        egui::Event::PointerMoved(from),
        egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        },
    ]);
    let _ = d.frame(vec![egui::Event::PointerMoved(to)]);
    let _ = d.frame(vec![
        egui::Event::PointerMoved(to),
        egui::Event::PointerButton {
            pos: to,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        },
    ]);
    d.frames(1);
    d.key_with(Key::C, egui::Modifiers::COMMAND);
    d.frames(2);
    assert_eq!(d.app.status, "Copied selection.");
    // Clicking the Activity ID link without dragging still jumps to the activity.
    assert_ne!(d.app.selected, Some(target));
    d.click(&format!("chat_link_{code}"));
    d.frames(2);
    assert_eq!(d.app.selected, Some(target));
}

#[test]
fn applying_a_proposal_notifies_the_agent_to_continue() {
    let (mut d, log) = driver("chat-apply-continue");
    let code = d.app.project.model.activities[0].task_code.clone();
    log.lock().unwrap().apply_status = Some("Applied 1 schedule edit.".into());
    script(
        &log,
        vec![vec![
            AgentEvent::Proposed {
                proposal: 7,
                rows: vec![change(&code, "Original Duration", "5d", "3d")],
            },
            done(None),
        ]],
    );
    script(
        &log,
        vec![vec![
            AgentEvent::Message("Saw the applied edit, continuing with step 2.".into()),
            done(None),
        ]],
    );
    open_chat(&mut d);
    d.app.chat.mode = Mode::Edit;
    ask(
        &mut d,
        "Shorten first activity and then check critical path",
    );
    d.frames(3);
    assert!(d.has("chat_apply"));
    d.click("chat_apply");
    d.frames(4);
    let sent = log.lock().unwrap().sent.clone();
    assert_eq!(sent.len(), 2, "Apply should auto-send continuation turn");
    assert!(
        sent[1].0.contains("Applied 1 schedule edit."),
        "got: {}",
        sent[1].0
    );
    assert!(shown(&mut d, "continuing with step 2"));
}

#[test]
fn queuing_messages_while_running_and_send_now_work() {
    let (mut d, log) = driver("chat-queue-send-now");
    open_chat(&mut d);
    // Keep turn 1 running across many frames (no TurnFinished yet).
    script(
        &log,
        vec![vec![AgentEvent::TextDelta("Working on step 1...".into())]],
    );
    ask(&mut d, "Start analysis");
    d.frames(2);
    assert!(d.app.chat.conv.running);
    // Type a follow-up while running and press Enter -> queued.
    d.click("chat_message");
    d.type_text("Also check lags");
    d.frames(1);
    assert!(
        d.has("chat_queue"),
        "Queue button shows when draft non-empty"
    );
    d.key(Key::Enter);
    d.frames(2);
    assert_eq!(d.app.chat.queued_messages, vec!["Also check lags"]);
    assert!(d.app.chat.draft.is_empty());
    assert!(d.has("chat_queue_now_0"));
    assert!(d.has("chat_queue_drop_0"));
    // Queue a second message via the Queue button, then drop it.
    d.click("chat_message");
    d.type_text("Never mind this one");
    d.frames(1);
    d.click("chat_queue");
    d.frames(2);
    assert_eq!(d.app.chat.queued_messages.len(), 2);
    d.click("chat_queue_drop_1");
    d.frames(2);
    assert_eq!(d.app.chat.queued_messages, vec!["Also check lags"]);
    // Click "Send now" -> interrupts current turn and sends immediately.
    script(
        &log,
        vec![vec![
            AgentEvent::Message("Checked lags right away.".into()),
            done(None),
        ]],
    );
    d.click("chat_queue_now_0");
    d.frames(4);
    assert!(d.app.chat.queued_messages.is_empty());
    assert_eq!(log.lock().unwrap().stops, 1);
    assert_eq!(log.lock().unwrap().sent.len(), 2);
    assert_eq!(log.lock().unwrap().sent[1].0, "Also check lags");
    assert!(shown(&mut d, "Checked lags right away."));
}

#[test]
fn ask_user_question_card_lets_user_pick_options_and_add_context() {
    use serde_json::json;
    let (mut d, log) = driver("chat-ask-user");
    open_chat(&mut d);
    script(
        &log,
        vec![vec![
            AgentEvent::UiAction(json!({
                "action": "ask_user",
                "title": "Baseline selection",
                "questions": [{
                    "question": "Which baseline should I compare against?",
                    "options": [
                        {"label": "Current Baseline", "description": "Assigned project baseline"},
                        {"label": "Primary Baseline"}
                    ]
                }]
            })),
            done(None),
        ]],
    );
    ask(&mut d, "Compare against baseline");
    d.frames(3);
    assert!(shown(&mut d, "Which baseline should I compare against?"));
    assert!(d.has("chat_q_1_0_opt_0"));
    assert!(d.has("chat_q_1_0_other"));
    // Pick option 0 and add extra context in the Other box, then click Send Answer.
    d.click("chat_q_1_0_opt_0");
    d.click("chat_q_1_0_other");
    d.type_text("focus on structural");
    d.frames(1);
    script(
        &log,
        vec![vec![
            AgentEvent::Message("Comparing structural against Current Baseline.".into()),
            done(None),
        ]],
    );
    d.click("chat_q_send_1");
    d.frames(4);
    let sent = log.lock().unwrap().sent.clone();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[1].0, "Current Baseline (focus on structural)");
    assert!(shown(
        &mut d,
        "Answered: Current Baseline (focus on structural)"
    ));
}
