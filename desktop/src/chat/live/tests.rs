//! The live backend on fakes: detection and install shapes, both hosted
//! agents on their recorded transcripts, the ChatGPT route on the chat
//! crate's fake service, every event and error kind, proposals and Apply,
//! the token rules, cleanup, Check Setup. Then the whole window over this
//! backend through the real menu command, F8 and the window's controls, for
//! each route. No real agent, browser, host or Credential Manager is used.
use super::fakes::*;
use super::*;
use crate::chat::backend::ChatContext as WindowContext;
use crate::qa_sweep::Driver;
use eframe::egui::{self, Key};
use openxer_chat::testkit as kit;
use std::path::Path;
use std::time::Duration;

// ------------------------------------------------------------- helpers

fn sample() -> ProjectData {
    openxer_xer::sample_project::sample_project().current
}

fn context(selection: Selection) -> WindowContext {
    WindowContext {
        project_id: "RIVERBEND".into(),
        selection,
        data_date: "12-Oct-26".into(),
        ..Default::default()
    }
}

/// Polls until `done` holds for the events so far (10 s at most).
fn pump(b: &mut LiveBackend, done: impl Fn(&[AgentEvent]) -> bool) -> Vec<AgentEvent> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut all = vec![];
    loop {
        all.extend(b.poll());
        if done(&all) {
            return all;
        }
        assert!(
            Instant::now() < deadline,
            "timed out; events so far {all:#?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn turn_end(e: &[AgentEvent]) -> bool {
    e.iter().any(|e| {
        matches!(
            e,
            AgentEvent::TurnFinished { .. } | AgentEvent::Error { .. }
        )
    })
}

/// Polls until no agent is being looked for.
fn detected(b: &mut LiveBackend) -> Vec<AgentInfo> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        b.poll();
        let agents = b.agents();
        if !agents.iter().any(|a| a.state == AgentState::Checking) {
            return agents;
        }
        assert!(Instant::now() < deadline, "detection ends");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn state(agents: &[AgentInfo], id: &str) -> AgentState {
    agents.iter().find(|a| a.id == id).unwrap().state.clone()
}

struct Ask<'a> {
    agent: &'a str,
    text: &'a str,
    mode: Mode,
    model: ModelChoice,
    selection: Selection,
    revision: u64,
}

impl<'a> Ask<'a> {
    fn new(agent: &'a str, text: &'a str) -> Ask<'a> {
        Ask {
            agent,
            text,
            mode: Mode::Ask,
            model: ModelChoice::AgentDefault,
            selection: Selection::None,
            revision: 1,
        }
    }
    fn send(&self, b: &mut LiveBackend, project: &ProjectData) {
        let ctx = context(self.selection.clone());
        b.send(Turn {
            agent: self.agent,
            text: self.text,
            mode: self.mode,
            context: &ctx,
            model: &self.model,
            project: Some(project),
            revision: self.revision,
            project_instructions: None,
            recap: None,
        });
    }
}

fn texts(events: &[AgentEvent]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Message(t) => Some(t.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn proposal(events: &[AgentEvent]) -> (u64, Vec<ChangeRow>) {
    events
        .iter()
        .find_map(|e| match e {
            AgentEvent::Proposed { proposal, rows } => Some((*proposal, rows.clone())),
            _ => None,
        })
        .expect("a proposal")
}

fn error_of(events: &[AgentEvent]) -> (ErrorKind, String) {
    events
        .iter()
        .find_map(|e| match e {
            AgentEvent::Error { kind, message } => Some((*kind, message.clone())),
            _ => None,
        })
        .expect("an error")
}

fn name_of(p: &ProjectData, code: &str) -> String {
    let a = p
        .model
        .activities
        .iter()
        .find(|a| a.task_code == code)
        .unwrap();
    p.details[&a.task_id].name.clone()
}

// ------------------------------------------------- detection and shapes

#[test]
fn detection_reads_every_state_without_blocking() {
    let mut rig = Rig::bare("detect states");
    rig.profile.install(Agent::ClaudeCode, Shape::Npm);
    rig.agents.with(Agent::ClaudeCode, |b| b.signed_in = false);
    rig.profile.install(Agent::Codex, Shape::Exe);
    rig.agents.set(Agent::Codex, Behaviour::of(Agent::Codex));
    let started = Instant::now();
    let mut b = rig.backend();
    let first = b.agents();
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "making the backend and reading its agents never waits for detection"
    );
    // ChatGPT is listed first: the route with nothing to install.
    assert_eq!(first[0].id, CHATGPT);
    assert_eq!(first[0].state, AgentState::SignInNeeded);
    assert!(first[0].plan && first[0].install_url.is_none());
    let agents = detected(&mut b);
    assert_eq!(state(&agents, CLAUDE_CODE), AgentState::SignInNeeded);
    assert_eq!(state(&agents, CODEX), AgentState::Ready);
    assert!(agents.iter().all(|a| !a.setup.is_empty()));
    // Nothing installed: each says where to get it.
    let rig = Rig::bare("detect none");
    let mut b = rig.backend();
    let agents = detected(&mut b);
    assert_eq!(state(&agents, CLAUDE_CODE), AgentState::NotInstalled);
    assert_eq!(state(&agents, CODEX), AgentState::NotInstalled);
    let urls: Vec<_> = agents
        .iter()
        .filter_map(|a| a.install_url.clone())
        .collect();
    assert_eq!(urls, [CLAUDE_CODE_URL, CODEX_URL]);
    for url in urls {
        assert!(url.starts_with("https://"), "{url}");
    }
}

#[test]
fn every_install_shape_is_found_and_started_as_installed() {
    // npm (.cmd beside a .ps1), a winget .exe, a path with spaces and a
    // profile with non-ASCII letters. The fake spawner stands in for the
    // processes: no real npm-installed agent runs here.
    for (tag, shape) in [
        ("npm shim", Shape::Npm),
        ("winget exe", Shape::Exe),
        ("Ünïcödé Üser", Shape::Npm),
        ("with spaces and (brackets)", Shape::Exe),
    ] {
        let mut rig = Rig::bare(tag);
        let claude = rig.profile.install(Agent::ClaudeCode, shape);
        let codex = rig.profile.install(Agent::Codex, shape);
        rig.agents
            .set(Agent::ClaudeCode, Behaviour::of(Agent::ClaudeCode));
        rig.agents.set(Agent::Codex, Behaviour::of(Agent::Codex));
        let mut b = rig.backend();
        let agents = detected(&mut b);
        assert_eq!(state(&agents, CLAUDE_CODE), AgentState::Ready, "{tag}");
        assert_eq!(state(&agents, CODEX), AgentState::Ready, "{tag}");
        let programs: Vec<PathBuf> = rig
            .agents
            .launches()
            .iter()
            .map(|l| l.program.clone())
            .collect();
        assert!(programs.contains(&claude), "{tag}: {programs:?}");
        assert!(programs.contains(&codex), "{tag}: {programs:?}");
        assert!(
            !programs
                .iter()
                .any(|p| p.extension().is_some_and(|e| e == "ps1")),
            "{tag}: a PowerShell script is never started"
        );
        // A conversation starts the same program, from a folder of its own.
        let project = sample();
        Ask::new(CODEX, "How many activities?").send(&mut b, &project);
        pump(&mut b, turn_end);
        let launch = rig.agents.launches().into_iter().next_back().unwrap();
        assert_eq!(launch.program, codex, "{tag}");
        assert!(
            launch.args.iter().all(|a| !a.contains('\n')),
            "{tag}: batch-safe arguments"
        );
        b.end();
    }
}

#[test]
fn versions_older_than_the_tested_ones_get_a_notice_and_still_run() {
    let rig = Rig::new("old versions");
    rig.agents
        .with(Agent::Codex, |b| b.version = "codex-cli 0.150.0".into());
    let mut b = rig.backend();
    let agents = detected(&mut b);
    let codex = agents.iter().find(|a| a.id == CODEX).unwrap();
    assert_eq!(
        codex.state,
        AgentState::Ready,
        "an old version is still tried"
    );
    let notice = codex.notice.clone().expect("a notice");
    assert!(
        notice.contains("Codex 0.150.0 is older") && notice.contains("0.166.0"),
        "{notice}"
    );
    let project = sample();
    Ask::new(CODEX, "How many activities?").send(&mut b, &project);
    let events = pump(&mut b, turn_end);
    assert!(
        matches!(&events[0], AgentEvent::Notice(n) if n.contains("is older")),
        "the conversation says so first: {events:?}"
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, AgentEvent::TurnFinished { .. })));
}

#[test]
fn a_version_that_refuses_its_flags_says_it_is_not_supported() {
    for (agent, id, refusal) in [
        (Agent::ClaudeCode, CLAUDE_CODE, "error: unknown option '--restricted'\n    at parse (cli.js:12)\n    at main (cli.js:40)"),
        (Agent::Codex, CODEX, "Error: unknown feature `apps` in --disable\nUsage: codex exec [OPTIONS]"),
    ] {
        let rig = Rig::new("refused");
        rig.agents.with(agent, |b| {
            b.version = match agent {
                Agent::ClaudeCode => "1.9.0 (Claude Code)".into(),
                Agent::Codex => "codex-cli 0.90.1".into(),
            };
            b.refuses = Some(refusal.into());
        });
        let mut b = rig.backend();
        detected(&mut b);
        let project = sample();
        let started = Instant::now();
        Ask::new(id, "Hello").send(&mut b, &project);
        let events = pump(&mut b, |e| e.iter().any(|e| matches!(e, AgentEvent::Error { .. })));
        assert!(started.elapsed() < Duration::from_secs(5), "fails in seconds");
        let (kind, message) = error_of(&events);
        assert_eq!(kind, ErrorKind::Failed);
        let version = if agent == Agent::ClaudeCode { "1.9.0" } else { "0.90.1" };
        assert!(
            message.starts_with(&format!("{} {version} is not supported by this version of OpenXER", agent.name())),
            "{message}"
        );
        assert!(message.contains(&format!("Update {}", agent.name())));
        let first = refusal.lines().next().unwrap();
        assert!(message.ends_with(first), "only the first line: {message}");
        assert!(!message.contains("at main"), "no stack: {message}");
    }
}

#[test]
fn a_launch_that_never_answers_ends_after_the_ready_timeout() {
    let mut rig = Rig::new("hang");
    rig.ready_timeout = Duration::from_millis(300);
    rig.agents.with(Agent::ClaudeCode, |b| b.hangs = true);
    let mut b = rig.backend();
    detected(&mut b);
    Ask::new(CLAUDE_CODE, "Hello").send(&mut b, &sample());
    let events = pump(&mut b, |e| {
        e.iter().any(|e| matches!(e, AgentEvent::Error { .. }))
    });
    let (_, message) = error_of(&events);
    assert!(message.contains("did not start within"), "{message}");
    // The hung process was stopped.
    let deadline = Instant::now() + Duration::from_secs(5);
    while rig.agents.alive() > 0 {
        assert!(Instant::now() < deadline, "the hung agent is stopped");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn the_agents_own_sign_in_runs_and_can_be_cancelled() {
    let rig = Rig::new("local sign-in");
    rig.agents.with(Agent::ClaudeCode, |b| b.signed_in = false);
    let mut b = rig.backend();
    assert_eq!(
        state(&detected(&mut b), CLAUDE_CODE),
        AgentState::SignInNeeded
    );
    b.sign_in(CLAUDE_CODE);
    assert_eq!(state(&b.agents(), CLAUDE_CODE), AgentState::SigningIn);
    assert!(rig
        .agents
        .launches()
        .iter()
        .any(|l| l.args == ["auth", "login"]));
    b.cancel_sign_in(CLAUDE_CODE);
    assert_eq!(state(&b.agents(), CLAUDE_CODE), AgentState::SignInNeeded);
    // Again, and it finishes: the agent is looked at again and is ready.
    b.sign_in(CLAUDE_CODE);
    rig.agents.with(Agent::ClaudeCode, |b| b.signed_in = true);
    *rig.agents.sign_in_done.lock().unwrap() = Some(true);
    let deadline = Instant::now() + Duration::from_secs(30);
    while state(&b.agents(), CLAUDE_CODE) != AgentState::Ready {
        b.poll();
        assert!(Instant::now() < deadline, "ready after sign-in");
        std::thread::sleep(Duration::from_millis(5));
    }
}

// --------------------------------------------------------- hosted routes

#[test]
fn claude_code_answers_with_tools_and_proposes_through_its_transcript() {
    let rig = Rig::new("claude route");
    let mut b = rig.backend();
    detected(&mut b);
    let project = sample();
    let mut ask = Ask::new(CLAUDE_CODE, "What drives the finish?");
    ask.selection = Selection::One("PM0920".into());
    ask.send(&mut b, &project);
    let events = pump(&mut b, turn_end);
    assert!(events.contains(&AgentEvent::Ready));
    assert!(
        events.iter().any(|e| matches!(e, AgentEvent::TextDelta(_))),
        "it streams"
    );
    assert!(events.iter().any(|e| matches!(e,
        AgentEvent::ToolStarted { tool, target, .. } if tool == "read schedule" && target == "critical activities")));
    assert!(events
        .iter()
        .any(|e| matches!(e, AgentEvent::ToolFinished { ok: true, .. })));
    assert!(texts(&events).contains("PM0920"));
    let usage = events.iter().find_map(|e| match e {
        AgentEvent::TurnFinished { usage } => *usage,
        _ => None,
    });
    assert_eq!(
        usage,
        Some(Usage {
            input: 4 + 5698 + 3746,
            output: 331
        }),
        "the usage line"
    );
    // The agent ran in a fresh temp folder of its own, with the snapshot.
    let launch = rig
        .agents
        .launches()
        .into_iter()
        .find(|l| l.args.contains(&"-p".to_string()))
        .unwrap();
    let dir = launch.cwd.clone().unwrap();
    assert!(dir.starts_with(&rig.temp), "{dir:?}");
    assert!(dir.join(work::SNAPSHOT).is_file());
    assert!(work::listed(&dir));
    // The first message carries the preamble with the selection.
    let first = rig.agents.stdin()[0].clone();
    assert!(
        first.contains("Open project:") && first.contains("PM0920"),
        "{first}"
    );
    // The second turn proposes renaming PM0910 (Edit mode).
    let mut ask = Ask::new(CLAUDE_CODE, "Rename PM0910");
    ask.mode = Mode::Edit;
    ask.selection = Selection::One("PM0920".into());
    ask.send(&mut b, &project);
    let events = pump(&mut b, turn_end);
    let (id, rows) = proposal(&events);
    assert!(
        rows.iter().any(|r| r.activity == "PM0910"
            && r.field == "Activity Name"
            && r.new == "Submit as-built drawings and O&M manuals"),
        "{rows:?}"
    );
    // Edit mode restarted Claude Code with the edit tools, resuming.
    let resumed = rig
        .agents
        .launches()
        .into_iter()
        .rfind(|l| l.args.contains(&"-p".to_string()))
        .unwrap();
    assert!(
        resumed.args.contains(&"--resume".to_string()),
        "{:?}",
        resumed.args
    );
    assert!(resumed.args.iter().any(|a| a.contains("openxer_apply")));
    // Apply works it out on the open project: one change, as previewed.
    let Ok(Applied::Project(next, _)) = b.apply(id, &project, 1) else {
        panic!("applies")
    };
    assert_eq!(
        name_of(&next, "PM0910"),
        "Submit as-built drawings and O&M manuals"
    );
    assert_eq!(
        name_of(&project, "PM0910"),
        "Submit as-built drawings and closeout documents"
    );
    assert!(b.apply(id, &next, 2).is_err(), "a proposal applies once");
    // Ending removes the folder and asks Claude Code to forget it.
    b.end();
    assert!(!dir.exists() && !work::listed(&dir));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !rig
        .agents
        .launches()
        .iter()
        .any(|l| l.args.first().is_some_and(|a| a == "purge"))
    {
        assert!(
            Instant::now() < deadline,
            "the stored conversation is purged"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    while rig.agents.alive() > 0 {
        assert!(Instant::now() < deadline, "no agent process is left");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn codex_resumes_its_thread_and_warns_once_about_a_personal_instructions_file() {
    let rig = Rig::new("codex route");
    let file = rig.profile.codex_instructions("Always answer in French.");
    let mut b = rig.backend();
    detected(&mut b);
    let project = sample();
    Ask::new(CODEX, "Which activities are critical?").send(&mut b, &project);
    let first = pump(&mut b, turn_end);
    let warnings = |e: &[AgentEvent]| {
        e.iter()
            .filter(|e| matches!(e, AgentEvent::Notice(n) if n.contains("own instructions file")))
            .count()
    };
    assert_eq!(warnings(&first), 1, "{first:?}");
    assert!(file.is_file());
    assert!(!texts(&first).is_empty());
    let mut ask = Ask::new(CODEX, "And propose a fix");
    ask.mode = Mode::Edit;
    ask.send(&mut b, &project);
    let second = pump(&mut b, turn_end);
    assert_eq!(warnings(&second), 0, "once per conversation");
    let runs: Vec<_> = rig
        .agents
        .launches()
        .into_iter()
        .filter(|l| l.args.first().is_some_and(|a| a == "exec"))
        .collect();
    assert_eq!(runs.len(), 2);
    assert_eq!(
        runs[1].args[1], "resume",
        "the second turn resumes the thread"
    );
    assert!(second
        .iter()
        .any(|e| matches!(e, AgentEvent::TurnFinished { .. })));
    b.end();
}

#[test]
fn hosted_errors_are_plain_status_lines() {
    // Not signed in (the agent says so in its result).
    let rig = Rig::new("errors");
    rig.agents.with(Agent::ClaudeCode, |b| {
        b.turns = vec![vec![
            r#"{"type":"system","subtype":"init","session_id":"00000000-0000-4000-8000-000000000009","model":"m","mcp_servers":[{"name":"openxer","status":"connected"}]}"#.to_string(),
            r#"{"type":"result","subtype":"success","is_error":true,"result":"Not logged in · Please run /login","usage":{}}"#.to_string(),
        ]]
        .into();
    });
    // Codex's process dies in the middle of the reply.
    rig.agents.with(Agent::Codex, |b| {
        b.turns = vec![vec![
            r#"{"type":"thread.started","thread_id":"00000000-0000-4000-8000-00000000000a"}"#
                .to_string(),
        ]]
        .into();
    });
    let mut b = rig.backend();
    detected(&mut b);
    let project = sample();
    Ask::new(CLAUDE_CODE, "Hi").send(&mut b, &project);
    let (kind, message) = error_of(&pump(&mut b, turn_end));
    assert_eq!(kind, ErrorKind::NotSignedIn, "{message}");
    b.new_conversation(CODEX);
    Ask::new(CODEX, "Hi").send(&mut b, &project);
    let (kind, message) = error_of(&pump(&mut b, |e| {
        e.iter().any(|e| matches!(e, AgentEvent::Error { .. }))
    }));
    assert_eq!(kind, ErrorKind::Failed);
    assert!(
        message.contains("Codex stopped before the reply ended"),
        "{message}"
    );
    // Not installed, and no openxer-cli beside the app.
    let mut rig = Rig::bare("errors 2");
    let mut b = rig.backend();
    detected(&mut b);
    Ask::new(CODEX, "Hi").send(&mut b, &project);
    assert!(error_of(&b.poll()).1.contains("Codex is not installed"));
    rig.profile.install(Agent::Codex, Shape::Npm);
    rig.agents.set(Agent::Codex, Behaviour::of(Agent::Codex));
    rig.profile.env.cli = Some(rig.profile.root.join("missing").join("openxer-cli.exe"));
    let mut b = rig.backend();
    detected(&mut b);
    Ask::new(CODEX, "Hi").send(&mut b, &project);
    assert!(error_of(&b.poll()).1.contains("was not found"));
    // No project.
    let ctx = context(Selection::None);
    b.send(Turn {
        agent: CODEX,
        text: "Hi",
        mode: Mode::Ask,
        context: &ctx,
        model: &ModelChoice::AgentDefault,
        project: None,
        revision: 0,
        project_instructions: None,
        recap: None,
    });
    assert!(error_of(&b.poll()).1.contains("No project is open"));
}

#[test]
fn stop_ends_the_reply_and_the_next_message_goes_on() {
    let rig = Rig::new("stop");
    rig.agents.with(Agent::ClaudeCode, |b| {
        // The first reply never ends; the second is the recorded one.
        b.turns.push_front(vec![
            r#"{"type":"system","subtype":"init","session_id":"00000000-0000-4000-8000-000000000001","model":"m","mcp_servers":[{"name":"openxer","status":"connected"}]}"#.to_string(),
        ]);
    });
    let mut b = rig.backend();
    detected(&mut b);
    let project = sample();
    Ask::new(CLAUDE_CODE, "Long one").send(&mut b, &project);
    pump(&mut b, |e| e.contains(&AgentEvent::Ready));
    b.stop();
    let after = {
        std::thread::sleep(Duration::from_millis(100));
        b.poll()
    };
    assert!(
        after
            .iter()
            .all(|e| !matches!(e, AgentEvent::TurnFinished { .. })),
        "a stopped reply says nothing more: {after:?}"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while rig.agents.alive() > 0 {
        assert!(Instant::now() < deadline, "its process was stopped");
        std::thread::sleep(Duration::from_millis(5));
    }
    Ask::new(CLAUDE_CODE, "Next").send(&mut b, &project);
    let events = pump(&mut b, turn_end);
    assert!(texts(&events).contains("PM0920"), "{events:?}");
    b.end();
}

// ---------------------------------------------------------- token rules

#[test]
fn the_preamble_goes_once_the_selection_only_when_it_changed_and_the_snapshot_only_after_an_edit() {
    let rig = Rig::new("tokens");
    rig.agents.with(Agent::ClaudeCode, |b| {
        let one = claude_turns()[0].clone();
        b.turns = vec![one.clone(), one.clone(), one.clone(), one].into();
    });
    let mut b = rig.backend();
    detected(&mut b);
    let project = sample();
    let mut ask = Ask::new(CLAUDE_CODE, "first");
    ask.selection = Selection::One("PM0910".into());
    ask.send(&mut b, &project);
    pump(&mut b, turn_end);
    // Same selection, same project.
    let mut ask = Ask::new(CLAUDE_CODE, "second");
    ask.selection = Selection::One("PM0910".into());
    ask.send(&mut b, &project);
    pump(&mut b, turn_end);
    assert_eq!(b.snapshot_writes, 1, "no new snapshot without an edit");
    // Another selection after an edit.
    let mut ask = Ask::new(CLAUDE_CODE, "third");
    ask.selection = Selection::Many(vec!["PM0900".into(), "PM0920".into()]);
    ask.revision = 2;
    ask.send(&mut b, &project);
    pump(&mut b, turn_end);
    assert_eq!(b.snapshot_writes, 2, "a new snapshot after the edit");
    let lines = rig.agents.stdin();
    assert_eq!(lines.len(), 3);
    let text = |n: usize| -> String {
        let v: serde_json::Value = serde_json::from_str(&lines[n]).unwrap();
        v["message"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert!(
        text(0).contains("Open project:"),
        "the preamble first: {}",
        text(0)
    );
    assert!(
        !text(1).contains("Open project:") && !text(1).contains("Context:"),
        "nothing resent: {}",
        text(1)
    );
    assert_eq!(text(1), "second");
    assert!(
        text(2).contains("Selected now: PM0900, PM0920."),
        "{}",
        text(2)
    );
    assert!(
        text(2).contains("changed in the app since the last message"),
        "{}",
        text(2)
    );
    assert!(!text(2).contains("Open project:"));
    b.end();
}

// --------------------------------------------------------------- Apply

#[test]
fn a_stale_proposal_is_worked_out_again_or_refused() {
    let rig = Rig::new("stale");
    let mut b = rig.backend();
    detected(&mut b);
    let project = sample();
    let mut ask = Ask::new(CLAUDE_CODE, "Rename PM0910");
    ask.mode = Mode::Edit;
    ask.send(&mut b, &project);
    pump(&mut b, turn_end);
    ask.send(&mut b, &project);
    let (id, rows) = proposal(&pump(&mut b, turn_end));
    // The same project in another state (an edit elsewhere): the rows still
    // read the same, so it applies.
    let mut edited = project.clone();
    let other = edited
        .model
        .activities
        .iter()
        .find(|a| a.task_code == "PM0920")
        .unwrap()
        .task_id;
    edited.details.get_mut(&other).unwrap().name = "Finish everything".into();
    let mut b2_project = edited.clone();
    // PM0910 renamed by hand: the change now reads differently.
    let target = b2_project
        .model
        .activities
        .iter()
        .find(|a| a.task_code == "PM0910")
        .unwrap()
        .task_id;
    b2_project.details.get_mut(&target).unwrap().name = "Hand edit".into();
    match b.apply(id, &b2_project, 7) {
        Ok(Applied::Refreshed(now)) => {
            assert_ne!(now, rows);
            assert!(now.iter().any(|r| r.old == "Hand edit"), "{now:?}");
        }
        other => panic!("refreshed rows, got {other:?}"),
    }
    // Applied again on that state, as shown.
    let Ok(Applied::Project(next, _)) = b.apply(id, &b2_project, 7) else {
        panic!("applies once the rows were seen")
    };
    assert_eq!(
        name_of(&next, "PM0910"),
        "Submit as-built drawings and O&M manuals"
    );
    // A proposal for an activity that is gone is refused with the reason.
    let rig = Rig::new("gone");
    let mut b = rig.backend();
    detected(&mut b);
    ask.send(&mut b, &project);
    pump(&mut b, turn_end);
    ask.send(&mut b, &project);
    let (id, _) = proposal(&pump(&mut b, turn_end));
    let mut gone = project.clone();
    gone.model.activities.retain(|a| a.task_code != "PM0910");
    gone.model.links.clear();
    let e = b.apply(id, &gone, 9).unwrap_err();
    assert!(e.contains("no longer applies"), "{e}");
    // Discard tells the next message, and changes nothing.
    b.discard(id);
    assert!(b.apply(id, &project, 1).is_err());
}

// --------------------------------------------------------------- ChatGPT

fn signed_in_rig(tag: &str) -> Rig {
    let rig = Rig::bare(tag);
    rig.sign_in_chatgpt();
    rig
}

/// Polls until the model list is in.
fn models(b: &mut LiveBackend) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        b.poll();
        let m = b.agents()[0].models.clone();
        if !m.is_empty() {
            return m;
        }
        assert!(Instant::now() < deadline, "the model list arrives");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn chatgpt_signs_in_through_the_opener_and_lists_its_models() {
    let rig = Rig::bare("plan sign-in");
    let mut b = rig.backend();
    let a = b.agents()[0].clone();
    assert_eq!(a.consent.as_deref(), Some(oc::CONSENT_TEXT));
    assert_eq!(a.privacy, CHATGPT_PRIVACY);
    // Cancel while the browser has not come back.
    *rig.browser.lock().unwrap() = kit::Browser::Nothing;
    b.sign_in(CHATGPT);
    assert_eq!(b.agents()[0].state, AgentState::SigningIn);
    // Its page is open in the (fake) browser before the user cancels.
    let deadline = Instant::now() + Duration::from_secs(30);
    while rig.opened.lock().unwrap().is_empty() {
        assert!(Instant::now() < deadline, "the sign-in page opens");
        std::thread::sleep(Duration::from_millis(5));
    }
    b.cancel_sign_in(CHATGPT);
    b.poll();
    assert_eq!(b.agents()[0].state, AgentState::SignInNeeded);
    assert!(b.agents()[0].notice.is_none(), "a cancel is no failure");
    // Approve: signed in under the account, models listed.
    *rig.browser.lock().unwrap() = kit::Browser::approve_first();
    b.sign_in(CHATGPT);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !matches!(b.agents()[0].state, AgentState::SignedIn { .. }) {
        b.poll();
        assert!(Instant::now() < deadline, "signed in");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        b.agents()[0].state,
        AgentState::SignedIn {
            account: "pat@example.test".into()
        }
    );
    let opened = rig.opened.lock().unwrap().clone();
    assert_eq!(
        opened.len(),
        2,
        "each sign-in opened one page through the opener"
    );
    assert!(opened.iter().all(|u| u.contains("/authorize?")));
    assert_eq!(models(&mut b), ["Model A", "Model B"]);
    assert!(b.agents()[0].can_sign_out);
    // Sign out: signed out, models gone.
    b.sign_out(CHATGPT);
    let deadline = Instant::now() + Duration::from_secs(30);
    while b.agents()[0].state != AgentState::SignInNeeded {
        b.poll();
        assert!(Instant::now() < deadline, "signed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn chatgpt_sign_in_failures_say_why() {
    // This PC's own proxy settings do not count here.
    let _one = PROXY.lock().unwrap_or_else(|p| p.into_inner());
    kit::FAKE_PROXY_ALL.store(1, std::sync::atomic::Ordering::SeqCst);
    let _reset = Reset;
    // The network is down: the token endpoint cannot be reached.
    let plan = Plan2::with(|c| {
        c.token_url = oc::http::Url::parse("http://127.0.0.1:9/token").unwrap();
        c.http.connect_timeout = Duration::from_millis(500);
    });
    let rig = Rig::bare_with("plan offline", plan);
    let mut b = rig.backend();
    b.sign_in(CHATGPT);
    let deadline = Instant::now() + Duration::from_secs(15);
    while b.agents()[0].notice.is_none() {
        b.poll();
        assert!(Instant::now() < deadline, "the failure is reported");
        std::thread::sleep(Duration::from_millis(10));
    }
    let a = &b.agents()[0];
    assert_eq!(a.state, AgentState::SignInNeeded);
    assert!(
        a.notice.as_ref().unwrap().contains("Network error"),
        "{:?}",
        a.notice
    );
}

/// The chat crate's proxy switch is process-wide: one test at a time.
static PROXY: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Puts the proxy switch back, also when the test fails.
struct Reset;

impl Drop for Reset {
    fn drop(&mut self) {
        kit::FAKE_PROXY_ALL.store(0, std::sync::atomic::Ordering::SeqCst);
    }
}

#[test]
fn chatgpt_behind_a_proxy_says_proxies_are_not_supported() {
    let _one = PROXY.lock().unwrap_or_else(|p| p.into_inner());
    let rig = signed_in_rig("plan proxy");
    let mut config = rig.config();
    config.api.base_url = oc::http::Url::parse("http://127.0.0.1:9/v1").unwrap();
    let mut b = LiveBackend::new(config);
    // The sign-in is read on a worker thread first.
    let deadline = Instant::now() + Duration::from_secs(20);
    while !b.plan_ready() {
        b.poll();
        assert!(Instant::now() < deadline, "the sign-in is read");
        std::thread::sleep(Duration::from_millis(5));
    }
    b.plan.models = vec![oc::ModelInfo {
        slug: "m".into(),
        display_name: "M".into(),
    }];
    b.plan.models_asked = true;
    kit::FAKE_PROXY_ALL.store(2, std::sync::atomic::Ordering::SeqCst);
    Ask::new(CHATGPT, "Hi").send(&mut b, &sample());
    let events = pump(&mut b, turn_end);
    kit::FAKE_PROXY_ALL.store(0, std::sync::atomic::Ordering::SeqCst);
    let (kind, message) = error_of(&events);
    assert_eq!(kind, ErrorKind::Failed);
    assert!(message.contains("proxy"), "{message}");
}

#[test]
fn chatgpt_runs_the_same_tools_on_the_snapshot_and_proposes() {
    let rig = signed_in_rig("plan route");
    let mut b = rig.backend();
    models(&mut b);
    let project = sample();
    // A tool call, then the answer.
    rig.plan.queue(kit::calls_reply(&[(
        "call_1",
        "openxer_load",
        r#"{"critical":true,"fields":["task_code","name"],"limit":5}"#,
    )]));
    rig.plan
        .queue(kit::text_reply("PM0920 finishes the project."));
    let mut ask = Ask::new(CHATGPT, "What is critical?");
    ask.selection = Selection::One("PM0920".into());
    ask.send(&mut b, &project);
    let events = pump(&mut b, turn_end);
    assert!(events.iter().any(|e| matches!(e,
        AgentEvent::ToolStarted { tool, target, .. } if tool == "read schedule" && target == "critical activities")));
    assert!(events
        .iter()
        .any(|e| matches!(e, AgentEvent::ToolFinished { ok: true, .. })));
    assert!(texts(&events).contains("PM0920"));
    assert_eq!(
        events.iter().find_map(|e| match e {
            AgentEvent::TurnFinished { usage } => *usage,
            _ => None,
        }),
        Some(Usage {
            input: 150 + 100,
            output: 20 + 10
        }),
        "the usage line sums the turn's requests"
    );
    // The tool really read the snapshot: its result went back to the model.
    let requests = rig.plan.requests();
    let output = requests[1]["input"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["type"] == "function_call_output")
        .unwrap()["output"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        output.contains("PM0920") && output.contains("\"ok\":true"),
        "{output}"
    );
    // The same tool definitions as the agent host's MCP server (Ask mode).
    let names: Vec<String> = requests[0]["tools"][0]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    let mut expected: Vec<String> = chat_tools::tools(&chat_tools::ChatContext {
        snapshot: Some(std::path::PathBuf::from("snap.xer")),
        mode: ChatMode::Ask,
        playbooks_dir: None,
        ui_actions: Some(std::path::PathBuf::from("ui-actions.jsonl")),
    })
    .iter()
    .map(|t| t.name.to_string())
    .collect();
    expected.sort();
    assert_eq!(names, expected);
    // Edit mode: a proposal through openxer_apply, applied on the project.
    let changes = r#"{"changes":{"format":"openxer-changes/1","operations":[{"op":"set_activity","activity":"PM0910","field":"activity_name","value":"Hand over as-built drawings"}]}}"#;
    rig.plan
        .queue(kit::calls_reply(&[("call_2", "openxer_apply", changes)]));
    rig.plan.queue(kit::text_reply("I proposed the new name."));
    let mut ask = Ask::new(CHATGPT, "Rename PM0910");
    ask.mode = Mode::Edit;
    ask.selection = Selection::One("PM0920".into());
    ask.send(&mut b, &project);
    let events = pump(&mut b, turn_end);
    let (id, rows) = proposal(&events);
    assert!(
        rows.iter()
            .any(|r| r.activity == "PM0910" && r.new == "Hand over as-built drawings"),
        "{rows:?}"
    );
    let Ok(Applied::Project(next, _)) = b.apply(id, &project, 1) else {
        panic!("applies")
    };
    assert_eq!(name_of(&next, "PM0910"), "Hand over as-built drawings");
    // Token rules: the instructions (preamble) stay the same; the second
    // message carries no preamble and no unchanged selection; the third
    // tells of the apply.
    rig.plan.queue(kit::text_reply("Noted."));
    let mut ask = Ask::new(CHATGPT, "Thanks");
    ask.selection = Selection::One("PM0920".into());
    ask.revision = 2;
    ask.send(&mut b, &next);
    pump(&mut b, turn_end);
    let requests = rig.plan.requests();
    let first = &requests[0]["instructions"];
    assert!(first.as_str().unwrap().contains("Open project:"));
    // Requests 0-1 and 4 are in Ask mode, 2-3 in Edit: the prefix is the
    // same for a mode (the mode line is part of the one prompt).
    let of = |n: usize| &requests[n]["instructions"];
    assert_eq!(of(2), of(3));
    assert_ne!(of(0), of(2));
    assert!([1, 4].iter().all(|n| of(*n) == first), "a stable prefix");
    let users: Vec<String> = requests.last().unwrap()["input"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["role"] == "user")
        .map(|i| i["content"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(users[0], "What is critical?");
    assert_eq!(users[1], "Rename PM0910");
    assert!(
        users[2].starts_with("Context: The user applied the proposed changes"),
        "{}",
        users[2]
    );
    assert!(
        !users[2].contains("Selected now"),
        "the selection did not change"
    );
    assert_eq!(b.snapshot_writes, 2);
    b.end();
}

#[test]
fn chatgpt_errors_are_plain_status_lines() {
    let rig = signed_in_rig("plan errors");
    let mut b = rig.backend();
    models(&mut b);
    let project = sample();
    rig.plan.queue(kit::Reply::json(
        429,
        &serde_json::json!({"error": {"code": oc::USAGE_LIMIT_CODE, "message": "limit"}}),
    ));
    Ask::new(CHATGPT, "Hi").send(&mut b, &project);
    let (kind, message) = error_of(&pump(&mut b, turn_end));
    assert_eq!(kind, ErrorKind::UsageLimit);
    assert_eq!(message, oc::ui::USAGE_LIMIT_BODY);
    assert_eq!(rig.plan.requests().len(), 1, "nothing is retried");
    // Signed out on the service: a second 401.
    rig.plan
        .queue(kit::Reply::json(401, &serde_json::json!({})));
    rig.plan
        .queue(kit::Reply::json(401, &serde_json::json!({})));
    Ask::new(CHATGPT, "Hi").send(&mut b, &project);
    let (kind, _) = error_of(&pump(&mut b, turn_end));
    assert_eq!(kind, ErrorKind::NotSignedIn);
    // Not signed in at all.
    let rig = Rig::bare("plan signed out");
    let mut b = rig.backend();
    Ask::new(CHATGPT, "Hi").send(&mut b, &project);
    let (kind, _) = error_of(&b.poll());
    assert_eq!(kind, ErrorKind::NotSignedIn);
}

#[test]
fn chatgpt_stop_cancels_the_turn() {
    let rig = signed_in_rig("plan stop");
    let mut b = rig.backend();
    models(&mut b);
    let mut slow = kit::text_reply("slow answer");
    slow.stall = Duration::from_secs(3);
    rig.plan.queue(slow);
    Ask::new(CHATGPT, "Hi").send(&mut b, &sample());
    std::thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    b.stop();
    std::thread::sleep(Duration::from_millis(200));
    let events = b.poll();
    assert!(
        events.iter().all(|e| !matches!(
            e,
            AgentEvent::TurnFinished { .. } | AgentEvent::Error { .. }
        )),
        "{events:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    rig.plan.queue(kit::text_reply("Fresh answer"));
    Ask::new(CHATGPT, "Again").send(&mut b, &sample());
    assert!(texts(&pump(&mut b, turn_end)).contains("Fresh answer"));
}

// ----------------------------------------------------------- Check Setup

fn run_checks(b: &mut LiveBackend) -> Vec<checks::Row> {
    let started = Instant::now();
    assert!(b.check_setup(true).is_some());
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "starting never waits"
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let c = b.check_setup(false).unwrap();
        c.poll();
        if !c.running() {
            return c.rows.clone();
        }
        assert!(Instant::now() < deadline, "the checks end");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn find<'a>(rows: &'a [checks::Row], area: &str, text: &str) -> &'a checks::Row {
    rows.iter()
        .find(|r| r.area == area && r.text.contains(text))
        .unwrap_or_else(|| panic!("{area}: {text} in {rows:#?}"))
}

#[test]
fn check_setup_lists_passes_and_fails_with_their_fixes() {
    // A fresh PC: nothing installed, ChatGPT signed out.
    let rig = Rig::bare("checks fresh");
    let mut b = rig.backend();
    let rows = run_checks(&mut b);
    let r = find(&rows, "ChatGPT", "Not signed in");
    assert_eq!(
        (r.mark, r.fix.clone()),
        (
            checks::Mark::Fail,
            Some(checks::Fix::SignIn(CHATGPT.into()))
        )
    );
    let r = find(&rows, "Claude Code", "Not installed");
    assert!(
        matches!(&r.fix, Some(checks::Fix::Install { url, label }) if url == CLAUDE_CODE_URL && label == "Get Claude Code...")
    );
    let r = find(&rows, "Codex", "Not installed");
    assert!(matches!(&r.fix, Some(checks::Fix::Install { url, .. }) if url == CODEX_URL));
    assert_eq!(
        find(&rows, "This PC", "Reaches 127.0.0.1").mark,
        checks::Mark::Pass
    );
    assert_eq!(
        find(&rows, "This PC", "openxer-cli answers").mark,
        checks::Mark::Pass
    );
    // Everything set up, one old version, Claude Code signed out, a
    // personal instructions file.
    let rig = Rig::new("checks mixed");
    rig.sign_in_chatgpt();
    rig.agents.with(Agent::ClaudeCode, |b| b.signed_in = false);
    rig.agents
        .with(Agent::Codex, |b| b.version = "codex-cli 0.120.0".into());
    rig.profile.codex_instructions("Be brief.");
    let mut b = rig.backend();
    let rows = run_checks(&mut b);
    assert_eq!(find(&rows, "ChatGPT", "Signed in").mark, checks::Mark::Pass);
    assert_eq!(
        find(&rows, "ChatGPT", "can be renewed").mark,
        checks::Mark::Pass
    );
    assert_eq!(
        find(&rows, "ChatGPT", "plan is allowed").mark,
        checks::Mark::Pass
    );
    assert_eq!(
        find(&rows, "Claude Code", "Installed at").mark,
        checks::Mark::Pass
    );
    assert_eq!(
        find(&rows, "Claude Code", "at or above the tested 2.1.330").mark,
        checks::Mark::Pass
    );
    assert_eq!(
        find(&rows, "Claude Code", "Not signed in").fix,
        Some(checks::Fix::SignIn(CLAUDE_CODE.into()))
    );
    assert_eq!(
        find(&rows, "Codex", "older than the tested 0.166.0").fix,
        Some(checks::Fix::Update(CODEX_URL.into()))
    );
    assert_eq!(
        find(&rows, "Codex", "personal instructions file").mark,
        checks::Mark::Note
    );
    // Unreachable hosts fail, a proxy is noted.
    let _one = PROXY.lock().unwrap_or_else(|p| p.into_inner());
    let rig = Rig::bare("checks offline");
    let mut config = rig.config();
    config.hosts = vec![oc::http::Url::parse("http://127.0.0.1:9/").unwrap()];
    let mut b = LiveBackend::new(config);
    kit::FAKE_PROXY_ALL.store(2, std::sync::atomic::Ordering::SeqCst);
    let rows = run_checks(&mut b);
    kit::FAKE_PROXY_ALL.store(0, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        find(&rows, "This PC", "Cannot reach 127.0.0.1").mark,
        checks::Mark::Fail
    );
    assert_eq!(
        find(&rows, "This PC", "proxy server is set up").mark,
        checks::Mark::Note
    );
}

#[test]
fn check_setup_can_be_cancelled_and_copies_nothing_private() {
    let rig = Rig::new("checks cancel");
    rig.sign_in_chatgpt();
    let mut b = rig.backend();
    let c = b.check_setup(true).unwrap();
    c.cancel();
    assert!(!c.running() && c.cancelled);
    let rows = run_checks(&mut b);
    let c = b.check_setup(false).unwrap();
    assert_eq!(c.rows, rows);
    let text = c.text();
    let home = rig.profile.env.home.clone().unwrap().display().to_string();
    assert!(
        rows.iter().any(|r| r.text.contains(&home)),
        "the rows name the install folder"
    );
    assert!(!text.contains(&home), "{text}");
    assert!(text.contains("%USERPROFILE%"), "{text}");
    assert!(!text.contains("pat@example.test"));
    for mark in kit::SECRET_MARKS {
        assert!(!text.contains(mark), "{mark} in {text}");
    }
    assert_eq!(
        checks::redact(
            "Mail Pat.Example@Example.test at C:\\USERS\\me\\x, c:\\users\\ME\\y",
            Some(Path::new("C:\\Users\\Me"))
        ),
        "Mail <email> at %USERPROFILE%\\x, %USERPROFILE%\\y"
    );
}

// ---------------------------------------------------- the whole window

/// The sample project in the app, Chat on the live backend over `rig`.
pub(crate) fn live_driver(name: &str, rig: &Rig) -> Driver {
    let mut p = sample();
    crate::scheduling::schedule_model(&mut p.model, &p.resources).unwrap();
    let mut d = Driver::new(name, p);
    d.app.chat.opener = rig.opener.clone();
    d.app.chat.backend = Box::new(rig.backend());
    d.frames(2);
    d
}

pub(crate) fn shown(d: &mut Driver, needle: &str) -> bool {
    d.texts().iter().any(|(t, _)| t.contains(needle))
}

/// Frames until `check` holds (10 s at most).
pub(crate) fn until(d: &mut Driver, what: &str, mut check: impl FnMut(&mut Driver) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !check(d) {
        assert!(Instant::now() < deadline, "{what}");
        d.frames(1);
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Scrolls the message list with the wheel until control `key` is in view,
/// as a user would, and clicks it.
pub(crate) fn reveal_click(d: &mut Driver, key: &str) {
    for _ in 0..40 {
        let list = d.rect("chat_list");
        let r = d.rect(key);
        if list.contains_rect(r) {
            break;
        }
        let up = r.top() < list.top();
        let _ = d.frame(vec![
            egui::Event::PointerMoved(list.center()),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::Vec2::new(0., if up { 60. } else { -60. }),
                modifiers: Default::default(),
            },
        ]);
        d.frames(1);
    }
    d.click(key);
}

fn ask_in_window(d: &mut Driver, text: &str) {
    d.click("chat_message");
    d.type_text(text);
    d.frames(1);
    d.key(Key::Enter);
    d.frames(2);
}

fn select_code(d: &mut Driver, code: &str) {
    let i = d
        .app
        .project
        .model
        .activities
        .iter()
        .position(|a| a.task_code == code)
        .unwrap();
    d.app.select_activities(&[i]);
    d.frames(1);
}

/// One route end to end in the window: F8, ask, the streamed answer with
/// tool rows and a linked Activity ID, a proposal applied as one undo
/// step, Undo and Redo, New Chat, closing (files and processes gone).
fn route_flow(agent: &str, rig: &Rig, answer: &str) {
    route_flow_with(agent, rig, answer, &|_| {});
}

/// [`route_flow`] with the window prepared first (a connection set up).
fn route_flow_with(agent: &str, rig: &Rig, answer: &str, prepare: &dyn Fn(&mut Driver)) {
    let mut d = live_driver(&format!("chat-live-{agent}"), rig);
    prepare(&mut d);
    d.app.chat.agent = Some(agent.into());
    d.app.chat.picked = true;
    // OpenAI's welcome box has its own test.
    d.app.chat.welcomed = Some(true);
    d.key(Key::F8);
    d.frames(3);
    assert!(d.app.chat.open, "F8 opens Chat");
    until(&mut d, "the agent is ready", |d| {
        d.app.chat.backend.agents().iter().any(|a| {
            a.id == agent && a.state.usable() && (agent != CHATGPT || !a.models.is_empty())
        })
    });
    d.frames(2);
    select_code(&mut d, "PM0920");
    ask_in_window(&mut d, "What drives the finish?");
    assert!(
        d.app.chat.conv.running || !d.app.chat.conv.is_empty(),
        "sent: {:?} / {}",
        d.app.chat.conv.items,
        d.app.status
    );
    until(&mut d, "the answer ends", |d| !d.app.chat.conv.running);
    assert!(shown(&mut d, answer), "the answer shows");
    assert!(shown(&mut d, "read schedule"), "the tool row");
    assert!(shown(&mut d, "tokens"), "the usage line");
    assert!(d.has("chat_link_PM0920"), "the Activity ID is a link");
    d.click("chat_link_PM0920");
    let i = d
        .app
        .project
        .model
        .activities
        .iter()
        .position(|a| a.task_code == "PM0920")
        .unwrap();
    assert_eq!(d.app.selected, Some(i));
    // The follow-up, in Edit mode: a proposal.
    d.click("chat_mode");
    d.click("chat_mode_item_1");
    let before = d.app.project.clone();
    let undo = d.app.history.counts().0;
    ask_in_window(&mut d, "Rename PM0910");
    until(&mut d, "the proposal", |d| {
        d.app.chat.conv.pending().is_some()
    });
    until(&mut d, "the reply ends", |d| !d.app.chat.conv.running);
    assert!(shown(&mut d, "Nothing is written until you apply"));
    d.click("chat_apply");
    d.frames(2);
    assert!(shown(&mut d, "Applied."));
    let renamed = name_of(&d.app.project, "PM0910");
    assert_ne!(
        renamed,
        name_of(&before, "PM0910"),
        "Apply changed the activity"
    );
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");
    d.frames(2);
    d.click("undo");
    d.frames(2);
    assert_eq!(
        name_of(&d.app.project, "PM0910"),
        name_of(&before, "PM0910")
    );
    assert!(
        d.app.project.model == before.model,
        "Undo restores the exact state"
    );
    d.click("redo");
    d.frames(2);
    assert_eq!(
        name_of(&d.app.project, "PM0910"),
        renamed,
        "Redo applies it again"
    );
    // New Chat ends the conversation: its folder goes.
    let dirs = rig.work_dirs();
    assert_eq!(dirs.len(), 1, "{dirs:?}");
    d.click("chat_new");
    assert!(d.app.chat.conv.is_empty());
    until(&mut d, "the folder goes", |_| rig.work_dirs().is_empty());
    // Closing the window ends everything.
    d.key(Key::Escape);
    assert!(!d.app.chat.open);
    until(&mut d, "no agent process is left", |_| {
        rig.agents.alive() == 0
    });
    assert!(rig.work_dirs().is_empty());
}

#[test]
fn the_window_runs_claude_code_end_to_end() {
    let rig = Rig::new("window claude");
    route_flow(CLAUDE_CODE, &rig, "PM0920");
}

#[test]
fn the_window_runs_codex_end_to_end() {
    let rig = Rig::new("window codex");
    route_flow(CODEX, &rig, "PM0920");
}

#[test]
fn the_window_runs_chatgpt_end_to_end() {
    let rig = signed_in_rig("window chatgpt");
    rig.plan.queue(kit::calls_reply(&[(
        "call_1",
        "openxer_load",
        r#"{"critical":true,"fields":["task_code","name"],"limit":5}"#,
    )]));
    rig.plan
        .queue(kit::text_reply("**PM0920** finishes the project."));
    let changes = r#"{"changes":{"format":"openxer-changes/1","operations":[{"op":"set_activity","activity":"PM0910","field":"activity_name","value":"Hand over as-built drawings"}]}}"#;
    rig.plan
        .queue(kit::calls_reply(&[("call_2", "openxer_apply", changes)]));
    rig.plan.queue(kit::text_reply("I proposed the new name."));
    route_flow(CHATGPT, &rig, "finishes the project");
}

#[test]
fn a_manual_edit_refreshes_a_waiting_proposal_and_the_next_snapshot() {
    let rig = Rig::new("window stale");
    let mut d = live_driver("chat-live-stale", &rig);
    d.app.chat.agent = Some(CLAUDE_CODE.into());
    d.app.chat.picked = true;
    d.key(Key::F8);
    until(&mut d, "ready", |d| {
        d.app
            .chat
            .backend
            .agents()
            .iter()
            .any(|a| a.id == CLAUDE_CODE && a.state.usable())
    });
    d.click("chat_mode");
    d.click("chat_mode_item_1");
    ask_in_window(&mut d, "first");
    until(&mut d, "reply", |d| !d.app.chat.conv.running);
    ask_in_window(&mut d, "Rename PM0910");
    until(&mut d, "proposal", |d| {
        d.app.chat.conv.pending().is_some() && !d.app.chat.conv.running
    });
    // The user renames PM0910 by hand first (one undo step of its own).
    let time = d.ctx.input(|i| i.time);
    let before = d.app.project.clone();
    let rev = d.app.records_revision;
    let id = d
        .app
        .project
        .model
        .activities
        .iter()
        .find(|a| a.task_code == "PM0910")
        .unwrap()
        .task_id;
    d.app.project.details.get_mut(&id).unwrap().name = "Typed by hand".into();
    d.app.edit_step(before, rev, time);
    d.frames(2);
    d.click("chat_apply");
    d.frames(2);
    assert!(
        shown(&mut d, "The project changed since this was proposed."),
        "the refreshed rows wait"
    );
    assert!(
        shown(&mut d, "Typed by hand"),
        "the rows read the current name"
    );
    assert_eq!(
        name_of(&d.app.project, "PM0910"),
        "Typed by hand",
        "nothing applied yet"
    );
    d.click("chat_apply");
    d.frames(2);
    assert_eq!(
        name_of(&d.app.project, "PM0910"),
        "Submit as-built drawings and O&M manuals"
    );
    d.app.close_chat();
}

#[test]
fn closing_the_project_ends_the_conversation() {
    let rig = Rig::new("window close project");
    let mut d = live_driver("chat-live-project", &rig);
    d.app.chat.agent = Some(CLAUDE_CODE.into());
    d.app.chat.picked = true;
    d.key(Key::F8);
    until(&mut d, "ready", |d| {
        d.app
            .chat
            .backend
            .agents()
            .iter()
            .any(|a| a.id == CLAUDE_CODE && a.state.usable())
    });
    ask_in_window(&mut d, "first");
    until(&mut d, "reply", |d| !d.app.chat.conv.running);
    assert_eq!(rig.work_dirs().len(), 1);
    // Another project in its place.
    d.app.project.source_name = "another.xer".into();
    d.frames(2);
    assert!(d.app.chat.conv.is_empty(), "the conversation ended");
    until(&mut d, "its folder goes", |_| rig.work_dirs().is_empty());
}

// --------------------------------------------------- fresh-user matrix

fn open_live(d: &mut Driver) {
    d.home();
    d.click("menu_bar_ai_agents");
    d.click(crate::chat::COMMAND);
    d.frames(3);
}

#[test]
fn a_fresh_pc_shows_how_to_connect_and_each_action_works() {
    let rig = Rig::bare("fresh nothing");
    let mut d = live_driver("chat-fresh", &rig);
    open_live(&mut d);
    until(&mut d, "detection", |d| {
        !d.app
            .chat
            .backend
            .agents()
            .iter()
            .any(|a| a.state == AgentState::Checking)
    });
    d.frames(2);
    let texts: Vec<String> = d.texts().into_iter().map(|(t, _)| t).collect();
    assert!(shown(&mut d, "Choose how to connect"), "{texts:?}");
    assert!(shown(&mut d, "Use your ChatGPT plan."));
    // ChatGPT first, then the API key or local model, Claude Code, Codex.
    assert!(
        d.rect(&crate::chat::list::setup_key(0, false)).top()
            < d.rect(&crate::chat::list::setup_key(1, false)).top()
    );
    reveal_click(&mut d, &crate::chat::list::setup_key(2, false));
    assert!(shown(&mut d, "Needs the Claude Code app"));
    assert_eq!(rig.opened.lock().unwrap().last().unwrap(), CLAUDE_CODE_URL);
    reveal_click(&mut d, &crate::chat::list::setup_key(3, false));
    assert_eq!(rig.opened.lock().unwrap().last().unwrap(), CODEX_URL);
    // Check Again looks once more.
    let before = rig.agents.launches().len();
    reveal_click(&mut d, &crate::chat::list::setup_key(2, true));
    d.frames(3);
    assert!(rig.agents.launches().len() >= before);
    // ChatGPT: consent first (Cancel is the default), then the sign-in.
    reveal_click(&mut d, &crate::chat::list::setup_key(0, false));
    assert_eq!(d.app.chat.consent.as_deref(), Some(CHATGPT));
    assert!(shown(&mut d, "Chat with your ChatGPT plan"));
    d.click("chat_consent_continue with chatgpt");
    until(&mut d, "signed in", |d| {
        d.app.chat.backend.agents()[0].state.usable()
    });
    d.frames(2);
    // The welcome box, once.
    d.frames(2);
    let texts: Vec<String> = d.texts().into_iter().map(|(t, _)| t).collect();
    assert!(d.has("chat_welcome_got it"), "{texts:?}");
    assert!(
        shown(&mut d, "Eligible usage in this app uses your ChatGPT plan."),
        "{texts:?}"
    );
    d.click("chat_welcome_got it");
    d.frames(2);
    assert!(!d.has("chat_welcome_got it"));
    // Only one route is ready: straight to its empty state.
    assert!(!shown(&mut d, "Choose how to connect"));
    assert!(shown(&mut d, "Signed in to ChatGPT as pat@example.test."));
    assert!(
        shown(&mut d, openxer_chat::ui::PLAN_INDICATOR),
        "the plan label near the composer"
    );
    d.click("chat_plan_manage_usage");
    assert_eq!(
        rig.opened.lock().unwrap().last().unwrap(),
        openxer_chat::USAGE_SETTINGS_URL
    );
    // Ask, once the model list is in.
    until(&mut d, "models", |d| {
        !d.app.chat.backend.agents()[0].models.is_empty()
    });
    rig.plan.queue(kit::text_reply("There are 187 activities."));
    ask_in_window(&mut d, "How many activities?");
    until(&mut d, "answer", |d| !d.app.chat.conv.running);
    assert!(shown(&mut d, "There are 187 activities."));
    // The usage limit: Manage usage is the main action.
    rig.plan.queue(kit::Reply::json(
        429,
        &serde_json::json!({"error": {"code": oc::USAGE_LIMIT_CODE}}),
    ));
    ask_in_window(&mut d, "Again");
    until(&mut d, "limit", |d| !d.app.chat.conv.running);
    assert!(shown(&mut d, "Usage limit reached."));
    assert_eq!(
        d.rect("chat_manage_usage").size(),
        crate::dialog_kit::BUTTON
    );
    d.click("chat_manage_usage");
    assert_eq!(
        rig.opened.lock().unwrap().last().unwrap(),
        openxer_chat::USAGE_SETTINGS_URL
    );
    // The welcome box does not come back after a restart of the window.
    d.app.close_chat();
    d.app.chat.welcomed = None;
    open_live(&mut d);
    d.frames(3);
    assert!(!d.has("chat_welcome_got it"));
    d.app.close_chat();
}

#[test]
fn claude_code_installed_but_signed_out_signs_in_from_the_setup_list() {
    let mut rig = Rig::bare("fresh claude signed out");
    rig.profile.install(Agent::ClaudeCode, Shape::Npm);
    rig.agents.set(
        Agent::ClaudeCode,
        Behaviour {
            signed_in: false,
            ..Behaviour::of(Agent::ClaudeCode)
        },
    );
    let mut d = live_driver("chat-fresh-claude", &rig);
    open_live(&mut d);
    until(&mut d, "detection", |d| {
        !d.app
            .chat
            .backend
            .agents()
            .iter()
            .any(|a| a.state == AgentState::Checking)
    });
    d.frames(2);
    assert!(shown(&mut d, "Choose how to connect"));
    reveal_click(&mut d, &crate::chat::list::setup_key(2, false));
    assert!(shown(&mut d, "Signing in in your browser"));
    rig.agents.with(Agent::ClaudeCode, |b| b.signed_in = true);
    *rig.agents.sign_in_done.lock().unwrap() = Some(true);
    until(&mut d, "ready", |d| shown(d, "Claude Code is ready."));
    assert!(
        !shown(&mut d, "Choose how to connect"),
        "the first ready route is chosen"
    );
    d.app.close_chat();
}

#[test]
fn a_remembered_choice_wins_when_two_are_ready_and_falls_back_when_it_is_gone() {
    // Two ready, Codex remembered.
    let rig = Rig::new("fresh two ready");
    let mut d = live_driver("chat-fresh-two", &rig);
    d.app.chat.agent = Some(CODEX.into());
    open_live(&mut d);
    until(&mut d, "detection", |d| {
        !d.app
            .chat
            .backend
            .agents()
            .iter()
            .any(|a| a.state == AgentState::Checking)
    });
    d.frames(2);
    assert!(shown(&mut d, "Codex is ready."));
    d.app.close_chat();
    // The remembered agent was removed since: the ready one is used.
    let mut rig = Rig::bare("fresh removed");
    rig.profile.install(Agent::ClaudeCode, Shape::Exe);
    rig.agents
        .set(Agent::ClaudeCode, Behaviour::of(Agent::ClaudeCode));
    let mut d = live_driver("chat-fresh-removed", &rig);
    d.app.chat.agent = Some(CODEX.into());
    open_live(&mut d);
    until(&mut d, "detection", |d| {
        !d.app
            .chat
            .backend
            .agents()
            .iter()
            .any(|a| a.state == AgentState::Checking)
    });
    d.frames(2);
    assert!(shown(&mut d, "Claude Code is ready."));
    // The setup list stays reachable from the Agent row.
    d.click("chat_agent");
    let n = d.app.chat.backend.agents().len();
    d.click(&format!("chat_agent_item_{n}"));
    d.frames(2);
    assert!(shown(&mut d, "Choose how to connect"));
    reveal_click(&mut d, &crate::chat::list::setup_key(2, false));
    d.frames(2);
    assert!(shown(&mut d, "Claude Code is ready."), "Use picks it");
    d.app.close_chat();
}

#[test]
fn check_setup_runs_from_the_window_and_copies_its_rows() {
    let rig = Rig::bare("window checks");
    let mut d = live_driver("chat-checks", &rig);
    open_live(&mut d);
    until(&mut d, "detection", |d| {
        !d.app
            .chat
            .backend
            .agents()
            .iter()
            .any(|a| a.state == AgentState::Checking)
    });
    d.frames(2);
    d.click("chat_check_setup");
    assert!(d.app.chat.check.is_some());
    until(&mut d, "the checks end", |d| shown(d, "Done:"));
    assert_eq!(
        d.rect("chat_checks_list_row_0").height(),
        crate::dialog_kit::ROW
    );
    // Select the ChatGPT row: its fix is Sign In.
    d.click("chat_checks_list_row_0");
    d.frames(1);
    assert!(shown(&mut d, "Sign In..."));
    d.click("chat_checks_copy");
    assert!(d.app.status.contains("copied"));
    d.click("chat_checks_fix");
    d.frames(2);
    assert!(d.app.chat.check.is_none(), "the fix took over");
    assert_eq!(
        d.app.chat.consent.as_deref(),
        Some(CHATGPT),
        "Sign In asks its consent first"
    );
    d.key(Key::Escape);
    d.frames(2);
    // Escape closes Check Setup too.
    d.click("chat_check_setup");
    d.frames(2);
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.chat.check.is_none());
    d.app.close_chat();
}

#[test]
fn nothing_in_a_test_build_opens_a_browser_or_reaches_a_real_host() {
    // The app's own default backend in a test build: an empty profile, an
    // in-memory sign-in and endpoints on a closed local port.
    let mut p = sample();
    crate::scheduling::schedule_model(&mut p.model, &p.resources).unwrap();
    let mut d = Driver::new("chat-default-backend", p);
    let (opener, opened) = recording_opener();
    d.app.chat.opener = opener;
    open_live(&mut d);
    assert!(
        !d.app.chat.backend.is_offline(),
        "opening Chat makes the real backend"
    );
    until(&mut d, "detection", |d| {
        !d.app
            .chat
            .backend
            .agents()
            .iter()
            .any(|a| a.state == AgentState::Checking)
    });
    let agents = d.app.chat.backend.agents();
    assert!(agents.iter().all(|a| !a.state.usable()));
    d.app.close_chat();
    assert!(opened.lock().unwrap().is_empty());
    let _ = egui::Context::default();
}

// ------------------------------------------------------------ live runs

/// One real agent through the real backend and window, headless, on the
/// synthetic sample project only (a temp workspace; the agent reads a
/// snapshot in its own temp folder): ask and answer, then propose, Apply,
/// check the change and Undo. Prints the time and tokens of each reply.
fn live_run(agent: &str) {
    let cli = openxer_automation::connect::resolve_cli(std::env::current_exe().ok().as_deref());
    let Some(cli) = cli.filter(|c| c.is_file()) else {
        println!("skipped: build openxer-cli first (cargo build -p openxer-automation)");
        return;
    };
    let (opener, _opened) = recording_opener();
    let temp = std::env::temp_dir().join("openxer-chat-live-runs");
    std::fs::create_dir_all(&temp).unwrap();
    let config = Config {
        env: Env::from_process(Some(cli)),
        spawner: Arc::new(host::SystemSpawner),
        auth: Arc::new(oc::Auth::new(
            oc::AuthConfig::openai(),
            Arc::new(oc::MemoryStore::new()),
        )),
        api: oc::ApiConfig::openai(),
        opener: opener.clone(),
        temp: temp.clone(),
        hosts: vec![],
        ready_timeout: compat::READY_TIMEOUT,
        // Never the maintainer's own instructions file.
        instructions: test_instructions_path(),
        keys: Arc::new(oc::MemoryStore::new()),
    };
    let mut p = sample();
    crate::scheduling::schedule_model(&mut p.model, &p.resources).unwrap();
    let mut d = Driver::new(&format!("chat-live-run-{agent}"), p);
    d.app.chat.opener = opener;
    d.app.chat.backend = Box::new(LiveBackend::new(config));
    d.app.chat.agent = Some(agent.into());
    d.app.chat.picked = true;
    d.app.chat.model = ModelChoice::LowerCost;
    d.key(Key::F8);
    let wait = |d: &mut Driver, what: &str, secs: u64, check: &dyn Fn(&Driver) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(secs);
        while !check(d) {
            assert!(Instant::now() < deadline, "{what}");
            d.frames(1);
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    wait(&mut d, "the agent is found", 60, &|d| {
        d.app
            .chat
            .backend
            .agents()
            .iter()
            .any(|a| a.id == agent && a.state != AgentState::Checking)
    });
    let state = d
        .app
        .chat
        .backend
        .agents()
        .into_iter()
        .find(|a| a.id == agent)
        .unwrap()
        .state;
    assert!(state.usable(), "{agent} is not ready on this PC: {state:?}");
    let ask = |d: &mut Driver, text: &str| {
        d.click("chat_message");
        d.type_text(text);
        d.frames(1);
        d.key(Key::Enter);
        d.frames(2);
    };
    let started = Instant::now();
    ask(
        &mut d,
        "Which activity finishes last? Answer in one sentence with its Activity ID.",
    );
    wait(&mut d, "the answer", 300, &|d| !d.app.chat.conv.running);
    let answer_time = started.elapsed();
    let answer_usage = d.app.chat.conv.usage;
    let answered = d.app.chat.conv.items.iter().any(|i| matches!(i, crate::chat::model::Item::Reply { text, .. } if text.contains("PM0920") || text.contains("PM0910")));
    // (PM0910 and PM0920 finish together in the sample.)
    assert!(answered, "{:#?}", d.app.chat.conv.items);
    d.click("chat_mode");
    d.click("chat_mode_item_1");
    let before = d.app.project.model.clone();
    let started = Instant::now();
    ask(&mut d, "Rename activity PM0910 to \"Submit as-built drawings and O&M manuals\". Propose it with openxer_apply.");
    wait(&mut d, "the proposal", 300, &|d| {
        !d.app.chat.conv.running && d.app.chat.conv.pending().is_some()
    });
    let propose_time = started.elapsed();
    let propose_usage = d.app.chat.conv.usage;
    d.click("chat_apply");
    d.frames(2);
    assert_eq!(
        name_of(&d.app.project, "PM0910"),
        "Submit as-built drawings and O&M manuals"
    );
    d.click("undo");
    d.frames(2);
    assert!(d.app.project.model == before, "Undo restores it");
    d.app.close_chat();
    println!(
        "LIVE {agent}: answer {:.1} s, {answer_usage:?}; proposal {:.1} s, {propose_usage:?}",
        answer_time.as_secs_f64(),
        propose_time.as_secs_f64()
    );
}

#[test]
#[ignore = "runs the real Claude Code on this PC (uses its sign-in and usage)"]
fn live_claude_code_answers_proposes_and_applies() {
    live_run(CLAUDE_CODE);
}

#[test]
#[ignore = "runs the real Codex on this PC (uses its sign-in and usage)"]
fn live_codex_answers_proposes_and_applies() {
    live_run(CODEX);
}

// ------------------------------------------ tools reached, one prompt

#[test]
fn an_answer_written_without_openxers_tools_is_not_shown() {
    let rig = Rig::new("tools down");
    rig.agents.with(Agent::ClaudeCode, |b| {
        b.turns = vec![vec![
            r#"{"type":"system","subtype":"init","session_id":"00000000-0000-4000-8000-000000000021","model":"m","mcp_servers":[{"name":"openxer","status":"failed"}]}"#.to_string(),
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Probably A1010 drives the finish."}]}}"#.to_string(),
            r#"{"type":"result","subtype":"success","usage":{"input_tokens":10,"output_tokens":5}}"#.to_string(),
        ]]
        .into();
    });
    rig.agents.with(Agent::Codex, |b| {
        b.turns = vec![vec![
            r#"{"type":"thread.started","thread_id":"00000000-0000-4000-8000-000000000022"}"#.to_string(),
            r#"{"type":"item.completed","item":{"id":"item_0","type":"error","message":"MCP client for `openxer` failed to start: request timed out"}}"#.to_string(),
            r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"Probably A1010 drives the finish."}}"#.to_string(),
            r#"{"type":"turn.completed","usage":{"input_tokens":10,"output_tokens":5}}"#.to_string(),
        ]]
        .into();
    });
    let mut b = rig.backend();
    detected(&mut b);
    let project = sample();
    for agent in [CLAUDE_CODE, CODEX] {
        b.new_conversation(agent);
        Ask::new(agent, "What drives the finish?").send(&mut b, &project);
        let events = pump(&mut b, turn_end);
        assert!(
            texts(&events).is_empty(),
            "{agent}: no ungrounded answer: {events:?}"
        );
        assert!(
            !events.iter().any(|e| matches!(e, AgentEvent::TextDelta(_))),
            "{agent}: {events:?}"
        );
        assert_eq!(
            error_of(&events),
            (ErrorKind::Failed, TOOLS_UNREACHABLE.to_string()),
            "{agent}"
        );
    }
    // Codex is told its server is required.
    let codex = rig
        .agents
        .launches()
        .into_iter()
        .find(|l| l.args.first().is_some_and(|a| a == "exec"))
        .unwrap();
    assert!(
        codex
            .args
            .iter()
            .any(|a| a == "mcp_servers.openxer.required=true"),
        "{:?}",
        codex.args
    );
    b.end();
}

#[test]
fn every_route_gets_the_one_prompt_with_the_users_instructions() {
    let rig = signed_in_rig("one prompt");
    let mut rig = rig;
    rig.profile.install(Agent::ClaudeCode, Shape::Npm);
    rig.agents
        .set(Agent::ClaudeCode, Behaviour::of(Agent::ClaudeCode));
    let file = rig.instructions();
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "Always give dates as DD-Mon-YY.").unwrap();
    let mut b = rig.backend();
    detected(&mut b);
    models(&mut b);
    let project = sample();
    // Claude Code reads it from its work folder.
    Ask::new(CLAUDE_CODE, "When does it finish?").send(&mut b, &project);
    // The agent starts on a worker thread; its prompt file stays while
    // the conversation lasts.
    pump(&mut b, turn_end);
    let launch = rig
        .agents
        .launches()
        .into_iter()
        .find(|l| l.args.contains(&"-p".to_string()))
        .unwrap();
    let dir = launch.cwd.unwrap();
    let prompt = std::fs::read_to_string(dir.join("openxer-chat-instructions.md")).unwrap();
    let expected =
        chat_prompt::system_prompt(ChatMode::Ask, Some("Always give dates as DD-Mon-YY."));
    assert_eq!(prompt, expected);
    assert!(prompt.starts_with(&chat_prompt::CORE[..40]));
    // ChatGPT gets the same text as its instructions (Edit mode here).
    b.new_conversation(CHATGPT);
    rig.plan.queue(kit::text_reply("26-Apr-27."));
    let mut ask = Ask::new(CHATGPT, "When does it finish?");
    ask.mode = Mode::Edit;
    ask.send(&mut b, &project);
    pump(&mut b, turn_end);
    let sent = rig.plan.requests();
    let instructions = sent.last().unwrap()["instructions"]
        .as_str()
        .unwrap()
        .to_string();
    let edit = chat_prompt::system_prompt(ChatMode::Edit, Some("Always give dates as DD-Mon-YY."));
    assert!(instructions.starts_with(&edit), "{instructions}");
    assert!(instructions.contains("Open project:"), "then the preamble");
    // A file over the limit is cut, and the conversation says so once.
    std::fs::write(&file, "x".repeat(chat_prompt::USER_LIMIT + 50)).unwrap();
    b.new_conversation(CHATGPT);
    rig.plan.queue(kit::text_reply("ok"));
    Ask::new(CHATGPT, "Hi").send(&mut b, &project);
    let events = pump(&mut b, turn_end);
    let cut = events
        .iter()
        .filter(|e| matches!(e, AgentEvent::Notice(n) if n.contains("was cut at 4000 characters")))
        .count();
    assert_eq!(cut, 1, "{events:?}");
    b.end();
}

#[test]
fn instructions_opens_the_users_file_made_from_the_starter() {
    let rig = Rig::new("instructions button");
    let mut d = live_driver("chat-instructions", &rig);
    let file = rig.instructions();
    d.app.chat.instructions = file.clone();
    d.key(Key::F8);
    d.frames(3);
    d.click("chat_options");
    d.click("chat_options_instructions");
    d.frames(2);
    assert!(file.is_file(), "the starter was made");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        chat_prompt::STARTER
    );
    assert_eq!(
        rig.opened.lock().unwrap().last().unwrap(),
        &file.display().to_string()
    );
    // An existing file is never overwritten.
    std::fs::write(&file, "Mine.").unwrap();
    d.click("chat_options");
    d.click("chat_options_instructions");
    d.frames(2);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "Mine.");
    d.app.close_chat();
}

// ------------------------------------------------ API key or local model

use openxer_chat::compat::{self as cc, Connection, Preset};
use openxer_chat::testkit::completions as cmp;

const KEY: &str = "sk-TESTKEY-0123456789";

/// Sets the endpoint and waits for its key to be saved and read back.
fn saved(b: &mut LiveBackend, conn: Option<Connection>, key: Option<&str>) {
    let rx = b.set_endpoint(conn, key.map(oc::Secret::new));
    rx.recv_timeout(Duration::from_secs(20))
        .unwrap()
        .expect("saved");
    let deadline = Instant::now() + Duration::from_secs(20);
    while b.endpoint.saving.is_some() || b.endpoint.key_rx.is_some() {
        b.poll();
        assert!(Instant::now() < deadline, "the key state is read back");
        std::thread::sleep(Duration::from_millis(5));
    }
}

type Replies = Arc<std::sync::Mutex<std::collections::VecDeque<kit::Reply>>>;

/// A Chat Completions server on 127.0.0.1 with two models; replies are
/// queued per test.
struct EndpointFake {
    server: kit::FakeServer,
    replies: Replies,
}

impl EndpointFake {
    fn new() -> EndpointFake {
        let server = kit::FakeServer::start();
        server.route_json(
            "GET",
            "/v1/models",
            200,
            serde_json::json!({"data": [{"id": "model-a"}, {"id": "model-b"}]}),
        );
        let replies: Replies = Arc::default();
        let r = replies.clone();
        server.route("POST", "/v1/chat/completions", move |_| {
            r.lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| cmp::text_reply("(no reply queued)", true))
        });
        EndpointFake { server, replies }
    }
    fn queue(&self, r: kit::Reply) {
        self.replies.lock().unwrap().push_back(r);
    }
    /// A Custom connection to this server.
    fn conn(&self, needs_key: bool) -> Connection {
        let mut c = Connection::new(Preset::Custom);
        c.base_url = format!("http://127.0.0.1:{}/v1", self.port());
        c.needs_key = needs_key;
        c.model = "model-a".into();
        c
    }
    fn port(&self) -> u16 {
        self.server.url("/").port
    }
    fn requests(&self) -> Vec<serde_json::Value> {
        self.server
            .seen("POST", "/v1/chat/completions")
            .iter()
            .map(|s| s.json())
            .collect()
    }
}

#[test]
fn the_endpoint_route_answers_with_the_same_tools_and_the_one_prompt() {
    let rig = Rig::bare("endpoint route");
    let ep = EndpointFake::new();
    let mut b = rig.backend();
    assert_eq!(b.agents()[1].id, endpoint::ENDPOINT, "listed after ChatGPT");
    assert_eq!(b.agents()[1].state, AgentState::NotSetUp);
    let conn = ep.conn(true);
    saved(&mut b, Some(conn.clone()), Some(KEY));
    assert!(
        cc::has_key(rig.keys.as_ref(), &conn).unwrap(),
        "the key is stored"
    );
    let a = b.agents()[1].clone();
    assert_eq!(a.state, AgentState::Ready);
    assert!(a.privacy.contains("127.0.0.1"));
    // A tool call on the snapshot, then the answer (with usage).
    ep.queue(cmp::calls_reply(&[(
        "call_1",
        "openxer_load",
        r#"{"critical":true,"fields":["task_code","name"],"limit":5}"#,
    )]));
    ep.queue(cmp::text_reply("PM0920 finishes the project.", true));
    let project = sample();
    Ask::new(endpoint::ENDPOINT, "What is critical?").send(&mut b, &project);
    let events = pump(&mut b, turn_end);
    assert!(events
        .iter()
        .any(|e| matches!(e, AgentEvent::ToolStarted { tool, .. } if tool == "read schedule")));
    assert!(events
        .iter()
        .any(|e| matches!(e, AgentEvent::ToolFinished { ok: true, .. })));
    assert!(texts(&events).contains("PM0920"));
    assert!(events
        .iter()
        .any(|e| matches!(e, AgentEvent::TurnFinished { usage: Some(_) })));
    let sent = ep.requests();
    let system = sent[0]["messages"][0]["content"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        system.starts_with(&chat_prompt::system_prompt(ChatMode::Ask, None)),
        "the one prompt: {system}"
    );
    let names: Vec<&str> = sent[0]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"openxer_playbook") && names.contains(&"openxer_check_schedule"));
    let auth = ep.server.seen("POST", "/v1/chat/completions")[0]
        .header("Authorization")
        .unwrap()
        .to_string();
    assert_eq!(auth, format!("Bearer {KEY}"));
    // A server that reports no usage: "not reported", never zeros.
    ep.queue(cmp::text_reply("Noted.", false));
    Ask::new(endpoint::ENDPOINT, "Thanks").send(&mut b, &project);
    let events = pump(&mut b, turn_end);
    assert!(events.contains(&AgentEvent::UsageNotReported), "{events:?}");
    assert!(events.contains(&AgentEvent::TurnFinished { usage: None }));
    // A wrong key: a plain status line.
    ep.queue(kit::Reply::json(
        401,
        &serde_json::json!({"error": {"message": "bad key"}}),
    ));
    Ask::new(endpoint::ENDPOINT, "Again").send(&mut b, &project);
    let (_, message) = error_of(&pump(&mut b, turn_end));
    assert!(message.contains("refused the API key"), "{message}");
    assert!(!message.contains("TESTKEY"));
    b.end();
}

#[test]
fn a_local_model_starts_in_ask_only_and_says_so() {
    let rig = Rig::bare("endpoint local");
    let ep = EndpointFake::new();
    let mut b = rig.backend();
    saved(&mut b, Some(ep.conn(false)), None);
    let a = b.agents()[1].clone();
    assert!(a.ask_only);
    assert!(
        a.privacy.contains("Nothing is sent off this computer"),
        "{}",
        a.privacy
    );
    assert!(a.notice.unwrap().contains(cc::ASK_ONLY_NOTE));
    ep.queue(cmp::text_reply("187 activities.", true));
    Ask::new(endpoint::ENDPOINT, "How many?").send(&mut b, &sample());
    let events = pump(&mut b, turn_end);
    let notes = events
        .iter()
        .filter(|e| matches!(e, AgentEvent::Notice(n) if n == cc::ASK_ONLY_NOTE))
        .count();
    assert_eq!(notes, 1);
    assert!(ep.server.seen("POST", "/v1/chat/completions")[0]
        .header("Authorization")
        .is_none());
    b.end();
}

#[test]
fn test_reports_every_outcome_plainly() {
    let _one = PROXY.lock().unwrap_or_else(|p| p.into_inner());
    kit::FAKE_PROXY_ALL.store(1, std::sync::atomic::Ordering::SeqCst);
    let _reset = Reset;
    let rig = Rig::bare("endpoint test");
    let ep = EndpointFake::new();
    let mut b = rig.backend();
    let run = |b: &mut LiveBackend, conn: Connection, key: Option<&str>| -> String {
        let rx = b.check_endpoint(conn, key.map(oc::Secret::new)).unwrap();
        rx.recv_timeout(Duration::from_secs(20)).unwrap().line
    };
    let ok = run(&mut b, ep.conn(true), Some(KEY));
    assert_eq!(ok, "Connected. The server lists 2 model(s).");
    // The typed key was used for the test only, never stored.
    assert!(!cc::has_key(rig.keys.as_ref(), &ep.conn(true)).unwrap());
    let no_key = ep.conn(true);
    assert_eq!(
        run(&mut b, no_key, None),
        cc::TestOutcome::NoKey.to_string()
    );
    let wrong = EndpointFake::new();
    wrong
        .server
        .route_json("GET", "/v1/models", 401, serde_json::json!({}));
    assert!(run(&mut b, wrong.conn(true), Some("sk-wrong")).contains("refused the API key"));
    let mut missing = ep.conn(false);
    missing.base_url = format!("http://127.0.0.1:{}/nothing", ep.port());
    assert!(run(&mut b, missing, None).contains("not found"));
    let mut down = ep.conn(false);
    down.base_url = "http://127.0.0.1:9/v1".into();
    assert!(run(&mut b, down, None).starts_with("Nothing answers at that address."));
    let mut plain = ep.conn(true);
    plain.base_url = "http://example.test/v1".into();
    assert!(run(&mut b, plain, Some(KEY)).starts_with("The address cannot be used"));
}

#[test]
fn check_setup_lists_the_endpoint_rows() {
    let rig = Rig::bare("endpoint checks");
    let ep = EndpointFake::new();
    let mut b = rig.backend();
    let rows = run_checks(&mut b);
    assert_eq!(
        find(&rows, endpoint::ENDPOINT_NAME, "Not set up").fix,
        Some(checks::Fix::SetUp)
    );
    let mut conn = ep.conn(true);
    saved(&mut b, Some(conn.clone()), Some(KEY));
    let rows = run_checks(&mut b);
    assert_eq!(
        find(&rows, endpoint::ENDPOINT_NAME, "Set up: ").mark,
        checks::Mark::Pass
    );
    assert_eq!(
        find(&rows, endpoint::ENDPOINT_NAME, "key is accepted (2 models)").mark,
        checks::Mark::Pass
    );
    assert_eq!(
        find(
            &rows,
            endpoint::ENDPOINT_NAME,
            "Model model-a is set and listed"
        )
        .mark,
        checks::Mark::Pass
    );
    let text = b.check_setup(false).unwrap().text();
    assert!(!text.contains("TESTKEY"), "{text}");
    conn.model = "model-z".into();
    saved(&mut b, Some(conn), None);
    let rows = run_checks(&mut b);
    assert_eq!(
        find(&rows, endpoint::ENDPOINT_NAME, "not in the server's list").fix,
        Some(checks::Fix::SetUp)
    );
}

/// Fills the Set Up... dialog for a Custom connection to `ep`.
fn fill_endpoint_dialog(d: &mut Driver, ep: &EndpointFake, key: Option<&str>) {
    let custom = cc::presets()
        .iter()
        .position(|p| p.preset == Preset::Custom)
        .unwrap();
    d.click("chat_endpoint_preset");
    d.click(&format!("chat_endpoint_preset_item_{custom}"));
    d.frames(2);
    d.click("chat_endpoint_address");
    d.key_with(Key::A, egui::Modifiers::COMMAND);
    d.type_text(&format!("http://127.0.0.1:{}/v1", ep.port()));
    if let Some(key) = key {
        d.click("chat_endpoint_key");
        d.type_text(key);
    }
    d.frames(1);
    d.click("chat_endpoint_test");
    until(d, "the test result", |d| {
        shown(d, "Connected. The server lists 2 model(s).")
    });
    // The listed models fill the Model combo; pick one.
    d.click("chat_endpoint_models");
    d.click("chat_endpoint_models_item_1");
    d.frames(2);
}

#[test]
fn api_key_only_sets_up_in_the_dialog_and_asks() {
    let rig = Rig::bare("fresh api key");
    let ep = EndpointFake::new();
    let mut d = live_driver("chat-fresh-key", &rig);
    open_live(&mut d);
    until(&mut d, "detection", |d| {
        !d.app
            .chat
            .backend
            .agents()
            .iter()
            .any(|a| a.state == AgentState::Checking)
    });
    d.frames(2);
    reveal_click(&mut d, &crate::chat::list::setup_key(1, false));
    assert!(d.app.chat.endpoint.is_some(), "Set Up... opens the dialog");
    fill_endpoint_dialog(&mut d, &ep, Some(KEY));
    // The key is never drawn.
    let texts: Vec<String> = d.texts().into_iter().map(|(t, _)| t).collect();
    assert!(!texts.iter().any(|t| t.contains("TESTKEY")), "{texts:?}");
    assert!(shown(&mut d, "pay-as-you-go"), "the key caution");
    d.click("chat_endpoint_ok");
    d.frames(2);
    assert!(d.app.chat.endpoint.is_none());
    let pref = d
        .app
        .store
        .preference(crate::chat::endpoint_dialog::PREF_KEY)
        .unwrap()
        .unwrap();
    assert!(
        pref.contains("model-b") && !pref.contains("TESTKEY"),
        "{pref}"
    );
    let conn = Connection::from_json(&serde_json::from_str(&pref).unwrap()).unwrap();
    assert!(cc::has_key(rig.keys.as_ref(), &conn).unwrap());
    // The only ready route: straight to its empty state; ask.
    until(&mut d, "ready", |d| shown(d, "is ready."));
    ep.queue(cmp::text_reply("There are 187 activities.", false));
    ask_in_window(&mut d, "How many activities?");
    until(&mut d, "answer", |d| !d.app.chat.conv.running);
    assert!(shown(&mut d, "There are 187 activities."));
    assert!(shown(&mut d, "tokens not reported"));
    assert_eq!(ep.requests()[0]["model"], "model-b");
    // Copy Results of Check Setup never holds the key.
    d.click("chat_options");
    d.click("chat_options_check");
    until(&mut d, "checks", |d| shown(d, "Done:"));
    let text = d.app.chat.backend.check_setup(false).unwrap().text();
    assert!(!text.contains("TESTKEY"));
    d.key(Key::Escape);
    d.app.close_chat();
}

#[test]
fn local_model_only_sets_up_without_a_key_and_starts_in_ask_only() {
    let rig = Rig::bare("fresh local model");
    let ep = EndpointFake::new();
    let mut d = live_driver("chat-fresh-local", &rig);
    d.app.chat.mode = Mode::Edit;
    open_live(&mut d);
    until(&mut d, "detection", |d| {
        !d.app
            .chat
            .backend
            .agents()
            .iter()
            .any(|a| a.state == AgentState::Checking)
    });
    d.frames(2);
    reveal_click(&mut d, &crate::chat::list::setup_key(1, false));
    fill_endpoint_dialog(&mut d, &ep, None);
    assert!(shown(&mut d, "This connection runs on this PC."));
    assert!(shown(&mut d, "starts in Ask only"));
    d.click("chat_endpoint_ok");
    d.frames(2);
    assert_eq!(
        d.app.chat.mode,
        Mode::Ask,
        "a local model starts in Ask only"
    );
    until(&mut d, "ready", |d| shown(d, "is ready."));
    ep.queue(cmp::text_reply("Fine.", true));
    ask_in_window(&mut d, "Hello");
    until(&mut d, "answer", |d| !d.app.chat.conv.running);
    assert!(shown(&mut d, "Fine."));
    d.app.close_chat();
}

#[test]
fn ending_one_conversation_leaves_another_ones_folder_alone() {
    let (a, b) = (Rig::new("two convs a"), Rig::new("two convs b"));
    let (mut x, mut y) = (a.backend(), b.backend());
    detected(&mut x);
    detected(&mut y);
    let project = sample();
    Ask::new(CODEX, "first").send(&mut x, &project);
    Ask::new(CODEX, "first").send(&mut y, &project);
    pump(&mut x, turn_end);
    pump(&mut y, turn_end);
    let kept = b.work_dirs();
    assert_eq!(kept.len(), 1);
    x.end();
    drop(x);
    assert!(a.work_dirs().is_empty());
    assert!(kept[0].is_dir(), "the other conversation keeps its folder");
    y.end();
}

// ------------------------------------------- the key, its address, frames

/// Saves `conn` with `key` through the window's backend and waits.
fn saved_in(d: &mut Driver, conn: Connection, key: &str) {
    let rx = d
        .app
        .chat
        .backend
        .set_endpoint(Some(conn.clone()), Some(oc::Secret::new(key)));
    rx.recv_timeout(Duration::from_secs(20)).unwrap().unwrap();
    until(d, "the key is known", |d| {
        d.app.chat.backend.endpoint_has_key(&conn)
    });
}

#[test]
fn a_stored_key_never_goes_to_another_address_from_set_up() {
    let rig = Rig::bare("key binding");
    let (a, b) = (EndpointFake::new(), EndpointFake::new());
    let mut d = live_driver("chat-key-binding", &rig);
    open_live(&mut d);
    let conn = a.conn(true);
    saved_in(&mut d, conn.clone(), KEY);
    d.app.open_endpoint_dialog();
    d.frames(2);
    assert!(shown(&mut d, "A key is stored."));
    // The address changes to another server: no stored key for it.
    d.click("chat_endpoint_address");
    d.key_with(Key::A, egui::Modifiers::COMMAND);
    d.type_text(&format!("http://127.0.0.1:{}/v1", b.port()));
    d.frames(2);
    assert!(!shown(&mut d, "A key is stored."));
    d.click("chat_endpoint_test");
    until(&mut d, "the test", |d| {
        shown(d, "Connected. The server lists 2 model(s).")
    });
    let seen = b.server.seen("GET", "/v1/models");
    assert!(!seen.is_empty());
    assert!(
        seen.iter().all(|s| s.header("Authorization").is_none()),
        "the key for the other address was not sent"
    );
    // OK: the old address's key is deleted.
    d.click("chat_endpoint_ok");
    until(&mut d, "saved", |d| d.app.chat.endpoint.is_none());
    let item = conn.key_item().unwrap();
    use oc::CredentialStore;
    assert!(
        rig.keys.read(item).unwrap().is_none(),
        "the old key is gone"
    );
    // A preset change shows no stored key either.
    let mut c2 = a.conn(true);
    c2.id = conn.id.clone();
    saved_in(&mut d, c2, KEY);
    d.app.open_endpoint_dialog();
    d.frames(2);
    assert!(shown(&mut d, "A key is stored."));
    let ollama = cc::presets()
        .iter()
        .position(|p| p.preset == Preset::Ollama)
        .unwrap();
    d.click("chat_endpoint_preset");
    d.click(&format!("chat_endpoint_preset_item_{ollama}"));
    d.frames(2);
    assert!(!shown(&mut d, "A key is stored."));
    assert!(
        !d.has("chat_endpoint_key"),
        "a local preset asks for no key"
    );
    d.click("chat_endpoint_cancel");
    d.app.close_chat();
}

#[test]
fn remove_key_deletes_it_and_check_setup_then_asks_for_one() {
    let rig = Rig::bare("remove key");
    let a = EndpointFake::new();
    let mut d = live_driver("chat-remove-key", &rig);
    open_live(&mut d);
    let conn = a.conn(true);
    saved_in(&mut d, conn.clone(), KEY);
    d.app.open_endpoint_dialog();
    d.frames(2);
    d.click("chat_endpoint_remove_key");
    until(&mut d, "removed", |d| shown(d, "The key was removed."));
    assert!(!cc::has_key(rig.keys.as_ref(), &conn).unwrap());
    assert!(!d.app.chat.backend.endpoint_has_key(&conn));
    d.click("chat_endpoint_cancel");
    d.frames(1);
    let rows = {
        let b = &mut d.app.chat.backend;
        assert!(b.check_setup(true).is_some());
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let c = b.check_setup(false).unwrap();
            c.poll();
            if !c.running() {
                break c.rows.clone();
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    assert_eq!(
        find(&rows, endpoint::ENDPOINT_NAME, "needs an API key").mark,
        checks::Mark::Fail
    );
    d.app.close_chat();
}

/// A credential store that counts its calls.
#[derive(Default)]
struct Counting {
    inner: oc::MemoryStore,
    calls: std::sync::atomic::AtomicUsize,
}

impl oc::CredentialStore for Counting {
    fn read(&self, item: oc::Item) -> Result<Option<oc::Secret>, oc::StoreError> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.read(item)
    }
    fn write(&self, item: oc::Item, value: &oc::Secret) -> Result<(), oc::StoreError> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.write(item, value)
    }
    fn delete(&self, item: oc::Item) -> Result<(), oc::StoreError> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.delete(item)
    }
}

#[test]
fn frames_never_touch_the_credential_store() {
    let rig = Rig::bare("counting store");
    let a = EndpointFake::new();
    let keys = Arc::new(Counting::default());
    let tokens = Arc::new(Counting::default());
    let mut config = rig.config();
    config.keys = keys.clone();
    config.auth = Arc::new(oc::Auth::new(
        rig.plan.fx.auth.config().clone(),
        tokens.clone(),
    ));
    let mut p = sample();
    crate::scheduling::schedule_model(&mut p.model, &p.resources).unwrap();
    let mut d = Driver::new("chat-counting", p);
    d.app.chat.opener = rig.opener.clone();
    d.app.chat.backend = Box::new(LiveBackend::new(config));
    open_live(&mut d);
    saved_in(&mut d, a.conn(true), KEY);
    // Everything known now; the window and Set Up... stay open.
    d.frames(5);
    d.app.open_endpoint_dialog();
    d.frames(5);
    let total = |k: &Counting| k.calls.load(std::sync::atomic::Ordering::SeqCst);
    let before = (total(&keys), total(&tokens));
    d.frames(40);
    assert_eq!(
        (total(&keys), total(&tokens)),
        before,
        "no frame reads or writes the credential store"
    );
    d.click("chat_endpoint_cancel");
    d.app.close_chat();
}

// ------------------------------------- tools reached, per agent process

fn init(status: &str, id: &str) -> String {
    format!(
        r#"{{"type":"system","subtype":"init","session_id":"00000000-0000-4000-8000-0000000000{id}","model":"m","mcp_servers":[{{"name":"openxer","status":"{status}"}}]}}"#
    )
}

fn said(text: &str) -> String {
    format!(r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":"{text}"}}]}}}}"#)
}

const RESULT: &str =
    r#"{"type":"result","subtype":"success","usage":{"input_tokens":10,"output_tokens":5}}"#;

#[test]
fn a_stop_after_the_tools_failed_lets_the_next_process_answer() {
    let rig = Rig::new("tools per process");
    rig.agents.with(Agent::ClaudeCode, |b| {
        b.turns = vec![
            // A process whose tools did not start, stopped by the user.
            vec![init("failed", "31"), said("Guessing.")],
            // The next process reaches its tools; a greeting needs none.
            vec![init("connected", "32"), said("Hello."), RESULT.to_string()],
        ]
        .into();
    });
    let mut b = rig.backend();
    detected(&mut b);
    let project = sample();
    Ask::new(CLAUDE_CODE, "What drives the finish?").send(&mut b, &project);
    pump(&mut b, |e| e.contains(&AgentEvent::Ready));
    std::thread::sleep(Duration::from_millis(100));
    let held = b.poll();
    assert!(texts(&held).is_empty(), "{held:?}");
    b.stop();
    Ask::new(CLAUDE_CODE, "hello").send(&mut b, &project);
    let events = pump(&mut b, turn_end);
    assert_eq!(texts(&events), "Hello.", "{events:?}");
    assert!(!events.iter().any(|e| matches!(e, AgentEvent::Error { .. })));
    b.end();
}

#[test]
fn a_new_process_must_reach_its_tools_again() {
    let rig = Rig::new("tools new process");
    let mut turns = codex_turns();
    // The second turn's process cannot start its server.
    turns[1] = vec![
        r#"{"type":"thread.started","thread_id":"00000000-0000-4000-8000-000000000033"}"#.to_string(),
        r#"{"type":"item.completed","item":{"id":"item_0","type":"error","message":"MCP client for `openxer` failed to start"}}"#.to_string(),
        r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"PM0910 probably."}}"#.to_string(),
        r#"{"type":"turn.completed","usage":{"input_tokens":10,"output_tokens":5}}"#.to_string(),
    ];
    rig.agents.with(Agent::Codex, |b| b.turns = turns.into());
    let mut b = rig.backend();
    detected(&mut b);
    let project = sample();
    Ask::new(CODEX, "Which is critical?").send(&mut b, &project);
    let first = pump(&mut b, turn_end);
    assert!(first
        .iter()
        .any(|e| matches!(e, AgentEvent::ToolFinished { ok: true, .. })));
    Ask::new(CODEX, "And now?").send(&mut b, &project);
    let second = pump(&mut b, turn_end);
    assert!(texts(&second).is_empty(), "{second:?}");
    assert_eq!(error_of(&second).1, TOOLS_UNREACHABLE);
    b.end();
}

#[test]
fn a_server_still_connecting_at_start_is_not_a_failure() {
    let rig = Rig::new("tools pending");
    rig.agents.with(Agent::ClaudeCode, |b| {
        b.turns = vec![vec![
            init("pending", "34"),
            said("Hello."),
            RESULT.to_string(),
        ]]
        .into();
    });
    let mut b = rig.backend();
    detected(&mut b);
    Ask::new(CLAUDE_CODE, "hello").send(&mut b, &sample());
    let events = pump(&mut b, turn_end);
    assert_eq!(texts(&events), "Hello.", "{events:?}");
    assert!(!events.iter().any(|e| matches!(e, AgentEvent::Error { .. })));
    b.end();
}

#[test]
fn the_first_message_to_a_remote_service_asks_once_per_host() {
    let rig = Rig::bare("remote consent");
    let mut d = live_driver("chat-remote-consent", &rig);
    open_live(&mut d);
    let mut conn = Connection::new(Preset::Custom);
    conn.base_url = "https://compatible.example.test/v1".into();
    conn.needs_key = true;
    conn.model = "model-a".into();
    saved_in(&mut d, conn, KEY);
    d.app.chat.agent = Some(endpoint::ENDPOINT.into());
    d.app.chat.picked = true;
    until(&mut d, "ready", |d| shown(d, "is ready."));
    ask_in_window(&mut d, "How many activities?");
    d.frames(2);
    assert!(d.app.chat.use_consent.is_some(), "it asks first");
    assert!(shown(&mut d, "compatible.example.test"), "what goes where");
    assert!(d.app.chat.conv.is_empty(), "nothing is sent yet");
    // Enter is Cancel: nothing is sent, the message stays to send again.
    d.key(Key::Enter);
    d.frames(2);
    assert!(d.app.chat.use_consent.is_none());
    assert!(d.app.chat.conv.is_empty());
    assert_eq!(d.app.chat.draft, "How many activities?");
    // Continue: remembered for this host, and the message goes.
    d.click("chat_message");
    d.key(Key::Enter);
    d.frames(2);
    d.click("chat_use_consent_continue");
    d.frames(3);
    assert!(!d.app.chat.conv.is_empty(), "sent after Continue");
    until(&mut d, "the reply ends", |d| !d.app.chat.conv.running);
    assert!(d
        .app
        .store
        .preference("chat_consent_endpoint_compatible.example.test")
        .unwrap()
        .is_some());
    // The next message does not ask again.
    ask_in_window(&mut d, "And another?");
    d.frames(2);
    assert!(d.app.chat.use_consent.is_none());
    d.app.close_chat();
}

// ----------------------------------------- nothing slow runs on a frame

#[test]
fn a_slow_agent_start_never_holds_a_frame_and_stop_reaches_it() {
    let rig = Rig::new("slow start");
    rig.agents.with(Agent::ClaudeCode, |b| {
        b.start_delay = Duration::from_millis(800)
    });
    let mut b = rig.backend();
    detected(&mut b);
    let project = sample();
    let started = Instant::now();
    Ask::new(CLAUDE_CODE, "What drives the finish?").send(&mut b, &project);
    assert!(
        started.elapsed() < Duration::from_millis(300),
        "send returned at once: {:?}",
        started.elapsed()
    );
    assert_eq!(b.snapshot_writes, 0, "the snapshot is written by a worker");
    let events = pump(&mut b, turn_end);
    assert!(texts(&events).contains("PM0920"));
    assert_eq!(b.snapshot_writes, 1);
    // Stop while the next message is still starting: it is stopped as it
    // arrives, and says nothing more.
    rig.agents.with(Agent::ClaudeCode, |b| {
        b.start_delay = Duration::from_millis(800);
        b.turns.push_front(claude_turns()[0].clone());
    });
    let mut ask = Ask::new(CLAUDE_CODE, "Again, in Edit mode");
    ask.mode = Mode::Edit;
    ask.send(&mut b, &project);
    b.stop();
    std::thread::sleep(Duration::from_millis(1500));
    let after: Vec<AgentEvent> = (0..20)
        .flat_map(|_| {
            std::thread::sleep(Duration::from_millis(20));
            b.poll()
        })
        .collect();
    assert!(
        !after
            .iter()
            .any(|e| matches!(e, AgentEvent::TurnFinished { .. })),
        "{after:?}"
    );
    // Claude Code's process stays for the next message; ending stops it.
    b.end();
    let deadline = Instant::now() + Duration::from_secs(20);
    while rig.agents.alive() > 0 {
        assert!(Instant::now() < deadline, "no agent process is left");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn ending_while_a_message_starts_leaves_no_folder() {
    let rig = Rig::new("end while starting");
    rig.agents
        .with(Agent::Codex, |b| b.start_delay = Duration::from_millis(500));
    let mut b = rig.backend();
    detected(&mut b);
    Ask::new(CODEX, "Hi").send(&mut b, &sample());
    b.end();
    let deadline = Instant::now() + Duration::from_secs(20);
    while !rig.work_dirs().is_empty() || rig.agents.alive() > 0 {
        assert!(Instant::now() < deadline, "{:?}", rig.work_dirs());
        std::thread::sleep(Duration::from_millis(10));
    }
}

// --------------------------------------------- Apply on every route

#[test]
fn the_window_runs_the_api_key_route_end_to_end() {
    let rig = Rig::bare("window endpoint");
    let ep = EndpointFake::new();
    ep.queue(cmp::calls_reply(&[(
        "call_1",
        "openxer_load",
        r#"{"critical":true,"fields":["task_code","name"],"limit":5}"#,
    )]));
    ep.queue(cmp::text_reply("**PM0920** finishes the project.", true));
    let changes = r#"{"changes":{"format":"openxer-changes/1","operations":[{"op":"set_activity","activity":"PM0910","field":"activity_name","value":"Hand over as-built drawings"}]}}"#;
    ep.queue(cmp::calls_reply(&[("call_2", "openxer_apply", changes)]));
    ep.queue(cmp::text_reply("I proposed the new name.", true));
    let conn = ep.conn(true);
    route_flow_with(endpoint::ENDPOINT, &rig, "finishes the project", &|d| {
        saved_in(d, conn.clone(), KEY)
    });
}

#[test]
fn apply_works_while_the_reply_is_still_running() {
    let rig = Rig::new("apply while running");
    let mut proposal = claude_turns()[1].clone();
    // The reply goes on after its proposal: no result line yet.
    proposal.retain(|l| !l.contains("\"type\":\"result\""));
    rig.agents.with(Agent::ClaudeCode, |b| {
        b.turns = vec![claude_turns()[0].clone(), proposal].into()
    });
    let mut d = live_driver("chat-apply-running", &rig);
    d.app.chat.agent = Some(CLAUDE_CODE.into());
    d.app.chat.picked = true;
    d.key(Key::F8);
    until(&mut d, "ready", |d| {
        d.app
            .chat
            .backend
            .agents()
            .iter()
            .any(|a| a.id == CLAUDE_CODE && a.state.usable())
    });
    ask_in_window(&mut d, "first");
    until(&mut d, "reply", |d| !d.app.chat.conv.running);
    d.click("chat_mode");
    d.click("chat_mode_item_1");
    ask_in_window(&mut d, "Rename PM0910");
    until(&mut d, "the proposal", |d| {
        d.app.chat.conv.pending().is_some()
    });
    assert!(d.app.chat.conv.running, "the reply is still running");
    let undo = d.app.history.counts().0;
    d.click("chat_apply");
    d.frames(2);
    assert_eq!(
        name_of(&d.app.project, "PM0910"),
        "Submit as-built drawings and O&M manuals"
    );
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");
    d.click("chat_stop");
    d.app.close_chat();
}

#[test]
fn the_preview_and_the_apply_of_an_unchanged_project_read_the_same() {
    let project = sample();
    let dir = std::env::temp_dir()
        .join("openxer-chat-tests")
        .join(format!("preview-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let snapshot = work::write_snapshot(&dir, &project).unwrap();
    let ctx = chat_tools::ChatContext {
        snapshot: Some(snapshot),
        mode: ChatMode::Edit,
        playbooks_dir: None,
        ui_actions: None,
    };
    let batches = [
        r#"[{"op":"set_activity","activity":"PM0910","field":"activity_name","value":"Hand over drawings"}]"#,
        r#"[{"op":"set_activity","activity":"PM0910","field":"remaining_duration","value":40}]"#,
        r#"[{"op":"add_activity","activity":"PM0915","name":"Review closeout with owner","wbs":"PM"}]"#,
        r#"[{"op":"relationship","action":"add","predecessor":"PM0900","successor":"PM0910","kind":"SS","lag_hours":8}]"#,
        r#"[{"op":"set_activity","activity":"PM0900","field":"physical_percent_complete","value":50},{"op":"schedule"}]"#,
    ];
    for ops in batches {
        let changes: Value = serde_json::from_str(&format!(
            r#"{{"format":"openxer-changes/1","operations":{ops}}}"#
        ))
        .unwrap();
        let preview = chat_tools::call(&ctx, chat_tools::APPLY_TOOL, json!({ "changes": changes }));
        assert!(!preview.is_error, "{ops}: {}", preview.text);
        let shown = rows::rows(&preview.envelope["result"]["report"]["diff"]);
        let mut next = project.clone();
        let report = chat_tools::apply_proposal(&mut next, changes).unwrap();
        let applied = rows::rows(&report["diff"]);
        assert!(!shown.is_empty(), "{ops}");
        assert_eq!(shown, applied, "{ops}: preview and apply read the same");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
